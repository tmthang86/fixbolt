//! Timing for `fixbolt-sbe`'s decode, field, group-walk and encode paths —
//! the same four cases `benches/alloc.rs` proves allocation-free, timed.
//!
//! The harness is `crates/codec/benches/harness.rs`, included by path rather
//! than copied (one rule, one place), exactly as `crates/engine/benches/dispatch.rs`
//! and `crates/session/benches/heartbeat.rs` already do from outside `codec`.
//!
//! **No baseline is recorded here.** `benches/baselines.tsv`'s header says
//! timing baselines come only from the `DESIGN.md` §9 Linux desk, and this
//! step ran on a macOS laptop — every case below is expected to print
//! `NO BASELINE` until someone records one there (CLAUDE.md §2 non-negotiable
//! 10).
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

#[path = "../../codec/benches/harness.rs"]
mod harness;

#[path = "support/mod.rs"]
mod support;
use support::{BenchSchema, INNER, INNER_FIELDS, OUTER, OUTER_DATA, OUTER_FIELDS, ROOT_FIELDS};

use fixbolt_sbe::{Encoding, FieldId, MessageWriter, Sbe, SbeTemplate, SbeView, Validation, Value};
use std::hint::black_box;

type E = Sbe<BenchSchema>;

fn main() {
    harness::suite(|b| {
        let t: SbeTemplate<BenchSchema, 96> =
            SbeTemplate::new(99).expect("NewOrderSingle template");
        let transact_time = 1_700_000_000_000_000_000u64.to_le_bytes();
        let qty = 7i32.to_le_bytes();
        let price = 99_610i64.to_le_bytes();
        let slots: [(FieldId, &[u8]); 8] = [
            (FieldId(11), b"ORD00001"),
            (FieldId(1), b"ACCT01\0\0"),
            (FieldId(55), b"GEM4\0\0\0\0"),
            (FieldId(54), b"1"),
            (FieldId(60), &transact_time),
            (FieldId(38), &qty),
            (FieldId(40), b"2"),
            (FieldId(44), &price),
        ];
        let mut nos_out = [0u8; 96];
        let range = E::encode::<0, 96>(&t, &mut nos_out, &slots).expect("warm encode");
        let mut scratch = <E as Encoding>::Scratch::default();
        let _ =
            E::parse(&nos_out[..range.len()], &mut scratch, Validation::ALL).expect("warm parse");
        let view = E::view(&scratch, &nos_out[..range.len()]);
        assert_eq!(
            E::field(view, FieldId(55)),
            Some(b"GEM4\0\0\0\0".as_ref()),
            "the field path must actually read what encode wrote"
        );

        let mut nested_buf = [0u8; 128];
        let nested_len = {
            let mut w =
                MessageWriter::new::<BenchSchema>(&mut nested_buf, 1).expect("nested writer");
            w.put(&ROOT_FIELDS[0], Value::UInt(9)).expect("root field");
            w.group(&OUTER[0], |gw| {
                for outer_val in [10u64, 20u64] {
                    gw.entry(|ew| {
                        ew.put(&OUTER_FIELDS[0], Value::UInt(outer_val))?;
                        ew.group(&INNER[0], |igw| {
                            for inner_val in [1u64, 2u64] {
                                igw.entry(|iew| iew.put(&INNER_FIELDS[0], Value::UInt(inner_val)))?;
                            }
                            Ok(())
                        })?;
                        ew.var_data(&OUTER_DATA[0], b"note")?;
                        Ok(())
                    })?;
                }
                Ok(())
            })
            .expect("build nested message");
            w.finish().expect("finish nested message")
        };
        let nested_msg = &nested_buf[..nested_len];
        let v = SbeView::decode::<BenchSchema>(nested_msg).expect("decode nested warm");
        assert_eq!(
            v.root::<BenchSchema>()
                .expect("root")
                .value(&ROOT_FIELDS[0])
                .expect("root value"),
            Some(Value::UInt(9)),
            "the parse path must actually read the root field"
        );

        b.bench("parse nested (header+root)", || {
            let dv = SbeView::decode::<BenchSchema>(black_box(nested_msg)).expect("decode");
            black_box(dv.root::<BenchSchema>().ok());
        });

        b.bench("field", || {
            let f = E::field(black_box(view), FieldId(55));
            black_box(f);
        });

        b.bench("walk nested group + varData", || {
            let mut outer = v
                .tail::<BenchSchema>()
                .expect("tail")
                .group(&OUTER[0])
                .expect("outer group");
            while let Some(e) = outer.next_entry().expect("outer entry") {
                black_box(e.block().value(&OUTER_FIELDS[0]).expect("outer value"));
                let mut inner = e.tail().group(&INNER[0]).expect("inner group");
                while let Some(ie) = inner.next_entry().expect("inner entry") {
                    black_box(ie.block().value(&INNER_FIELDS[0]).expect("inner value"));
                }
                let note = inner
                    .finish()
                    .expect("inner finish")
                    .var_data(&OUTER_DATA[0])
                    .expect("note");
                black_box(note);
            }
            black_box(outer.finish().expect("outer finish").position());
        });

        b.bench("encode NewOrderSingle", || {
            let r = E::encode::<0, 96>(black_box(&t), &mut nos_out, black_box(&slots));
            black_box(r).ok();
        });
    });
}
