//! `standard` mode, steps 2 and 3 of `docs/plans/2026-08-30-standard-mode.md`.
//!
//! **Step 2** asks whether `source()` names the socket it claims to name and
//! whether `poll(2)` agrees. **Step 3** asks whether `wait::Block` wakes on the
//! bytes rather than on its own clock — which is the assertion the whole mode
//! rests on, and the only one here that a correct-but-slow engine fails.
//!
//! # Why this file does not compare `source()` with `as_raw_fd()`
//!
//! Because that compares a function with its own body. `TcpTransport::source`
//! *is* `as_raw_fd`, so the assertion would hold for a version that returned
//! the wrong socket's descriptor as long as it got it the same wrong way.
//!
//! So the check is behavioural instead: **two sockets, bytes written into one
//! of them, and `poll` asked which one has something.** A `source()` that named
//! a constant, the listener, or the other end of the pair fails it. That is
//! also the only reason the `unsafe` in `crates/engine/src/poll.rs` is
//! believable — `CLAUDE.md` §2 rule 8 asks for the thing that proves it sound,
//! and this is it.
//!
//! Nothing here needs an engine, a session or a dictionary. It is one syscall
//! and two sockets.
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use fixbolt_engine::block::{Block, MIN_TIMEOUT_MS};
use fixbolt_engine::poll::{PollError, Poller};
use fixbolt_engine::transport::{Interest, Source, TcpTransport, Transport};
use fixbolt_engine::wait::Waiting;

/// A connected pair: the client end, and the engine's end wrapped as a
/// transport.
fn pair() -> (TcpStream, TcpTransport) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");
    let client = TcpStream::connect(addr).expect("connect");
    let (server, _) = listener.accept().expect("accept");
    (client, TcpTransport::new(server).expect("non-blocking"))
}

/// `poll` says the socket with bytes in it is ready, and the quiet one is not.
///
/// **This is the reversal target.** Make `TcpTransport::source` return any
/// descriptor other than its own — the other transport's, the listener's, a
/// constant — and one of these two assertions fails, because they disagree
/// about which socket is which. A test that only asserted "something is ready"
/// would survive all of those.
#[test]
fn poll_can_tell_the_two_sockets_apart_by_their_source() {
    let (mut loud_client, loud) = pair();
    let (_quiet_client, quiet) = pair();

    loud_client.write_all(b"8=FIX.4.4\x01").expect("write");
    // The bytes have to have arrived before `poll` is asked, and loopback is
    // fast but not instant. Poll the loud one with a real timeout so this is a
    // wait rather than a race; the quiet one is then asked with timeout 0,
    // where a "ready" answer cannot be blamed on timing.
    let mut poller = Poller::with_capacity(4);

    let loud_source = loud.source().expect("a TCP transport has a descriptor");
    let quiet_source = quiet.source().expect("a TCP transport has a descriptor");
    assert_ne!(
        loud_source, quiet_source,
        "two different sockets must not report the same source"
    );

    let ready = poller
        .wait(&[Interest::readable(loud_source)], 1_000)
        .expect("poll answered");
    assert_eq!(
        ready.count, 1,
        "the socket that was written to is the one poll calls ready"
    );

    let ready = poller
        .wait(&[Interest::readable(quiet_source)], 0)
        .expect("poll answered");
    assert_eq!(
        ready.count, 0,
        "and the socket nobody wrote to is not ready — this is the half that \
         catches a source() naming the wrong socket"
    );
}

/// A descriptor the kernel does not know is [`PollError::BadSource`], never a
/// quiet socket.
///
/// `poll(2)` counts `POLLNVAL` in its return value, so a poller that trusted
/// that number would report an unknown descriptor as **ready**. Then a reversal
/// of `source()` landing on a bogus descriptor would still read green. This is
/// the assertion that makes the distinction real.
///
/// # Why the descriptor is `i32::MAX` and not a socket that was just closed
///
/// `[measured 2026-08-30]` the first version of this test closed a socket and
/// asked about its descriptor. **It went red once and then passed 30 runs in a
/// row.** The panic landed on the `Ok(count == 0)` branch, which says exactly
/// what happened: another test thread in this same binary had already reopened
/// that number, so the descriptor was valid, live and quiet — and "quiet" is
/// indistinguishable from "closed" at this layer, which is the whole point of
/// the assertion.
///
/// Descriptor numbers are reused eagerly, lowest-free-first, and most of the
/// tests in this file open sockets, on as many threads as the harness runs. So
/// the green depended on scheduling. **A flaky guard is worse than a missing one**, and the fix is not
/// a retry: it is to ask about a number nothing in this process can ever have
/// been given.
#[test]
fn an_unknown_descriptor_is_an_error_and_not_a_quiet_socket() {
    // Above any conceivable RLIMIT_NOFILE, so it cannot be a live socket
    // belonging to another test — deterministic where a closed descriptor was
    // not.
    let source = Source::from_raw_fd(i32::MAX);

    let mut poller = Poller::with_capacity(1);
    assert_eq!(
        poller.wait(&[Interest::readable(source)], 0),
        Err(PollError::BadSource),
        "a descriptor the kernel does not know must never look like a quiet socket"
    );
}

/// Asking with a zero timeout is not a wait, and it must still answer.
///
/// `standard` never uses a zero timeout — ADR-0014 decision 6 names it as one
/// of the four ways to build a spin wearing `standard`'s name — but the probe
/// above depends on it, so it gets an assertion of its own rather than being
/// assumed.
#[test]
fn a_zero_timeout_returns_immediately_with_an_answer() {
    let (_client, transport) = pair();
    let source = transport.source().expect("a descriptor");
    let mut poller = Poller::with_capacity(1);

    let before = std::time::Instant::now();
    let ready = poller
        .wait(&[Interest::readable(source)], 0)
        .expect("poll answered");
    let elapsed = before.elapsed();

    assert_eq!(ready.count, 0, "nothing was written, so nothing is ready");
    assert!(
        elapsed < std::time::Duration::from_millis(50),
        "a zero timeout waits for nothing; this took {elapsed:?}"
    );
}

/// An empty source list with a timeout is a plain sleep, and it must not be
/// mistaken for readiness.
///
/// This is the shape `Engine::idle` passes today, and it is exactly what a
/// `standard` strategy must never be given: it blocks for the whole timeout and
/// then reports nothing ready, forever. Asserting it here means the behaviour
/// is written down before step 4 makes it reachable.
#[test]
fn an_empty_source_list_waits_out_the_timeout_and_finds_nothing() {
    let mut poller = Poller::with_capacity(1);
    let before = std::time::Instant::now();
    let ready = poller.wait(&[], 50).expect("poll answered");
    let elapsed = before.elapsed();

    assert_eq!(ready.count, 0, "no sources, nothing ready");
    assert!(
        elapsed >= std::time::Duration::from_millis(40),
        "with no sources there is nothing to wake it early; this took {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// Step 3: `wait::Block` — the policy around the syscall.
// ---------------------------------------------------------------------------

/// A quiet socket costs the whole timeout. This is the easy half.
#[test]
fn a_quiet_socket_costs_the_whole_timeout() {
    let (_client, transport) = pair();
    let source = transport.source().expect("a descriptor");
    let mut block = Block::with_timeout_ms(4, 200);

    let before = Instant::now();
    block.idle(&[Interest::readable(source)]);
    let elapsed = before.elapsed();

    assert!(
        elapsed >= Duration::from_millis(180),
        "nothing arrived, so it should have waited out the timeout; took {elapsed:?}"
    );
    assert_eq!(block.last_error(), None, "and it waited rather than slept");
}

/// **Bytes wake it, and not the clock. This is the assertion the whole mode
/// rests on.**
///
/// A `Block` that always returned after exactly its timeout would satisfy every
/// other test here: it blocks, it gives the core back, the engine is correct.
/// It would also add 100 ms to every message. That bug is invisible to a
/// correctness suite and invisible to a CPU measurement, and this is the only
/// thing in the repository that sees it.
#[test]
fn bytes_wake_it_far_sooner_than_the_timeout() {
    let (mut client, transport) = pair();
    let source = transport.source().expect("a descriptor");
    // A long timeout deliberately: the gap between "woken by data" and "woken
    // by the clock" is what is being measured, so it has to be wide enough that
    // scheduling noise cannot close it.
    let mut block = Block::with_timeout_ms(4, 2_000);

    client.write_all(b"8=FIX.4.4\x01").expect("write");

    let before = Instant::now();
    block.idle(&[Interest::readable(source)]);
    let elapsed = before.elapsed();

    assert!(
        elapsed < Duration::from_millis(200),
        "the bytes were already there; a wait of {elapsed:?} against a 2000 ms \
         timeout means it woke on the clock, not on the data"
    );
    assert_eq!(block.last_error(), None);
}

/// A signal is not a wakeup: `EINTR` goes back and waits out what is left.
///
/// Reversal: treat `PollError::Interrupted` as a wakeup and return, and this
/// drops from ~300 ms to ~60 ms.
#[test]
fn a_signal_does_not_end_the_wait_early() {
    // SAFETY: installs a no-op handler for SIGUSR1, whose default action would
    // otherwise terminate the process. Nothing in this binary, in `std`, or in
    // this crate uses SIGUSR1, and the handler touches no state at all.
    #[allow(unsafe_code)]
    unsafe {
        extern "C" fn noop(_: libc::c_int) {}
        libc::signal(libc::SIGUSR1, noop as *const () as libc::sighandler_t);
    }

    let (_client, transport) = pair();
    let source = transport.source().expect("a descriptor");
    let (tx, rx) = std::sync::mpsc::channel();

    let waiter = std::thread::spawn(move || {
        // SAFETY: `pthread_self` reads the calling thread's own identifier and
        // has no preconditions. It is sent to the main thread so the signal can
        // be delivered to THIS thread rather than to whichever one the kernel
        // picks — a process-wide `kill` would land anywhere and make this test
        // depend on scheduling, which is how the last flaky test in this file
        // got written.
        #[allow(unsafe_code)]
        let me = unsafe { libc::pthread_self() };
        tx.send(me as usize).expect("send");

        let mut block = Block::with_timeout_ms(4, 300);
        let before = Instant::now();
        block.idle(&[Interest::readable(source)]);
        (before.elapsed(), block.last_error())
    });

    let target = rx.recv().expect("the waiter published its thread id");
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(40));
        // SAFETY: `target` is the live pthread_t the waiting thread just sent;
        // it is joined below, so it has not exited. SIGUSR1 has the no-op
        // handler installed above.
        #[allow(unsafe_code)]
        unsafe {
            libc::pthread_kill(target as libc::pthread_t, libc::SIGUSR1);
        }
    }

    let (elapsed, err) = waiter.join().expect("the waiter finished");
    assert_eq!(err, None, "a signal is not a poll failure");
    assert!(
        elapsed >= Duration::from_millis(260),
        "three signals arrived during the wait; it should still have waited out \
         its 300 ms rather than returning on the first one. Took {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(1_000),
        "and it should not have restarted the full timeout on each signal, \
         which would let a stream of them extend an idle turn without bound. \
         Took {elapsed:?}"
    );
}

/// A `poll` that fails is recorded, and **still gives the core back**.
///
/// The dangerous shape is the other one: an error that turns `idle` into an
/// immediate return, so the engine loop spins at full tilt under `standard`'s
/// name with nothing to show for it.
#[test]
fn a_failing_poll_is_recorded_and_still_sleeps() {
    let bad = Source::from_raw_fd(i32::MAX);
    let mut block = Block::with_timeout_ms(4, 200);

    let before = Instant::now();
    block.idle(&[Interest::readable(bad)]);
    let elapsed = before.elapsed();

    assert_eq!(
        block.last_error(),
        Some(PollError::BadSource),
        "the failure is observable rather than silent"
    );
    assert!(
        elapsed >= Duration::from_millis(180),
        "a failing poll must not become a spin; took {elapsed:?}"
    );
}

/// A timeout of zero is a spin wearing this mode's name, and is refused.
#[test]
fn a_zero_timeout_is_raised_to_the_floor() {
    assert_eq!(Block::with_timeout_ms(1, 0).timeout_ms(), MIN_TIMEOUT_MS);
    assert_eq!(Block::with_timeout_ms(1, 1).timeout_ms(), MIN_TIMEOUT_MS);
    assert_eq!(Block::with_timeout_ms(1, 250).timeout_ms(), 250);

    const { assert!(Block::SLEEPS, "standard blocks, by definition") };
    const {
        assert!(
            Block::NEEDS_SOURCES,
            "and it is the first strategy that does"
        )
    };
}
