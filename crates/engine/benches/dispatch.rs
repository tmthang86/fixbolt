//! `DESIGN.md` §6: **the ring's hop, measured and published, whatever it is.**
//!
//! D4 says inline is the default and the ring is the option, and ADR-0002 says
//! the ring buys one thing — an application that stalls does not stall the
//! session layer. This is what that costs.
//!
//! The harness is `crates/codec/benches/harness.rs`, included by path rather
//! than copied: one rule, one place. It asserts a **regression ceiling**, not a
//! published target, for the reason written there.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[path = "../../codec/benches/harness.rs"]
mod harness;

use std::hint::black_box;
use std::ops::Range;

use nanofix_engine::dispatch::{Dispatch, InlineDispatch, RingApp, RingDispatch};
use nanofix_engine::ring;
use nanofix_session::Application;

const M: usize = 512;

/// Copies the message back. Deliberately the *cheapest honest* handler there
/// is: anything more and the measurement would be the handler's, not the
/// dispatch's.
struct Bounce;

impl Application for Bounce {
    fn on_message(
        &mut self,
        msg: &[u8],
        _seq: u32,
        _stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        let n = msg.len().min(out.len());
        out[..n].copy_from_slice(&msg[..n]);
        Some(0..n)
    }
}

/// Takes a message and answers nothing, for the one-way measurement.
struct Mute;

impl Application for Mute {
    fn on_message(&mut self, _: &[u8], _: u32, _: &[u8], _: &mut [u8]) -> Option<Range<usize>> {
        None
    }
}

fn main() {
    // The NewOrderSingle from reference/measured-costs.md — the same shape
    // every other benchmark in this repository is measured on.
    let msg: &[u8] = b"8=FIX.4.4\x019=126\x0135=D\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0111=ID\x0121=1\x0138=002000.00\x0140=1\x01\
54=1\x0155=INTC\x0160=00000000-00:00:00.000\x01167=BOO\x0110=098\x01";
    let stamp = b"20260828-12:00:00.000";
    let mut out = [0u8; 1024];

    let mut inline = InlineDispatch::new(Bounce);
    harness::bench("inline deliver + reply", CEILING_INLINE, || {
        let r = inline.deliver(0, black_box(msg), 2, stamp, &mut out);
        black_box(r);
    });

    // A ring big enough that the round trip below never meets a full queue —
    // what a full queue does is step 5's question, not this one's.
    let (to_app, from_engine) = ring::pair(1 << 16);
    let (to_engine, from_app) = ring::pair(1 << 16);
    let mut ringed: RingDispatch<M> = RingDispatch::new(to_app, from_app);
    let mut app: RingApp<M> = RingApp::new(from_engine, to_engine);

    // One way only: the engine pushes, the application takes it and says
    // nothing. This is the byte-at-a-time copy on its own, which is what the
    // `AtomicU8` buffer buys the absence of `unsafe` with.
    harness::bench("ring, one way", CEILING_RING_ONE_WAY, || {
        let r = ringed.deliver(0, black_box(msg), 2, stamp, &mut out);
        black_box(r);
        // Drained by a handler that answers nothing, so nothing comes back and
        // the queue cannot fill.
        let n = app.pump(&mut Mute);
        black_box(n);
    });

    harness::bench("ring, round trip", CEILING_RING_TRIP, || {
        ringed.deliver(0, black_box(msg), 2, stamp, &mut out);
        let n = app.pump(&mut Bounce);
        black_box(n);
        let mut back = 0;
        ringed.collect(|_, b| back += b.len());
        black_box(back);
    });

    assert_eq!(ringed.refused(), 0, "no measurement here met a full ring");
    assert_eq!(ringed.dropped(), 0, "and none met a reply that did not fit");
    assert_eq!(app.dropped(), 0);
}

// Regression ceilings, roughly 2x the baseline measured on this machine.
// NOT published targets — see harness.rs and DESIGN.md §6. Every figure is
// best-of-7 x 200,000 iterations on an Apple M5, macOS 25.6, unpinned,
// 2026-08-30.
/// Baseline 2.5–4.9 ns across runs. The spread is the measurement's, not the
/// code's: at this size the loop is a handful of instructions and the harness's
/// own overhead is the same order.
const CEILING_INLINE: f64 = 15.0;
/// Baseline 128.9 ns for a 163-byte message: ~0.8 ns per byte, which is the
/// `AtomicU8` copy and nothing else.
const CEILING_RING_ONE_WAY: f64 = 260.0;
/// Baseline 247.6 ns — two copies and a handler.
const CEILING_RING_TRIP: f64 = 500.0;
