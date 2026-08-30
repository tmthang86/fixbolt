//! The 59 acceptance definitions, **through a real socket**.
//!
//! `crates/session/tests/score.rs` runs the same files in process, handing the
//! session a slice and collecting its answers through a closure. This runs them
//! over TCP: the bytes go through the kernel, come back through a framer, and
//! nothing in the path is a test double except the clock.
//!
//! # Why the clock is injected and nothing else is
//!
//! Every `I` line in the corpus carries a fixed instant. Against the wall clock
//! that is two days of skew and every message is refused for a reason that has
//! nothing to do with the engine. `nanofix_engine::clock::ManualClock` is the
//! one seam; the sockets, the framing, the session and the application are all
//! the real ones.
//!
//! # And why there is no thread
//!
//! `Engine::turn` is one non-blocking pass. Driving it by hand makes this test
//! as deterministic as the in-process gate — no sleeps, no timing window, no
//! flake. A background thread would have bought nothing and cost that.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::ops::Range;

use nanofix_conformance::runner::{Conn, Input, Link, SessionUnderTest, run};
use nanofix_conformance::script::FIXED_TIME_MILLIS;
use nanofix_engine::clock::ManualClock;
use nanofix_engine::dispatch::InlineDispatch;
use nanofix_engine::transport::TcpTransport;
use nanofix_engine::{Acceptor, Engine};
use nanofix_session::{Application, Config};

const N: usize = 256;
const RX: usize = 4096;
const TX: usize = 8192;

/// The acceptance server's own application, wired through the engine.
///
/// The same one `crates/session/tests/score.rs` uses, for the same reason: 42
/// of the corpus's 250 `E` lines carry `35=D`, so a session layer alone cannot
/// pass. See `nanofix_conformance::echo`.
#[derive(Default)]
struct EchoApp {
    seen: Vec<Vec<u8>>,
}

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        let msg_type = field(msg, 35)?;
        if msg_type != b"D" && msg_type != b"d" {
            return nanofix_conformance::echo::business_reject(msg, out, seq, stamp).ok();
        }
        if let Some(id) = field(msg, 11) {
            let already = self.seen.iter().any(|s| s == id);
            if field(msg, 97) == Some(b"Y") && already {
                return None;
            }
            if !already {
                self.seen.push(id.to_vec());
            }
        }
        nanofix_conformance::echo::echo(msg, out, seq, stamp).ok()
    }
}

fn field(wire: &[u8], tag: u32) -> Option<&[u8]> {
    let needle = format!("{tag}=").into_bytes();
    let mut at = 0;
    while at < wire.len() {
        let end = wire[at..].iter().position(|b| *b == 1)? + at;
        if wire[at..end].starts_with(&needle) {
            return Some(&wire[at + needle.len()..end]);
        }
        at = end + 1;
    }
    None
}

/// The counterparty: one client socket per `Conn`, and the engine on the other
/// side of the loopback interface.
struct Wire {
    acceptor: Acceptor,
    engine: Engine<
        TcpTransport,
        nanofix_session::Acceptor,
        InlineDispatch<EchoApp>,
        ManualClock,
        Park,
        N,
        RX,
        TX,
    >,
    clients: Vec<(Conn, Option<TcpStream>)>,
}

/// The engine idles by yielding here. A spinning engine in a test suite is a
/// test suite that pins a core for no reason — `wait::Spin` is for `tools/w2w`.
use nanofix_engine::wait::Park;

impl Wire {
    fn new() -> Self {
        let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
        Self {
            acceptor,
            engine: Engine::new(
                Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
                InlineDispatch::new(EchoApp::default()),
                ManualClock::at(FIXED_TIME_MILLIS),
                Park,
                4,
            ),
            clients: Vec::new(),
        }
    }

    fn at(&mut self, conn: Conn) -> usize {
        if let Some(i) = self.clients.iter().position(|(c, _)| *c == conn) {
            return i;
        }
        self.clients.push((conn, None));
        self.clients.len() - 1
    }

    /// Turn the engine until it settles.
    ///
    /// **Idle once is not settled.** Loopback delivery is not instantaneous:
    /// the first turn after a write regularly finds nothing, and a pump that
    /// stopped there would report silence for a message that arrives a
    /// microsecond later. So it keeps turning until a run of turns in a row has
    /// moved nothing, and gives up after a bound rather than hanging.
    fn pump(&mut self) {
        let mut quiet = 0;
        for _ in 0..20_000 {
            let mut moved = false;
            while let Some(t) = self.acceptor.accept() {
                let _ = self.engine.add(t);
                moved = true;
            }
            moved |= self.engine.turn();
            if moved {
                quiet = 0;
            } else {
                quiet += 1;
                if quiet > 200 {
                    return;
                }
            }
        }
    }

    /// Everything the engine has written to this client, cut into messages.
    ///
    /// Returns `true` if the engine closed the connection — `Ok(0)` on a
    /// non-blocking read is end-of-stream and nothing else, which is the
    /// distinction `transport::Io` exists to keep.
    fn drain(&mut self, i: usize, emit: &mut impl FnMut(&[u8])) -> bool {
        let mut buf = [0u8; 16384];
        let mut held: Vec<u8> = Vec::new();
        let mut closed = false;
        if let Some(sock) = self.clients[i].1.as_mut() {
            loop {
                match sock.read(&mut buf) {
                    Ok(0) => {
                        closed = true;
                        break;
                    }
                    Ok(n) => held.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
        }
        let mut at = 0;
        while let Some(end) = next_message(&held[at..]) {
            emit(&held[at..at + end]);
            at += end;
        }
        closed
    }
}

/// Where the message starting at `bytes[0]` ends, by its own `9=` and trailer.
fn next_message(bytes: &[u8]) -> Option<usize> {
    let at = bytes.windows(3).position(|w| w == b"\x019=")?;
    let digits = &bytes[at + 3..];
    let end = digits.iter().position(|b| *b == 1)?;
    let len: usize = core::str::from_utf8(&digits[..end]).ok()?.parse().ok()?;
    let stop = at + 3 + end + 1 + len;
    if bytes.len() < stop + 4 || bytes.get(stop..stop + 3) != Some(b"10=") {
        return None;
    }
    let k = bytes[stop + 3..].iter().position(|b| *b == 1)?;
    Some(stop + 3 + k + 1)
}

impl SessionUnderTest for Wire {
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, mut emit: F) -> Link {
        let i = self.at(conn);
        match input {
            Input::Connect => {
                let addr = self.acceptor.local_addr().expect("bound");
                let sock = TcpStream::connect(addr).expect("loopback");
                sock.set_nonblocking(true).expect("non-blocking");
                self.clients[i].1 = Some(sock);
            }
            Input::Disconnect => {
                self.clients[i].1 = None;
            }
            Input::Bytes(b) => {
                if let Some(sock) = self.clients[i].1.as_mut() {
                    let _ = sock.write_all(b);
                }
            }
            Input::Tick(ms) => {
                // The corpus's clock is the engine's clock, and it is the only
                // thing in this test that is not the real component.
                *self.engine_clock() = ManualClock::at(ms);
            }
        }
        self.pump();
        let closed = self.drain(i, &mut emit);
        if closed {
            self.clients[i].1 = None;
            return Link::Dropped;
        }
        Link::Up
    }
}

impl Wire {
    fn engine_clock(&mut self) -> &mut ManualClock {
        self.engine.clock_mut()
    }
}

/// `[measured 2026-08-30]` step 3 of the engine plan.
#[test]
fn the_fifty_nine_definitions_pass_through_a_real_socket() {
    let report = run(|_| Wire::new()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.passed, 59, "over TCP, not in process:\n{report}");
}
