//! ADR-0170 decision 4: **the exporter allocates nothing per scrape**, counted
//! over the whole process — the engine thread, the exporter thread, and the
//! client doing the scraping.
//!
//! Not non-negotiable 1, which is about the engine thread. Required anyway,
//! because `tools/w2w` — the kill line's instrument — counts every thread in
//! its process and must go on asserting zero with an exporter in it, and
//! because a neighbour thread in `malloc` shares the allocator with the engine.
//! So this counts every allocation anywhere in the process across its window,
//! not only the exporter's: a scrape client written to allocate nothing, an
//! engine turning on its own thread, and the exporter between them.
//!
//! Two cases, each asserting its own path is live — a zero from a path that
//! did nothing is the trap `docs/reference/measured-costs.md` records:
//!
//! - `metrics-idle`: the exporter ticking with its event stream handed over,
//!   no scrape. After the window, a scrape still answers.
//! - `metrics-scraped`: two hundred scrapes, each of which asks the engine for
//!   a fresh snapshot (`min_request_interval` zero, the worst case). Each
//!   answer is a `200` carrying `fixbolt_sessions_logged_on{engine="a"} 1`,
//!   and `published()` moves by at least one per scrape.
//!
//! # The `unsafe` here
//!
//! Identical to `crates/engine/benches/alloc.rs` and sound for the same three
//! reasons: every method forwards to `System` unchanged but for a relaxed
//! counter; this is a benchmark binary, so nothing ships it; and it is proven
//! by reversal (plan step 4, R6), not by reading.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(unsafe_code)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};
use fixbolt_metrics::Exporter;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System`, which is a correct allocator, with
// the same pointer, layout and size it was given. The only addition is a
// relaxed counter increment. See the module comment.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

struct Silent;

impl Application for Silent {
    fn on_message(
        &mut self,
        _: &[u8],
        _: fixbolt_session::Header<'_>,
        _: &mut [u8],
    ) -> Option<Range<usize>> {
        None
    }
}

fn count<F: FnOnce()>(f: F) -> usize {
    let before = ALLOCS.load(Ordering::Relaxed);
    f();
    ALLOCS.load(Ordering::Relaxed) - before
}

/// Where a scrape's answer lands: boxed once before any window, so the client
/// allocates nothing either.
struct Reply {
    buf: [u8; 1 << 18],
    len: usize,
}

/// One `GET /metrics`, allocating nothing: a fixed request, a fixed buffer,
/// and a search by `windows`. Returns whether the reply is a `200` whose body
/// holds `needle`.
fn scrape(addr: SocketAddr, reply: &mut Reply, needle: &[u8]) -> bool {
    let Ok(mut s) = TcpStream::connect(addr) else {
        return false;
    };
    if s.set_read_timeout(Some(Duration::from_secs(5))).is_err()
        || s.write_all(b"GET /metrics HTTP/1.1\r\nHost: bench\r\n\r\n")
            .is_err()
    {
        return false;
    }
    reply.len = 0;
    loop {
        match s.read(&mut reply.buf[reply.len..]) {
            Ok(0) => break,
            Ok(n) => reply.len += n,
            Err(_) => return false,
        }
        if reply.len == reply.buf.len() {
            return false;
        }
    }
    let got = &reply.buf[..reply.len];
    got.starts_with(b"HTTP/1.1 200 OK\r\n") && got.windows(needle.len()).any(|w| w == needle)
}

fn main() {
    // An engine with one logged-on session, turning on its own thread — built
    // there, so nothing about it has to be `Send`.
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let flag = Arc::clone(&stop);
    let engine = std::thread::spawn(move || {
        let mut engine: Engine<
            Loopback,
            fixbolt_session::Acceptor,
            InlineDispatch<Silent>,
            ManualClock,
            Yield,
            Store,
            256,
            4096,
            8192,
        > = Engine::new(
            Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
            InlineDispatch::new(Silent),
            ManualClock::at(FIXED_TIME_MILLIS),
            Yield,
            4,
        );
        let observer = engine.observer();
        let (mut peer, side) = Loopback::pair();
        let _ = engine.add(side);
        let body =
            "35=A\x0134=1\x0149=TW44\x0152=20260828-12:00:00.000\x0156=ISLD\x0198=0\x01108=30\x01";
        let logon = with_real_checksum(
            format!("8=FIX.4.4\x019={}\x01{body}10=0\x01", body.len()).as_bytes(),
        );
        let _ = peer.send(&logon);
        engine.turn();
        let mut sink = [0u8; 4096];
        while let Io::Ready(_) = peer.recv(&mut sink) {}
        tx.send(observer).expect("main is waiting");
        while !flag.load(Ordering::Relaxed) {
            engine.turn();
            std::thread::sleep(Duration::from_micros(50));
        }
        drop(peer);
    });
    let observer = rx.recv().expect("the engine started");

    let events_seen = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&events_seen);
    let exporter = Exporter::builder("127.0.0.1:0".parse().expect("an address"))
        .engine("a", observer.clone())
        // The worst case: every scrape asks, so every scrape builds.
        .min_request_interval(Duration::ZERO)
        .tick(Duration::from_millis(1))
        .with_events(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        })
        .spawn()
        .expect("the exporter starts");
    let addr = exporter.local_addr();

    let mut reply = Box::new(Reply {
        buf: [0; 1 << 18],
        len: 0,
    });
    let needle = b"\nfixbolt_sessions_logged_on{engine=\"a\"} 1\n";

    // Warm-up, outside every window: the first connect, the first accept and
    // the first snapshot each touch something once.
    for _ in 0..5 {
        assert!(scrape(addr, &mut reply, needle), "warm-up scrape answered");
    }
    assert!(
        events_seen.load(Ordering::Relaxed) >= 1,
        "the exporter drained the engine's `LoggedOn`: its event path is live"
    );

    // --- metrics-idle: ticking, draining, nobody scraping -------------------
    let idle_allocs = count(|| std::thread::sleep(Duration::from_millis(500)));
    assert!(
        scrape(addr, &mut reply, needle),
        "metrics-idle: the exporter still answers after the window"
    );

    // --- metrics-scraped: two hundred scrapes, each building a snapshot -----
    let published_before = observer.published();
    let mut answered = 0usize;
    let scraped_allocs = count(|| {
        for _ in 0..200 {
            if scrape(addr, &mut reply, needle) {
                answered += 1;
            }
        }
    });
    let built = observer.published() - published_before;
    assert_eq!(
        answered, 200,
        "metrics-scraped: every scrape a 200 with the logged-on session in it"
    );
    assert!(
        built >= 200,
        "metrics-scraped: every scrape asked and the engine built {built} — the window \
         must have walked the snapshot path it counted"
    );

    exporter.stop();
    stop.store(true, Ordering::Relaxed);
    engine.join().expect("the engine thread did not panic");

    println!("allocations: metrics-idle {idle_allocs} metrics-scraped {scraped_allocs}");
    assert_eq!(
        [idle_allocs, scraped_allocs],
        [0; 2],
        "ADR-0170 decision 4: the exporter allocates nothing per scrape, on any thread"
    );
}
