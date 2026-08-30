//! How long an application may stall before the ring stops taking messages.
//!
//! `STATUS.md` open item 5, step 1. The policy question — *what should happen
//! when the application behind the ring falls behind* — is ADR-0011's, and it
//! cannot be answered without this number. Choosing a policy without knowing how
//! fast the ring fills is choosing blind.
//!
//! **This is not `DESIGN.md` D10.** D10 answered "the consumer on the wire is
//! slow" and shipped as `Backpressure::{Disconnect, Queue, Block}`. This is the
//! other side of the engine: the application thread, behind the ring.
//!
//! Today's answer is a counter. `RingDispatch::refused()` goes up and nothing
//! reads it, which is a silent failure with a struct field — a message the
//! session accepted, numbered and journalled, that the application never sees.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::hint::black_box;
use std::time::Instant;

use nanofix_engine::dispatch::{Dispatch, RingDispatch};
use nanofix_engine::ring;

const M: usize = 512;
/// The capacity `benches/dispatch.rs` measures the hop at, so the two numbers
/// describe the same ring.
const CAPACITY: usize = 1 << 16;

fn main() {
    let msg: &[u8] = b"8=FIX.4.4\x019=126\x0135=D\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x0111=ID\x0121=1\x0138=002000.00\x0140=1\x01\
54=1\x0155=INTC\x0160=00000000-00:00:00.000\x01167=BOO\x0110=098\x01";
    let stamp = b"20260828-12:00:00.000";
    let mut out = [0u8; 1024];

    let (to_app, _from_engine) = ring::pair(CAPACITY);
    let (to_engine, from_app) = ring::pair(CAPACITY);
    // `_from_engine` is held and never drained: that IS the stalled application.
    let mut ringed: RingDispatch<M> = RingDispatch::new(to_app, from_app);
    drop(to_engine);

    // Push until the ring refuses, counting both sides.
    let start = Instant::now();
    let mut accepted = 0usize;
    let mut pushes = 0usize;
    while ringed.refused() == 0 {
        ringed.deliver(0, black_box(msg), 2, stamp, &mut out);
        pushes += 1;
        if ringed.refused() == 0 {
            accepted += 1;
        }
        assert!(
            pushes < 1_000_000,
            "the ring never filled; it must be bounded"
        );
    }
    let fill = start.elapsed();

    // **A refusal count means nothing unless something proves the ring also
    // ACCEPTED.** A ring that refused every message from the first would report
    // a fine-looking number here, and it is the same shape as a benchmark
    // reporting zero allocations for a path that never ran.
    assert!(
        accepted > 0,
        "the ring must have taken messages before refusing"
    );
    assert!(
        ringed.refused() > 0,
        "the ring must have refused; that is the point"
    );

    let per = fill.as_nanos() / accepted.max(1) as u128;
    println!("ring capacity      {CAPACITY} bytes");
    println!("message            {} bytes + header", msg.len());
    println!("messages accepted  {accepted}");
    println!("refused at         message {pushes}");
    println!("time to fill       {:?}  ({per} ns per message)", fill);
    println!();
    println!(
        "So an application that stops reading has roughly {:?} of slack at",
        fill
    );
    println!("this end's full rate before the engine starts dropping messages the");
    println!("session has already accepted, numbered and journalled.");
    println!();
    println!("NOT a latency number: this machine is not DESIGN.md §9. What it");
    println!("bounds is a COUNT and a duration under saturation, which is what");
    println!("ADR-0011 needs and which does not need an isolated core.");
}
