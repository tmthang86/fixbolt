//! Two numbers an exporter needs and a `Snapshot` did not carry: how full the
//! ring to the application is, and how many pre-session slots are taken.
//!
//! Step 1 of `docs/plans/2026-09-24-p4-metrics-exporter.md`, ADR-0170 decision
//! 8, and written to be **red**: `[verified 2026-09-24]` `Snapshot` had neither
//! number, `Dispatch` had no way to report a ring, and `Observer` could read the
//! cell only by raising the flag that makes the engine build another snapshot.
//!
//! # `None` is asserted, not only `Some`
//!
//! A number nobody reported is not an empty ring or a free front door, and an
//! exporter that printed a zero for it would say "healthy" about something it
//! cannot see. So each `Some` has a `None` beside it, on the shape of engine
//! that has nothing to report: an inline dispatch has no ring, and an engine
//! built by hand has no pre-session stage in front of it.
//!
//! # Why `latest` is proven by `published`
//!
//! `latest` exists so that a reader waiting for a fresh snapshot does not pay
//! the engine for a second one. The only observable difference between it and
//! `request` is the count of snapshots built, so that count is the assertion.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it.
#![allow(clippy::indexing_slicing)]

use std::ops::Range;

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::{Dispatch, InlineDispatch, RingDispatch};
use fixbolt_engine::journal::Store;
use fixbolt_engine::ring;
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};

const M: usize = 512;

/// The bytes asked of `ring::pair` for the ring to the application. A power of
/// two, so the capacity the snapshot reports is exactly this number rather
/// than the rounded-up one.
const RING: usize = 8192;

/// Answers nothing. What is measured here is the engine's bookkeeping, not an
/// application's reply.
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

/// A whole message, header fields first and in tag order.
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

type Wired<D> =
    Engine<Loopback, fixbolt_session::Acceptor, D, ManualClock, Yield, Store, 256, 4096, 8192>;

/// An engine built by hand — no `serve`, so no pre-session stage in front of it
/// — holding one logged-on session.
fn logged_on<D: Dispatch>(dispatch: D) -> (Loopback, Wired<D>) {
    let (mut peer, side) = Loopback::pair();
    let mut engine: Wired<D> = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        dispatch,
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let _ = engine.add(side);
    let _ = peer.send(&logon());
    engine.turn();
    drain(&mut peer);
    (peer, engine)
}

/// Ask, let the engine turn once, and take what it published.
fn asked<D: Dispatch>(engine: &mut Wired<D>) -> fixbolt_engine::observe::Snapshot {
    let observer = engine.observer();
    let _ = observer.request();
    engine.turn();
    observer.request().expect("the engine published on request")
}

#[test]
fn a_ring_dispatch_reports_how_full_the_ring_to_the_application_is() {
    let (to_app, _from_engine) = ring::pair(RING);
    let (_to_engine, from_app) = ring::pair(RING);
    let (mut peer, mut engine) = logged_on(RingDispatch::<M>::new(to_app, from_app));

    // The application never drains, so the order stays in the ring.
    let _ = peer.send(&order());
    engine.turn();
    let snap = asked(&mut engine);
    assert_eq!(
        snap.sessions().len(),
        1,
        "the session is still up: {snap:?}"
    );

    let ring = snap
        .ring_to_app()
        .expect("a ring dispatch reports its ring to the application");
    assert_eq!(
        ring.capacity(),
        RING,
        "the capacity is the ring's allocated size, in bytes"
    );
    assert!(
        ring.used() > 0,
        "an undrained order is in the ring, so it cannot read empty: {ring:?}"
    );
    assert!(
        ring.used() <= ring.capacity(),
        "used within capacity: {ring:?}"
    );
}

#[test]
fn an_inline_dispatch_reports_no_ring() {
    let (_peer, mut engine) = logged_on(InlineDispatch::new(Silent));
    let snap = asked(&mut engine);
    assert_eq!(
        snap.sessions().len(),
        1,
        "the snapshot is a real one: {snap:?}"
    );
    assert_eq!(
        snap.ring_to_app(),
        None,
        "an inline dispatch has no ring; a zero here would read as an empty one"
    );
}

#[test]
fn a_hand_built_engine_reports_no_presession_slots() {
    let (_peer, mut engine) = logged_on(InlineDispatch::new(Silent));
    let snap = asked(&mut engine);
    assert_eq!(
        snap.sessions().len(),
        1,
        "the snapshot is a real one: {snap:?}"
    );
    assert_eq!(
        snap.presession_slots(),
        None,
        "nothing stands in front of a hand-built engine, so nothing reported slots"
    );
}

#[test]
fn latest_reads_without_asking() {
    let (_peer, mut engine) = logged_on(InlineDispatch::new(Silent));
    let observer = engine.observer();
    assert!(
        observer.latest().is_none(),
        "nothing published yet, so nothing to read"
    );
    for _ in 0..100 {
        engine.turn();
    }
    assert_eq!(
        observer.published(),
        0,
        "`latest` before anything was published asked for nothing"
    );

    let _ = observer.request();
    engine.turn();
    assert_eq!(observer.published(), 1, "one request, one snapshot");

    let snap = observer.latest().expect("the published snapshot");
    assert_eq!(
        snap.sessions().len(),
        1,
        "it is the published one: {snap:?}"
    );
    for _ in 0..100 {
        let _ = observer.latest();
        engine.turn();
    }
    assert_eq!(
        observer.published(),
        1,
        "`latest` read the cell a hundred times and the engine built nothing more"
    );
}

/// Through the front door, which is the only place a pre-session stage exists:
/// two sockets that connect and say nothing hold two slots until their
/// `Logon` deadline.
#[cfg(all(feature = "standard", unix))]
#[test]
fn the_front_door_reports_its_presession_slots() {
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};

    use fixbolt_engine::observe::Handles;
    use fixbolt_engine::presession::{Limits, Table};

    const PENDING: usize = 8;

    let addr = {
        let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
        l.local_addr().expect("bound").to_string()
    };
    let limits = Limits::new(PENDING, 30_000).expect("both above zero");
    let handles = Handles::new();
    let observer = handles.observer();
    let admin = handles.admin();
    let serving = addr.clone();
    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve(
            &serving,
            Table::with_capacity(1).serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
            Silent,
            4,
            limits,
            fixbolt_engine::msglog::NoLog,
            handles,
        )
    });

    let connect = || {
        for _ in 0..500 {
            if let Ok(s) = TcpStream::connect(&addr) {
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the serving loop never bound {addr}");
    };
    let silent_a = connect();
    let silent_b = connect();

    // `standard` wakes at least every 100 ms, and publishes on the first turn
    // after a request — so asking every 10 ms sees the slots within a few
    // hundred milliseconds or not at all.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen = None;
    while Instant::now() < deadline {
        if let Some(s) = observer.request() {
            seen = s.presession_slots();
            if seen.is_some_and(|o| o.used() == 2) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    admin.shutdown(2_000);
    let _ = engine.join().expect("the serving thread did not panic");
    drop((silent_a, silent_b));

    let slots = seen.expect("the front door reports its pre-session slots");
    assert_eq!(
        slots.used(),
        2,
        "two sockets that said nothing hold two slots: {slots:?}"
    );
    assert_eq!(
        slots.capacity(),
        limits.pending(),
        "the ceiling is `Limits::pending()`"
    );
}
