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
pub mod backpressure;
pub mod clock;
pub mod conn;
pub mod dispatch;
pub mod frame;
pub mod journal;
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
        let mut moved = false;
        let mut i = 0;
        while i < self.conns.len() {
            // **One identity, one connection**, and it is asked before the
            // message is judged because `1b_DuplicateIdentity.def` and
            // `AlreadyLoggedOn.def` both expect no reply at all on the second.
            // Counted first so the immutable borrow ends before the mutable
            // one begins.
            let others_on = self
                .conns
                .iter()
                .enumerate()
                .filter(|(j, c)| *j != i && c.session.is_logged_on())
                .count();

            let mut deliver = Deliver {
                dispatch: &mut self.dispatch,
                conn: self.conns[i].id,
            };
            let outcome = self.conns[i].turn(now, &mut deliver, |msg| {
                others_on > 0 && msg_type_is_logon(msg)
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
/// # Errors
///
/// Whatever binding the listener returns.
#[cfg(all(feature = "standard", unix))]
pub fn serve<A: Application>(
    addr: &str,
    cfg: Config,
    app: A,
    capacity: usize,
) -> std::io::Result<core::convert::Infallible> {
    let acceptor = Acceptor::bind(addr)?;
    let engine: StandardAcceptorEngine<A> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        // Sized for the connections, the listener and the waker. An inline
        // dispatch has no waker, so this is one spare.
        crate::block::Block::new(capacity + 2),
        capacity,
    );
    pump(acceptor, engine)
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
/// Whatever binding the listener returns.
pub fn serve_hft<A: Application>(
    addr: &str,
    cfg: Config,
    app: A,
    capacity: usize,
) -> std::io::Result<core::convert::Infallible> {
    let acceptor = Acceptor::bind(addr)?;
    let engine: HftAcceptorEngine<A> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        crate::wait::Spin,
        capacity,
    );
    pump(acceptor, engine)
}

/// The loop both `serve` functions run: accept what is waiting, turn every
/// connection, and idle when nothing moved.
///
/// One function so the two modes differ in **exactly one type** and in nothing
/// else. A loop written twice is two loops that will drift, and the listener
/// being registered in one and not the other is precisely the kind of drift
/// that costs a whole timeout and shows up as nothing at all.
fn pump<A: Application, W: Waiting>(
    acceptor: Acceptor,
    mut engine: TcpAcceptorEngine<A, W>,
) -> std::io::Result<core::convert::Infallible> {
    // The listener, so an idle turn waits on "somebody connected" as well as on
    // "somebody sent something". Leave it out and a new connection waits up to
    // a whole timeout to be accepted.
    let listener = acceptor.source().map(Interest::readable);
    let extra: &[Interest] = listener.as_slice();
    loop {
        let mut moved = false;
        while let Some(t) = acceptor.accept() {
            engine.add(t);
            moved = true;
        }
        moved |= engine.turn();
        if !moved {
            engine.idle_with(extra);
        }
    }
}

/// Is this message a `Logon`?
///
/// Read off the raw bytes rather than parsed: the engine has no dictionary and
/// wants none. `35=` is the third field of a well-formed message and the
/// session refuses anything else, so a scan for it is enough here.
fn msg_type_is_logon(msg: &[u8]) -> bool {
    let mut at = 0;
    while at < msg.len() {
        let Some(end) = msg[at..].iter().position(|b| *b == 1).map(|e| e + at) else {
            return false;
        };
        if msg[at..end] == *b"35=A" {
            return true;
        }
        at = end + 1;
    }
    false
}

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
