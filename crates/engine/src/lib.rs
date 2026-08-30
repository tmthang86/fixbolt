//! The engine: TCP on one side, [`nanofix_session`] on the other, and a thread
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

pub mod clock;
pub mod conn;
pub mod dispatch;
pub mod frame;
pub mod ring;
pub mod transport;
pub mod wait;

use std::net::{TcpListener, TcpStream};

pub use nanofix_session::{Application, Config, Role, Session};

use crate::clock::Clock;
use crate::conn::{Connection, Turn};
use crate::dispatch::{ConnId, Dispatch, InlineDispatch};
use crate::transport::{TcpTransport, Transport};
use crate::wait::Waiting;

/// A running engine: the connections it holds, and what drives them.
///
/// Every size is the caller's: `N` the session's field index, `RX` a
/// connection's receive buffer, `TX` its outbound queue.
pub struct Engine<T, R: Role, D, C, W, const N: usize, const RX: usize, const TX: usize> {
    conns: Vec<Connection<T, R, N, RX, TX>>,
    cfg: Config,
    dispatch: D,
    clock: C,
    wait: W,
    /// The next connection id. Only ever counts up, so an id is never reused
    /// and a reply that arrives after a hang-up matches nothing.
    next_id: ConnId,
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

impl<T, R, D, C, W, const N: usize, const RX: usize, const TX: usize>
    Engine<T, R, D, C, W, N, RX, TX>
where
    T: Transport,
    R: Role,
    D: Dispatch,
    C: Clock,
    W: Waiting,
{
    /// An engine with no connections yet.
    ///
    /// `capacity` is reserved once, here, so that adding a connection later
    /// does not allocate on a thread that must not — non-negotiable 1.
    pub fn new(cfg: Config, dispatch: D, clock: C, wait: W, capacity: usize) -> Self {
        Self {
            conns: Vec::with_capacity(capacity),
            cfg,
            dispatch,
            clock,
            wait,
            next_id: 0,
        }
    }

    /// Take on a connection that is already open, and tell its session so.
    ///
    /// Returns the id a reply from another thread is routed by.
    pub fn add(&mut self, transport: T) -> ConnId {
        let id = self.next_id;
        self.next_id += 1;
        let mut conn = Connection::new(id, transport, Session::new(self.cfg));
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
            match outcome {
                Turn::Up(m) => {
                    moved |= m;
                    i += 1;
                }
                Turn::Gone => {
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

    /// One idle turn, by the chosen [`Waiting`] strategy.
    pub fn idle(&mut self) {
        self.wait.idle();
    }

    /// Turn forever, idling by the chosen [`Waiting`] strategy.
    ///
    /// The default strategy is [`wait::Spin`], which is D8: no `epoll_wait`, no
    /// futex, no blocking read.
    pub fn run(&mut self) -> ! {
        loop {
            if !self.turn() {
                self.wait.idle();
            }
        }
    }
}

/// An acceptor with the usual sizes, spinning, with the application inline.
/// The shape a deployment runs.
pub type TcpAcceptorEngine<A> = Engine<
    TcpTransport,
    nanofix_session::Acceptor,
    InlineDispatch<A>,
    crate::clock::SystemClock,
    crate::wait::Spin,
    256,
    4096,
    8192,
>;

/// Accept FIX connections on `addr` and never return.
///
/// This is the loop `DESIGN.md` D8 describes, written once so a caller does not
/// have to: accept what is waiting, turn every connection, and spin when there
/// is nothing to do.
///
/// # Errors
///
/// Whatever binding the listener returns.
pub fn serve<A: Application>(
    addr: &str,
    cfg: Config,
    app: A,
    capacity: usize,
) -> std::io::Result<core::convert::Infallible> {
    let acceptor = Acceptor::bind(addr)?;
    let mut engine: TcpAcceptorEngine<A> = Engine::new(
        cfg,
        InlineDispatch::new(app),
        crate::clock::SystemClock,
        crate::wait::Spin,
        capacity,
    );
    loop {
        let mut moved = false;
        while let Some(t) = acceptor.accept() {
            engine.add(t);
            moved = true;
        }
        moved |= engine.turn();
        if !moved {
            engine.idle();
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
