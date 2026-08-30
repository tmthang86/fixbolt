//! Bytes in, bytes out, and the three answers a non-blocking socket gives.
//!
//! # Why not `io::Result<usize>`
//!
//! The plan said `io::Result<usize>` with `WouldBlock` reported as `Ok(0)`.
//! Writing it made the problem obvious: on a stream socket `Ok(0)` already
//! means **end of stream**, so that encoding hands the caller one value for two
//! opposite facts. A session dropped because the counterparty was quiet, or a
//! loop spinning forever on a socket that closed — both are one missing
//! distinction.
//!
//! [`Io`] says the three things separately, which is the same answer the codec
//! reached for `Parsed::Incomplete` (`DESIGN.md` D2): *waiting for more* is not
//! an error, and it is not the end.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::rc::Rc;

/// What one read or write did.
///
/// Fieldless but for the byte count and an `ErrorKind`, both `Copy` — nothing
/// here allocates, on any path, including the failure one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Io {
    /// This many bytes moved. Never zero.
    Ready(usize),
    /// Nothing to do right now. Not an error.
    Idle,
    /// The peer hung up.
    Closed,
    /// The OS refused. The kind is kept because it is the only thing that
    /// distinguishes a configuration mistake from a broken wire.
    Failed(io::ErrorKind),
}

/// A handle something can wait on until it is ready. On POSIX, a descriptor.
///
/// `standard` mode blocks until one of these becomes readable rather than
/// asking each of them in turn — ADR-0013 decision 2, ADR-0014 decision 3. It
/// is a distinct type rather than a bare `RawFd` so that a transport which has
/// no descriptor cannot accidentally produce a plausible-looking one.
///
/// **It borrows; it does not own.** A `Source` outliving the socket it came
/// from names a descriptor the kernel has reused, so it is only ever read from
/// a transport the caller still holds. [`crate::Engine`] rebuilds its list from
/// the live connections on every idle turn for exactly that reason.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source(std::os::fd::RawFd);

/// A handle something can wait on. On a target with no poller this carries
/// nothing, and [`Transport::POLLABLE`] is `false` everywhere — ADR-0014
/// decision 2.
#[cfg(not(unix))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source(());

#[cfg(unix)]
impl Source {
    /// Name a descriptor the caller already holds open.
    ///
    /// **This is how a `Transport` outside this crate becomes pollable**, and
    /// it is public for that reason and no other: without it,
    /// [`Transport::POLLABLE`] could only ever be `true` for the transports
    /// shipped here, which would make the trait a trait in name only.
    ///
    /// It is safe, because holding the wrong number is not unsound — `poll`
    /// answers `POLLNVAL` for a descriptor the kernel does not know, and
    /// [`crate::poll::Poller`] turns that into an error rather than into
    /// "nothing has arrived". What it is **not** is harmless:
    ///
    /// > **The descriptor must stay open for as long as this `Source` is
    /// > used.** Descriptor numbers are reused as soon as they are freed, and
    /// > eagerly — the kernel hands out the lowest one available. A `Source`
    /// > that outlives its socket does not go bad in any way a test can see; it
    /// > quietly starts naming **somebody else's** socket, and waits on that.
    ///
    /// `[measured 2026-08-30]` that is not a hypothetical. A test in
    /// `crates/engine/tests/standard.rs` closed a socket and asserted about its
    /// descriptor; it went red once and then passed 30 runs in a row, because
    /// whether it failed depended on whether another test thread had reopened
    /// that number yet. The rule below is what came out of it: rebuild the list
    /// from live transports every turn, and never store one.
    #[must_use]
    pub const fn from_raw_fd(fd: std::os::fd::RawFd) -> Self {
        Self(fd)
    }

    /// The descriptor, for a poller.
    #[must_use]
    pub const fn as_raw_fd(self) -> std::os::fd::RawFd {
        self.0
    }
}

/// One source, and what is being waited for on it.
///
/// `writable` is not symmetric with readable: a connection is **always** worth
/// waiting on for arriving bytes, and worth waiting on for room to write only
/// while it still has bytes queued. Asking for writability unconditionally
/// would wake the engine every time any socket had room, which is always.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interest {
    /// What to wait on.
    pub source: Source,
    /// Also wake when this source will accept bytes, not only when it has some.
    pub writable: bool,
}

impl Interest {
    /// Wait for bytes to arrive.
    #[must_use]
    pub const fn readable(source: Source) -> Self {
        Self {
            source,
            writable: false,
        }
    }

    /// Wait for bytes to arrive, or for room to send the ones still queued.
    #[must_use]
    pub const fn readable_and_writable(source: Source) -> Self {
        Self {
            source,
            writable: true,
        }
    }
}

/// One connection's bytes.
pub trait Transport {
    /// Whether this transport can be waited on at all.
    ///
    /// `false` is the default because it is the safe answer: a transport that
    /// does not override it cannot be used in `standard` mode, and
    /// [`crate::Engine::run`] refuses that pairing **when it is compiled**
    /// rather than when it is run — ADR-0014 decision 4. [`Loopback`] is the
    /// case that matters: it is the acceptance corpus's transport, it has no
    /// descriptor, and an engine that blocked on an empty source list would
    /// still pass the corpus while waking only on its own timeout.
    const POLLABLE: bool = false;

    /// Read what has arrived, if anything.
    fn recv(&mut self, buf: &mut [u8]) -> Io;
    /// Write what fits. A short write is [`Io::Ready`] with fewer bytes than
    /// offered, and the caller keeps the rest — that is backpressure, and its
    /// policy is `DESIGN.md` D10's business rather than this trait's.
    fn send(&mut self, buf: &[u8]) -> Io;

    /// The handle to wait on. `Some` whenever [`Self::POLLABLE`].
    ///
    /// Has a default body so that a transport somebody else wrote keeps
    /// compiling across this change; what it cannot do is join a `standard`
    /// engine, and that is a compile error rather than a slow one.
    fn source(&self) -> Option<Source> {
        None
    }
}

/// A non-blocking TCP stream.
pub struct TcpTransport {
    sock: TcpStream,
}

impl TcpTransport {
    /// Take ownership of `sock` and put it in non-blocking mode.
    ///
    /// **Non-blocking is not optional here.** A stream left in blocking mode
    /// would park the engine thread inside `read`, which is non-negotiable 4
    /// broken by a single missing call — so it is done here, once, rather than
    /// trusted to the caller.
    ///
    /// # Errors
    ///
    /// Whatever `set_nonblocking` returns.
    pub fn new(sock: TcpStream) -> io::Result<Self> {
        sock.set_nonblocking(true)?;
        // Nagle batches small writes, which is the opposite of what a FIX
        // session wants: a Logon reply held back for 40 ms is 40 ms of latency
        // nothing in this design can recover.
        let _ = sock.set_nodelay(true);
        Ok(Self { sock })
    }

    /// The stream, for a caller that needs its address or wants to close it.
    #[must_use]
    pub const fn socket(&self) -> &TcpStream {
        &self.sock
    }
}

impl Transport for TcpTransport {
    const POLLABLE: bool = cfg!(unix);

    fn source(&self) -> Option<Source> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            Some(Source::from_raw_fd(self.sock.as_raw_fd()))
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    fn recv(&mut self, buf: &mut [u8]) -> Io {
        match self.sock.read(buf) {
            Ok(0) => Io::Closed,
            Ok(n) => Io::Ready(n),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Io::Idle,
            // A signal arriving mid-syscall is not a session event.
            Err(e) if e.kind() == io::ErrorKind::Interrupted => Io::Idle,
            Err(e) => Io::Failed(e.kind()),
        }
    }

    fn send(&mut self, buf: &[u8]) -> Io {
        if buf.is_empty() {
            return Io::Idle;
        }
        match self.sock.write(buf) {
            Ok(0) => Io::Idle,
            Ok(n) => Io::Ready(n),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Io::Idle,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => Io::Idle,
            Err(e) => Io::Failed(e.kind()),
        }
    }
}

/// An in-memory transport, for tests and for the corpus.
///
/// A test that binds a real port fails on a busy machine for a reason that has
/// nothing to do with FIX, and CI is exactly where that bites. This has the
/// same three answers and no kernel.
///
/// It allocates — deliberately, and only here. Nothing in `benches/alloc.rs`
/// runs over it.
pub struct Loopback {
    /// What this end reads from.
    inbox: Rc<core::cell::RefCell<Pipe>>,
    /// What this end writes into.
    outbox: Rc<core::cell::RefCell<Pipe>>,
}

#[derive(Default)]
struct Pipe {
    bytes: std::collections::VecDeque<u8>,
    closed: bool,
}

impl Loopback {
    /// Two ends of one wire.
    #[must_use]
    pub fn pair() -> (Self, Self) {
        let a = Rc::new(core::cell::RefCell::new(Pipe::default()));
        let b = Rc::new(core::cell::RefCell::new(Pipe::default()));
        (
            Self {
                inbox: Rc::clone(&a),
                outbox: Rc::clone(&b),
            },
            Self {
                inbox: b,
                outbox: a,
            },
        )
    }

    /// Hang up. The other end then reads [`Io::Closed`] once it has drained
    /// what was already sent — a close does not swallow bytes already on the
    /// wire, and neither does TCP.
    pub fn close(&mut self) {
        self.outbox.borrow_mut().closed = true;
    }
}

impl Transport for Loopback {
    fn recv(&mut self, buf: &mut [u8]) -> Io {
        let mut pipe = self.inbox.borrow_mut();
        let n = buf.len().min(pipe.bytes.len());
        if n == 0 {
            return if pipe.closed { Io::Closed } else { Io::Idle };
        }
        for slot in buf.iter_mut().take(n) {
            *slot = pipe.bytes.pop_front().unwrap_or(0);
        }
        Io::Ready(n)
    }

    fn send(&mut self, buf: &[u8]) -> Io {
        if buf.is_empty() {
            return Io::Idle;
        }
        let mut pipe = self.outbox.borrow_mut();
        if pipe.closed {
            return Io::Closed;
        }
        pipe.bytes.extend(buf.iter().copied());
        Io::Ready(buf.len())
    }
}
