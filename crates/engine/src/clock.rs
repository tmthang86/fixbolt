//! Where the engine gets the time.
//!
//! A trait, for one reason that matters more than testability: **the
//! acceptance corpus writes a fixed instant into every message it sends.** A
//! `SendingTime` two days out of date fails the 120-second skew check, so an
//! engine wired to the wall clock cannot be driven by the corpus at all. The
//! same seam that makes the 59 definitions runnable over a socket is the one a
//! deployment uses to feed a hardware clock.
//!
//! Milliseconds since **0000-01-01**, matching `nanofix_session::clock` and
//! `DESIGN.md` D13.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds from 0000-01-01 to 1970-01-01.
///
/// Not restated: taken from the session layer, which derives it rather than
/// recalling it.
pub const YEAR_ZERO_TO_EPOCH: u64 = nanofix_session::clock::MILLIS_YEAR_ZERO_TO_EPOCH;

/// What time it is, in the session layer's units.
pub trait Clock {
    /// Milliseconds since 0000-01-01.
    fn now_ms(&mut self) -> u64;
}

/// The wall clock. The default everywhere but a test.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&mut self) -> u64 {
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
        YEAR_ZERO_TO_EPOCH.saturating_add(since_epoch)
    }
}

/// A clock somebody else moves.
///
/// The corpus is the somebody: its harness advances time one `HeartBtInt` at a
/// step, and nothing in a `.def` file happens because a real second passed.
#[derive(Debug, Clone, Copy, Default)]
pub struct ManualClock {
    now_ms: u64,
}

impl ManualClock {
    /// A clock reading `now_ms`.
    #[must_use]
    pub const fn at(now_ms: u64) -> Self {
        Self { now_ms }
    }

    /// Move it. Backwards is allowed and is the caller's problem — the session
    /// layer saturates rather than wrapping.
    pub const fn set(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }
}

impl Clock for ManualClock {
    fn now_ms(&mut self) -> u64 {
        self.now_ms
    }
}
