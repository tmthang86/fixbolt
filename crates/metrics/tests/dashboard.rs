//! `every_series_the_dashboard_queries_is_scraped`: every `fixbolt_*` name
//! `tools/grafana/fixbolt.json` names in a panel target is a series a real
//! scrape actually produces.
//!
//! ADR-0171 decision 5. `scripts/check-grafana-dashboard.py` holds the
//! dashboard's *shape* (no `__inputs`, no `${DS_`, every panel on the
//! `${datasource}` variable); it cannot know a series' *name* changed, because
//! nothing in the toolchain sees a renamed string in a JSON blob
//! (ADR-0171 *Context* item 1). This test is that check: scan the committed
//! dashboard for every name shaped like `fixbolt_...`, scan a real scrape's
//! sample lines for the names it actually printed, and fail on the first
//! dashboard name that is not in the second set — a rename that would
//! otherwise read as "No data" in someone's Grafana and nowhere else.
//!
//! # The fixture needs all three deployment shapes at once
//!
//! `fixbolt_ring_to_app_*` exists only under [`RingDispatch`]; the two
//! `fixbolt_presession_slots_*` names only behind the real
//! [`fixbolt_engine::serve`] front door; `fixbolt_events_total` and
//! `fixbolt_session_ends_total` only when the exporter is built with
//! [`Builder::with_events`] (ADR-0170 decisions 7, 8). The dashboard's
//! *Pressure* and *Vì sao phiên kết thúc* rows query all of them, so the
//! fixture this test scrapes has to be all three at once: a hand-built engine
//! on `RingDispatch`, a second engine behind `serve`, and events turned on.
//!
//! This builds the same fixture as `examples/scrape_fixture.rs`, from
//! scratch, rather than sharing a module: the brief for this step may not add
//! anything under `crates/metrics/src/`, and every other test file in this
//! crate already duplicates its own `Silent`/`logon`/`drain` rather than
//! share one (`tests/exporter.rs`, `crates/engine/tests/observe_occupancy.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::ops::Range;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::RingDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::observe::Handles;
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::ring;
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};
use fixbolt_metrics::Exporter;

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

fn wire(msg_type: &str, seq: u32, body: &str) -> Vec<u8> {
    let body = format!(
        "35={msg_type}\x0134={seq}\x0149=TW44\x0152=20260828-12:00:00.000\x0156=ISLD\x01{body}"
    );
    with_real_checksum(format!("8=FIX.4.4\x019={}\x01{body}10=0\x01", body.len()).as_bytes())
}

fn logon() -> Vec<u8> {
    wire("A", 1, "98=0\x01108=30\x01")
}

fn order() -> Vec<u8> {
    wire(
        "D",
        2,
        "11=ID-1\x0121=1\x0138=100\x0140=1\x0154=1\x0155=INTC\x0160=20260828-12:00:00.000\x01",
    )
}

fn drain(peer: &mut Loopback) {
    let mut buf = [0u8; 4096];
    while let Io::Ready(_) = peer.recv(&mut buf) {}
}

type Wired = Engine<
    Loopback,
    fixbolt_session::Acceptor,
    RingDispatch<512>,
    ManualClock,
    Yield,
    Store,
    256,
    4096,
    8192,
>;

/// One scrape of a fixture holding all three deployment shapes: a hand-built
/// engine on `RingDispatch` with a logged-on session and an order left in its
/// ring, and an engine behind the real `serve` front door — both watched by
/// one exporter built with `with_events`.
#[cfg(unix)]
fn scrape_the_fixture() -> String {
    let (to_app, _app_end) = ring::pair(8192);
    let (_back, from_app) = ring::pair(8192);
    let mut ring_engine: Wired = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        RingDispatch::<512>::new(to_app, from_app),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let ring_observer = ring_engine.observer();
    let (mut peer, side) = Loopback::pair();
    let _ = ring_engine.add(side);
    let _ = peer.send(&logon());
    ring_engine.turn();
    drain(&mut peer);
    let _ = peer.send(&order());
    ring_engine.turn();

    const PENDING: usize = 8;
    let limits = Limits::new(PENDING, 30_000).expect("both above zero");
    let handles = Handles::new();
    let front_observer = handles.observer();
    let front_admin = handles.admin();
    let front_thread = std::thread::spawn(move || {
        fixbolt_engine::serve(
            "127.0.0.1:0",
            Table::with_capacity(1).serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
            Silent,
            4,
            limits,
            fixbolt_engine::msglog::NoLog,
            handles,
        )
    });

    let addr: SocketAddr = "127.0.0.1:0".parse().expect("an address");
    let exporter = Exporter::builder(addr)
        .engine("ring", ring_observer.clone())
        .engine("front-door", front_observer.clone())
        .min_request_interval(Duration::from_millis(1))
        .fresh_wait(Duration::from_millis(50))
        .with_events(|_, _| {})
        .spawn()
        .expect("the exporter starts");

    let _ = ring_observer.request();
    ring_engine.turn();

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if front_observer
            .request()
            .is_some_and(|s| s.presession_slots().is_some())
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let body = {
        let mut s = TcpStream::connect(exporter.local_addr()).expect("connect to the exporter");
        s.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("a timeout");
        s.write_all(b"GET /metrics HTTP/1.1\r\nHost: fixbolt\r\nAccept: text/plain\r\n\r\n")
            .expect("send the request");
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).expect("read the reply");
        let text = String::from_utf8(raw).expect("the reply is text");
        let (_head, body) = text
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("no end of headers in {text:?}"));
        body.to_owned()
    };

    exporter.stop();
    front_admin.shutdown(2_000);
    let _ = front_thread
        .join()
        .expect("the serving thread did not panic");
    drop(peer);
    drop(ring_engine);
    body
}

/// Every metric name a Prometheus text-format scrape actually printed: the
/// first `{`- or space-delimited token of every line that is not a `#`
/// comment. A name may repeat (one per label combination); the set is what
/// matters here.
fn scraped_names(body: &str) -> BTreeSet<&str> {
    body.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let brace = l.find('{');
            let space = l.find(' ');
            let end = match (brace, space) {
                (Some(b), Some(s)) => b.min(s),
                (Some(b), None) => b,
                (None, Some(s)) => s,
                (None, None) => l.len(),
            };
            &l[..end]
        })
        .collect()
}

/// Every `fixbolt_...` name in `text`, wherever it appears — the dashboard has
/// no dependency this crate may add to parse its JSON properly (ADR-0170
/// decision 9), and a name is a name whether it sits in an `expr` string, a
/// `legendFormat`, or a title. **At least one identifier character after the
/// prefix** is required (`+`, not `*`): a stray mention of the *pattern*
/// `fixbolt_*` in prose — this file's own doc comment above, or a future
/// dashboard description — has nothing after the underscore an identifier
/// character would match, and is correctly not a name.
fn dashboard_names(text: &str) -> BTreeSet<&str> {
    const PREFIX: &str = "fixbolt_";
    let bytes = text.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while let Some(start) = text[i..].find(PREFIX) {
        let start = i + start;
        let mut end = start + PREFIX.len();
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end > start + PREFIX.len() {
            out.insert(&text[start..end]);
        }
        i = end.max(start + 1);
    }
    out
}

fn dashboard_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tools")
        .join("grafana")
        .join("fixbolt.json")
}

#[test]
fn dashboard_names_finds_a_name_and_ignores_the_glob_pattern() {
    let names = dashboard_names("fixbolt_healthy and fixbolt_* is a pattern, not a name");
    assert_eq!(
        names.into_iter().collect::<Vec<_>>(),
        vec!["fixbolt_healthy"],
        "a real name is found and a bare glob is not"
    );
}

#[test]
fn scraped_names_reads_the_bare_name_off_labelled_and_unlabelled_lines() {
    let body = "\
# HELP fixbolt_healthy x
# TYPE fixbolt_healthy gauge
fixbolt_healthy{engine=\"a\"} 1
fixbolt_exporter_scrapes_total 3
";
    let names = scraped_names(body);
    assert!(names.contains("fixbolt_healthy"), "{names:?}");
    assert!(
        names.contains("fixbolt_exporter_scrapes_total"),
        "{names:?}"
    );
    assert_eq!(names.len(), 2, "{names:?}");
}

#[cfg(unix)]
#[test]
fn every_series_the_dashboard_queries_is_scraped() {
    let path = dashboard_path();
    let dashboard_text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let wanted = dashboard_names(&dashboard_text);
    assert!(
        wanted.len() >= 20,
        "found only {} fixbolt_ names in {} — the scan likely broke, not the \
         dashboard: {wanted:?}",
        wanted.len(),
        path.display()
    );

    let body = scrape_the_fixture();
    let scraped = scraped_names(&body);

    let missing: Vec<_> = wanted.difference(&scraped).collect();
    assert!(
        missing.is_empty(),
        "tools/grafana/fixbolt.json queries {missing:?}, which no series in a \
         real scrape produced (fixture shapes: RingDispatch, a serve front \
         door, with_events) — a name here and the encoder's name in \
         crates/metrics/src/series.rs have drifted apart. Scraped names: \
         {scraped:?}"
    );
}

#[cfg(not(unix))]
#[test]
#[ignore = "the fixture needs `fixbolt_engine::serve`, which is unix-only"]
fn every_series_the_dashboard_queries_is_scraped() {}
