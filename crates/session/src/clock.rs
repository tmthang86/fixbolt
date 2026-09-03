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

/// The two widths FIX 4.4 puts on the wire: `YYYYMMDD-HH:MM:SS` and the same
/// with `.sss`. The corpus uses both — 17 bytes on `I` lines, 21 on `E`.
const LEN_SECONDS: usize = 17;
const LEN_MILLIS: usize = 21;

/// Milliseconds since 0000-01-01T00:00:00Z, or `None` if `s` is not a
/// `UTCTimestamp`.
///
/// Rejects rather than repairs: a field that is not exactly one of the two
/// documented widths, or that holds a digit out of range, is not a timestamp.
/// The session turns `None` into a refusal, which is what
/// `1d_InvalidLogonBadSendingTime` asks for.
#[must_use]
pub fn parse_utc(s: &[u8]) -> Option<u64> {
    if s.len() != LEN_SECONDS && s.len() != LEN_MILLIS {
        return None;
    }
    if s[8] != b'-' || s[11] != b':' || s[14] != b':' {
        return None;
    }
    let year = num(s, 0, 4)?;
    let month = num(s, 4, 2)?;
    let day = num(s, 6, 2)?;
    let hour = num(s, 9, 2)?;
    let minute = num(s, 12, 2)?;
    let second = num(s, 15, 2)?;
    let milli = if s.len() == LEN_MILLIS {
        if s[17] != b'.' {
            return None;
        }
        num(s, 18, 3)?
    } else {
        0
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
            assert_eq!(core::str::from_utf8(cache.format(unix)), Ok(s), "{s}");
        }
    }

    #[test]
    fn a_bad_sending_time_is_refused_rather_than_repaired() {
        for s in [
            &b""[..],
            b"20260828",              // date only
            b"20260828-13:45",        // no seconds
            b"20260828 13:45:59",     // space, not `-`
            b"20260828-13:45:59.12",  // two-digit millis
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
    fn a_leap_second_and_a_leap_day_are_both_accepted() {
        assert!(parse_utc(b"20161231-23:59:60").is_some());
        assert!(parse_utc(b"20240229-00:00:00").is_some());
    }
}
