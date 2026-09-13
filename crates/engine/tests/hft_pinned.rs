//! `serve_hft_pinned`: **refuses a bad core first, pins, and only then binds.**
//!
//! Step 5 of [the-residue-of-an-obligation], `STATUS.md` item 21. `DESIGN.md`
//! D8 has said since 2026-08-30 that in `hft` the polling thread is pinned to an
//! isolated core, and the single-engine door `serve_hft` pins nothing and
//! refuses nothing. This file is the gate on the door that does both.
//!
//! # What each test watches, and the reversal that proves it
//!
//! 1. **The order.** A core the machine does not have, and an address that
//!    cannot be bound — a port this test already holds. A refusal is only
//!    evidence of *order* if the bind would have failed too, so the wrong order
//!    reads `Io` rather than a hang. Reversal R21-1: bind moved above validate.
//! 2. **The isolation rule is on by default.** The first online core outside
//!    `isolcpus` — one always exists, because `isolcpus` never names every
//!    core — refused as `NotIsolated`. Chosen from [`Topology::read`], **not**
//!    from the thread's own mask: on a §9 box that mask can be an isolated core,
//!    and this test would then have nothing to refuse.
//! 3. **The pin took, observed rather than returned.** The application reads
//!    `/proc/thread-self/stat` and `sched_getaffinity` from inside its own
//!    callback, which runs on the engine thread — ADR-0015 decision 2: a call
//!    returning `Ok` is not evidence. Reversal R21-2 (the pin call removed)
//!    goes red at the scheduler's `processor` field, and — because an unpinned
//!    thread can land on the right core by chance — at the mask the kernel
//!    reports when it does — `[measured 2026-09-13]` on a 16-core desk the
//!    `processor` field alone read the right core in 9 of 13 unpinned runs, so
//!    without the mask this reversal is green more often than red. After the
//!    door returns, the same thread's mask is read again: the rustdoc says the
//!    pin outlives the call. Reversal R21-3 (`allow_unisolated` ignored) turns
//!    this test red and leaves test 2 green: two tests, two halves of one rule.
//!
//! **Every test serves from a spawned thread.** Pinning the harness thread would
//! narrow every other test in this binary to one core, and the damage would read
//! as flakiness — `tests/affinity.rs` says the same.
//!
//! # What this file does NOT prove
//!
//! **That the pinned engine thread never sleeps in the kernel.** Like
//! `tests/hft_wire.rs`, this is a behaviour gate on a door, not a mode
//! measurement: non-negotiable 4 is a syscall-level claim, and
//! `scripts/check-no-kernel-sleep.sh` traces `tools/w2w`, not this function.
//!
//! [the-residue-of-an-obligation]: ../../../docs/plans/2026-09-13-the-residue-of-an-obligation.md

// `serve_hft_pinned` exists only with `affinity` on Linux, and its pre-session
// stage needs `standard`'s poller exactly as `serve_hft` does
// (`tests/hft_wire.rs`). Non-negotiable 6: gated on the item, not only here.
#![cfg(all(feature = "affinity", feature = "standard", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fixbolt_engine::affinity::{self, AffinityError, CoreId, CorePin, Topology};
use fixbolt_engine::observe::{EventKind, Handles};
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::{Application, Config, ServeError, Shutdown};

/// What the engine thread saw of itself, from inside the application.
#[derive(Debug, Default)]
struct Seen {
    /// `processor` in `/proc/thread-self/stat` — where the scheduler ran it.
    running_on: Option<CoreId>,
    /// `sched_getaffinity` — where the kernel allows it to run.
    mask: Option<Vec<CoreId>>,
}

/// Answers an application message, and records where it runs when a session
/// comes up.
struct WhereApp {
    echo: fixbolt_conformance::echo::Echo,
    seen: Arc<Mutex<Seen>>,
}

impl Application for WhereApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        let (seq, stamp) = (hdr.seq, hdr.stamp);
        self.echo.reply(msg, seq, stamp, out)
    }

    /// Asked by the engine at the turn the session goes up — on the engine
    /// thread, which is the thread `serve_hft_pinned` pinned.
    fn on_logon(
        &mut self,
        nth: u32,
        _peer: fixbolt_session::Peer<'_>,
        _out: &mut [u8],
    ) -> Option<Range<usize>> {
        if nth == 0 {
            let mut seen = self.seen.lock().unwrap();
            seen.running_on = affinity::running_on().ok();
            seen.mask = affinity::current_mask().ok();
        }
        None
    }
}

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

fn limits() -> Limits {
    Limits::new(8, 30_000).expect("both above zero")
}

fn free_addr() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

/// An address this process is already listening on, and the listener holding
/// it. **Binding it fails for anybody, root included** — which a privileged
/// port does not promise.
fn a_held_addr() -> (TcpListener, String) {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    (l, a)
}

/// A core this thread is allowed to run on, whatever the process's mask is.
///
/// Copied from `tests/affinity.rs`. Not `CoreId(0)`: a cgroup cpuset can
/// exclude it, and a test that fails because of the container it runs in is
/// testing the container.
fn a_core_we_may_use() -> CoreId {
    let mask = affinity::current_mask().expect("reading this thread's own mask");
    *mask
        .first()
        .expect("a thread is allowed on at least one core")
}

fn read_one(client: &mut TcpStream) -> String {
    let mut buf = [0u8; 4096];
    let n = client.read(&mut buf).expect("a reply");
    String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|")
}

/// A `Logon` stamped at the wall clock — see `tests/hft_wire.rs::logon_now`
/// for why the corpus's fixed instant would be refused here.
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

/// Call the door on a thread of its own, with an application that records
/// nothing, and return what it returned.
fn refused(pin: CorePin, addr: String) -> Result<Shutdown, ServeError> {
    std::thread::spawn(move || {
        fixbolt_engine::serve_hft_pinned(
            &pin,
            &addr,
            Table::with_capacity(1).serving(cfg()),
            WhereApp {
                echo: fixbolt_conformance::echo::Echo::default(),
                seen: Arc::default(),
            },
            4,
            limits(),
            fixbolt_engine::msglog::NoLog,
            Handles::new(),
        )
    })
    .join()
    .expect("the serving thread did not panic")
}

/// **The core is refused before a socket exists.**
///
/// `addr` is a port this test is already listening on, so a door that bound
/// first would return `Io` — the refusal can only arrive if validation ran
/// before the bind. A socket bound and only then told the core is wrong is a
/// port held while the operator reads the error.
#[test]
fn serve_hft_pinned_refuses_a_core_the_machine_does_not_have() {
    let (_held, addr) = a_held_addr();
    let result = refused(CorePin::to(CoreId(4096)), addr);

    match result {
        Err(ServeError::Affinity(e)) => assert_eq!(
            e,
            AffinityError::NoSuchCore(CoreId(4096)),
            "the refusal names the core that does not exist"
        ),
        other => panic!(
            "expected Err(Affinity(NoSuchCore(CoreId(4096)))) before any bind; got {other:?}"
        ),
    }
}

/// **The isolation rule is on unless it was waived.**
#[test]
fn serve_hft_pinned_refuses_a_core_outside_isolcpus_unless_told() {
    let topology = Topology::read().expect("reading this machine's topology");
    let core = topology
        .online()
        .iter()
        .copied()
        .find(|c| !topology.isolated().contains(c))
        .expect("isolcpus never names every online core");

    let (_held, addr) = a_held_addr();
    let result = refused(CorePin::to(core), addr);

    match result {
        Err(ServeError::Affinity(e)) => assert_eq!(
            e,
            AffinityError::NotIsolated(core),
            "{core} is online and outside isolcpus (isolated: {:?}), and no waiver was given",
            topology.isolated()
        ),
        other => {
            panic!("expected Err(Affinity(NotIsolated({core:?}))) before any bind; got {other:?}")
        }
    }
}

/// **The engine thread runs on the core it was given — as the scheduler and
/// the kernel report it, from inside the application.**
///
/// The frame is `tests/hft_wire.rs::serve_hft_serves_a_session_and_stops`.
#[test]
fn serve_hft_pinned_serves_on_the_core_it_was_given() {
    let core = a_core_we_may_use();
    let addr = free_addr();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let app_seen = Arc::clone(&seen);

    let engine = std::thread::spawn(move || {
        let returned = fixbolt_engine::serve_hft_pinned(
            &CorePin::to(core).allow_unisolated(),
            &serving,
            Table::with_capacity(1).serving(cfg()),
            WhereApp {
                echo: fixbolt_conformance::echo::Echo::default(),
                seen: app_seen,
            },
            4,
            limits(),
            fixbolt_engine::msglog::NoLog,
            serving_handles,
        );
        // The same thread, after the door has returned: the rustdoc says the
        // pin outlives the call, and this is what holds it to that.
        (returned, affinity::current_mask())
    });

    // **If the door returns before it binds, say what it returned.** A refusal
    // here would otherwise read as five seconds of "never bound" with the reason
    // thrown away — `[measured 2026-09-13]` that is exactly how R21-3 first
    // went red.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut client = loop {
        if let Ok(s) = TcpStream::connect(&addr) {
            s.set_nodelay(true).expect("nodelay");
            s.set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            break s;
        }
        if engine.is_finished() {
            let (returned, _) = engine.join().expect("the serving thread did not panic");
            panic!("serve_hft_pinned returned before it bound {addr}: {returned:?}");
        }
        assert!(
            Instant::now() < deadline,
            "the pinned hft serving loop never bound {addr}"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let began = Instant::now();
    client.write_all(&logon_now(1)).expect("send the Logon");
    let reply = read_one(&mut client);
    let round_trip = began.elapsed();

    assert!(
        reply.contains("|35=A|"),
        "the pinned hft acceptor answered the Logon: {reply}"
    );

    let events = wait_for_event(&handles, &EventKind::LoggedOn, Duration::from_secs(5));
    assert!(
        events.contains(&EventKind::LoggedOn),
        "the session finished its Logon exchange; the stream held {events:?}"
    );

    // Not a latency figure: see `tests/hft_wire.rs` for why an `hft` door
    // carries a bound three orders of magnitude loose.
    assert!(
        round_trip < Duration::from_millis(500),
        "the Logon round trip took {round_trip:?} — an hft engine that takes \
         half a second is waiting on something"
    );

    admin.shutdown(2_000);
    let (stopped, mask_after) = engine.join().expect("the serving thread did not panic");
    let shutdown = stopped.expect("serve_hft_pinned returned a Shutdown rather than an error");
    assert_eq!(
        shutdown.sessions(),
        1,
        "the shutdown counted the session it was serving: {shutdown:?}"
    );

    let seen = seen.lock().unwrap();
    // The scheduler's answer first: where the engine thread actually ran.
    assert_eq!(
        seen.running_on,
        Some(core),
        "the engine thread was pinned to {core}; /proc/thread-self/stat read from \
         inside on_logon says otherwise (None = on_logon never ran or the file was unreadable)"
    );
    // And the kernel's: an unpinned thread can be scheduled on `core` by chance,
    // but it cannot be *allowed* on that core alone by chance.
    assert_eq!(
        seen.mask,
        Some(vec![core]),
        "the engine thread's affinity mask, read from inside on_logon, is not exactly {core}"
    );
    // Nothing unpins the thread on the way out.
    assert_eq!(
        mask_after,
        Ok(vec![core]),
        "after serve_hft_pinned returned, the thread that called it is no longer pinned to {core} alone"
    );
}
