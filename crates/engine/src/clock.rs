//! Where the engine gets the time.
//!
//! A trait, for one reason that matters more than testability: **the
//! acceptance corpus writes a fixed instant into every message it sends.** A
//! `SendingTime` two days out of date fails the 120-second skew check, so an
//! engine wired to the wall clock cannot be driven by the corpus at all. The
//! same seam that makes the 59 definitions runnable over a socket is the one a
//! deployment uses to feed a hardware clock.
//!
//! Milliseconds since **0000-01-01**, matching `fixbolt_session::clock` and
//! `DESIGN.md` D13.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds from 0000-01-01 to 1970-01-01.
///
/// Not restated: taken from the session layer, which derives it rather than
/// recalling it.
pub const YEAR_ZERO_TO_EPOCH: u64 = fixbolt_session::clock::MILLIS_YEAR_ZERO_TO_EPOCH;

/// One reading of a clock: a millisecond, and the part of the instant below it.
///
/// **One value and not two**, because the two must come from the same reading.
/// Fetching them separately lets them drift, and a `52=` whose fraction belongs
/// to a different millisecond than its seconds is wrong in a way nothing
/// downstream can detect —
/// [ADR-0057](../../../docs/decisions/ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md)
/// decision 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reading {
    /// Milliseconds since 0000-01-01, the scale `Session::tick` takes (D13).
    pub ms: u64,
    /// Nanoseconds **inside** `ms`, 0 to 999 999. Zero from a clock that does
    /// not know any better, which is the truth and not a placeholder.
    pub sub_ms_nanos: u32,
}

/// What time it is, in the session layer's units.
pub trait Clock {
    /// Milliseconds since 0000-01-01.
    fn now_ms(&mut self) -> u64;

    /// The same instant, plus whatever this clock knows below a millisecond.
    ///
    /// **The default answers `0` for the remainder, and that is not a stub.** A
    /// clock that only knows milliseconds says so, and the session writes a
    /// 21-byte `52=` rather than padding three zeroes onto a resolution nobody
    /// had. Overriding this is how a clock offers more; [`SystemClock`] does,
    /// [`ManualClock`] deliberately does not — the acceptance corpus drives one,
    /// and every byte it compares is a millisecond byte.
    fn now(&mut self) -> Reading {
        Reading {
            ms: self.now_ms(),
            sub_ms_nanos: 0,
        }
    }
}

/// The wall clock. The default everywhere but a test.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&mut self) -> u64 {
        self.now().ms
    }

    /// **One `SystemTime::now()`, split at the millisecond.** `as_millis()` used
    /// to be the whole of this function and threw the remainder away before
    /// anything could ask for it; the reading has always had nanoseconds in it.
    fn now(&mut self) -> Reading {
        let Ok(d) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return Reading::default();
        };
        let ms = u64::try_from(d.as_millis()).unwrap_or(0);
        Reading {
            ms: YEAR_ZERO_TO_EPOCH.saturating_add(ms),
            sub_ms_nanos: d.subsec_nanos() % 1_000_000,
        }
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
