//! What an operator learns about a connection that ended, from another thread.
//!
//! Step 3 of [why-a-connection-ended]. `STATUS.md` item 30 (d).
//!
//! # Why the read happens while the engine is turning
//!
//! Same reason as `tests/observe.rs`: a test that stops the engine and then
//! reads passes against a mechanism that is not thread-safe at all, and proves
//! nothing about the one property this design exists for.
//!
//! # Why losses are asserted and not merely counted
//!
//! An event stream that loses silently is worse than no event stream — it is a
//! source an operator will trust and should not. So the loss counter has its
//! own test, and a full ring is driven on purpose rather than hoped against.
//!
//! [why-a-connection-ended]: ../../../docs/plans/2026-09-02-why-a-connection-ended.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::net::TcpStream;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::observe::{EVENT_CAPACITY, Event, EventKind};
use fixbolt_engine::transport::TcpTransport;
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Acceptor, Application, Config, Engine};
use fixbolt_session::DropReason;

const N: usize = 256;
const RX: usize = 4096;
const TX: usize = 8192;

type Acc = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    ManualClock,
    Yield,
    Store,
    N,
    RX,
    TX,
>;

#[derive(Default)]
struct EchoApp(fixbolt_conformance::echo::Echo);

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        self.0.reply(msg, seq, stamp, out)
    }
}

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// A FIX.4.4 message from `TW44` at the corpus's fixed instant, `9=` and `10=`
/// computed. `begin` lets a test send a deliberately wrong `BeginString`.
fn message(begin: &str, body: &str) -> Vec<u8> {
    let stamp = fixbolt_conformance::script::FIXED_TIME_IN;
    let cut = body.find('\u{1}').expect("a first field") + 1;
    let (head, rest) = body.split_at(cut);
    let inner = format!("{head}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}{rest}");
    let framed = format!("8={begin}\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    with_real_checksum(framed.as_bytes())
}

fn good_logon() -> Vec<u8> {
    message("FIX.4.4", "35=A\u{1}34=1\u{1}98=0\u{1}108=30\u{1}")
}

/// An engine on its own thread, and the handle an operator would hold.
struct Running {
    addr: String,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    observer: fixbolt_engine::observe::Observer,
}

impl Running {
    fn start() -> Self {
        let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
        let addr = acceptor.local_addr().expect("bound").to_string();
        let mut engine: Acc = Engine::new(
            cfg(),
            InlineDispatch::new(EchoApp::default()),
            ManualClock::at(FIXED_TIME_MILLIS),
            Yield,
            8,
        );
        let observer = engine.observer();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                while let Some(t) = acceptor.accept() {
                    engine.add(t);
                }
                if !engine.turn() {
                    std::thread::yield_now();
                }
            }
        });
        Self {
            addr,
            stop,
            handle: Some(handle),
            observer,
        }
    }

    /// Collect events until `f` is satisfied, or give up and say what was seen.
    fn wait_for<F>(&self, what: &str, mut f: F) -> Vec<Event>
    where
        F: FnMut(&[Event]) -> bool,
    {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            self.observer.events(&mut seen);
            if f(&seen) {
                return seen;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("timed out waiting for {what}; saw: {seen:?}");
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// **The specification.** Two connections end for two different reasons, and an
/// operator on another thread can tell which was which.
#[test]
fn an_operator_learns_why_each_connection_ended() {
    let engine = Running::start();

    // **No logged-on session first, and that is not an accident.** `[measured
    // 2026-09-02]` the first version of this test opened a good session before
    // these two, and ADR-0030's single-logon rule refused both of them as
    // duplicates of it — before either message was ever judged. Both reported
    // the refusal and neither reported its own fault: correct behaviour, and a
    // test measuring something other than its name.
    //
    // One with the wrong FIX version, one pointed at the wrong counterparty.
    let mut bad_version = TcpStream::connect(&engine.addr).expect("connect");
    bad_version
        .write_all(&message(
            "FIX.4.2",
            "35=A\u{1}34=1\u{1}98=0\u{1}108=30\u{1}",
        ))
        .expect("send");
    let mut bad_sender = TcpStream::connect(&engine.addr).expect("connect");
    bad_sender
        .write_all(&{
            let m = message("FIX.4.4", "35=A\u{1}34=1\u{1}98=0\u{1}108=30\u{1}");
            let s = String::from_utf8(m).expect("ascii");
            with_real_checksum(s.replace("49=TW44", "49=NOPE").as_bytes())
        })
        .expect("send");

    let seen = engine.wait_for("both endings", |e| {
        e.iter()
            .filter(|x| matches!(x.kind(), EventKind::Ended(_)))
            .count()
            >= 2
    });

    let reasons: Vec<DropReason> = seen
        .iter()
        .filter_map(|e| match e.kind() {
            EventKind::Ended(r) => Some(r),
            _ => None,
        })
        .collect();

    assert!(
        reasons.contains(&DropReason::WrongBeginString),
        "one of them was on the wrong FIX version: {reasons:?}"
    );
    assert!(
        reasons.contains(&DropReason::WrongSenderCompId),
        "the other was pointed at the wrong counterparty: {reasons:?}"
    );
    assert!(
        !reasons.contains(&DropReason::SendingTimeOutOfRange),
        "and neither was a clock problem, which is the answer a wire-only \
         observer would have had to guess at: {reasons:?}"
    );
    assert!(
        !seen
            .iter()
            .any(|e| e.kind() == EventKind::EndedWithoutReason),
        "every ending named itself: {seen:?}"
    );
    assert_eq!(
        engine.observer.events_lost(),
        0,
        "and nothing was lost on the way"
    );
}

/// Every event carries the connection it is about, so two counterparties are
/// two stories rather than one.
#[test]
fn events_name_the_connection_they_belong_to() {
    let engine = Running::start();
    let mut a = TcpStream::connect(&engine.addr).expect("connect");
    a.write_all(&good_logon()).expect("send");
    let mut b = TcpStream::connect(&engine.addr).expect("connect");
    b.write_all(&message(
        "FIX.4.2",
        "35=A\u{1}34=1\u{1}98=0\u{1}108=30\u{1}",
    ))
    .expect("send");

    let seen = engine.wait_for("one of each", |e| {
        e.iter().any(|x| x.kind() == EventKind::LoggedOn)
            && e.iter().any(|x| matches!(x.kind(), EventKind::Ended(_)))
    });

    let on = seen
        .iter()
        .find(|e| e.kind() == EventKind::LoggedOn)
        .expect("a logon");
    let off = seen
        .iter()
        .find(|e| matches!(e.kind(), EventKind::Ended(_)))
        .expect("an ending");
    assert_ne!(on.id(), off.id(), "two connections, two ids: {seen:?}");
    assert!(on.at_ms() > 0, "and an instant: {on:?}");
}

/// **The loss counter, driven rather than hoped for.** More events than the
/// ring holds, with nobody reading, and the count must say so.
///
/// Without this, `events_lost` is a number that is always zero in the tests and
/// always trusted in production.
#[test]
fn a_reader_that_falls_behind_is_told_how_much_it_missed() {
    let engine = Running::start();

    // Every one of these is refused before a session exists, so each is one
    // `Ended` event and nothing else.
    let bad = message("FIX.4.2", "35=A\u{1}34=1\u{1}98=0\u{1}108=30\u{1}");
    let want = EVENT_CAPACITY + 32;
    for _ in 0..want {
        if let Ok(mut c) = TcpStream::connect(&engine.addr) {
            let _ = c.write_all(&bad);
        }
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    while engine.observer.events_lost() == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        engine.observer.events_lost() > 0,
        "{want} events into a ring of {EVENT_CAPACITY} with nobody reading, and \
         the engine reported no loss — which would make the counter a decoration"
    );

    // And the ring still hands back what it did keep.
    let mut buf = Vec::new();
    let n = engine.observer.events(&mut buf);
    assert!(n > 0, "a full ring is still readable");
}

/// **A policy decision must not be reported as a network fault.**
///
/// ADR-0030's single-logon rule refuses a second connection from a counterparty
/// that is already on. `[measured 2026-09-02]` the engine reported that as
/// `TransportClosed` — blaming the socket for a decision it made itself, and
/// sending whoever is on call to look at the wrong layer. It was found by the
/// test above failing for a reason that had nothing to do with what it was
/// testing.
#[test]
fn a_duplicate_identity_says_so_rather_than_blaming_the_socket() {
    let engine = Running::start();

    let mut first = TcpStream::connect(&engine.addr).expect("connect");
    first.set_nodelay(true).expect("nodelay");
    first.write_all(&good_logon()).expect("send");
    engine.wait_for("the first session", |e| {
        e.iter().any(|x| x.kind() == EventKind::LoggedOn)
    });

    // The same counterparty, again, while the first is still on.
    let mut second = TcpStream::connect(&engine.addr).expect("connect");
    second.write_all(&good_logon()).expect("send");

    let seen = engine.wait_for("the refusal", |e| {
        e.iter().any(|x| matches!(x.kind(), EventKind::Ended(_)))
    });
    let reasons: Vec<DropReason> = seen
        .iter()
        .filter_map(|e| match e.kind() {
            EventKind::Ended(r) => Some(r),
            _ => None,
        })
        .collect();

    assert!(
        reasons.contains(&DropReason::DuplicateIdentity),
        "the engine refused it under its own rule and must say so: {reasons:?}"
    );
    assert!(
        !reasons.contains(&DropReason::TransportClosed),
        "and must not blame the network for it: {reasons:?}"
    );
}
