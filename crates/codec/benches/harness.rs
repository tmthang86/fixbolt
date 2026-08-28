//! A timing harness small enough to have no dependencies.
//!
//! Criterion is what `DESIGN.md` §6 names, and it gives outlier detection and
//! confidence intervals this does not. It is deferred for one reason: **these
//! benchmarks must assert**, and Criterion measures without asserting. A target
//! that lives in a comment is a wish — the reference project missed its own
//! commented target by 7x and nothing noticed.
//!
//! What is asserted is a **regression ceiling**, not the published target. The
//! measured baseline is 139 ns for a `NewOrderSingle` and the published gate is
//! 150 ns, an 8% margin; an unpinned laptop varies by more than 8%, so a hard
//! assert would go red at random and a gate that goes red at random gets
//! switched off. `DESIGN.md` §6 states both numbers and why they differ.

use std::hint::black_box;
use std::time::Instant;

/// Time `f` and assert it stays under `ceiling_ns` per operation.
pub fn bench<F: FnMut()>(name: &str, ceiling_ns: f64, mut f: F) {
    for _ in 0..10_000 {
        f();
    }
    let mut best = f64::INFINITY;
    for _ in 0..7 {
        let iters = 200_000u32;
        let t = Instant::now();
        for _ in 0..iters {
            f();
        }
        let ns = t.elapsed().as_nanos() as f64 / f64::from(iters);
        best = best.min(ns);
    }
    println!("{name:<34} {best:>8.1} ns/op   ceiling {ceiling_ns:.0}");
    assert!(
        best <= ceiling_ns,
        "{name}: {best:.1} ns/op exceeds the {ceiling_ns:.0} ns regression ceiling"
    );
    black_box(best);
}
