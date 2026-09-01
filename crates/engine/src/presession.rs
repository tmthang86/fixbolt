//! Who is on the other end, before there is a session to ask.
//!
//! `DESIGN.md` D8 gives an `Engine` one `Config` and therefore one FIX
//! identity, which is how it answers *"is this identity already logged on"* —
//! it counts the connections **it** holds. `[measured 2026-08-31]` splitting
//! those connections across shards took the acceptance corpus from **59 to
//! 57**, failing exactly `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def`,
//! because there was nothing left to count.
//!
//! The fix is not a cleverer assignment. Assignment happens at `accept`, and
//! the `Logon` that says who the connection belongs to has not arrived yet. So
//! something has to own the socket until it does, and read the identity off it
//! — [ADR-0020].
//!
//! # This module reads bytes. It is not a second session layer
//!
//! ADR-0020 decision 2. `35=`, `49=` and `56=` come off the buffer by direct
//! scan, the way the engine's own `Logon` check already did. **No dictionary,
//! no parse, nothing from `fixbolt_session` but `Config`.** A stage that had to
//! ask the session a question would be designed wrong, because the session it
//! would ask does not exist yet — which is the entire reason this stage is
//! here.
//!
//! It does not frame, either: [`crate::frame::Framer`] already cuts a stream
//! into messages and carries the one rule the corpus taught
//! (`2m_BodyLengthValueNotCorrect.def`). Everything here takes **one complete
//! message** and answers a question about it. Two copies of a framing rule are
//! two rules that will disagree.
//!
//! [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md

/// The two sides a FIX message names, borrowed from the buffer it arrived in.
///
/// **In wire order.** `sender` is `49=` and `target` is `56=` exactly as they
/// appear in the incoming message, which for an acceptor means `sender` is the
/// counterparty. Both connections from one counterparty therefore carry the
/// same pair, which is what lets a router send them to the same shard and lets
/// the single-logon rule count them together again.
///
/// It borrows. Nothing here allocates — `CLAUDE.md` §2 non-negotiable 1 — and a
/// caller that needs to keep an identity past the buffer copies it deliberately
/// rather than by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity<'a> {
    /// `49=`, the SenderCompID as it appears on the wire.
    pub sender: &'a [u8],
    /// `56=`, the TargetCompID as it appears on the wire.
    pub target: &'a [u8],
}

/// The value of the first field whose tag is `tag`, which must include the `=`.
///
/// Fields, not bytes: the scan walks SOH-separated fields and matches the tag
/// at the **start** of one. A search for `49=` anywhere in the message would
/// read a `Text` field containing `49=` as the sender, which is a value a
/// counterparty controls — see
/// `tests/presession.rs::a_field_value_that_looks_like_an_identity_is_not_one`.
///
/// A trailing field with no SOH is not a field. `msg` is a message
/// [`crate::frame::Framer`] has already cut, so the last one is `10=…` and
/// terminated; a caller passing something else gets the same answer for the
/// same reason.
fn field_value<'a>(msg: &'a [u8], tag: &[u8]) -> Option<&'a [u8]> {
    let mut at = 0;
    while at < msg.len() {
        let end = msg[at..].iter().position(|b| *b == 1).map(|e| e + at)?;
        if let Some(value) = msg[at..end].strip_prefix(tag) {
            return Some(value);
        }
        at = end + 1;
    }
    None
}

/// Both sides of one complete message, or `None` if it does not name both.
///
/// Answers a different question from [`is_logon`] on purpose: a caller that
/// wants to route needs the identity, and a caller deciding whether to accept
/// the connection at all needs the message type. Fusing them would make a
/// non-`Logon` indistinguishable from a `Logon` that named nobody, and those
/// two are dropped for different reasons.
#[must_use]
pub fn identity_of(msg: &[u8]) -> Option<Identity<'_>> {
    Some(Identity {
        sender: field_value(msg, b"49=")?,
        target: field_value(msg, b"56=")?,
    })
}

/// Is this complete message a `Logon`?
///
/// Read off the raw bytes rather than parsed: the engine has no dictionary and
/// wants none. `35=` is the third field of a well-formed message and the
/// session refuses anything else, so a scan for it is enough here.
#[must_use]
pub fn is_logon(msg: &[u8]) -> bool {
    field_value(msg, b"35=") == Some(b"A")
}

// --- holding the socket ------------------------------------------------------

use crate::frame::{Cut, Framer};
use crate::transport::{Io, Transport};

/// The two hard limits, which [ADR-0020] decision 4 gives no defaults.
///
/// A named struct rather than two arguments to a constructor, because both are
/// numbers and a transposed pair would compile: `(30_000, 8)` is a table of
/// thirty thousand slots that expires after eight milliseconds, and nothing
/// would say so.
///
/// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pending: usize,
    logon_ms: u64,
}

/// Why a set of limits was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LimitError {
    /// A ceiling of zero refuses every connection there will ever be.
    NoPendingAllowed,
    /// A deadline of zero expires every connection before it can answer.
    NoTimeToLogOn,
}

impl Limits {
    /// How many connections may wait at once, and how long each has to log on.
    ///
    /// # Errors
    ///
    /// [`LimitError`] for a zero, which is a mistake in both directions rather
    /// than a strict choice. One of each is allowed: that is somebody being
    /// deliberate, and the caller who wrote it can see what it does.
    pub const fn new(pending: usize, logon_ms: u64) -> Result<Self, LimitError> {
        if pending == 0 {
            return Err(LimitError::NoPendingAllowed);
        }
        if logon_ms == 0 {
            return Err(LimitError::NoTimeToLogOn);
        }
        Ok(Self { pending, logon_ms })
    }

    /// The ceiling on connections waiting to identify themselves.
    #[must_use]
    pub const fn pending(self) -> usize {
        self.pending
    }

    /// How long a connection has to send a `Logon`, in milliseconds.
    #[must_use]
    pub const fn logon_ms(self) -> u64 {
        self.logon_ms
    }
}

/// Why a connection was not taken on, carrying the socket back.
///
/// It carries the transport so the refusal is something the caller closes on
/// purpose. Dropping the `Refused` closes it, which is the right default; a
/// caller that wants to say something first still can.
#[derive(Debug)]
#[non_exhaustive]
pub enum Refused<T> {
    /// The table is at its ceiling. Refused **now** rather than queued: a queue
    /// with no bound is the thing the ceiling exists to prevent.
    Full(T),
}

/// What one [`PendingSet::turn`] did.
///
/// Counts rather than a list, so nothing is allocated to report them. A caller
/// that logs reads these; the tests assert them, which is how each limit gets a
/// failing case naming the reason rather than `is_err()`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Connections that produced a whole `Logon` this turn and are waiting to
    /// be taken.
    pub settled: usize,
    /// Connections dropped for reaching their deadline without a `Logon`.
    pub timed_out: usize,
    /// Connections dropped because their first whole message was not a `Logon`.
    pub not_logon: usize,
    /// Connections dropped because the peer left or sent a frame that can never
    /// be a message.
    pub gone: usize,
}

/// One socket that has not said who it is.
pub struct Pending<T, const PRE: usize> {
    transport: T,
    rx: Framer<PRE>,
    deadline_ms: u64,
    /// Length of the whole `Logon` at the front, once there is one.
    settled: Option<usize>,
}

impl<T, const PRE: usize> Pending<T, PRE> {
    /// Every byte read off this socket, `Logon` and anything pipelined behind
    /// it.
    ///
    /// **This is what must reach the session.** A stage that handed on only the
    /// message it routed by would drop whatever followed, and the counterparty
    /// would wait for a reply to something the engine never saw.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.rx.all()
    }

    /// The socket, for handing to an engine.
    pub fn into_transport(self) -> T {
        self.transport
    }

    /// The socket and the bytes, for moving both across a channel.
    ///
    /// The buffer moves as an array rather than as a `Vec`, so handing a
    /// connection to a shard thread allocates nothing — non-negotiable 1, and
    /// `benches/alloc.rs` asserts it.
    pub fn into_parts(self) -> (T, [u8; PRE], usize) {
        let (buf, len) = self.rx.into_parts();
        (self.transport, buf, len)
    }
}

/// Sockets waiting to say who they are.
///
/// [ADR-0020] decision 1. It runs on the **acceptor** thread, which ADR-0013
/// leaves free to block; putting this on an engine thread is what non-negotiable
/// 4 forbids.
///
/// Everything is allocated once, in [`Self::new`], to the ceiling the caller
/// named. Admitting, turning and taking allocate nothing — non-negotiable 1.
///
/// # Indices are not stable across a [`Self::turn`]
///
/// Taking a connection out is a `swap_remove`, so the index of another may
/// change. [`Self::settled`], [`Self::identity_at`] and [`Self::take`] are meant
/// to be used together, without a `turn` in between.
///
/// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
pub struct PendingSet<T, const PRE: usize> {
    slots: Vec<Pending<T, PRE>>,
    limits: Limits,
}

impl<T: Transport, const PRE: usize> PendingSet<T, PRE> {
    /// Room for `limits.pending()` sockets, taken once and never grown.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            slots: Vec::with_capacity(limits.pending()),
            limits,
        }
    }

    /// How many sockets are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether any are.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The limits this set was built with.
    #[must_use]
    pub const fn limits(&self) -> Limits {
        self.limits
    }

    /// Take a socket on, with the deadline running from `now_ms`.
    ///
    /// # Errors
    ///
    /// [`Refused::Full`] when the ceiling is reached, carrying the socket back.
    pub fn admit(&mut self, transport: T, now_ms: u64) -> Result<(), Refused<T>> {
        if self.slots.len() >= self.limits.pending() {
            return Err(Refused::Full(transport));
        }
        self.slots.push(Pending {
            transport,
            rx: Framer::new(),
            deadline_ms: now_ms.saturating_add(self.limits.logon_ms()),
            settled: None,
        });
        Ok(())
    }

    /// Read from every waiting socket, and let go of the ones that are done for.
    ///
    /// A socket that has already settled is left alone: it has said what it
    /// needed to and is waiting to be taken, so it neither reads nor expires.
    pub fn turn(&mut self, now_ms: u64) -> Progress {
        let mut p = Progress::default();
        let mut i = 0;
        while i < self.slots.len() {
            let Some(slot) = self.slots.get_mut(i) else {
                break;
            };
            if slot.settled.is_some() {
                i += 1;
                continue;
            }
            match Self::advance(slot, now_ms) {
                Step::Waiting => i += 1,
                Step::Settled => {
                    p.settled += 1;
                    i += 1;
                }
                Step::TimedOut => {
                    p.timed_out += 1;
                    self.slots.swap_remove(i);
                }
                Step::NotLogon => {
                    p.not_logon += 1;
                    self.slots.swap_remove(i);
                }
                Step::Gone => {
                    p.gone += 1;
                    self.slots.swap_remove(i);
                }
            }
        }
        p
    }

    /// One socket: read what is there, then decide what it has become.
    fn advance(slot: &mut Pending<T, PRE>, now_ms: u64) -> Step {
        let spare = slot.rx.spare();
        if !spare.is_empty() {
            match slot.transport.recv(spare) {
                Io::Ready(n) => slot.rx.filled(n),
                Io::Idle => {}
                // A peer that left, or a socket that failed, is gone either
                // way: there is no session here to tell, and nothing to say.
                Io::Closed | Io::Failed(_) => return Step::Gone,
            }
        }
        match slot.rx.cut() {
            Cut::Message(n) => {
                if is_logon(slot.rx.bytes(n)) {
                    slot.settled = Some(n);
                    Step::Settled
                } else {
                    Step::NotLogon
                }
            }
            // Unreadable, and this stage has no session to hand it to. The
            // session's own rule about a garbled *Logon* still applies to
            // connections that get past here.
            Cut::Garbage(_) => Step::Gone,
            Cut::Need => {
                if now_ms >= slot.deadline_ms {
                    Step::TimedOut
                } else {
                    Step::Waiting
                }
            }
        }
    }

    /// The soonest deadline any waiting socket has, if any.
    ///
    /// It is what a caller waits **until**: an acceptor that slept on a fixed
    /// interval would either wake for nothing or let a deadline slip past by up
    /// to that interval, and neither is a number anybody chose. Sockets that
    /// have already settled are not counted — they are not waiting for
    /// anything.
    #[must_use]
    pub fn earliest_deadline(&self) -> Option<u64> {
        self.slots
            .iter()
            .filter(|s| s.settled.is_none())
            .map(|s| s.deadline_ms)
            .min()
    }

    /// Append one readable interest per waiting socket.
    ///
    /// Appends rather than returns, so the caller reuses one buffer and this
    /// allocates nothing. A socket with no descriptor contributes none, the way
    /// [`crate::Engine`]'s own interest list does.
    pub fn interests(&self, out: &mut Vec<crate::transport::Interest>) {
        out.extend(
            self.slots
                .iter()
                .filter_map(|s| s.transport.source())
                .map(crate::transport::Interest::readable),
        );
    }

    /// The first socket that has produced a whole `Logon`, if any.
    #[must_use]
    pub fn settled(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.settled.is_some())
    }

    /// Who the socket at `i` says it is.
    ///
    /// `None` when there is no such slot, when it has not settled, or when the
    /// `Logon` named only one side — three different things a caller may want
    /// to tell apart by asking [`Self::settled`] first.
    #[must_use]
    pub fn identity_at(&self, i: usize) -> Option<Identity<'_>> {
        let slot = self.slots.get(i)?;
        identity_of(slot.rx.bytes(slot.settled?))
    }

    /// Take the socket at `i` out, with everything read off it.
    pub fn take(&mut self, i: usize) -> Option<Pending<T, PRE>> {
        (i < self.slots.len()).then(|| self.slots.swap_remove(i))
    }
}

/// What one socket became this turn.
enum Step {
    Waiting,
    Settled,
    TimedOut,
    NotLogon,
    Gone,
}

// --- choosing a shard --------------------------------------------------------

/// Which shard takes a connection, once it has said who it is.
///
/// [ADR-0015] decision 7: this belongs to the caller. Real deployments shard by
/// counterparty, and the engine does not know which counterparty matters.
///
/// It is asked **after** the `Logon`, not at accept time — which is the whole
/// reason the pre-session stage exists ([ADR-0020]). Its predecessor `Assign`
/// was asked at accept, when nothing knew whose socket it was, and could not
/// have answered this question however it was written.
///
/// [ADR-0015]: ../../../docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md
/// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
pub trait Route: Send {
    /// A shard index in `0..shards`. Out of range is refused, not clamped.
    fn shard_for(&mut self, id: Identity<'_>, shards: usize) -> usize;
}

/// The default: a stable hash of `(sender, target)`.
///
/// **Stable is the requirement, not fast.** The single-logon rule can only
/// count connections one engine holds, so the same counterparty has to reach
/// the same shard on this run, on the next run, and after a reconnect.
///
/// `std::collections::hash_map::DefaultHasher` is therefore not usable here and
/// [ADR-0020] decision 7 says so: it is seeded per process, so two runs of one
/// binary would route the same counterparty differently, and the rule would
/// hold within a run and break across a restart — the worst shape a bug can
/// have, because every test passes.
///
/// Hashing is the sensible default and explicitly not the final answer. A real
/// deployment shards by counterparty deliberately; [`Route`] is the seam.
///
/// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
#[derive(Debug, Default, Clone, Copy)]
pub struct HashRoute;

/// FNV-1a, 64-bit, written out rather than borrowed from `std`.
///
/// The separator between the two halves is not decoration: without it
/// `("AB", "C")` and `("A", "BC")` hash alike, and two different counterparties
/// would share a shard for a reason nobody could see.
const fn fnv1a(sender: &[u8], target: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    let mut i = 0;
    while i < sender.len() {
        h = (h ^ sender[i] as u64).wrapping_mul(PRIME);
        i += 1;
    }
    // SOH, the separator FIX itself uses, and a byte no comp ID may contain.
    h = (h ^ 1u64).wrapping_mul(PRIME);
    let mut j = 0;
    while j < target.len() {
        h = (h ^ target[j] as u64).wrapping_mul(PRIME);
        j += 1;
    }
    h
}

impl Route for HashRoute {
    fn shard_for(&mut self, id: Identity<'_>, shards: usize) -> usize {
        if shards == 0 {
            return 0;
        }
        // `as u64` on a usize is lossless on every target this builds for, and
        // the remainder is then in range by construction.
        #[allow(clippy::cast_possible_truncation)]
        {
            (fnv1a(id.sender, id.target) % shards as u64) as usize
        }
    }
}
