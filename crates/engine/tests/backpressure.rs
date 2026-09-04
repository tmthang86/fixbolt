//! `DESIGN.md` D10: what a connection does when the counterparty stops reading.
//!
//! Driven against [`Connection`] rather than [`fixbolt_engine::Engine`], on
//! purpose: the policy is a per-connection property and the test wants to hold
//! the socket still, which is easier one connection at a time.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::VecDeque;

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::backpressure::Backpressure;
use fixbolt_engine::conn::{Connection, Turn};
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::{Io, Transport};
use fixbolt_session::{Acceptor, Config, Session, Silent};

const N: usize = 256;
const RX: usize = 4096;
/// Small on purpose: the corpus's own messages are ~100 bytes, so a 512-byte
/// queue fills after five of them and the test does not have to send hundreds.
const TX: usize = 512;

/// A socket that takes only as many bytes as it is told to.
///
/// `allow` is the whole point: set it to zero and every `send` answers
/// [`Io::Idle`], which is exactly what a real socket does when the
/// counterparty's receive window has closed.
#[derive(Default)]
struct Choked {
    inbox: VecDeque<u8>,
    sent: Vec<u8>,
    /// Bytes this socket will still accept.
    allow: usize,
    /// `send` calls that answered `Idle`. `Block` spins on these.
    refusals: usize,
    /// The peer hung up: every `send` answers [`Io::Closed`].
    dead: bool,
}

impl Transport for Choked {
    fn recv(&mut self, buf: &mut [u8]) -> Io {
        let n = buf.len().min(self.inbox.len());
        if n == 0 {
            return Io::Idle;
        }
        for slot in buf.iter_mut().take(n) {
            *slot = self.inbox.pop_front().unwrap_or(0);
        }
        Io::Ready(n)
    }

    fn send(&mut self, buf: &[u8]) -> Io {
        if self.dead {
            return Io::Closed;
        }
        let n = buf.len().min(self.allow);
        if n == 0 {
            self.refusals += 1;
            return Io::Idle;
        }
        self.allow -= n;
        self.sent.extend_from_slice(&buf[..n]);
        Io::Ready(n)
    }
}

type Conn = Connection<Choked, Acceptor, Store, N, RX, TX>;

fn conn(policy: Backpressure, allow: usize) -> Conn {
    let transport = Choked {
        allow,
        ..Default::default()
    };
    let mut c = Connection::new(
        1,
        transport,
        Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
        Store::new(),
    )
    .with_backpressure(policy);
    c.opened();
    c
}

/// A whole message. Header fields first and in tag order — a `49=` after a
/// body field answers `373=14` and the test would measure the reject path.
fn wire(msg_type: &str, seq: u32, body: &str) -> Vec<u8> {
    let body = format!(
        "35={msg_type}\x0134={seq}\x0149=TW44\x0152=20260828-12:00:00.000\x0156=ISLD\x01{body}"
    );
    with_real_checksum(format!("8=FIX.4.4\x019={}\x01{body}10=0\x01", body.len()).as_bytes())
}

fn logon() -> Vec<u8> {
    wire("A", 1, "98=0\x01108=30\x01")
}

/// A `TestRequest`, which a session must answer with a `Heartbeat`. One
/// inbound message, one outbound message, and nothing the application decides.
fn test_request(seq: u32) -> Vec<u8> {
    wire("1", seq, "112=REQ\x01")
}

fn feed<const T: usize>(c: &mut Connection<Choked, Acceptor, Store, N, RX, T>, bytes: &[u8]) {
    c.transport.inbox.extend(bytes.iter().copied());
}

fn turn(c: &mut Conn) -> Turn {
    c.turn(FIXED_TIME_MILLIS, &mut Silent, |_| false)
}

fn text<const T: usize>(c: &Connection<Choked, Acceptor, Store, N, RX, T>) -> String {
    String::from_utf8_lossy(&c.transport.sent).into_owned()
}

#[test]
fn a_queue_that_fills_ends_the_session_with_a_reason() {
    // The socket never takes a byte, so everything the session says queues.
    let mut c = conn(Backpressure::Disconnect, 0);
    feed(&mut c, &logon());
    turn(&mut c);

    // Five or six ~100-byte answers fill 512 bytes.
    let mut ended = false;
    for seq in 2..=12u32 {
        feed(&mut c, &test_request(seq));
        if turn(&mut c) == Turn::Gone {
            ended = true;
            break;
        }
        if !c.has_pending_output() {
            panic!("the socket took bytes it was told not to");
        }
    }
    assert!(!ended, "it cannot leave while the Logout has nowhere to go");

    // The counterparty starts reading again. The Logout is what comes out.
    c.transport.allow = 64 * 1024;
    let outcome = turn(&mut c);
    let out = text(&c);
    assert!(out.contains("35=5\u{1}"), "a Logout, not silence: {out}");
    assert!(
        out.contains("58=slow consumer\u{1}"),
        "and it says why, in D10's own words: {out}"
    );
    assert_eq!(outcome, Turn::Gone, "and then the connection is finished");
}

#[test]
fn every_byte_that_reaches_the_wire_is_part_of_a_whole_message() {
    // The socket drains freely at first, then stops, then opens again once the
    // queue has overflowed. If the queue ever wrote as much of a message as
    // would fit, the stream below would not decompose cleanly.
    let mut c = conn(Backpressure::Disconnect, 64 * 1024);
    feed(&mut c, &logon());
    turn(&mut c);
    for seq in 2..=6u32 {
        feed(&mut c, &test_request(seq));
        assert_ne!(turn(&mut c), Turn::Gone);
    }

    c.transport.allow = 0;
    for seq in 7..=20u32 {
        feed(&mut c, &test_request(seq));
        if turn(&mut c) == Turn::Gone {
            break;
        }
    }
    c.transport.allow = 64 * 1024;
    turn(&mut c);

    let out = c.transport.sent.clone();
    let mut at = 0;
    let mut whole = 0;
    while at < out.len() {
        let end = next_message(&out[at..])
            .unwrap_or_else(|| panic!("a fragment at byte {at}: {}", text(&c)));
        at += end;
        whole += 1;
    }
    assert!(whole >= 7, "there were several messages to check, not one");
    assert_eq!(at, out.len(), "and nothing was left over");
}

/// Where the message starting at `bytes[0]` ends, by its own `9=` and trailer.
/// `None` if it is not a whole message.
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

#[test]
fn queue_bounds_the_wait_more_tightly_than_the_buffer() {
    // A bound smaller than one message: the very first answer overflows.
    let mut c = conn(Backpressure::Queue { max_bytes: 32 }, 0);
    feed(&mut c, &logon());
    turn(&mut c);
    c.transport.allow = 64 * 1024;
    let outcome = turn(&mut c);
    let out = text(&c);
    assert!(
        out.contains("58=slow consumer\u{1}"),
        "the Logon answer did not fit and the session said so: {out}"
    );
    assert_eq!(outcome, Turn::Gone);

    // The same traffic with the whole buffer available survives it.
    let mut roomy = conn(Backpressure::Disconnect, 64 * 1024);
    feed(&mut roomy, &logon());
    assert_ne!(turn(&mut roomy), Turn::Gone);
    let out = text(&roomy);
    assert!(out.contains("35=A\u{1}"), "the Logon was answered: {out}");
    assert!(
        !out.contains("slow consumer"),
        "and nothing was refused: {out}"
    );
}

#[test]
fn block_waits_for_the_socket_instead_of_ending_the_session() {
    // A socket with room, so `Block` never has to spin — the ordinary case.
    let mut c = conn(Backpressure::Block, 64 * 1024);
    feed(&mut c, &logon());
    assert_ne!(turn(&mut c), Turn::Gone);

    for seq in 2..=40u32 {
        feed(&mut c, &test_request(seq));
        assert_ne!(
            turn(&mut c),
            Turn::Gone,
            "forty messages through a 512-byte queue, and it never gave up"
        );
    }
    let out = text(&c);
    assert!(
        !out.contains("slow consumer"),
        "Block never says this: {out}"
    );
    assert_eq!(
        out.matches("35=0\u{1}").count(),
        39,
        "every TestRequest was answered: {out}"
    );
}

#[test]
fn block_spins_on_a_socket_that_is_not_ready_and_then_writes() {
    // Room for the Logon answer and nothing more.
    let mut c = conn(Backpressure::Block, 0);
    feed(&mut c, &logon());
    turn(&mut c);
    assert!(c.has_pending_output(), "the Logon answer is queued");

    // Fill the queue right up, so the next message has to wait for room.
    for seq in 2..=12u32 {
        feed(&mut c, &test_request(seq));
        // The socket opens by exactly one message's worth each time it is
        // asked, so `Block` must spin, drain, and carry on.
        c.transport.allow = 128;
        assert_ne!(turn(&mut c), Turn::Gone);
    }
    assert!(
        c.transport.refusals > 0,
        "the test never actually made the socket say no"
    );
    let out = text(&c);
    assert!(
        !out.contains("slow consumer"),
        "and it still never gave up: {out}"
    );
}

#[test]
fn a_socket_that_dies_while_being_waited_on_ends_the_connection() {
    let mut c = conn(Backpressure::Block, 64 * 1024);
    feed(&mut c, &logon());
    turn(&mut c);

    // The peer hangs up. There are bytes queued, and there is now nowhere to
    // put them. Waiting for the queue to drain would wait for ever.
    c.transport.allow = 0;
    c.transport.dead = true;
    feed(&mut c, &test_request(2));
    let outcome = turn(&mut c);
    assert_eq!(
        outcome,
        Turn::Gone,
        "a dead socket ends the connection rather than being spun on"
    );
}

// ------------------------------------- and the resend that used to cause it
//
// ADR-0046 decision 4. D10 is right about a counterparty that has stopped
// reading; it was being fired at one that had just asked a question.

/// A wide transmit buffer, so the only thing under test is how much one call
/// puts into it.
type Wide = Connection<Choked, Acceptor, Store, N, RX, 8192>;

fn wide(policy: Backpressure, batch: u16) -> Wide {
    let transport = Choked {
        allow: 1 << 20,
        ..Default::default()
    };
    let mut c = Connection::new(
        1,
        transport,
        Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_resend_batch(batch)),
        Store::new(),
    )
    .with_backpressure(policy);
    c.opened();
    c
}

/// A `NewOrderSingle` around 200 bytes, which the echo answers with one of its
/// own.
fn fat_order(seq: u32) -> Vec<u8> {
    let pad = "X".repeat(96);
    wire(
        "D",
        seq,
        &format!("11=ORD-{pad}\x0155=IBM\x0154=1\x0138=1\x0140=1\x0160=20260828-12:00:00\x01"),
    )
}

fn resend_request(seq: u32, from: u32, to: u32) -> Vec<u8> {
    wire("2", seq, &format!("7={from}\x0116={to}\x01"))
}

fn turn_echo(c: &mut Wide) -> Turn {
    struct Echo;
    impl fixbolt_session::Application for Echo {
        fn on_message(
            &mut self,
            msg: &[u8],
            seq: u32,
            stamp: &[u8],
            out: &mut [u8],
        ) -> Option<std::ops::Range<usize>> {
            fixbolt_conformance::echo::echo(msg, out, seq, stamp).ok()
        }
    }
    c.turn(FIXED_TIME_MILLIS, &mut Echo, |_| false)
}

/// A counterparty asks for a hundred messages back and **stays connected**.
///
/// `[2026-09-04]` this is the test the plan for ADR-0046 exists for. Before it,
/// the replay loop emitted the whole range in one call: a hundred ~200-byte
/// messages against an 8 KiB transmit buffer set `overflow`, and D10's
/// `Disconnect` answered a `ResendRequest` with `Logout 58=slow consumer` —
/// while the counterparty's own socket was empty and it had asked for nothing
/// but what this end had already sent it.
///
/// **The corpus cannot see this.** Every one of the 59 definitions asks for at
/// most three messages, so 59 / 59 is green either way. Only a wire test with a
/// real transmit buffer can tell.
#[test]
fn a_resend_larger_than_tx_does_not_end_the_session() {
    let mut c = wide(Backpressure::Disconnect, 8);
    feed(&mut c, &logon());
    assert_ne!(turn_echo(&mut c), Turn::Gone, "logged on");

    // A hundred orders, each echoed: outbound 34=2..=101, all kept.
    for seq in 2..=101u32 {
        feed(&mut c, &fat_order(seq));
        assert_ne!(turn_echo(&mut c), Turn::Gone, "order {seq}");
    }
    let before = text(&c).matches("35=D\u{1}").count();
    assert_eq!(before, 100, "a hundred echoes went out: {before}");

    // "Send me everything from 2." 100 x ~200 bytes is well over TX = 8192.
    feed(&mut c, &resend_request(102, 2, 0));
    let mut turns = 0;
    loop {
        turns += 1;
        assert_ne!(
            turn_echo(&mut c),
            Turn::Gone,
            "the session ended on turn {turns} while answering a resend"
        );
        let replayed = text(&c).matches("43=Y\u{1}").count();
        if replayed >= 100 {
            break;
        }
        assert!(
            turns < 200,
            "only {replayed} of 100 came back in {turns} turns"
        );
    }

    let out = text(&c);
    assert!(
        !out.contains("58=slow consumer\u{1}"),
        "and nobody was hung up on for being slow: {}",
        &out[out.len().saturating_sub(400)..]
    );
    assert!(turns > 1, "a hundred messages did not go out in one call");

    // Every original number came back, in order, exactly once.
    let order: Vec<u32> = out
        .split("8=FIX.4.4\u{1}")
        .filter(|m| m.contains("43=Y\u{1}"))
        .filter_map(|m| {
            let at = m.find("\u{1}34=")? + 4;
            let end = m[at..].find('\u{1}')? + at;
            m[at..end].parse().ok()
        })
        .collect();
    assert_eq!(order.len(), 100, "a hundred replays");
    assert_eq!(
        order,
        (2..=101).collect::<Vec<u32>>(),
        "in order, none missing"
    );
}
