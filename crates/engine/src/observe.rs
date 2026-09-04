//! What an operator can see, without touching the hot path.
//!
//! `[verified 2026-09-01]` an `Engine`'s entire observable surface was
//! `connections()`, `sources_missing()` and `refused_connections()` — three
//! numbers, none of them about a session — and all three readable only from the
//! engine's own thread. `STATUS.md` open item 30 (b) and (f).
//!
//! # On demand, and that is the whole design
//!
//! An engine that published its state every turn would pay for an operator who
//! is not there, on the path `DESIGN.md` D8 exists to keep empty. So nothing is
//! built until somebody asks: the cost of being observable, when nobody is
//! observing, is **one relaxed load per turn**.
//!
//! [`Observer::request`] raises a flag and returns the most recent snapshot the
//! engine published — `None` until it has published one. A caller that needs a
//! fresh one asks again; there is no blocking, in either direction.
//!
//! # Why not the ring
//!
//! `DESIGN.md` D10's ring is the application path, and
//! [ADR-0011](../../../docs/decisions/ADR-0011-a-full-ring-disconnects.md) says
//! a full ring disconnects the session. **An operator asking a question must not
//! be able to drop a connection.** Two mechanisms, two purposes, and this one
//! does not get to share that risk.
//!
//! # Why the engine never blocks, and there is no `unsafe`
//!
//! `try_lock`, never `lock`. Non-negotiable 4 forbids the engine thread
//! blocking in the kernel, and a mutex can. If the reader happens to hold the
//! cell at that instant the engine skips publishing and **leaves the request
//! standing**, so the next turn does it — a reader is never starved and the
//! engine is never stopped. The alternative was a seqlock over `UnsafeCell`, and
//! [ADR-0007](../../../docs/decisions/ADR-0007-spsc-ring-without-unsafe.md)
//! already settled this house's preference: safe first, `unsafe` only when a
//! measurement asks.
//!
//! # Why the snapshot is a fixed array
//!
//! Non-negotiable 1: no allocation on the hot path. [`MAX_SESSIONS`] slots and a
//! [`Snapshot::truncated`] flag, because `hft` carries a ceiling of four
//! ([ADR-0025](../../../docs/decisions/ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md))
//! but `standard` carries none — so *"there were more than this"* is a fact to
//! report, not a case to fail on.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::ConnId;

/// How many sessions one snapshot can carry.
///
/// Above this, [`Snapshot::truncated`] is set and the rest are not described.
/// Sixteen times `hft`'s ceiling of four, and a `standard` engine holding more
/// than this has an operator problem that a longer array would not solve.
pub const MAX_SESSIONS: usize = 64;

/// One session, as an operator sees it.
///
/// Plain `Copy` data. It is a **copy taken at an instant**, not a view: by the
/// time it is read the engine has moved on, and a number that could change
/// under the reader would be worse than one that is honestly a moment old.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SessionSnapshot {
    id: ConnId,
    logged_on: bool,
    next_out: u32,
    next_in: u32,
    last_skew_ms: Option<i64>,
    has_pending_output: bool,
    puts_refused: u32,
    resend_beyond_journal: u32,
}

impl SessionSnapshot {
    /// The connection id, which is what a reply from another thread is routed
    /// by.
    #[must_use]
    pub const fn id(&self) -> ConnId {
        self.id
    }

    /// Has this session exchanged a `Logon`?
    #[must_use]
    pub const fn logged_on(&self) -> bool {
        self.logged_on
    }

    /// `34=` on the next message this session will send.
    #[must_use]
    pub const fn next_out(&self) -> u32 {
        self.next_out
    }

    /// `34=` this session expects next from the counterparty.
    #[must_use]
    pub const fn next_in(&self) -> u32 {
        self.next_in
    }

    /// The engine's clock minus the `SendingTime` of the last message whose
    /// `52=` could be read, in milliseconds. Positive means the counterparty's
    /// stamp is behind ours.
    ///
    /// **This is the field that answers a 3 a.m. question.** `Config`'s
    /// `max_skew_ms` silently refuses a message whose `52=` is too far from this
    /// engine's clock, and nothing else in this engine would ever say why: on a
    /// box whose NTP has drifted, a counterparty simply stops working. `None`
    /// until a message with a readable `SendingTime` has arrived.
    ///
    /// **Recorded whether that message was accepted or refused** — the refusal
    /// is the case it exists to explain.
    #[must_use]
    pub const fn last_skew_ms(&self) -> Option<i64> {
        self.last_skew_ms
    }

    /// Are there bytes queued that have not reached the socket?
    #[must_use]
    pub const fn has_pending_output(&self) -> bool {
        self.has_pending_output
    }

    /// Application messages this session sent that the journal would not keep.
    ///
    /// The running total for this connection. The event
    /// [`EventKind::JournalRefused`] carries the change; this carries where it
    /// got to, for an operator who started reading late.
    #[must_use]
    pub const fn puts_refused(&self) -> u32 {
        self.puts_refused
    }

    /// Numbers a `ResendRequest` asked for that had already fallen out of the
    /// journal, and were gap-filled instead of replayed.
    ///
    /// Counted in **messages**. See [`EventKind::ResendBeyondJournal`].
    #[must_use]
    pub const fn resend_beyond_journal(&self) -> u32 {
        self.resend_beyond_journal
    }
}

/// What a session's journal has cost it, for [`SessionSnapshot::describe`].
///
/// A struct rather than two more parameters: eight positional arguments of
/// which four are `u32` is a call nobody can read, and two of them swapped is a
/// silent wrong answer rather than a compile error.
#[derive(Debug, Clone, Copy)]
pub(crate) struct JournalHealth {
    pub refused: u32,
    pub beyond: u32,
}

impl SessionSnapshot {
    /// Build one. `pub(crate)`: only the engine describes a session.
    pub(crate) const fn describe(
        id: ConnId,
        logged_on: bool,
        next_out: u32,
        next_in: u32,
        last_skew_ms: Option<i64>,
        has_pending_output: bool,
        journal: JournalHealth,
    ) -> Self {
        Self {
            id,
            logged_on,
            next_out,
            next_in,
            last_skew_ms,
            has_pending_output,
            puts_refused: journal.refused,
            resend_beyond_journal: journal.beyond,
        }
    }
}

/// One engine, at one instant.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    sessions: [SessionSnapshot; MAX_SESSIONS],
    len: usize,
    truncated: bool,
    connections: usize,
    refused_connections: usize,
    sources_missing: usize,
    log_lost: u64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            sessions: [SessionSnapshot::default(); MAX_SESSIONS],
            len: 0,
            truncated: false,
            connections: 0,
            refused_connections: 0,
            sources_missing: 0,
            log_lost: 0,
        }
    }
}

impl Snapshot {
    /// Every session this engine held, up to [`MAX_SESSIONS`].
    #[must_use]
    pub fn sessions(&self) -> &[SessionSnapshot] {
        self.sessions.get(..self.len).unwrap_or(&[])
    }

    /// Were there more sessions than the snapshot could carry?
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// How many connections the engine held, whether described here or not.
    #[must_use]
    pub const fn connections(&self) -> usize {
        self.connections
    }

    /// Connections ended because the dispatch would not take a message —
    /// ADR-0011. **Zero on a healthy engine.**
    #[must_use]
    pub const fn refused_connections(&self) -> usize {
        self.refused_connections
    }

    /// Is this engine healthy enough to serve? Item 30 (f), and **a pure
    /// function on data already gathered** — no I/O, no second mechanism, so a
    /// health endpoint and an operator's `Debug` print can never disagree.
    ///
    /// Healthy means: at least one session, every one of them logged on, and
    /// neither counter that is only ever non-zero on a broken engine. It does
    /// **not** look at [`Self::truncated`]: more sessions than the snapshot can
    /// carry is a reporting limit, not an unhealthy engine.
    #[must_use]
    pub fn healthy(&self) -> bool {
        !self.sessions().is_empty()
            && self.sessions().iter().all(SessionSnapshot::logged_on)
            && self.refused_connections == 0
            && self.sources_missing == 0
    }

    /// Add one session, or record that it did not fit.
    ///
    /// `pub(crate)`: only the engine builds one of these. Beyond
    /// [`MAX_SESSIONS`] the session is dropped and [`Self::truncated`] is set —
    /// `standard` has no session ceiling, so *"there were more"* is a fact to
    /// report rather than a case to fail on.
    pub(crate) const fn push(&mut self, s: SessionSnapshot) {
        if self.len < MAX_SESSIONS {
            self.sessions[self.len] = s;
            self.len += 1;
        } else {
            self.truncated = true;
        }
    }

    /// Fill in the engine-wide counters. `pub(crate)`: only the engine builds
    /// one of these.
    pub(crate) const fn set_counters(
        &mut self,
        connections: usize,
        refused_connections: usize,
        sources_missing: usize,
        log_lost: u64,
    ) {
        self.connections = connections;
        self.refused_connections = refused_connections;
        self.sources_missing = sources_missing;
        self.log_lost = log_lost;
    }

    /// Messages the message log never wrote. **Zero on a healthy engine.**
    ///
    /// The running total since the log was opened. See
    /// [`EventKind::MessageLogLost`] for the per-turn event, and `GUIDE.md` §6a
    /// for what to do about a number that climbs.
    #[must_use]
    pub const fn log_lost(&self) -> u64 {
        self.log_lost
    }

    /// Connections that claimed to be pollable and produced no descriptor.
    /// **Zero on a healthy engine**; anything else is a `Transport` breaking its
    /// own contract, and the symptom is lateness rather than a crash.
    #[must_use]
    pub const fn sources_missing(&self) -> usize {
        self.sources_missing
    }
}

/// What the engine and the operator share. One allocation, at
/// [`crate::Engine::observer`], and never on a turn.
#[derive(Debug)]
pub(crate) struct Shared {
    /// Somebody asked. Cleared when the engine publishes.
    wanted: AtomicBool,
    /// How many snapshots the engine has built. **The number that keeps *"on
    /// demand"* honest** — an implementation publishing every turn passes every
    /// content assertion and fails this one.
    published: AtomicU64,
    cell: Mutex<Snapshot>,
    /// Events, which unlike a snapshot are **pushed** rather than requested:
    /// an event nobody asked for at the right moment is an event lost, and the
    /// whole point is that they are not.
    pub(crate) events: Events,
    /// Commands going the other way. Same `Arc`, same fixed shapes, **a
    /// different capability**: [`Observer`] cannot reach this and [`Admin`]
    /// can.
    pub(crate) commands: Commands,
    /// Messages an application originated on another thread, waiting for the
    /// engine's next turn. **A third capability on one `Arc`**: an
    /// [`Observer`] cannot reach it, an [`Admin`] cannot either, and a
    /// [`Sender`](crate::origin::Sender) can — ADR-0048 decision 4.
    pub(crate) origin: crate::origin::Origin,
    /// Somebody asked the engine to stop, and how long they will wait.
    ///
    /// Two atomics rather than one: the grace period is stored **before** the
    /// flag is set, and the flag is `Release`, so a turn that sees the flag is
    /// guaranteed to see the number that goes with it.
    stop: AtomicBool,
    stop_grace_ms: AtomicU64,
}

impl Shared {
    pub(crate) fn new() -> Self {
        Self {
            wanted: AtomicBool::new(false),
            published: AtomicU64::new(0),
            cell: Mutex::new(Snapshot::default()),
            events: Events::new(),
            commands: Commands::new(),
            origin: crate::origin::Origin::new(),
            stop: AtomicBool::new(false),
            stop_grace_ms: AtomicU64::new(0),
        }
    }

    /// Has somebody asked? One relaxed load, and it is the entire cost of being
    /// observable while nobody is observing.
    pub(crate) fn wanted(&self) -> bool {
        self.wanted.load(Ordering::Relaxed)
    }

    /// Record one event. See [`Events::push`] — `try_lock`, and a refusal is
    /// counted rather than waited on.
    pub(crate) fn emit(&self, id: ConnId, at_ms: u64, kind: EventKind) {
        self.events.push(Event { id, at_ms, kind });
    }

    /// Has somebody asked the engine to stop, and with how long a grace?
    ///
    /// **One relaxed load** on a turn where nobody has, which is the entire
    /// cost of an engine being stoppable.
    pub(crate) fn stop_asked(&self) -> Option<u64> {
        self.stop
            .load(Ordering::Acquire)
            .then(|| self.stop_grace_ms.load(Ordering::Relaxed))
    }

    /// Publish, unless the reader holds the cell right now.
    ///
    /// `try_lock`, never `lock` — non-negotiable 4. A refusal leaves `wanted`
    /// set, so the next turn publishes and the reader is not starved.
    pub(crate) fn publish(&self, snap: &Snapshot) {
        if let Ok(mut slot) = self.cell.try_lock() {
            *slot = *snap;
            self.wanted.store(false, Ordering::Release);
            self.published.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// An operator's handle on a running engine. `Send + Sync`; hold it on whatever
/// thread does the asking.
#[derive(Debug, Clone)]
pub struct Observer(pub(crate) std::sync::Arc<Shared>);

impl Observer {
    /// Ask for a snapshot, and take the most recent one the engine published.
    ///
    /// **Returns what is there now, and asks for a fresh one** — it does not
    /// wait, in either direction. `None` before the engine has published
    /// anything at all. A caller wanting a snapshot taken *after* this call
    /// asks again; a caller wanting one at all polls until it is `Some`.
    ///
    /// Blocking here would put the operator's thread at the mercy of the engine
    /// and vice versa, which is the coupling this whole module exists to avoid.
    #[must_use]
    pub fn request(&self) -> Option<Snapshot> {
        self.0.wanted.store(true, Ordering::Release);
        if self.0.published.load(Ordering::Acquire) == 0 {
            return None;
        }
        self.0.cell.lock().ok().map(|s| *s)
    }

    /// Append every event the engine has recorded, oldest first, and remove
    /// them from the ring. Returns how many were appended.
    ///
    /// **A `Vec` on purpose.** The reader is not the engine thread and may
    /// allocate; the engine's side of this is a fixed ring and allocates
    /// nothing, which is what non-negotiable 1 is about. Reusing one `Vec`
    /// across calls costs nothing after the first.
    ///
    /// Whatever it could not keep is counted by [`Self::events_lost`], which is
    /// **not** implied by the return value — a drain of zero with a rising loss
    /// count means the reader is being starved by its own timing, not that
    /// nothing happened.
    pub fn events(&self, out: &mut Vec<Event>) -> usize {
        self.0.events.drain(out)
    }

    /// How many events were never delivered — the ring was full, or the engine
    /// could not take the lock without blocking.
    ///
    /// **Monotonic, and it is the number that makes this stream honest.** A
    /// stream that loses silently is a source an operator will trust and should
    /// not. Non-zero means read more often, or with a bigger buffer.
    #[must_use]
    pub fn events_lost(&self) -> u64 {
        self.0.events.lost.load(Ordering::Relaxed)
    }

    /// How many snapshots the engine has built since it started.
    ///
    /// For a test, and for an operator wondering whether the engine is turning
    /// at all. **It is the assertion that keeps this module's central claim
    /// honest**: an engine publishing on every turn would satisfy every other
    /// question here and pay for it on the hot path.
    #[must_use]
    pub fn published(&self) -> u64 {
        self.0.published.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SESSIONS, SessionSnapshot, Snapshot};

    fn session(id: u64, logged_on: bool) -> SessionSnapshot {
        SessionSnapshot::describe(
            id,
            logged_on,
            1,
            1,
            None,
            false,
            super::JournalHealth {
                refused: 0,
                beyond: 0,
            },
        )
    }

    /// More sessions than the array holds is a **reported fact**, not a panic
    /// and not a silent short list. `standard` has no session ceiling.
    #[test]
    fn a_snapshot_that_cannot_carry_every_session_says_so() {
        let mut snap = Snapshot::default();
        for i in 0..MAX_SESSIONS {
            snap.push(session(i as u64, true));
        }
        assert!(!snap.truncated(), "exactly full is not truncated");
        assert_eq!(snap.sessions().len(), MAX_SESSIONS);

        snap.push(session(MAX_SESSIONS as u64, true));
        assert!(snap.truncated(), "one past the end is");
        assert_eq!(
            snap.sessions().len(),
            MAX_SESSIONS,
            "and the ones that fit are still all there"
        );
    }

    /// The health probe, item 30 (f). Each clause gets its own case, because a
    /// probe that is wrong in one direction reports a healthy engine as dead at
    /// 3 a.m. and a dead one as healthy for a whole trading session.
    #[test]
    fn health_is_every_session_logged_on_and_no_counter_that_should_be_zero() {
        let mut snap = Snapshot::default();
        assert!(
            !snap.healthy(),
            "an engine serving nobody is not a healthy acceptor"
        );

        snap.push(session(1, true));
        assert!(snap.healthy(), "one logged-on session, nothing broken");

        snap.push(session(2, false));
        assert!(
            !snap.healthy(),
            "a connection that never logged on is exactly what a probe is for"
        );

        let mut refused = Snapshot::default();
        refused.push(session(1, true));
        refused.set_counters(1, 1, 0, 0);
        assert!(!refused.healthy(), "ADR-0011 dropped a session");

        let mut missing = Snapshot::default();
        missing.push(session(1, true));
        missing.set_counters(1, 0, 1, 0);
        assert!(
            !missing.healthy(),
            "a transport broke its own contract; the symptom is lateness"
        );

        // Truncation is a reporting limit, not a sickness.
        let mut full = Snapshot::default();
        for i in 0..=MAX_SESSIONS {
            full.push(session(i as u64, true));
        }
        assert!(full.truncated());
        assert!(
            full.healthy(),
            "more sessions than we can list is not unhealthy"
        );
    }
}

/// How many events the buffer holds before the oldest are lost.
///
/// Events are **rare** — a logon, a logout, a gap, a disconnect — not one per
/// message, which D8 would forbid. A reader polling once a second has room for
/// a burst; one that stops reading loses the oldest and
/// [`Observer::events_lost`] says how many.
pub const EVENT_CAPACITY: usize = 256;

/// Something worth telling an operator about.
///
/// Deliberately **not** one per message: that is the hot path and `DESIGN.md`
/// D8 forbids work there for an observer who may not exist. These are the state
/// changes — a session came up, a session went away and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventKind {
    /// A session finished its `Logon` exchange.
    LoggedOn,
    /// A connection ended. **The reason is the point of this whole module** —
    /// see [`fixbolt_session::DropReason`].
    Ended(fixbolt_session::DropReason),
    /// A connection ended and the session had no reason to give. **Zero on a
    /// healthy engine**: it means something ended the link without going
    /// through `Session::end`, which is a gap in this engine rather than a
    /// fault of the counterparty.
    EndedWithoutReason,
    /// An operator's [`Command`] was applied, or was not.
    ///
    /// **The audit trail is the same channel as everything else**, so one
    /// stream records both what the engine did by itself and what it did
    /// because somebody asked. An operator reading only their own outcomes
    /// would not see the disconnect that raced their command.
    Administered {
        /// Which number was aimed at.
        change: Change,
        /// The number requested.
        to: u32,
        /// What became of it.
        outcome: Outcome,
    },
    /// A `ResendRequest` reached past what the journal still holds, and the
    /// missing numbers were gap-filled instead of replayed.
    ///
    /// **This is the event `docs/plans/2026-09-03-resend-from-the-journal.md`
    /// exists for.** Filling is legal on the wire and invisible to the
    /// counterparty's engine, which sees a `SequenceReset` and moves on; what
    /// it loses is the messages themselves. Before this, an acceptor that
    /// replayed eight of a hundred said nothing at all.
    ///
    /// Non-zero means **the ring is too small for this counterparty's
    /// disconnections** — `GUIDE.md` §6 has the arithmetic for choosing a
    /// bigger one.
    /// [ADR-0046](../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md).
    ResendBeyondJournal {
        /// How many **messages** were filled over rather than replayed, on this
        /// turn. Not how many times it happened.
        filled: u32,
        /// The lowest number the journal could still answer for, or `None` if
        /// it holds nothing. What the ring would have had to reach back to.
        oldest: Option<u32>,
    },
    /// Application messages the journal would not keep.
    ///
    /// **Zero on a healthy acceptor.** Anything else means replies are longer
    /// than a journal slot: they went out on the wire, and every future
    /// `ResendRequest` covering one of them is answered with a gap fill.
    JournalRefused {
        /// How many were refused on this turn.
        count: u32,
    },
    /// Messages the message log did not manage to write.
    ///
    /// **Zero on a healthy engine, and it is not a session's fault.** A full
    /// ring means the writer thread is behind the engine thread — a slow disk,
    /// a log on a network mount, or a burst the ring was sized too small for.
    /// The log drops rather than waits, because a log that blocks the engine is
    /// worse than a log with a hole (ADR-0011's rule, pointed the other way),
    /// but the hole is **counted** so nobody reads the file believing it is
    /// complete.
    ///
    /// It counts both losses the log can suffer: a `push` the ring refused, and
    /// a record the writer's buffer could not take.
    MessageLogLost {
        /// How many records were lost since the previous turn that reported
        /// any. Not the running total — [`Snapshot::log_lost`] is that.
        count: u64,
    },
    /// An application still had something to say when
    /// [`crate::MAX_ON_LOGON`] was reached, and the engine stopped asking.
    ///
    /// **Zero on a healthy engine.** The bound is a guard against a handler
    /// that never answers `None`, not a quota, so reaching it means either a
    /// bug in the handler or a session opening that genuinely needs more than
    /// the bound — and the operator has to be able to tell which. It is an
    /// event rather than a counter because the alternative is a session that
    /// starts a few messages short and never says so
    /// ([ADR-0048](../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md)
    /// decision 3).
    SpokeFirstToTheBound {
        /// How many messages did go out. Always [`crate::MAX_ON_LOGON`].
        sent: u32,
    },
    /// A message an application handed to a [`Sender`] never reached a
    /// connection.
    ///
    /// **Zero on a healthy engine.** The connection had already gone by the
    /// time the engine drained the queue, so the session that owned its
    /// sequence numbers went with it and the message had nowhere legal to go
    /// (ADR-0048 decision 5). A queue that was *full* is not this: that is
    /// refused at [`Sender::send`], which answers `false`.
    ///
    /// [`Sender`]: crate::origin::Sender
    /// [`Sender::send`]: crate::origin::Sender::send
    OriginationUndeliverable {
        /// How many were dropped since the previous turn that reported any.
        count: u64,
    },
    /// Bytes the message log called `OUT` that never reached the wire.
    ///
    /// **Zero on every healthy ending.** `Out::push` writes the `OUT` line when
    /// a message reaches the outbound queue, which is the only moment the
    /// engine can name it; a socket that dies takes the queue with it. The
    /// lines cannot be un-written, so this says how many bytes at the tail of
    /// that connection's output the file is wrong about.
    ///
    /// It is deliberately **not** the same event as a slow consumer
    /// (ADR-0035): that one is a queue the engine refused to grow, and nothing
    /// was ever logged for it. This one is a queue the engine had already
    /// promised.
    MessageLogUnsent {
        /// How many bytes were discarded.
        bytes: usize,
    },
}

/// One event, with enough context to act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    id: ConnId,
    at_ms: u64,
    kind: EventKind,
}

impl Event {
    /// Which connection.
    #[must_use]
    pub const fn id(&self) -> ConnId {
        self.id
    }

    /// When, on the engine's clock.
    #[must_use]
    pub const fn at_ms(&self) -> u64 {
        self.at_ms
    }

    /// What happened.
    #[must_use]
    pub const fn kind(&self) -> EventKind {
        self.kind
    }
}

/// A fixed ring of events plus a count of what it could not keep.
///
/// **The count is not optional.** An event stream that loses silently is worse
/// than no event stream: it is a source an operator will trust and should not.
#[derive(Debug)]
pub(crate) struct Events {
    ring: Mutex<EventRing>,
    /// Bumped when the engine could not take the lock, or when the ring was
    /// full. Read without the lock, so a reader can always learn it has missed
    /// something even while the engine holds the cell.
    lost: AtomicU64,
}

#[derive(Debug)]
struct EventRing {
    slots: [Option<Event>; EVENT_CAPACITY],
    head: usize,
    len: usize,
}

impl Events {
    pub(crate) fn new() -> Self {
        Self {
            ring: Mutex::new(EventRing {
                slots: [None; EVENT_CAPACITY],
                head: 0,
                len: 0,
            }),
            lost: AtomicU64::new(0),
        }
    }

    /// Record an event, unless the reader holds the ring right now.
    ///
    /// `try_lock`, never `lock` — non-negotiable 4. A refusal counts as a loss
    /// rather than blocking the engine thread, which is the trade this design
    /// makes and states.
    pub(crate) fn push(&self, e: Event) {
        let Ok(mut r) = self.ring.try_lock() else {
            self.lost.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if r.len == EVENT_CAPACITY {
            // Full: drop the oldest, and say so.
            let head = r.head;
            r.slots[head] = Some(e);
            r.head = (head + 1) % EVENT_CAPACITY;
            self.lost.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let at = (r.head + r.len) % EVENT_CAPACITY;
        r.slots[at] = Some(e);
        r.len += 1;
    }

    fn drain(&self, out: &mut Vec<Event>) -> usize {
        let Ok(mut r) = self.ring.lock() else {
            return 0;
        };
        let mut n = 0;
        while r.len > 0 {
            let head = r.head;
            if let Some(e) = r.slots[head].take() {
                out.push(e);
                n += 1;
            }
            r.head = (head + 1) % EVENT_CAPACITY;
            r.len -= 1;
        }
        n
    }
}

// --- administration ------------------------------------------------------
//
// The other direction. Everything above carries facts from the engine thread
// outwards; everything below carries instructions inwards, through the same
// `Arc` and the same fixed shapes.

/// How many commands may wait for the engine's next turn.
///
/// Small on purpose. A command is a human action — somebody on the phone at
/// 3 a.m. — not a data stream, and a full queue means something is submitting
/// in a loop rather than that the engine is behind.
pub const COMMAND_CAPACITY: usize = 32;

/// Which number an [`Command`] moved, for the record it leaves behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Change {
    /// The number the next outbound message will carry.
    NextOut,
    /// The number the session next expects to receive.
    NextIn,
    /// A `SequenceReset` was sent and the outbound number followed it.
    SequenceReset,
}

/// What became of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// It was applied to the session named.
    Applied,
    /// No connection with that [`ConnId`] is on this engine any more. **The
    /// ordinary answer for a command that raced a disconnect**, and the reason
    /// commands report an outcome rather than returning one at submit time.
    NoSuchConnection,
    /// The session refused it — `n == 0`, or a `SequenceReset` that could not
    /// be built.
    Refused,
}

/// An instruction for one session, applied on the engine's next turn.
///
/// **Sequence-number administration is the whole of it today.** `STATUS.md`
/// item 30 (c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// Set the next outbound number. **Says nothing on the wire** — see
    /// [`fixbolt_session::Session::set_next_out`], which this calls.
    SetNextOut {
        /// The connection to administer.
        id: ConnId,
        /// The number to set.
        n: u32,
    },
    /// Set the number the session next expects. Local, and not a lie.
    SetNextIn {
        /// The connection to administer.
        id: ConnId,
        /// The number to set.
        n: u32,
    },
    /// Send `35=4` with `123=N` and `36=n`, then become `n`. **The honest way
    /// to change an outbound number.**
    SendSequenceReset {
        /// The connection to administer.
        id: ConnId,
        /// The number the next message will carry.
        n: u32,
    },
}

impl Command {
    /// Which connection this is aimed at.
    #[must_use]
    pub const fn id(&self) -> ConnId {
        match *self {
            Self::SetNextOut { id, .. }
            | Self::SetNextIn { id, .. }
            | Self::SendSequenceReset { id, .. } => id,
        }
    }

    /// Which number it moves, for the event it leaves behind.
    #[must_use]
    pub const fn change(&self) -> Change {
        match *self {
            Self::SetNextOut { .. } => Change::NextOut,
            Self::SetNextIn { .. } => Change::NextIn,
            Self::SendSequenceReset { .. } => Change::SequenceReset,
        }
    }

    /// The number requested.
    #[must_use]
    pub const fn to(&self) -> u32 {
        match *self {
            Self::SetNextOut { n, .. }
            | Self::SetNextIn { n, .. }
            | Self::SendSequenceReset { n, .. } => n,
        }
    }
}

/// A fixed queue of commands waiting for the engine's next turn.
///
/// # Why this one may not drop and [`Events`] may
///
/// An event that is lost is a fact an operator did not learn, and the loss is
/// counted so they learn *that* instead. **A command that is lost is an action
/// that silently did not happen**, and there is no counter that makes that
/// acceptable. So the engine's `try_lock` failing leaves the queue exactly as
/// it was, for the next turn.
///
/// The asymmetry that makes it work: **the submitting thread is allowed to
/// block and the engine thread is not** (non-negotiable 4). So the operator
/// takes the lock and the engine tries.
#[derive(Debug)]
pub(crate) struct Commands {
    queue: Mutex<CommandQueue>,
    /// How many are waiting. **Read before the lock is even attempted**, so a
    /// turn on an engine nobody is administering costs one relaxed load and not
    /// a `try_lock` — the same bargain [`Shared::wanted`] makes for snapshots.
    waiting: AtomicUsize,
    /// How many times the engine has reached for the lock. **The number that
    /// keeps *"one relaxed load"* honest** — an implementation that attempts a
    /// mutex every turn passes every content assertion and fails this one.
    /// Exactly the role [`Shared::published`] plays for snapshots, and it
    /// exists because that gap has already been found once here.
    drains: AtomicU64,
}

#[derive(Debug)]
struct CommandQueue {
    slots: [Option<Command>; COMMAND_CAPACITY],
    head: usize,
    len: usize,
}

impl Commands {
    pub(crate) const fn new() -> Self {
        Self {
            queue: Mutex::new(CommandQueue {
                slots: [None; COMMAND_CAPACITY],
                head: 0,
                len: 0,
            }),
            waiting: AtomicUsize::new(0),
            drains: AtomicU64::new(0),
        }
    }

    /// Called from the operator's thread, which may block.
    ///
    /// `false` means the queue is full — the operator learns it **now**, at the
    /// call, rather than by the command quietly never happening.
    fn submit(&self, c: Command) -> bool {
        let Ok(mut q) = self.queue.lock() else {
            return false;
        };
        if q.len == COMMAND_CAPACITY {
            return false;
        }
        let at = (q.head + q.len) % COMMAND_CAPACITY;
        q.slots[at] = Some(c);
        q.len += 1;
        // Release, and inside the lock: the engine's relaxed load may be stale
        // by one turn, which costs a turn's delay and never a lost command.
        self.waiting.store(q.len, Ordering::Release);
        true
    }

    /// Called on the engine thread. `try_lock`, never `lock`.
    ///
    /// Fills `out` and returns how many were taken. **A refused lock takes
    /// nothing and loses nothing**: the queue is untouched and the next turn
    /// tries again.
    pub(crate) fn drain(&self, out: &mut [Option<Command>; COMMAND_CAPACITY]) -> usize {
        self.drains.fetch_add(1, Ordering::Relaxed);
        let Ok(mut q) = self.queue.try_lock() else {
            return 0;
        };
        // Cleared inside the lock so a submit racing this one cannot be lost:
        // the submit takes the lock, so it either lands before this read or
        // after this store.
        let n = q.len;
        for slot in out.iter_mut().take(n) {
            let head = q.head;
            *slot = q.slots[head].take();
            q.head = (head + 1) % COMMAND_CAPACITY;
        }
        q.len = 0;
        self.waiting.store(0, Ordering::Release);
        n
    }

    /// Is there anything to take? **One relaxed load**, and the whole cost of
    /// an engine that is administrable while nobody is administering it.
    pub(crate) fn waiting(&self) -> bool {
        self.waiting.load(Ordering::Relaxed) != 0
    }

    fn drains(&self) -> u64 {
        self.drains.load(Ordering::Relaxed)
    }
}

/// The handle that can **change** a running engine, as [`Observer`] is the one
/// that can only look.
///
/// Two handles over one `Arc` and one mechanism. What is separated is the
/// capability: everything that watches can hold an `Observer`, and only what
/// administers needs this.
///
/// `Send + Sync`. Hold it on whatever thread takes the 3 a.m. phone call.
#[derive(Debug, Clone)]
pub struct Admin(pub(crate) std::sync::Arc<Shared>);

impl Admin {
    /// Queue a command for the engine's next turn.
    ///
    /// **`true` means queued, not done.** The engine applies it on its next
    /// turn and the outcome arrives on the event stream as
    /// [`EventKind::Administered`] — which is where a command that named a
    /// connection that no longer exists reports itself, and there is no way to
    /// know that at submit time.
    ///
    /// `false` means the queue is full ([`COMMAND_CAPACITY`]) and **nothing was
    /// taken**. Unlike a lost event, a lost command is never silent.
    pub fn submit(&self, c: Command) -> bool {
        self.0.commands.submit(c)
    }

    /// How many times the engine has reached for the command queue.
    ///
    /// **This is what keeps *"a turn costs one relaxed load"* falsifiable.** An
    /// engine that attempts the lock every turn behaves identically in every
    /// other respect and is a different bargain entirely;
    /// `crates/engine/tests/admin.rs::an_engine_nobody_is_administering_does_not_reach_for_the_lock`
    /// is what notices.
    #[must_use]
    pub fn drains(&self) -> u64 {
        self.0.commands.drains()
    }

    /// Ask the engine to stop, and say how long it may wait for goodbyes.
    ///
    /// Not a [`Command`], because it is not about one connection: it is the
    /// engine's own life. It rides the same `Arc` and the same capability
    /// split — an [`Observer`] cannot do this and an `Admin` can.
    ///
    /// **This returns immediately.** The engine says goodbye to every session
    /// on its next turn and then waits, up to `grace_ms` on **its own clock**,
    /// for each counterparty to answer. What it managed comes back from
    /// `Engine::run`, `serve` or `serve_hft` as a [`crate::Shutdown`].
    ///
    /// Asking twice is harmless; the first grace period stands, because a
    /// second call must not be able to extend a shutdown already under way.
    pub fn shutdown(&self, grace_ms: u64) {
        if self.0.stop.load(Ordering::Acquire) {
            return;
        }
        // The number first, then the flag, and the flag is `Release`: a turn
        // that sees the flag is guaranteed to see this.
        self.0.stop_grace_ms.store(grace_ms, Ordering::Relaxed);
        self.0.stop.store(true, Ordering::Release);
    }

    /// The same events [`Observer::events`] gives, so a thread that
    /// administers can read the outcome of what it did without holding a second
    /// handle.
    pub fn events(&self, out: &mut Vec<Event>) -> usize {
        self.0.events.drain(out)
    }
}
