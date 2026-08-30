//! Step 2 of `docs/plans/2026-08-30-standard-mode.md`: does `source()` name the
//! socket it claims to name, and does `poll(2)` agree?
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

use fixbolt_engine::poll::{PollError, Poller};
use fixbolt_engine::transport::{Interest, TcpTransport, Transport};

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
/// Descriptor numbers are reused eagerly, lowest-free-first, and there are four
/// tests here opening sockets on four threads. So the green depended on
/// scheduling. **A flaky guard is worse than a missing one**, and the fix is not
/// a retry: it is to ask about a number nothing in this process can ever have
/// been given.
#[test]
fn an_unknown_descriptor_is_an_error_and_not_a_quiet_socket() {
    // Above any conceivable RLIMIT_NOFILE, so it cannot be a live socket
    // belonging to another test — deterministic where a closed descriptor was
    // not.
    let source = fixbolt_engine::transport::Source::from_raw_fd(i32::MAX);

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
