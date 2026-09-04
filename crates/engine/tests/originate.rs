//! `STATUS.md` item 46, [ADR-0048]: an engine that can speak first.
//!
//! Every other test in this directory is *stimulus → response*. These are the
//! ones that are not, and that is the whole point: the capability under test
//! here is the one no stimulus-shaped fixture can name.
//!
//! [ADR-0048]: ../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};

// ------------------------------------------------------------------ harness

type Wired<A> = Engine<
    Loopback,
    fixbolt_session::Acceptor,
    InlineDispatch<A>,
    ManualClock,
    Yield,
    Store,
    256,
    4096,
    8192,
>;

fn engine<A: Application>(app: A) -> (Loopback, Wired<A>) {
    let (peer, engine, _) = engine_with_id(app);
    (peer, engine)
}

/// The same, keeping the id [`Engine::add`] issued — which is what a real
/// caller routes an origination by, and the reason `add` returns one.
fn engine_with_id<A: Application>(app: A) -> (Loopback, Wired<A>, u64) {
    let (peer, side) = Loopback::pair();
    let mut engine: Wired<A> = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(app),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let id = engine.add(side);
    (peer, engine, id)
}

fn logon() -> Vec<u8> {
    wire("A", 1, "98=0\x01108=30\x01")
}

fn wire(msg_type: &str, seq: u32, body: &str) -> Vec<u8> {
    let body = format!(
        "35={msg_type}\x0134={seq}\x0149=TW44\x0152=20260828-12:00:00.000\x0156=ISLD\x01{body}"
    );
    with_real_checksum(format!("8=FIX.4.4\x019={}\x01{body}10=0\x01", body.len()).as_bytes())
}

fn drain(peer: &mut Loopback) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    while let Io::Ready(n) = peer.recv(&mut buf) {
        out.extend_from_slice(&buf[..n]);
    }
    out
}

/// Every `34=` on the wire, in order, so a test can say *which* numbers.
fn seqs(blob: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(at) = blob[i..].windows(4).position(|w| w == b"\x0134=") {
        let start = i + at + 4;
        let end = start + blob[start..].iter().position(|b| *b == 1).unwrap();
        out.push(String::from_utf8_lossy(&blob[start..end]).parse().unwrap());
        i = end;
    }
    out
}

fn count(blob: &[u8], needle: &[u8]) -> usize {
    blob.windows(needle.len()).filter(|w| *w == needle).count()
}

/// A `35=B` News, laid out by hand into `out`, with `34=` and `52=` left for
/// the session to write. The body length is deliberately wrong — the session
/// rebuilds the frame, so a correct one here would prove nothing about who did.
fn news(out: &mut [u8], text: &str) -> Option<Range<usize>> {
    let msg = format!("8=FIX.4.4\x019=0\x0135=B\x0149=ISLD\x0156=TW44\x01148={text}\x0110=000\x01");
    out.get_mut(..msg.len())?.copy_from_slice(msg.as_bytes());
    Some(0..msg.len())
}

// ------------------------------------------------------- the door on logon

/// The control. Every `Application` written before ADR-0048 is this one.
struct Quiet;

impl Application for Quiet {
    fn on_message(&mut self, _: &[u8], _: u32, _: &[u8], _: &mut [u8]) -> Option<Range<usize>> {
        None
    }
}

#[test]
fn an_application_that_says_nothing_on_logon_is_unchanged() {
    let (mut peer, mut e) = engine(Quiet);
    let _ = peer.send(&logon());
    e.turn();

    let out = drain(&mut peer);
    assert_eq!(
        seqs(&out),
        vec![1],
        "the Logon answer and nothing else: {}",
        String::from_utf8_lossy(&out)
    );
}

/// Two unprompted `35=B`, which is what `tools/interop`'s initiator role has
/// been asking this engine for since 2026-09-04 and not getting.
struct SpeaksFirst;

impl Application for SpeaksFirst {
    fn on_message(&mut self, _: &[u8], _: u32, _: &[u8], _: &mut [u8]) -> Option<Range<usize>> {
        None
    }

    fn on_logon(
        &mut self,
        nth: u32,
        peer: fixbolt_session::Peer<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        assert_eq!(peer.begin_string, b"FIX.4.4");
        assert_eq!(peer.sender, b"ISLD", "this end");
        assert_eq!(peer.target, b"TW44", "the counterparty");
        match nth {
            0 => news(out, "first"),
            1 => news(out, "second"),
            _ => None,
        }
    }
}

#[test]
fn an_application_can_speak_first_and_the_session_numbers_what_it_says() {
    let (mut peer, mut e) = engine(SpeaksFirst);
    let _ = peer.send(&logon());
    e.turn();

    let out = drain(&mut peer);
    assert_eq!(
        count(&out, b"\x0135=B\x01"),
        2,
        "both News went out: {}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(
        seqs(&out),
        vec![1, 2, 3],
        "the Logon answer is 1 and the session numbered the two originations 2 and 3 — \
         the application was never told either number: {}",
        String::from_utf8_lossy(&out)
    );
    // `news` writes no `52=` at all, so every one on the wire was put there by
    // the session. Asserting the *value* here would prove nothing — the manual
    // clock and the fixture literal are the same instant on purpose — so the
    // count is the observable, and
    // `what_an_application_writes_into_34_and_52_is_ignored` is where a wrong
    // value is refuted.
    assert_eq!(
        count(&out, b"\x0152="),
        3,
        "and the session stamped all three, having been given none: {}",
        String::from_utf8_lossy(&out)
    );
}

#[test]
fn speaking_first_happens_once_per_session_not_once_per_turn() {
    let (mut peer, mut e) = engine(SpeaksFirst);
    let _ = peer.send(&logon());
    e.turn();
    let _ = drain(&mut peer);

    e.turn();
    e.turn();
    assert!(
        drain(&mut peer).is_empty(),
        "a session that is already up says nothing more"
    );
}

/// A handler with a bug: it never answers `None`.
struct NeverStops;

impl Application for NeverStops {
    fn on_message(&mut self, _: &[u8], _: u32, _: &[u8], _: &mut [u8]) -> Option<Range<usize>> {
        None
    }

    fn on_logon(
        &mut self,
        _: u32,
        _: fixbolt_session::Peer<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        news(out, "again")
    }
}

#[test]
fn a_handler_that_never_stops_does_not_hold_the_engine_thread() {
    let (mut peer, mut e) = engine(NeverStops);
    let _ = peer.send(&logon());
    e.turn();

    let out = drain(&mut peer);
    assert_eq!(
        count(&out, b"\x0135=B\x01"),
        fixbolt_engine::MAX_ON_LOGON as usize,
        "the engine stopped asking at the bound rather than spinning: {}",
        String::from_utf8_lossy(&out)
    );
}

#[test]
fn what_an_application_writes_into_34_and_52_is_ignored() {
    /// Writes a `34=` and a `52=` of its own, both wrong.
    struct Liar;

    impl Application for Liar {
        fn on_message(&mut self, _: &[u8], _: u32, _: &[u8], _: &mut [u8]) -> Option<Range<usize>> {
            None
        }

        fn on_logon(
            &mut self,
            nth: u32,
            _: fixbolt_session::Peer<'_>,
            out: &mut [u8],
        ) -> Option<Range<usize>> {
            if nth > 0 {
                return None;
            }
            let msg = "8=FIX.4.4\x019=0\x0135=B\x0134=9999\x0149=ISLD\x01\
                       52=19700101-00:00:00.000\x0156=TW44\x01148=hi\x0110=000\x01";
            out.get_mut(..msg.len())?.copy_from_slice(msg.as_bytes());
            Some(0..msg.len())
        }
    }

    let (mut peer, mut e) = engine(Liar);
    let _ = peer.send(&logon());
    e.turn();

    let out = drain(&mut peer);
    assert_eq!(
        seqs(&out),
        vec![1, 2],
        "the session's number, not the application's 9999: {}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(
        count(&out, b"52=19700101"),
        0,
        "and the session's clock, not the application's 1970: {}",
        String::from_utf8_lossy(&out)
    );
}

// ------------------------------------------------------- the door from away

use fixbolt_engine::origin::{ORIGIN_CAPACITY, ORIGIN_LEN};

/// A `35=B` as a caller on another thread would build one: whole, addressed,
/// and carrying neither `34=` nor `52=`.
fn away(text: &str) -> Vec<u8> {
    format!("8=FIX.4.4\x019=0\x0135=B\x0149=ISLD\x0156=TW44\x01148={text}\x0110=000\x01")
        .into_bytes()
}

#[test]
fn an_application_on_another_thread_can_originate() {
    let (mut peer, mut e, id) = engine_with_id(Quiet);
    let sender = e.sender();
    let _ = peer.send(&logon());
    e.turn();
    let _ = drain(&mut peer);

    let handle = std::thread::spawn(move || sender.send(id, &away("from-away")));
    assert!(handle.join().unwrap(), "the queue took it");

    e.turn();
    let out = drain(&mut peer);
    assert_eq!(
        count(&out, b"\x0135=B\x01"),
        1,
        "it reached the wire: {}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(
        seqs(&out),
        vec![2],
        "numbered by the session, following the Logon answer's 1: {}",
        String::from_utf8_lossy(&out)
    );
}

#[test]
fn an_engine_nobody_sends_through_does_not_reach_for_the_lock() {
    let (mut peer, mut e, id) = engine_with_id(Quiet);
    let sender = e.sender();
    let _ = peer.send(&logon());
    for _ in 0..20 {
        e.turn();
    }
    assert_eq!(
        sender.drains(),
        0,
        "twenty turns and the mutex was never attempted — one relaxed load is \
         the whole cost of being sendable-to while nobody sends"
    );

    assert!(sender.send(id, &away("now")));
    e.turn();
    assert_eq!(sender.drains(), 1, "and exactly one attempt once there was");
}

#[test]
fn a_full_queue_refuses_at_the_call_rather_than_losing_a_message() {
    let (mut peer, mut e, id) = engine_with_id(Quiet);
    let sender = e.sender();
    let _ = peer.send(&logon());
    e.turn();

    for i in 0..ORIGIN_CAPACITY {
        assert!(sender.send(id, &away("x")), "slot {i} was free");
    }
    assert!(
        !sender.send(id, &away("one-too-many")),
        "and the caller is told now, not by the message never arriving"
    );

    // The space comes back.
    e.turn();
    assert!(sender.send(id, &away("after-the-drain")));
}

#[test]
fn a_message_longer_than_a_slot_is_refused_and_an_empty_one_too() {
    let (mut peer, mut e, id) = engine_with_id(Quiet);
    let sender = e.sender();
    let _ = peer.send(&logon());
    e.turn();

    assert!(!sender.send(id, &vec![b'x'; ORIGIN_LEN + 1]), "too long");
    assert!(
        sender.send(id, &vec![b'x'; ORIGIN_LEN]),
        "exactly full fits"
    );
    assert!(!sender.send(id, b""), "and nothing is not a message");
}

#[test]
fn a_message_for_a_connection_that_has_gone_is_dropped_and_counted() {
    let (mut peer, mut e) = engine(Quiet);
    let sender = e.sender();
    let mut events = Vec::new();
    let observer = e.observer();
    let _ = peer.send(&logon());
    e.turn();
    let _ = drain(&mut peer);

    // A connection id no engine ever issued.
    assert!(sender.send(u64::MAX - 1, &away("nowhere")));
    e.turn();

    assert_eq!(
        sender.undeliverable(),
        1,
        "the drop is counted rather than passed over"
    );
    observer.events(&mut events);
    assert!(
        events.iter().any(|ev| matches!(
            ev.kind(),
            fixbolt_engine::observe::EventKind::OriginationUndeliverable { count: 1 }
        )),
        "and it reaches an operator: {events:?}"
    );
    assert!(
        drain(&mut peer).is_empty(),
        "and it was not sent to somebody else"
    );
}

#[test]
fn an_origination_and_a_reply_in_one_turn_do_not_corrupt_each_other() {
    struct Echo;
    impl Application for Echo {
        fn on_message(
            &mut self,
            msg: &[u8],
            seq: u32,
            stamp: &[u8],
            out: &mut [u8],
        ) -> Option<Range<usize>> {
            fixbolt_conformance::echo::echo(msg, out, seq, stamp).ok()
        }
    }

    let (mut peer, mut e, id) = engine_with_id(Echo);
    let sender = e.sender();
    let _ = peer.send(&logon());
    e.turn();
    let _ = drain(&mut peer);

    // Both in flight for the same turn: one queued from away, one about to be
    // produced by the handler for a message arriving now.
    assert!(sender.send(id, &away("queued")));
    let order = wire(
        "D",
        2,
        "11=ID-1\x0121=1\x0138=100\x0140=1\x0154=1\x0155=INTC\x0160=20260828-12:00:00.000\x01",
    );
    let _ = peer.send(&order);
    e.turn();

    let out = drain(&mut peer);
    assert_eq!(
        seqs(&out),
        vec![2, 3],
        "the queued message went first — it had been waiting since the previous \
         turn — and the reply followed, each whole: {}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(
        count(&out, b"\x01148=queued\x01"),
        1,
        "the origination is intact"
    );
    assert_eq!(count(&out, b"\x0111=ID-1\x01"), 1, "and so is the echo");
}
