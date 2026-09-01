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
use crate::presession::{Pending, identity_of};
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
pub trait Shardable: Send {
    /// Take ownership of a connection this shard has been given, with the
    /// bytes the pre-session stage already read off it.
    ///
    /// `false` if those bytes do not fit the engine's receive buffer, in which
    /// case the connection is dropped rather than served with part of its first
    /// message missing. A caller keeps `PRE <= RX` and this never happens.
    fn add(&mut self, transport: TcpTransport, prefix: &[u8]) -> bool;
    /// One non-blocking pass. `true` if anything moved.
    fn turn(&mut self) -> bool;
    /// Nothing moved. Whatever this shard's mode does about that.
    fn idle(&mut self);
}

impl<R, D, C, W, J, const N: usize, const RX: usize, const TX: usize> Shardable
    for crate::Engine<TcpTransport, R, D, C, W, J, N, RX, TX>
where
    Self: Send,
    TcpTransport: Transport,
    R: Role,
    D: Dispatch,
    C: Clock,
    W: Waiting,
    // `Engine::add` builds a journal for the new connection, so this runtime
    // can only carry engines whose journal has a default. `add_with_journal`
    // is the escape hatch and it needs a journal per connection, which is not
    // something an accept loop can supply.
    J: SessionJournal + Default,
{
    fn add(&mut self, transport: TcpTransport, prefix: &[u8]) -> bool {
        crate::Engine::add_with_prefix(self, transport, prefix).is_ok()
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
/// [`Assign`] cannot fix this. It is asked at accept time and the `Logon` has
/// not arrived, so nothing at that moment knows which identity the socket
/// carries. **Until that is decided** (`STATUS.md` open item 24), this runtime is
/// sound only where each shard serves an identity of its own — which the API
/// above cannot yet arrange.
///
/// **Dropping this shuts them down.** Each thread's loop ends when its channel
/// disconnects, which happens when the last [`Shards`] holding the sender is
/// dropped; the engine goes with it, and so do the connections it owned. That
/// is process shutdown, and it is the only shutdown this offers.
pub struct Shards<const PRE: usize = 4096> {
    senders: Vec<Sender<Pending<TcpTransport, PRE>>>,
    cores: Vec<CoreId>,
    route: Box<dyn Route>,
    threads: Vec<JoinHandle<()>>,
}

impl<const PRE: usize> Shards<PRE> {
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
        E: Shardable + 'static,
        F: Fn(usize) -> E + Send + Sync + 'static,
    {
        // ADR-0015 decision 6: before a single thread exists.
        plan.validate()?;

        let make = Arc::new(make);
        let gate = Arc::new(AtomicU8::new(WAIT));
        let (status_tx, status_rx) = mpsc::channel::<Result<CoreId, AffinityError>>();

        let mut senders = Vec::with_capacity(plan.shards().len());
        let mut threads = Vec::with_capacity(plan.shards().len());

        for (i, core) in plan.shards().iter().copied().enumerate() {
            let (tx, rx) = mpsc::channel::<Pending<TcpTransport, PRE>>();
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
                    loop {
                        let mut moved = false;
                        loop {
                            match rx.try_recv() {
                                Ok(p) => {
                                    // The array moves; nothing is allocated to
                                    // carry a connection across the channel.
                                    let (t, buf, len) = p.into_parts();
                                    let _ = engine.add(t, buf.get(..len).unwrap_or(&[]));
                                    moved = true;
                                }
                                Err(TryRecvError::Empty) => break,
                                // The runtime was dropped. Shutdown.
                                Err(TryRecvError::Disconnected) => return,
                            }
                        }
                        moved |= engine.turn();
                        if !moved {
                            engine.idle();
                        }
                    }
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
    pub fn hand(&mut self, pending: Pending<TcpTransport, PRE>) -> Result<usize, ShardError> {
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
            .send(pending)
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
/// [`Limits`] has no defaults ([ADR-0020] decision 4). A connection that opens
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
/// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
#[cfg(feature = "standard")]
pub fn serve_sharded_hft<A, F>(
    addr: &str,
    cfg: fixbolt_session::Config,
    plan: &ShardPlan,
    capacity: usize,
    limits: crate::presession::Limits,
    make_app: F,
) -> Result<core::convert::Infallible, ShardError>
where
    A: fixbolt_session::Application + Send + 'static,
    F: Fn(usize) -> A + Send + Sync + 'static,
{
    use crate::clock::{Clock, SystemClock};
    use crate::presession::PendingSet;
    use crate::transport::Interest;

    // The pre-session buffer matches `TcpAcceptorEngine`'s RX, so a prefix can
    // never be too long for the connection it is handed to.
    const PRE: usize = 4096;

    let acceptor = crate::Acceptor::bind(addr).map_err(ShardError::Io)?;
    let mut shards = Shards::<PRE>::start(plan, move |i| -> crate::HftAcceptorEngine<A> {
        crate::Engine::new(
            cfg,
            crate::dispatch::InlineDispatch::new(make_app(i)),
            SystemClock,
            crate::wait::Spin,
            capacity,
        )
    })?;

    let mut set: PendingSet<crate::transport::TcpTransport, PRE> = PendingSet::new(limits);
    let mut poller = crate::poll::Poller::with_capacity(limits.pending() + 1);
    let mut interests: Vec<Interest> = Vec::with_capacity(limits.pending() + 1);
    let mut clock = SystemClock;

    loop {
        // Take on whatever is waiting. `admit` refuses when full, and the
        // refusal closes the socket rather than queueing it.
        while set.len() < limits.pending() {
            let Some(t) = acceptor.accept() else { break };
            // Dropping the refusal closes the socket, which is what a caller
            // with nowhere to put a connection should do.
            drop(set.admit(t, clock.now_ms()));
        }

        let now = clock.now_ms();
        set.turn(now);
        while let Some(i) = set.settled() {
            let Some(p) = set.take(i) else { break };
            match shards.hand(p) {
                Ok(_) => {}
                // A `Logon` that named nobody, or a route that named a shard
                // that does not exist: the connection is dropped. A dead shard
                // thread is different — nothing here can recover from it.
                Err(ShardError::ThreadGone(n)) => return Err(ShardError::ThreadGone(n)),
                Err(_) => {}
            }
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
        // Whatever it says, the loop above re-reads every socket anyway; a
        // failed wait costs one extra pass, and a poller that refused to
        // continue would be a hung acceptor.
        let _ = poller.wait(&interests, timeout);
    }
}
