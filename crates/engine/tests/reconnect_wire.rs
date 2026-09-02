//! An initiator that lost its connection, over a real socket.
//!
//! Step 3 of [an-initiator-that-comes-back]. `tests/reconnect.rs` is the policy
//! with no I/O in sight; this is the loop that uses it, against a `TcpListener`
//! that stops answering.
//!
//! # What plays the counterparty
//!
//! A hand-written acceptor in this file, not `libquickfix` and not the corpus.
//! It answers a `Logon` with a `Logon` and does nothing else, which is all this
//! file is about — **whether the loop comes back**, not whether the protocol is
//! right. `scripts/interop.sh` is what checks the protocol, against an engine
//! that never heard of this project.
//!
//! [an-initiator-that-comes-back]: ../../../docs/plans/2026-09-02-an-initiator-that-comes-back.md
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fixbolt_engine::reconnect::Policy;
use fixbolt_session::{Application, Config};

/// The application that is never reached: everything here is administrative.
struct Never;

impl Application for Never {
    fn on_message(
        &mut self,
        _msg: &[u8],
        _seq: u32,
        _stamp: &[u8],
        _out: &mut [u8],
    ) -> Option<Range<usize>> {
        None
    }
}

fn cfg() -> Config {
    Config::initiator(b"FIX.4.4", b"FIXBOLT", b"VENUE").with_heart_bt_int(30)
}

/// One whole FIX message, by its own `9=` and trailer.
fn whole(bytes: &[u8]) -> Option<usize> {
    let at = bytes.windows(3).position(|w| w == b"\x019=")?;
    let digits = &bytes[at + 3..];
    let end = digits.iter().position(|b| *b == 1)?;
    let len: usize = core::str::from_utf8(&digits[..end]).ok()?.parse().ok()?;
    let stop = at + 3 + end + 1 + len;
    if bytes.len() < stop + 4 || bytes.get(stop..stop + 3) != Some(b"10=") {
        return None;
    }
    let k = bytes[stop + 3..].iter().position(|b| *b == 1)?;
    Some(stop + 3 + k + 1)
}

fn field(wire: &[u8], tag: &[u8]) -> Option<Vec<u8>> {
    let mut at = 0;
    while at < wire.len() {
        let end = wire[at..].iter().position(|b| *b == 1)? + at;
        if wire[at..end].starts_with(tag) {
            return Some(wire[at + tag.len()..end].to_vec());
        }
        at = end + 1;
    }
    None
}

/// A `Logon` back, at `seq`, stamped now so it survives the skew check.
fn logon_reply(seq: u32) -> Vec<u8> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86_400;
    let (y, m, d) = civil(days);
    let t = secs % 86_400;
    let stamp = format!(
        "{y:04}{m:02}{d:02}-{:02}:{:02}:{:02}",
        t / 3600,
        (t % 3600) / 60,
        t % 60
    );
    let body = format!(
        "35=A\u{1}34={seq}\u{1}49=VENUE\u{1}52={stamp}\u{1}56=FIXBOLT\u{1}98=0\u{1}108=30\u{1}"
    );
    let head = format!("8=FIX.4.4\u{1}9={}\u{1}", body.len());
    let mut wire = head.into_bytes();
    wire.extend_from_slice(body.as_bytes());
    let sum: u32 = wire.iter().map(|b| u32::from(*b)).sum();
    wire.extend_from_slice(format!("10={:03}\u{1}", sum % 256).as_bytes());
    wire
}

/// Days since the Unix epoch to a civil date. Howard Hinnant's algorithm.
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Read one whole message off a socket, or give up.
fn read_one(sock: &mut TcpStream, deadline: Instant) -> Option<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    while Instant::now() < deadline {
        if let Some(end) = whole(&buf) {
            buf.truncate(end);
            return Some(buf);
        }
        match sock.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => return None,
        }
    }
    None
}

/// **The counterparty: it answers one Logon, then hangs up.**
///
/// Hanging up is the point. The initiator has to notice, wait out its policy,
/// and come back — and the `34=` it comes back with is what the assertion is
/// about.
#[test]
fn an_initiator_whose_counterparty_hangs_up_comes_back() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound").to_string();

    // Every Logon this venue sees, in order, as raw wire.
    let seen: Arc<std::sync::Mutex<Vec<Vec<u8>>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&seen);
    let rounds = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&rounds);

    let venue = std::thread::spawn(move || {
        // Two connections and then stop: the second is the whole test, and a
        // venue that accepted for ever would leave this thread alive after it.
        for _ in 0..2 {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            sock.set_nodelay(true).ok();
            sock.set_read_timeout(Some(Duration::from_secs(10))).ok();
            let deadline = Instant::now() + Duration::from_secs(10);
            let Some(logon) = read_one(&mut sock, deadline) else {
                return;
            };
            recorded.lock().map(|mut g| g.push(logon)).ok();
            let _ = sock.write_all(&logon_reply(1));
            counted.fetch_add(1, Ordering::Release);
            // Hang up. `drop` closes it, which is what the initiator has to
            // survive.
        }
    });

    let stop = Arc::new(AtomicUsize::new(0));
    let watcher = Arc::clone(&stop);
    let dialled = addr.clone();
    let engine = std::thread::spawn(move || {
        // 50 ms first delay so the test is not waiting on a production-shaped
        // ladder; the ladder itself is `tests/reconnect.rs`'s subject, and this
        // file is about whether the loop uses it at all.
        let policy = Policy::new(50, 200).expect("a legal pair");
        let _ = fixbolt_engine::connect_and_serve::<Never, fixbolt_engine::journal::Store, _>(
            &dialled,
            cfg(),
            Never,
            policy,
            fixbolt_engine::recovery::NoRecovery,
        );
        watcher.fetch_add(1, Ordering::Release);
    });

    // Two Logons, or the test says which it got.
    let deadline = Instant::now() + Duration::from_secs(20);
    while rounds.load(Ordering::Acquire) < 2 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let got = rounds.load(Ordering::Acquire);
    assert_eq!(
        got, 2,
        "the initiator dialled {got} time(s). One means it never came back \
         after the venue hung up, which is the whole of item 35"
    );

    venue.join().ok();
    let logons = seen.lock().expect("not poisoned").clone();
    assert_eq!(logons.len(), 2, "two Logons on the wire");
    for (i, l) in logons.iter().enumerate() {
        assert_eq!(
            field(l, b"35=").as_deref(),
            Some(&b"A"[..]),
            "message {i} is a Logon"
        );
        assert_eq!(
            field(l, b"49=").as_deref(),
            Some(&b"FIXBOLT"[..]),
            "and it is ours"
        );
    }

    // The engine thread is left dialling a port nobody answers; it is a test
    // process about to exit and the loop has no stop signal from outside yet.
    drop(engine);
}

/// **A policy that says `Stop` is obeyed before a single socket is opened.**
///
/// The control for the test above: without it, "it dialled twice" could be a
/// property of the loop rather than of the policy, and a loop that ignored the
/// policy entirely would pass that test and fail this one.
#[test]
fn a_policy_that_says_stop_never_dials_at_all() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound").to_string();
    listener
        .set_nonblocking(true)
        .expect("so accept does not block");

    let mut policy = Policy::new(50, 200).expect("a legal pair");
    policy.stop();

    // **On a thread, with a deadline, and that is not caution.**
    // `[measured 2026-09-02]` the reversal for this test — the loop ignoring
    // the policy — does not make it fail. It makes it **hang**: a loop that
    // never sees `Stop` never returns, and `cargo test` was killed at ten
    // minutes with no output at all. This repository already has that shape
    // written down (`docs/reference/a-reversal-can-fail-by-hanging.md`) and
    // this is the second instance of it.
    //
    // So the deadline is the assertion. A hang becomes a failure with a
    // sentence attached, which is the difference between a suite that reports
    // and one that has to be killed.
    let returned = Arc::new(AtomicUsize::new(0));
    let flag = Arc::clone(&returned);
    let dialled = addr.clone();
    std::thread::spawn(move || {
        let done = fixbolt_engine::connect_and_serve::<Never, fixbolt_engine::journal::Store, _>(
            &dialled,
            cfg(),
            Never,
            policy,
            fixbolt_engine::recovery::NoRecovery,
        );
        if done.is_ok() {
            flag.fetch_add(1, Ordering::Release);
        }
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    while returned.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(
        returned.load(Ordering::Acquire),
        1,
        "a stopped policy must return, and quickly. Still running after ten \
         seconds means the loop is not consulting the policy at all — which is \
         a hang rather than a wrong answer, and is why this has a deadline"
    );
    assert!(
        listener.accept().is_err(),
        "and nothing ever connected — a loop that dialled first and asked the \
         policy afterwards would leave a socket here"
    );
}
