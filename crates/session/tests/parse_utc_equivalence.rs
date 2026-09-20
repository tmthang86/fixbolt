//! `parse_utc` answers exactly what the one-rule reader on `main` answers.
//!
//! **This file is an A/B arm's safety net, not a specification.** The arm on
//! `ab/parse-utc-fast-path` adds a straight-line branch for the two widths the
//! wire actually carries — 17 and 21 bytes — and leaves ADR-0058's rule in
//! charge of every other width. A fast path that is fast and *wrong* teaches
//! nothing about the 33 ns of `STATUS.md` item 93's bisect segment (3), so the
//! question this file asks is not "is the reader correct" — `timestamp_widths.rs`
//! and `scripts/interop.sh` §4i ask that — but **"does it still answer what it
//! answered before, byte for byte, value for value"**.
//!
//! The oracle is [`reference`]: `crates/session/src/clock.rs`'s `parse_utc` as
//! it stands on `main`, transcribed here with its helpers. It was run against
//! the unmodified reader first and agreed on every input below, which is what
//! makes the transcription trustworthy as an oracle rather than a second guess.
//!
//! And because `52=` has **two** readers
//! (`docs/reference/one-field-two-readers.md`), every input the reader accepts
//! is also put to `fixbolt_dict::FieldType::UtcTimestamp`. That is the exact
//! direction of the defect that page exists for: a value this engine reads and
//! the dictionary refuses is a `35=3 ... 371=52 373=6` per message after the
//! Logon.

use fixbolt_dict::FieldType;
use fixbolt_session::clock::parse_utc;

// ---------------------------------------------------------------------------
// The oracle: `main`'s reader, transcribed.
// ---------------------------------------------------------------------------

const LEN_SECONDS: usize = 17;
const LEN_MAX: usize = 30;
const DAYS_YEAR_ZERO_TO_EPOCH: i64 = 719_528;

/// `crates/session/src/clock.rs::parse_utc` as of `main`, unchanged.
fn reference(s: &[u8]) -> Option<u64> {
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
        if !frac.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let digit = |i: usize| u32::from(frac.get(i).copied().unwrap_or(b'0') - b'0');
        digit(0) * 100 + digit(1) * 10 + digit(2)
    };

    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let days = days_from_civil(i64::from(year), month, day) + DAYS_YEAR_ZERO_TO_EPOCH;
    let days = u64::try_from(days).ok()?;
    Some(
        days * 86_400_000
            + u64::from(hour) * 3_600_000
            + u64::from(minute) * 60_000
            + u64::from(second) * 1_000
            + u64::from(milli),
    )
}

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

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * u64::from(mp) + 2) / 5 + u64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

// ---------------------------------------------------------------------------
// The corpus.
// ---------------------------------------------------------------------------

/// A well-formed stamp with `digits` fractional digits taken from `pool`.
fn stamp(head: &str, digits: usize, pool: &[u8]) -> Vec<u8> {
    let mut v = head.as_bytes().to_vec();
    if digits > 0 {
        v.push(b'.');
        for i in 0..digits {
            v.push(pool.get(i % pool.len()).copied().unwrap_or(b'0'));
        }
    }
    v
}

/// Widths: every fraction length from 0 to 16, in three digit patterns, plus
/// the bare `.` that ADR-0058 decision 2 refuses.
fn corpus_widths(out: &mut Vec<Vec<u8>>) {
    for pool in [
        &b"123456789012345678"[..],
        b"000000000000000000",
        b"999999999999999999",
    ] {
        for digits in 0..=16 {
            out.push(stamp("20260909-10:00:00", digits, pool));
        }
    }
    out.push(b"20260909-10:00:00.".to_vec());
    out.push(b"20260909-10:00:00..".to_vec());
    out.push(b"20260909-10:00:00 123".to_vec());
}

/// Every total length from 0 to 40: the 30-byte stamp cut short, and the same
/// stamp run on with more digits.
fn corpus_lengths(out: &mut Vec<Vec<u8>>) {
    let wide = b"20260909-10:00:00.123456789012";
    for n in 0..=40usize {
        let mut v: Vec<u8> = wide.iter().take(n).copied().collect();
        while v.len() < n {
            v.push(b'7');
        }
        out.push(v);
    }
}

/// Every byte of five well-formed stamps, replaced by every byte that has ever
/// been a separator, a digit, or neither.
fn corpus_mutations(out: &mut Vec<Vec<u8>>) {
    let bases: [&[u8]; 5] = [
        b"20260909-10:00:00",
        b"20260909-10:00:00.1",
        b"20260909-10:00:00.123",
        b"20260909-10:00:00.123456",
        b"20260909-10:00:00.123456789012",
    ];
    for base in bases {
        for at in 0..base.len() {
            for byte in *b".:-+ aZ09/\x00\xff" {
                let mut v = base.to_vec();
                if let Some(slot) = v.get_mut(at) {
                    *slot = byte;
                }
                out.push(v);
            }
        }
    }
}

/// The calendar and the clock, one component at a time off a valid base —
/// month 00 to 13, day 00 to 32, hour to 25, minute and second to 61, and the
/// four leap-year rules at 29 February.
fn corpus_calendar(out: &mut Vec<Vec<u8>>) {
    let write = |y: u32, mo: u32, d: u32, h: u32, mi: u32, s: u32, frac: &str| -> Vec<u8> {
        format!("{y:04}{mo:02}{d:02}-{h:02}:{mi:02}:{s:02}{frac}").into_bytes()
    };
    for frac in ["", ".1", ".123", ".123456", ".123456789012"] {
        for mo in 0..=13 {
            out.push(write(2026, mo, 15, 10, 0, 0, frac));
        }
        for d in 0..=32 {
            for mo in [1, 2, 4, 12] {
                out.push(write(2026, mo, d, 10, 0, 0, frac));
            }
        }
        for h in 0..=25 {
            out.push(write(2026, 9, 9, h, 0, 0, frac));
        }
        for mi in 0..=61 {
            out.push(write(2026, 9, 9, 10, mi, 0, frac));
        }
        for s in 0..=61 {
            out.push(write(2026, 9, 9, 10, 0, s, frac));
        }
        for y in [
            0, 1, 1899, 1900, 1960, 1969, 1970, 2000, 2024, 2026, 2100, 9999,
        ] {
            for (mo, d) in [(1, 1), (2, 28), (2, 29), (2, 30), (12, 31)] {
                out.push(write(y, mo, d, 23, 59, 60, frac));
            }
        }
    }
}

/// Deterministic noise: 40 000 strings of length 0 to 35 over an alphabet of
/// the bytes a real stamp is made of plus the ones that spoil it.
fn corpus_noise(out: &mut Vec<Vec<u8>>) {
    let alphabet = b"0123456789.-: aZ+/";
    let mut state: u64 = 0x2026_0909_1000_0000;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..40_000u32 {
        let len = (next() % 36) as usize;
        let mut v = Vec::with_capacity(len);
        for _ in 0..len {
            let i = (next() % alphabet.len() as u64) as usize;
            v.push(alphabet.get(i).copied().unwrap_or(b'0'));
        }
        out.push(v);
    }
}

fn corpus() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    corpus_widths(&mut out);
    corpus_lengths(&mut out);
    corpus_mutations(&mut out);
    corpus_calendar(&mut out);
    corpus_noise(&mut out);
    out
}

// ---------------------------------------------------------------------------
// The two assertions.
// ---------------------------------------------------------------------------

#[test]
fn the_reader_answers_what_the_one_rule_reader_answered() {
    let inputs = corpus();
    assert!(
        inputs.len() > 40_000,
        "the corpus collapsed: {}",
        inputs.len()
    );
    let mut accepted = 0u32;
    for v in &inputs {
        let got = parse_utc(v);
        let want = reference(v);
        assert_eq!(
            got,
            want,
            "parse_utc disagrees with the `main` rule on {:?} ({} bytes)",
            String::from_utf8_lossy(v),
            v.len()
        );
        if want.is_some() {
            accepted += 1;
        }
    }
    // A corpus that rejects everything would pass the loop above and prove
    // nothing. `[measured 2026-09-20]` it accepts roughly a thousand.
    assert!(
        accepted > 500,
        "only {accepted} inputs were timestamps at all"
    );
}

#[test]
fn nothing_the_reader_takes_is_refused_by_the_dictionary() {
    // `docs/reference/one-field-two-readers.md`: `52=` has two readers, and the
    // failure that cost a day was a value `parse_utc` read and `dict` refused —
    // a `Reject` per message after the Logon, invisible to every gate here.
    // This is that direction, over the whole corpus.
    for v in &corpus() {
        if parse_utc(v).is_some() {
            assert!(
                FieldType::UtcTimestamp.accepts(v),
                "the reader took {:?} and the dictionary refused it",
                String::from_utf8_lossy(v)
            );
        }
    }
}

#[test]
fn both_readers_still_draw_the_width_line_in_the_same_place() {
    // `timestamp_widths.rs::both_readers_agree_on_every_width` asks this from 0
    // to 34 on one digit pattern; the fast path is a *width* dispatch, so it is
    // asked again here on a well-formed stamp at every length to 40.
    for n in 0..=40usize {
        let mut v: Vec<u8> = b"20260909-10:00:00.123456789012"
            .iter()
            .take(n)
            .copied()
            .collect();
        while v.len() < n {
            v.push(b'7');
        }
        assert_eq!(
            parse_utc(&v).is_some(),
            FieldType::UtcTimestamp.accepts(&v),
            "the two readers disagree at {n} bytes: {:?}",
            String::from_utf8_lossy(&v)
        );
    }
}
