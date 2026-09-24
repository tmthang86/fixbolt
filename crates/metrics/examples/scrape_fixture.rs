//! One scrape of a fixture that has all three deployment shapes a
//! `fixbolt-metrics` series can come from, printed to stdout exactly once.
//!
//! Row 6 of `docs/plans/2026-09-24-p4-metrics-exporter.md`. ADR-0171 decision
//! 4 makes this the input `scripts/check-metrics-format.sh` pipes into
//! `promtool check metrics` — the format oracle, not this crate's own
//! encoder read back by itself. ADR-0171 decision 6 is why the fixture needs
//! all three shapes at once: `tests/dashboard.rs`'s
//! `every_series_the_dashboard_queries_is_scraped` has to find every series
//! `tools/grafana/fixbolt.json` names in a real scrape, and the ring series,
//! the pre-session series and the event series each exist only behind one
//! shape (ADR-0170 decisions 7, 8; `crates/metrics/src/series.rs`).
//!
//! The three shapes:
//!
//! - **`ring`** — a hand-built engine on [`RingDispatch`], holding a logged-on
//!   session with an order sitting undrained in its ring to the application:
//!   [`Snapshot::ring_to_app`] and every per-session series.
//! - **`front-door`** — an engine behind the real [`fixbolt_engine::serve`],
//!   the only place [`Snapshot::presession_slots`] is ever `Some`.
//! - **Events, turned on** for both, at the exporter level
//!   ([`Builder::with_events`]): `fixbolt_events_total` and
//!   `fixbolt_session_ends_total` print their whole fixed label set once the
//!   exporter owns an engine's event stream, whether or not anything
//!   happened on it yet (`crates/metrics/tests/exporter.rs`
//!   `with_events_the_exporter_counts_and_hands_each_event_on`).
//!
//! This file and `tests/dashboard.rs` build the same fixture from scratch,
//! on purpose: the brief for this step may not add a shared module under
//! `crates/metrics/src/`, and the plan's own file list treats the two as
//! separate, self-contained files — the shape every other test file in this
//! crate already takes (`tests/exporter.rs` and
//! `crates/engine/tests/observe_occupancy.rs` duplicate the same `Silent`,
//! `logon`, `drain` rather than share them).
//!
//! ```text
//! cargo run -p fixbolt-metrics --example scrape_fixture | promtool check metrics
//! ```
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// this fixture — like every test file in this crate — reads its own state
// back and panics if it is not what was built.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

// `serve` exists only on a unix target (non-negotiable 6), and this example's
// whole point is exercising it — `standard` comes unconditionally from this
// crate's dev-dependency on `fixbolt-engine`, so the only gate needed here is
// the platform one.
#[cfg(unix)]
fn main() {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::ops::Range;
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

    // ---- the `ring` engine: hand-built, a logged-on session, an order left
    // undrained in the ring to the application -------------------------------
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

    // ---- the `front-door` engine: the real serving loop, no connections
    // needed — `note_presession_slots` is called every loop turn whatever
    // `used` is, so `Some(Occupancy { used: 0, .. })` is enough to prove the
    // series is reported at all ------------------------------------------
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

    // ---- the exporter watches both, with events turned on for both --------
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("an address");
    let exporter = Exporter::builder(addr)
        .engine("ring", ring_observer.clone())
        .engine("front-door", front_observer.clone())
        .min_request_interval(Duration::from_millis(1))
        .fresh_wait(Duration::from_millis(50))
        .with_events(|_, _| {})
        .spawn()
        .expect("the exporter starts");

    // The ring engine turns only when this process asks it to: ask once more
    // so the order-in-the-ring turn above is the one a request sees.
    let _ = ring_observer.request();
    ring_engine.turn();

    // `standard` wakes at least every 100 ms and publishes on the first turn
    // after a request; poll until the front door has reported real slots
    // rather than trusting a fixed sleep — the same pattern
    // `crates/engine/tests/observe_occupancy.rs`
    // `the_front_door_reports_its_presession_slots` uses.
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

    // ---- exactly one scrape -------------------------------------------
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

    // The body only, with no extra byte on top of what the encoder wrote —
    // `promtool check metrics` is strict about the exposition format, and
    // this is its entire input.
    std::io::stdout()
        .write_all(body.as_bytes())
        .expect("stdout");
}

#[cfg(not(unix))]
fn main() {
    eprintln!("`serve` needs a unix target — this fixture cannot run here");
    std::process::exit(1);
}
