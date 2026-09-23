//! Reading a price and writing one back — ADR-0120, plan
//! `docs/plans/2026-09-23-p3-decimal.md` step 3.
//!
//! A target of its own rather than two more cases in `parse.rs` and
//! `serialize.rs`: new code in an existing binary moves the code already there,
//! and ADR-0049 holds a case's layout only to about 4%, so adding here could
//! shift the committed baselines of cases that did not change.
//!
//! No figure from this file is a claim until a baseline row exists for it,
//! recorded on the §9 machine (plan step 7). Until then both cases print
//! `NO BASELINE`, which is not a pass.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#[path = "harness.rs"]
mod harness;

use fixbolt_codec::{Decimal, as_decimal};
use std::hint::black_box;

fn main() {
    harness::suite(|b| {
        let price: &[u8] = b"12345.6789";
        let value = Decimal::new(123_456_789, -4);

        // **Assert the input, not the output** (ADR-0089): a benchmark that
        // timed a parse returning `Err` early would publish a figure for work
        // it never did. Both cases run on this one value, asserted both ways.
        assert_eq!(as_decimal(price), Ok(value), "the parse input must parse");
        let mut out = [0u8; Decimal::MAX_LEN];
        assert_eq!(
            value.format(&mut out),
            price,
            "the format input must write the same bytes back"
        );

        b.bench("decimal parse 12345.6789", || {
            black_box(as_decimal(black_box(price))).ok();
        });
        b.bench("decimal format 12345.6789", || {
            let written = black_box(value).format(&mut out);
            black_box(written);
        });
    });
}
