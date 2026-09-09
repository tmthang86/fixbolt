//! `52=SendingTime` on the way **out**, at three widths.
//!
//! Half A of `timestamp-micros` made this engine *read* a 24- or 27-byte stamp.
//! This file is the other direction, and it is
//! [ADR-0057](../../../docs/decisions/ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md)'s
//! half: a venue on MiFID II RTS 25 requires microsecond `52=` **from** this
//! engine, and until now every message it sent carried 21 bytes at every
//! configuration.
//!
//! # The corpus cannot see any of this, and that is not a complaint
//!
//! 0 of the 59 acceptance definitions carry a stamp wider than 21 bytes. Running
//! them green says *nothing regressed*; it does not say this works. This file
//! is what says it works, and `scripts/interop.sh` at
//! `TimestampPrecision=6` is the only second opinion that exists.
//!
//! # The assertion that matters most is the one about padding
//!
//! `a_configured_width_is_a_ceiling_and_not_a_promise` is the reason
//! [`Precision::coarser`] exists. `.123000` from a clock that only knows
//! milliseconds is the option ADR-0057 declined by name: to a venue *measuring*
//! divergence it claims microsecond resolution falsely, where `.123` claims
//! millisecond resolution truthfully.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: `scripts/check-indexing-debt.sh` counts nothing
// outside `crates/*/src`, and an index that panics in a test is a failing test.
#![allow(clippy::indexing_slicing)]

use fixbolt_codec::Precision;
use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_session::{Acceptor, Config, Session};

fn cfg(precision: Precision) -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_timestamp_precision(precision)
}

/// A real Logon from the corpus rather than a hand-written packet (§7).
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
    let s = String::from_utf8(wire).expect("ascii");
    with_real_checksum(s.replace("56=DLSI", "56=ISLD").as_bytes())
}

/// `52=`'s value out of one whole message on the wire.
fn sending_time(msg: &[u8]) -> String {
    let s = String::from_utf8(msg.to_vec()).expect("ascii");
    s.split('\x01')
        .find_map(|f| f.strip_prefix("52=").map(str::to_owned))
        .expect("every message this layer sends carries 52=")
}

/// Log a session on and return the Logon it answers with.
///
/// `sub_ms_nanos` of `None` uses the millisecond door, which is what a caller
/// with no finer clock has.
fn logon_reply(precision: Precision, sub_ms_nanos: Option<u32>) -> Vec<u8> {
    let mut session: Session<Acceptor, 256> = Session::new(cfg(precision));
    session.connect(|_| {});
    match sub_ms_nanos {
        Some(n) => session.tick_at(FIXED_TIME_MILLIS, n, |_| {}),
        None => session.tick(FIXED_TIME_MILLIS, |_| {}),
    };
    let mut out = Vec::new();
    let link = session.received(&good_logon(), |b| out.extend_from_slice(b));
    assert_eq!(link, fixbolt_session::Link::Up, "the Logon was accepted");
    assert!(!out.is_empty(), "an accepted Logon is answered");
    out
}

/// **The default is every byte this engine has ever put on a wire.** A widened
/// default would break a venue that accepts only 21 bytes, to serve one that
/// asked for 24 — so it is opt-in, and this is the assertion that says so.
#[test]
fn the_default_still_sends_twenty_one_bytes() {
    assert_eq!(
        cfg(Precision::Millis).timestamp_precision(),
        Precision::Millis
    );
    assert_eq!(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").timestamp_precision(),
        Precision::Millis,
        "a configuration nobody touched writes milliseconds"
    );
    let stamp = sending_time(&logon_reply(Precision::Millis, Some(456_789)));
    assert_eq!(stamp.len(), 21, "{stamp}");
    assert!(stamp.ends_with(".000"), "{stamp}");
}

#[test]
fn a_microsecond_session_sends_twenty_four_bytes() {
    let stamp = sending_time(&logon_reply(Precision::Micros, Some(456_789)));
    assert_eq!(stamp.len(), 24, "{stamp}");
    assert!(stamp.ends_with(".000456"), "{stamp}");
}

#[test]
fn a_nanosecond_session_sends_twenty_seven_bytes() {
    let stamp = sending_time(&logon_reply(Precision::Nanos, Some(456_789)));
    assert_eq!(stamp.len(), 27, "{stamp}");
    assert!(stamp.ends_with(".000456789"), "{stamp}");
}

/// **ADR-0057's rejection of a padded stamp, held by a test rather than by a
/// paragraph.**
///
/// A session configured for microseconds but driven through the millisecond
/// door has no sub-millisecond clock behind it. It writes 21 bytes. It does
/// **not** write `.xxx000`, which would claim a resolution this end does not
/// have — and which is exactly what a naive implementation of this feature
/// produces, silently, on every message.
#[test]
fn a_configured_width_is_a_ceiling_and_not_a_promise() {
    for p in [Precision::Micros, Precision::Nanos] {
        let stamp = sending_time(&logon_reply(p, None));
        assert_eq!(
            stamp.len(),
            21,
            "a millisecond tick must not be padded out to {p:?}: {stamp}"
        );
    }
}

/// The whole message, not only the field: `9=` counts the body and `10=` sums
/// it, so a stamp three bytes wider than the template was built for would show
/// up here first.
///
/// **This is not the trap the plan wrote down.** It expected a fixed-width slot
/// that a wider value would overrun into `56=`. There is no such slot:
/// `Template::encode_with` writes `Part::Slot` at whatever length it is given
/// and derives `9=` from the write position. What is worth guarding is the
/// arithmetic, and that is what this does.
#[test]
fn the_body_length_and_checksum_follow_the_wider_stamp() {
    for (p, want) in [
        (Precision::Millis, 21),
        (Precision::Micros, 24),
        (Precision::Nanos, 27),
    ] {
        let msg = logon_reply(p, Some(456_789));
        assert_eq!(sending_time(&msg).len(), want);

        let s = String::from_utf8(msg.clone()).expect("ascii");
        let body_at = s.find("9=").expect("9= is there");
        let declared: usize = s[body_at + 2..]
            .split('\x01')
            .next()
            .expect("9= has a value")
            .parse()
            .expect("9= is a number");
        let body_starts = s[body_at..].find('\x01').expect("9= ends") + body_at + 1;
        let checksum_at = s.rfind("10=").expect("10= is there");
        assert_eq!(
            checksum_at - body_starts,
            declared,
            "9={declared} must count the bytes actually written at {p:?}"
        );

        let sum: u32 = msg[..checksum_at].iter().map(|b| u32::from(*b)).sum();
        let stated: u32 = s[checksum_at + 3..]
            .split('\x01')
            .next()
            .expect("10= has a value")
            .parse()
            .expect("10= is a number");
        assert_eq!(sum % 256, stated, "10= must sum what was written at {p:?}");
    }
}

/// A wider stamp must not disturb the field it sits between. Non-negotiable 5:
/// the order is the dictionary's, and it does not move because a value grew.
#[test]
fn a_wider_stamp_does_not_move_a_field() {
    let order = |p, n| {
        let msg = logon_reply(p, n);
        let s = String::from_utf8(msg).expect("ascii");
        s.split('\x01')
            .filter_map(|f| f.split('=').next().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        order(Precision::Millis, None),
        order(Precision::Nanos, Some(456_789)),
        "widening 52= changed which fields appear, or in what order"
    );
}
