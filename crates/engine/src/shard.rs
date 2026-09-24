//! Many engines, one per pinned core.
//!
//! Step 4 of [threads-and-affinity]. [ADR-0012] chose one session per polling
//! thread and [ADR-0015] decided how a thread gets a core; this is the runtime
//! that puts the two together, and `GUIDE.md` §1a is what it replaces — that
//! section had to tell a reader to build all of this themselves.
//!
//! # The shape
//!
//! One thread per shard. Each pins itself as its **first act**, confirms the
//! pin, and only then builds its engine — so a connection's buffers are
//! allocated by the thread that will touch them, on the core they will be
//! touched from. A separate acceptor thread takes connections and hands each to
//! one shard over a channel.
//!
//! # Why the acceptor thread is allowed to block and the shard threads are not
//!
//! Accepting is not the hot path and an acceptor that spins burns a core to
//! wait for something that happens once per session. It uses a **blocking**
//! `accept`. The shard threads are engine threads, so `CLAUDE.md` §2
//! non-negotiable 4 applies to them and nothing here may put them to sleep.
//!
//! That is why the channel is `std::sync::mpsc` drained with `try_recv`, and
//! why that choice was measured rather than assumed: `[measured 2026-08-31]`
//! two million `try_recv` calls make **no syscall at all** —
//! `reference/measured-costs.md`. It is also why the startup gate below is a
//! spin rather than a park: "only at startup" is not a distinction
//! `scripts/check-no-kernel-sleep.sh` can make, and a `futex` in the trace is a
//! failure whatever caused it.
//!
//! [threads-and-affinity]: ../../../docs/plans/2026-08-30-threads-and-affinity.md
//! [ADR-0012]: ../../../docs/decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md
//! [ADR-0015]: ../../../docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::thread::JoinHandle;

use crate::affinity::{self, AffinityError, CoreId, ShardPlan};
use crate::clock::Clock;
use crate::dispatch::Dispatch;
// Used only by `serve_sharded_hft_with`, which is `#[cfg(feature = "standard")]`.
#[cfg(feature = "standard")]
use crate::msglog::MaybeLog;
use crate::presession::{Pending, identity_of};
use crate::recovery::Start;
use crate::transport::{TcpTransport, Transport};
use crate::wait::Waiting;
use fixbolt_session::Role;
use fixbolt_session::journal::Journal as SessionJournal;

/// What the shard runtime needs from an engine, and nothing else.
///
/// A trait rather than the concrete [`Engine`](crate::Engine) type so that
/// [`Shards`] carries none of its nine type parameters, and so that a test can
/// hand it something that is not an engine at all — which is how the assignment
/// policy is tested without a socket in sight.
pub trait Shardable<J = crate::journal::Store>: Send {
    /// Take ownership of a connection this shard has been given, with the bytes
    /// the pre-session stage already read off it and the journal its session
    /// starts from.
    ///
    /// `cfg` is the configuration the pre-session stage's registry chose for
    /// this counterparty — [ADR-0030]. It is **not** the engine's own: one shard
    /// engine holds as many counterparties as reach it.
    ///
    /// `start` is what the **acceptor** thread decided: a journal for a fresh
    /// session, or a [`Resumed`](crate::recovery::Resumed) one with the numbers
    /// to continue at. Deciding it there and carrying it here is the whole of
    /// [ADR-0088] — this thread is an engine thread and `CLAUDE.md` §2
    /// non-negotiable 4 forbids it to read a file.
    ///
    /// `false` if those bytes do not fit the engine's receive buffer, in which
    /// case the connection is dropped rather than served with part of its first
    /// message missing — its journal retired first, not joined (ADR-0154
    /// decision 5). **For an engine this cannot happen**: [`Shards::start`]
    /// refuses to compile with a `PRE` larger than [`Self::RX`].
    ///
    /// [ADR-0030]: ../../../docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md
    /// [ADR-0088]: ../../../docs/decisions/ADR-0088-recovery-reaches-the-sharded-runtime-and-the-journal-crosses-the-channel-with-the-connection.md
    fn add_started(
        &mut self,
        transport: TcpTransport,
        cfg: fixbolt_session::Config,
        prefix: &[u8],
        start: Start<J>,
    ) -> bool;

    /// The most bytes [`Self::add_started`] can take as a prefix — an
    /// engine's `RX`.
    ///
    /// `[2026-09-24]` **`PRE <= RX` is a compile-time assertion now, not a
    /// promise.** It was a sentence on `add_started` that nothing held: a
    /// `Shards::<PRE>` whose engines had a smaller `RX` compiled, and dropped
    /// every connection whose `Logon` arrived longer than `RX`. [`Shards::start`]
    /// asserts it with this constant. The default is unbounded, for a
    /// `Shardable` that is not an engine and has no buffer to overflow.
    /// ADR-0154 decision 5.
    const RX: usize = usize::MAX;

    /// [`add_started`](Self::add_started) for a session with nothing to
    /// continue.
    ///
    /// The `J: Default` bound is **on this method rather than on the trait**,
    /// which is where ADR-0039 decision 2 says such a bound belongs: on the
    /// callers that want it, so a journal with no honest `Default` — a
    /// `FileJournal` needs a path — does not lose the rest of the trait.
    fn add(&mut self, transport: TcpTransport, cfg: fixbolt_session::Config, prefix: &[u8]) -> bool
    where
        J: Default,
    {
        self.add_started(transport, cfg, prefix, Start::Fresh(J::default()))
    }
    /// One non-blocking pass. `true` if anything moved.
    fn turn(&mut self) -> bool;
    /// Nothing moved. Whatever this shard's mode does about that.
    fn idle(&mut self);
}

impl<R, D, C, W, J, const N: usize, const RX: usize, const TX: usize, L, const APP: usize>
    Shardable<J> for crate::Engine<TcpTransport, R, D, C, W, J, N, RX, TX, L, APP>
where
    Self: Send,
    TcpTransport: Transport,
    R: Role,
    D: Dispatch,
    C: Clock,
    W: Waiting,
    // **No `Default` here since ADR-0088.** It used to sit on this header
    // because `Engine::add` built the new connection's journal itself, which
    // shut every journal without an honest `Default` — a `FileJournal` needs a
    // path — out of the whole sharded runtime. The journal now arrives with the
    // connection, so the bound is on `Shardable::add` alone.
    J: SessionJournal,
    L: crate::msglog::MessageLog,
{
    const RX: usize = RX;

    fn add_started(
        &mut self,
        transport: TcpTransport,
        cfg: fixbolt_session::Config,
        prefix: &[u8],
        start: Start<J>,
    ) -> bool {
        crate::Engine::add_with_prefix_config_and_start(self, transport, cfg, prefix, start).is_ok()
    }
    fn turn(&mut self) -> bool {
        crate::Engine::turn(self)
    }
    fn idle(&mut self) {
        crate::Engine::idle(self);
    }
}

pub use crate::presession::{HashRoute, Route};

/// Why a shard runtime would not start, or would not take a connection.
///
/// Not `Box<dyn Error>`: `CLAUDE.md` §6 forbids that in a public API.
#[derive(Debug)]
#[non_exhaustive]
pub enum ShardError {
    /// The plan named a core this machine cannot honour, or the pin failed.
    Affinity(AffinityError),
    /// Binding, accepting, or spawning a thread.
    Io(std::io::Error),
    /// A shard thread is gone. Its engine, and every connection it owned, went
    /// with it.
    ThreadGone(usize),
    /// A [`Route`] returned an index outside `0..shards`.
    ///
    /// Refused rather than taken modulo: silently rewriting a caller's answer
    /// hides the bug and puts the connection somewhere nobody asked for — and
    /// somewhere is exactly where the single-logon rule breaks again.
    BadRoute { shard: usize, of: usize },
    /// The first message named no identity, so there is nothing to route by.
    NoIdentity,
    /// The registry serves no counterparty, so every shard would refuse every
    /// connection for as long as the process lived.
    ///
    /// The same refusal [`crate::ServeError::NoCounterparties`] makes, for the
    /// same reason: an empty registry is a valid one
    /// ([ADR-0026](../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)
    /// decision 6) and a *serving loop* built on one is a configuration mistake.
    NoCounterparties,
}

impl core::fmt::Display for ShardError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Affinity(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::ThreadGone(i) => write!(f, "shard {i} is no longer running"),
            Self::BadRoute { shard, of } => {
                write!(f, "route chose shard {shard} of {of}")
            }
            Self::NoIdentity => write!(f, "the first message named no identity"),
            Self::NoCounterparties => write!(
                f,
                "the registry serves no counterparty, so every shard would refuse every connection"
            ),
        }
    }
}

impl std::error::Error for ShardError {}

impl From<AffinityError> for ShardError {
    fn from(e: AffinityError) -> Self {
        Self::Affinity(e)
    }
}

impl From<std::io::Error> for ShardError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

// The startup gate. Every shard pins and reports before any of them runs, so a
// plan that fails on shard 3 does not leave shards 0 to 2 already serving.
const WAIT: u8 = 0;
const GO: u8 = 1;
const ABORT: u8 = 2;

/// A running set of pinned engine threads.
///
/// # A limit this cannot check for you
///
/// **Two connections for the same FIX identity must not land on different
/// shards.** An [`Engine`](crate::Engine) carries one `Config`, so it serves one
/// identity, and it enforces "that identity is already logged on" by looking at
/// the other connections **it** holds. Split those across engines and the rule
/// has nothing to look at: both `Logon`s are accepted.
///
/// `[measured 2026-08-31]` the acceptance corpus scores **59 through one shard
/// and 57 through two**, failing exactly `1b_DuplicateIdentity.def` and
/// `AlreadyLoggedOn.def` — `crates/engine/tests/shard_wire.rs`.
///
/// `Assign` cannot fix this — a code span rather than a link because the type it
/// names no longer exists; the pre-session stage replaced it ([ADR-0020], and
/// `presession.rs` calls it *its predecessor*). It was asked at accept time and the `Logon` has
/// not arrived, so nothing at that moment knows which identity the socket
/// carries. **Until that is decided** (`STATUS.md` open item 24), this runtime is
/// sound only where each shard serves an identity of its own — which the API
/// above cannot yet arrange.
///
/// **Dropping this shuts them down, and returns only once they are down.**
/// Each thread's loop ends when its channel disconnects, which the drop does
/// first; the engine goes with it, and so do the connections it owned; then
/// the thread waits for the journal writers those connections retired
/// (ADR-0153 decision 4). **The drop joins every shard thread**, so when it
/// returns an `Async` journal's writer has written its last byte — or the
/// shard's wait timed out first. That is process shutdown, and it is the only
/// shutdown this offers: no `Logout` is sent (ADR-0088 decision 5). It blocks
/// the dropping thread, which is never a shard thread, for up to one idle
/// wait plus that timeout; a [`Shardable::idle`] that never returns makes it
/// never return. `crates/engine/tests/after_serving.rs` holds the join.
///
/// `J` is the journal its engines hold, and it defaults to
/// [`Store`](crate::journal::Store) so `Shards::<PRE>` keeps meaning what it
/// meant before ADR-0088 gave the channel a journal to carry.
pub struct Shards<const PRE: usize = 4096, J = crate::journal::Store> {
    // **A pair, not a `Pending`, since ADR-0088.** What a shard thread needs to
    // build a session is the connection *and* the journal it starts from, and
    // the journal can only be decided on the acceptor thread — this channel is
    // the one thing that already crosses between the two.
    senders: Vec<Sender<(Pending<TcpTransport, PRE>, Start<J>)>>,
    cores: Vec<CoreId>,
    route: Box<dyn Route>,
    threads: Vec<JoinHandle<()>>,
}

impl<const PRE: usize, J> Shards<PRE, J> {
    /// Validate the plan, start one pinned thread per shard, and wait for every
    /// one of them to confirm its pin before any of them serves.
    ///
    /// `make` runs **on the pinned thread**, after the pin, so whatever it
    /// allocates is allocated by the core that will use it.
    ///
    /// # Errors
    ///
    /// [`ShardError::Affinity`] if the plan is refused or a pin fails — in which
    /// case **no shard is left running**; [`ShardError::Io`] if a thread cannot
    /// be spawned.
    pub fn start<E, F>(plan: &ShardPlan, make: F) -> Result<Self, ShardError>
    where
        E: Shardable<J> + 'static,
        F: Fn(usize) -> E + Send + Sync + 'static,
        // ADR-0088 decision 4: a `Start<J>` rides the channel, so the journal
        // has to be able to cross a thread boundary. `Store` can; a
        // `FileJournal` owns a file handle and can;
        // `tests/shard_recovery.rs::the_start_that_crosses_the_channel_is_send`
        // is where that is a compile-time fact rather than this sentence.
        J: Send + 'static,
    {
        // A pre-session prefix of `PRE` bytes must fit the engine's `RX`, or
        // every `Logon` longer than `RX` is dropped on the shard thread. A
        // compile error at the instantiation, not a comment. ADR-0154
        // decision 5.
        const {
            assert!(
                PRE <= E::RX,
                "Shards::<PRE> hands prefixes of up to PRE bytes to engines whose RX is smaller"
            );
        };
        // ADR-0015 decision 6: before a single thread exists.
        plan.validate()?;

        let make = Arc::new(make);
        let gate = Arc::new(AtomicU8::new(WAIT));
        let (status_tx, status_rx) = mpsc::channel::<Result<CoreId, AffinityError>>();

        let mut senders = Vec::with_capacity(plan.shards().len());
        let mut threads = Vec::with_capacity(plan.shards().len());

        for (i, core) in plan.shards().iter().copied().enumerate() {
            let (tx, rx) = mpsc::channel::<(Pending<TcpTransport, PRE>, Start<J>)>();
            senders.push(tx);

            let make = Arc::clone(&make);
            let gate = Arc::clone(&gate);
            let status = status_tx.clone();

            let handle = std::thread::Builder::new()
                .name(format!("fixbolt-shard-{i}"))
                .spawn(move || {
                    // Decision 2: the pin is this thread's first act, and the
                    // answer comes from the scheduler rather than from the call.
                    let confirmed =
                        affinity::pin_current_thread(core).and_then(|()| affinity::running_on());
                    let pinned = confirmed.is_ok();
                    let _ = status.send(confirmed);
                    if !pinned {
                        return;
                    }

                    // Spin, not park. See the module docs: a blocking call here
                    // is indistinguishable from one on the hot path to anything
                    // that traces this thread.
                    loop {
                        match gate.load(Ordering::Acquire) {
                            GO => break,
                            ABORT => return,
                            _ => std::hint::spin_loop(),
                        }
                    }

                    let mut engine = make(i);
                    'serving: loop {
                        let mut moved = false;
                        loop {
                            match rx.try_recv() {
                                Ok((p, start)) => {
                                    // The array moves; nothing is allocated to
                                    // carry a connection across the channel.
                                    // A `Pending` that reached a shard has
                                    // settled, so the registry has already
                                    // chosen its configuration. `None` here
                                    // cannot happen and drops the socket rather
                                    // than inventing an identity for it.
                                    let Some(cfg) = p.config() else { continue };
                                    let (t, buf, len) = p.into_parts();
                                    // **`add_started`, never `add`.** The
                                    // journal was decided on the acceptor
                                    // thread; this one may not read a file.
                                    let _ = engine.add_started(
                                        t,
                                        cfg,
                                        buf.get(..len).unwrap_or(&[]),
                                        start,
                                    );
                                    moved = true;
                                }
                                Err(TryRecvError::Empty) => break,
                                // The runtime was dropped. Shutdown.
                                Err(TryRecvError::Disconnected) => break 'serving,
                            }
                        }
                        moved |= engine.turn();
                        if !moved {
                            engine.idle();
                        }
                    }
                    // **Teardown, after the loop**: the engine is dropped —
                    // every journal it still holds retired, none waited for —
                    // and only then are their writers awaited. ADR-0153
                    // decision 4. No shutdown grace reaches a shard, so the
                    // floor is the timeout.
                    drop(engine);
                    crate::after_serving(None);
                })?;
            threads.push(handle);
        }
        drop(status_tx);

        let mut cores = Vec::with_capacity(threads.len());
        let mut failure: Option<AffinityError> = None;
        for i in 0..threads.len() {
            match status_rx.recv() {
                Ok(Ok(core)) => cores.push(core),
                Ok(Err(e)) => failure = failure.or(Some(e)),
                // A thread that died before reporting. Nothing here can say
                // more than which one, and saying that is better than a
                // plausible guess at why.
                Err(_) => {
                    gate.store(ABORT, Ordering::Release);
                    for h in threads {
                        drop(h.join());
                    }
                    return Err(ShardError::ThreadGone(i));
                }
            }
        }

        if let Some(e) = failure {
            gate.store(ABORT, Ordering::Release);
            for h in threads {
                drop(h.join());
            }
            return Err(ShardError::Affinity(e));
        }

        gate.store(GO, Ordering::Release);
        cores.sort_unstable();

        Ok(Self {
            senders,
            cores,
            route: Box::new(HashRoute),
            threads,
        })
    }

    /// Replace the routing policy. A stable hash of the identity until told
    /// otherwise — see [`HashRoute`].
    #[must_use]
    pub fn with_route(mut self, route: Box<dyn Route>) -> Self {
        self.route = route;
        self
    }

    /// Give a connection to whichever shard the route names for its identity.
    ///
    /// The identity is read from the bytes the pre-session stage already
    /// collected, so this asks the route the question it can actually answer.
    ///
    /// # Errors
    ///
    /// [`ShardError::NoIdentity`] if the first message named no `49=`/`56=`,
    /// [`ShardError::BadRoute`] if the policy names a shard that does not
    /// exist, [`ShardError::ThreadGone`] if that shard's thread has ended.
    pub fn hand(&mut self, pending: Pending<TcpTransport, PRE>) -> Result<usize, ShardError>
    where
        J: Default,
    {
        self.hand_started(pending, Start::Fresh(J::default()))
    }

    /// [`hand`](Self::hand), carrying the journal the session starts from.
    ///
    /// The journal travels **with** the connection because the two decisions
    /// are on two threads: what a counterparty left behind can only be read
    /// where blocking is allowed, and the session is built where it is not
    /// ([ADR-0088]).
    ///
    /// # Errors
    ///
    /// As [`hand`](Self::hand). A connection refused here takes its journal
    /// with it and both are dropped — which for a journal on disk means a file
    /// opened and closed for nothing, named in ADR-0088's *Consequences*.
    ///
    /// [ADR-0088]: ../../../docs/decisions/ADR-0088-recovery-reaches-the-sharded-runtime-and-the-journal-crosses-the-channel-with-the-connection.md
    pub fn hand_started(
        &mut self,
        pending: Pending<TcpTransport, PRE>,
        start: Start<J>,
    ) -> Result<usize, ShardError> {
        let of = self.senders.len();
        let shard = {
            let id = identity_of(pending.bytes()).ok_or(ShardError::NoIdentity)?;
            self.route.shard_for(id, of)
        };
        let sender = self
            .senders
            .get(shard)
            .ok_or(ShardError::BadRoute { shard, of })?;
        sender
            .send((pending, start))
            .map_err(|_| ShardError::ThreadGone(shard))?;
        Ok(shard)
    }

    /// How many shards are running.
    #[must_use]
    pub fn len(&self) -> usize {
        self.senders.len()
    }

    /// Whether there are none. There never are — [`start`](Self::start) refuses
    /// an empty plan — but clippy asks and the answer is cheap.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.senders.is_empty()
    }

    /// The cores the shard threads **were observed on**, ascending.
    ///
    /// Read back from the scheduler by each thread after it pinned itself, not
    /// copied from the plan. If this does not equal the plan's cores, the plan
    /// is not what is running.
    #[must_use]
    pub fn confirmed_cores(&self) -> &[CoreId] {
        &self.cores
    }

    /// Whether every shard thread is still running.
    #[must_use]
    pub fn all_alive(&self) -> bool {
        self.threads.iter().all(|h| !h.is_finished())
    }
}

/// **Disconnect, then join.** Without the join, the thread that dropped this
/// raced the shard's teardown: a `wait_for_retired_writers` asked right after
/// the drop could read zero **before** the shard had retired anything, and a
/// process exiting there lost what the writer had not reached. `[measured
/// 2026-09-24]` main's CI run 35918095562, 1990 of 2000 records; locally 2 of
/// 100 runs. Trap: `docs/reference/a-drop-that-only-signals-is-not-a-shutdown.md`.
impl<const PRE: usize, J> Drop for Shards<PRE, J> {
    fn drop(&mut self) {
        // The order is the whole fix: a thread joined while its sender is
        // alive never sees the disconnect, and the join never returns.
        self.senders.clear();
        for h in self.threads.drain(..) {
            // A shard that panicked has nothing left to wait for, and a drop
            // may not panic in turn.
            let _ = h.join();
        }
    }
}

/// Accept on `addr` and serve it from one pinned engine per core, routing each
/// connection by the identity in its `Logon`. **`hft` mode: every shard spins
/// and burns its core for as long as the process lives.**
///
/// `plan` is checked before a thread exists, every thread confirms its own pin
/// before any of them serves, and the pre-session stage runs on **this** thread
/// — which blocks, because it is not an engine thread.
///
/// `make_app` runs on the shard's own thread, once, after that thread is pinned.
/// Each shard gets its own application: they are on different threads and share
/// nothing, which is the point.
///
/// # The two limits are yours to choose
///
/// [`crate::presession::Limits`] has no defaults ([ADR-0020] decision 4). A connection that opens
/// and never sends a `Logon` costs a slot until its deadline, and a table with
/// no ceiling costs memory without one — so the deadline and the ceiling are
/// arguments, and there is no value here that somebody who has not seen your
/// deployment picked for you.
///
/// # How it waits
///
/// Not in `accept`. A thread parked there cannot expire a silent connection, so
/// a logon deadline would fire only when somebody else happened to connect —
/// load-dependent behaviour, and the wrong kind. It waits on the listener **and
/// every pending socket**, for exactly as long as the soonest deadline allows.
///
/// # Errors
///
/// [`ShardError::Affinity`] if the plan is refused or a pin fails,
/// [`ShardError::Io`] from binding, [`ShardError::ThreadGone`] if a shard dies
/// under it.
///
/// The plan is refused **before** the address is bound, so an `Io` from binding
/// is a real binding error and never a plan refusal hidden behind a held port
/// ([ADR-0064]).
///
/// [ADR-0064]: ../../../docs/decisions/ADR-0064-a-door-acquires-nothing-before-it-has-validated.md
/// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
#[cfg(feature = "standard")]
pub fn serve_sharded_hft<A, F>(
    addr: &str,
    table: crate::presession::Table,
    plan: &ShardPlan,
    capacity: usize,
    limits: crate::presession::Limits,
    make_app: F,
    log_path: Option<&std::path::Path>,
) -> Result<core::convert::Infallible, ShardError>
where
    A: fixbolt_session::Application + Send + 'static,
    F: Fn(usize) -> A + Send + Sync + 'static,
{
    serve_sharded_hft_with::<256, 4096, 8192, 1024, A, F>(
        addr, table, plan, capacity, limits, make_app, log_path,
    )
}

/// The same, with the three buffer sizes named by the caller. See
/// [`crate::serve_with`] for what `N`, `RX` and `TX` mean and what they cost.
///
/// `RX` sizes **both** the pre-session buffer here and each shard engine's
/// receive buffer, which is the invariant the two used to state in a comment
/// apiece — see the note on [`Shards`].
///
/// # Errors
///
/// As [`serve_sharded_hft`].
#[cfg(feature = "standard")]
pub fn serve_sharded_hft_with<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A,
    F,
>(
    addr: &str,
    table: crate::presession::Table,
    plan: &ShardPlan,
    capacity: usize,
    limits: crate::presession::Limits,
    make_app: F,
    log_path: Option<&std::path::Path>,
) -> Result<core::convert::Infallible, ShardError>
where
    A: fixbolt_session::Application + Send + 'static,
    F: Fn(usize) -> A + Send + Sync + 'static,
{
    // **One loop, not two.** ADR-0088 decision 3 gave the sharded doors the
    // shape ADR-0034 decision 3 gave `serve`/`serve_with_recovery`: the fresh
    // path *is* the resumed path with [`NoRecovery`](crate::recovery::NoRecovery)
    // for an answer, so `serve_sharded_hft_serves_a_session` proves the loop
    // that `serve_sharded_hft_with_recovery` runs.
    serve_sharded_hft_with_recovery_with::<
        N,
        RX,
        TX,
        APP,
        A,
        crate::journal::Store,
        crate::recovery::NoRecovery,
        F,
    >(
        addr,
        table,
        plan,
        capacity,
        limits,
        make_app,
        crate::recovery::NoRecovery,
        log_path,
    )
}

/// [`serve_sharded_hft`], asking `recovery` what each counterparty left behind.
///
/// **The sharded half of `STATUS.md` item 32 (a).** `crate::serve_with_recovery`
/// and [`crate::serve_hft_with_recovery`] let a single-engine deployment resume
/// its sequence numbers across a restart; this is the same seam for a deployment
/// that runs one pinned engine per core.
///
/// `recovery` is asked **on this thread**, once per connection, the moment the
/// pre-session stage has named the counterparty — so an implementation may read
/// a file ([ADR-0020]). What it answers crosses the channel to the shard thread
/// with the connection, because that thread is an engine thread and `CLAUDE.md`
/// §2 non-negotiable 4 forbids it to block. [ADR-0088].
///
/// A slow `Recovery` delays every pre-session connection behind it, and a
/// journal is built before [`Shards::hand_started`] can refuse a connection —
/// both are named in ADR-0088's *Consequences* and neither is bounded here.
///
/// # Errors
///
/// As [`serve_sharded_hft`].
///
/// [ADR-0088]: ../../../docs/decisions/ADR-0088-recovery-reaches-the-sharded-runtime-and-the-journal-crosses-the-channel-with-the-connection.md
/// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
// **Eight, and clippy's ceiling is seven.** The same deliberate parameter
// ADR-0054 took for `serve_with_recovery` and the three doors beside it: a
// `Serve` builder is the recorded alternative, deferred until an eleventh
// parameter is wanted.
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "standard")]
pub fn serve_sharded_hft_with_recovery<A, J, V, F>(
    addr: &str,
    table: crate::presession::Table,
    plan: &ShardPlan,
    capacity: usize,
    limits: crate::presession::Limits,
    make_app: F,
    recovery: V,
    log_path: Option<&std::path::Path>,
) -> Result<core::convert::Infallible, ShardError>
where
    A: fixbolt_session::Application + Send + 'static,
    F: Fn(usize) -> A + Send + Sync + 'static,
    J: SessionJournal + Send + 'static,
    V: crate::recovery::Recovery<J>,
{
    serve_sharded_hft_with_recovery_with::<256, 4096, 8192, 1024, A, J, V, F>(
        addr, table, plan, capacity, limits, make_app, recovery, log_path,
    )
}

/// The same, with the three buffer sizes named by the caller. See
/// [`crate::serve_with`] for what `N`, `RX` and `TX` mean and what they cost.
///
/// **This is the only sharded serving loop.** [`serve_sharded_hft_with`] is one
/// call through [`NoRecovery`](crate::recovery::NoRecovery) into it.
///
/// # Errors
///
/// As [`serve_sharded_hft`].
// Eight, as above.
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "standard")]
pub fn serve_sharded_hft_with_recovery_with<
    const N: usize,
    const RX: usize,
    const TX: usize,
    const APP: usize,
    A,
    J,
    V,
    F,
>(
    addr: &str,
    table: crate::presession::Table,
    plan: &ShardPlan,
    capacity: usize,
    limits: crate::presession::Limits,
    make_app: F,
    mut recovery: V,
    log_path: Option<&std::path::Path>,
) -> Result<core::convert::Infallible, ShardError>
where
    A: fixbolt_session::Application + Send + 'static,
    F: Fn(usize) -> A + Send + Sync + 'static,
    J: SessionJournal + Send + 'static,
    V: crate::recovery::Recovery<J>,
{
    use crate::clock::{Clock, SystemClock};
    use crate::presession::PendingSet;
    use crate::transport::Interest;

    // The engine's own `Config` is the default for `Engine::add`, which this
    // loop never calls: every connection arrives from the pre-session stage
    // carrying the one the registry chose ([ADR-0030]). An empty table has none,
    // and an acceptor that would refuse every connection forever is refused here
    // instead.
    //
    // [ADR-0030]: ../../../docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md
    let cfg = table
        .first()
        .map(crate::presession::Entry::config)
        .ok_or(ShardError::NoCounterparties)?;

    // ADR-0064: the plan is checked against this machine **before** anything is
    // acquired. Bound first, a held port and a bad core in the same call would
    // answer `Io(AddrInUse)` and hide the refusal an operator has to act on —
    // `tests/shard_hft.rs::serve_sharded_hft_refuses_the_plan_before_it_binds`.
    // `Shards::start` below validates again on its own: it is public by itself,
    // and a second read of `/sys` at startup is not on any hot path.
    plan.validate()?;

    let acceptor = crate::Acceptor::bind(addr).map_err(ShardError::Io)?;

    // **One file per shard, opened here rather than on the shard thread.**
    // Every engine numbers its own connections from zero, so N shards sharing
    // one path would write `conn=0` for N different sockets and interleave N
    // writer threads into one descriptor. `path.<i>` keeps them apart, and
    // opening up front means a bad path is a startup error with a name rather
    // than a shard thread that quietly has no log.
    //
    // Opened **once**: reopening on the shard thread would mark a torn tail
    // twice for a file that was killed mid-write. The `Mutex<Vec<Option<_>>>`
    // exists only so the `Fn` closure can take the `i`th one — it is touched
    // once per shard at startup and never again, and no engine thread ever
    // waits on it.
    let mut opened: Vec<Option<crate::msglog::FileLog>> = Vec::with_capacity(plan.shards().len());
    if let Some(base) = log_path {
        for i in 0..plan.shards().len() {
            opened.push(Some(
                crate::msglog::FileLog::open(&crate::msglog::shard_path(base, i))
                    .map_err(ShardError::Io)?,
            ));
        }
    } else {
        opened.resize_with(plan.shards().len(), || None);
    }
    let logs = std::sync::Mutex::new(opened);

    let mut shards = Shards::<RX, J>::start(
        plan,
        // `HftAcceptorEngine` with `J` in place of its `Store`: same shape,
        // same `Spin`, and the journal the caller's `Recovery` answers with.
        move |i| -> crate::TcpAcceptorEngine<A, crate::wait::Spin, J, MaybeLog, N, RX, TX, APP> {
            let taken = logs
                .lock()
                .ok()
                .and_then(|mut v| v.get_mut(i).and_then(Option::take));
            // **Annotated before `with_log`, not after.** `Engine::new` builds
            // its own `L` from `Default`, so naming only the closure's return
            // type leaves `new`'s parameter ambiguous — and it is ambiguous
            // only on the path this `#[cfg]` compiles, which is why it reached
            // CI rather than a local build. `[measured 2026-09-04]` run
            // 33859821622, the `affinity` job, E0283.
            let bare: crate::TcpAcceptorEngine<
                A,
                crate::wait::Spin,
                J,
                crate::msglog::NoLog,
                N,
                RX,
                TX,
                APP,
            > = crate::Engine::new(
                cfg,
                crate::dispatch::InlineDispatch::new(make_app(i)),
                SystemClock,
                crate::wait::Spin,
                capacity,
            );
            bare.with_shard(u16::try_from(i).unwrap_or(u16::MAX))
                .with_log(MaybeLog(taken))
        },
    )?;

    let mut set: PendingSet<crate::transport::TcpTransport, crate::presession::Table, RX> =
        PendingSet::new(limits, table);
    let mut poller = crate::poll::Poller::with_capacity(limits.pending() + 1);
    let mut interests: Vec<Interest> = Vec::with_capacity(limits.pending() + 1);
    let mut clock = SystemClock;
    // Settled connections whose recovery said "not yet" — ADR-0154 decision 3,
    // the same rule `serve*` follows, for one behaviour everywhere. This thread
    // may block, but a wait here would stall every other counterparty's
    // `Logon` behind one busy journal. They hold pre-session slots, which keeps
    // `park` within the room taken here.
    let mut parked: crate::recovery::Parking<Pending<TcpTransport, RX>> =
        crate::recovery::Parking::with_capacity(limits.pending());

    loop {
        // Take on whatever is waiting. `admit` refuses when full, and the
        // refusal closes the socket rather than queueing it.
        while set.len() + parked.len() < limits.pending() {
            let Some(t) = acceptor.accept() else { break };
            // Dropping the refusal closes the socket, which is what a caller
            // with nowhere to put a connection should do.
            drop(set.admit(t, clock.now_ms()));
        }

        let now = clock.now_ms();
        set.turn(now);
        while let Some(i) = set.settled() {
            let Some(p) = set.take(i) else { break };
            // A `Pending` that settled has a configuration; `None` cannot
            // happen and drops the socket rather than inventing an identity
            // for it.
            let Some(cfg) = p.config() else { continue };
            // Asked before `recover`; "not yet" parks it (ADR-0154).
            if !recovery.ready(&cfg) {
                let limit = crate::recovery::Parking::<()>::limit_ms(&cfg, limits.logon_ms());
                drop(parked.park(p, cfg, now, limit));
                continue;
            }
            start_and_hand(&mut shards, &mut recovery, p, cfg)?;
        }
        let _ = parked.expire(now);
        while let Some((p, cfg)) = parked.next_ready(now, &mut recovery) {
            start_and_hand(&mut shards, &mut recovery, p, cfg)?;
        }

        // Wait until something happens or the soonest deadline arrives —
        // derived, so there is no polling interval anybody had to choose.
        interests.clear();
        if let Some(s) = acceptor.source() {
            interests.push(Interest::readable(s));
        }
        set.interests(&mut interests);
        let timeout = set.earliest_deadline().map_or(1_000, |d| {
            i32::try_from(d.saturating_sub(now)).unwrap_or(i32::MAX)
        });
        // A parked connection is asked again on the next millisecond, so the
        // wait is no longer than that while one is parked.
        let timeout = if parked.len() == 0 {
            timeout
        } else {
            timeout.min(1)
        };
        // Whatever it says, the loop above re-reads every socket anyway; a
        // failed wait costs one extra pass, and a poller that refused to
        // continue would be a hung acceptor.
        let _ = poller.wait(&interests, timeout);
    }
}

/// Ask `recovery` what the counterparty left behind and hand the connection to
/// its shard. The acceptor thread's one door, for a connection that settled
/// this turn and for one that was parked.
///
/// **The one place recovery is asked**, and it is this thread — the
/// acceptor's, which ADR-0020 allows to block. The identity is known now and
/// was not a moment ago.
///
/// # Errors
///
/// [`ShardError::ThreadGone`] only: a `Logon` that named nobody, or a route
/// that named a shard that does not exist, drops the connection. A dead shard
/// thread is different — nothing here can recover from it.
#[cfg(feature = "standard")]
fn start_and_hand<const PRE: usize, J, V: crate::recovery::Recovery<J>>(
    shards: &mut Shards<PRE, J>,
    recovery: &mut V,
    p: Pending<TcpTransport, PRE>,
    cfg: fixbolt_session::Config,
) -> Result<(), ShardError> {
    let start = match recovery.recover(&cfg) {
        Some(resumed) => Start::Resumed(resumed),
        None => Start::Fresh(recovery.fresh(&cfg)),
    };
    match shards.hand_started(p, start) {
        Err(ShardError::ThreadGone(n)) => Err(ShardError::ThreadGone(n)),
        Ok(_) | Err(_) => Ok(()),
    }
}
