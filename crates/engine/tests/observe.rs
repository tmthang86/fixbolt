//! What an operator can see, from another thread, while the engine runs.
//!
//! Step 1 of [operability], and it is written to be **red**: `[verified
//! 2026-09-01]` an `Engine`'s entire observable surface is `connections()`,
//! `sources_missing()` and `refused_connections()` — three numbers, none of them
//! about a session. There is no state, no `next_out`/`next_in`, no measured
//! clock skew, and no way to ask from anywhere but the engine's own thread.
//!
//! # Why the read happens while the engine is turning
//!
//! A test that stops the engine and then reads its state passes against a
//! mechanism that is not thread-safe at all, and proves nothing about the one
//! property this whole design exists for. So the engine runs on its own thread
//! here and the assertions are made from the test's thread while it turns.
//!
//! That makes this the timing-sensitive sibling of a deterministic gate, which
//! is why every wait is bounded and reports what it saw rather than hanging.
//!
//! # Why clock skew is asserted and not merely present
//!
//! `max_skew_ms` silently refuses a message whose `SendingTime` is too far from
//! the engine's clock, and **nothing today would say why**. On a box whose NTP
//! has drifted, a counterparty simply stops working. The measured skew is the
//! one field in this snapshot that answers a 3 a.m. question, so it gets its own
//! assertion rather than riding along.
//!
//! [operability]: ../../../docs/plans/2026-09-01-operability.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, load_all};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::presession::is_logon;
use fixbolt_engine::transport::TcpTransport;
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Acceptor, Application, Config, Engine};

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

/// A real, in-sequence `Logon` from the acceptance corpus.
fn corpus_logon() -> Vec<u8> {
    load_all()
        .expect("the corpus is fetched — scripts/fetch-quickfix-assets.sh")
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m)
                if is_logon(&m.wire)
                    && field(&m.wire, b"34=") == Some(&b"1"[..])
                    && field(&m.wire, b"49=") == Some(&b"TW44"[..])
                    && field(&m.wire, b"56=") == Some(&b"ISLD"[..])
                    && field(&m.wire, b"8=") == Some(&b"FIX.4.4"[..]) =>
            {
                Some(m.wire)
            }
            _ => None,
        })
        .expect("the corpus sends a well-formed FIX.4.4 Logon from TW44")
}

fn field<'a>(msg: &'a [u8], tag: &[u8]) -> Option<&'a [u8]> {
    msg.split(|b| *b == 1).find_map(|f| f.strip_prefix(tag))
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
        Self::spawn(None)
    }

    /// The same engine, given a cell that existed before it did.
    fn start_adopting(handles: &fixbolt_engine::observe::Handles) -> Self {
        Self::spawn(Some(handles))
    }

    fn spawn(adopt: Option<&fixbolt_engine::observe::Handles>) -> Self {
        let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
        let addr = acceptor.local_addr().expect("bound").to_string();
        let mut engine: Acc = Engine::new(
            Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
            InlineDispatch::new(EchoApp::default()),
            // The corpus's fixed instant, so a SendingTime two days out is not
            // refused for skew — and so the skew this test asserts is a number
            // the test chose rather than whatever the wall clock happened to be.
            ManualClock::at(FIXED_TIME_MILLIS),
            Yield,
            4,
        );
        if let Some(h) = adopt {
            assert!(engine.adopt(h), "a fresh engine has no cell of its own yet");
        }
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

    /// Poll the observer until `f` is satisfied, or give up and say what was
    /// last seen. Bounded: a test that hangs reports nothing.
    fn wait_for<F>(&self, what: &str, mut f: F) -> fixbolt_engine::observe::Snapshot
    where
        F: FnMut(&fixbolt_engine::observe::Snapshot) -> bool,
    {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut last = None;
        while Instant::now() < deadline {
            if let Some(s) = self.observer.request() {
                if f(&s) {
                    return s;
                }
                last = Some(s);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("timed out waiting for {what}; last snapshot: {last:?}");
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

/// **The specification.** An operator, on another thread, while the engine runs.
///
/// Everything asserted here is a question somebody asks at 3 a.m. and cannot
/// answer today: is this session logged on, what sequence numbers does it hold,
/// and how far is our clock from theirs.
#[test]
fn an_operator_sees_session_state_from_another_thread() {
    let engine = Running::start();

    // Nothing connected yet: the snapshot exists and is empty.
    let empty = engine.wait_for("an empty snapshot", |s| s.sessions().is_empty());
    assert_eq!(empty.connections(), 0);
    assert!(!empty.truncated(), "no sessions cannot be truncated");

    let mut client = TcpStream::connect(&engine.addr).expect("connect");
    client.set_nodelay(true).expect("nodelay");
    client.write_all(&corpus_logon()).expect("send the Logon");

    let s = engine.wait_for("the session to log on", |s| {
        s.sessions().first().is_some_and(|x| x.logged_on())
    });

    assert_eq!(s.connections(), 1, "one connection: {s:?}");
    let sess = s.sessions().first().expect("one session");

    assert!(sess.logged_on(), "it answered the Logon: {sess:?}");
    assert_eq!(
        sess.next_in(),
        2,
        "the Logon was 34=1, so the next inbound expected is 2: {sess:?}"
    );
    assert_eq!(
        sess.next_out(),
        2,
        "the engine sent its own Logon as 34=1: {sess:?}"
    );

    // The corpus's Logon carries the same instant this engine's clock reads, so
    // the measured skew is zero — and zero is a value, not an absence.
    assert_eq!(
        sess.last_skew_ms(),
        Some(0),
        "SendingTime matched the engine's clock exactly: {sess:?}"
    );

    // And the reply really did come back, so the state above describes a session
    // that worked rather than one that merely exists.
    let mut buf = [0u8; 1024];
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let n = client.read(&mut buf).expect("a reply");
    assert!(
        field(&buf[..n], b"35=") == Some(&b"A"[..]),
        "the acceptor answered with a Logon: {}",
        String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|")
    );
}

/// A snapshot is only produced when somebody asks for one.
///
/// **This is the assertion that keeps "on demand" honest.** Without it, an
/// implementation that publishes on every single turn passes every other test in
/// this file — and pays for an operator who is not there, on the hot path, which
/// is the one thing this design exists to avoid.
#[test]
fn the_engine_publishes_nothing_until_it_is_asked() {
    let engine = Running::start();

    // The engine has been turning for a moment with nobody asking.
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        engine.observer.published(),
        0,
        "no request has been made, so no snapshot should have been built"
    );

    // Ask exactly once, and then never touch `request` again: it is what raises
    // the flag, so polling with it would keep asking and the count below would
    // rise for the right reason and prove nothing.
    let _ = engine.observer.request();
    let deadline = Instant::now() + Duration::from_secs(5);
    while engine.observer.published() == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let once = engine.observer.published();
    assert_eq!(once, 1, "one request, one snapshot");

    // The engine keeps turning. Nobody is asking.
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        engine.observer.published(),
        once,
        "and it went back to publishing nothing: {once} then {}",
        engine.observer.published()
    );
}

/// **The specification for `STATUS.md` item 47, at the seam under the front
/// door.**
///
/// Everything above is written the way the whole `observe` module was tested:
/// build an `Engine`, ask it for a handle, drive it. A caller who comes through
/// `serve` never holds an `Engine` and can never make that call — which is why
/// item 30 has been finished and unreachable since it landed. So the handle has
/// to be able to exist **before** the engine does.
///
/// This is that seam, one layer below `serve`: the cell is made first, the
/// engine adopts it, and what the operator reads was never asked of the engine
/// at all.
#[test]
fn a_handle_made_before_the_engine_sees_its_first_logon() {
    // Made first. There is no engine at this point, and on the front-door path
    // there never will be one the caller can name.
    let handles = fixbolt_engine::observe::Handles::new();
    let observer = handles.observer();

    let engine = Running::start_adopting(&handles);

    let mut client = TcpStream::connect(&engine.addr).expect("connect");
    client.set_nodelay(true).expect("nodelay");
    client.write_all(&corpus_logon()).expect("send the Logon");

    // Read through the handle made before the engine, not through
    // `Running::observer`, which is the one the engine handed out.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last = None;
    let s = loop {
        if let Some(s) = observer.request() {
            if s.sessions().first().is_some_and(|x| x.logged_on()) {
                break s;
            }
            last = Some(s);
        }
        assert!(
            Instant::now() < deadline,
            "the pre-made handle never saw the session log on; last snapshot: {last:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    };

    let sess = s.sessions().first().expect("one session");
    assert!(sess.logged_on(), "it answered the Logon: {sess:?}");
    assert_eq!(sess.next_out(), 2, "the acceptor's own Logon was 34=1");
}

/// Two cells on one engine are two truths, so the second one is refused.
///
/// And the assertion that matters is not the `false` — it is that the engine
/// **publishes into the cell it was given**. An `adopt` that quietly made its
/// own cell would leave every reading above empty forever, with nothing to say
/// why.
#[test]
fn an_engine_publishes_into_the_cell_it_was_given() {
    let handles = fixbolt_engine::observe::Handles::new();
    let mut engine: Acc = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(EchoApp::default()),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    assert!(engine.adopt(&handles), "an engine with no cell takes one");
    assert!(
        !engine.adopt(&fixbolt_engine::observe::Handles::new()),
        "a second cell is refused rather than replacing the first"
    );

    // Ask through the pre-made handle, turn once, and the answer must have been
    // published into that cell.
    assert_eq!(handles.observer().published(), 0, "nothing asked yet");
    let _ = handles.observer().request();
    engine.turn();
    assert_eq!(
        handles.observer().published(),
        1,
        "the engine published into the caller's cell, not one of its own"
    );

    // And the three older methods find the adopted cell rather than making a
    // second one — the same number, read through the engine's own handle.
    assert_eq!(
        engine.observer().published(),
        1,
        "`Engine::observer` hands out a handle onto the adopted cell"
    );
}
