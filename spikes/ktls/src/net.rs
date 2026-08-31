//! Plain sockets, spun on rather than waited on.
//!
//! Nothing in this file may block. That is the whole point of the spike: the
//! engine's `hft` mode never sleeps in the kernel (`CLAUDE.md` §2 rule 4), so a
//! transport that only works when someone calls `epoll_wait` is no use here.
//! Every wait below is a spin with a deadline.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// A connected, non-blocking loopback pair: `(accepted, connected)`.
pub fn pair() -> io::Result<(TcpStream, TcpStream)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let client = TcpStream::connect(addr)?;
    let (server, _) = listener.accept()?;

    // Nagle off on both ends. Left on, a 40 ms delayed-ack interaction turns
    // every spin loop below into a timeout, and the spike would report a
    // transport failure that is really a socket option.
    server.set_nodelay(true)?;
    client.set_nodelay(true)?;
    server.set_nonblocking(true)?;
    client.set_nonblocking(true)?;

    Ok((server, client))
}

pub const DEADLINE: Duration = Duration::from_secs(10);

/// Write every byte, spinning through `WouldBlock`.
pub fn spin_write_all(sock: &mut TcpStream, mut buf: &[u8]) -> io::Result<()> {
    let start = Instant::now();
    while !buf.is_empty() {
        match sock.write(buf) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
            Ok(n) => buf = &buf[n..],
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if start.elapsed() > DEADLINE {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "spin_write_all"));
                }
                std::hint::spin_loop();
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Read at least one byte, spinning through `WouldBlock`.
pub fn spin_read(sock: &mut TcpStream, buf: &mut [u8]) -> io::Result<usize> {
    let start = Instant::now();
    loop {
        match sock.read(buf) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
            Ok(n) => return Ok(n),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if start.elapsed() > DEADLINE {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "spin_read"));
                }
                std::hint::spin_loop();
            }
            Err(e) => return Err(e),
        }
    }
}

/// Read whatever is already there and throw it away, until the socket is empty.
/// Used to reach a known-clean starting point before a wire observation.
pub fn drain(sock: &mut TcpStream) -> io::Result<usize> {
    let mut scratch = [0u8; 4096];
    let mut total = 0;
    loop {
        match sock.read(&mut scratch) {
            Ok(0) => return Ok(total),
            Ok(n) => total += n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(total),
            Err(e) => return Err(e),
        }
    }
}

/// Spin until `flag` reads `want`. The two halves of a wire observation have to
/// agree on an order, and a mutex would put a futex on a path this spike is
/// trying to prove has no blocking call on it.
pub fn spin_until(flag: &std::sync::atomic::AtomicU32, want: u32) -> io::Result<()> {
    let start = Instant::now();
    while flag.load(std::sync::atomic::Ordering::Acquire) != want {
        if start.elapsed() > DEADLINE {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "spin_until"));
        }
        std::hint::spin_loop();
    }
    Ok(())
}

/// `/proc/net/tls_stat`, parsed. The kernel's own count of kTLS sockets — an
/// observation made outside this process's own code, which is the only kind
/// that can contradict it.
pub fn tls_stat() -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string("/proc/net/tls_stat") else {
        return out;
    };
    for line in text.lines() {
        if let Some((k, v)) = line.split_once(char::is_whitespace)
            && let Ok(n) = v.trim().parse::<u64>()
        {
            out.insert(k.trim_end_matches(':').to_string(), n);
        }
    }
    out
}
