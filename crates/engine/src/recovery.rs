//! What a counterparty left behind, asked for at the moment its identity is
//! known.
//!
//! # The seam this fills
//!
//! `[verified 2026-09-02]` [`crate::Engine::add_resumed`] can continue a session
//! that outlived the process, but [`crate::serve`] and friends **accept
//! connections themselves** — the embedder never sees a transport to call it
//! with. So a deployment that used the convenient entry point could not resume
//! anything, which made recovery a feature you had to give up the serving loop
//! to use. `STATUS.md` item 31.
//!
//! # Why it is asked here and not earlier
//!
//! Before the `Logon` there is **no identity**. The pre-session stage owns the
//! socket until one arrives
//! ([ADR-0020](../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)),
//! and the registry turns that identity into a [`Config`]
//! ([ADR-0026](../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)).
//! Only then is there anything to look a journal up by. So [`Recovery`] is
//! consulted in exactly one place: after the registry has chosen, before the
//! connection reaches the engine.
//!
//! # Why a trait, and not a map
//!
//! The same reasoning ADR-0026 gave for [`crate::presession::Registry`]. One
//! deployment reads a `FileJournal` per counterparty off disk, another asks a
//! database, a third is a test. The engine does not need to know which, and
//! returning [`None`] — *"this counterparty left nothing"* — is a complete
//! answer rather than an error.
//!
//! # What it deliberately does not do
//!
//! It does not read your journal for you and it does not guess.
//! [ADR-0010](../../../docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)
//! is explicit that choosing between a restart and a continuation belongs to
//! the caller; this is where the engine asks, not where it decides.

use fixbolt_session::Config;

/// What one counterparty left behind.
///
/// The journal travels **with** the numbers, and that is not a convenience:
/// correct counts over an empty journal answer the first `ResendRequest` with a
/// `SequenceReset` gap fill — legal, and a silent loss of exactly what the
/// counterparty asked for.
/// `crates/engine/tests/engine_recovery.rs::a_resumed_session_with_an_empty_journal_fills_the_gap_instead`
/// is the test that tells the two outcomes apart.
#[derive(Debug)]
pub struct Resumed<J> {
    /// What this session already sent, and how far its inbound count reached.
    pub journal: J,
    /// `34=` on the next message this session will send. Usually
    /// `journal.highest() + 1`, and *usually* is why the engine does not
    /// compute it.
    pub next_out: u32,
    /// `34=` this session expects next from the counterparty.
    pub next_in: u32,
    /// When this session was last known to be active, on the engine's clock
    /// scale.
    ///
    /// [`Some`] and a schedule boundary crossed since then restarts both counts
    /// ([ADR-0033](../../../docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)).
    /// [`None`] and no boundary is ever noticed — right under
    /// `Schedule::always`, and wrong under anything else.
    ///
    /// It is separate from the counts because they cannot imply it:
    /// `next_out = 9` says nothing about whether a trading day has ended since
    /// 9 was reached.
    pub last_active_ms: Option<u64>,
}

/// Asked once per connection, after the registry has named the counterparty.
///
/// Returning [`None`] means *"start fresh"* and is the ordinary answer for a
/// counterparty with no history.
pub trait Recovery<J> {
    /// What did this counterparty leave behind?
    ///
    /// Called on the acceptor thread, which ADR-0020 allows to block — so an
    /// implementation may read a file. It is **not** called on the engine
    /// thread and never on a turn.
    fn recover(&mut self, cfg: &Config) -> Option<Resumed<J>>;
}

/// Every session starts fresh. The default, and it must be **exactly neutral**.
///
/// What [`crate::serve`] and [`crate::serve_hft`] use, so their behaviour is
/// unchanged from before [`Recovery`] existed — and the 59 acceptance
/// definitions run under it.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRecovery;

impl<J> Recovery<J> for NoRecovery {
    fn recover(&mut self, _cfg: &Config) -> Option<Resumed<J>> {
        None
    }
}

/// A `Recovery` that is a plain function.
///
/// For the common case where the lookup is one closure and a named type would
/// be ceremony:
///
/// ```no_run
/// # use fixbolt_engine::recovery::{FromFn, Resumed};
/// # use fixbolt_engine::journal::Store;
/// let recovery = FromFn::new(|_cfg: &fixbolt_session::Config| -> Option<Resumed<Store>> {
///     None
/// });
/// ```
#[derive(Debug, Clone, Copy)]
pub struct FromFn<F>(F);

impl<F> FromFn<F> {
    /// Wrap the closure.
    pub const fn new(f: F) -> Self {
        Self(f)
    }
}

impl<J, F: FnMut(&Config) -> Option<Resumed<J>>> Recovery<J> for FromFn<F> {
    fn recover(&mut self, cfg: &Config) -> Option<Resumed<J>> {
        (self.0)(cfg)
    }
}
