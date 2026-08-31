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
        b.bench("parse NewOrderSingle (validated)", || {
            let r = parse_into::<NoDict, 64>(black_box(msg), &mut idx, Validation::ALL);
            black_box(r).ok();
        });
        b.bench("parse NewOrderSingle (no checks)", || {
            let r = parse_into::<NoDict, 64>(black_box(msg), &mut idx, Validation::NONE);
            black_box(r).ok();
        });

        let hb: &[u8] = b"8=FIX.4.4\x019=49\x0135=0\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0110=000\x01";
        b.bench("parse Heartbeat (validated)", || {
            let r = parse_into::<NoDict, 64>(black_box(hb), &mut idx, Validation::ALL);
            black_box(r).ok();
        });
    });
}
