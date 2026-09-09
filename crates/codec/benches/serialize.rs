//! Template encoding cost — D9's whole reason for existing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#[path = "harness.rs"]
mod harness;

use fixbolt_codec::{NoDict, Precision, TemplateBuilder, TimestampCache};
use std::hint::black_box;

fn main() {
    harness::suite(|b| {
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
        let mut stamp = [0u8; 21];
        stamp.copy_from_slice(clock.format(1_787_000_000_000, 0));

        b.bench("encode ExecutionReport (template)", || {
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

        // **The name of the first arm is load-bearing** and is not renamed:
        // `benches/baselines.tsv:141` carries `SendingTime from the cache` at
        // 4.9 ns on the AMD Ryzen 7 3700X, n = 20, `[2026-09-05]`, and ADR-0057
        // decision 5 makes that row the constraint the runtime precision field
        // has to answer to. The two arms beside it are what it costs to write
        // more digits, on the same machine in the same run or not at all.
        let mut ms = 1_787_000_000_000u64;
        b.bench("SendingTime from the cache", || {
            ms += 1;
            black_box(clock.format(black_box(ms), black_box(0)));
        });

        let mut micros = TimestampCache::with_precision(Precision::Micros);
        b.bench("SendingTime from the cache, micros", || {
            ms += 1;
            black_box(micros.format(black_box(ms), black_box(456_789)));
        });

        let mut nanos = TimestampCache::with_precision(Precision::Nanos);
        b.bench("SendingTime from the cache, nanos", || {
            ms += 1;
            black_box(nanos.format(black_box(ms), black_box(456_789)));
        });
    });
}
