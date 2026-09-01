//! Parsing cost, on the message shape the whole design is measured against.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#[path = "harness.rs"]
mod harness;

use fixbolt_codec::{FieldIndex, NoDict, Validation, parse_into};
use std::hint::black_box;

// `[measured 2026-09-01]` the `black_box(&idx)` calls below were added after
// `crates/engine/benches/dispatch.rs` was found to be measuring 1.3 ns for work
// that takes 8.5, because its output buffer was written and never read. These
// three cases were audited the same way and **did not move** — 124.5 -> 123.1,
// 117.1 -> 113.7, 58.3 -> 56.6, all inside their own run-to-run spread — so
// nothing here was being deleted.
//
// The lines stay anyway. A benchmark that is safe because today's optimiser
// happens not to see through it is safe by luck; one whose output demonstrably
// escapes is safe by construction. `CLAUDE.md` §4: prose does not hold a
// constraint.
fn main() {
    harness::suite(|b| {
        // The NewOrderSingle from reference/measured-costs.md.
        let msg: &[u8] = b"8=FIX.4.4\x019=126\x0135=D\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0111=ID\x0121=1\x0138=002000.00\x0140=1\x01\
54=1\x0155=INTC\x0160=00000000-00:00:00.000\x01167=BOO\x0110=098\x01";

        let mut idx: FieldIndex<64> = FieldIndex::new();
        b.bench("parse NewOrderSingle (validated)", || {
            let r = parse_into::<NoDict, 64>(black_box(msg), &mut idx, Validation::ALL);
            black_box(r).ok();
            black_box(&idx); // audited 2026-09-01; see the note in main()
        });
        b.bench("parse NewOrderSingle (no checks)", || {
            let r = parse_into::<NoDict, 64>(black_box(msg), &mut idx, Validation::NONE);
            black_box(r).ok();
            black_box(&idx); // audited 2026-09-01; see the note in main()
        });

        let hb: &[u8] = b"8=FIX.4.4\x019=49\x0135=0\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0110=000\x01";
        b.bench("parse Heartbeat (validated)", || {
            let r = parse_into::<NoDict, 64>(black_box(hb), &mut idx, Validation::ALL);
            black_box(r).ok();
            black_box(&idx); // audited 2026-09-01; see the note in main()
        });
    });
}
