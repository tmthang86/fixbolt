//! `DESIGN.md` D4: the two dispatch shapes, and the ring underneath one of them.
//!
//! The test that matters is the last one — **the same engine, the same corpus
//! message, and the same bytes out** whichever dispatch is fitted. Anything
//! else would make the ring a different protocol rather than a different
//! thread.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use nanofix_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use nanofix_engine::clock::ManualClock;
use nanofix_engine::dispatch::{Dispatch, InlineDispatch, RingApp, RingDispatch};
use nanofix_engine::ring;
use nanofix_engine::transport::{Io, Loopback, Transport};
use nanofix_engine::wait::Park;
use nanofix_engine::{Application, Config, Engine};

const M: usize = 512;

/// Echoes every application message straight back, unchanged but for the
/// session fields the session itself rewrites.
struct Echo;

impl Application for Echo {
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        nanofix_conformance::echo::echo(msg, out, seq, stamp).ok()
    }
}

// ---------------------------------------------------------------- the ring

#[test]
fn a_record_comes_back_whole_or_not_at_all() {
    let (mut tx, mut rx) = ring::pair(64);
    assert!(tx.push(&[b"one"]));
    assert!(tx.push(&[b"two", b"-and-a-bit"]));

    let mut out = [0u8; 64];
    assert_eq!(rx.pop(&mut out).map(|n| &out[..n]), Some(&b"one"[..]));
    assert_eq!(
        rx.pop(&mut out).map(|n| &out[..n]),
        Some(&b"two-and-a-bit"[..]),
        "parts are laid end to end and come back as one record"
    );
    assert_eq!(rx.pop(&mut out), None, "and then it is empty");
}

#[test]
fn a_full_ring_refuses_and_writes_nothing() {
    // 16 bytes of buffer, 4 of which every record spends on its header.
    let (mut tx, mut rx) = ring::pair(16);
    assert!(tx.push(&[&[b'a'; 12][..]]), "exactly full is still a fit");
    assert!(!tx.push(&[b"x"]), "and one byte more does not");

    let mut out = [0u8; 32];
    assert_eq!(
        rx.pop(&mut out).map(|n| &out[..n]),
        Some(&[b'a'; 12][..]),
        "the refusal left the record that was already there intact"
    );
    assert_eq!(rx.pop(&mut out), None);

    // And the space comes back.
    assert!(tx.push(&[&[b'b'; 12][..]]));
}

#[test]
fn a_record_too_big_for_the_reader_is_dropped_rather_than_wedging_the_queue() {
    let (mut tx, mut rx) = ring::pair(64);
    assert!(tx.push(&[b"a-long-record"]));
    assert!(tx.push(&[b"short"]));

    let mut small = [0u8; 5];
    assert_eq!(
        rx.pop(&mut small),
        Some(0),
        "reported as zero, which is not the same answer as an empty queue"
    );
    assert_eq!(
        rx.pop(&mut small).map(|n| &small[..n]),
        Some(&b"short"[..]),
        "and the queue moved on rather than offering the same record forever"
    );
}

#[test]
fn the_ring_carries_bytes_across_a_real_thread() {
    let (mut to, mut from) = ring::pair(4096);
    let handle = std::thread::spawn(move || {
        let mut out = [0u8; 64];
        let mut got = Vec::new();
        while got.len() < 100 {
            if let Some(n) = from.pop(&mut out) {
                got.push(out[..n].to_vec());
            }
        }
        got
    });
    for i in 0..100u32 {
        let bytes = i.to_le_bytes();
        while !to.push(&[&bytes]) {
            std::hint::spin_loop();
        }
    }
    let got = handle.join().expect("the consumer thread");
    let want: Vec<Vec<u8>> = (0..100u32).map(|i| i.to_le_bytes().to_vec()).collect();
    assert_eq!(got, want, "in order, whole, and none lost");
}

// ------------------------------------------------------------- the dispatch

/// A Logon and one order, as the corpus writes them.
fn traffic() -> (Vec<u8>, Vec<u8>) {
    (
        wire("A", 1, "98=0\x01108=30\x01"),
        wire(
            "D",
            2,
            "11=ID-1\x0121=1\x0138=100\x0140=1\x0154=1\x0155=INTC\x0160=20260828-12:00:00.000\x01",
        ),
    )
}

/// A whole message. **The header fields come first and in tag order**, because
/// they must: a `49=` after a body field answers `373=14`, tag out of required
/// order, and the test would then measure the reject path rather than the echo.
fn wire(msg_type: &str, seq: u32, body: &str) -> Vec<u8> {
    let body = format!(
        "35={msg_type}\x0134={seq}\x0149=TW44\x0152=20260828-12:00:00.000\x0156=ISLD\x01{body}"
    );
    with_real_checksum(format!("8=FIX.4.4\x019={}\x01{body}10=0\x01", body.len()).as_bytes())
}

/// Everything the engine wrote back, as one blob.
fn drain(peer: &mut Loopback) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    while let Io::Ready(n) = peer.recv(&mut buf) {
        out.extend_from_slice(&buf[..n]);
    }
    out
}

type Wired<D> = Engine<Loopback, nanofix_session::Acceptor, D, ManualClock, Park, 256, 4096, 8192>;

fn engine<D: Dispatch>(dispatch: D) -> (Loopback, Wired<D>) {
    let (peer, side) = Loopback::pair();
    let mut engine: Wired<D> = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        dispatch,
        ManualClock::at(FIXED_TIME_MILLIS),
        Park,
        4,
    );
    let _ = engine.add(side);
    (peer, engine)
}

#[test]
fn inline_answers_on_the_same_turn() {
    let (logon, order) = traffic();
    let (mut peer, mut e) = engine(InlineDispatch::new(Echo));

    let _ = peer.send(&logon);
    e.turn();
    assert!(!drain(&mut peer).is_empty(), "the Logon is answered");

    let _ = peer.send(&order);
    e.turn();
    let out = drain(&mut peer);
    assert!(
        out.windows(9).any(|w| w == b"\x0111=ID-1\x01"),
        "the echo came back on the very same turn: {}",
        String::from_utf8_lossy(&out)
    );
}

#[test]
fn the_ring_answers_a_turn_later_and_says_so() {
    let (logon, order) = traffic();
    let (to_app, from_engine) = ring::pair(8192);
    let (to_engine, from_app) = ring::pair(8192);
    let (mut peer, mut e) = engine(RingDispatch::<M>::new(to_app, from_app));
    let mut app: RingApp<M> = RingApp::new(from_engine, to_engine);

    let _ = peer.send(&logon);
    e.turn();
    let _ = drain(&mut peer);

    let _ = peer.send(&order);
    e.turn();
    assert!(
        drain(&mut peer).is_empty(),
        "nothing yet: the application has not run"
    );

    assert_eq!(app.pump(&mut Echo), 1, "and now it has");
    e.turn();
    let out = drain(&mut peer);
    assert!(
        out.windows(9).any(|w| w == b"\x0111=ID-1\x01"),
        "the reply arrives on the turn after the application ran: {}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(e.dispatch_mut().refused(), 0, "nothing was refused");
    assert_eq!(app.dropped(), 0, "and nothing was dropped");
}

/// The one that makes the ring a *thread* rather than a *protocol*.
#[test]
fn inline_and_ring_put_the_same_bytes_on_the_wire() {
    let (logon, order) = traffic();

    let (mut peer_a, mut a) = engine(InlineDispatch::new(Echo));
    let _ = peer_a.send(&logon);
    a.turn();
    let _ = peer_a.send(&order);
    a.turn();
    a.turn();
    let inline = drain(&mut peer_a);

    let (to_app, from_engine) = ring::pair(8192);
    let (to_engine, from_app) = ring::pair(8192);
    let (mut peer_b, mut b) = engine(RingDispatch::<M>::new(to_app, from_app));
    let mut app: RingApp<M> = RingApp::new(from_engine, to_engine);
    let _ = peer_b.send(&logon);
    b.turn();
    let _ = peer_b.send(&order);
    b.turn();
    app.pump(&mut Echo);
    b.turn();
    let ringed = drain(&mut peer_b);

    assert_eq!(
        String::from_utf8_lossy(&inline),
        String::from_utf8_lossy(&ringed),
        "the dispatch decides which thread, and nothing else"
    );
}

#[test]
fn a_reply_for_a_connection_that_has_gone_is_dropped_not_misrouted() {
    let (logon, order) = traffic();
    let (to_app, from_engine) = ring::pair(8192);
    let (to_engine, from_app) = ring::pair(8192);
    let (mut peer, mut e) = engine(RingDispatch::<M>::new(to_app, from_app));
    let mut app: RingApp<M> = RingApp::new(from_engine, to_engine);

    let _ = peer.send(&logon);
    e.turn();
    let _ = peer.send(&order);
    e.turn();
    let _ = drain(&mut peer);

    // The counterparty hangs up while the application is still thinking.
    peer.close();
    e.turn();
    assert_eq!(e.connections(), 0, "the engine noticed");

    app.pump(&mut Echo);
    e.turn();

    // A second connection now exists. A reply routed by index rather than by
    // id would land on it, because `swap_remove` moved something into slot 0.
    let (mut peer2, side2) = Loopback::pair();
    let _ = e.add(side2);
    e.turn();
    let _ = peer2.send(&logon);
    e.turn();
    let _ = drain(&mut peer2);

    // Its *own* order does come back — which is what stops this test passing
    // for the trivial reason that out-of-band collection is switched off.
    let order2 = wire(
        "D",
        2,
        "11=ID-2\x0121=1\x0138=100\x0140=1\x0154=1\x0155=INTC\x0160=20260828-12:00:00.000\x01",
    );
    let _ = peer2.send(&order2);
    e.turn();
    app.pump(&mut Echo);
    e.turn();
    let out = drain(&mut peer2);
    assert!(
        out.windows(9).any(|w| w == b"\x0111=ID-2\x01"),
        "this connection's own reply must arrive: {}",
        String::from_utf8_lossy(&out)
    );
    assert!(
        !out.windows(9).any(|w| w == b"\x0111=ID-1\x01"),
        "the dead connection's reply must not: {}",
        String::from_utf8_lossy(&out)
    );
}
