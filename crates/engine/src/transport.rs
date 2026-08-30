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

/// One connection's bytes.
pub trait Transport {
    /// Read what has arrived, if anything.
    fn recv(&mut self, buf: &mut [u8]) -> Io;
    /// Write what fits. A short write is [`Io::Ready`] with fewer bytes than
    /// offered, and the caller keeps the rest — that is backpressure, and its
    /// policy is `DESIGN.md` D10's business rather than this trait's.
    fn send(&mut self, buf: &[u8]) -> Io;
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
