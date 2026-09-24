//! The exporter, from outside: a real engine, a real socket, a real HTTP
//! request, and the bytes that come back.
//!
//! Step 3 of `docs/plans/2026-09-24-p4-metrics-exporter.md`, ADR-0170. Every
//! test here is a behaviour a Prometheus server or an operator relies on and
//! that no type can hold: the exact `Content-Type`, the ceiling on how often
//! the engine is asked, a stale snapshot reported as stale, a series with no
//! source left out rather than printed as zero, and the event stream left to
//! its owner unless the caller hands it over.
//!
//! # Why most engines here are built by hand
//!
//! A hand-built engine turns only when told, which is what makes "the engine
//! is asleep" and "the engine answers at once" two states a test can hold.
//! [`Turning`] is the one that answers at once — it turns on its own thread
//! continuously — and is what the scrape-storm test needs, because the ceiling
//! it proves is the exporter's, not the engine's wake-up interval.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::{Dispatch, InlineDispatch, RingDispatch};
use fixbolt_engine::journal::Store;
use fixbolt_engine::observe::{EventKind, Observer};
use fixbolt_engine::ring;
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};
use fixbolt_metrics::Exporter;

/// The exact header Prometheus 3 parses as the classic text format. One
/// character different and it falls back or refuses the scrape.
const CONTENT_TYPE: &str = "Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n";

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

fn logon() -> Vec<u8> {
    let body =
        "35=A\x0134=1\x0149=TW44\x0152=20260828-12:00:00.000\x0156=ISLD\x0198=0\x01108=30\x01";
    with_real_checksum(format!("8=FIX.4.4\x019={}\x01{body}10=0\x01", body.len()).as_bytes())
}

fn drain(peer: &mut Loopback) {
    let mut buf = [0u8; 4096];
    while let Io::Ready(_) = peer.recv(&mut buf) {}
}

type Wired<D> =
    Engine<Loopback, fixbolt_session::Acceptor, D, ManualClock, Yield, Store, 256, 4096, 8192>;

/// A hand-built engine and its observer, **before** anything is added — so an
/// observer exists when the first event is emitted.
fn bare<D: Dispatch>(dispatch: D) -> (Wired<D>, Observer) {
    let mut engine: Wired<D> = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        dispatch,
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let observer = engine.observer();
    (engine, observer)
}

/// The same, holding one logged-on session. The peer is returned so the
/// session is not closed under the test.
fn logged_on<D: Dispatch>(dispatch: D) -> (Wired<D>, Observer, Loopback) {
    let (mut engine, observer) = bare(dispatch);
    let (mut peer, side) = Loopback::pair();
    let _ = engine.add(side);
    let _ = peer.send(&logon());
    engine.turn();
    drain(&mut peer);
    (engine, observer, peer)
}

/// An engine on its own thread that turns continuously, so it publishes on the
/// first turn after any request.
struct Turning {
    observer: Observer,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Turning {
    fn start(with_session: bool) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let (mut engine, observer) = bare(InlineDispatch::new(Silent));
            let mut peer = None;
            if with_session {
                let (mut p, side) = Loopback::pair();
                let _ = engine.add(side);
                let _ = p.send(&logon());
                engine.turn();
                drain(&mut p);
                peer = Some(p);
            }
            tx.send(observer).expect("the test is waiting");
            while !flag.load(Ordering::Relaxed) {
                engine.turn();
                std::thread::sleep(Duration::from_micros(100));
            }
            drop(peer);
        });
        let observer = rx.recv().expect("the engine thread started");
        Self {
            observer,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Turning {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn any_port() -> SocketAddr {
    "127.0.0.1:0".parse().expect("an address")
}

/// Send `request` whole, read until the exporter closes, and split the reply.
fn exchange(addr: SocketAddr, request: &[u8]) -> (String, String) {
    let mut s = TcpStream::connect(addr).expect("connect to the exporter");
    s.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("a timeout");
    s.write_all(request).expect("send the request");
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).expect("read the reply");
    let text = String::from_utf8(raw).expect("the reply is text");
    let (head, body) = text
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no end of headers in {text:?}"));
    (format!("{head}\r\n"), body.to_owned())
}

fn get(addr: SocketAddr, path: &str) -> (String, String) {
    exchange(
        addr,
        format!("GET {path} HTTP/1.1\r\nHost: fixbolt\r\nAccept: text/plain\r\n\r\n").as_bytes(),
    )
}

/// The value of the one sample line that starts with `series` (name and
/// labels, exactly as printed), or `None` when there is no such line.
fn sample<'a>(body: &'a str, series: &str) -> Option<&'a str> {
    body.lines()
        .find_map(|l| l.strip_prefix(series)?.strip_prefix(' '))
}

#[test]
fn the_content_type_is_exactly_the_one_prometheus_3_parses() {
    let (_engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let exporter = Exporter::builder(any_port())
        .engine("a", observer)
        .fresh_wait(Duration::from_millis(5))
        .spawn()
        .expect("the exporter starts");

    let (head, body) = get(exporter.local_addr(), "/metrics");
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
    assert!(
        head.contains(CONTENT_TYPE),
        "the Content-Type must be exactly {CONTENT_TYPE:?}, byte for byte: {head}"
    );
    assert!(
        head.contains(&format!("Content-Length: {}\r\n", body.len())),
        "a Content-Length that matches the body: {head}"
    );
    assert!(head.contains("Connection: close\r\n"), "{head}");

    let (head, body) = exchange(
        exporter.local_addr(),
        b"HEAD /metrics HTTP/1.1\r\nHost: fixbolt\r\n\r\n",
    );
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
    assert!(head.contains(CONTENT_TYPE), "HEAD says the same: {head}");
    assert!(body.is_empty(), "HEAD carries no body: {body:?}");
    exporter.stop();
}

#[test]
fn a_scrape_before_the_first_snapshot_says_so() {
    // Never turned: the engine has published nothing, and nothing the exporter
    // does will make it.
    let (_engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let exporter = Exporter::builder(any_port())
        .engine("a", observer.clone())
        .fresh_wait(Duration::from_millis(5))
        .spawn()
        .expect("the exporter starts");
    let (head, body) = get(exporter.local_addr(), "/metrics");
    exporter.stop();

    assert_eq!(observer.published(), 0, "nothing was published");
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
    assert_eq!(
        sample(&body, "fixbolt_snapshot_available{engine=\"a\"}"),
        Some("0"),
        "{body}"
    );
    assert_eq!(
        sample(&body, "fixbolt_snapshots_published_total{engine=\"a\"}"),
        Some("0"),
        "{body}"
    );
    assert!(
        !body.contains("fixbolt_connections"),
        "no snapshot, so no number read off one — not a zero: {body}"
    );
    assert!(
        !body.contains("fixbolt_snapshot_age_seconds"),
        "no snapshot was ever seen, so it has no age: {body}"
    );
}

/// Fifty scrapers at once, ten scrapes each, against an engine that would
/// answer every request at once. **Concurrent, because that is what a storm
/// is**: the exporter answers every connection waiting in the backlog on each
/// wake, so one client scraping in a loop is paced by `tick` — about ten a
/// second — and would prove nothing about the ceiling.
#[test]
fn a_scrape_storm_builds_at_most_one_snapshot_per_interval() {
    const CLIENTS: usize = 50;
    const EACH: usize = 10;

    let engine = Turning::start(true);
    let exporter = Exporter::builder(any_port())
        .engine("a", engine.observer.clone())
        .spawn()
        .expect("the exporter starts");
    let addr = exporter.local_addr();

    let before = engine.observer.published();
    let began = Instant::now();
    let clients: Vec<_> = (0..CLIENTS)
        .map(|_| {
            std::thread::spawn(move || {
                for _ in 0..EACH {
                    let (head, _) = get(addr, "/metrics");
                    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
                }
            })
        })
        .collect();
    for c in clients {
        c.join().expect("a scraper did not panic");
    }
    let took = began.elapsed();
    let built = engine.observer.published() - before;
    exporter.stop();

    let scrapes = CLIENTS * EACH;
    // One per 100 ms interval over the window, plus the one at its start.
    let ceiling = took.as_millis() as u64 / 100 + 1;
    assert!(
        took < Duration::from_secs(3),
        "{scrapes} scrapes took {took:?}: the exporter is not keeping up"
    );
    assert!(
        built <= ceiling,
        "{scrapes} scrapes in {took:?} made the engine build {built} snapshots; \
         `min_request_interval` allows at most {ceiling}"
    );
    assert!(built >= 1, "and it did ask: {built}");
    println!("storm: {scrapes} scrapes in {took:?}, {built} snapshots built, ceiling {ceiling}");
}

#[test]
fn the_snapshot_age_grows_while_the_engine_sleeps() {
    let (mut engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let exporter = Exporter::builder(any_port())
        .engine("a", observer.clone())
        .min_request_interval(Duration::from_millis(1))
        .fresh_wait(Duration::from_millis(5))
        .spawn()
        .expect("the exporter starts");
    let addr = exporter.local_addr();

    // One snapshot, built while the test holds the engine, and then the engine
    // is left alone: an asleep `standard` engine, as the exporter sees it.
    let _ = observer.request();
    engine.turn();
    assert_eq!(observer.published(), 1);

    let age = |body: &str| -> f64 {
        sample(body, "fixbolt_snapshot_age_seconds{engine=\"a\"}")
            .unwrap_or_else(|| panic!("an age: {body}"))
            .parse()
            .expect("a number")
    };
    let (_, first) = get(addr, "/metrics");
    assert_eq!(
        sample(&first, "fixbolt_snapshot_available{engine=\"a\"}"),
        Some("1"),
        "{first}"
    );
    let young = age(&first);
    std::thread::sleep(Duration::from_millis(400));
    let (_, second) = get(addr, "/metrics");
    let old = age(&second);
    exporter.stop();

    assert!(young < 0.2, "just seen: {young}");
    assert!(
        old >= young + 0.3,
        "the engine built nothing for 400 ms, so the age must say so: {young} then {old}"
    );
    assert_eq!(
        observer.published(),
        1,
        "the exporter asked, and an asleep engine was not woken to answer"
    );
}

#[test]
fn healthz_follows_snapshot_healthy() {
    let serving = Turning::start(true);
    let empty = Turning::start(false);
    let up = Exporter::builder(any_port())
        .engine("up", serving.observer.clone())
        .spawn()
        .expect("the exporter starts");
    let down = Exporter::builder(any_port())
        .engine("down", empty.observer.clone())
        .spawn()
        .expect("the exporter starts");

    let (head, _) = get(up.local_addr(), "/healthz");
    assert!(
        head.starts_with("HTTP/1.1 200 OK\r\n"),
        "one logged-on session, nothing broken: {head}"
    );
    let (head, _) = get(down.local_addr(), "/healthz");
    assert!(
        head.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
        "an engine serving nobody is not healthy (`Snapshot::healthy`): {head}"
    );
    let (_, body) = get(down.local_addr(), "/metrics");
    assert_eq!(
        sample(&body, "fixbolt_healthy{engine=\"down\"}"),
        Some("0"),
        "{body}"
    );
    up.stop();
    down.stop();
}

#[test]
fn other_paths_and_methods_are_refused_and_counted() {
    let (_engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let exporter = Exporter::builder(any_port())
        .engine("a", observer)
        .fresh_wait(Duration::from_millis(5))
        .spawn()
        .expect("the exporter starts");
    let addr = exporter.local_addr();

    let (head, _) = get(addr, "/");
    assert!(head.starts_with("HTTP/1.1 404 Not Found\r\n"), "{head}");
    let (head, _) = exchange(addr, b"POST /metrics HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(
        head.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"),
        "{head}"
    );
    assert!(head.contains("Allow: GET, HEAD\r\n"), "{head}");
    let (head, _) = exchange(addr, b"nonsense\r\n\r\n");
    assert!(head.starts_with("HTTP/1.1 400 Bad Request\r\n"), "{head}");

    let (_, body) = get(addr, "/metrics");
    exporter.stop();
    assert_eq!(
        sample(&body, "fixbolt_exporter_bad_requests_total"),
        Some("3"),
        "{body}"
    );
    assert_eq!(
        sample(&body, "fixbolt_exporter_scrapes_total"),
        Some("1"),
        "this scrape counts itself: {body}"
    );
}

#[test]
fn a_request_larger_than_4_kib_is_refused_not_buffered() {
    let (_engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let exporter = Exporter::builder(any_port())
        .engine("a", observer)
        .fresh_wait(Duration::from_millis(5))
        .spawn()
        .expect("the exporter starts");
    let addr = exporter.local_addr();

    // A header block that never ends inside 4 KiB.
    let mut huge = b"GET /metrics HTTP/1.1\r\nX-Filler: ".to_vec();
    huge.resize(5 * 1024, b'a');
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("a timeout");
    s.write_all(&huge).expect("send");
    let mut reply = Vec::new();
    // The refusal may arrive, or the connection may be reset under it when the
    // exporter closes with bytes unread; both are a refusal. What must not
    // happen is a 200.
    let _ = s.read_to_end(&mut reply);
    let reply = String::from_utf8_lossy(&reply);
    assert!(
        reply.is_empty() || reply.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "{reply}"
    );

    let (head, body) = get(addr, "/metrics");
    exporter.stop();
    assert!(
        head.starts_with("HTTP/1.1 200 OK\r\n"),
        "and the exporter serves on: {head}"
    );
    assert_eq!(
        sample(&body, "fixbolt_exporter_bad_requests_total"),
        Some("1"),
        "{body}"
    );
}

#[test]
fn a_client_that_sends_nothing_is_dropped_after_the_read_timeout() {
    let (_engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let exporter = Exporter::builder(any_port())
        .engine("a", observer)
        .read_timeout(Duration::from_millis(200))
        .fresh_wait(Duration::from_millis(5))
        .spawn()
        .expect("the exporter starts");
    let addr = exporter.local_addr();

    let mut silent = TcpStream::connect(addr).expect("connect");
    silent
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("a timeout");
    let began = Instant::now();
    let mut buf = [0u8; 64];
    let n = silent.read(&mut buf).unwrap_or(0);
    let took = began.elapsed();
    assert_eq!(n, 0, "closed without an answer");
    assert!(
        took >= Duration::from_millis(150) && took < Duration::from_secs(3),
        "dropped at the read timeout, not before and not never: {took:?}"
    );

    let (head, _) = get(addr, "/metrics");
    exporter.stop();
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
}

#[test]
fn without_events_the_exporter_leaves_the_stream_alone() {
    // The observer exists before the Logon, so the `LoggedOn` event is recorded.
    let (_engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let exporter = Exporter::builder(any_port())
        .engine("a", observer.clone())
        .tick(Duration::from_millis(5))
        .fresh_wait(Duration::from_millis(5))
        .spawn()
        .expect("the exporter starts");
    let (_, body) = get(exporter.local_addr(), "/metrics");
    // Many ticks: every one of them would have drained the stream.
    std::thread::sleep(Duration::from_millis(100));
    exporter.stop();

    assert!(
        !body.contains("fixbolt_events_total"),
        "no event counters unless the caller hands the stream over: {body}"
    );
    assert!(
        body.contains("fixbolt_events_lost_total{engine=\"a\"} 0"),
        "the loss count is always there: {body}"
    );
    let mut events = Vec::new();
    observer.events(&mut events);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.kind(), EventKind::LoggedOn)),
        "the application's `LoggedOn` event is still there for the application: {events:?}"
    );
}

#[test]
fn with_events_the_exporter_counts_and_hands_each_event_on() {
    let (_engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&seen);
    let exporter = Exporter::builder(any_port())
        .engine("a", observer.clone())
        .tick(Duration::from_millis(5))
        .fresh_wait(Duration::from_millis(5))
        .with_events(move |e| {
            if matches!(e.kind(), EventKind::LoggedOn) {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        })
        .spawn()
        .expect("the exporter starts");
    let (_, body) = get(exporter.local_addr(), "/metrics");
    exporter.stop();

    assert_eq!(
        sample(
            &body,
            "fixbolt_events_total{engine=\"a\",kind=\"logged_on\"}"
        ),
        Some("1"),
        "{body}"
    );
    assert_eq!(
        sample(
            &body,
            "fixbolt_session_ends_total{engine=\"a\",reason=\"engine_shutdown\"}"
        ),
        Some("0"),
        "the reason set is fixed and printed whole once events are on: {body}"
    );
    assert_eq!(
        seen.load(Ordering::Relaxed),
        1,
        "the caller's closure saw it"
    );
    let mut events = Vec::new();
    observer.events(&mut events);
    assert!(
        events.is_empty(),
        "the exporter is the stream's only reader: {events:?}"
    );
}

#[test]
fn a_series_with_no_source_is_omitted_not_zeroed() {
    let (mut engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let exporter = Exporter::builder(any_port())
        .engine("a", observer.clone())
        .fresh_wait(Duration::from_millis(5))
        .spawn()
        .expect("the exporter starts");
    let _ = observer.request();
    engine.turn();
    let (_, body) = get(exporter.local_addr(), "/metrics");
    exporter.stop();

    assert_eq!(
        sample(&body, "fixbolt_sessions_logged_on{engine=\"a\"}"),
        Some("1"),
        "the snapshot is a real one: {body}"
    );
    for absent in [
        "fixbolt_ring_to_app_used_bytes",
        "fixbolt_ring_to_app_capacity_bytes",
        "fixbolt_presession_slots_used",
        "fixbolt_presession_slots_capacity",
        "fixbolt_events_total",
        "fixbolt_session_ends_total",
    ] {
        assert!(
            !body.contains(absent),
            "{absent}: nothing reports it for an inline, hand-built engine without events, \
             so it is left out — a zero would read as empty and healthy: {body}"
        );
    }
}

#[test]
fn a_ring_is_exported_when_the_dispatch_reports_one() {
    let (to_app, _app_end) = ring::pair(8192);
    let (_back, from_app) = ring::pair(8192);
    let (mut engine, observer, _peer) = logged_on(RingDispatch::<512>::new(to_app, from_app));
    let exporter = Exporter::builder(any_port())
        .engine("r", observer.clone())
        .fresh_wait(Duration::from_millis(5))
        .spawn()
        .expect("the exporter starts");
    let _ = observer.request();
    engine.turn();
    let (_, body) = get(exporter.local_addr(), "/metrics");
    exporter.stop();

    assert_eq!(
        sample(&body, "fixbolt_ring_to_app_capacity_bytes{engine=\"r\"}"),
        Some("8192"),
        "{body}"
    );
    assert_eq!(
        sample(&body, "fixbolt_ring_to_app_used_bytes{engine=\"r\"}"),
        Some("0"),
        "an empty ring that IS reported reads zero: {body}"
    );
}

#[test]
fn stop_joins_the_thread() {
    /// Set when the exporter thread's state is dropped — which happens when the
    /// thread's loop returns, and only then.
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let (_engine, observer, _peer) = logged_on(InlineDispatch::new(Silent));
    let gone = Arc::new(AtomicBool::new(false));
    let witness = Dropped(Arc::clone(&gone));
    let exporter = Exporter::builder(any_port())
        .engine("a", observer)
        .with_events(move |_| {
            let _ = &witness;
        })
        .spawn()
        .expect("the exporter starts");
    let addr = exporter.local_addr();

    // Positive only: other tests in this binary run exporters of their own, so
    // "no thread of that name" is not something this test can assert.
    #[cfg(target_os = "linux")]
    assert!(
        thread_named("fixbolt-metrics"),
        "the exporter's thread carries its name, so `ps -L` can find it"
    );
    assert!(!gone.load(Ordering::SeqCst), "running");
    let began = Instant::now();
    exporter.stop();
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "stop returns within a tick: {:?}",
        began.elapsed()
    );
    assert!(
        gone.load(Ordering::SeqCst),
        "the thread's state was dropped before `stop` returned — joined, not detached"
    );
    assert!(
        TcpStream::connect(addr).is_err(),
        "and the listener is closed"
    );
}

/// Whether a thread of this process carries `name` right now.
#[cfg(target_os = "linux")]
fn thread_named(name: &str) -> bool {
    std::fs::read_dir("/proc/self/task")
        .expect("procfs")
        .filter_map(Result::ok)
        .any(|t| std::fs::read_to_string(t.path().join("comm")).is_ok_and(|c| c.trim_end() == name))
}
