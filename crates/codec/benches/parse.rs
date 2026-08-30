//! Parsing cost, on the message shape the whole design is measured against.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#[path = "harness.rs"]
mod harness;

use fixbolt_codec::{FieldIndex, NoDict, Validation, parse_into};
use std::hint::black_box;

fn main() {
    harness::suite(|b| {
        // The NewOrderSingle from reference/measured-costs.md.
        let msg: &[u8] = b"8=FIX.4.4\x019=126\x0135=D\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0111=ID\x0121=1\x0138=002000.00\x0140=1\x01\
54=1\x0155=INTC\x0160=00000000-00:00:00.000\x01167=BOO\x0110=098\x01";

        let mut idx: FieldIndex<64> = FieldIndex::new();
        b.bench(
            "parse NewOrderSingle (validated)",
            CEILING_VALIDATED,
            || {
                let r = parse_into::<NoDict, 64>(black_box(msg), &mut idx, Validation::ALL);
                black_box(r).ok();
            },
        );
        b.bench("parse NewOrderSingle (no checks)", CEILING_RAW, || {
            let r = parse_into::<NoDict, 64>(black_box(msg), &mut idx, Validation::NONE);
            black_box(r).ok();
        });

        let hb: &[u8] = b"8=FIX.4.4\x019=49\x0135=0\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0110=000\x01";
        b.bench("parse Heartbeat (validated)", CEILING_HEARTBEAT, || {
            let r = parse_into::<NoDict, 64>(black_box(hb), &mut idx, Validation::ALL);
            black_box(r).ok();
        });
    });
}

// Regression ceilings, roughly 2x the baseline measured on this machine on
// 2026-08-28. NOT the published targets — see harness.rs and DESIGN.md §6.
// Every figure is best-of-7 x 200,000 iterations, which is an optimistic
// estimator: it reports the least-disturbed run, not the mean.
/// Baseline 72.8 ns on an Apple M5, macOS, unpinned, 2026-08-28.
const CEILING_VALIDATED: f64 = 150.0;
/// Baseline 69.2 ns on an Apple M5, macOS, unpinned, 2026-08-28.
const CEILING_RAW: f64 = 145.0;
/// Baseline 33.2 ns on an Apple M5, macOS, unpinned, 2026-08-28.
const CEILING_HEARTBEAT: f64 = 70.0;
