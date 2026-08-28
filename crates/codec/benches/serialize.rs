//! Template encoding cost — D9's whole reason for existing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#[path = "harness.rs"]
mod harness;

use nanofix_codec::{NoDict, TemplateBuilder, TimestampCache};
use std::hint::black_box;

fn main() {
    let t = TemplateBuilder::<32, 512>::new(b"FIX.4.4")
        .field(35, b"8")
        .field(49, b"ISLD")
        .field(56, b"TW44")
        .slot(34)
        .slot(52)
        .slot(37)
        .slot(17)
        .slot(150)
        .slot(39)
        .slot(55)
        .slot(54)
        .slot(38)
        .slot(32)
        .slot(31)
        .slot(151)
        .slot(14)
        .slot(6)
        .build::<NoDict>()
        .expect("template");

    let mut out = [0u8; 512];
    let mut clock = TimestampCache::new();
    let stamp = *clock.format(1_787_000_000_000);

    harness::bench("encode ExecutionReport (template)", CEILING_ENCODE, || {
        let r = t.encode(
            black_box(&mut out),
            &[
                (34, b"2".as_ref()),
                (52, &stamp),
                (37, b"ORD00001".as_ref()),
                (17, b"EXE00001".as_ref()),
                (150, b"F".as_ref()),
                (39, b"2".as_ref()),
                (55, b"INTC".as_ref()),
                (54, b"1".as_ref()),
                (38, b"2000".as_ref()),
                (32, b"2000".as_ref()),
                (31, b"20.15".as_ref()),
                (151, b"0".as_ref()),
                (14, b"2000".as_ref()),
                (6, b"20.15".as_ref()),
            ],
        );
        black_box(r).ok();
    });

    let mut ms = 1_787_000_000_000u64;
    harness::bench("SendingTime from the cache", CEILING_TIMESTAMP, || {
        ms += 1;
        black_box(clock.format(black_box(ms)));
    });
}

/// Baseline 90.3 ns on an Apple M5, macOS, unpinned, 2026-08-28.
const CEILING_ENCODE: f64 = 190.0;
/// Baseline 1.7 ns on an Apple M5, macOS, unpinned, 2026-08-28.
const CEILING_TIMESTAMP: f64 = 5.0;
