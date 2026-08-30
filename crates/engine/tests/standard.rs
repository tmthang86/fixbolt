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

// ---------------------------------------------------------------------------
// Step 4: the source list the engine hands over.
//
// Every test here READS THE LIST rather than timing a wakeup. All four ways of
// getting this wrong produce an engine that works and is one timeout slower, so
// the obvious test is a timing test — and a timing test for a 100 ms difference
// on a shared CI runner is the flaky kind. The list is the fact; the latency is
// only its symptom.
// ---------------------------------------------------------------------------

use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::Io;
use fixbolt_engine::{Acceptor, Config, Engine};

/// An application that never answers.
struct Silent;
impl fixbolt_session::Application for Silent {
    fn on_message(
        &mut self,
        _m: &[u8],
        _s: u32,
        _t: &[u8],
        _o: &mut [u8],
    ) -> Option<core::ops::Range<usize>> {
        None
    }
}

/// A real socket that **never accepts a byte**.
///
/// `Connection::flush` treats `Io::Idle` as "the socket would not take it, keep
/// it queued", so this leaves `has_pending_output()` true deterministically.
/// The alternative — filling a real kernel send buffer — depends on
/// `net.core.wmem_*` and on how fast the other end is not reading, which is a
/// test that passes or fails for reasons that have nothing to do with FIX.
struct Stubborn(TcpTransport);

impl Transport for Stubborn {
    const POLLABLE: bool = true;
    fn source(&self) -> Option<Source> {
        self.0.source()
    }
    fn recv(&mut self, buf: &mut [u8]) -> Io {
        self.0.recv(buf)
    }
    fn send(&mut self, _buf: &[u8]) -> Io {
        Io::Idle
    }
}

type TestEngine<T> = Engine<
    T,
    fixbolt_session::Acceptor,
    InlineDispatch<Silent>,
    ManualClock,
    fixbolt_engine::wait::Yield,
    Store,
    256,
    4096,
    8192,
>;

fn engine<T: Transport>() -> TestEngine<T> {
    Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TEST"),
        InlineDispatch::new(Silent),
        ManualClock::at(fixbolt_conformance::script::FIXED_TIME_MILLIS),
        fixbolt_engine::wait::Yield,
        4,
    )
}

/// Every connection is in the list, once, naming its own socket.
#[test]
fn every_connection_contributes_exactly_one_readable_interest() {
    let mut e: TestEngine<TcpTransport> = engine();
    let (_c1, t1) = pair();
    let (_c2, t2) = pair();
    let s1 = t1.source().expect("a descriptor");
    let s2 = t2.source().expect("a descriptor");
    e.add(t1);
    e.add(t2);

    let list = e.refresh_interests().to_vec();
    assert_eq!(
        list.len(),
        2,
        "one interest per connection, no more, no fewer"
    );
    assert!(
        list.contains(&Interest::readable(s1)),
        "the first socket is in it"
    );
    assert!(
        list.contains(&Interest::readable(s2)),
        "and so is the second"
    );
    assert_eq!(e.sources_missing(), 0);
}

/// **Writable is asked for only while bytes are queued**, and it really is
/// asked for when they are.
///
/// Both halves matter and they fail differently. Never asking means a stalled
/// flush waits for the timeout. Always asking means the engine is woken every
/// time any socket has room — which is always — and `standard` is a spin again.
#[test]
fn writable_is_asked_for_exactly_while_bytes_are_queued() {
    let mut e: TestEngine<Stubborn> = engine();
    let (mut client, t) = pair();
    let source = t.source().expect("a descriptor");
    e.add(Stubborn(t));

    assert_eq!(
        e.refresh_interests(),
        [Interest::readable(source)],
        "nothing is queued yet, so writability is not asked for"
    );

    // A Logon draws a Logon back, and this transport refuses every byte, so the
    // reply stays in the outbound queue.
    client
        .write_all(&logon())
        .expect("the client can always write");
    for _ in 0..1_000 {
        e.turn();
        if e.refresh_interests() == [Interest::readable_and_writable(source)] {
            return;
        }
    }
    panic!(
        "the reply never queued, or writability was never asked for: {:?}",
        e.refresh_interests()
    );
}

/// A transport that cannot name a source is counted, not silently dropped.
#[test]
fn a_connection_with_no_source_is_counted() {
    struct Blind(TcpTransport);
    impl Transport for Blind {
        const POLLABLE: bool = true;
        fn source(&self) -> Option<Source> {
            None
        }
        fn recv(&mut self, buf: &mut [u8]) -> Io {
            self.0.recv(buf)
        }
        fn send(&mut self, buf: &[u8]) -> Io {
            self.0.send(buf)
        }
    }

    let mut e: TestEngine<Blind> = engine();
    let (_c, t) = pair();
    e.add(Blind(t));

    assert!(
        e.refresh_interests().is_empty(),
        "it cannot be waited on, so it is not in the list"
    );
    assert_eq!(
        e.sources_missing(),
        1,
        "and that is counted — otherwise its traffic is simply one timeout late \
         and nothing anywhere says so"
    );
}

/// The listener reaches the poll set that `idle_with` actually waits on.
///
/// Without it a new connection is accepted on the next timeout instead of on
/// the connect: the handshake works, and is up to 100 ms slower.
///
/// # This test was wrong once, and it is worth saying how
///
/// `[measured 2026-08-30]` the first version built the list by hand —
/// `refresh_interests().to_vec()` and then `push(listener)` — and asserted the
/// listener was in it. Of course it was: the test had put it there. Deleting
/// `idle_with`'s own `extend_from_slice(extra)` left this **green**, because
/// the test never went near that line.
///
/// So it now goes through `refresh_interests_with`, which is the same call
/// `idle_with` makes. A test that assembles the thing it is checking is
/// checking itself.
#[test]
fn the_listener_reaches_the_set_idle_with_waits_on() {
    let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
    let listener = acceptor.source().expect("a listener has a descriptor");

    let mut e: TestEngine<TcpTransport> = engine();
    let (_c, t) = pair();
    let conn = t.source().expect("a descriptor");
    e.add(t);

    let list = e
        .refresh_interests_with(&[Interest::readable(listener)])
        .to_vec();
    assert_eq!(list.len(), 2, "the connection and the listener, both");
    assert!(list.contains(&Interest::readable(conn)));
    assert!(
        list.contains(&Interest::readable(listener)),
        "the listener must be in the set the engine waits on, not only in the \
         one the caller handed over"
    );

    // And it is a descriptor `poll` will actually accept, which no `contains`
    // assertion can show.
    let mut poller = Poller::with_capacity(4);
    assert!(
        poller.wait(&list, 0).is_ok(),
        "poll must accept the listener's descriptor alongside a connection's"
    );
}

/// A well-formed Logon from a client, at the corpus's fixed instant.
///
/// The instant is [`fixbolt_conformance::script::FIXED_TIME_IN`] and not a
/// hand-written one: the engine's clock here is a `ManualClock` at
/// `FIXED_TIME_MILLIS`, and a `52=` more than 120 seconds from it is refused for
/// skew. `[measured 2026-08-30]` the first version of this test invented a
/// timestamp, the Logon was Rejected, the connection was dropped, and the
/// symptom was an **empty** interest list — which reads exactly like "the list
/// was never built".
fn logon() -> Vec<u8> {
    let body = format!(
        "35=A\x0134=1\x0149=TEST\x0152={}\x0156=ISLD\x0198=0\x01108=30\x01",
        fixbolt_conformance::script::FIXED_TIME_IN
    );
    let head = format!("8=FIX.4.4\x019={}\x01", body.len());
    let mut out = head.into_bytes();
    out.extend_from_slice(body.as_bytes());
    let sum: u32 = out.iter().map(|b| u32::from(*b)).sum();
    out.extend_from_slice(format!("10={:03}\x01", sum % 256).as_bytes());
    out
}

// ---------------------------------------------------------------------------
// Step 5: the waker.
//
// In `hft` this whole problem is absent: the engine is spinning and sees
// out-of-band work microseconds later. In `standard` it is asleep inside
// `poll`, which wakes for descriptors and not for a ring buffer.
// ---------------------------------------------------------------------------

use fixbolt_engine::waker::Waker;

/// A blocking engine over TCP, with a waker and no connections.
///
/// No connections on purpose: what is being measured is whether the *waker*
/// wakes it, and a socket in the set would be a second thing that could.
type BlockingEngine = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<Silent>,
    ManualClock,
    Block,
    Store,
    256,
    4096,
    8192,
>;

fn blocking_engine(timeout_ms: u32) -> (BlockingEngine, fixbolt_engine::waker::WakeHandle) {
    let (waker, handle) = Waker::new().expect("a pipe");
    let e = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TEST"),
        InlineDispatch::new(Silent),
        ManualClock::at(fixbolt_conformance::script::FIXED_TIME_MILLIS),
        Block::with_timeout_ms(8, timeout_ms),
        4,
    )
    .with_waker(waker);
    (e, handle)
}

/// The engine puts its own waker in the poll set. The caller cannot forget it.
#[test]
fn the_waker_is_in_the_set_without_the_caller_adding_it() {
    let (mut e, _h) = blocking_engine(100);
    assert_eq!(
        e.refresh_interests().len(),
        1,
        "no connections, and still one interest: the waker's own descriptor"
    );
}

/// A wake ends the wait at once, instead of after the timeout.
#[test]
fn a_wake_ends_the_wait_immediately() {
    let (mut e, handle) = blocking_engine(2_000);

    let waker_thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        handle.wake();
    });

    let before = Instant::now();
    e.idle();
    let elapsed = before.elapsed();
    waker_thread.join().expect("the waker thread finished");

    assert!(
        elapsed < Duration::from_millis(500),
        "a wake arrived after 50 ms against a 2000 ms timeout; waiting {elapsed:?} \
         means the engine slept through it and woke on the clock"
    );
}

/// **The wake is drained, so the next wait is a real wait.**
///
/// This is the failure the mechanism is most likely to have: a self-pipe holding
/// an unread byte stays readable, so every subsequent `poll` returns instantly,
/// for ever. The engine keeps working perfectly and burns a core — which is the
/// single thing `standard` exists to avoid, and neither a correctness suite nor
/// a wakeup-latency test would notice.
///
/// Reversal: delete the `drain()` in `Engine::idle_with` and this fails.
#[test]
fn a_wake_is_drained_so_the_next_wait_still_waits() {
    let (mut e, handle) = blocking_engine(300);

    handle.wake();
    let before = Instant::now();
    e.idle();
    assert!(
        before.elapsed() < Duration::from_millis(150),
        "the pending wake should have ended the first wait at once"
    );

    // Nothing has woken it since. If the byte were still in the pipe this
    // returns instantly and `standard` is a spin.
    let before = Instant::now();
    e.idle();
    let elapsed = before.elapsed();
    assert!(
        elapsed >= Duration::from_millis(250),
        "the second wait had nothing to wake it and must have run its full \
         timeout; {elapsed:?} means the pipe was never drained and this engine \
         now polls in a tight loop for ever"
    );
}

/// Waking far more often than anyone drains neither blocks nor fails.
///
/// A pipe holds 64 KiB. Once it is full a write returns `EAGAIN`, and that is
/// **correct rather than lost work**: a pipe with unread bytes is already
/// readable, which is the whole signal. One pending wake and a hundred thousand
/// mean the same thing.
#[test]
fn waking_more_often_than_anyone_drains_is_harmless() {
    let (mut e, handle) = blocking_engine(100);
    let before = Instant::now();
    for _ in 0..200_000 {
        handle.wake();
    }
    assert!(
        before.elapsed() < Duration::from_secs(10),
        "wake() must never block, even with nobody draining"
    );
    // And the engine still comes back promptly, then drains it all.
    let before = Instant::now();
    e.idle();
    assert!(before.elapsed() < Duration::from_millis(50));
}

/// A reply produced on another thread reaches a sleeping engine at once.
///
/// The end-to-end shape: `RingApp` on the application's thread pushes a reply
/// and wakes; the engine is inside `poll` and comes back for it. Without the
/// waker this takes a whole timeout — correct, and 100 ms late.
#[test]
fn a_reply_from_another_thread_wakes_a_sleeping_engine() {
    use fixbolt_engine::dispatch::{RingApp, RingDispatch};
    use fixbolt_engine::ring;

    let (waker, handle) = Waker::new().expect("a pipe");
    let (to_app_tx, to_app_rx) = ring::pair(4096);
    let (to_engine_tx, to_engine_rx) = ring::pair(4096);

    type RingEngine = Engine<
        TcpTransport,
        fixbolt_session::Acceptor,
        RingDispatch<512>,
        ManualClock,
        Block,
        Store,
        256,
        4096,
        8192,
    >;
    let mut engine: RingEngine = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TEST"),
        RingDispatch::new(to_app_tx, to_engine_rx),
        ManualClock::at(fixbolt_conformance::script::FIXED_TIME_MILLIS),
        Block::with_timeout_ms(8, 2_000),
        4,
    )
    .with_waker(waker);

    // The application thread: hand it something to reply to, then let it push.
    let mut app: RingApp<512> = RingApp::new(to_app_rx, to_engine_tx).with_waker(handle);
    struct Echo;
    impl fixbolt_session::Application for Echo {
        fn on_message(
            &mut self,
            m: &[u8],
            _s: u32,
            _t: &[u8],
            out: &mut [u8],
        ) -> Option<core::ops::Range<usize>> {
            out[..m.len()].copy_from_slice(m);
            Some(0..m.len())
        }
    }

    // Put one message on the outbound ring by hand, exactly as `deliver` would.
    {
        use fixbolt_engine::dispatch::Dispatch;
        engine
            .dispatch_mut()
            .deliver(0, b"35=D\x01", 2, b"20260828-12:00:00.000", &mut [0u8; 512]);
    }

    let pusher = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(app.pump(&mut Echo), 1, "the application saw the message");
        app
    });

    let before = Instant::now();
    engine.idle();
    let elapsed = before.elapsed();
    let app = pusher.join().expect("the application thread finished");
    assert_eq!(app.dropped(), 0);

    assert!(
        elapsed < Duration::from_millis(500),
        "the reply was pushed after 50 ms against a 2000 ms timeout; {elapsed:?} \
         means the engine slept through it — which is exactly the cliff the \
         waker exists to remove"
    );
}
