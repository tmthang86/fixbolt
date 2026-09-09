//! Reading a FIX `UTCTimestamp`, and the epoch the session counts from.
//!
//! # Why milliseconds since 0000-01-01, and not since 1970
//!
//! `Input::Tick` carries a `u64`, and `SendingTime` is `YYYYMMDD-…` — four
//! digits, so the wire can name any year from 0000 to 9999. Counted from 1970,
//! **more than a fifth of that range is not representable at all**, and a
//! counterparty sending `52=19600101-00:00:00` would wrap the subtraction into
//! a skew of half a billion years — which passes no check but crosses one.
//!
//! Counting from 0000-01-01 makes every timestamp FIX can express a
//! non-negative `u64`, so the skew is a plain `abs_diff` that cannot wrap and
//! the parser never needs a signed type. The engine converts once, at the edge:
//! `year_zero_millis = unix_millis + MILLIS_YEAR_ZERO_TO_EPOCH`.

/// Days from 0000-01-01 to 1970-01-01, proleptic Gregorian.
///
/// Not a remembered constant: the test `the_epoch_offset_is_derived_not_recalled`
/// derives it from `days_from_civil` rather than trusting this line.
pub const DAYS_YEAR_ZERO_TO_EPOCH: i64 = 719_528;

/// [`DAYS_YEAR_ZERO_TO_EPOCH`] in milliseconds. What the engine adds to a
/// `SystemTime` reading to get the scale `Input::Tick` uses.
pub const MILLIS_YEAR_ZERO_TO_EPOCH: u64 = 719_528 * 86_400_000;

/// A `UTCTimestamp` with no fraction: `YYYYMMDD-HH:MM:SS`.
///
/// FIX 4.4 documents this and the 21-byte millisecond form, and the corpus uses
/// both — 17 bytes on `I` lines, 21 on `E`. Everything wider is what a venue
/// actually sends: the FIX EP wording adds three, six and nine fractional
/// digits, the Technical Addendum on time precision adds twelve, and MiFID II
/// RTS 25 requires a clock synchronised to the microsecond, so a European
/// counterparty stamps `52=` with six digits and expects to be understood.
const LEN_SECONDS: usize = 17;

/// The widest a `UTCTimestamp` gets: `LEN_SECONDS`, a `.`, and twelve digits.
///
/// **Picoseconds, and this engine refused them until [ADR-0058].** QuickFIX/J
/// accepts exactly 30 bytes; QuickFIX C++ stops at 27; quickfix-go stops at 27;
/// QuickFIX/n has no ceiling at all. `reference/prior-art.md` has the survey and
/// the reason a *reader* takes the union rather than any one of those sets: a
/// width accepted here can never break interoperability with a stricter engine,
/// because a stricter engine never sends one.
///
/// **The corpus cannot see any of this**: 0 of the 59 definitions carry a
/// stamp wider than 21 bytes, so `59 / 59` says this change broke nothing and
/// says nothing about what it added. `crates/session/tests/timestamp_widths.rs`
/// asks the question the corpus cannot, and `scripts/interop.sh` §4i asks it of
/// somebody else's engine.
///
/// [ADR-0058]: ../../../docs/decisions/ADR-0058-a-timestamp-is-read-at-every-precision-and-written-at-three.md
const LEN_MAX: usize = 30;

/// Milliseconds since 0000-01-01T00:00:00Z, or `None` if `s` is not a
/// `UTCTimestamp`.
///
/// Rejects rather than repairs: a field whose length is not [`LEN_SECONDS`] or
/// between `LEN_SECONDS + 2` and [`LEN_MAX`], or that holds a digit out of
/// range, is not a timestamp. The session
/// turns `None` into a refusal, which is what `1d_InvalidLogonBadSendingTime`
/// asks for — and, until 2026-09-08, is also what a *valid* microsecond stamp
/// got: `None`, then `Refusal::BadSendingTime`, then a hang-up with no byte
/// sent, because before a `Logon` there is no session to answer with.
///
/// # Anything finer than a millisecond is dropped, not rounded
///
/// The return type is milliseconds because `Input::Tick` is milliseconds
/// (`DESIGN.md` D13), and skew, schedules and heartbeats are all measured in
/// them. `.123999` is 123 ms: truncation loses at most 999 µs of a skew
/// measured against a 120 000 ms bound, where rounding would let a stamp
/// arrive one millisecond in the future.
///
/// # The fraction is positional, and a short one is padded on the right
///
/// `.1` is a *tenth of a second* — 100 ms — not one millisecond. Reading the
/// digits as an integer is wrong by up to 99 ms and no gate here could see it,
/// which is why [ADR-0058] decision 3 says this out loud and
/// `tests/timestamp_widths.rs` holds it.
///
/// **Reading a stamp is not sending one.** `[corrected 2026-09-09]` this said
/// the send half "is not built" for a day after it was; the widths written are
/// 21, 24 or 27 and the key is `TimestampPrecision` (ADR-0057). What is read
/// here is deliberately wider than what is written — strict out, liberal in,
/// [ADR-0058] decision 4.
#[must_use]
pub fn parse_utc(s: &[u8]) -> Option<u64> {
    // Width picks the number of fractional digits, and a length outside the
    // range is not a timestamp. `get` rather than `s[8]`: this file carried a
    // file-wide `#![allow(clippy::indexing_slicing)]` until half A, and
    // non-negotiable 7 is about a panic in a library crate, which a subscript
    // is.
    //
    // One rule, not a table of accepted widths. The table was the shape that
    // let half A widen this reader and leave `dict`'s
    // (`reference/one-field-two-readers.md`); a rule cannot be half-updated.
    //
    // `LEN_SECONDS + 1` — a `.` with nothing after it — is **not** a width.
    // ADR-0058 decision 2: QuickFIX C++ takes it as `fraction = 0` and cannot
    // ever send one, so the divergence is unreachable from the oracle.
    let frac_digits = match s.len() {
        LEN_SECONDS => 0,
        n if (LEN_SECONDS + 2..=LEN_MAX).contains(&n) => n - LEN_SECONDS - 1,
        _ => return None,
    };
    if s.get(8) != Some(&b'-') || s.get(11) != Some(&b':') || s.get(14) != Some(&b':') {
        return None;
    }
    let year = num(s, 0, 4)?;
    let month = num(s, 4, 2)?;
    let day = num(s, 6, 2)?;
    let hour = num(s, 9, 2)?;
    let minute = num(s, 12, 2)?;
    let second = num(s, 15, 2)?;
    let milli = if frac_digits == 0 {
        0
    } else {
        if s.get(LEN_SECONDS) != Some(&b'.') {
            return None;
        }
        let frac = s.get(LEN_SECONDS + 1..)?;
        // Every fractional digit is checked, including the ones dropped below:
        // `20260828-12:00:00.123abc` is not a timestamp whose tail happens to
        // be unreadable, it is not a timestamp.
        if !frac.iter().all(u8::is_ascii_digit) {
            return None;
        }
        // **Positional, not an integer, and this is the whole trap.** A
        // fraction is a decimal fraction: `.1` is one tenth of a second, so a
        // digit's place decides its value and a short fraction is padded on the
        // right. Reading `.1` as the number 1 would give 1 ms instead of 100 —
        // wrong by 99 ms, and invisible to every gate here, because a skew is
        // judged against `max_skew_ms`, 120 000 by default.
        //
        // Two independent implementations agree: QuickFIX C++ multiplies the
        // fraction by `PRECISION_FACTOR[digits]` (`FieldTypes.h:56`), and
        // QuickFIX/n accumulates from `decimalBase = 0.1` downwards. Held by
        // `tests/timestamp_widths.rs::a_single_fractional_digit_is_a_tenth_of_a_second`.
        let digit = |i: usize| u32::from(frac.get(i).copied().unwrap_or(b'0') - b'0');
        digit(0) * 100 + digit(1) * 10 + digit(2)
    };

    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    // 60 is a leap second. FIX permits it; treat it as the last second of the
    // minute rather than rolling into the next, which is what every engine on
    // the wire does.
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let days = days_from_civil(i64::from(year), month, day) + DAYS_YEAR_ZERO_TO_EPOCH;
    // Non-negative for every year FIX can express, which is the whole point of
    // the epoch choice above.
    let days = u64::try_from(days).ok()?;
    Some(
        days * 86_400_000
            + u64::from(hour) * 3_600_000
            + u64::from(minute) * 60_000
            + u64::from(second) * 1_000
            + u64::from(milli),
    )
}

/// `width` decimal digits starting at `at`. `None` on anything else — a `+`, a
/// space, a letter.
fn num(s: &[u8], at: usize, width: usize) -> Option<u32> {
    let mut v: u32 = 0;
    for &b in s.get(at..at + width)? {
        if !b.is_ascii_digit() {
            return None;
        }
        v = v * 10 + u32::from(b - b'0');
    }
    Some(v)
}

const fn is_leap(y: u32) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

const fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01. Howard Hinnant's `days_from_civil` — the exact
/// inverse of `fixbolt_codec::timestamp`'s `civil_from_days`, which is what
/// [`tests::a_timestamp_survives_a_round_trip_through_the_codec`] uses as its
/// oracle.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // [0, 11]
    let doy = (153 * u64::from(mp) + 2) / 5 + u64::from(d) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe as i64 - 719_468
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a test asserting a constant is not a library call site"
)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_offset_is_derived_not_recalled() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(-days_from_civil(0, 1, 1), DAYS_YEAR_ZERO_TO_EPOCH);
        assert_eq!(
            MILLIS_YEAR_ZERO_TO_EPOCH,
            (DAYS_YEAR_ZERO_TO_EPOCH as u64) * 86_400_000
        );
    }

    #[test]
    fn a_year_before_1970_is_an_ordinary_number_here() {
        // The whole reason for the epoch. Under a Unix-epoch `u64` the first of
        // these is unrepresentable and the subtraction below wraps.
        let old = parse_utc(b"19600101-00:00:00").expect("1960 is a year");
        let new = parse_utc(b"20260828-12:00:00").expect("2026 is a year");
        assert!(old < new);
        assert!(new.abs_diff(old) > 66 * 365 * 86_400_000);
    }

    #[test]
    fn the_corpus_placeholder_is_not_a_date() {
        // `00000000-00:00:00` is month 00, day 00. The corpus writes it as a
        // placeholder for output nothing compares; a `SendingTime` check that
        // accepted it would accept anything. See `script::FIXED_TIME_IN`.
        assert_eq!(parse_utc(b"00000000-00:00:00"), None);
        assert_eq!(parse_utc(b"00000000-00:00:00.000"), None);
    }

    #[test]
    fn a_timestamp_survives_a_round_trip_through_the_codec() {
        // `civil_from_days` in `codec` is the inverse function. Feeding it what
        // this module produced is an oracle that is not this module.
        let mut cache = fixbolt_codec::TimestampCache::new();
        for s in [
            "19700101-00:00:00.000",
            "20260828-13:45:59.123",
            "20000229-23:59:59.999",
            "19991231-00:00:00.000",
            "24000101-12:00:00.500",
        ] {
            let ms = parse_utc(s.as_bytes()).expect(s);
            let unix = ms - MILLIS_YEAR_ZERO_TO_EPOCH;
            assert_eq!(core::str::from_utf8(cache.format(unix, 0)), Ok(s), "{s}");
        }
    }

    #[test]
    fn a_bad_sending_time_is_refused_rather_than_repaired() {
        for s in [
            &b""[..],
            b"20260828",           // date only
            b"20260828-13:45",     // no seconds
            b"20260828 13:45:59",  // space, not `-`
            b"20260828-13:45:59.", // `[amended 2026-09-09]` a `.` and no digits.
            // Two-digit millis stood here and is now a timestamp worth 120 ms
            // — ADR-0058 decision 1. A bare `.` replaces it, because that is
            // the case still refused and decision 2 is why.
            b"2026082X-13:45:59",     // not a digit
            b"20261328-13:45:59",     // month 13
            b"20260230-13:45:59",     // 30 February
            b"20250229-13:45:59",     // 2025 is not a leap year
            b"20260828-24:00:00",     // hour 24
            b"20260828-13:60:00",     // minute 60
            b"20260828-13:45:61",     // second 61
            b"20260828-13:45:59,123", // comma, not `.`
        ] {
            assert_eq!(parse_utc(s), None, "{}", String::from_utf8_lossy(s));
        }
    }

    #[test]
    fn all_four_widths_name_the_same_instant() {
        // The whole point: four spellings, one millisecond. If any of these
        // disagreed, a venue would be judged for skew against a different
        // instant depending on how precisely it can read its own clock.
        let base = parse_utc(b"20260828-12:00:00").expect("17 bytes");
        assert_eq!(parse_utc(b"20260828-12:00:00.000"), Some(base), "21 bytes");
        assert_eq!(
            parse_utc(b"20260828-12:00:00.000000"),
            Some(base),
            "24 bytes"
        );
        assert_eq!(
            parse_utc(b"20260828-12:00:00.000000000"),
            Some(base),
            "27 bytes"
        );
    }

    #[test]
    fn anything_finer_than_a_millisecond_is_dropped_and_not_rounded() {
        // `.999999` is 999 ms, not 1 000. Rounding here would put a stamp one
        // millisecond into the future, and `last_skew_ms` would report a
        // counterparty's clock as ahead when it is exactly right.
        let sec = parse_utc(b"20260828-12:00:00").expect("17 bytes");
        assert_eq!(parse_utc(b"20260828-12:00:00.999999"), Some(sec + 999));
        assert_eq!(parse_utc(b"20260828-12:00:00.999999999"), Some(sec + 999));
        assert_eq!(parse_utc(b"20260828-12:00:00.123456"), Some(sec + 123));
        assert_eq!(parse_utc(b"20260828-12:00:00.123999999"), Some(sec + 123));
    }

    #[test]
    fn a_wide_stamp_is_still_refused_when_it_is_not_a_timestamp() {
        for s in [
            // `[amended 2026-09-09, ADR-0058]` **four entries here were widths
            // and nothing else, and widths are no longer a reason to refuse.**
            // 23, 25, 26 and 28 bytes are timestamps now. What this test is
            // actually for — a stamp that is wide *and* malformed — is
            // unchanged, and the boundary cases replace the width-only ones.
            &b"20260828-12:00:00:123456"[..], // `:` where the `.` belongs
            b"20260828-12:00:00.123abc",      // the dropped digits are not digits
            b"20260828-12:00:00.",            // 18 bytes: a `.` and no fraction
            b"20260828-12:00:00.1234567890123", // 31 bytes: past picoseconds
            b"20261328-12:00:00.123456",      // month 13, at the new width
            b"20260828-12:00:00.12345678901a", // 30 bytes, and the last is not a digit
        ] {
            assert_eq!(parse_utc(s), None, "{}", String::from_utf8_lossy(s));
        }
    }

    #[test]
    fn a_leap_second_and_a_leap_day_are_both_accepted() {
        assert!(parse_utc(b"20161231-23:59:60").is_some());
        assert!(parse_utc(b"20240229-00:00:00").is_some());
    }
}
