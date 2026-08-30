//! `standard` mode's idle turn: block on readiness, and give the core back.
//!
//! ADR-0014 decisions 1, 5 and 6. [`crate::poll::Poller`] is the syscall; this
//! is the policy around it — how long to wait, what a signal means, and what
//! happens when the kernel refuses.
//!
//! # The timeout is a correctness parameter, not a knob
//!
//! `Session` takes no clock (`DESIGN.md` D1): it judges `SendingTime` and
//! heartbeats against the last `Input::Tick` it was handed. In `hft` the engine
//! ticks every turn, thousands of times a second, so time is effectively
//! continuous to the session. **In `standard` the engine is asleep**, and the
//! only thing that wakes it with nothing on the wire is this timeout — so the
//! timeout *is* the coarsest grain of time the session can see.
//!
//! [`DEFAULT_TIMEOUT_MS`] is 100. `HeartBtInt` is a whole number of seconds and
//! the three thresholds are 1.0, 1.2 and 2.4 times it, so 100 ms is a tenth of
//! the smallest interval that means anything.
//!
//! # A timeout of zero is not this mode
//!
//! It is a spin wearing this mode's name — ADR-0014 decision 6 lists it as one
//! of the four ways to build a `standard` engine that is not one. [`Block::new`]
//! cannot produce one and [`Block::with_timeout_ms`] refuses it.

use std::time::{Duration, Instant};

use crate::poll::{PollError, Poller};
use crate::transport::Interest;
use crate::wait::Waiting;

/// The default poll timeout, in milliseconds. ADR-0014 decision 5.
pub const DEFAULT_TIMEOUT_MS: u32 = 100;

/// The smallest timeout this type will accept.
///
/// Not zero, and not one: a timeout short enough to be indistinguishable from a
/// spin defeats the entire mode, and the gate that measures CPU over a
/// wall-clock window would be the first thing to notice — after the engine had
/// already shipped burning a core.
pub const MIN_TIMEOUT_MS: u32 = 5;

/// Block until a source is ready or the timeout passes. **`standard` mode.**
///
/// # It cannot be paired with an `Engine` yet, and that is a compile error
///
/// [`crate::Engine::idle`] still hands its waiter an **empty** source list, so
/// a `Block` inside one would block on nothing and wake only on its own
/// timeout. That engine is *correct*: it answers every message, passes the 59
/// acceptance definitions, and reads 0% CPU. It is also 100 ms slower per
/// message, which no correctness suite and no CPU measurement can see.
///
/// So the pairing is refused where it is written, not remembered:
///
/// ```compile_fail,E0080
/// # struct App;
/// # impl fixbolt_session::Application for App {
/// #     fn on_message(&mut self, _m: &[u8], _s: u32, _t: &[u8], _o: &mut [u8])
/// #         -> Option<core::ops::Range<usize>> { None }
/// # }
/// use fixbolt_engine::{Engine, block::Block, clock::SystemClock};
/// use fixbolt_engine::{dispatch::InlineDispatch, journal::Store, transport::TcpTransport};
///
/// let mut engine: Engine<
///     TcpTransport, fixbolt_session::Acceptor, InlineDispatch<App>,
///     SystemClock, Block, Store, 256, 4096, 8192,
/// > = Engine::new(
///     fixbolt_session::Config::acceptor(b"FIX.4.4", b"ISLD", b"TEST"),
///     InlineDispatch::new(App),
///     SystemClock,
///     Block::new(8),
///     4,
/// );
/// engine.idle();
/// ```
///
/// Step 4 of `docs/plans/2026-08-30-standard-mode.md` builds the real list and
/// swaps that assertion for ADR-0014 decision 4's, which refuses a transport
/// that cannot be waited on at all. **This doctest is expected to keep failing
/// to compile then too** — for the other reason.
///
/// # Errors it cannot return
///
/// [`Waiting::idle`] returns `()`, so a `poll` that fails has nowhere to go.
/// Two things happen instead, and neither is silence:
///
/// - **The core is still given back.** A failing `poll` must not turn this into
///   a busy loop that carries `standard`'s name; the remaining time is slept
///   out instead.
/// - **The error is kept**, readable through [`Block::last_error`]. An error
///   that cannot be returned must at least be observable — the same principle
///   ADR-0011 settled for a full ring: *the refusal is never silent*.
pub struct Block {
    poller: Poller,
    timeout: Duration,
    timeout_ms: u32,
    last_error: Option<PollError>,
}

impl Block {
    /// A blocking strategy sized for `capacity` sources, at
    /// [`DEFAULT_TIMEOUT_MS`].
    ///
    /// `capacity` is every source one idle turn can carry: one per connection,
    /// plus the listener, plus the waker. Undersizing it is not fatal — it
    /// costs one allocation on a path that must not have any, and
    /// [`crate::poll::Poller::wait`] says so at the point it happens.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self::with_timeout_ms(capacity, DEFAULT_TIMEOUT_MS)
    }

    /// As [`Self::new`], with a timeout the caller chose.
    ///
    /// A timeout below [`MIN_TIMEOUT_MS`] is **raised to it** rather than
    /// honoured. Refusing would mean returning a `Result` from a constructor
    /// that has nothing else to fail on; honouring it would mean shipping a
    /// spin under this name. Read [`Self::timeout_ms`] back if it matters.
    #[must_use]
    pub fn with_timeout_ms(capacity: usize, timeout_ms: u32) -> Self {
        let timeout_ms = timeout_ms.max(MIN_TIMEOUT_MS);
        Self {
            poller: Poller::with_capacity(capacity),
            timeout: Duration::from_millis(u64::from(timeout_ms)),
            timeout_ms,
            last_error: None,
        }
    }

    /// The timeout actually in force, which is not always the one asked for.
    #[must_use]
    pub const fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }

    /// The last `poll` failure, if there has been one.
    ///
    /// `None` on a healthy engine. Anything else means idle turns are being
    /// slept through rather than waited through, and the reason is here.
    #[must_use]
    pub const fn last_error(&self) -> Option<PollError> {
        self.last_error
    }

    /// Forget the last failure, after acting on it.
    pub const fn clear_last_error(&mut self) {
        self.last_error = None;
    }
}

impl Waiting for Block {
    const SLEEPS: bool = true;
    const NEEDS_SOURCES: bool = true;

    fn idle(&mut self, interests: &[Interest]) {
        let started = Instant::now();
        loop {
            let elapsed = started.elapsed();
            let Some(left) = self.timeout.checked_sub(elapsed) else {
                return;
            };
            // Saturating rather than `as`: a timeout that overflowed `c_int`
            // would wrap to a negative number, and a negative timeout means
            // **wait forever** — a tick that never arrives and a session that
            // never heartbeats.
            let left_ms = i32::try_from(left.as_millis()).unwrap_or(i32::MAX);
            if left_ms <= 0 {
                return;
            }

            match self.poller.wait(interests, left_ms) {
                Ok(_) => return,
                // A signal is not a wakeup and is not an error. Go back and
                // wait out **what is left** — re-waiting the full timeout would
                // let a stream of signals extend an idle turn without bound,
                // and with it the grain of time the session sees.
                Err(PollError::Interrupted) => {}
                Err(e) => {
                    self.last_error = Some(e);
                    // Still give the core back. A `poll` that refuses must not
                    // silently convert this mode into the one it exists to
                    // replace.
                    if let Some(left) = self.timeout.checked_sub(started.elapsed()) {
                        std::thread::sleep(left);
                    }
                    return;
                }
            }
        }
    }
}
