//! The only thing that proves non-negotiable 1.
//!
//! `#![no_std]` does not prove it: the crate could pull in `alloc`, and a caller
//! can allocate freely around a call that does not. A counting allocator does
//! prove it, because it counts what actually happened.
//!
//! # The `unsafe` here
//!
//! `GlobalAlloc` is an unsafe trait and there is no safe way to install one, so
//! `unsafe_code` is allowed **in this file only**. `CLAUDE.md` §2 rule 8 asks
//! what proves it sound; three things do:
//!
//! * every method forwards to `System` unchanged — the only addition is a
//!   relaxed counter increment, which cannot affect an allocation;
//! * this is a benchmark binary, not a library crate, so nothing ships it;
//! * it is proven by reversal, not by reading. Adding one `Vec::with_capacity`
//!   to the counted loop reports `allocations: parse 10000` and the assertion
//!   below fails. Observed on 2026-08-28.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use nanofix_codec::{FieldIndex, NoDict, TemplateBuilder, TimestampCache, Validation, parse_into};
use nanofix_dict::Fix44;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System`, which is a correct allocator, with
// the same pointer, layout and size it was given. The only addition is a relaxed
// counter increment. See the module comment for what proves this sound.
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

fn main() {
    let msg: &[u8] = b"8=FIX.4.4\x019=126\x0135=D\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0111=ID\x0121=1\x0138=002000.00\x0140=1\x01\
54=1\x0155=INTC\x0160=00000000-00:00:00.000\x01167=BOO\x0110=098\x01";

    // Everything is built before counting starts. What is measured is the hot
    // path — parse, encode, timestamp — not construction.
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let t = TemplateBuilder::<32, 512>::new(b"FIX.4.4")
        .field(35, b"8")
        .field(49, b"ISLD")
        .field(56, b"TW44")
        .slot(34)
        .slot(52)
        .slot(37)
        .build::<NoDict>()
        .expect("template");
    let mut out = [0u8; 512];
    let mut clock = TimestampCache::new();
    let _ = clock.format(1_787_000_000_000);

    // Warm anything lazy in the runtime before the counted section.
    let _ = parse_into::<NoDict, 64>(msg, &mut idx, Validation::ALL);
    let _ = t.encode(&mut out, &[(34, b"1".as_ref())]);

    let parse_allocs = count(|| {
        for _ in 0..10_000 {
            let _ = parse_into::<NoDict, 64>(msg, &mut idx, Validation::ALL);
        }
    });

    let encode_allocs = count(|| {
        for i in 0..10_000u64 {
            let stamp = *clock.format(1_787_000_000_000 + i);
            let _ = t.encode(
                &mut out,
                &[
                    (34, b"2".as_ref()),
                    (52, &stamp),
                    (37, b"ORD00001".as_ref()),
                ],
            );
        }
    });

    let lookup_allocs = count(|| {
        let view = idx.view(msg);
        for _ in 0..10_000 {
            let _ = view.get(55);
            let _ = view.get(38);
        }
    });

    // A repeating group, walked to all four levels. `GroupIter` is a pair of
    // positions into the index the parser already filled; if it ever built a
    // tree instead, this is where that would show.
    let ae: &[u8] = b"8=FIX.4.4\x019=163\x0135=AE\x0134=2\x0149=TW\x0156=ISLD\x01\
52=20260828-00:00:00.000\x01571=TRID\x01487=0\x01856=0\x01828=0\x01552=1\x0154=1\x01\
37=ORD1\x0178=1\x0179=ACC1\x01756=1\x01757=NP1\x01806=1\x01760=SUB1\x0180=50\x01\
60=20260828-00:00:00.000\x0110=228\x01";
    let mut gidx: FieldIndex<64> = FieldIndex::new();
    parse_into::<Fix44, 64>(ae, &mut gidx, Validation::NONE).expect("AE parses");
    {
        let v = gidx.view(ae);
        let s1 = v
            .group::<Fix44>(b"AE", 552)
            .expect("552")
            .next()
            .expect("side");
        let a1 = s1
            .group::<Fix44>(b"AE", 78)
            .expect("78")
            .next()
            .expect("alloc");
        let p1 = a1
            .group::<Fix44>(b"AE", 756)
            .expect("756")
            .next()
            .expect("party");
        assert!(p1.group::<Fix44>(b"AE", 806).expect("806").next().is_some());
    }
    let group_allocs = count(|| {
        let v = gidx.view(ae);
        for _ in 0..10_000 {
            for side in v.group::<Fix44>(b"AE", 552).expect("552") {
                for a in side.group::<Fix44>(b"AE", 78).expect("78") {
                    let _ = a.get(80);
                    for p in a.group::<Fix44>(b"AE", 756).expect("756") {
                        for sub in p.group::<Fix44>(b"AE", 806).expect("806") {
                            let _ = sub.get(760);
                        }
                    }
                }
            }
        }
    });

    // The five dictionary lookups a message under validation makes, once per
    // field. `enum_allows` is the one that could allocate — its values are
    // variable-length byte strings, and comparing them the obvious wrong way
    // would build a `String` per call.
    let validate_allocs = count(|| {
        for _ in 0..10_000 {
            let _ = Fix44::is_defined_tag(55);
            let _ = Fix44::is_defined_tag(999);
            let _ = Fix44::is_msg_type(b"D");
            let _ = Fix44::is_msg_type(b"*");
            let _ = Fix44::allows(b"D", 55);
            let _ = Fix44::allows(b"0", 55);
            let _ = Fix44::field_type(38).map(|t| t.accepts(b"+200.00"));
            let _ = Fix44::field_type(126).map(|t| t.accepts(b"20040415-12:00:00"));
            let _ = Fix44::enum_allows(167, b"BOO");
            let _ = Fix44::enum_allows(40, b"1");
        }
    });

    println!("allocations: parse   {parse_allocs}");
    println!("allocations: encode  {encode_allocs}");
    println!("allocations: lookup  {lookup_allocs}");
    println!("allocations: group   {group_allocs}");
    println!("allocations: validate {validate_allocs}");
    assert_eq!(parse_allocs, 0, "parse must not allocate");
    assert_eq!(encode_allocs, 0, "encode must not allocate");
    assert_eq!(lookup_allocs, 0, "field lookup must not allocate");
    assert_eq!(
        group_allocs, 0,
        "walking a repeating group must not allocate"
    );
    assert_eq!(
        validate_allocs, 0,
        "a dictionary lookup must not allocate — it happens once per field"
    );
    println!("allocations: 0");
}
