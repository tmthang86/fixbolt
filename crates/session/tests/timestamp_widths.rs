//! Every precision a counterparty can be configured to send, read by both of
//! this codebase's readers, and the two of them held to the same answer.
//!
//! [ADR-0058] is the decision. `STATUS.md` open item 59 is the defect: a width
//! this engine does not read becomes `time_ok = false`, and in `AwaitingLogon`
//! that is a hang-up with **no byte sent** — the same silence half A of
//! `timestamp-micros` was written to close, one width over.
//!
//! **The corpus cannot see any of this.** 0 of the 59 definitions carry a stamp
//! wider than 21 bytes, so `59 / 59` means *nothing regressed* and cannot mean
//! *this works*. The gate that can is `scripts/interop.sh` §4i, where somebody
//! else's engine is configured for a width this one never sends.
//!
//! [ADR-0058]: ../../../docs/decisions/ADR-0058-a-timestamp-is-read-at-every-precision-and-written-at-three.md

use fixbolt_dict::FieldType;
use fixbolt_session::clock::parse_utc;

/// `20260909-10:00:00` plus `digits` fractional digits, all `1`.
fn stamp(digits: usize) -> String {
    let mut s = String::from("20260909-10:00:00");
    if digits > 0 {
        s.push('.');
        for _ in 0..digits {
            s.push('1');
        }
    }
    s
}

/// Milliseconds since year zero for `20260909-10:00:00`, no fraction.
const BASE_MS: u64 = 63_956_167_200_000;

#[test]
fn every_precision_from_zero_to_twelve_is_a_timestamp() {
    // ADR-0058 decision 1. The four this engine already took are in here too,
    // so a regression on them fails in the same test as a gap in the new ones.
    for digits in [0usize, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12] {
        let s = stamp(digits);
        let b = s.as_bytes();
        assert!(
            parse_utc(b).is_some(),
            "parse_utc refused {digits} fractional digits: {s} ({} bytes)",
            b.len()
        );
        assert!(
            FieldType::UtcTimestamp.accepts(b),
            "dict refused {digits} fractional digits: {s} ({} bytes)",
            b.len()
        );
    }
}

#[test]
fn a_single_fractional_digit_is_a_tenth_of_a_second() {
    // **The trap this test exists for.** A fraction is a decimal fraction, so
    // the digits are positional: `.1` is 100 ms. Reading them as an integer
    // gives 1 ms — wrong by 99 ms, and *no other gate here could see it*,
    // because every skew is judged against a 120 000 ms bound.
    //
    // Confirmed twice, from two independent implementations: QuickFIX C++'s
    // `PRECISION_FACTOR` table (`FieldTypes.h:56`), and QuickFIX/n's
    // `ParseFraction`, which starts at `decimalBase = 0.1` and multiplies down.
    assert_eq!(parse_utc(b"20260909-10:00:00.1"), Some(BASE_MS + 100));
    assert_eq!(parse_utc(b"20260909-10:00:00.12"), Some(BASE_MS + 120));
    assert_eq!(parse_utc(b"20260909-10:00:00.123"), Some(BASE_MS + 123));
    // Past three digits the remainder is dropped, never rounded — ADR-0057.
    assert_eq!(parse_utc(b"20260909-10:00:00.1239"), Some(BASE_MS + 123));
    assert_eq!(parse_utc(b"20260909-10:00:00.999999"), Some(BASE_MS + 999));
    assert_eq!(
        parse_utc(b"20260909-10:00:00.000000000001"),
        Some(BASE_MS),
        "twelve digits of almost nothing is still zero milliseconds"
    );
}

#[test]
fn a_dot_with_no_digits_is_not_a_timestamp() {
    // ADR-0058 decision 2, and a deliberate divergence: QuickFIX C++ accepts
    // this as `fraction = 0`. It can never *send* one — precision 0 writes no
    // `.` — so the divergence is unreachable through the oracle.
    let b = b"20260909-10:00:00.";
    assert_eq!(b.len(), 18);
    assert_eq!(parse_utc(b), None);
    assert!(!FieldType::UtcTimestamp.accepts(b));
}

#[test]
fn both_readers_agree_on_every_width() {
    // The half-A defect was one reader moving and the other staying, and only
    // somebody else's engine could see it — `reference/one-field-two-readers.md`.
    // This asks both the same question across every width in range and out.
    for len in 0..=34usize {
        let mut s = String::from("20260909-10:00:00.111111111111");
        s.truncate(len.min(s.len()));
        while s.len() < len {
            s.push('1');
        }
        let b = s.as_bytes();
        assert_eq!(
            parse_utc(b).is_some(),
            FieldType::UtcTimestamp.accepts(b),
            "the two readers disagree at {len} bytes: {s}"
        );
    }
}

#[test]
fn a_fractional_digit_that_is_not_a_digit_is_refused_at_every_width() {
    // Widening must not stop the tail being checked. Every one of these is a
    // legal *length* with an illegal body.
    for bad in [
        &b"20260909-10:00:00.a"[..],
        &b"20260909-10:00:00.12a"[..],
        &b"20260909-10:00:00.123abcd"[..],
        &b"20260909-10:00:00.12345678901a"[..],
    ] {
        assert_eq!(parse_utc(bad), None, "{}", String::from_utf8_lossy(bad));
        assert!(
            !FieldType::UtcTimestamp.accepts(bad),
            "{}",
            String::from_utf8_lossy(bad)
        );
    }
}

#[test]
fn nothing_wider_than_thirty_bytes_is_a_timestamp() {
    // The type has no form wider than picoseconds and no surveyed engine sends
    // one. ADR-0058 decision 1's ceiling.
    for digits in [13usize, 14, 17] {
        let s = stamp(digits);
        assert_eq!(parse_utc(s.as_bytes()), None, "{s}");
        assert!(!FieldType::UtcTimestamp.accepts(s.as_bytes()), "{s}");
    }
}
