//! The only thing that proves non-negotiable 1 for `fixbolt-sbe`: decode,
//! one field read, a nested-group-plus-`varData` walk, and encode through
//! `codec::Encoding`, all zero allocations.
//!
//! `#![no_std]` (`crates/sbe/src/lib.rs`) does not prove it — the crate could
//! pull in `alloc`, and a caller can allocate freely around a call that does
//! not — a counting allocator does, because it counts what actually
//! happened. `crates/codec/benches/alloc.rs` is the pattern this follows
//! exactly, table for table.
//!
//! # The `unsafe` here
//!
//! As `crates/codec/benches/alloc.rs`: `GlobalAlloc` is an unsafe trait with
//! no safe way to install one, so `unsafe_code` is allowed **in this file
//! only**. What proves it sound: every method forwards to `System`
//! unchanged, this is a benchmark binary that nothing ships, and it is proven
//! by reversal, not by reading — see the reversal in the plan step's report.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(unsafe_code)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it.
#![allow(clippy::indexing_slicing)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "support/mod.rs"]
mod support;
use support::{BenchSchema, INNER, INNER_FIELDS, OUTER, OUTER_DATA, OUTER_FIELDS, ROOT_FIELDS};

use fixbolt_sbe::{
    Encoding, FieldId, MessageWriter, Parsed, Sbe, SbeTemplate, SbeView, Validation, Value,
};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System`, which is a correct allocator,
// with the same pointer, layout and size it was given. The only addition is
// a relaxed counter increment. See the module comment for what proves this
// sound.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn count<F: FnOnce()>(f: F) -> usize {
    let before = ALLOCS.load(Ordering::Relaxed);
    f();
    ALLOCS.load(Ordering::Relaxed) - before
}

type E = Sbe<BenchSchema>;

fn main() {
    // Everything is built before counting starts. What is measured is the
    // hot path — decode, one field, a group walk, encode — not construction.

    // --- the NewOrderSingle-shaped flat message, built once through the
    // template so `encode` has something to repeat and `field` has something
    // to read. Values are this bench's own choice, not RC4's published dump —
    // this proves the path runs allocation-free, not that the bytes match the
    // spec (`crates/sbe/src/tests.rs` already proves that).
    let t: SbeTemplate<BenchSchema, 96> = SbeTemplate::new(99).expect("NewOrderSingle template");
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
    let nos_msg_len = range.len();

    // Liveness: the warm encode must have actually written the fields, proven
    // by parsing the result back through the same `Encoding` and reading one
    // by id — the field this bench's `field` case reads.
    let mut scratch = <E as Encoding>::Scratch::default();
    let warm_parse = E::parse(&nos_out[..nos_msg_len], &mut scratch, Validation::ALL);
    assert_eq!(
        warm_parse,
        Ok(Parsed::Complete {
            consumed: nos_msg_len
        }),
        "the encoded NewOrderSingle must parse back whole"
    );
    let view = E::view(&scratch, &nos_out[..nos_msg_len]);
    assert_eq!(
        E::field(view, FieldId(55)),
        Some(b"GEM4\0\0\0\0".as_ref()),
        "the field path must actually read what encode wrote"
    );

    // --- the nested message: one root field, an outer group nesting an
    // inner group, a `varData` per outer entry — built with `MessageWriter`
    // rather than hand-typed hex, so the fixture is provably a message this
    // crate's own writer produces.
    let mut nested_buf = [0u8; 128];
    let nested_len = {
        let mut w = MessageWriter::new::<BenchSchema>(&mut nested_buf, 1).expect("nested writer");
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

    // Liveness: decode once outside the counted section and check the root
    // field and both group levels really are what the writer above wrote.
    let v = SbeView::decode::<BenchSchema>(nested_msg).expect("decode nested warm");
    assert_eq!(
        v.root::<BenchSchema>()
            .expect("root")
            .value(&ROOT_FIELDS[0])
            .expect("root value"),
        Some(Value::UInt(9)),
        "the parse path must actually read the root field"
    );
    {
        let mut outer = v
            .tail::<BenchSchema>()
            .expect("tail")
            .group(&OUTER[0])
            .expect("outer group");
        let mut seen_outer = [0u64; 2];
        let mut seen_inner = [0u64; 4];
        let (mut i, mut k) = (0, 0);
        while let Some(e) = outer.next_entry().expect("outer entry") {
            if let Some(Value::UInt(o)) = e.block().value(&OUTER_FIELDS[0]).expect("outer value") {
                seen_outer[i] = o;
            }
            let mut inner = e.tail().group(&INNER[0]).expect("inner group");
            while let Some(ie) = inner.next_entry().expect("inner entry") {
                if let Some(Value::UInt(x)) =
                    ie.block().value(&INNER_FIELDS[0]).expect("inner value")
                {
                    seen_inner[k] = x;
                }
                k += 1;
            }
            let (note, _) = inner
                .finish()
                .expect("inner finish")
                .var_data(&OUTER_DATA[0])
                .expect("note");
            assert_eq!(
                note,
                Some(&b"note"[..]),
                "the group walk must reach its varData"
            );
            i += 1;
        }
        assert_eq!(
            seen_outer,
            [10, 20],
            "the walk must actually visit every outer entry"
        );
        assert_eq!(
            seen_inner,
            [1, 2, 1, 2],
            "the walk must actually visit every inner entry of both outer entries"
        );
    }

    // --- the four counted cases ------------------------------------------

    let parse_allocs = count(|| {
        for _ in 0..10_000 {
            let v =
                SbeView::decode::<BenchSchema>(std::hint::black_box(nested_msg)).expect("decode");
            std::hint::black_box(v.root::<BenchSchema>().ok());
        }
    });

    let field_allocs = count(|| {
        for _ in 0..10_000 {
            let f = E::field(std::hint::black_box(view), FieldId(55));
            std::hint::black_box(f);
        }
    });

    let group_allocs = count(|| {
        for _ in 0..10_000 {
            let mut outer = v
                .tail::<BenchSchema>()
                .expect("tail")
                .group(&OUTER[0])
                .expect("outer group");
            while let Some(e) = outer.next_entry().expect("outer entry") {
                let _ = e.block().value(&OUTER_FIELDS[0]);
                let mut inner = e.tail().group(&INNER[0]).expect("inner group");
                while let Some(ie) = inner.next_entry().expect("inner entry") {
                    let _ = ie.block().value(&INNER_FIELDS[0]);
                }
                let _ = inner
                    .finish()
                    .expect("inner finish")
                    .var_data(&OUTER_DATA[0])
                    .expect("note");
            }
            let _ = outer.finish().expect("outer finish");
        }
    });

    let encode_allocs = count(|| {
        for _ in 0..10_000 {
            let r = E::encode::<0, 96>(
                std::hint::black_box(&t),
                &mut nos_out,
                std::hint::black_box(&slots),
            );
            std::hint::black_box(r).ok();
        }
    });

    println!("allocations: parse nested (header+root) {parse_allocs}");
    println!("allocations: field                       {field_allocs}");
    println!("allocations: walk nested group + varData {group_allocs}");
    println!("allocations: encode NewOrderSingle        {encode_allocs}");
    assert_eq!(parse_allocs, 0, "decoding header+root must not allocate");
    assert_eq!(
        field_allocs, 0,
        "reading a root field through Encoding::field must not allocate"
    );
    assert_eq!(
        group_allocs, 0,
        "walking a nested group and its varData must not allocate"
    );
    assert_eq!(
        encode_allocs, 0,
        "encoding through Encoding::encode with an SbeTemplate must not allocate"
    );
    println!("allocations: 0");
}
