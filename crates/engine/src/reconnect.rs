//! When an initiator that lost its connection should try again.
//!
//! `STATUS.md` item 35. `fixbolt_engine::connect` opens a socket; nothing
//! decided when to open it again, and **nothing covered that**: the 59
//! acceptance definitions are written for an acceptor and never reconnect an
//! initiator, the mirrored corpus is at 2 / 50, and `scripts/interop.sh`
//! connects once. So every test in `tests/reconnect.rs` is **invented**, which
//! is stated rather than left to be discovered — the same weakness
//! `crates/dict/tests/field_types.rs` carries and says so.
//!
//! # It answers a question; it does not act
//!
//! [`Policy::next`] returns an instant, and **never sleeps**. Non-negotiable 4:
//! in `hft` the engine thread does not block in the kernel, so a policy that
//! waited would be a bug in the one place the design is least willing to have
//! one. The caller already has a wait strategy; this tells it what to wait for.
//!
//! It also owns no clock. Time arrives as an argument, the way it does
//! everywhere else in this workspace.

use fixbolt_session::schedule::Schedule;

/// What the caller should do about the connection it does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Open one now.
    Now,
    /// Not yet — ask again at this instant, on the scale `Tick` carries.
    At(u64),
    /// Never again. The caller said stop.
    Stop,
}

/// Why a set of bounds was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolicyError {
    /// A first delay of zero is a reconnect loop with no gap in it.
    FirstIsZero,
    /// A ceiling below the first delay is a ceiling that never applies, and
    /// almost certainly two arguments the wrong way round.
    CeilingBelowFirst,
}

/// The rule an initiator follows after a connection ends.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    first_ms: u64,
    ceiling_ms: u64,
    schedule: Schedule,
    attempt: u32,
    not_before_ms: u64,
    stopped: bool,
}

impl Policy {
    /// Doubling from `first_ms`, never past `ceiling_ms`.
    ///
    /// # Errors
    ///
    /// [`PolicyError`] for a zero first delay or a ceiling below it.
    pub const fn new(first_ms: u64, ceiling_ms: u64) -> Result<Self, PolicyError> {
        if first_ms == 0 {
            return Err(PolicyError::FirstIsZero);
        }
        if ceiling_ms < first_ms {
            return Err(PolicyError::CeilingBelowFirst);
        }
        Ok(Self {
            first_ms,
            ceiling_ms,
            schedule: Schedule::always(),
            attempt: 0,
            not_before_ms: 0,
            stopped: false,
        })
    }

    /// Only call out inside these hours.
    #[must_use]
    pub const fn with_schedule(mut self, s: Schedule) -> Self {
        self.schedule = s;
        self
    }

    /// The connection ended at `now_ms`. Climb one rung.
    ///
    /// Called for **every** ending, including one this end asked for: a policy
    /// that only counted failures would reconnect instantly after a clean
    /// logout, which is a reconnect storm with a polite name. A caller that
    /// meant to stop calls [`Self::stop`].
    ///
    /// [an-initiator-that-comes-back]: ../../../docs/plans/2026-09-02-an-initiator-that-comes-back.md
    pub const fn dropped(&mut self, now_ms: u64) {
        self.attempt = self.attempt.saturating_add(1);
        self.not_before_ms = now_ms.saturating_add(self.delay());
    }

    /// A session got up. Put the ladder back at the bottom.
    ///
    /// **Not "a socket connected".** A TCP connection that is refused a `Logon`
    /// and dropped is a failure, and counting it as success is how a policy
    /// ends up hammering a counterparty that is up but refusing — which is the
    /// case backoff exists for.
    pub const fn logged_on(&mut self) {
        self.attempt = 0;
    }

    /// Stop reconnecting. Final.
    pub const fn stop(&mut self) {
        self.stopped = true;
    }

    /// What to do at `now_ms`.
    ///
    /// **The schedule is asked before the ladder.** Outside its hours the
    /// answer is never `Now`, whatever the ladder says — dialling a venue that
    /// is shut earns a refusal, and a refusal climbs the ladder for a reason
    /// that has nothing to do with the network.
    #[must_use]
    pub fn next(&self, now_ms: u64) -> Next {
        if self.stopped {
            return Next::Stop;
        }
        if !self.schedule.contains(now_ms) {
            // Ask again, rather than name an instant this cannot compute:
            // `Schedule` answers *is this instant inside a window* and has no
            // "when does the next one open". See the plan's Sửa 1.
            return Next::At(now_ms.saturating_add(self.ceiling_ms));
        }
        if now_ms >= self.not_before_ms {
            return Next::Now;
        }
        Next::At(self.not_before_ms)
    }

    /// `first_ms << (attempt - 1)`, capped — and capped **without shifting past
    /// the width of the type**, which is why this is not one expression.
    ///
    /// `1u64 << 64` is undefined in C and a panic in a debug Rust build; an
    /// initiator that has been down for a few thousand attempts is exactly the
    /// caller that would find it.
    const fn delay(&self) -> u64 {
        let steps = self.attempt.saturating_sub(1);
        if steps >= 63 {
            return self.ceiling_ms;
        }
        match self.first_ms.checked_shl(steps) {
            Some(d) if d <= self.ceiling_ms => d,
            _ => self.ceiling_ms,
        }
    }
}
