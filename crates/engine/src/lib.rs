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
pub mod msglog;
pub mod observe;
pub mod origin;
pub mod presession;
pub mod reconnect;
pub mod recovery;
// ADR-0110: public so a user's own `MessageLog` and the allocation bench can
// call it. No `cfg`: it pulls in no dependency and no feature.
pub mod redact;
pub mod settings;
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
// Non-negotiable 6: the feature gates the `mod` declaration itself, not only
// the manifest entry. `scripts/check-no-optional-deps.sh` asks per crate,
// because at workspace scope a sibling turns the flag back on.
#[cfg(feature = "tls")]
pub mod tls;
pub mod transport;
pub mod wait;
#[cfg(all(feature = "standard", unix))]
pub mod waker;

use std::net::{TcpListener, TcpStream};

pub use fixbolt_session::{Application, Config, Role, Session};

/// The tag=value [`Encoding`], re-exported because an engine's `E` parameter
/// defaults to `TagValue<Fix44, N>` and a type nobody can name is a type
/// nobody can substitute.
pub use fixbolt_codec::TagValue;
use fixbolt_codec::{Encoding, FieldIndex, MessageView, ParseError, Template};
/// The FIX 4.4 dictionary, for the same reason [`TagValue`] is here.
pub use fixbolt_dict::Fix44;
use fixbolt_dict::Tables;

/// One FIX 4.4 acceptor session: tag=value, `N` fields of index.
///
/// ADR-0079 *Consequences*: *"generic engines mean generic front doors"*. This
/// is the door keeping its old shape — `Session<TagValue<Fix44, N>, Acceptor,
/// APP>` is what `Session<Acceptor, N, APP>` meant before the encoding became a
/// parameter, and a caller that wants another index size still writes the
/// number, exactly as it always did (`CLAUDE.md` §6, the caller picks `N`).
pub type AcceptorFix44<
    const N: usize = 256,
    const APP: usize = { fixbolt_session::DEFAULT_APP_SCRATCH },
> = Session<TagValue<Fix44, N>, fixbolt_session::Acceptor, APP>;

/// One FIX 4.4 initiator session. [`AcceptorFix44`] for the other role.
pub type InitiatorFix44<
    const N: usize = 256,
    const APP: usize = { fixbolt_session::DEFAULT_APP_SCRATCH },
> = Session<TagValue<Fix44, N>, fixbolt_session::Initiator, APP>;

use crate::backpressure::Backpressure;
use crate::clock::Clock;
use crate::conn::{Connection, Turn};
use crate::dispatch::{ConnId, Dispatch, InlineDispatch};
use crate::msglog::{MessageLog, NoLog};
use crate::transport::{Interest, Source, TcpTransport, Transport};
use crate::wait::Waiting;
use fixbolt_session::journal::Journal as SessionJournal;

/// The most messages one session may originate from
/// [`fixbolt_session::Application::on_logon`].
///
/// **A guard against a handler that never answers `None`, not a tuning knob.**
/// The engine asks `on_logon` in a loop and a handler that always answers would
/// otherwise hold the engine thread for good — which non-negotiable 4 forbids
/// in `hft` and which is merely unusable everywhere else.
///
/// **There is no measurement behind the number** and
/// [ADR-0048](../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md)
/// says so: 16 is comfortably above the handful of messages a session opening
/// has ever been observed to need, and far below anything that would matter to
/// a turn. Hitting it emits
/// [`EventKind::SpokeFirstToTheBound`](crate::observe::EventKind::SpokeFirstToTheBound)
/// rather than passing in silence.
pub const MAX_ON_LOGON: u32 = 16;

/// A running engine: the connections it holds, and what drives them.
///
/// Every size is the caller's: `N` the session's field index, `RX` a
/// connection's receive buffer, `TX` its outbound queue, `APP` the scratch an
/// [`Application`] lays its reply out in.
///
/// `[2026-09-05]` **`APP` is here because it was the tightest of the four and
/// the only one with no name** — `docs/reference/a-ceiling-has-more-than-one-floor.md`.
///
/// `[2026-09-19]` **`E` is the encoding** ([`fixbolt_codec::Encoding`],
/// ADR-0079: *"generic engines mean generic front doors"*). It is the last
/// parameter and it has a default, so every `Engine<T, R, D, C, W, J, N, RX,
/// TX, L, APP>` written before phase 2 still names the same type — FIX 4.4
/// tag=value over an `N`-field index. `N` did **not** fold into it: it is what
/// a deployment sizes and what `docs/CONFIGURATION.md` and
/// `serve_with` name, and the impl below binds
/// `E::Scratch` to `FieldIndex<N>` so the two can never mean different
/// numbers.
pub struct Engine<
    T,
    R: Role,
    D,
    C,
    W,
    J: SessionJournal,
    const N: usize,
    const RX: usize,
    const TX: usize,
    L = NoLog,
    const APP: usize = 1024,
    E: Encoding = TagValue<Fix44, N>,
> {
    conns: Vec<Connection<T, R, J, N, RX, TX, APP, E>>,
    /// Every message this engine sees or sends, if anybody asked for them.
    ///
    /// [`NoLog`] by default and it compiles away: `MessageLog::LOGS` is a
    /// constant, so every hook folds out of an engine that was not given one.
    /// [`Self::with_log`] is how a log arrives, and it changes this type
    /// parameter — there is no half-configured engine, because an engine
    /// without a log is a *different type* from one with it.
    log: L,
    /// What [`msglog::MessageLog::lost`] read the last time an event was
    /// emitted for it, so the event carries **this turn's** loss rather than
    /// the running total. Same shape as the journal counters above.
    log_lost_reported: u64,
    /// Whether a connection that did not reach the kernel is refused.
    ///
    /// `[2026-09-10]` **[ADR-0060] decision 1, second half.** `false` by
    /// default and for every non-TLS engine — a `TcpTransport` answers
    /// [`crate::transport::TlsMode::Plain`], which is not a fallback, so this
    /// flag costs a plain engine one never-taken branch per **connection** and
    /// nothing per turn.
    ///
    /// The first half of that decision is a startup probe in
    /// [`serve_tls_with_offload`] and is not here: this is the case a probe
    /// cannot see, where the kernel was capable and the suite was not.
    ///
    /// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
    require_kernel: bool,
    /// Which shard this engine is, for a log line to name.
    ///
    /// Zero unless `shard::serve_sharded_hft` says otherwise. `ConnId` restarts
    /// at zero in every engine, so without this two shards write `conn=0` for
    /// two different sockets.
    shard: u16,
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
    /// Messages applications originated from
    /// [`fixbolt_session::Application::on_logon`].
    ///
    /// **Exists so a benchmark can prove the door opened.** `benches/alloc.rs`
    /// case `logon-first` counts allocations across many sessions logging on,
    /// and a zero there must mean *"did not allocate"* rather than *"did not
    /// run"* — which nothing else on this engine could distinguish, because a
    /// handler with nothing to say and a door that never opens look identical
    /// from outside. See [`Self::speak_first_sends`].
    speak_first_sends: u64,
    /// Connections ended because the dispatch would not take a message.
    ///
    /// **Zero on a healthy engine.** Non-zero means the application behind the
    /// ring fell far enough behind that ADR-0011's policy fired and the session
    /// was dropped. The counterparty was told why — see
    /// [`crate::backpressure::SLOW_APPLICATION`] — so this counter is the same
    /// event seen from the inside. See [`Self::refused_connections`].
    refused_connections: usize,
    /// Sockets the pre-session stage let go because their first message could
    /// not be framed.
    ///
    /// **Told to the engine rather than counted by it.** The stage sits in
    /// front of the engine and owns its own sockets ([ADR-0020]); this is the
    /// only place an operator can read the number, because `Snapshot` is the
    /// only thing an operator reads at all. `serve` hands it over each turn.
    ///
    /// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
    unframeable_prelogon: usize,
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
    /// Sessions that have logged on since this engine was built. See
    /// [`Self::logons`] — it exists because an event stream has exactly one
    /// reader and `dial` used to be it.
    logons: u64,
    /// Set the first turn after an operator asked to stop. `None` while the
    /// engine is simply running, which is the whole cost of being stoppable.
    stopping: Option<Stopping>,
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
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<core::ops::Range<usize>> {
        self.dispatch.deliver(self.conn, msg, hdr, out)
    }
}

/// The `E` bound is [`Session`]'s own, repeated — see the impl on `Session` in
/// `crates/session/src/lib.rs` for what each equality buys. It travels with
/// every type that holds a session, because [`Encoding`]'s four operations are
/// fewer than the session layer needs.
impl<T, R, D, C, W, J, const N: usize, const RX: usize, const TX: usize, L, const APP: usize, E>
    Engine<T, R, D, C, W, J, N, RX, TX, L, APP, E>
where
    T: Transport,
    R: Role,
    D: Dispatch,
    C: Clock,
    W: Waiting,
    J: SessionJournal,
    L: MessageLog,
    E: for<'a> Encoding<
            View<'a> = MessageView<'a, N>,
            Scratch = FieldIndex<N>,
            Template<24, 320> = Template<24, 320>,
            Field = u32,
            ParseError = ParseError,
        >,
    E::Dict: Tables,
{
    /// The same engine, recording every message it sees or sends into `log`.
    ///
    /// **This changes the engine's type**, from `Engine<…, NoLog>` to
    /// `Engine<…, L2>`, which is why there is no window in which an engine
    /// exists with a log half-attached. It is also why `new` does not take one:
    /// every one of this repository's 38 `Engine::new` call sites keeps
    /// compiling, and the ones that never wanted a log never mention it.
    #[must_use]
    pub fn with_log<L2: MessageLog>(
        self,
        log: L2,
    ) -> Engine<T, R, D, C, W, J, N, RX, TX, L2, APP, E> {
        Engine {
            conns: self.conns,
            log,
            log_lost_reported: self.log_lost_reported,
            require_kernel: self.require_kernel,
            shard: self.shard,
            interests: self.interests,
            sources_missing: self.sources_missing,
            unframeable_prelogon: self.unframeable_prelogon,
            speak_first_sends: self.speak_first_sends,
            refused_connections: self.refused_connections,
            cfg: self.cfg,
            dispatch: self.dispatch,
            clock: self.clock,
            wait: self.wait,
            next_id: self.next_id,
            backpressure: self.backpressure,
            observe: self.observe,
            logons: self.logons,
            stopping: self.stopping,
            #[cfg(all(feature = "standard", unix))]
            waker: self.waker,
        }
    }

    /// The same engine, told which shard it is. See [`Self::shard`].
    #[must_use]
    pub const fn with_shard(mut self, shard: u16) -> Self {
        self.shard = shard;
        self
    }

    /// Which shard this engine is. Zero unless it was told otherwise.
    #[must_use]
    pub const fn shard(&self) -> u16 {
        self.shard
    }

    /// The message log this engine was given.
    ///
    /// For an orderly shutdown that wants the file complete before the process
    /// leaves — `FileLog::close` joins the writer — and for a test or a bench
    /// that has to read the file back and therefore has to say when. Dropping
    /// the engine does the same thing; this is how to make it happen earlier.
    pub const fn log_mut(&mut self) -> &mut L {
        &mut self.log
    }

    /// An engine with no connections yet.
    ///
    /// `capacity` is reserved once, here, so that adding a connection later
    /// does not allocate on a thread that must not — non-negotiable 1.
    pub fn new(cfg: Config, dispatch: D, clock: C, wait: W, capacity: usize) -> Self
    where
        L: Default,
    {
        Self {
            conns: Vec::with_capacity(capacity),
            log: L::default(),
            log_lost_reported: 0,
            unframeable_prelogon: 0,
            require_kernel: false,
            shard: 0,
            // Two more than the connections: `serve` adds the listener, and the
            // out-of-band waker is one more. Going over is not fatal — it costs
            // one allocation on a path that must not have any, which is a
            // sizing mistake rather than a steady state.
            interests: Vec::with_capacity(capacity + 2),
            sources_missing: 0,
            speak_first_sends: 0,
            refused_connections: 0,
            cfg,
            dispatch,
            clock,
            wait,
            next_id: 0,
            backpressure: Backpressure::Disconnect,
            observe: None,
            logons: 0,
            stopping: None,
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
            .with_backpressure(self.backpressure)
            .with_shard(self.shard);
        let at = self.clock.now_ms();
        conn.opened(at, &mut self.log);
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
    /// continuation is the caller's; this is where they say. The numbers are
    /// `journal.highest_out() + 1` and `journal.highest_in() + 1`, and
    /// [`recovery::Resumed::from_journal`] computes both — **not `highest()`**, which is
    /// the highest message held for a *replay* and is short by every
    /// administrative message since the last application one
    /// ([ADR-0053](../../../docs/decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md)).
    /// Deciding *whether* to resume is still the caller's.
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
        let mut conn = Connection::new(id, transport, session, journal)
            .with_backpressure(self.backpressure)
            .with_shard(self.shard);
        let at = self.clock.now_ms();
        conn.opened(at, &mut self.log);
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
        self.add_with_prefix_config_and_journal(transport, cfg, prefix, state, J::default)
    }

    /// As [`Self::add_with_prefix_config_and_state`], with the empty journal
    /// **supplied by the caller** instead of taken from [`Default`].
    ///
    /// `[2026-09-02]` this exists because `J: Default` was the single thing
    /// keeping a journal on disk out of the serving loop: a `FileJournal` needs
    /// a path, so it has no honest `Default`, and a dishonest one would have
    /// been an in-memory journal wearing a durable journal's name.
    /// `STATUS.md` item 32 (b).
    ///
    /// `fresh` is called **only** when `state` is [`None`], and only after the
    /// prefix has been checked — so a connection that is about to be refused
    /// does not open a file.
    ///
    /// # Errors
    ///
    /// [`PrefixTooLong`] if the bytes already read will not fit `RX`.
    pub fn add_with_prefix_config_and_journal<F: FnOnce() -> J>(
        &mut self,
        transport: T,
        cfg: Config,
        prefix: &[u8],
        state: Option<crate::recovery::Resumed<J>>,
        fresh: F,
    ) -> Result<ConnId, PrefixTooLong> {
        // **The length check happens here as well as inside, and on purpose.**
        // `fresh` is documented to be called only after the prefix has been
        // checked, so a connection that is about to be refused does not open a
        // file. Building the `Start` below calls it, so the check has to have
        // run by then; the inner one is the one that refuses.
        if prefix.len() > RX {
            return Err(PrefixTooLong {
                got: prefix.len(),
                capacity: RX,
            });
        }
        let start = match state {
            Some(r) => crate::recovery::Start::Resumed(r),
            None => crate::recovery::Start::Fresh(fresh()),
        };
        self.add_with_prefix_config_and_start(transport, cfg, prefix, start)
    }

    /// As [`Self::add_with_prefix_config_and_journal`], with the two answers
    /// already collapsed into one [`crate::recovery::Start`].
    ///
    /// # Why this exists, and why it is the one holding the body
    ///
    /// `[2026-09-20]` because the sharded runtime decides `Fresh` or `Resumed`
    /// on the **acceptor** thread and builds the session on a **shard** thread
    /// (ADR-0088), so what it holds by then is a `Start<J>` and not a
    /// `(state, fresh)` pair. Forwarding a `Start` to
    /// [`Self::add_with_prefix_config_and_journal`] cannot be written: its
    /// `fresh` is `FnOnce() -> J`, the `Resumed` arm has no second journal to
    /// give it, and a closure that is never called still has to typecheck —
    /// every way of writing one is a panic, an `unsafe`, or a `J: Default`
    /// bound, and non-negotiables 7, 8 and ADR-0039 rule out all three. So the
    /// body lives here and `..._journal` builds a `Start` and delegates, which
    /// keeps **one** construction path rather than two.
    ///
    /// `pub(crate)`: [`crate::shard::Shardable::add_started`] is the only caller
    /// besides the delegation above, and ADR-0088 decided no new public door on
    /// [`Engine`].
    ///
    /// # Errors
    ///
    /// [`PrefixTooLong`] if the bytes already read will not fit `RX`.
    pub(crate) fn add_with_prefix_config_and_start(
        &mut self,
        transport: T,
        cfg: Config,
        prefix: &[u8],
        start: crate::recovery::Start<J>,
    ) -> Result<ConnId, PrefixTooLong> {
        if prefix.len() > RX {
            return Err(PrefixTooLong {
                got: prefix.len(),
                capacity: RX,
            });
        }
        let id = self.next_id;
        self.next_id += 1;
        let (session, journal) = match start {
            crate::recovery::Start::Resumed(r) => {
                let s = match r.last_active_ms {
                    Some(at) => Session::resume_at(cfg, r.next_out, r.next_in, at),
                    None => Session::resume(cfg, r.next_out, r.next_in),
                };
                (s, r.journal)
            }
            crate::recovery::Start::Fresh(j) => (Session::new(cfg), j),
        };
        let mut conn = Connection::new(id, transport, session, journal)
            .with_backpressure(self.backpressure)
            .with_shard(self.shard);
        if !conn.prime(prefix) {
            self.next_id -= 1;
            return Err(PrefixTooLong {
                got: prefix.len(),
                capacity: RX,
            });
        }
        let at = self.clock.now_ms();
        conn.opened(at, &mut self.log);
        // **[ADR-0060], and it is here rather than in the turn loop on purpose.**
        // The handover happened in the pre-session stage, so the mode is already
        // decided and cannot change again; asking once per connection keeps it
        // off the hot path entirely, where asking per turn would put a branch
        // there for an answer that never moves. A `TcpTransport` answers `Plain`
        // and this whole block folds to one never-taken comparison.
        if conn.transport.tls_mode() == crate::transport::TlsMode::Userspace {
            if let Some(shared) = self.observe.as_ref() {
                shared.emit(id, at, crate::observe::EventKind::TlsFellBackToUserspace);
            }
            if self.require_kernel {
                // **Ended, not refused, and the difference is the error type.**
                // Returning `Err` here would mean answering a TLS question with
                // `PrefixTooLong` — the exact shape of `STATUS.md` item 54,
                // where three socket endings shared one word. The connection is
                // added and marked dead instead, so the engine's own machinery
                // closes it and the event above is on the stream either way,
                // which ADR-0060 decision 2 requires.
                conn.refuse_without_a_session(fixbolt_session::DropReason::RefusedByDeployment);
            }
        }
        self.conns.push(conn);
        Ok(id)
    }

    /// Take the cell somebody made before this engine existed, so the handles
    /// they already hold watch **this** engine.
    ///
    /// `true` if it was taken. **`false`, changing nothing, if this engine
    /// already has a cell** — two cells on one engine are two truths: the engine
    /// would publish into one and the operator read the other, and every symptom
    /// of that is silence. An engine gets a cell from [`Self::observer`],
    /// [`Self::admin`] or [`Self::sender`], so the refusal is for a caller who
    /// asked the engine for a handle first and then tried to give it one.
    ///
    /// After this, those three methods hand out handles onto the adopted cell —
    /// they find it already there. `STATUS.md` item 47; every front door calls
    /// this for you, which is the whole point.
    /// Refuse any connection whose bytes did not reach the kernel.
    ///
    /// `[2026-09-10]` **[ADR-0060] decision 1**, the per-connection half.
    /// `TlsRequireKernel=Y`. Off by default, which is ADR-0060 decision 2: a
    /// deployment that has not thought about kTLS still runs, **and is still
    /// told** — [`crate::observe::EventKind::TlsFellBackToUserspace`] is raised
    /// either way, because the report does not depend on the strictness.
    ///
    /// It does nothing on an engine whose transport carries no TLS: a
    /// [`crate::transport::TcpTransport`] reports
    /// [`crate::transport::TlsMode::Plain`], and `Plain` is not a fallback.
    ///
    /// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
    pub const fn require_kernel(&mut self, yes: bool) {
        self.require_kernel = yes;
    }

    pub fn adopt(&mut self, handles: &crate::observe::Handles) -> bool {
        if self.observe.is_some() {
            return false;
        }
        self.observe = Some(std::sync::Arc::clone(&handles.0));
        true
    }

    /// How many sessions have logged on since this engine was built.
    ///
    /// **A counter, because the event stream has one reader.**
    /// [`crate::observe::Observer::events`] drains the ring, so two readers
    /// share events rather than each seeing them — and `connect_and_serve`
    /// used to be one of those readers, quietly taking every `LoggedOn` the
    /// caller was waiting for. The reconnect loop compares this number across a
    /// turn instead.
    ///
    /// It costs one increment, in the branch that already tests whether the
    /// session just came up.
    #[must_use]
    pub const fn logons(&self) -> u64 {
        self.logons
    }

    /// Which of [ADR-0005]'s three answers is carrying connection `id`'s bytes,
    /// **as its transport reports it**, or `None` if this engine holds no
    /// connection by that id.
    ///
    /// `[2026-09-13]` step 6a of the `tls` plan. A figure measured over TLS is
    /// about whichever path actually carried it, so a tool that prints the mode
    /// must read it from here rather than echo what it asked for: a connection
    /// told to offload that fell back to userspace is a different code path, and
    /// ADR-0005 open question 3 is exactly that confusion. `tools/w2w` prints
    /// this as its `tls:` line; its reversal — the engine forced to userspace
    /// while `--tls ktls` is asked for — prints `tls: userspace`.
    ///
    /// **Read it after the session is up**, for the reason
    /// [`crate::transport::Transport::tls_mode`] gives: a handshake in flight
    /// reports `Userspace`. Not behind the `tls` feature, because neither is
    /// [`crate::transport::TlsMode`] ([ADR-0060] decision 3): every plain engine
    /// answers `Some(TlsMode::Plain)` for a connection it holds.
    ///
    /// A linear search over this engine's connections — for a caller that asks
    /// once per connection, not per turn.
    ///
    /// [ADR-0005]: ../../../docs/decisions/ADR-0005-tls.md
    /// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
    #[must_use]
    pub fn tls_mode(&self, id: ConnId) -> Option<crate::transport::TlsMode> {
        self.conns
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.transport.tls_mode())
    }

    /// Tell this engine how many sockets the pre-session stage in front of it
    /// let go because their first message could not be framed.
    ///
    /// The stage owns its own sockets and is not part of the engine
    /// ([ADR-0020]), so the number has to be handed over: `Snapshot` is the
    /// only thing an operator reads, and a count nothing publishes is a count
    /// nobody has. `serve` calls this once a turn with what
    /// `presession::Progress` reported; a caller driving an `Engine` by hand
    /// with its own pre-session stage does the same.
    ///
    /// Cumulative — the argument is the number **added** this turn.
    ///
    /// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
    pub const fn note_unframeable(&mut self, n: usize) {
        self.unframeable_prelogon = self.unframeable_prelogon.saturating_add(n);
    }

    /// Tell this engine how many TLS handshakes were refused in front of it —
    /// `presession::Progress::tls_refused` on an acceptor, `1` for a dial whose
    /// handshake was refused on an initiator.
    ///
    /// `[2026-09-23]` [ADR-0151] decision 4. Raises
    /// [`crate::observe::EventKind::TlsHandshakeRefused`] under [`ConnId::MAX`] when
    /// `n > 0` and somebody has called [`Self::observer`]; otherwise it does
    /// nothing, and `n == 0` — every turn of a healthy acceptor — reads no
    /// clock. The event is one `Copy` value pushed into the fixed ring, so
    /// this allocates nothing (non-negotiable 1).
    ///
    /// [ADR-0151]: ../../../docs/decisions/ADR-0151-a-tls-handshake-this-end-refuses-sends-its-alert-and-is-counted-and-a-peer-that-leaves-is-not.md
    pub fn note_tls_refused(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let Some(shared) = self.observe.as_ref() else {
            return;
        };
        let now = crate::clock::Clock::now_ms(&mut self.clock);
        shared.emit(
            ConnId::MAX,
            now,
            crate::observe::EventKind::TlsHandshakeRefused {
                count: u64::try_from(n).unwrap_or(u64::MAX),
            },
        );
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

    /// Application messages this engine has sent from
    /// [`fixbolt_session::Application::on_logon`].
    ///
    /// The running total since the engine was built. See the field's own note:
    /// it exists so `benches/alloc.rs` can tell *"allocated nothing"* from
    /// *"never ran"*.
    #[must_use]
    pub const fn speak_first_sends(&self) -> u64 {
        self.speak_first_sends
    }

    /// A handle that can **change** this engine, as [`Self::observer`] is the
    /// one that can only look.
    ///
    /// Same `Arc`, same mechanism, **a different capability**: hand out
    /// `Observer` to everything that watches and `Admin` only to what
    /// administers. `STATUS.md` item 30 (c).
    ///
    /// One allocation, here, never on a turn — and none at all if this is
    /// called after [`Self::observer`], because they share the cell.
    pub fn admin(&mut self) -> crate::observe::Admin {
        let shared = self
            .observe
            .get_or_insert_with(|| std::sync::Arc::new(crate::observe::Shared::new()));
        crate::observe::Admin(std::sync::Arc::clone(shared))
    }

    /// A handle that can make this engine **say something it was not asked
    /// for**, from any thread. [ADR-0048] door 2.
    ///
    /// Same `Arc` as [`Self::observer`] and [`Self::admin`], same mechanism,
    /// **a third capability**: an `Observer` can only look, an `Admin` can move
    /// sequence numbers, and a [`Sender`](crate::origin::Sender) can originate.
    /// `STATUS.md` item 46.
    ///
    /// One allocation, here, never on a turn — and none at all if this is
    /// called after either of the other two, because all three share the cell.
    ///
    /// [ADR-0048]: ../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md
    pub fn sender(&mut self) -> crate::origin::Sender {
        let shared = self
            .observe
            .get_or_insert_with(|| std::sync::Arc::new(crate::observe::Shared::new()));
        crate::origin::Sender(std::sync::Arc::clone(shared))
    }

    /// Take whatever an operator queued and apply it, on this thread.
    ///
    /// **`try_lock`, never `lock`** — non-negotiable 4. A refused lock takes
    /// nothing and **loses nothing**: unlike an event, a command that vanished
    /// is an action that silently did not happen, so the queue is left exactly
    /// as it was and the next turn tries again.
    ///
    /// No allocation: the landing area is a fixed array on the stack, and it is
    /// only touched on a turn where something was actually queued.
    /// `benches/alloc.rs` cases `admin-idle` and `admin-busy` are what prove
    /// it.
    fn administer(&mut self) {
        let Some(shared) = self.observe.as_ref() else {
            return;
        };
        // One relaxed load, and it is the entire cost of being administrable
        // while nobody is administering — the same bargain `wanted` makes for
        // snapshots. Without it every turn on an observed engine would attempt
        // a mutex, which is a worse deal than ADR-0032 claims for this
        // mechanism.
        if !shared.commands.waiting() {
            return;
        }
        let mut taken: [Option<crate::observe::Command>; crate::observe::COMMAND_CAPACITY] =
            [None; crate::observe::COMMAND_CAPACITY];
        let n = shared.commands.drain(&mut taken);
        if n == 0 {
            return;
        }
        let now = self.clock.now_ms();
        let log = &mut self.log;
        for c in taken.iter().take(n).flatten() {
            let outcome = match self.conns.iter_mut().find(|x| x.id == c.id()) {
                Some(conn) => conn.administer(*c, now, log),
                None => crate::observe::Outcome::NoSuchConnection,
            };
            // Re-borrowed rather than held across the loop: `administer` takes
            // `&mut self.conns` and the emit takes `&self.observe`.
            if let Some(shared) = self.observe.as_ref() {
                shared.emit(
                    c.id(),
                    now,
                    crate::observe::EventKind::Administered {
                        change: c.change(),
                        to: c.to(),
                        outcome,
                    },
                );
            }
        }
    }

    /// Send whatever a [`Sender`](crate::origin::Sender) queued from another
    /// thread. [ADR-0048] door 2.
    ///
    /// **`try_lock`, never `lock`** — non-negotiable 4 — and one relaxed load
    /// before the lock is even attempted, so an engine nobody sends through
    /// pays a load per turn rather than a mutex.
    /// `crates/engine/tests/originate.rs::an_engine_nobody_sends_through_does_not_reach_for_the_lock`
    /// is what keeps that falsifiable.
    ///
    /// A message for a connection that has gone is **dropped**, deliberately:
    /// the session that owned its sequence numbers went with it, and sending it
    /// anywhere else would be worse. The drop is counted and emitted rather
    /// than passed over in silence.
    ///
    /// [ADR-0048]: ../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md
    fn originate(&mut self, now: u64) -> bool {
        let Self {
            conns,
            log,
            observe,
            ..
        } = self;
        let Some(shared) = observe.as_ref() else {
            return false;
        };
        if !shared.origin.waiting() {
            return false;
        }
        let mut sent = 0usize;
        let mut gone = 0u64;
        shared
            .origin
            .drain(|id, msg| match conns.iter_mut().find(|c| c.id == id) {
                Some(c) => {
                    c.send_application(msg, now, log);
                    sent += 1;
                    true
                }
                None => {
                    gone += 1;
                    false
                }
            });
        if gone > 0 {
            shared.emit(
                ConnId::MAX,
                now,
                crate::observe::EventKind::OriginationUndeliverable { count: gone },
            );
        }
        sent > 0
    }

    /// Say goodbye to everyone, once, on the first turn after somebody asked.
    ///
    /// **One relaxed load while nobody has asked** — the same bargain
    /// `wanted` and the command queue make, and
    /// `crates/engine/tests/shutdown.rs::an_engine_nobody_stopped_pays_one_load`
    /// is what keeps it falsifiable rather than asserted.
    /// The grace an `Admin::shutdown` asked for, if one was asked. What the
    /// serve functions give [`journal::wait_for_retired_writers`] after their
    /// loop, ADR-0153 decision 4.
    fn stop_grace_ms(&self) -> Option<u64> {
        self.observe.as_ref().and_then(|s| s.stop_asked())
    }

    fn begin_shutdown_if_asked(&mut self, now: u64) {
        if self.stopping.is_some() {
            return;
        }
        let Some(shared) = self.observe.as_ref() else {
            return;
        };
        let Some(grace_ms) = shared.stop_asked() else {
            return;
        };
        let mut st = Stopping {
            deadline_ms: now.saturating_add(grace_ms),
            sessions: 0,
            said_goodbye: 0,
            acked: 0,
            timed_out: 0,
        };
        let log = &mut self.log;
        for c in &mut self.conns {
            st.sessions += 1;
            // **The moment that matters.** A planned restart wants to know when
            // the session was last alive, and this is that instant. Recorded
            // before the goodbye, because the goodbye may not go out.
            c.journal.mark_active(now);
            // `begin_logout` returns `Up` only when the goodbye actually went
            // into the buffer. A session that never logged on, or that cannot
            // build the message, answers `Dropped` and is simply closed —
            // waiting for an answer to a message that was not sent is how a
            // shutdown hangs.
            if c.begin_logout(now, log) {
                st.said_goodbye += 1;
            }
        }
        self.stopping = Some(st);
    }

    /// Has the shutdown finished, and what did it manage?
    ///
    /// [`Some`] once every connection has gone, or once the deadline has
    /// passed — at which point whatever is left is closed and **counted as
    /// having timed out**, because *"we stopped"* and *"we stopped while two
    /// counterparties never answered"* are different facts and an operator must
    /// be able to tell them apart before restarting.
    pub fn shutdown_finished(&mut self) -> Option<Shutdown> {
        let st = self.stopping.as_mut()?;
        let now = self.clock.now_ms();
        if !self.conns.is_empty() && now < st.deadline_ms {
            return None;
        }
        st.timed_out = self.conns.len();
        // **Each one still says why it ended.** Clearing the vector would take
        // them away without a word, and an operator reading the event stream
        // would see connections vanish with no cause — exactly the hole
        // ADR-0035 exists to close.
        for c in &mut self.conns {
            c.session
                .note_drop_reason(fixbolt_session::DropReason::EngineShutdown);
        }
        if let Some(shared) = self.observe.as_ref() {
            for c in &self.conns {
                shared.emit(
                    c.id,
                    now,
                    crate::observe::EventKind::Ended(fixbolt_session::DropReason::EngineShutdown),
                );
            }
        }
        self.conns.clear();
        let done = Shutdown {
            sessions: st.sessions,
            said_goodbye: st.said_goodbye,
            acked: st.acked,
            timed_out: st.timed_out,
        };
        Some(done)
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
        conns: &[Connection<T, R, J, N, RX, TX, APP, E>],
        refused_connections: usize,
        sources_missing: usize,
        log_lost: u64,
        unframeable_prelogon: usize,
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
                crate::observe::JournalHealth {
                    refused: c.session.puts_refused(),
                    beyond: c.session.resend_beyond_journal(),
                },
            ));
        }
        snap.set_counters(
            conns.len(),
            refused_connections,
            sources_missing,
            log_lost,
            unframeable_prelogon,
        );
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
    // `[measured 2026-09-08]` 18 indexing/slicing sites. Scoped to this
    // function and NOT to the file: a crate-root `#![allow]` silences the
    // whole crate, submodules included. STATUS.md item 55.
    #[allow(clippy::indexing_slicing)]
    /// One non-blocking pass over every connection. `true` if anything moved.
    ///
    /// Nothing in here can block: every transport call is non-blocking and
    /// every "nothing yet" is [`transport::Io::Idle`].
    pub fn turn(&mut self) -> bool {
        // **One reading, and both halves of it.** `now.ms` is what everything
        // in this function already used; `now.sub_ms_nanos` goes to exactly one
        // place, the `52=` formatter inside the session, and is fetched here so
        // it cannot belong to a different millisecond than its own seconds
        // (ADR-0057 decision 1).
        let reading = self.clock.now();
        let now = reading.ms;
        // Being observable costs this, and only this, while nobody is
        // observing: one relaxed load through an `Option` that is `None` on an
        // engine whose `observer()` was never called. See `observe`.
        if let Some(shared) = self.observe.as_ref()
            && shared.wanted()
        {
            let snap = Self::snapshot(
                &self.conns,
                self.refused_connections,
                self.sources_missing,
                self.log.lost(),
                self.unframeable_prelogon,
            );
            shared.publish(&snap);
        }
        // **Before anything is judged or numbered.** A command applied after
        // the messages of this turn would be setting a number that has already
        // gone out — `crates/engine/tests/admin.rs` holds the order.
        self.administer();
        self.begin_shutdown_if_asked(now);
        let mut moved = false;
        // **Beside the commands, and before a byte is read.** A message an
        // application queued has been waiting since the previous turn, so it
        // goes out ahead of any reply this turn is about to produce
        // (ADR-0048 decision 3's ordering; `tests/originate.rs` holds it).
        moved |= self.originate(now);
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
            // Before the turn, so a session that logs on during it is seen as
            // a change rather than as the state it started in.
            let was_on = self.conns[i].session.is_logged_on();
            // The two journal counters, read before the turn so the event
            // carries what **this turn** did rather than the running total.
            // Only when somebody is observing: on an engine whose `observer()`
            // was never called this is two reads that never happen.
            let (was_refused, was_beyond) = if self.observe.is_some() {
                (
                    self.conns[i].session.puts_refused(),
                    self.conns[i].session.resend_beyond_journal(),
                )
            } else {
                (0, 0)
            };
            let shard = self.shard;
            let Self { conns, log, .. } = self;
            let outcome = conns[i].turn(
                reading,
                &mut deliver,
                |msg| others_on > 0 && presession::is_logon(msg),
                shard,
                log,
            );
            // **When the session came up, on disk.** Not per message: that is
            // the hot path and D8 forbids a write there. A durable journal
            // records it; everything else ignores it, because the trait's
            // default body is empty. `STATUS.md` item 32 (c).
            if !was_on && self.conns[i].session.is_logged_on() {
                self.conns[i].journal.mark_active(now);
                // **The same instant the `LoggedOn` event is emitted from**, and
                // deliberately outside the `observe` test below: `dial` needs it
                // whether or not anybody is watching. See [`Self::logons`].
                self.logons = self.logons.saturating_add(1);
                // **The session may now speak first** (ADR-0048 door 1). Here
                // and nowhere else: this is the one instant that is neither a
                // reply nor a tick, and it is reached once per session rather
                // than once per turn.
                moved |= self.speak_first(i, now);
            }
            // Events cost one `Option` test per connection per turn while
            // nobody is observing, and nothing at all on an engine whose
            // `observer()` was never called.
            if let Some(shared) = self.observe.as_ref() {
                let id = self.conns[i].id;
                if !was_on && self.conns[i].session.is_logged_on() {
                    shared.emit(id, now, crate::observe::EventKind::LoggedOn);
                }
                // **One event per turn that changed, not one per message.**
                // ADR-0035: the observer is asked once per turn, and a
                // `try_lock` per message would put a lock on the hot path.
                let refused_now = self.conns[i].session.puts_refused();
                if refused_now != was_refused {
                    shared.emit(
                        id,
                        now,
                        crate::observe::EventKind::JournalRefused {
                            count: refused_now.saturating_sub(was_refused),
                        },
                    );
                }
                let beyond_now = self.conns[i].session.resend_beyond_journal();
                if beyond_now != was_beyond {
                    shared.emit(
                        id,
                        now,
                        crate::observe::EventKind::ResendBeyondJournal {
                            filled: beyond_now.saturating_sub(was_beyond),
                            oldest: self.conns[i].journal.oldest(),
                        },
                    );
                }
                if matches!(outcome, Turn::Gone) {
                    // `EndedWithoutReason` is not decoration: it is how a new
                    // way of ending a link that skipped `Session::end` becomes
                    // visible instead of silently reading as a normal close.
                    let kind = self.conns[i].session.last_drop_reason().map_or(
                        crate::observe::EventKind::EndedWithoutReason,
                        crate::observe::EventKind::Ended,
                    );
                    shared.emit(id, now, kind);
                }
            }
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
                        let now_for_log = now;
                        let Self { conns, log, .. } = self;
                        conns[i].slow_application(now_for_log, log);
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
                    // **Before the connection is dropped, because the count
                    // goes with it.** The message log already wrote `OUT` for
                    // these bytes; this is the engine saying how much of that
                    // connection's tail is wrong. Behind `L::LOGS`, so an
                    // engine with no log does not pay a comparison for a number
                    // that describes a file nobody is writing.
                    if L::LOGS {
                        let who = self.conns[i].id;
                        let unsent = self.conns[i].unsent_bytes();
                        if unsent > 0
                            && let Some(shared) = self.observe.as_ref()
                        {
                            shared.emit(
                                who,
                                now,
                                crate::observe::EventKind::MessageLogUnsent { bytes: unsent },
                            );
                        }
                    }
                    // **Counted here rather than at the deadline**: by then the
                    // connection is gone and the reason with it. A shutdown
                    // that reports "all clear" without checking *why* each
                    // session left would count a heartbeat timeout as an
                    // orderly goodbye.
                    if let Some(st) = self.stopping.as_mut()
                        && self.conns[i].session.last_drop_reason()
                            == Some(fixbolt_session::DropReason::PeerLogout)
                    {
                        st.acked += 1;
                    }
                    self.conns.swap_remove(i);
                    moved = true;
                }
            }
        }

        // **Once per turn, not per connection.** A record the ring refused is
        // gone before anything knows which session it belonged to, so there is
        // no honest id to attribute it to — `ConnId::MAX` says *the engine*,
        // and the whole point of the event is that the file has a hole in it.
        //
        // `L::LOGS` is a constant, so an engine with `NoLog` compiles this
        // block away rather than paying a comparison for a counter that can
        // never move.
        if L::LOGS {
            let lost = self.log.lost();
            if lost != self.log_lost_reported
                && let Some(shared) = self.observe.as_ref()
            {
                shared.emit(
                    ConnId::MAX,
                    now,
                    crate::observe::EventKind::MessageLogLost {
                        count: lost.saturating_sub(self.log_lost_reported),
                    },
                );
                self.log_lost_reported = lost;
            }
        }

        // Anything the application produced on another thread. The constant is
        // `false` for `InlineDispatch`, so this whole block compiles away
        // rather than costing a branch on the commonest engine there is.
        if D::OUT_OF_BAND {
            // One clock read for the whole collected batch: every reply this
            // block queues is part of the same turn, and shares its instant.
            let at_for_log = now;
            let Self { conns, log, .. } = self;
            let mut any = false;
            self.dispatch.collect(|id, msg| {
                any = true;
                if let Some(c) = conns.iter_mut().find(|c| c.id == id) {
                    c.send_application(msg, at_for_log, log);
                }
                // A reply for a connection that has gone is dropped, on
                // purpose: the session that owned its sequence numbers is gone
                // with it, and sending it anywhere else would be worse.
            });
            moved |= any;
        }
        moved
    }
    // `[measured 2026-09-08]` 3 indexing/slicing sites. Scoped to this
    // function and NOT to the file: a crate-root `#![allow]` silences the
    // whole crate, submodules included. STATUS.md item 55.
    #[allow(clippy::indexing_slicing)]
    /// Ask the application whether it has anything to say, now that this
    /// session is up. [ADR-0048] door 1.
    ///
    /// `nth = 0, 1, 2, …` until the application answers `None`, each message
    /// sent as it comes so nothing is accumulated and one buffer serves them
    /// all. The buffer is a local rather than a field: this runs **once per
    /// session**, never per turn, so its cost belongs to the connection that
    /// caused it and not to every turn of an engine whose applications are
    /// quiet.
    ///
    /// The three identity strings are copied out before the loop because the
    /// application is reached through `&mut self` and the configuration through
    /// `&self`. They are 32 bytes each at most.
    ///
    /// [ADR-0048]: ../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md
    fn speak_first(&mut self, i: usize, now: u64) -> bool {
        let id = self.conns[i].id;
        let mut begin = [0u8; fixbolt_session::MAX_BEGIN_STRING_LEN];
        let mut sender = [0u8; fixbolt_session::MAX_COMP_ID_LEN];
        let mut target = [0u8; fixbolt_session::MAX_COMP_ID_LEN];
        let (b_len, s_len, t_len) = {
            let cfg = self.conns[i].session.config();
            let (b, s, t) = (
                cfg.begin_string(),
                cfg.sender_comp_id(),
                cfg.target_comp_id(),
            );
            if let Some(d) = begin.get_mut(..b.len()) {
                d.copy_from_slice(b);
            }
            if let Some(d) = sender.get_mut(..s.len()) {
                d.copy_from_slice(s);
            }
            if let Some(d) = target.get_mut(..t.len()) {
                d.copy_from_slice(t);
            }
            (b.len(), s.len(), t.len())
        };
        let peer = fixbolt_session::Peer {
            begin_string: begin.get(..b_len).unwrap_or(b""),
            sender: sender.get(..s_len).unwrap_or(b""),
            target: target.get(..t_len).unwrap_or(b""),
        };
        let mut buf = [0u8; APP];
        let mut sent = 0u32;
        for nth in 0..MAX_ON_LOGON {
            let Self {
                conns,
                log,
                dispatch,
                ..
            } = self;
            let Some(r) = dispatch.on_logon(id, nth, peer, &mut buf) else {
                return sent > 0;
            };
            let Some(msg) = buf.get(r) else {
                // A range outside the buffer it was handed. The application is
                // broken, and the honest answer is to stop asking it rather
                // than to send something else.
                return sent > 0;
            };
            conns[i].send_application(msg, now, log);
            sent += 1;
            self.speak_first_sends += 1;
        }
        // Reached the bound with the application still willing. Never silent:
        // a session that opens a few messages short and says nothing is the
        // failure this event exists to make visible.
        if let Some(shared) = self.observe.as_ref() {
            shared.emit(
                id,
                now,
                crate::observe::EventKind::SpokeFirstToTheBound { sent },
            );
        }
        sent > 0
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
        conns: &[Connection<T, R, J, N, RX, TX, APP, E>],
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

    /// Turn until somebody stops it, idling by the chosen [`Waiting`] strategy.
    ///
    /// `[2026-09-02]` this used to return `!`. Nothing could stop this engine
    /// except killing the process, which tells the counterparty nothing, loses
    /// bytes that had already spent their sequence numbers, and can leave the
    /// journal with a torn tail. `Admin::shutdown` is how it is asked;
    /// [`Shutdown`] says what it managed.
    ///
    /// The default strategy is [`wait::Spin`], which is D8's `hft` half: no
    /// `epoll_wait`, no futex, no blocking read.
    ///
    /// See [`Self::idle`] for why the source list is empty today and what fills
    /// it.
    pub fn run(&mut self) -> Shutdown {
        loop {
            if !self.turn() {
                self.idle();
            }
            if let Some(done) = self.shutdown_finished() {
                return done;
            }
        }
    }
}

/// An acceptor with the usual sizes and the application inline, over whichever
/// idle strategy the caller names.
///
/// `W` is the mode: [`wait::Spin`] for `hft`, `block::Block` for `standard`.
/// [`HftAcceptorEngine`] and `StandardAcceptorEngine` name the two.
///
/// `[2026-09-05]` **`N`, `RX` and `TX` are parameters with defaults, not
/// literals.** They read the same at every existing call site — a default is
/// applied where nothing is written — but they can now be named, which is what
/// `crate::serve_with` exists to pass on. Before this they were spelled out
/// here as `256, 4096, 8192`, and an alias is exactly as much of a hidden
/// constant as a `const` when nothing above it can say otherwise
/// (`CLAUDE.md` §6).
pub type TcpAcceptorEngine<
    A,
    W,
    J = crate::journal::Store,
    L = NoLog,
    const N: usize = 256,
    const RX: usize = 4096,
    const TX: usize = 8192,
    const APP: usize = 1024,
    E = TagValue<Fix44, N>,
> = AcceptorEngineOver<TcpTransport, A, W, J, L, N, RX, TX, APP, E>;

/// The same acceptor shape, over **any** transport.
///
/// `[2026-09-10]` **added by step 4a of the `tls` plan**, and it is the only
/// thing that step needed from this file besides one line in `pump`:
/// [`Engine`] was already generic over its transport (`conns: Vec<Connection<T,
/// …>>`), so reaching TLS is a type substitution rather than a redesign. What
/// was missing was a name for the substitution.
///
/// [`TcpAcceptorEngine`] is this with `T = TcpTransport` and is what almost
/// everything wants; `T = tls::TlsTransport` is what `serve_tls` builds.
///
/// **The two names above are code spans and not intra-doc links on purpose.**
/// `pump` is private, and `serve_tls` does not exist unless `--features tls` is
/// on — while this alias does. `[measured 2026-09-10]` linking them failed the
/// `rustdoc` job on a build where the target of the link was not compiled, which
/// is the feature-gate trap of non-negotiable 6 arriving through documentation.
pub type AcceptorEngineOver<
    T,
    A,
    W,
    J = crate::journal::Store,
    L = NoLog,
    const N: usize = 256,
    const RX: usize = 4096,
    const TX: usize = 8192,
    const APP: usize = 1024,
    E = TagValue<Fix44, N>,
> = Engine<
    T,
    fixbolt_session::Acceptor,
    InlineDispatch<A>,
    crate::clock::SystemClock,
    W,
    J,
    N,
    RX,
    TX,
    L,
    APP,
    E,
>;

/// The `hft` shape: spins, burns a core, and needs a machine that satisfies
/// `DESIGN.md` §9.
pub type HftAcceptorEngine<
    A,
    L = NoLog,
    const N: usize = 256,
    const RX: usize = 4096,
    const TX: usize = 8192,
    const APP: usize = 1024,
    E = TagValue<Fix44, N>,
> = TcpAcceptorEngine<A, crate::wait::Spin, crate::journal::Store, L, N, RX, TX, APP, E>;

/// The same shape, dialling out. `STATUS.md` item 35.
pub type TcpInitiatorEngine<
    A,
    W,
    J = crate::journal::Store,
    L = NoLog,
    const N: usize = 256,
    const RX: usize = 4096,
    const TX: usize = 8192,
    const APP: usize = 1024,
    E = TagValue<Fix44, N>,
> = InitiatorEngineOver<TcpTransport, A, W, J, L, N, RX, TX, APP, E>;

/// The initiator shape, over **any** transport — [`AcceptorEngineOver`]'s
/// counterpart for the end that dials.
///
/// `[2026-09-13]` **added by step 5b of the `tls` plan**, for the same reason
/// the acceptor alias was added by 4a: [`Engine`] was already generic over its
/// transport, and what was missing was a name for the substitution.
/// [`TcpInitiatorEngine`] is this with `T = TcpTransport`; `connect_and_serve_tls`
/// builds it with `T = tls::TlsTransport<tls::Client>` — code spans rather than
/// links, for the reason [`AcceptorEngineOver`] gives.
pub type InitiatorEngineOver<
    T,
    A,
    W,
    J = crate::journal::Store,
    L = NoLog,
    const N: usize = 256,
    const RX: usize = 4096,
    const TX: usize = 8192,
    const APP: usize = 1024,
    E = TagValue<Fix44, N>,
> = Engine<
    T,
    fixbolt_session::Initiator,
    InlineDispatch<A>,
    crate::clock::SystemClock,
    W,
    J,
    N,
    RX,
    TX,
    L,
    APP,
    E,
>;

/// The `standard` shape, and **the default**: blocks on readiness and gives the
/// core back.
#[cfg(all(feature = "standard", unix))]
pub type StandardAcceptorEngine<
    A,
    L = NoLog,
    const N: usize = 256,
    const RX: usize = 4096,
    const TX: usize = 8192,
    const APP: usize = 1024,
    E = TagValue<Fix44, N>,
> = TcpAcceptorEngine<A, crate::block::Block, crate::journal::Store, L, N, RX, TX, APP, E>;

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
pub fn serve<A: Application, L: MessageLog>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    serve_with::<256, 4096, 8192, 1024, A, L>(addr, table, app, capacity, limits, log, handles)
}

/// The same, with the three buffer sizes named by the caller.
///
/// `N` is the field index, `RX` the receive buffer — **the largest message this
/// acceptor can frame** — and `TX` the write queue. [`serve`] is this function
/// with the defaults `256, 4096, 8192`, and is what to call unless a
/// counterparty needs something else.
///
/// ```ignore
/// // a counterparty that sends messages up to 16 KiB
/// serve_with::<256, 16_384, 8_192, _, _>(addr, table, app, 64, limits, NoLog)?;
/// ```
///
/// The two `_` are the language's, not this API's: a turbofish must supply
/// every generic argument, and `A` and `L` are inferred from the arguments.
/// Const parameters cannot carry defaults on a function, which is why this is a
/// second function rather than two more parameters on the first.
///
/// **`RX` also sizes the pre-session buffer**, so a counterparty may pipeline
/// behind its `Logon` up to the same bound. See `docs/CONFIGURATION.md` for what
/// each costs: `[measured 2026-09-04]` one connection is 23 752 bytes at
/// `RX = 4096` and 36 040 at 16 384, against ~2 MiB of journal per session — so
/// four times the buffer is 0.57% more memory per connection.
///
/// # Errors
///
/// As [`serve`].
#[cfg(all(feature = "standard", unix))]
pub fn serve_with<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    L: MessageLog,
>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    let cfg = default_config(&table)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let mut engine: StandardAcceptorEngine<A, NoLog, N, RX, TX, APP> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        // Sized for the connections, the listener, the waker and the sockets
        // still waiting to identify themselves.
        crate::block::Block::new(capacity + limits.pending() + 2),
        capacity,
    );
    // A brand-new engine has no cell of its own, so this cannot refuse.
    // `with_log` carries the cell across to the engine it returns.
    let _ = engine.adopt(&handles);
    pump(
        acceptor,
        Some,
        engine.with_log(log),
        table,
        limits,
        crate::recovery::NoRecovery,
    )
}

/// Accept FIX connections on `addr` **over TLS** and never return.
/// **`standard` mode, Linux.**
///
/// `[2026-09-10]` **step 4a of the `tls` plan, and the step that makes TLS
/// reachable at all.** Steps 1-3 built and tested [`tls::TlsTransport`]; until
/// this function there was no way to put one inside an engine short of
/// assembling the engine by hand, so the feature existed and no deployment
/// could use it.
///
/// # Why it takes a certificate and a key rather than a `rustls::ServerConfig`
///
/// Because `enable_secret_extraction` is load-bearing and a caller cannot be
/// expected to know it. `dangerous_into_kernel_connection` — the call that hands
/// the keys to the kernel — refuses without it, and **the connection is then
/// dropped rather than served from userspace**: that call consumes the rustls
/// connection, so by the time the refusal is known there is nothing left to fall
/// back to. `[measured 2026-09-10]` a `ServerConfig` built the obvious way
/// compiles, handshakes, and hands the counterparty a `ConnectionReset` with no
/// FIX-level explanation. The suite narrowing is the same kind of thing — kTLS
/// carries fewer suites than `rustls` will negotiate.
///
/// Both are constraints the type system cannot hold, and `CLAUDE.md` §4 sends
/// those to `GUIDE.md`. This one does not go there: it is held by not offering
/// the caller the choice.
///
/// # What this does not decide
///
/// **Whether the kernel actually took the keys.** A session that fell back to
/// userspace serves correctly and looks identical from outside. Reading the
/// mode, raising [`observe::EventKind`] for a fallback, and refusing one with
/// `TlsRequireKernel` are step 4b and are **not here**.
///
/// # Errors
///
/// As [`serve`], plus [`ServeError::Tls`] if the certificate and key do not
/// make a server configuration.
#[cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
// **Nine, and clippy's ceiling is seven.** Same answer as the four
// `*_with_recovery` entry points above: ADR-0054 recorded a `Serve` builder as
// the alternative and named its reopening condition — *the first time an
// eleventh parameter is wanted*. A certificate and a key are two things and
// bundling them into one struct to buy a number would be arranging the code for
// the lint. Nine is under the condition, so the condition is not tripped and
// this is noted rather than silently muted.
#[allow(clippy::too_many_arguments)]
pub fn serve_tls<A: Application, L: MessageLog>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    log: L,
    handles: crate::observe::Handles,
    certs: Vec<rustls::pki_types::CertificateDer<'static>>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Result<Shutdown, ServeError> {
    serve_tls_with::<256, 4096, 8192, 1024, A, L>(
        addr, table, app, capacity, limits, log, handles, certs, key,
    )
}

/// As [`serve_tls`], with `TlsRequireKernel` set by the caller.
///
/// `[2026-09-10]` [ADR-0060]. `require_kernel` is `TlsRequireKernel=Y`: this
/// kernel is probed once before binding, and any later handshake that still
/// lands in userspace ends that connection. **Either way the fallback is
/// reported** — decision 2, and the reason [`serve_tls`] is not the reckless
/// choice.
///
/// # Errors
///
/// As [`serve_tls_with_offload`].
///
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
#[allow(clippy::too_many_arguments)]
#[cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
pub fn serve_tls_requiring<A: Application, L: MessageLog>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    log: L,
    handles: crate::observe::Handles,
    certs: Vec<rustls::pki_types::CertificateDer<'static>>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
    require_kernel: bool,
) -> Result<Shutdown, ServeError> {
    serve_tls_with_offload::<256, 4096, 8192, 1024, A, L>(
        addr,
        table,
        app,
        capacity,
        limits,
        log,
        handles,
        certs,
        key,
        require_kernel,
        crate::tls::TlsProbe::Real,
    )
}

/// The full door: buffer sizes, `TlsRequireKernel`, **and the offload seam**.
///
/// `offload` is not a deployment knob and a deployment passes `true`. It exists
/// for the reason [`tls::TlsTransport::with_offload`] does: the userspace
/// fallback is reached only when a kernel refuses, this desk's kernel does not
/// refuse, and a path that cannot be reached from a test is a path that has
/// never run — `CLAUDE.md` §10's promise rather than evidence.
///
/// **`false` makes both halves of [ADR-0060] decision 1 reachable**: the startup
/// probe is treated as refusing, so `require_kernel` refuses to bind; and any
/// handshake that completes lands in userspace, so the per-connection half fires
/// too. `[measured 2026-09-10]` `crates/engine/tests/tls_mode.rs` drives every
/// arm through it.
///
/// # Errors
///
/// As [`serve_tls`], plus [`ServeError::Tls`] when `require_kernel` is set and
/// this kernel cannot offload TLS.
///
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
// **Eleven**, and that is the count [ADR-0054] named as the reopening condition
// for a `Serve` builder — *the first time an eleventh parameter is wanted*. It
// is reached here. Said out loud in that ADR's own terms rather than passed
// over; `STATUS.md` carries the decision as due rather than made.
#[allow(clippy::too_many_arguments)]
#[cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
pub fn serve_tls_with_offload<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    L: MessageLog,
>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    log: L,
    handles: crate::observe::Handles,
    certs: Vec<rustls::pki_types::CertificateDer<'static>>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
    require_kernel: bool,
    probe: crate::tls::TlsProbe,
) -> Result<Shutdown, ServeError> {
    // **ADR-0060 decision 1, first half, and it happens before `bind`.** An
    // operator whose host was built without CONFIG_TLS finds out here, from a
    // sentence about the kernel, rather than later from a session dropped for
    // reasons the counterparty had nothing to do with.
    let kernel_offloads = match probe {
        crate::tls::TlsProbe::PretendKernelCannotOffload => false,
        crate::tls::TlsProbe::Real | crate::tls::TlsProbe::PretendHandshakeFallsBack => {
            crate::tls::kernel_can_offload()
        }
    };
    if require_kernel && !kernel_offloads {
        return Err(ServeError::Tls(
            "TlsRequireKernel=Y, and this kernel cannot offload TLS: it has no \
             tls ULP, so every session would fall back to userspace rustls and \
             leave the hot-path guarantee"
                .to_owned(),
        ));
    }
    let cfg = default_config(&table)?;
    let tls = crate::tls::server_config(certs, key)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let mut engine: AcceptorEngineOver<
        crate::tls::TlsTransport,
        A,
        crate::block::Block,
        crate::journal::Store,
        NoLog,
        N,
        RX,
        TX,
        APP,
    > = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        crate::block::Block::new(capacity + limits.pending() + 2),
        capacity,
    );
    engine.require_kernel(require_kernel);
    let _ = engine.adopt(&handles);
    pump(
        acceptor,
        move |sock| {
            rustls::server::UnbufferedServerConnection::new(std::sync::Arc::clone(&tls))
                .ok()
                .map(|conn| {
                    crate::tls::TlsTransport::with_offload(
                        sock,
                        crate::tls::Handshake::new(conn),
                        matches!(probe, crate::tls::TlsProbe::Real),
                    )
                })
        },
        engine.with_log(log),
        table,
        limits,
        crate::recovery::NoRecovery,
    )
}

/// The same, with the four buffer sizes named by the caller.
///
/// See [`serve_with`] for what each one costs.
///
/// # Errors
///
/// As [`serve_tls`].
#[cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
// **Nine, and clippy's ceiling is seven.** Same answer as the four
// `*_with_recovery` entry points above: ADR-0054 recorded a `Serve` builder as
// the alternative and named its reopening condition — *the first time an
// eleventh parameter is wanted*. A certificate and a key are two things and
// bundling them into one struct to buy a number would be arranging the code for
// the lint. Nine is under the condition, so the condition is not tripped and
// this is noted rather than silently muted.
#[allow(clippy::too_many_arguments)]
pub fn serve_tls_with<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    L: MessageLog,
>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    log: L,
    handles: crate::observe::Handles,
    certs: Vec<rustls::pki_types::CertificateDer<'static>>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Result<Shutdown, ServeError> {
    let cfg = default_config(&table)?;
    let tls = crate::tls::server_config(certs, key)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let mut engine: AcceptorEngineOver<
        crate::tls::TlsTransport,
        A,
        crate::block::Block,
        crate::journal::Store,
        NoLog,
        N,
        RX,
        TX,
        APP,
    > = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        crate::block::Block::new(capacity + limits.pending() + 2),
        capacity,
    );
    let _ = engine.adopt(&handles);
    pump(
        acceptor,
        // **The handshake starts here, on the acceptor thread**, which ADR-0020
        // permits to block and which is not the engine thread. A connection
        // whose `rustls` state machine will not even start is dropped: there is
        // no session to tell and nothing intelligible to say on the socket.
        move |sock| {
            rustls::server::UnbufferedServerConnection::new(std::sync::Arc::clone(&tls))
                .ok()
                .map(|conn| crate::tls::TlsTransport::new(sock, crate::tls::Handshake::new(conn)))
        },
        engine.with_log(log),
        table,
        limits,
        crate::recovery::NoRecovery,
    )
}

/// Dial `addr`, run one initiator session on it, and **come back when it
/// ends**. **`standard` mode.**
///
/// `STATUS.md` item 35, and the gap it closes is not subtle:
/// [`connect`] opens a socket and nothing decided when to open it again. An
/// initiator that lost its connection was on its own, and no corpus here
/// covered that — the 59 acceptance definitions are written for an acceptor and
/// never reconnect one, and `scripts/interop.sh` connects once.
///
/// # What it does on each ending
///
/// [`reconnect::Policy`] decides, and it decides for **every** ending including
/// a clean logout — see [`reconnect::Policy::dropped`]. This loop never sleeps
/// waiting for the next attempt: it idles on the engine's own wait strategy,
/// which has a bounded timeout, and re-asks. Non-negotiable 4.
///
/// # Sequence numbers across a reconnect
///
/// [ADR-0010](../../../docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md):
/// FIX 4.4 numbers a **session**, not a connection. So `recovery` is asked on
/// **every** attempt, not only the first, and its answer is what decides
/// whether the new connection continues or restarts.
///
/// With [`recovery::NoRecovery`] every connection starts at `34=1`, which is
/// right for an in-memory journal — it could not have replayed anything anyway
/// — and **wrong for a counterparty that expects continuity**. A deployment
/// that wants numbers to survive a reconnect passes a `Recovery` backed by a
/// journal on disk, exactly as [`serve_with_recovery`] does. `GUIDE.md` carries
/// that, because the type system cannot.
///
/// # Errors
///
/// [`ServeError`] if the engine cannot be built. **A connection that cannot be
/// made is not an error** — it is the case this function exists for, and it
/// goes to the policy.
#[cfg(all(feature = "standard", unix))]
pub fn connect_and_serve<
    A: Application,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
>(
    addr: &str,
    cfg: Config,
    app: A,
    policy: crate::reconnect::Policy,
    recovery: V,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    connect_and_serve_with::<256, 4096, 8192, 1024, A, J, V, L>(
        addr, cfg, app, policy, recovery, log, handles,
    )
}

/// The same, with the three buffer sizes named by the caller. See
/// [`serve_with`] for what `N`, `RX` and `TX` mean and what they cost.
///
/// An initiator sizes `RX` for what the **venue** sends, which is usually the
/// larger direction: a `SecurityList` or a mass quote can dwarf anything this
/// end originates.
///
/// # Errors
///
/// As [`connect_and_serve`].
#[cfg(all(feature = "standard", unix))]
pub fn connect_and_serve_with<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
>(
    addr: &str,
    cfg: Config,
    app: A,
    policy: crate::reconnect::Policy,
    recovery: V,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    let mut engine: TcpInitiatorEngine<A, crate::block::Block, J, NoLog, N, RX, TX, APP> =
        Engine::new(
            cfg,
            InlineDispatch::new(app),
            crate::clock::SystemClock,
            // One connection, one waker. An initiator holds a single session; many
            // of them is `shard`'s problem and is deliberately not this function's.
            crate::block::Block::new(2),
            1,
        );
    let _ = engine.adopt(&handles);
    dial(addr, cfg, Some, engine.with_log(log), policy, recovery)
}

/// Dial `addr` **over TLS**, run one initiator session on it, and come back
/// when it ends. **`standard` mode, Linux.**
///
/// `[2026-09-13]` **step 5b of the `tls` plan**, and the door that lets this
/// engine dial a TLS venue at all. [`connect_and_serve`]'s parameters plus one:
/// [`tls::ClientTls`] carries the roots, the optional client identity, the name
/// the venue's certificate must carry, and `TlsRequireKernel`. See that type
/// for why it is one struct and not four parameters.
///
/// # Where the handshake runs, and why it matters
///
/// **Inside the dial loop, on this thread, before the engine is given the
/// connection.** [ADR-0060]'s check — raise
/// [`observe::EventKind::TlsFellBackToUserspace`], refuse under
/// `require_kernel` — runs when a connection is added, and a client still
/// mid-handshake reads as userspace. Adding straight after `connect`, as the
/// plain loop can, would report a false fallback on every connection and refuse
/// all of them under `TlsRequireKernel=Y`.
/// `tests/tls_initiator_wire.rs::the_dial_loop_adds_no_connection_before_the_handshake_is_decided`
/// holds that.
///
/// The handshake never blocks this thread: the socket is non-blocking, and
/// while the venue has not answered the loop idles on the socket's readiness
/// through the engine's own wait strategy, so `standard` sleeps rather than
/// spins (non-negotiable 4). **Measured, not asserted**:
/// `tests/tls_initiator_wire.rs::the_dial_loop_sleeps_rather_than_spins_while_the_handshake_waits`
/// reads this thread's `utime + stime` out of `/proc` over three seconds
/// against a venue that accepts and never speaks. `[measured 2026-09-13]`
/// **0.00% of a core**, found sleeping 30 reads out of 30; with the
/// `idle_with` call below deleted, **99.90%** and 0 out of 30.
/// `scripts/check-standard-gives-the-core-back.sh` cannot see this loop —
/// it traces `tools/w2w`, which is an acceptor.
///
/// # A venue that never answers
///
/// The handshake is given the configuration's `LogonTimeout` —
/// [`Config::logon_timeout_ms`], `0` for no limit — counted from the moment
/// the TCP connection is made. When it passes, the socket is closed and the
/// ending goes to `policy` like any other.
/// `tests/tls_initiator_wire.rs::a_counterparty_that_accepts_and_never_speaks_tls_is_dropped_at_the_deadline`.
///
/// # Errors
///
/// [`ServeError::Tls`] if `tls` does not make a client configuration, or
/// `require_kernel` is set and this kernel cannot offload TLS. **A handshake
/// that fails is not an error**: like a refused dial, it goes to the policy.
///
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
#[cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
// **Eight, and clippy's ceiling is seven.** Same answer as the four
// `*_with_recovery` entry points: ADR-0054 recorded a `Serve` builder as the
// alternative and named its reopening condition — *the first time an eleventh
// parameter is wanted*. Eight is under it because the four TLS facts travel as
// one `ClientTls`; spread out they would be eleven and trip it.
#[allow(clippy::too_many_arguments)]
pub fn connect_and_serve_tls<
    A: Application,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
>(
    addr: &str,
    cfg: Config,
    app: A,
    policy: crate::reconnect::Policy,
    recovery: V,
    log: L,
    handles: crate::observe::Handles,
    tls: crate::tls::ClientTls,
) -> Result<Shutdown, ServeError> {
    connect_and_serve_tls_with::<256, 4096, 8192, 1024, A, J, V, L>(
        addr,
        cfg,
        app,
        policy,
        recovery,
        log,
        handles,
        tls,
        crate::tls::TlsProbe::Real,
    )
}

/// The full initiator TLS door: buffer sizes, **and the offload seam**.
///
/// See [`serve_with`] for what `N`, `RX` and `TX` cost, and
/// [`serve_tls_with_offload`] for `probe`: a deployment passes
/// [`tls::TlsProbe::Real`], and the other two arms exist so both halves of
/// [ADR-0060] decision 1 can be reached from a test on a kernel that offloads.
///
/// # Errors
///
/// As [`connect_and_serve_tls`].
///
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
#[cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
// **Nine** — [`connect_and_serve_tls`]'s eight plus the probe, which is a test
// seam and deliberately not a field of `ClientTls`. Under ADR-0054's eleven.
#[allow(clippy::too_many_arguments)]
pub fn connect_and_serve_tls_with<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
>(
    addr: &str,
    cfg: Config,
    app: A,
    policy: crate::reconnect::Policy,
    recovery: V,
    log: L,
    handles: crate::observe::Handles,
    tls: crate::tls::ClientTls,
    probe: crate::tls::TlsProbe,
) -> Result<Shutdown, ServeError> {
    let crate::tls::ClientTls {
        roots,
        identity,
        server_name,
        require_kernel,
    } = tls;
    // **ADR-0060 decision 1, first half, before the first dial** — the
    // initiator's counterpart of the check `serve_tls_with_offload` makes
    // before `bind`, with the same sentence.
    let kernel_offloads = match probe {
        crate::tls::TlsProbe::PretendKernelCannotOffload => false,
        crate::tls::TlsProbe::Real | crate::tls::TlsProbe::PretendHandshakeFallsBack => {
            crate::tls::kernel_can_offload()
        }
    };
    if require_kernel && !kernel_offloads {
        return Err(ServeError::Tls(
            "TlsRequireKernel=Y, and this kernel cannot offload TLS: it has no \
             tls ULP, so every session would fall back to userspace rustls and \
             leave the hot-path guarantee"
                .to_owned(),
        ));
    }
    let client = crate::tls::client_config(roots, identity)?;
    let mut engine: InitiatorEngineOver<
        crate::tls::TlsTransport<crate::tls::Client>,
        A,
        crate::block::Block,
        J,
        NoLog,
        N,
        RX,
        TX,
        APP,
    > = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        // One connection — or one handshake, which is not yet a connection —
        // and one waker, as `connect_and_serve_with`.
        crate::block::Block::new(2),
        1,
    );
    engine.require_kernel(require_kernel);
    let _ = engine.adopt(&handles);
    let offload = matches!(probe, crate::tls::TlsProbe::Real);
    dial(
        addr,
        cfg,
        // **Per connection, not per message**: one `Arc` count and one name
        // clone at dial time, inside the handshake carve-out of non-negotiable
        // 1. A connection whose `rustls` state machine will not start is
        // dropped and goes to the policy.
        move |sock| {
            rustls::client::UnbufferedClientConnection::new(
                std::sync::Arc::clone(&client),
                server_name.clone(),
            )
            .ok()
            .map(|conn| {
                crate::tls::TlsTransport::with_offload(
                    sock,
                    crate::tls::Handshake::new(conn),
                    offload,
                )
            })
        },
        engine.with_log(log),
        policy,
        recovery,
    )
}

/// What a freshly dialled transport says about joining the engine.
#[cfg(all(feature = "standard", unix))]
#[cfg_attr(
    not(all(feature = "tls", target_os = "linux")),
    expect(
        dead_code,
        reason = "only a TLS transport is ever Pending or Failed; a plain socket is Ready at connect"
    )
)]
enum Admission {
    /// Decided: hand it to the engine. For TLS, the keys are in the kernel or
    /// the handshake fell back — either way [`Transport::tls_mode`] is final.
    Ready,
    /// Still negotiating. Wait on the socket and ask again.
    Pending,
    /// It will never be ready; the ending goes to the policy.
    Failed,
}

/// A transport `dial` hands to the engine **only once it says so**.
///
/// `[2026-09-13]` step 5b of the `tls` plan, 6.2 item 5. Private: it is the one
/// question `dial` asks that [`Transport`] does not, and it has exactly two
/// answers in this crate.
#[cfg(all(feature = "standard", unix))]
trait Dialled: Transport {
    fn admission(&mut self) -> Admission;
}

/// A plain socket is decided the moment `connect` returns, so the plain loop
/// adds on the same turn it always did.
#[cfg(all(feature = "standard", unix))]
impl Dialled for TcpTransport {
    fn admission(&mut self) -> Admission {
        Admission::Ready
    }
}

#[cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
impl Dialled for crate::tls::TlsTransport<crate::tls::Client> {
    fn admission(&mut self) -> Admission {
        if self.is_ready() || self.fell_back() {
            return Admission::Ready;
        }
        // **An empty send is how the handshake is driven without touching
        // application bytes.** `TlsTransport::send` advances the handshake
        // first and then offers the buffer: while handshaking it answers
        // `Idle`; on the kernel the empty buffer reaches `TcpTransport::send`,
        // which returns `Idle` without a syscall; in userspace `Traffic::send`
        // flushes and returns `Idle` for an empty buffer. `recv` would do the
        // driving too, but could consume a record the venue sent straight after
        // its `Finished` before the session exists to read it.
        match Transport::send(self, &[]) {
            crate::transport::Io::Failed(_) | crate::transport::Io::Closed => {
                return Admission::Failed;
            }
            crate::transport::Io::Idle | crate::transport::Io::Ready(_) => {}
        }
        if self.is_ready() || self.fell_back() {
            Admission::Ready
        } else {
            Admission::Pending
        }
    }
}

/// The loop [`connect_and_serve`] runs, generic over the wait strategy so a
/// test can drive it without a real clock.
///
/// `[2026-09-13]` **generic over the transport, and holding a handshake slot**
/// — step 5b of the `tls` plan. `wrap` turns a connected socket into this
/// engine's transport, as it does for `pump`; for the plain doors it is `Some`
/// and folds away. The socket is added to the engine only once
/// [`Dialled::admission`] says `Ready`, because ADR-0060's check runs when a
/// connection is added and must see a decided mode. Until then it sits in
/// `handshaking`, bounded by `LogonTimeout`.
#[cfg(all(feature = "standard", unix))]
fn dial<
    T: Dialled,
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    W: Waiting,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
    F: FnMut(TcpTransport) -> Option<T>,
>(
    addr: &str,
    cfg: Config,
    wrap: F,
    mut engine: InitiatorEngineOver<T, A, W, J, L, N, RX, TX, APP>,
    policy: crate::reconnect::Policy,
    recovery: V,
) -> Result<Shutdown, ServeError> {
    let done = dial_loop(addr, cfg, wrap, &mut engine, policy, recovery);
    let grace = engine.stop_grace_ms();
    // Whatever the loop left is retired here, before the wait, not after it.
    drop(engine);
    after_serving(grace);
    done
}

/// [`dial`]'s loop. It borrows the engine so that [`dial`] can drop it — and
/// retire every journal it still holds — before waiting for their writers.
#[cfg(all(feature = "standard", unix))]
fn dial_loop<
    T: Dialled,
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    W: Waiting,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
    F: FnMut(TcpTransport) -> Option<T>,
>(
    addr: &str,
    cfg: Config,
    mut wrap: F,
    engine: &mut InitiatorEngineOver<T, A, W, J, L, N, RX, TX, APP>,
    mut policy: crate::reconnect::Policy,
    mut recovery: V,
) -> Result<Shutdown, ServeError> {
    // **A counter, not the event stream.** `Observer::events` *drains* the
    // ring, so this loop reading it would take every `LoggedOn` the caller's own
    // `Observer` is waiting for — two readers on one cell share events rather
    // than each seeing them. Before `Handles`, the caller had no cell and there
    // was nobody to take them from. ADR-0054 decision 5.
    let mut logons = engine.logons();
    let mut clock = crate::clock::SystemClock;
    let mut up = false;
    // `LogonTimeout`, `0` for none — plan 6.2 item 6, approved 2026-09-13.
    let handshake_ms = cfg.logon_timeout_ms();
    // A connected socket whose handshake is not yet decided, and when it was
    // connected. Always `None` between turns for a plain transport.
    let mut handshaking: Option<(T, u64)> = None;
    loop {
        let now = crate::clock::Clock::now_ms(&mut clock);

        if engine.connections() == 0 {
            // **The ending is recorded once, on the turn the connection goes.**
            // `dropped` climbs the ladder, so calling it every idle turn would
            // walk to the ceiling in milliseconds.
            if up {
                policy.dropped(now);
                up = false;
            }
            if handshaking.is_none() {
                match policy.next(now) {
                    // Nothing is connected, so there is nothing to say goodbye
                    // to. A `Shutdown` reporting zero sessions is the truth
                    // here, not a placeholder.
                    crate::reconnect::Next::Stop => return Ok(Shutdown::default()),
                    // **Not yet: fall through, do not `continue`.** The wait
                    // is the third idle arm at the bottom of this loop, and
                    // the way there passes `engine.turn()` — where a
                    // shutdown is noticed — and `shutdown_finished()` — where
                    // it is returned. A `continue` here skipped both, so
                    // `Admin::shutdown` was heard only when the policy said
                    // `Now` again: `[measured 2026-09-23]` +26 s with
                    // `ReconnectInterval=30`. Plan
                    // `2026-09-23-phase-3-found-defects` row D2;
                    // `tests/reconnect_wire.rs::a_dial_waiting_to_reconnect_stops_when_asked_not_when_the_timer_fires`.
                    crate::reconnect::Next::At(_) => {}
                    crate::reconnect::Next::Now => match connect(addr) {
                        Ok(t) => match wrap(t) {
                            Some(t) => handshaking = Some((t, now)),
                            None => policy.dropped(now),
                        },
                        // A refused dial is an ending like any other. It is the
                        // commonest one there is, and treating it as an error
                        // would end the loop the first time a venue was slow to
                        // come up.
                        Err(_) => policy.dropped(now),
                    },
                }
            }
            if let Some((mut t, since)) = handshaking.take() {
                match t.admission() {
                    Admission::Ready => {
                        // **Asked on every attempt, not only the first.** A
                        // reconnect is not a restart (ADR-0010), and whether
                        // this one continues is the recovery's answer, not this
                        // loop's guess. Asked once the connection is decided,
                        // so a handshake that fails reads no journal.
                        //
                        // The prefix door rather than `add_with_journal`, and
                        // the prefix is empty: it is the add that carries
                        // ADR-0060's check. For a plain socket that check is
                        // one never-taken comparison and the connection is
                        // built exactly as `add_with_journal`/`add_resumed`
                        // build it.
                        let state = recovery.recover(&cfg);
                        match engine.add_with_prefix_config_and_journal(t, cfg, &[], state, || {
                            recovery.fresh(&cfg)
                        }) {
                            // **Up from the add, not from the next turn.** A
                            // connection ADR-0060 refuses is gone within its
                            // first turn; judged from after the turn it would
                            // never have been up, `dropped` would never run,
                            // and the policy would answer `Now` again at once —
                            // a reconnect storm against a venue whose only
                            // fault is a cipher suite. `[measured 2026-09-13]`
                            // 879 dials in 1.5 s without this line, 7 with it;
                            // `tests/tls_initiator_wire.rs::an_initiator_that_fell_back_is_reported_and_refused_when_the_kernel_was_demanded`
                            // counts the refusals.
                            Ok(_) => up = true,
                            // An empty prefix always fits; if it somehow did
                            // not, the socket is gone and that is an ending.
                            Err(_) => policy.dropped(now),
                        }
                    }
                    Admission::Pending
                        if handshake_ms != 0 && now.saturating_sub(since) >= handshake_ms =>
                    {
                        // A venue that took the TCP connection and never
                        // finished the handshake. No session exists to time it
                        // out, so this loop does. Dropping `t` closes it.
                        policy.dropped(now);
                    }
                    Admission::Pending => handshaking = Some((t, since)),
                    Admission::Failed => {
                        // ADR-0151 decision 4, the initiator's half: a venue
                        // that refused the handshake is said, a venue that
                        // vanished is not. Either way it is an ending.
                        if t.handshake_refused() {
                            engine.note_tls_refused(1);
                        }
                        policy.dropped(now);
                    }
                }
            }
        }

        let moved = engine.turn();
        if engine.connections() > 0 {
            up = true;
        }
        // `LoggedOn` is what resets the ladder, and it is not "a socket
        // connected": a connection refused its `Logon` and dropped is a
        // failure, and counting it as success is how a policy hammers a
        // counterparty that is up but refusing.
        let now_on = engine.logons();
        if now_on != logons {
            logons = now_on;
            policy.logged_on();
        }

        // A shutdown asked for mid-handshake finishes here too: the engine
        // holds no connection, and returning drops the socket in the slot.
        if let Some(done) = engine.shutdown_finished() {
            return Ok(done);
        }
        if !moved {
            if engine.connections() > 0 {
                engine.idle();
            } else if let Some((t, _)) = handshaking.as_ref() {
                // **Wait on the venue, never spin and never block on a read.**
                // In `standard` this sleeps until the socket is readable or the
                // strategy's own timeout passes, which also bounds how late the
                // deadline above is noticed.
                //
                // Deleting this arm is the reversal of
                // `tests/tls_initiator_wire.rs::the_dial_loop_sleeps_rather_than_spins_while_the_handshake_waits`:
                // `[measured 2026-09-13]` 0.00% of a core becomes 99.90%.
                match t.source() {
                    Some(source) => engine.idle_with(&[Interest::readable(source)]),
                    None => engine.idle(),
                }
            } else {
                // **Nothing connected, nothing handshaking: the reconnect
                // wait.** Nothing to wait on but the clock, and the wait
                // strategy's own timeout bounds it — this does not sleep on a
                // deadline it chose, which is what non-negotiable 4 is about.
                // Each wake goes round through `turn()` above, so a shutdown is
                // heard within one timeout rather than at the next dial.
                //
                // Deleting this arm is the reversal of
                // `tests/reconnect_wire.rs::the_dial_loop_sleeps_rather_than_spins_while_it_waits_to_reconnect`.
                engine.idle_with(&[]);
            }
        }
    }
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
/// **`standard` only**, like [`serve`] — it builds the blocking engine, and
/// `crate::block` does not exist without that feature. Non-negotiable 6: the
/// `#[cfg]` is on the item, not only in `Cargo.toml`.
#[cfg(all(feature = "standard", unix))]
// **Eight, and clippy's ceiling is seven.** ADR-0054 took the parameter
// deliberately and recorded the alternative — a `Serve` builder — as deferred
// with its reopening condition: *the first time an eleventh parameter is
// wanted*. This lint is the language's opinion arriving early; it is noted in
// that ADR's Consequences rather than silently muted, and the four entry points
// it names are exactly the four that carry both a `Recovery` and a log.
#[allow(clippy::too_many_arguments)]
pub fn serve_with_recovery<
    A: Application,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    recovery: V,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    serve_with_recovery_with::<256, 4096, 8192, 1024, A, J, V, L>(
        addr, table, app, capacity, limits, recovery, log, handles,
    )
}

/// The same, with the three buffer sizes named by the caller. See
/// [`serve_with`] for what `N`, `RX` and `TX` mean and what they cost.
///
/// # Errors
///
/// As [`serve_with_recovery`].
#[cfg(all(feature = "standard", unix))]
// **Eight, and clippy's ceiling is seven.** ADR-0054 took the parameter
// deliberately and recorded the alternative — a `Serve` builder — as deferred
// with its reopening condition: *the first time an eleventh parameter is
// wanted*. This lint is the language's opinion arriving early; it is noted in
// that ADR's Consequences rather than silently muted, and the four entry points
// it names are exactly the four that carry both a `Recovery` and a log.
#[allow(clippy::too_many_arguments)]
pub fn serve_with_recovery_with<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    recovery: V,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    let cfg = default_config(&table)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let mut engine: TcpAcceptorEngine<A, crate::block::Block, J, NoLog, N, RX, TX, APP> =
        Engine::new(
            cfg,
            InlineDispatch::new(app),
            crate::clock::SystemClock,
            crate::block::Block::new(capacity + limits.pending() + 2),
            capacity,
        );
    let _ = engine.adopt(&handles);
    pump(
        acceptor,
        Some,
        engine.with_log(log),
        table,
        limits,
        recovery,
    )
}

/// As `serve`, in `hft` mode: **spins, and burns a core for as long as the
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
pub fn serve_hft<A: Application, L: MessageLog>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    serve_hft_with::<256, 4096, 8192, 1024, A, L>(addr, table, app, capacity, limits, log, handles)
}

/// The same, with the three buffer sizes named by the caller. See
/// `serve_with` for what `N`, `RX` and `TX` mean and what they cost.
///
/// # Errors
///
/// As [`serve_hft`].
pub fn serve_hft_with<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    L: MessageLog,
>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    let cfg = default_config(&table)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let mut engine: HftAcceptorEngine<A, NoLog, N, RX, TX, APP> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        crate::wait::Spin,
        capacity,
    );
    let _ = engine.adopt(&handles);
    pump(
        acceptor,
        Some,
        engine.with_log(log),
        table,
        limits,
        crate::recovery::NoRecovery,
    )
}

/// As [`serve_hft`], on the core `pin` names: **refuses a bad core first, pins
/// the thread that called it, and only then binds.**
///
/// `DESIGN.md` D8 says the `hft` polling thread is pinned to an isolated core.
/// [`serve_hft`] leaves that thread yours to pin, which is a valid choice
/// (`taskset` around the process); this is the door that does it for you and
/// refuses what `serve_sharded_hft` would
/// refuse.
///
/// # The order, and why it is this one
///
/// 1. [`CorePin::validate`](affinity::CorePin::validate) — every rule
///    [`affinity::Topology::validate`] already has, and none of its own: a core
///    that is absent, offline, or outside `isolcpus` unless
///    [`allow_unisolated`](affinity::CorePin::allow_unisolated) was called.
/// 2. [`affinity::pin_current_thread`], which reads the mask back rather than
///    trusting the call's return — ADR-0015 decision 2.
/// 3. Bind and serve, exactly as [`serve_hft`] does.
///
/// **Refusal before bind** because a listener bound first and only then told
/// the core is wrong is a port held while the operator reads the error — and a
/// refusal after a socket exists is the half-started runtime ADR-0015 decision 6
/// forbids. **Pin before bind** because the thread that binds is the thread that
/// serves, and nothing it does should happen unpinned.
/// `tests/hft_pinned.rs::serve_hft_pinned_refuses_a_core_the_machine_does_not_have`
/// holds the order: it hands this function a port that is already taken, so a
/// door that bound first would answer [`ServeError::Io`].
///
/// Both syscalls and the `/sys` reads happen **once, before the first socket**,
/// and [`Engine::turn`] is untouched — non-negotiables 1 and 4 are about the
/// loop, and none of this is in it.
///
/// # The pin outlives the call
///
/// It pins **the calling thread**, because that is the thread the engine runs
/// on, and nothing unpins it afterwards: not when this returns a [`Shutdown`],
/// and not when it returns an error raised after step 2 —
/// [`ServeError::NoCounterparties`], [`ServeError::Io`]. Call it from a thread
/// that exists to serve. `tests/hft_pinned.rs::serve_hft_pinned_serves_on_the_core_it_was_given`
/// reads the calling thread's mask after a clean return. A refused core, or a
/// `sched_setaffinity` the kernel refuses, leaves the thread as it was, because
/// a failing call changes nothing (see [`affinity::pin_current_thread`]). A
/// [`ReadbackMismatch`](affinity::AffinityError::ReadbackMismatch) is the one
/// affinity error that does not: the call succeeded, and the thread's mask is
/// whatever the kernel made it.
///
/// # Errors
///
/// [`ServeError::Affinity`] if the core is refused or the pin does not take,
/// before any socket exists; otherwise as [`serve_hft`].
#[cfg(all(feature = "affinity", target_os = "linux"))]
// **Eight, and clippy's ceiling is seven.** The eighth is the pin. ADR-0054's
// Consequences took this same lint on four entry points rather than muting it,
// and recorded the deferred `Serve` builder's reopening condition: *the first
// time an eleventh parameter is wanted*. This is a fifth door at eight, not an
// eleventh parameter, so the condition has not arrived; it is noted here so the
// next parameter meets a count instead of a habit.
#[allow(clippy::too_many_arguments)]
pub fn serve_hft_pinned<A: Application, L: MessageLog>(
    pin: &affinity::CorePin,
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    pin.validate().map_err(ServeError::Affinity)?;
    affinity::pin_current_thread(pin.core()).map_err(ServeError::Affinity)?;
    serve_hft_with::<256, 4096, 8192, 1024, A, L>(addr, table, app, capacity, limits, log, handles)
}

/// As [`serve_hft`], asking `recovery` what each counterparty left behind. See
/// `serve_with_recovery`.
///
/// # Errors
///
/// As [`serve_hft`].
// **Eight, and clippy's ceiling is seven.** ADR-0054 took the parameter
// deliberately and recorded the alternative — a `Serve` builder — as deferred
// with its reopening condition: *the first time an eleventh parameter is
// wanted*. This lint is the language's opinion arriving early; it is noted in
// that ADR's Consequences rather than silently muted, and the four entry points
// it names are exactly the four that carry both a `Recovery` and a log.
#[allow(clippy::too_many_arguments)]
pub fn serve_hft_with_recovery<
    A: Application,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    recovery: V,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    serve_hft_with_recovery_with::<256, 4096, 8192, 1024, A, J, V, L>(
        addr, table, app, capacity, limits, recovery, log, handles,
    )
}

/// The same, with the three buffer sizes named by the caller. See
/// `serve_with` for what `N`, `RX` and `TX` mean and what they cost.
///
/// # Errors
///
/// As [`serve_hft_with_recovery`].
// **Eight, and clippy's ceiling is seven.** ADR-0054 took the parameter
// deliberately and recorded the alternative — a `Serve` builder — as deferred
// with its reopening condition: *the first time an eleventh parameter is
// wanted*. This lint is the language's opinion arriving early; it is noted in
// that ADR's Consequences rather than silently muted, and the four entry points
// it names are exactly the four that carry both a `Recovery` and a log.
#[allow(clippy::too_many_arguments)]
pub fn serve_hft_with_recovery_with<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
>(
    addr: &str,
    table: presession::Table,
    app: A,
    capacity: usize,
    limits: presession::Limits,
    recovery: V,
    log: L,
    handles: crate::observe::Handles,
) -> Result<Shutdown, ServeError> {
    let cfg = default_config(&table)?;
    let acceptor = Acceptor::bind(addr).map_err(ServeError::Io)?;
    let mut engine: TcpAcceptorEngine<A, crate::wait::Spin, J, NoLog, N, RX, TX, APP> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        crate::wait::Spin,
        capacity,
    );
    let _ = engine.adopt(&handles);
    pump(
        acceptor,
        Some,
        engine.with_log(log),
        table,
        limits,
        recovery,
    )
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
    /// `FileLogPath` named somewhere this process cannot append to.
    ///
    /// **Its own variant, not [`Self::Io`].** A missing directory and a port
    /// already in use send an operator to two different places, and one variant
    /// covering both sends them to the wrong one first.
    LogPath(std::io::Error),
    /// TLS could not be set up: the certificate and key do not make a server
    /// configuration, the roots or identity do not make a client one, or
    /// `TlsRequireKernel=Y` on a kernel that cannot offload.
    ///
    /// **Its own variant for the same reason [`Self::LogPath`] is.** A bad
    /// certificate and a busy port are two different mornings, and one variant
    /// covering both sends an operator to the wrong one first. `[2026-09-10]`
    /// added with `serve_tls`, step 4a of the `tls` plan.
    ///
    /// It carries a `String` rather than the `rustls` error: `ServeError` is a
    /// public type of this crate and `rustls` is optional, so a variant holding
    /// one would change shape with a feature flag.
    #[cfg(feature = "tls")]
    Tls(String),
    /// The core named for the engine thread was refused, or pinning to it
    /// failed — raised by [`serve_hft_pinned`], **always before a socket
    /// exists**.
    ///
    /// **Its own variant for the reason [`Self::LogPath`] is.** A core outside
    /// `isolcpus` and a port already in use send an operator to two different
    /// places — the kernel command line and the process table — and one variant
    /// covering both sends them to the wrong one first. It carries the
    /// [`affinity::AffinityError`] whole, because that error names the
    /// offending core (ADR-0015 decision 4).
    ///
    /// Behind the same `cfg` as `mod affinity`, because the error it holds does
    /// not exist without it — the shape `Tls` already has. `ServeError`
    /// is `#[non_exhaustive]`, so the variant is not a breaking change.
    /// `[2026-09-13]` added with [`serve_hft_pinned`], `STATUS.md` item 21.
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    Affinity(crate::affinity::AffinityError),
}

impl core::fmt::Display for ServeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoCounterparties => write!(
                f,
                "the registry serves no counterparty, so this acceptor would refuse every connection"
            ),
            Self::Io(e) => write!(f, "binding the listener: {e}"),
            Self::LogPath(e) => write!(f, "opening the message log named by FileLogPath: {e}"),
            #[cfg(feature = "tls")]
            // **Side-neutral since step 5b of the `tls` plan.** It read
            // "building the TLS server configuration" while only an acceptor
            // could raise it; `tls::client_config` and the initiator's
            // `TlsRequireKernel` refusal raise it now too, and a sentence naming
            // the wrong end sends an operator to the wrong configuration file.
            // The message after the colon already names what failed, which is
            // why this is a wording change and not a new variant — a variant
            // would be a breaking change to a public enum for a distinction
            // the text already makes.
            Self::Tls(e) => write!(f, "setting up TLS: {e}"),
            #[cfg(all(feature = "affinity", target_os = "linux"))]
            Self::Affinity(e) => write!(f, "pinning the engine thread: {e}"),
        }
    }
}

impl std::error::Error for ServeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoCounterparties => None,
            #[cfg(feature = "tls")]
            Self::Tls(_) => None,
            Self::Io(e) | Self::LogPath(e) => Some(e),
            #[cfg(all(feature = "affinity", target_os = "linux"))]
            Self::Affinity(e) => Some(e),
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

/// How many turns before `pump` asks the listener again — ADR-0069 decision 1.
///
/// A `u32` pair on the serving thread's stack: a compare, a subtraction, no
/// clock and nothing allocated. Cadence 1 answers `true` on every turn, which
/// is the loop as it was written before this existed.
struct ListenerCadence {
    every: u32,
    until: u32,
}

impl ListenerCadence {
    const fn new(every: core::num::NonZeroU32) -> Self {
        Self {
            every: every.get(),
            until: 0,
        }
    }

    /// Whether this turn asks the listener. Asking rearms the countdown.
    fn poll_now(&mut self) -> bool {
        if self.until != 0 {
            self.until -= 1;
            return false;
        }
        self.until = self.every - 1;
        true
    }

    /// The engine came back from `idle_with`: the next turn asks the listener
    /// whatever the countdown said — ADR-0069 decision 3. In `standard` the
    /// wake may *be* the listener, and a wake answered by skipping the accept
    /// leaves it readable, so `poll` returns at once and the loop spins through
    /// the whole countdown: a working engine burning a core, the half of
    /// non-negotiable 4 this mode exists to satisfy. In the spin half `idle`
    /// returns immediately, so this also means an idle engine accepts on the
    /// next turn at any cadence — the cadence thins out `accept4` between
    /// messages, which is what Boot B step B9 measured, and costs nothing when
    /// there are none.
    fn woke(&mut self) {
        self.until = 0;
    }
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
/// `[2026-09-10]` **It is generic over the transport, and that is step 4a of the
/// `tls` plan.** It used to name `TcpTransport` in one place — the `PendingSet`
/// annotation below — and that single line was the whole reason no deployment
/// could reach TLS: `TlsTransport` existed, was tested, and had nowhere to go.
///
/// `[2026-09-18]` **the listener is asked on a cadence, not on every turn.**
/// `limits.listener_every()` is how many turns apart the accept loop runs
/// (ADR-0069, `STATUS.md` item 89: `accept4` on an empty listener held 37-50%
/// of the engine thread's samples between messages). The countdown is a `u32`
/// on this stack and is reset to zero after every return from `idle_with`, so a
/// blocking engine always answers a wake with an accept. Default 1 is this loop
/// turn for turn.
///
/// `wrap` turns an accepted socket into whatever this engine's connections are.
/// For every plain caller it is `Some`, and the compiler removes it; for
/// [`serve_tls`] it starts a handshake. Returning `None` drops the socket, which
/// is the same answer this loop already gave a connection it had no room for.
fn pump<
    T: crate::transport::Transport,
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    W: Waiting,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
    F: FnMut(TcpTransport) -> Option<T>,
>(
    acceptor: Acceptor,
    wrap: F,
    mut engine: AcceptorEngineOver<T, A, W, J, L, N, RX, TX, APP>,
    table: presession::Table,
    limits: presession::Limits,
    recovery: V,
) -> Result<Shutdown, ServeError> {
    let done = pump_loop(acceptor, wrap, &mut engine, table, limits, recovery);
    let grace = engine.stop_grace_ms();
    // Whatever the loop left is retired here, before the wait, not after it.
    drop(engine);
    after_serving(grace);
    done
}

/// The least [`after_serving`] waits for retired journal writers, whatever
/// grace the shutdown was asked with.
///
/// `Admin::shutdown(0)` asks for the **counterparties** to be cut off at once;
/// it does not ask for this process's own journal to be cut short, which is the
/// loss ADR-0153 rejected as *"detach with no barrier"*. The wait returns as
/// soon as the writers are done, so the floor costs nothing when they are
/// quick. `[unmeasured]` how long a writer takes to drain a full 1 MiB ring;
/// one second is a bound on a page-cache write, not a measurement.
const RETIRED_WRITERS_FLOOR_MS: u64 = 1_000;

/// After a serving loop has returned and its engine has been dropped — every
/// journal in it retired — wait for their writers. ADR-0153 decision 4.
///
/// **Teardown, not serving**: this sleeps, which ADR-0152 decision 1 allows
/// once the loop has returned. The timeout is the shutdown's grace, never less
/// than [`RETIRED_WRITERS_FLOOR_MS`]. A writer still running when it passes is
/// left to the process's exit; the serve function's result does not change,
/// because what it reports is the sessions, and they have already ended.
pub(crate) fn after_serving(grace_ms: Option<u64>) {
    let ms = grace_ms.unwrap_or(0).max(RETIRED_WRITERS_FLOOR_MS);
    let _ = journal::wait_for_retired_writers(std::time::Duration::from_millis(ms));
}

/// [`pump`]'s loop. It borrows the engine so that [`pump`] can drop it — and
/// retire every journal it still holds — before waiting for their writers.
fn pump_loop<
    T: crate::transport::Transport,
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A: Application,
    W: Waiting,
    J: SessionJournal,
    V: crate::recovery::Recovery<J>,
    L: MessageLog,
    F: FnMut(TcpTransport) -> Option<T>,
>(
    acceptor: Acceptor,
    mut wrap: F,
    engine: &mut AcceptorEngineOver<T, A, W, J, L, N, RX, TX, APP>,
    table: presession::Table,
    limits: presession::Limits,
    mut recovery: V,
) -> Result<Shutdown, ServeError> {
    // `[2026-09-05]` **The pre-session buffer IS the engine's `RX`, and that is
    // now a type rather than a promise.** It used to read
    // `const PRE: usize = 4096;` under a comment saying it matched the engine —
    // in this function and again in `shard.rs`, two copies of one invariant with
    // nothing checking either. A prefix longer than the connection's buffer is
    // unframeable the instant it is handed over, so the two cannot drift; now
    // they cannot be written apart.
    let mut set: presession::PendingSet<T, presession::Table, RX> =
        presession::PendingSet::new(limits, table);
    let mut clock = crate::clock::SystemClock;
    let listener = acceptor.source().map(Interest::readable);
    let mut extra: Vec<Interest> = Vec::with_capacity(limits.pending() + 1);
    let mut cadence = ListenerCadence::new(limits.listener_every());
    loop {
        let mut moved = false;
        if cadence.poll_now() {
            while set.len() < limits.pending() {
                let Some(t) = acceptor.accept() else { break };
                // `wrap` returning `None` closes the socket too — a TLS acceptor
                // that cannot build a connection for it has nothing to say on it.
                let Some(t) = wrap(t) else {
                    moved = true;
                    continue;
                };
                // Dropping the refusal closes the socket, which is what a caller
                // with nowhere to put a connection should do.
                drop(set.admit(t, crate::clock::Clock::now_ms(&mut clock)));
                moved = true;
            }
        }
        let now = crate::clock::Clock::now_ms(&mut clock);
        let p = set.turn(now);
        // The pre-session stage is in front of the engine and keeps its own
        // counts; this is the one line that lets an operator see this one.
        engine.note_unframeable(p.unframeable);
        // ADR-0151 decision 4: a handshake TLS refused is an event, where a
        // peer that left (`p.gone`) is not.
        engine.note_tls_refused(p.tls_refused);
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
            // `fresh` is a closure rather than `J::default`, which is the
            // whole of item 32 (b): a `FileJournal` has no honest `Default`.
            let _ = engine.add_with_prefix_config_and_journal(t, cfg, &buf[..len], state, || {
                recovery.fresh(&cfg)
            });
            moved = true;
        }
        moved |= engine.turn();
        if let Some(done) = engine.shutdown_finished() {
            // **Sockets still waiting to identify themselves are dropped, not
            // logged out.** There is no session on them and nothing to say
            // goodbye to; `PendingSet`'s own drop closes them.
            return Ok(done);
        }
        if !moved {
            extra.clear();
            extra.extend(listener);
            set.interests(&mut extra);
            engine.idle_with(&extra);
            // **After every return from `idle_with`, the listener is asked on
            // the next turn, whatever the cadence says** — see
            // [`ListenerCadence::woke`].
            cadence.woke();
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

/// The engine's own record of a shutdown in progress.
#[derive(Debug, Clone, Copy)]
struct Stopping {
    deadline_ms: u64,
    sessions: usize,
    said_goodbye: usize,
    acked: usize,
    timed_out: usize,
}

/// What an ordered shutdown managed, and what it did not.
///
/// **`sessions == acked` is the clean case and nothing else is.** An operator
/// restarting after `timed_out > 0` is restarting against counterparties that
/// never acknowledged, and may have to reconcile sequence numbers by hand — so
/// the two outcomes are reported apart rather than folded into "stopped".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shutdown {
    sessions: usize,
    said_goodbye: usize,
    acked: usize,
    timed_out: usize,
}

impl Shutdown {
    /// Connections the engine held when the shutdown began.
    #[must_use]
    pub const fn sessions(&self) -> usize {
        self.sessions
    }

    /// How many of them actually had a `Logout` written for them.
    ///
    /// Lower than [`Self::sessions`] when a connection had not logged on, or
    /// could not build the message.
    #[must_use]
    pub const fn said_goodbye(&self) -> usize {
        self.said_goodbye
    }

    /// How many answered with a `Logout` of their own before the deadline.
    #[must_use]
    pub const fn acked(&self) -> usize {
        self.acked
    }

    /// How many were still there when the deadline passed, and were closed.
    #[must_use]
    pub const fn timed_out(&self) -> usize {
        self.timed_out
    }

    /// Every session that was told, answered.
    #[must_use]
    pub const fn clean(&self) -> bool {
        self.timed_out == 0 && self.acked == self.said_goodbye
    }
}

/// The cadence counter `pump` keeps, on its own so it can be asked questions.
///
/// `[2026-09-18]` **pure logic, so it is tested as pure logic** — `CLAUDE.md`
/// §7. In `pump` the countdown is unobservable: the reset after `idle_with`
/// (ADR-0069 decision 3) reaches the listener whenever the engine has a spare
/// moment, so a countdown that never reached zero left every socket test in
/// `crates/engine/tests/listener_cadence.rs` green. Here the same break is one
/// red assertion.
#[cfg(test)]
mod cadence_tests {
    use super::ListenerCadence;
    use core::num::NonZeroU32;

    /// `const`, and matched rather than unwrapped: non-negotiable 7 is a
    /// workspace lint and this module is inside the library crate, so there is
    /// no `expect` to be had here — `crates/engine/tests/` is where those live.
    const fn every(n: u32) -> ListenerCadence {
        let every = match NonZeroU32::new(n) {
            Some(n) => n,
            // Unreachable for every caller below; a cadence of zero is refused
            // by `Limits::with_listener_every` long before `pump` sees one.
            None => NonZeroU32::MIN,
        };
        ListenerCadence::new(every)
    }

    /// Cadence 1 is the loop as it was: the listener on every turn.
    #[test]
    fn every_one_asks_on_every_turn() {
        let mut c = every(1);
        for turn in 0..10 {
            assert!(c.poll_now(), "cadence 1 skipped turn {turn}");
        }
    }

    /// Cadence 3 asks on one turn in three, starting with the first.
    #[test]
    fn every_three_asks_once_in_three() {
        let mut c = every(3);
        let answers: Vec<bool> = (0..7).map(|_| c.poll_now()).collect();
        assert_eq!(
            answers,
            vec![true, false, false, true, false, false, true],
            "one turn in three asks the listener, and the first one does"
        );
    }

    /// A wake is answered by an accept wherever the countdown had got to —
    /// the half of ADR-0069 that keeps non-negotiable 4's `standard` side true.
    #[test]
    fn woke_forces_the_next_turn_whatever_the_position() {
        for skipped in 0..4 {
            let mut c = every(5);
            assert!(c.poll_now(), "the first turn asks");
            for _ in 0..skipped {
                assert!(!c.poll_now(), "mid-cadence turns skip");
            }
            c.woke();
            assert!(
                c.poll_now(),
                "after a wake the listener is asked, {skipped} turns into the \
                 cadence"
            );
        }
    }

    /// The largest cadence there is arms and counts without overflowing.
    #[test]
    fn the_largest_cadence_does_not_overflow() {
        let mut c = every(u32::MAX);
        assert!(c.poll_now(), "the first turn asks");
        for _ in 0..1_000 {
            assert!(!c.poll_now(), "and then it counts down, u32::MAX and all");
        }
        c.woke();
        assert!(c.poll_now(), "and a wake still resets it");
    }
}
