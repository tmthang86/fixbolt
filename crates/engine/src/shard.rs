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
    /// Take ownership of a connection this shard has been given.
    fn add(&mut self, transport: TcpTransport);
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
    fn add(&mut self, transport: TcpTransport) {
        let _ = crate::Engine::add(self, transport);
    }
    fn turn(&mut self) -> bool {
        crate::Engine::turn(self)
    }
    fn idle(&mut self) {
        crate::Engine::idle(self);
    }
}

/// Which shard takes the next connection.
///
/// [ADR-0015] decision 7: this belongs to the caller. Real deployments shard by
/// counterparty, and the engine does not know which counterparty matters — nor,
/// at accept time, which counterparty this even is. That is the honest limit of
/// what this trait can be given: the `Logon` has not arrived yet.
///
/// [ADR-0015]: ../../../docs/decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md
pub trait Assign: Send {
    /// A shard index in `0..shards`. Out of range is refused, not clamped.
    fn shard_for(&mut self, shards: usize) -> usize;
}

/// Even spread, in accept order. The default, and rarely the right answer past
/// the point where sessions stop being interchangeable.
#[derive(Debug, Default, Clone, Copy)]
pub struct RoundRobin {
    next: usize,
}

impl Assign for RoundRobin {
    fn shard_for(&mut self, shards: usize) -> usize {
        if shards == 0 {
            return 0;
        }
        let i = self.next % shards;
        self.next = self.next.wrapping_add(1);
        i
    }
}

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
    /// An [`Assign`] returned an index outside `0..shards`.
    ///
    /// Refused rather than taken modulo: silently rewriting a caller's answer
    /// hides the bug and puts the connection somewhere nobody asked for.
    BadAssignment { shard: usize, of: usize },
}

impl core::fmt::Display for ShardError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Affinity(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::ThreadGone(i) => write!(f, "shard {i} is no longer running"),
            Self::BadAssignment { shard, of } => {
                write!(f, "assignment chose shard {shard} of {of}")
            }
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
pub struct Shards {
    senders: Vec<Sender<TcpTransport>>,
    cores: Vec<CoreId>,
    assign: Box<dyn Assign>,
    threads: Vec<JoinHandle<()>>,
}

impl Shards {
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
            let (tx, rx) = mpsc::channel::<TcpTransport>();
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
                                Ok(t) => {
                                    engine.add(t);
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
            assign: Box::new(RoundRobin::default()),
            threads,
        })
    }

    /// Replace the assignment policy. Round-robin until told otherwise.
    #[must_use]
    pub fn with_assign(mut self, assign: Box<dyn Assign>) -> Self {
        self.assign = assign;
        self
    }

    /// Give a connection to whichever shard the policy names.
    ///
    /// # Errors
    ///
    /// [`ShardError::BadAssignment`] if the policy names a shard that does not
    /// exist, [`ShardError::ThreadGone`] if that shard's thread has ended.
    pub fn hand(&mut self, transport: TcpTransport) -> Result<usize, ShardError> {
        let of = self.senders.len();
        let shard = self.assign.shard_for(of);
        let sender = self
            .senders
            .get(shard)
            .ok_or(ShardError::BadAssignment { shard, of })?;
        sender
            .send(transport)
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

/// Accept on `addr` and serve it from one pinned engine per core. **`hft` mode:
/// every shard spins and burns its core for as long as the process lives.**
///
/// `plan` is checked before a thread exists, every thread confirms its own pin
/// before any of them serves, and the acceptor runs on this thread — blocking,
/// because it is not an engine thread.
///
/// `make_app` runs on the shard's own thread, once, after that thread is pinned.
/// Each shard gets its own application: they are on different threads and share
/// nothing, which is the point.
///
/// **Read [`Shards`]'s limit before using this.** Every shard here is built from
/// the same `cfg`, so every shard serves the same identity — which is exactly
/// the arrangement in which the single-logon rule stops working. This function
/// is honest for one shard and is a known defect for more than one.
///
/// # Errors
///
/// [`ShardError::Affinity`] if the plan is refused or a pin fails,
/// [`ShardError::Io`] from binding or accepting, [`ShardError::ThreadGone`] if a
/// shard dies under it.
pub fn serve_sharded_hft<A, F>(
    addr: &str,
    cfg: fixbolt_session::Config,
    plan: &ShardPlan,
    capacity: usize,
    make_app: F,
) -> Result<core::convert::Infallible, ShardError>
where
    A: fixbolt_session::Application + Send + 'static,
    F: Fn(usize) -> A + Send + Sync + 'static,
{
    let acceptor = crate::Acceptor::bind_blocking(addr)?;
    let mut shards = Shards::start(plan, move |i| -> crate::HftAcceptorEngine<A> {
        crate::Engine::new(
            cfg,
            crate::dispatch::InlineDispatch::new(make_app(i)),
            crate::clock::SystemClock,
            crate::wait::Spin,
            capacity,
        )
    })?;
    loop {
        let transport = acceptor.accept_blocking()?;
        shards.hand(transport)?;
    }
}
