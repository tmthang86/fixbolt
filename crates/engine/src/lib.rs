//! The engine: TCP on one side, [`fixbolt_session`] on the other, and a thread
//! that never sleeps in the kernel.
//!
//! `DESIGN.md` D8 is the shape of this crate: the loop spins on non-blocking
//! sockets, and a blocking call on the engine thread is a bug rather than a
//! style choice (`CLAUDE.md` §2 non-negotiable 4).
//!
//! # One turn at a time
//!
//! [`Engine::turn`] is one non-blocking pass over every connection.
//! [`Engine::run`] is `loop { turn(); wait.idle() }` and nothing more. Keeping
//! the pass separate from the loop is what makes the 59 acceptance definitions
//! runnable **through a real socket** without a background thread, a sleep, or
//! a timing window — `crates/engine/tests/wire.rs` drives `turn` by hand and is
//! as deterministic as the in-process gate.

// ADR-0015 decision 9, and non-negotiable 6 again: the feature gates the `mod`
// declaration, not only the manifest. `cfg(target_os = "linux")` on top of it
// because `sched_setaffinity` is a Linux interface — on anything else the
// module does not exist, so code written against it fails to compile rather
// than failing at startup. Same shape as `standard` above.
#[cfg(all(feature = "affinity", target_os = "linux"))]
pub mod affinity;
// Step 4 of threads-and-affinity. Gated with `affinity` because a shard without
// a pinned core is just an engine, and the whole point of the runtime is that
// each one has a core of its own.
pub mod backpressure;
pub mod clock;
pub mod conn;
pub mod dispatch;
pub mod frame;
pub mod journal;
pub mod observe;
pub mod presession;
pub mod recovery;
#[cfg(all(feature = "affinity", target_os = "linux"))]
pub mod shard;
// ADR-0014 decision 1 and 2. The feature gates the `mod` declaration itself,
// not only the manifest — non-negotiable 6, and `CLAUDE.md` §10 lists getting
// this wrong as a standing trap. `cfg(unix)` on top of it is decision 2: on a
// target with no poller `standard` does not exist, so code written against it
// does not compile there rather than failing at startup.
#[cfg(all(feature = "standard", unix))]
pub mod block;
#[cfg(all(feature = "standard", unix))]
pub mod poll;
pub mod ring;
pub mod transport;
pub mod wait;
#[cfg(all(feature = "standard", unix))]
pub mod waker;

use std::net::{TcpListener, TcpStream};

pub use fixbolt_session::{Application, Config, Role, Session};

use crate::backpressure::Backpressure;
use crate::clock::Clock;
use crate::conn::{Connection, Turn};
use crate::dispatch::{ConnId, Dispatch, InlineDispatch};
use crate::transport::{Interest, Source, TcpTransport, Transport};
use crate::wait::Waiting;
use fixbolt_session::journal::Journal as SessionJournal;

/// A running engine: the connections it holds, and what drives them.
///
/// Every size is the caller's: `N` the session's field index, `RX` a
/// connection's receive buffer, `TX` its outbound queue.
pub struct Engine<T, R: Role, D, C, W, J, const N: usize, const RX: usize, const TX: usize> {
    conns: Vec<Connection<T, R, J, N, RX, TX>>,
    /// What an idle turn waits on, rebuilt in place every time it is needed.
    ///
    /// A [`Source`] borrows a descriptor rather than owning one, so this list is
    /// **never** carried across a turn: a connection that hung up between two
    /// turns would leave behind a number the kernel has already reissued.
    interests: Vec<Interest>,
    /// Connections that claimed to be pollable and then produced no source.
    ///
    /// Zero on a healthy engine. Anything else is a `Transport` breaking its own
    /// contract, and the symptom is not a crash — it is that connection's
    /// messages arriving up to one whole timeout late. Counted so the failure is
    /// visible rather than merely slow. See [`Self::sources_missing`].
    sources_missing: usize,
    /// Connections ended because the dispatch would not take a message.
    ///
    /// **Zero on a healthy engine.** Non-zero means the application behind the
    /// ring fell far enough behind that ADR-0011's policy fired and the session
    /// was dropped. The counterparty was told why — see
    /// [`crate::backpressure::SLOW_APPLICATION`] — so this counter is the same
    /// event seen from the inside. See [`Self::refused_connections`].
    refused_connections: usize,
    cfg: Config,
    dispatch: D,
    clock: C,
    wait: W,
    /// The next connection id. Only ever counts up, so an id is never reused
    /// and a reply that arrives after a hang-up matches nothing.
    next_id: ConnId,
    /// What every connection this engine takes on does when its outbound queue
    /// fills. `DESIGN.md` D10.
    backpressure: Backpressure,
    /// What an operator reads, when there is one. `None` until
    /// [`Self::observer`] is called, so an engine nobody watches carries a
    /// null-pointer-sized field and does no work at all — `observe`'s whole
    /// design.
    observe: Option<std::sync::Arc<crate::observe::Shared>>,
    /// The self-pipe another thread writes to when it has produced work.
    ///
    /// `None` unless [`Self::with_waker`] was called. Only `standard` needs
    /// one: a spinning engine sees out-of-band work on its next turn anyway.
    #[cfg(all(feature = "standard", unix))]
    waker: Option<crate::waker::Waker>,
}

/// One connection's view of the dispatch, as the [`Application`] the session
/// takes.
///
/// The session's API is `received_with(bytes, app, emit)` and knows nothing
/// about connections; the dispatch routes by connection and knows nothing about
/// sessions. This is the four-line adapter between them, and it holds a
/// borrow rather than a copy, so it costs nothing.
struct Deliver<'a, D> {
    dispatch: &'a mut D,
    conn: ConnId,
}

impl<D: Dispatch> Application for Deliver<'_, D> {
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<core::ops::Range<usize>> {
        self.dispatch.deliver(self.conn, msg, seq, stamp, out)
    }
}

impl<T, R, D, C, W, J, const N: usize, const RX: usize, const TX: usize>
    Engine<T, R, D, C, W, J, N, RX, TX>
where
    T: Transport,
    R: Role,
    D: Dispatch,
    C: Clock,
    W: Waiting,
    J: SessionJournal,
{
    /// An engine with no connections yet.
    ///
    /// `capacity` is reserved once, here, so that adding a connection later
    /// does not allocate on a thread that must not — non-negotiable 1.
    pub fn new(cfg: Config, dispatch: D, clock: C, wait: W, capacity: usize) -> Self {
        Self {
            conns: Vec::with_capacity(capacity),
            // Two more than the connections: `serve` adds the listener, and the
            // out-of-band waker is one more. Going over is not fatal — it costs
            // one allocation on a path that must not have any, which is a
            // sizing mistake rather than a steady state.
            interests: Vec::with_capacity(capacity + 2),
            sources_missing: 0,
            refused_connections: 0,
            cfg,
            dispatch,
            clock,
            wait,
            next_id: 0,
            backpressure: Backpressure::Disconnect,
            observe: None,
            #[cfg(all(feature = "standard", unix))]
            waker: None,
        }
    }

    /// The same engine, with a different backpressure policy for every
    /// connection it takes on from now (D10). The default is
    /// [`Backpressure::Disconnect`].
    #[must_use]
    pub const fn with_backpressure(mut self, policy: Backpressure) -> Self {
        self.backpressure = policy;
        self
    }

    /// The same engine, woken by `waker` when another thread produces work.
    ///
    /// **Only an out-of-band dispatch needs this.** With
    /// [`dispatch::InlineDispatch`] the application runs on the engine thread
    /// and there is nobody else to wake. With [`dispatch::RingDispatch`] the
    /// application is elsewhere, and without a waker its reply waits until this
    /// engine's timeout expires — up to 100 ms, on the one path such an
    /// application cares most about.
    ///
    /// The engine keeps the read end, puts it in its own poll set, and drains
    /// it after every wait. The caller keeps the [`waker::WakeHandle`] and
    /// gives it to whoever produces the work.
    #[cfg(all(feature = "standard", unix))]
    #[must_use]
    pub fn with_waker(mut self, waker: crate::waker::Waker) -> Self {
        self.waker = Some(waker);
        self
    }

    /// Take on a connection that is already open, and tell its session so.
    ///
    /// Returns the id a reply from another thread is routed by.
    ///
    /// The journal is `J::default()`. A deployment whose journal needs a name
    /// per connection — a file, say — uses [`Self::add_with_journal`].
    pub fn add(&mut self, transport: T) -> ConnId
    where
        J: Default,
    {
        self.add_with_journal(transport, J::default())
    }

    /// As [`Self::add`], with a journal the caller built.
    pub fn add_with_journal(&mut self, transport: T, journal: J) -> ConnId {
        let id = self.next_id;
        self.next_id += 1;
        let mut conn = Connection::new(id, transport, Session::new(self.cfg), journal)
            .with_backpressure(self.backpressure);
        conn.opened();
        self.conns.push(conn);
        id
    }

    /// As [`Self::add_with_journal`], continuing a session that outlived the
    /// process.
    ///
    /// **This is the only way an `Engine` resumes anything.** `[verified
    /// 2026-09-02]` before it existed, every `add` built `Session::new`, which
    /// resets — so `Journal::highest`, `Session::resume`,
    /// [ADR-0010](../../../docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md),
    /// [ADR-0017](../../../docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md)
    /// and `Durability::Fsync` were all real, all tested, and all unreachable
    /// through this type. `STATUS.md` item 31.
    ///
    /// **The engine does not guess, and it does not read the journal for you.**
    /// ADR-0010's whole point is that choosing between a restart and a
    /// continuation is the caller's; this is where they say. `next_out` is
    /// usually `journal.highest() + 1` and `next_in` `journal.highest_in() + 1`,
    /// but *usually* is not *always* and the engine has no business deciding
    /// which.
    ///
    /// **The journal is taken as well as the numbers, and that is not a
    /// convenience.** Correct counts over an empty journal answer the first
    /// `ResendRequest` with a `SequenceReset` gap fill — legal, and a silent
    /// loss of everything the counterparty is asking for.
    /// `tests/engine_recovery.rs::a_resumed_session_replays_what_it_sent_before_the_restart`
    /// is what holds that.
    ///
    /// `last_active_ms` is the instant this session was last known to be
    /// active, on the scale [`crate::clock::Clock`] uses. Supply it and a
    /// schedule boundary crossed since then restarts both counts
    /// ([ADR-0033](../../../docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md));
    /// pass `None` and no boundary is ever noticed, which is right under
    /// [`Schedule::always`](fixbolt_session::schedule::Schedule::always) and
    /// wrong under anything else. `Session::last_active_ms` is what to persist.
    pub fn add_resumed(
        &mut self,
        transport: T,
        cfg: Config,
        journal: J,
        next_out: u32,
        next_in: u32,
        last_active_ms: Option<u64>,
    ) -> ConnId {
        let id = self.next_id;
        self.next_id += 1;
        let session = match last_active_ms {
            Some(at) => Session::resume_at(cfg, next_out, next_in, at),
            None => Session::resume(cfg, next_out, next_in),
        };
        let mut conn =
            Connection::new(id, transport, session, journal).with_backpressure(self.backpressure);
        conn.opened();
        self.conns.push(conn);
        id
    }

    /// As [`Self::add`], with bytes that were read off the socket before the
    /// engine took it on.
    ///
    /// The pre-session stage ([`presession`]) owns a socket until a `Logon`
    /// arrives, so by the time the engine sees it those bytes are already gone
    /// from the kernel. They go straight into the connection's receive buffer,
    /// and the first `turn` frames them exactly as if this engine had read
    /// them.
    ///
    /// **Everything the stage read**, not just the `Logon` it routed by: a
    /// counterparty may pipeline behind its `Logon`, and those bytes belong to
    /// the session too.
    ///
    /// # Errors
    ///
    /// [`PrefixTooLong`] when the bytes exceed this engine's `RX`. Refused
    /// rather than truncated — see [`conn::Connection::prime`].
    pub fn add_with_prefix(&mut self, transport: T, prefix: &[u8]) -> Result<ConnId, PrefixTooLong>
    where
        J: Default,
    {
        self.add_with_prefix_and_config(transport, self.cfg, prefix)
    }

    /// As [`Self::add_with_prefix`], for a counterparty this engine's own
    /// `Config` does not name.
    ///
    /// **This is what makes one engine an acceptor rather than a link.** The
    /// pre-session stage asked a [`presession::Registry`] which configuration
    /// serves the identity on the `Logon`; that configuration arrives here, and
    /// the connection's `Session` is built with it rather than with the
    /// engine's default —
    /// [ADR-0030](../../../docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md).
    ///
    /// `Engine::new`'s `Config` remains the default for [`Self::add`], which is
    /// what an engine driven without a pre-session stage still uses — the
    /// acceptance corpus runs that way in `tests/wire.rs`.
    ///
    /// # Errors
    ///
    /// [`PrefixTooLong`] when the bytes exceed this engine's `RX`.
    pub fn add_with_prefix_and_config(
        &mut self,
        transport: T,
        cfg: Config,
        prefix: &[u8],
    ) -> Result<ConnId, PrefixTooLong>
    where
        J: Default,
    {
        self.add_with_prefix_config_and_state(transport, cfg, prefix, None)
    }

    /// As [`Self::add_with_prefix_and_config`], optionally continuing a session
    /// that outlived the process.
    ///
    /// `state` is what [`crate::recovery::Recovery`] answered: `None` starts
    /// fresh, which is what the serving loop did before recovery existed and
    /// what [`crate::recovery::NoRecovery`] still means.
    ///
    /// **This is the seam `serve_with_recovery` needs.** The pre-session stage
    /// owns the socket until a `Logon` names the counterparty, so the engine is
    /// handed a transport, a `Config` and some bytes all at once — and a
    /// journal, if the caller found one.
    ///
    /// # Errors
    ///
    /// [`PrefixTooLong`] if the bytes already read will not fit `RX`.
    pub fn add_with_prefix_config_and_state(
        &mut self,
        transport: T,
        cfg: Config,
        prefix: &[u8],
        state: Option<crate::recovery::Resumed<J>>,
    ) -> Result<ConnId, PrefixTooLong>
    where
        J: Default,
    {
        if prefix.len() > RX {
            return Err(PrefixTooLong {
                got: prefix.len(),
                capacity: RX,
            });
        }
        let id = self.next_id;
        self.next_id += 1;
        let (session, journal) = match state {
            Some(r) => {
                let s = match r.last_active_ms {
                    Some(at) => Session::resume_at(cfg, r.next_out, r.next_in, at),
                    None => Session::resume(cfg, r.next_out, r.next_in),
                };
                (s, r.journal)
            }
            None => (Session::new(cfg), J::default()),
        };
        let mut conn =
            Connection::new(id, transport, session, journal).with_backpressure(self.backpressure);
        if !conn.prime(prefix) {
            self.next_id -= 1;
            return Err(PrefixTooLong {
                got: prefix.len(),
                capacity: RX,
            });
        }
        conn.opened();
        self.conns.push(conn);
        Ok(id)
    }

    /// A handle another thread reads this engine's state through.
    ///
    /// **Calling this is what makes the engine observable at all.** Until then
    /// the field is `None` and a turn does nothing about it; afterwards a turn
    /// does one relaxed load, and builds a snapshot only when somebody has
    /// asked. `STATUS.md` open item 30 (b).
    ///
    /// One allocation, here, never on a turn. Calling it twice hands out two
    /// handles onto the same shared cell rather than two cells.
    pub fn observer(&mut self) -> crate::observe::Observer {
        let shared = self
            .observe
            .get_or_insert_with(|| std::sync::Arc::new(crate::observe::Shared::new()));
        crate::observe::Observer(std::sync::Arc::clone(shared))
    }

    /// Describe every connection this engine holds, for [`Self::observer`].
    ///
    /// An associated function taking the slice rather than a method, so the
    /// borrow of `conns` ends before `turn` takes its mutable one.
    ///
    /// **No allocation**: `Snapshot` is a fixed array on the stack, and the one
    /// `Arc` this mechanism needs was allocated in [`Self::observer`].
    /// `benches/alloc.rs` cases `observe-idle` and `observe-asked` are what
    /// prove it, not this comment.
    fn snapshot(
        conns: &[Connection<T, R, J, N, RX, TX>],
        refused_connections: usize,
        sources_missing: usize,
    ) -> crate::observe::Snapshot {
        let mut snap = crate::observe::Snapshot::default();
        for c in conns {
            snap.push(crate::observe::SessionSnapshot::describe(
                c.id,
                c.session.is_logged_on(),
                c.session.next_out(),
                c.session.next_in(),
                c.session.last_skew_ms(),
                c.has_pending_output(),
            ));
        }
        snap.set_counters(conns.len(), refused_connections, sources_missing);
        snap
    }

    /// How many connections are live.
    #[must_use]
    pub fn connections(&self) -> usize {
        self.conns.len()
    }

    /// The dispatch, and through it whatever the application recorded.
    pub const fn dispatch_mut(&mut self) -> &mut D {
        &mut self.dispatch
    }

    /// The clock, for a caller that owns it.
    ///
    /// A deployment never touches this — `SystemClock` reads itself. The
    /// acceptance corpus does: it writes a fixed instant into every message it
    /// sends, so the harness has to be the engine's clock or nothing passes the
    /// 120-second skew check.
    pub const fn clock_mut(&mut self) -> &mut C {
        &mut self.clock
    }

    /// One non-blocking pass over every connection. `true` if anything moved.
    ///
    /// Nothing in here can block: every transport call is non-blocking and
    /// every "nothing yet" is [`transport::Io::Idle`].
    pub fn turn(&mut self) -> bool {
        let now = self.clock.now_ms();
        // Being observable costs this, and only this, while nobody is
        // observing: one relaxed load through an `Option` that is `None` on an
        // engine whose `observer()` was never called. See `observe`.
        if let Some(shared) = self.observe.as_ref()
            && shared.wanted()
        {
            let snap = Self::snapshot(&self.conns, self.refused_connections, self.sources_missing);
            shared.publish(&snap);
        }
        let mut moved = false;
        let mut i = 0;
        while i < self.conns.len() {
            // **One identity, one connection**, and it is asked before the
            // message is judged because `1b_DuplicateIdentity.def` and
            // `AlreadyLoggedOn.def` both expect no reply at all on the second.
            // Counted first so the immutable borrow ends before the mutable
            // one begins.
            //
            // `[2026-09-01]` **the comparison is the identity, not merely the
            // count.** This used to be `c.session.is_logged_on()` with nothing
            // else, which was right only while an `Engine` held one `Config` and
            // therefore one identity. It now holds as many as the pre-session
            // stage's registry has entries ([ADR-0030]), and
            // `1b_DuplicateIdentity.def`'s own comment says which rule this is:
            // *"If two logons with the same SenderCompID/TargetCompID
            // combination logon the second one must be disconnected."* Counting
            // without comparing refused the second **counterparty** as though it
            // were the second **connection**.
            //
            // [ADR-0030]: ../../../docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md
            let mine = self.conns[i].session.config();
            let others_on = self
                .conns
                .iter()
                .enumerate()
                .filter(|(j, c)| {
                    *j != i
                        && c.session.is_logged_on()
                        && c.session.config().same_identity_as(&mine)
                })
                .count();

            let mut deliver = Deliver {
                dispatch: &mut self.dispatch,
                conn: self.conns[i].id,
            };
            let outcome = self.conns[i].turn(now, &mut deliver, |msg| {
                others_on > 0 && presession::is_logon(msg)
            });
            // Asked here and nowhere else. `Deliver` above was built for this
            // connection's id and nothing else has run since, so a refusal
            // belongs to `conns[i]` — ADR-0011 decision 1, and the reason the
            // signal needs no id. `InlineDispatch` takes the default `false`
            // and this whole arm folds away.
            let refused = self.dispatch.take_refusal();
            match outcome {
                Turn::Up(m) => {
                    moved |= m;
                    if refused {
                        self.refused_connections += 1;
                        self.conns[i].slow_application();
                        moved = true;
                    }
                    i += 1;
                }
                Turn::Gone => {
                    // Already going; a Logout would have nowhere to go, but the
                    // count is still the truth about why.
                    if refused {
                        self.refused_connections += 1;
                    }
                    self.conns.swap_remove(i);
                    moved = true;
                }
            }
        }

        // Anything the application produced on another thread. The constant is
        // `false` for `InlineDispatch`, so this whole block compiles away
        // rather than costing a branch on the commonest engine there is.
        if D::OUT_OF_BAND {
            let conns = &mut self.conns;
            let mut any = false;
            self.dispatch.collect(|id, msg| {
                any = true;
                if let Some(c) = conns.iter_mut().find(|c| c.id == id) {
                    c.send_application(msg);
                }
                // A reply for a connection that has gone is dropped, on
                // purpose: the session that owned its sequence numbers is gone
                // with it, and sending it anywhere else would be worse.
            });
            moved |= any;
        }
        moved
    }

    /// Rebuild the list of sources an idle turn should wait on, and return it.
    ///
    /// One [`Interest`] per connection: **readable always**, and **writable
    /// only while that connection still has bytes queued**. The asymmetry is
    /// the point — a socket is almost always ready to accept bytes, so asking
    /// for writability unconditionally would wake the engine continuously and
    /// turn `standard` back into a spin.
    ///
    /// Rebuilt rather than cached, every time. A [`Source`] borrows a
    /// descriptor; one kept from a previous turn can name a socket that has
    /// since closed and been reissued to somebody else.
    ///
    /// Public because it is the only way to *see* this list. Every way of
    /// getting it wrong produces an engine that still works and is one timeout
    /// slower, and timing tests for that are exactly the flaky kind — so
    /// `crates/engine/tests/standard.rs` reads the list directly instead.
    pub fn refresh_interests(&mut self) -> &[Interest] {
        self.refresh_interests_with(&[])
    }

    /// As [`Self::refresh_interests`], also carrying sources this engine does
    /// not own — the listener, the waker.
    ///
    /// **This is the exact list [`Self::idle_with`] waits on**, and it is public
    /// for that reason. `[measured 2026-08-30]` the first version of this step
    /// had no such method, so the test for "the listener reaches the poll set"
    /// built the list by hand and appended the listener itself. It passed. It
    /// also passed with `idle_with`'s append **deleted**, because it never went
    /// near that code — a test named after a behaviour it did not exercise.
    pub fn refresh_interests_with(&mut self, extra: &[Interest]) -> &[Interest] {
        // The engine's own waker joins the list here rather than being left to
        // the caller. Leaving it out is the whole failure this mechanism
        // exists to prevent, so it is not something a call site can forget.
        #[cfg(all(feature = "standard", unix))]
        let own = self.waker.as_ref().map(|w| Interest::readable(w.source()));
        #[cfg(all(feature = "standard", unix))]
        let own = own.as_slice();
        #[cfg(not(all(feature = "standard", unix)))]
        let own: &[Interest] = &[];

        Self::rebuild(
            &self.conns,
            &mut self.interests,
            &mut self.sources_missing,
            own,
            extra,
        );
        &self.interests
    }

    /// How many connections claimed to be pollable and produced no source.
    ///
    /// Zero on a healthy engine. Anything else means those connections are not
    /// in the poll set, so their traffic waits for the timeout instead of for
    /// the data — a working engine, and a slow one.
    #[must_use]
    pub const fn sources_missing(&self) -> usize {
        self.sources_missing
    }

    /// Connections ended because the dispatch refused a message.
    ///
    /// **Zero on a healthy engine.** ADR-0011 decision 2 says the refusal is
    /// never silent, and it is visible in two places: here, for whoever embeds
    /// the engine, and in the `58=` of the `Logout` the counterparty receives.
    /// A counter alone would be a struct field nobody reads, which is the
    /// failure that ADR exists to end.
    ///
    /// Proven by `crates/engine/tests/dispatch.rs`.
    #[must_use]
    pub const fn refused_connections(&self) -> usize {
        self.refused_connections
    }

    fn rebuild(
        conns: &[Connection<T, R, J, N, RX, TX>],
        interests: &mut Vec<Interest>,
        missing: &mut usize,
        own: &[Interest],
        extra: &[Interest],
    ) {
        interests.clear();
        *missing = 0;
        let wanted = conns.len() + own.len() + extra.len();
        if interests.capacity() < wanted {
            interests.reserve(wanted - interests.capacity());
        }
        for conn in conns {
            match conn.source() {
                Some(source) if conn.has_pending_output() => {
                    interests.push(Interest::readable_and_writable(source));
                }
                Some(source) => interests.push(Interest::readable(source)),
                None => *missing += 1,
            }
        }
        interests.extend_from_slice(own);
        interests.extend_from_slice(extra);
    }

    /// One idle turn, by the chosen [`Waiting`] strategy.
    pub fn idle(&mut self) {
        self.idle_with(&[]);
    }

    /// One idle turn, also waiting on sources this engine does not own.
    ///
    /// **The listener is the one that matters.** [`Acceptor`] is deliberately
    /// not part of [`Engine`] — the acceptance corpus drives an engine over
    /// [`transport::Loopback`], which needs no port — so whoever holds both has
    /// to hand the listener over here. Leave it out and a new connection waits
    /// up to a whole timeout to be accepted: correct, and slow enough to
    /// notice in production and nowhere else.
    ///
    /// The out-of-band dispatch waker goes here too, when it exists.
    pub fn idle_with(&mut self, extra: &[Interest]) {
        // ADR-0014 decision 4. A strategy that must know the sockets, over a
        // transport that cannot name one, would block on an empty list and wake
        // only on its own timeout. That is a property of the types, known
        // before the program runs, so it is refused here rather than turned
        // into a `Result` on `add` — see `block::Block` for the `compile_fail`
        // doctest that keeps this honest.
        const {
            assert!(
                !W::NEEDS_SOURCES || T::POLLABLE,
                "this waiting strategy blocks on readiness, and this transport \
                 cannot say what to wait on. It would block on an empty list \
                 and wake only on its own timeout."
            )
        };
        // A constant, so for `Spin` and `Yield` the whole rebuild — and the
        // walk over every connection it costs — compiles away. `hft` budgets
        // 703 ns per socket per turn and would not survive paying for a list
        // nothing reads.
        if W::NEEDS_SOURCES {
            self.refresh_interests_with(extra);
            let Self {
                interests, wait, ..
            } = self;
            wait.idle(interests);
            // **After the wait, always.** A self-pipe holding an unread byte
            // stays readable, so a `poll` that is never drained returns
            // instantly for ever — a working engine, burning a core, which is
            // the one thing this mode exists to avoid.
            #[cfg(all(feature = "standard", unix))]
            if let Some(w) = &self.waker {
                w.drain();
            }
        } else {
            self.wait.idle(&[]);
        }
    }

    /// Turn forever, idling by the chosen [`Waiting`] strategy.
    ///
    /// The default strategy is [`wait::Spin`], which is D8's `hft` half: no
    /// `epoll_wait`, no futex, no blocking read.
    ///
    /// See [`Self::idle`] for why the source list is empty today and what fills
    /// it.
    pub fn run(&mut self) -> ! {
        loop {
            if !self.turn() {
                self.idle();
            }
        }
    }
}

/// An acceptor with the usual sizes and the application inline, over whichever
/// idle strategy the caller names.
///
/// `W` is the mode: [`wait::Spin`] for `hft`, `block::Block` for `standard`.
/// [`HftAcceptorEngine`] and [`StandardAcceptorEngine`] name the two.
pub type TcpAcceptorEngine<A, W> = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<A>,
    crate::clock::SystemClock,
    W,
    crate::journal::Store,
    256,
    4096,
    8192,
>;

/// The `hft` shape: spins, burns a core, and needs a machine that satisfies
/// `DESIGN.md` §9.
pub type HftAcceptorEngine<A> = TcpAcceptorEngine<A, crate::wait::Spin>;

/// The `standard` shape, and **the default**: blocks on readiness and gives the
/// core back.
#[cfg(all(feature = "standard", unix))]
pub type StandardAcceptorEngine<A> = TcpAcceptorEngine<A, crate::block::Block>;

/// Accept FIX connections on `addr` and never return. **`standard` mode.**
///
/// This is what you get if you say nothing, which is
/// [ADR-0013](../../../docs/decisions/ADR-0013-two-modes-standard-and-hft.md)
/// decision 1: it blocks when idle and gives the core back, so it runs on a
/// laptop, in a container, and on a machine somebody else is also using. It is
/// **not** the fastest shape — see [`serve_hft`] for that, and read
/// `docs/GUIDE.md` §0 before choosing it.
///
/// `[2026-08-30]` **This function used to spin.** An engine whose out-of-the-box
/// configuration pins a core at 100% is one most people cannot evaluate: it
/// looks broken. ADR-0013 reversed that default and this is where the reversal
/// lands.
///
/// `[2026-09-01]` **It used to take one `Config` and therefore serve one
/// counterparty.** It takes a [`presession::Table`] now — one entry per
/// counterparty — and a socket is held until its `Logon` says who it is
/// ([ADR-0026], [ADR-0030]).
///
/// # Errors
///
/// [`ServeError::NoCounterparties`] for an empty table, [`ServeError::Io`] from
/// binding.
///
/// [ADR-0026]: ../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md
/// [ADR-0030]: ../../../docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md
#[cfg(all(feature = "standard", unix))]
pub fn serve<A: Application>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
) -> Result<core::convert::Infallible, ServeError> {
    let cfg = default_config(&table)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let engine: StandardAcceptorEngine<A> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        // Sized for the connections, the listener, the waker and the sockets
        // still waiting to identify themselves.
        crate::block::Block::new(capacity + limits.pending() + 2),
        capacity,
    );
    pump(acceptor, engine, table, limits, crate::recovery::NoRecovery)
}

/// As [`serve`], asking `recovery` what each counterparty left behind.
///
/// `[2026-09-02]` **the seam that makes recovery reachable without giving up
/// the serving loop.** [`Engine::add_resumed`] could always continue a session,
/// but `serve` accepts connections itself, so an embedder never saw a transport
/// to call it with — `STATUS.md` item 31.
///
/// [`Recovery::recover`](crate::recovery::Recovery::recover) is called once per
/// connection, **after** the registry has named the counterparty and before the
/// connection reaches the engine, on the acceptor thread. Returning `None`
/// starts that session fresh, which is exactly what [`serve`] does.
///
/// # Errors
///
/// As [`serve`].
pub fn serve_with_recovery<A: Application, V: crate::recovery::Recovery<crate::journal::Store>>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    recovery: V,
) -> Result<core::convert::Infallible, ServeError> {
    let cfg = default_config(&table)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let engine: StandardAcceptorEngine<A> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        crate::block::Block::new(capacity + limits.pending() + 2),
        capacity,
    );
    pump(acceptor, engine, table, limits, recovery)
}

/// As [`serve`], in `hft` mode: **spins, and burns a core for as long as the
/// process lives.**
///
/// On a shared machine, in a container, or on a laptop that is not a bug you
/// will enjoy diagnosing — it is the engine doing exactly what you asked.
/// `DESIGN.md` §9 says what the machine has to look like for it to be worth it.
///
/// # Errors
///
/// [`ServeError::NoCounterparties`] for an empty table, [`ServeError::Io`] from
/// binding.
pub fn serve_hft<A: Application>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
) -> Result<core::convert::Infallible, ServeError> {
    let cfg = default_config(&table)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let engine: HftAcceptorEngine<A> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        crate::wait::Spin,
        capacity,
    );
    pump(acceptor, engine, table, limits, crate::recovery::NoRecovery)
}

/// As [`serve_hft`], asking `recovery` what each counterparty left behind. See
/// [`serve_with_recovery`].
///
/// # Errors
///
/// As [`serve_hft`].
pub fn serve_hft_with_recovery<
    A: Application,
    V: crate::recovery::Recovery<crate::journal::Store>,
>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    recovery: V,
) -> Result<core::convert::Infallible, ServeError> {
    let cfg = default_config(&table)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let engine: HftAcceptorEngine<A> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        crate::wait::Spin,
        capacity,
    );
    pump(acceptor, engine, table, limits, recovery)
}

/// Why a serving loop never started.
///
/// Fielded and `std::error::Error`, because it is returned once at startup and
/// never from a hot path — `CLAUDE.md` §6.
#[derive(Debug)]
#[non_exhaustive]
pub enum ServeError {
    /// The registry serves nobody, so this acceptor would refuse every
    /// connection for as long as the process lived.
    ///
    /// **Refused loudly at startup rather than quietly forever.** An empty
    /// [`presession::Table`] is a valid registry and
    /// [ADR-0026](../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)
    /// decision 6 is what makes it refuse everything — but a *serving loop*
    /// built on one is a configuration mistake, and the same reasoning that
    /// gives [`presession::Limits`] no defaults says so here.
    NoCounterparties,
    /// Binding the listener failed.
    Io(std::io::Error),
}

impl core::fmt::Display for ServeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoCounterparties => write!(
                f,
                "the registry serves no counterparty, so this acceptor would refuse every connection"
            ),
            Self::Io(e) => write!(f, "binding the listener: {e}"),
        }
    }
}

impl std::error::Error for ServeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoCounterparties => None,
            Self::Io(e) => Some(e),
        }
    }
}

/// The `Config` an engine is built with when a registry decides the rest.
///
/// It is the default for [`Engine::add`], which the serving loops never call:
/// every connection they take on arrives from the pre-session stage carrying the
/// configuration the registry chose for its identity. Taking the first entry is
/// therefore a way of having *a* valid one rather than a decision about which —
/// and an empty table has none, which is why that is refused.
fn default_config(table: &presession::Table) -> Result<Config, ServeError> {
    table
        .first()
        .map(presession::Entry::config)
        .ok_or(ServeError::NoCounterparties)
}

/// The loop both `serve` functions run: hold new sockets until they say who
/// they are, hand on the ones a registry serves, turn every connection, and idle
/// when nothing moved.
///
/// One function so the two modes differ in **exactly one type** and in nothing
/// else. A loop written twice is two loops that will drift, and the listener
/// being registered in one and not the other is precisely the kind of drift
/// that costs a whole timeout and shows up as nothing at all.
///
/// `[2026-09-01]` **it no longer hands a raw socket to the engine.** A
/// `PendingSet` owns each one until its first whole message, so the engine is
/// only ever given a connection whose counterparty is known — ADR-0020,
/// ADR-0026.
///
/// **Not behind `standard`, and it was for one commit.** `[measured 2026-09-01]`
/// CI run 33520447994 could not find it from `serve_hft` under
/// `--no-default-features`: `serve_hft` is the `hft` entry point and exists
/// without that feature, so a helper both modes share cannot be gated on one of
/// them. `CLAUDE.md` §10 lists this trap as *a `mod` behind a feature in
/// `Cargo.toml` but not behind `#[cfg]` in `lib.rs`*; this is the same mistake
/// from the other side, and `cargo check --no-default-features` is what catches
/// it.
fn pump<A: Application, W: Waiting, V: crate::recovery::Recovery<crate::journal::Store>>(
    acceptor: Acceptor,
    mut engine: TcpAcceptorEngine<A, W>,
    table: presession::Table,
    limits: presession::Limits,
    mut recovery: V,
) -> Result<core::convert::Infallible, ServeError> {
    // Matches the engine's RX, so a prefix can never be too long for the
    // connection it is handed to.
    const PRE: usize = 4096;

    let mut set: presession::PendingSet<TcpTransport, presession::Table, PRE> =
        presession::PendingSet::new(limits, table);
    let mut clock = crate::clock::SystemClock;
    let listener = acceptor.source().map(Interest::readable);
    let mut extra: Vec<Interest> = Vec::with_capacity(limits.pending() + 1);
    loop {
        let mut moved = false;
        while set.len() < limits.pending() {
            let Some(t) = acceptor.accept() else { break };
            // Dropping the refusal closes the socket, which is what a caller
            // with nowhere to put a connection should do.
            drop(set.admit(t, crate::clock::Clock::now_ms(&mut clock)));
            moved = true;
        }
        let now = crate::clock::Clock::now_ms(&mut clock);
        let p = set.turn(now);
        moved |= p != presession::Progress::default();
        while let Some(i) = set.settled() {
            let Some(pending) = set.take(i) else { break };
            let Some(cfg) = pending.config() else {
                continue;
            };
            // **The one place recovery is asked.** The identity is known now
            // and was not a moment ago — before the `Logon` there is nothing to
            // look a journal up by (ADR-0020, ADR-0026). This is the acceptor
            // thread, which is allowed to block, so an implementation may read
            // a file; it is not the engine thread and this is not a turn.
            let state = recovery.recover(&cfg);
            let (t, buf, len) = pending.into_parts();
            // A prefix that will not fit the engine's RX closes the socket
            // (dropping the transport). It cannot be a message this engine could
            // have read either way, and there is no session yet to tell.
            let _ = engine.add_with_prefix_config_and_state(t, cfg, &buf[..len], state);
            moved = true;
        }
        moved |= engine.turn();
        if !moved {
            extra.clear();
            extra.extend(listener);
            set.interests(&mut extra);
            engine.idle_with(&extra);
        }
    }
}

/// Bytes read before a connection existed did not fit its receive buffer.
///
/// Fielded rather than fieldless: it is not on a hot path, and both numbers are
/// what tells a caller whether to raise `RX` or lower the pre-session buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefixTooLong {
    /// How many bytes the pre-session stage had.
    pub got: usize,
    /// How many the connection can hold — the engine's `RX`.
    pub capacity: usize,
}

impl core::fmt::Display for PrefixTooLong {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} bytes read before the connection existed, but RX is {}",
            self.got, self.capacity
        )
    }
}

impl std::error::Error for PrefixTooLong {}

/// A non-blocking TCP listener that hands out [`TcpTransport`]s.
///
/// Separate from [`Engine`] so an engine can be driven over anything — the
/// acceptance corpus runs it over [`transport::Loopback`], which needs no port
/// and therefore never fails on a busy machine.
pub struct Acceptor {
    listener: TcpListener,
}

impl Acceptor {
    /// Listen on `addr`, without blocking.
    ///
    /// # Errors
    ///
    /// Whatever `bind` or `set_nonblocking` returns.
    pub fn bind(addr: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener })
    }

    /// The address actually bound, which matters when the port was 0.
    ///
    /// # Errors
    ///
    /// Whatever `local_addr` returns.
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }

    /// What a poller waits on to learn that a connection is waiting.
    ///
    /// Hand this to [`Engine::idle_with`]. Without it a `standard` engine still
    /// accepts every connection — on its next timeout, which is up to 100 ms
    /// after the counterparty connected. Nothing fails; the handshake is simply
    /// slow, on a path nobody times.
    #[must_use]
    pub fn source(&self) -> Option<Source> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            Some(Source::from_raw_fd(self.listener.as_raw_fd()))
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    /// Listen on `addr` and **block** in `accept`.
    ///
    /// For a thread that is not an engine thread. `crates/engine/src/shard.rs`
    /// runs its acceptor on one: accepting is not the hot path, it happens once
    /// per session, and a thread that spins to wait for it burns a core to do
    /// nothing. `CLAUDE.md` §2 non-negotiable 4 is about the **engine** thread,
    /// and `scripts/check-no-kernel-sleep.sh` attributes syscalls by tid for
    /// exactly this reason.
    ///
    /// # Errors
    ///
    /// Whatever `bind` returns.
    pub fn bind_blocking(addr: &str) -> std::io::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(addr)?,
        })
    }

    /// Wait for a connection. **Blocks** — see [`bind_blocking`](Self::bind_blocking).
    ///
    /// On an [`Acceptor`] built by [`bind`](Self::bind) this does not block; it
    /// returns `WouldBlock` when nothing is waiting, which is the answer that
    /// constructor promised.
    ///
    /// # Errors
    ///
    /// Whatever `accept` returns, and whatever making the socket non-blocking
    /// returns.
    pub fn accept_blocking(&self) -> std::io::Result<TcpTransport> {
        let (sock, _) = self.listener.accept()?;
        TcpTransport::new(sock)
    }

    /// One connection, if one is waiting. Never blocks.
    #[must_use]
    pub fn accept(&self) -> Option<TcpTransport> {
        match self.listener.accept() {
            Ok((sock, _)) => TcpTransport::new(sock).ok(),
            Err(_) => None,
        }
    }
}

/// Connect out, without blocking afterwards.
///
/// The `connect` itself blocks — briefly, once, before the session exists and
/// before the engine thread is doing anything else. `DESIGN.md` D8 is about the
/// hot path; a socket that is not yet a session is not on it.
///
/// # Errors
///
/// Whatever `connect` or `set_nonblocking` returns.
pub fn connect(addr: &str) -> std::io::Result<TcpTransport> {
    TcpTransport::new(TcpStream::connect(addr)?)
}
