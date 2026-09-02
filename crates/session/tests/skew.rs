//! The measured distance between our clock and theirs.
//!
//! **The corpus cannot see any of this.** `1d_InvalidLogonBadSendingTime.def` is
//! 2001 years out, so every bound and every sign reproduces it identically —
//! the same blind spot `STATUS.md` records for the 120-second default itself.
//! `Session::last_skew_ms` is held by this file alone.
//!
//! # Why the sign is asserted in both directions
//!
//! A skew reported with the wrong sign is worse than none: it sends whoever is
//! awake at 3 a.m. to adjust the wrong clock. One assertion at zero — which is
//! all `crates/engine/tests/observe.rs` can make, because the corpus's own
//! instant is what its engine reads — cannot tell `now - stamp` from
//! `stamp - now`.
//!
//! # Why a refused message is asserted too
//!
//! `max_skew_ms` refuses in silence, by protocol: before a `Logon` there is no
//! session to answer with. **The refusal is the case this number exists to
//! explain**, so recording it only on acceptance would leave it `None` exactly
//! when it is wanted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_session::{Acceptor, Config, Session};

fn acceptor() -> Session<Acceptor, 256> {
    Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
}

/// A real Logon from the corpus, stamped `20260828-12:00:00` — the instant
/// `FIXED_TIME_MILLIS` names. A hand-written packet would prove the parser
/// handles a packet nobody sends (`CLAUDE.md` §7).
fn good_logon() -> Vec<u8> {
    let wire = scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|s| s.file == "1c_InvalidTargetCompID.def")
        .expect("the corpus has it")
        .steps
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m) => Some(m.wire),
            _ => None,
        })
        .expect("it has an I line");
    // Its one deliberately wrong field, corrected: this file is about the
    // clock, not about identity.
    let s = String::from_utf8(wire).expect("ascii");
    let fixed = s.replace("56=DLSI", "56=ISLD");
    assert_ne!(
        fixed, s,
        "the file's wrong field is the one being corrected"
    );
    with_real_checksum(fixed.as_bytes())
}

/// Drive one session to `now_ms` and feed it the Logon. Returns what it
/// measured, and whether the link survived.
fn skew_at(now_ms: u64) -> (Option<i64>, bool) {
    let mut session = acceptor();
    session.connect(|_| {});
    session.tick(now_ms, |_| {});
    let link = session.received(&good_logon(), |_| {});
    (session.last_skew_ms(), link == fixbolt_session::Link::Up)
}

/// Nothing has arrived, so there is nothing to report. `None` is *"not yet"*,
/// and it must not be confused with a measured zero.
#[test]
fn a_session_that_has_received_nothing_reports_no_skew() {
    let mut session = acceptor();
    assert_eq!(session.last_skew_ms(), None);
    session.connect(|_| {});
    session.tick(FIXED_TIME_MILLIS, |_| {});
    assert_eq!(
        session.last_skew_ms(),
        None,
        "a tick is our own clock moving, not a message from them"
    );
}

/// Zero is a value. It is also the only thing the engine-level test can see,
/// which is why the two cases below exist.
#[test]
fn two_clocks_that_agree_measure_zero_and_not_nothing() {
    let (skew, up) = skew_at(FIXED_TIME_MILLIS);
    assert_eq!(skew, Some(0));
    assert!(up, "an in-sequence Logon at the right instant is accepted");
}

/// **Positive means their stamp is behind ours.** Get this backwards and the
/// operator adjusts the wrong machine.
#[test]
fn a_counterparty_whose_clock_lags_reports_a_positive_skew() {
    let (skew, up) = skew_at(FIXED_TIME_MILLIS + 5_000);
    assert_eq!(
        skew,
        Some(5_000),
        "our clock is five seconds ahead of their stamp"
    );
    assert!(up, "five seconds is well inside the 120-second default");
}

/// And the other direction, which is the half a single zero-valued assertion
/// cannot distinguish.
#[test]
fn a_counterparty_whose_clock_runs_ahead_reports_a_negative_skew() {
    let (skew, up) = skew_at(FIXED_TIME_MILLIS - 5_000);
    assert_eq!(
        skew,
        Some(-5_000),
        "their stamp is five seconds ahead of our clock"
    );
    assert!(up, "still inside the default bound");
}

/// **The case the field exists for.** The message is refused, the refusal is
/// silent, and the number that explains it is still there afterwards.
#[test]
fn a_message_refused_for_skew_still_records_the_skew_that_refused_it() {
    let over = FIXED_TIME_MILLIS + 200_000;
    let (skew, up) = skew_at(over);
    assert_eq!(
        skew,
        Some(200_000),
        "200 s is past the 120 s default, and the measurement survives the refusal"
    );
    assert!(
        !up,
        "and it really was refused — otherwise this test measures the accepted path"
    );
}
