//! What the loop does when there is nothing to do.
//!
//! `DESIGN.md` D8: the engine thread busy-polls and never sleeps in the kernel,
//! because an `epoll` wakeup costs 2–5 µs and brings scheduler jitter with it —
//! the single largest cost the engine controls on a path that is otherwise
//! under a microsecond. It burns a core. That is the price, and it is the
//! standard price.
//!
//! It is a trait for one reason: **every test in this repository uses
//! [`Park`]**. A CI machine running several spinning loops is a CI machine that
//! times out, and a strategy that is only reachable through a `#[cfg(test)]`
//! is a strategy nobody can choose in production either.

/// What to do on an idle turn of the loop.
pub trait Waiting {
    /// Whether this strategy leaves user space. Reading it is how a caller —
    /// or a test — can tell the two apart without timing anything.
    const SLEEPS: bool;

    /// One idle turn.
    fn idle(&mut self);
}

/// Spin. The default, and the reason `DESIGN.md` D8 exists.
#[derive(Debug, Clone, Copy, Default)]
pub struct Spin;

impl Waiting for Spin {
    const SLEEPS: bool = false;

    fn idle(&mut self) {
        // Tells the CPU this is a spin-wait: on x86 it issues `pause`, which
        // gives the sibling hyperthread its pipeline back and cuts the
        // memory-order violation on exit. It does **not** enter the kernel.
        core::hint::spin_loop();
    }
}

/// Yield to the scheduler. For tests, and for deployments that would rather
/// have the core back than have the microsecond.
#[derive(Debug, Clone, Copy, Default)]
pub struct Park;

impl Waiting for Park {
    const SLEEPS: bool = true;

    fn idle(&mut self) {
        std::thread::yield_now();
    }
}
