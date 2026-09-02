//! Stopping the engine without lying to the counterparty.
//!
//! **Step 1 of [an-ordered-shutdown], red at an assertion.**
//!
//! # The specification, as the counterparty experiences it
//!
//! A planned shutdown and a dead line look identical from the other end unless
//! a `Logout` arrives. One means *"they are doing maintenance"*; the other
//! means *"reconnect, and keep reconnecting"*. So the assertion is on the bytes
//! the counterparty receives, and it does not change between the step that
//! fails and the step that passes.
//!
//! # Why `ManualClock`
//!
//! The deadline is milliseconds on the engine's clock. A test that waited on a
//! real one would be slow when it passed and flaky when it did not.
//!
//! [an-ordered-shutdown]: ../../../docs/plans/2026-09-02-an-ordered-shutdown.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};

const N: usize = 256;
const RX: usize = 4096;
const TX: usize = 8192;

type Local = Engine<
    Loopback,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    ManualClock,
    Yield,
    Store,
    N,
    RX,
    TX,
>;

#[derive(Default)]
struct EchoApp(fixbolt_conformance::echo::Echo);

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        self.0.reply(msg, seq, stamp, out)
    }
}

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

fn message(body: &str) -> Vec<u8> {
    let stamp = fixbolt_conformance::script::FIXED_TIME_IN;
    let cut = body.find('\u{1}').expect("a first field") + 1;
    let (head, rest) = body.split_at(cut);
    let inner = format!("{head}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}{rest}");
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    with_real_checksum(framed.as_bytes())
}

fn local() -> (Local, Loopback) {
    let mut engine: Local = Engine::new(
        cfg(),
        InlineDispatch::new(EchoApp::default()),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let (peer, side) = Loopback::pair();
    engine.add(side);
    (engine, peer)
}

/// Everything the engine has said since the last call, `|` for `SOH`.
fn said(peer: &mut Loopback) -> String {
    let mut sink = [0u8; 16384];
    let n = match peer.recv(&mut sink) {
        Io::Ready(n) => n,
        _ => 0,
    };
    String::from_utf8_lossy(&sink[..n]).replace('\u{1}', "|")
}

/// Log a session on and drain the reply.
fn log_on(engine: &mut Local, peer: &mut Loopback) {
    let _ = peer.send(&message("35=A\u{1}34=1\u{1}98=0\u{1}108=30\u{1}"));
    engine.turn();
    let reply = said(peer);
    assert!(reply.contains("|35=A|"), "the premise: logged on: {reply}");
}

/// **The specification.** The engine is stopped, and the counterparty is told.
///
/// `[verified 2026-09-02]` there is no way to stop it: `run()` returns `!`,
/// `serve` returns `Result<Infallible, _>`, and the only exit is killing the
/// process — which sends nothing.
#[test]
fn a_counterparty_is_told_that_the_engine_is_going_away() {
    let (mut engine, mut peer) = local();
    log_on(&mut engine, &mut peer);

    let admin = engine.admin();
    admin.shutdown(30_000);
    for _ in 0..10 {
        engine.turn();
    }

    let goodbye = said(&mut peer);
    assert!(
        goodbye.contains("|35=5|"),
        "a planned shutdown and a dead line are the same thing to the other \
         end unless a Logout arrives: {goodbye:?}"
    );
}

/// The control: an engine that is simply running says nothing unprompted, so
/// the assertion above is about the shutdown rather than about noise.
#[test]
fn a_running_engine_says_nothing_unprompted() {
    let (mut engine, mut peer) = local();
    log_on(&mut engine, &mut peer);
    for _ in 0..10 {
        engine.turn();
    }
    let quiet = said(&mut peer);
    assert!(
        !quiet.contains("|35=5|"),
        "an engine nobody stopped must not say goodbye: {quiet:?}"
    );
}

/// **A counterparty that answers is a clean shutdown, and the report says so.**
#[test]
fn an_answered_goodbye_is_a_clean_shutdown() {
    let (mut engine, mut peer) = local();
    log_on(&mut engine, &mut peer);
    let admin = engine.admin();

    admin.shutdown(30_000);
    engine.turn();
    let goodbye = said(&mut peer);
    assert!(goodbye.contains("|35=5|"), "we said goodbye: {goodbye}");
    assert!(
        goodbye.contains("|58=shutting down|"),
        "and said why — not one of D10's two texts, because nobody is at \
         fault here: {goodbye}"
    );

    // They answer. `98=`/`108=` are not defined for a Logout.
    let _ = peer.send(&message("35=5\u{1}34=2\u{1}"));
    engine.turn();

    let done = engine.shutdown_finished().expect("it is over");
    assert_eq!(done.sessions(), 1);
    assert_eq!(done.said_goodbye(), 1);
    assert_eq!(done.acked(), 1, "they answered: {done:?}");
    assert_eq!(done.timed_out(), 0);
    assert!(done.clean(), "{done:?}");
}

/// **A counterparty that never answers must not hold the shutdown open**, and
/// the report must not call that clean.
///
/// This is the reversal-2 case as a test: without a deadline it would run for
/// ever, so a hang here is a failure and not a slow pass.
#[test]
fn a_silent_counterparty_does_not_hold_the_shutdown_open() {
    let (mut engine, mut peer) = local();
    log_on(&mut engine, &mut peer);
    let admin = engine.admin();

    admin.shutdown(30_000);
    engine.turn();
    assert!(said(&mut peer).contains("|35=5|"), "we said goodbye");

    // They say nothing at all. Before the deadline, the shutdown waits.
    for _ in 0..50 {
        engine.turn();
    }
    assert!(
        engine.shutdown_finished().is_none(),
        "inside the grace period there is still hope of an answer"
    );

    // The clock moves past the deadline. Nothing else changes.
    engine.clock_mut().set(FIXED_TIME_MILLIS + 30_001);
    engine.turn();

    let done = engine.shutdown_finished().expect("the deadline passed");
    assert_eq!(done.sessions(), 1);
    assert_eq!(done.said_goodbye(), 1);
    assert_eq!(done.acked(), 0);
    assert_eq!(done.timed_out(), 1, "{done:?}");
    assert!(
        !done.clean(),
        "an operator restarting after this may have to reconcile by hand, and \
         must not be told it was clean: {done:?}"
    );
}

/// A connection that never logged on has nothing to say goodbye to, and must
/// not be waited for.
#[test]
fn a_connection_that_never_logged_on_is_not_waited_for() {
    let (mut engine, _peer) = local();
    let admin = engine.admin();

    admin.shutdown(30_000);
    for _ in 0..5 {
        engine.turn();
    }

    let done = engine.shutdown_finished().expect("nothing to wait for");
    assert_eq!(done.sessions(), 1, "there was a connection");
    assert_eq!(done.said_goodbye(), 0, "but no session to tell");
    assert_eq!(done.timed_out(), 0, "and nothing to wait out: {done:?}");
}

/// An engine with no connections at all stops at once rather than waiting out
/// its whole grace period for nobody.
#[test]
fn an_empty_engine_stops_at_once() {
    let mut engine: Local = Engine::new(
        cfg(),
        InlineDispatch::new(EchoApp::default()),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let admin = engine.admin();
    admin.shutdown(3_600_000);
    engine.turn();
    let done = engine.shutdown_finished().expect("nothing to wait for");
    assert_eq!(done.sessions(), 0);
    assert!(done.clean(), "{done:?}");
}

/// **`run` returns.** `[2026-09-02]` it used to be `-> !`, so the only way out
/// of this engine was killing the process.
#[test]
fn run_returns_when_it_is_asked_to() {
    let (mut engine, mut peer) = local();
    log_on(&mut engine, &mut peer);
    let admin = engine.admin();

    // From another thread, as an operator would.
    let asker = std::thread::spawn(move || admin.shutdown(0));
    asker.join().expect("join");

    let done = engine.run();
    assert_eq!(done.sessions(), 1);
    assert!(
        done.said_goodbye() >= 1,
        "the goodbye was written before the deadline: {done:?}"
    );
}

/// Asking twice must not extend a shutdown already under way.
#[test]
fn a_second_ask_does_not_extend_the_first() {
    let (mut engine, mut peer) = local();
    log_on(&mut engine, &mut peer);
    let admin = engine.admin();

    admin.shutdown(1_000);
    engine.turn();
    admin.shutdown(3_600_000);

    engine.clock_mut().set(FIXED_TIME_MILLIS + 1_001);
    engine.turn();
    assert!(
        engine.shutdown_finished().is_some(),
        "the first grace period stands"
    );
}

/// **"One relaxed load" made falsifiable**, the same way `Admin::drains()` does
/// for the command queue. An engine nobody has stopped must not be doing work
/// for the possibility.
#[test]
fn an_engine_nobody_stopped_never_begins() {
    let (mut engine, mut peer) = local();
    log_on(&mut engine, &mut peer);
    let _admin = engine.admin();

    for _ in 0..1_000 {
        engine.turn();
    }
    assert!(
        engine.shutdown_finished().is_none(),
        "nobody asked, so there is no shutdown to finish"
    );
    let quiet = said(&mut peer);
    assert!(!quiet.contains("|35=5|"), "and nothing was said: {quiet}");
}

/// **The trap this shutdown path exists inside.** `[measured 2026-08-30]`
/// dropping the engine while another thread still held a `WakeHandle` closed
/// the self-pipe's read end; `libc::write` into the write end raised `SIGPIPE`,
/// whose default action terminates the process.
///
/// An ordered shutdown is precisely when an engine gets dropped with handles
/// still out, so the sequence is driven here rather than assumed safe. It is
/// invisible to an ordinary Rust test — the runtime sets `SIG_IGN` before
/// `main` — so what this asserts is the weaker, honest thing: **the sequence
/// runs to completion and the wake after it is not an error.**
#[cfg(all(feature = "standard", unix))]
#[test]
fn shutting_down_and_dropping_with_a_live_handle_is_survivable() {
    let (waker, handle) = fixbolt_engine::waker::Waker::new().expect("a self-pipe");
    let mut engine: Local = Engine::new(
        cfg(),
        InlineDispatch::new(EchoApp::default()),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    )
    .with_waker(waker);
    let (mut peer, side) = Loopback::pair();
    engine.add(side);
    log_on(&mut engine, &mut peer);

    let admin = engine.admin();
    admin.shutdown(0);
    engine.turn();
    let done = engine.shutdown_finished().expect("asked with no grace");
    assert_eq!(done.sessions(), 1);

    drop(engine);
    // The handle outlives the engine, which is the whole shape of the bug.
    handle.wake();
    handle.wake();
}
