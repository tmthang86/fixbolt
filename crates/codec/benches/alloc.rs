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

    println!("allocations: parse   {parse_allocs}");
    println!("allocations: encode  {encode_allocs}");
    println!("allocations: lookup  {lookup_allocs}");
    assert_eq!(parse_allocs, 0, "parse must not allocate");
    assert_eq!(encode_allocs, 0, "encode must not allocate");
    assert_eq!(lookup_allocs, 0, "field lookup must not allocate");
    println!("allocations: 0");
}
