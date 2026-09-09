//! `SendingTime` formatting, including the rollovers a per-minute cache can get
//! wrong in a way that only shows up at 00:00 or on New Year's Eve.
#![allow(clippy::unwrap_used, clippy::panic)]

use fixbolt_codec::{Precision, TimestampCache};

fn at(c: &mut TimestampCache, millis: u64, sub_ms_nanos: u32) -> String {
    String::from_utf8(c.format(millis, sub_ms_nanos).to_vec()).unwrap()
}

/// Milliseconds since the epoch for a UTC instant, computed independently of the
/// code under test — days_from_civil, the inverse of what `timestamp.rs` uses.
fn utc(y: i64, m: i64, d: i64, hh: u64, mm: u64, ss: u64, ms: u64) -> u64 {
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = (y2 - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe as i64 - 719_468;
    (days as u64) * 86_400_000 + hh * 3_600_000 + mm * 60_000 + ss * 1_000 + ms
}

#[test]
fn the_first_call_fills_an_empty_cache() {
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 10, 43, 7, 251), 0),
        "20260828-10:43:07.251"
    );
}

#[test]
fn the_second_within_the_minute_only_moves_the_seconds() {
    let mut c = TimestampCache::new();
    let base = utc(2026, 8, 28, 10, 43, 0, 0);
    assert_eq!(at(&mut c, base, 0), "20260828-10:43:00.000");
    assert_eq!(at(&mut c, base + 59_999, 0), "20260828-10:43:59.999");
}

#[test]
fn the_minute_rolls_over() {
    // The trap: a cache that only rebuilds when the HOUR changes leaves 12:34
    // in place and prints 12:34:00.000 for a message sent at 12:35.
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 12, 34, 59, 999), 0),
        "20260828-12:34:59.999"
    );
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 12, 35, 0, 0), 0),
        "20260828-12:35:00.000"
    );
}

#[test]
fn the_day_rolls_over() {
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 23, 59, 59, 999), 0),
        "20260828-23:59:59.999"
    );
    assert_eq!(
        at(&mut c, utc(2026, 8, 29, 0, 0, 0, 0), 0),
        "20260829-00:00:00.000"
    );
}

#[test]
fn the_year_rolls_over() {
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2026, 12, 31, 23, 59, 59, 999), 0),
        "20261231-23:59:59.999"
    );
    assert_eq!(
        at(&mut c, utc(2027, 1, 1, 0, 0, 0, 0), 0),
        "20270101-00:00:00.000"
    );
}

#[test]
fn a_leap_day_is_a_day() {
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2028, 2, 28, 12, 0, 0, 0), 0),
        "20280228-12:00:00.000"
    );
    assert_eq!(
        at(&mut c, utc(2028, 2, 29, 12, 0, 0, 0), 0),
        "20280229-12:00:00.000"
    );
    assert_eq!(
        at(&mut c, utc(2028, 3, 1, 12, 0, 0, 0), 0),
        "20280301-12:00:00.000"
    );
    // 2100 is not a leap year, and a naive %4 rule says it is.
    assert_eq!(
        at(&mut c, utc(2100, 2, 28, 12, 0, 0, 0), 0),
        "21000228-12:00:00.000"
    );
    assert_eq!(
        at(&mut c, utc(2100, 3, 1, 12, 0, 0, 0), 0),
        "21000301-12:00:00.000"
    );
}

#[test]
fn going_backwards_still_rebuilds() {
    // Nothing forbids a caller passing an earlier instant — a replayed journal
    // does exactly that. A cache keyed on "has the minute increased" would keep
    // the newer prefix and print the wrong time for every replayed message.
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 10, 0, 0, 0), 0),
        "20260828-10:00:00.000"
    );
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 9, 0, 0, 0), 0),
        "20260828-09:00:00.000"
    );
}

#[test]
fn the_epoch_itself() {
    let mut c = TimestampCache::new();
    assert_eq!(at(&mut c, 0, 0), "19700101-00:00:00.000");
}

/// **B1's target behaviour.** Its predecessor asserted only that the cache
/// *could not* exceed 21 bytes at all — red at `left: 21, right: 24`, quoted in
/// the plan's delivery log. That assertion could never become true, because the
/// default must stay milliseconds; the target needs a type that did not exist
/// when the red was run, which is where half A's "same test, no edited
/// assertion" pattern stops transferring to a step that *adds* an API.
#[test]
fn a_cache_asked_for_microseconds_writes_twenty_four_bytes() {
    let mut c = TimestampCache::with_precision(Precision::Micros);
    let rendered = at(&mut c, utc(2026, 8, 28, 10, 43, 7, 251), 456_789);
    assert_eq!(rendered, "20260828-10:43:07.251456");
    assert_eq!(rendered.len(), 24);
}

#[test]
fn a_cache_asked_for_nanoseconds_writes_twenty_seven_bytes() {
    let mut c = TimestampCache::with_precision(Precision::Nanos);
    let rendered = at(&mut c, utc(2026, 8, 28, 10, 43, 7, 251), 456_789);
    assert_eq!(rendered, "20260828-10:43:07.251456789");
    assert_eq!(rendered.len(), 27);
}

/// **The default is unchanged, and this is the assertion the whole change is
/// allowed to break and must not.** Every byte this engine has ever put on a
/// wire came out of a cache built by `new()`.
#[test]
fn the_default_is_still_milliseconds_to_the_byte() {
    let mut c = TimestampCache::new();
    assert_eq!(c.precision(), Precision::Millis);
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 10, 43, 7, 251), 456_789),
        "20260828-10:43:07.251"
    );
}

/// A caller with no sub-millisecond clock passes `0` and says nothing untrue;
/// a caller that passes one anyway is ignored rather than obeyed.
#[test]
fn the_remainder_is_ignored_at_millisecond_precision() {
    let mut a = TimestampCache::new();
    let mut b = TimestampCache::new();
    let t = utc(2026, 8, 28, 10, 43, 7, 251);
    assert_eq!(at(&mut a, t, 0), at(&mut b, t, 999_999));
}

/// The per-minute prefix cache is the one piece of state here, and a wider
/// fraction gives it more chances to be stale. Crossing a minute must rebuild
/// at every precision, not only at the one the tests happened to use.
#[test]
fn crossing_a_minute_rebuilds_at_every_precision() {
    for (p, before, after) in [
        (
            Precision::Millis,
            "20261231-23:59:59.999",
            "20270101-00:00:00.000",
        ),
        (
            Precision::Micros,
            "20261231-23:59:59.999123",
            "20270101-00:00:00.000000",
        ),
        (
            Precision::Nanos,
            "20261231-23:59:59.999123456",
            "20270101-00:00:00.000000000",
        ),
    ] {
        let mut c = TimestampCache::with_precision(p);
        assert_eq!(
            at(&mut c, utc(2026, 12, 31, 23, 59, 59, 999), 123_456),
            before
        );
        assert_eq!(at(&mut c, utc(2027, 1, 1, 0, 0, 0, 0), 0), after);
    }
}

/// A nanosecond count that does not fit inside its millisecond is a caller
/// error. Carrying it would put the stamp in the future, and a stamp in the
/// future fails a skew check at the far end; truncating it does not.
#[test]
fn a_remainder_that_overflows_its_millisecond_is_truncated_not_carried() {
    let mut c = TimestampCache::with_precision(Precision::Nanos);
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 10, 43, 7, 251), 5_000_000),
        "20260828-10:43:07.251999999"
    );
}

/// **The guard that makes ADR-0057's rejection of a padded stamp structural.**
/// `.123000` from a clock that only knows milliseconds is the lie the ADR
/// declined; `coarser` is what stops a caller producing it by accident.
#[test]
fn coarser_never_invents_a_digit() {
    assert_eq!(
        Precision::Micros.coarser(Precision::Millis),
        Precision::Millis
    );
    assert_eq!(
        Precision::Millis.coarser(Precision::Micros),
        Precision::Millis
    );
    assert_eq!(Precision::Nanos.coarser(Precision::Nanos), Precision::Nanos);
    assert_eq!(Precision::Millis.bytes(), 21);
    assert_eq!(Precision::Micros.bytes(), 24);
    assert_eq!(Precision::Nanos.bytes(), 27);
}

/// QuickFIX C++'s `TimestampPrecision` takes any integer 0-9. Six of those
/// widths this engine does not write, and answering with the nearest one it
/// does would put a number on the wire the operator did not choose.
#[test]
fn a_width_this_engine_does_not_write_is_refused_and_not_rounded() {
    for d in [3, 6, 9] {
        assert!(Precision::from_fractional_digits(d).is_some(), "{d}");
    }
    for d in [0, 1, 2, 4, 5, 7, 8, 10] {
        assert!(Precision::from_fractional_digits(d).is_none(), "{d}");
    }
}
