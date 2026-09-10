//! The `hft` front doors, driven through their own front doors.
//!
//! **Step 1, 2 and 4 of [the-hft-front-doors-have-no-gate].**
//!
//! # What was wrong, and it was not coverage
//!
//! `serve_hft`, `serve_hft_with_recovery` and `serve_sharded_hft` are public API
//! of the `fixbolt` crate (`crates/library/src/lib.rs:27, 36, 64`) and
//! `[measured 2026-09-10]` **no test, no bench and no tool called any of them**
//! — a `grep` across `crates/*/tests`, `crates/*/benches`, `benches/` and
//! `tools/` returned three hits and all three were comments.
//!
//! The part that makes it worse than an untested function: **`hft` *is* proven,
//! just not through its own door.** `tools/w2w` builds an `Engine` by hand and
//! picks `wait::Spin` itself (`tools/w2w/src/main.rs:122, 647-664`), and
//! `scripts/check-no-kernel-sleep.sh` traces that binary. So every published
//! `hft` figure describes a hand-assembled engine rather than the function an
//! embedder calls, and nothing had ever established that the two agree.
//!
//! # What this file does NOT prove, stated because ADR-0013 requires it
//!
//! **It does not prove the engine thread never sleeps.** These tests assert that
//! the `hft` entry points bind, serve a session over a kernel socket, and stop
//! when asked. Non-negotiable 4's *"in `hft` mode the engine thread never
//! sleeps in the kernel on the hot path"* is a syscall-level claim that needs
//! `strace` and a tid, which is `scripts/check-no-kernel-sleep.sh` on Linux —
//! step 6 of the plan, and **not done here**.
//!
//! Both halves of that invariant are rules, and a test that proved one while its
//! doc comment implied the other would be the mode-mixing ADR-0013 decision 4
//! forbids. So: this is a **behaviour** gate on three doors. It is not a mode
//! measurement, and no figure in it belongs in `DESIGN.md` §8.
//!
//! # The three reversals, and what the second one settled
//!
//! `[measured 2026-09-10]` these tests were **green on arrival** — the entry
//! points already existed, so there was no red-first step to show and the
//! reversals are the evidence instead.
//!
//! 1. **The `Recovery` answers `None` for everybody.**
//!    `serve_hft_with_recovery_resumes_a_session` goes red on the assertion it
//!    was meant to: `34=1` where `34=9` was wanted. So the resumed number is
//!    read from the wire and not assumed.
//! 2. **`serve_hft` swapped for `serve`.** `serve_hft_serves_a_session_and_stops`
//!    **still passes** — and it must, because both doors serve correctly. That
//!    is not a broken reversal; it is this gate's limitation, demonstrated
//!    rather than asserted. **This file cannot tell `hft` from `standard`.**
//!    What separates them is a syscall on the engine thread, and only
//!    `scripts/check-no-kernel-sleep.sh` on Linux can see it.
//! 3. **`wait_for_event` asked for `EndedWithoutReason`.** It reported
//!    `the stream held [LoggedOn]` — so the helper reads the event stream
//!    rather than returning a blind yes, which is what
//!    `docs/reference/a-reversal-needs-an-input-where-the-answers-differ.md` is
//!    about.
//!
//! [the-hft-front-doors-have-no-gate]: ../../../docs/plans/2026-09-10-the-hft-front-doors-have-no-gate.md

// `serve_hft` needs a poller for the pre-session stage exactly as `serve` does,
// so it does not exist without `standard` on unix — non-negotiable 6. The `mod`
// is gated, not only the manifest, and `[measured 2026-09-02]`
// `engine_recovery.rs` learned the hard way that a `--no-default-features` run
// can pass anyway because cargo unifies features across one invocation:
// `scripts/check-no-optional-deps.sh` asks per crate and is the real gate.
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fixbolt_engine::journal::Store;
use fixbolt_engine::observe::{EventKind, Handles};
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::recovery::{Recovery, Resumed};
use fixbolt_engine::{Application, Config};
use fixbolt_session::journal::Journal;

/// Answers an application message and nothing else.
#[derive(Default)]
struct EchoApp(fixbolt_conformance::echo::Echo);

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        let (seq, stamp) = (hdr.seq, hdr.stamp);
        self.0.reply(msg, seq, stamp, out)
    }
}

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

fn free_addr() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

fn connect(addr: &str) -> TcpStream {
    for _ in 0..500 {
        if let Ok(s) = TcpStream::connect(addr) {
            s.set_nodelay(true).expect("nodelay");
            s.set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            return s;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the hft serving loop never bound {addr}");
}

fn read_one(client: &mut TcpStream) -> String {
    let mut buf = [0u8; 4096];
    let n = client.read(&mut buf).expect("a reply");
    String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|")
}

/// A `Logon` stamped at the wall clock.
///
/// **Stamped now, not from the corpus.** These entry points build a
/// `SystemClock`, so the corpus's fixed instant would be refused by
/// `max_skew_ms` — and a clock refusal and an unknown counterparty are the same
/// silence on the wire, which is what
/// `docs/reference/two-time-rules-share-one-observable.md` records.
fn logon_now(seq: u32) -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = cache.format(now, 0);
    let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
    let inner = format!(
        "35=A\u{1}34={seq}\u{1}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}"
    );
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

/// Wait for `kind` on the stream, or say what did arrive.
///
/// **This is the assertion that separates "it served" from "it did not
/// crash".** A test that only reads a reply off the socket passes against an
/// engine that answered from the pre-session stage and never brought a session
/// up, which is the shape
/// `docs/reference/a-test-that-cannot-fail-reads-as-coverage.md` is about.
fn wait_for_event(handles: &Handles, kind: &EventKind, within: Duration) -> Vec<EventKind> {
    let observer = handles.observer();
    let mut seen = Vec::new();
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        let mut out = Vec::new();
        if observer.events(&mut out) > 0 {
            for e in out {
                seen.push(e.kind());
            }
            if seen.iter().any(|k| k == kind) {
                return seen;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    seen
}

/// **`serve_hft` binds, brings a session up over a kernel socket, and stops.**
///
/// The first thing in this repository to go through that door.
#[test]
fn serve_hft_serves_a_session_and_stops() {
    let addr = free_addr();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();

    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve_hft(
            &serving,
            Table::with_capacity(1).serving(cfg()),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            serving_handles,
        )
    });

    let mut client = connect(&addr);
    let began = Instant::now();
    client.write_all(&logon_now(1)).expect("send the Logon");
    let reply = read_one(&mut client);
    let round_trip = began.elapsed();

    assert!(
        reply.contains("|35=A|"),
        "the hft acceptor answered the Logon: {reply}"
    );
    assert!(
        reply.contains("|34=1|"),
        "and a session nobody resumed starts at one: {reply}"
    );

    let seen = wait_for_event(&handles, &EventKind::LoggedOn, Duration::from_secs(5));
    assert!(
        seen.contains(&EventKind::LoggedOn),
        "the session finished its Logon exchange; the stream held {seen:?}"
    );
    assert_eq!(
        handles.observer().events_lost(),
        0,
        "no event was lost, so the assertion above read the whole stream — ADR-0059"
    );

    // **A spinning engine answers inside a millisecond.** Not a latency figure
    // and not a mode measurement (see the module docs): the bound is three
    // orders of magnitude loose, and it is here for one reason — an engine that
    // ignored readiness and waited out a poll timeout would still satisfy every
    // assertion above. `[measured 2026-08-30]` that exact engine read 0% CPU,
    // was found sleeping 20 times out of 20, and only a round-trip assertion
    // saw it; `scripts/check-standard-gives-the-core-back.sh` needs four
    // assertions for the same reason.
    assert!(
        round_trip < Duration::from_millis(500),
        "the Logon round trip took {round_trip:?} — an hft engine that takes \
         half a second is waiting on something"
    );

    admin.shutdown(2_000);
    let stopped = engine.join().expect("the serving thread did not panic");
    let shutdown = stopped.expect("serve_hft returned a Shutdown rather than an error");
    assert_eq!(
        shutdown.sessions(),
        1,
        "the shutdown counted the session it was serving: {shutdown:?}"
    );
}

/// A `Recovery` that resumes exactly one counterparty and counts the asking.
struct OneCounterparty {
    asked: Arc<AtomicUsize>,
}

impl Recovery<Store> for OneCounterparty {
    fn fresh(&mut self, _cfg: &Config) -> Store {
        Store::default()
    }

    fn recover(&mut self, cfg: &Config) -> Option<Resumed<Store>> {
        self.asked.fetch_add(1, Ordering::Relaxed);
        // Only the counterparty this test is about, so "it answered for
        // everybody" and "it answered for the right one" are different results.
        if !cfg.serves(b"TW44", b"ISLD") {
            return None;
        }
        let mut journal = Store::new();
        journal.mark_in(11);
        Some(Resumed {
            journal,
            next_out: 9,
            next_in: 12,
            last_active_ms: None,
        })
    }
}

/// **`serve_hft_with_recovery` actually resumes**, rather than merely
/// compiling.
///
/// The distinction matters: the seam could be wired to a `Recovery` that is
/// never asked, and every assertion about the socket would still pass. So the
/// `Recovery` counts its own calls, and the wire is asserted to carry the
/// resumed number.
#[test]
fn serve_hft_with_recovery_resumes_a_session() {
    let addr = free_addr();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);

    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve_hft_with_recovery(
            &serving,
            Table::with_capacity(1).serving(cfg()),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            OneCounterparty { asked: counter },
            fixbolt_engine::msglog::NoLog,
            serving_handles,
        )
    });

    let mut client = connect(&addr);
    client.write_all(&logon_now(12)).expect("send the Logon");
    let reply = read_one(&mut client);

    assert!(
        reply.contains("|35=A|"),
        "the hft acceptor answered the Logon: {reply}"
    );
    assert!(
        reply.contains("|34=9|"),
        "and it resumed at nine rather than starting at one: {reply}"
    );
    assert!(
        asked.load(Ordering::Relaxed) >= 1,
        "the hft serving loop asked the Recovery at all — without this the \
         assertion above could pass on a seam nobody called"
    );

    let seen = wait_for_event(&handles, &EventKind::LoggedOn, Duration::from_secs(5));
    assert!(
        seen.contains(&EventKind::LoggedOn),
        "the resumed session finished its Logon exchange; the stream held {seen:?}"
    );

    admin.shutdown(2_000);
    let stopped = engine.join().expect("the serving thread did not panic");
    let shutdown = stopped.expect("serve_hft_with_recovery returned a Shutdown");
    assert_eq!(
        shutdown.sessions(),
        1,
        "the shutdown counted the resumed session: {shutdown:?}"
    );
}
