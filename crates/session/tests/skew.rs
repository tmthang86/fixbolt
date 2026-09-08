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

// ---------------------------------------------------------------------------
// Microsecond `SendingTime` — wave B plan 3, half A
// ---------------------------------------------------------------------------

/// `good_logon()` with its `52=` replaced, and `9=` / `10=` recomputed.
///
/// Both have to move: a 24-byte stamp is three bytes longer than the corpus's
/// 17, and `parse_into` checks the body length **before** it reaches the
/// checksum — so a message with only its checksum fixed would be refused for a
/// reason that has nothing to do with the clock, and this file would be
/// measuring the wrong rejection.
fn logon_stamped(sending_time: &str) -> Vec<u8> {
    const SOH: char = '\u{1}';
    let wire = String::from_utf8(good_logon()).expect("ascii");
    assert!(
        wire.contains(&format!("52=20260828-12:00:00{SOH}")),
        "the corpus stamps its `I` lines to the second; if that changed, so did this test's premise"
    );

    let mut head = String::new();
    let mut body = String::new();
    for field in wire.split(SOH).filter(|f| !f.is_empty()) {
        if field.starts_with("9=") || field.starts_with("10=") {
            continue;
        }
        if field.starts_with("8=") {
            head.push_str(field);
            head.push(SOH);
        } else if field.starts_with("52=") {
            body.push_str(&format!("52={sending_time}{SOH}"));
        } else {
            body.push_str(field);
            body.push(SOH);
        }
    }
    let mut out = format!("{head}9={}{SOH}{body}", body.len()).into_bytes();
    let sum: u8 = out.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    out.extend_from_slice(format!("10={sum:03}{SOH}").as_bytes());
    out
}

/// **A valid timestamp this engine cannot read, and the answer is silence.**
///
/// FIX 4.4 puts 17 or 21 bytes on the wire; a European venue under MiFID II
/// RTS 25 puts 24. Before wave B plan 3 `parse_utc` returned `None` for that
/// width, `time_ok` went false, and `AwaitingLogon` hung up **without sending a
/// byte** — the treatment a *wrong* clock earns, applied to a *right* one.
///
/// Three assertions, because each fails for its own reason: the link, the
/// measurement, and whether anything was said. The skew is `-123` and not
/// `-124`: the sub-millisecond part is truncated, never rounded.
#[test]
fn a_microsecond_sending_time_is_read_and_the_link_survives() {
    let mut session = acceptor();
    session.connect(|_| {});
    session.tick(FIXED_TIME_MILLIS, |_| {});

    let mut out: Vec<u8> = Vec::new();
    let link = session.received(&logon_stamped("20260828-12:00:00.123456"), |b| {
        out.extend_from_slice(b);
    });

    assert_eq!(
        link,
        fixbolt_session::Link::Up,
        "a valid microsecond SendingTime is not a reason to hang up"
    );
    assert_eq!(
        session.last_skew_ms(),
        Some(-123),
        "truncated to milliseconds, not rounded: .123456 is 123 ms ahead of us"
    );
    assert!(
        out.windows(5).any(|w| w == b"\x0135=A"),
        "and the session answered, rather than saying nothing at all"
    );
}
