//! What the loop does when there is nothing to do — **and there are two right
//! answers, one per mode.**
//!
//! `DESIGN.md` D8, as amended by
//! [ADR-0013](../../../docs/decisions/ADR-0013-two-modes-standard-and-hft.md):
//!
//! | | idle behaviour | the core |
//! |---|---|---|
//! | **`standard`** — the default | blocks on readiness, with a timeout | given back |
//! | **`hft`** — opt-in | spins on non-blocking sockets | burned, permanently |
//!
//! In `hft` an `epoll`-class wakeup costs 2–5 µs and brings scheduler jitter
//! with it, which on a path otherwise under a microsecond is the largest single
//! cost the engine controls. In `standard` an engine that pins a core at 100%
//! out of the box is one most people cannot even evaluate. **Neither half is
//! the weaker half** — `CLAUDE.md` §2 non-negotiable 4 is now mode-scoped and
//! says so, and each half has its own machine check.
//!
//! # Why `idle` is handed the sources
//!
//! Blocking until a socket is readable requires knowing the sockets, and
//! nothing about a spin does. Rather than split idling across two traits, the
//! waiter is **shown** the sources and ignores them if it does not need them:
//! [`Spin`] takes the slice and drops it, and the whole call disappears at
//! `-O2`. ADR-0014 decision 3.
//!
//! [`Waiting::NEEDS_SOURCES`] is how a caller tells the two apart without
//! timing anything, and it is what [`crate::Engine::run`] tests to refuse a
//! `standard` engine over a transport that cannot be waited on.

use crate::transport::Interest;

/// What to do on an idle turn of the loop.
pub trait Waiting {
    /// Whether this strategy leaves user space. Reading it is how a caller —
    /// or a test — can tell the two apart without timing anything.
    const SLEEPS: bool;

    /// Whether [`Self::idle`] needs the source list to be **correct and
    /// complete**, not merely present.
    ///
    /// `false` for a strategy that is going to return on its own anyway. `true`
    /// means a missing source is a message that arrives one timeout late, which
    /// is the failure mode `standard` has to be built against: every way of
    /// getting the list wrong still yields a working engine.
    const NEEDS_SOURCES: bool;

    /// One idle turn.
    ///
    /// `interests` is every source the caller currently cares about, rebuilt
    /// each turn — a [`crate::transport::Source`] does not own its descriptor
    /// and must not outlive the connection it came from.
    fn idle(&mut self, interests: &[Interest]);
}

/// Spin. `hft`, and the reason `DESIGN.md` D8 exists.
#[derive(Debug, Clone, Copy, Default)]
pub struct Spin;

impl Waiting for Spin {
    const SLEEPS: bool = false;
    const NEEDS_SOURCES: bool = false;

    fn idle(&mut self, _interests: &[Interest]) {
        // Tells the CPU this is a spin-wait: on x86 it issues `pause`, which
        // gives the sibling hyperthread its pipeline back and cuts the
        // memory-order violation on exit. It does **not** enter the kernel.
        core::hint::spin_loop();
    }
}

/// Yield to the scheduler. **Neither mode**, and that is its definition rather
/// than a defect in it.
///
/// It is `std::thread::yield_now()`. It hands the CPU to whoever else wants it
/// and comes straight back, so it neither spins honestly nor blocks honestly:
///
/// - **It fails the `hft` gate.** `sched_yield` is on the list
///   `scripts/check-no-kernel-sleep.sh` refuses.
/// - **It fails the `standard` gate.** A loop calling it consumes a whole core,
///   which is exactly what `scripts/check-standard-gives-the-core-back.sh`
///   measures and rejects.
///
/// What it is for is **tests**. Every test in this repository drives
/// [`crate::Engine::turn`] by hand and never needs to block; a test suite that
/// spun would pin a core for nothing, and one that blocked would need real
/// sockets to wake it. This is the useful thing in between. A deployment picks
/// [`Spin`] or `standard`'s blocking strategy, never this.
///
/// `[renamed 2026-08-30]` It was `Park`, sitting beside [`Spin`] as though the
/// two were peers. ADR-0014 decision 7.
#[derive(Debug, Clone, Copy, Default)]
pub struct Yield;

impl Waiting for Yield {
    const SLEEPS: bool = true;
    const NEEDS_SOURCES: bool = false;

    fn idle(&mut self, _interests: &[Interest]) {
        std::thread::yield_now();
    }
}
