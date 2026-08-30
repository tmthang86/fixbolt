//! `SendingTime` formatting, including the rollovers a per-minute cache can get
//! wrong in a way that only shows up at 00:00 or on New Year's Eve.
#![allow(clippy::unwrap_used, clippy::panic)]

use fixbolt_codec::TimestampCache;

fn at(c: &mut TimestampCache, millis: u64) -> String {
    String::from_utf8(c.format(millis).to_vec()).unwrap()
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
        at(&mut c, utc(2026, 8, 28, 10, 43, 7, 251)),
        "20260828-10:43:07.251"
    );
}

#[test]
fn the_second_within_the_minute_only_moves_the_seconds() {
    let mut c = TimestampCache::new();
    let base = utc(2026, 8, 28, 10, 43, 0, 0);
    assert_eq!(at(&mut c, base), "20260828-10:43:00.000");
    assert_eq!(at(&mut c, base + 59_999), "20260828-10:43:59.999");
}

#[test]
fn the_minute_rolls_over() {
    // The trap: a cache that only rebuilds when the HOUR changes leaves 12:34
    // in place and prints 12:34:00.000 for a message sent at 12:35.
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 12, 34, 59, 999)),
        "20260828-12:34:59.999"
    );
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 12, 35, 0, 0)),
        "20260828-12:35:00.000"
    );
}

#[test]
fn the_day_rolls_over() {
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 23, 59, 59, 999)),
        "20260828-23:59:59.999"
    );
    assert_eq!(
        at(&mut c, utc(2026, 8, 29, 0, 0, 0, 0)),
        "20260829-00:00:00.000"
    );
}

#[test]
fn the_year_rolls_over() {
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2026, 12, 31, 23, 59, 59, 999)),
        "20261231-23:59:59.999"
    );
    assert_eq!(
        at(&mut c, utc(2027, 1, 1, 0, 0, 0, 0)),
        "20270101-00:00:00.000"
    );
}

#[test]
fn a_leap_day_is_a_day() {
    let mut c = TimestampCache::new();
    assert_eq!(
        at(&mut c, utc(2028, 2, 28, 12, 0, 0, 0)),
        "20280228-12:00:00.000"
    );
    assert_eq!(
        at(&mut c, utc(2028, 2, 29, 12, 0, 0, 0)),
        "20280229-12:00:00.000"
    );
    assert_eq!(
        at(&mut c, utc(2028, 3, 1, 12, 0, 0, 0)),
        "20280301-12:00:00.000"
    );
    // 2100 is not a leap year, and a naive %4 rule says it is.
    assert_eq!(
        at(&mut c, utc(2100, 2, 28, 12, 0, 0, 0)),
        "21000228-12:00:00.000"
    );
    assert_eq!(
        at(&mut c, utc(2100, 3, 1, 12, 0, 0, 0)),
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
        at(&mut c, utc(2026, 8, 28, 10, 0, 0, 0)),
        "20260828-10:00:00.000"
    );
    assert_eq!(
        at(&mut c, utc(2026, 8, 28, 9, 0, 0, 0)),
        "20260828-09:00:00.000"
    );
}

#[test]
fn the_epoch_itself() {
    let mut c = TimestampCache::new();
    assert_eq!(at(&mut c, 0), "19700101-00:00:00.000");
}
