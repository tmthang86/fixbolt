//! What a counterparty left behind, asked for at the moment its identity is
//! known.
//!
//! # The seam this fills
//!
//! `[verified 2026-09-02]` [`crate::Engine::add_resumed`] can continue a session
//! that outlived the process, but `crate::serve` and friends **accept
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
    /// `34=` on the next message this session will send.
    ///
    /// **`journal.highest_out() + 1`, and [`Resumed::from_journal`] computes
    /// it.** `[measured 2026-09-05]` it was documented as *"usually
    /// `journal.highest() + 1`"* and every worked example in this repository
    /// took the *usually* as the rule. It is not: `highest` is the highest
    /// message **held for a replay**, and a `Logon`, a `Heartbeat` and a
    /// `Logout` spend numbers no journal holds bytes for, so the two differ by
    /// every administrative message sent since the last application one. A real
    /// `libquickfix` refused the resumed session over a difference of exactly
    /// one. `STATUS.md` item 48,
    /// [ADR-0053](../../../docs/decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md).
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

impl<J: fixbolt_session::journal::Journal> Resumed<J> {
    /// Everything this session left behind, read off the journal that holds it.
    ///
    /// `next_out = highest_out() + 1`, `next_in = highest_in() + 1`,
    /// `last_active_ms = last_active()`. [`None`] when the journal knows
    /// nothing at all — no message sent and none received — which is the
    /// *"start fresh"* answer [`Recovery::recover`] gives.
    ///
    /// # Why this exists rather than three lines at each call site
    ///
    /// `[measured 2026-09-05]` because the three lines were written twice and
    /// were wrong both times, in the same way: `journal.highest() + 1` for
    /// `next_out`, which is short by every administrative message sent after
    /// the last application one. A real `libquickfix` refused the resumed
    /// session with *"MsgSeqNum too low, expecting 4 but received 3"*. The
    /// arithmetic is the engine's to get right once. ADR-0053.
    ///
    /// **The engine still does not decide to resume.** ADR-0010 leaves that
    /// with the caller; this only computes the numbers once the caller has
    /// decided, which is the difference between a helper and a policy.
    ///
    /// A journal written before outbound marks existed answers from its kept
    /// messages and is short by exactly as much as it was before this existed.
    /// There is no number to recover that was never written.
    pub fn from_journal(journal: J) -> Option<Self> {
        let (out, inb) = (journal.highest_out(), journal.highest_in());
        let last_active_ms = journal.last_active();
        if out.is_none() && inb.is_none() && last_active_ms.is_none() {
            return None;
        }
        Some(Self {
            next_out: out.map_or(1, |h| h.saturating_add(1)),
            next_in: inb.map_or(1, |h| h.saturating_add(1)),
            last_active_ms,
            journal,
        })
    }
}

/// Asked once per connection, after the registry has named the counterparty.
///
/// Returning [`None`] means *"start fresh"* and is the ordinary answer for a
/// counterparty with no history.
pub trait Recovery<J> {
    /// Can [`Recovery::recover`] be asked for this counterparty **yet**?
    ///
    /// Asked before every `recover`. `false` means *"not yet"*, never *"no
    /// history"*: the engine **parks** the connection — beside the
    /// pre-session set in the single-engine `serve*` loop and on the sharded
    /// runtime's acceptor thread, in the handshake slot of
    /// `connect_and_serve*` — counts it against the pre-session ceiling, and
    /// asks again at most once per millisecond of its clock, never waiting in
    /// between. A connection still parked after its `LogonTimeout`
    /// ([`Config::logon_timeout_ms`], or the pre-session stage's own limit when
    /// that is zero; the handshake's limit in `connect_and_serve*`) is dropped
    /// unanswered, as one that never sent its `Logon` is.
    ///
    /// **A recovery that opens a `FileJournal` keeps the
    /// [`released`](crate::journal::FileJournal::released) handle of the journal
    /// it handed out for each counterparty, and answers with it.** A departing
    /// connection's journal is retired without waiting for its writer
    /// (ADR-0153), so a counterparty that reconnects at once can find the file
    /// still being written; `FileJournal::open` then refuses it with
    /// `WouldBlock` rather than read it short (ADR-0154). Without `ready`, that
    /// `WouldBlock` reaches `recover` — and must not be read as "nothing was
    /// left behind".
    ///
    /// ```ignore
    /// fn ready(&mut self, cfg: &Config) -> bool {
    ///     // No handle yet: nothing of ours holds the file.
    ///     self.handed_out(cfg).map_or(true, Released::is_released)
    /// }
    /// ```
    ///
    /// **It must answer without a system call.** In the single-engine `serve*`
    /// loops it runs on the engine thread, between turns, at most once per
    /// millisecond per parked connection — in `hft`, on the hot path.
    /// `journal::file_busy` is the wrong answer here: `open(2)` walks a path
    /// and can sleep in the kernel. `Released::is_released` is one atomic load.
    /// The default answers `true`, which is right for any recovery with nothing
    /// that can be busy.
    /// [ADR-0154](../../../docs/decisions/ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md)
    /// decision 3,
    /// [ADR-0155](../../../docs/decisions/ADR-0155-a-recovery-learns-its-writer-let-go-from-the-writer-not-from-the-filesystem.md)
    /// decisions 1–3.
    fn ready(&mut self, _cfg: &Config) -> bool {
        true
    }

    /// What did this counterparty leave behind?
    ///
    /// Called once [`Recovery::ready`] has said yes. **Which thread asks
    /// depends on the entry point**, and it is not always one that may block:
    /// the sharded runtime asks on its acceptor thread, which ADR-0020 and
    /// ADR-0088 allow to block; `connect_and_serve*` asks on its own thread
    /// while no session is up; but the single-engine `serve*` loops ask **on the
    /// engine thread, between turns, while other sessions are being served**.
    /// A recovery that reads a file there costs every session that engine
    /// serves the time of that read — a known cost, recorded in ADR-0154
    /// *Consequences*, not a licence to wait on anything else.
    fn recover(&mut self, cfg: &Config) -> Option<Resumed<J>>;

    /// A journal for a counterparty with no history.
    ///
    /// # Why this exists at all
    ///
    /// `[verified 2026-09-02]` the engine used to build one with `J::default()`
    /// when [`Recovery::recover`] answered [`None`], which put a `J: Default`
    /// bound on the whole serving loop. **A `FileJournal` has no honest
    /// `Default`** — it needs a path — so that bound, and nothing else, was
    /// what stopped `serve_with_recovery` from ever using a journal on disk.
    /// `STATUS.md` item 32 (b).
    ///
    /// # Why it has no default body
    ///
    /// `fn fresh(&mut self, cfg: &Config) -> J where J: Default` was written
    /// first, and it does not work: the `where` clause lands on **callers**,
    /// so the serving loop needed `J: Default` to call it at all and the bound
    /// had simply moved. Requiring the method puts the constraint where it
    /// belongs — on the implementations that want it.
    ///
    /// [`NoRecovery`] and [`FromFn`] implement it for any `J: Default`, so a
    /// `journal::Store` deployment writes nothing extra. A `FileJournal`
    /// deployment writes its own type, which it has to anyway: only it knows
    /// which path belongs to which counterparty.
    fn fresh(&mut self, cfg: &Config) -> J;
}

/// Every session starts fresh. The default, and it must be **exactly neutral**.
///
/// What `crate::serve` and [`crate::serve_hft`] use, so their behaviour is
/// unchanged from before [`Recovery`] existed — and the 59 acceptance
/// definitions run under it.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRecovery;

impl<J: Default> Recovery<J> for NoRecovery {
    fn recover(&mut self, _cfg: &Config) -> Option<Resumed<J>> {
        None
    }

    fn fresh(&mut self, _cfg: &Config) -> J {
        J::default()
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

impl<J: Default, F: FnMut(&Config) -> Option<Resumed<J>>> Recovery<J> for FromFn<F> {
    fn recover(&mut self, cfg: &Config) -> Option<Resumed<J>> {
        (self.0)(cfg)
    }

    fn fresh(&mut self, _cfg: &Config) -> J {
        J::default()
    }
}

/// How one connection's session begins: **fresh**, with a journal the
/// deployment built, or **continued** from what a [`Recovery`] found.
///
/// # Why this type exists at all
///
/// `[2026-09-20]` because the two moments are on different threads. In the
/// sharded runtime (`crate::shard`) the counterparty is named on the acceptor
/// thread, which
/// [ADR-0020](../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)
/// allows to block — so that is where a `Recovery` may read a file. The session
/// is built on a shard thread, which in `hft` mode never blocks (`CLAUDE.md` §2
/// non-negotiable 4). Whatever recovery produced therefore has to **travel**
/// from the first thread to the second, with the connection, and this is what
/// travels.
/// [ADR-0088](../../../docs/decisions/ADR-0088-recovery-reaches-the-sharded-runtime-and-the-journal-crosses-the-channel-with-the-connection.md).
///
/// It is not `Option<Resumed<J>>`: that shape says nothing about where the
/// **fresh** journal comes from, and a `FileJournal` has no honest
/// [`Default`] to fall back on ([ADR-0039]). Both arms carry a journal, so no
/// bound is needed on the thread that receives one.
///
/// `J: Send` is what lets it cross a channel, and
/// `crates/engine/tests/shard_recovery.rs::the_start_that_crosses_the_channel_is_send`
/// is where that is asserted rather than assumed.
///
/// [ADR-0039]: ../../../docs/decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md
#[derive(Debug)]
pub enum Start<J> {
    /// Nothing to continue. The journal is the one [`Recovery::fresh`]
    /// answered, already built on the thread that was allowed to build it.
    Fresh(J),
    /// A session that outlived the process, and the numbers to resume it at.
    Resumed(Resumed<J>),
}

/// Connections whose [`Recovery::ready`] answered *"not yet"*, held where they
/// are rather than waited for.
///
/// [ADR-0154](../../../docs/decisions/ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md)
/// decision 3, for the two loops that take connections from a pre-session set:
/// `pump` (the engine thread) and the sharded runtime's acceptor thread.
/// `connect_and_serve*` holds its one connection in its handshake slot instead.
///
/// **Allocated once, to the pre-session ceiling, and never grown**: a caller
/// admits a new socket only while `set.len() + parked.len()` is under that
/// ceiling, so [`Parking::park`] can never need more room and nothing here
/// allocates after construction (non-negotiable 1). Nothing here sleeps or
/// makes a system call except through `ready` itself.
pub(crate) struct Parking<P> {
    slots: Vec<Parked<P>>,
}

/// One parked connection.
struct Parked<P> {
    item: P,
    cfg: Config,
    /// Dropped, unanswered, once the clock reaches this.
    until_ms: u64,
    /// The millisecond of the engine's clock `ready` was last asked in — so it
    /// is asked **at most once per millisecond**, however fast the loop turns.
    asked_ms: u64,
}

impl<P> Parking<P> {
    /// Room for `n`, taken now.
    pub(crate) fn with_capacity(n: usize) -> Self {
        Self {
            slots: Vec::with_capacity(n),
        }
    }

    /// How many are parked.
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    /// How long a connection for `cfg` may stay parked: its `LogonTimeout`,
    /// or `fallback_ms` — the pre-session stage's limit — when that is zero
    /// ("no limit" is not a promise a parked connection can be given: it holds
    /// one of the pre-session stage's slots).
    pub(crate) const fn limit_ms(cfg: &Config, fallback_ms: u64) -> u64 {
        match cfg.logon_timeout_ms() {
            0 => fallback_ms,
            ms => ms,
        }
    }

    /// Park `item`, whose recovery was just asked at `now_ms` and said no.
    ///
    /// # Errors
    ///
    /// The item back when every slot is taken — unreachable while the caller
    /// keeps the admission rule above; dropping it closes the socket, which is
    /// the answer the pre-session stage gives a connection it has no room for.
    pub(crate) fn park(
        &mut self,
        item: P,
        cfg: Config,
        now_ms: u64,
        limit_ms: u64,
    ) -> Result<(), P> {
        if self.slots.len() >= self.slots.capacity() {
            return Err(item);
        }
        self.slots.push(Parked {
            item,
            cfg,
            until_ms: now_ms.saturating_add(limit_ms),
            asked_ms: now_ms,
        });
        Ok(())
    }

    /// Drop every connection parked past its limit, closing its socket
    /// unanswered. Returns how many.
    pub(crate) fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.slots.len();
        self.slots.retain(|p| now_ms < p.until_ms);
        before - self.slots.len()
    }

    /// The first parked connection whose recovery is ready now, taken out.
    ///
    /// Each is asked **at most once per millisecond** of `now_ms`; one already
    /// asked this millisecond is passed over. So a loop that calls this every
    /// turn — `hft` spins — costs `ready` once a millisecond per parked
    /// connection, and a `standard` loop asks on each of its wakes.
    pub(crate) fn next_ready<J, V: Recovery<J>>(
        &mut self,
        now_ms: u64,
        recovery: &mut V,
    ) -> Option<(P, Config)> {
        let at = self.slots.iter_mut().position(|p| {
            if now_ms <= p.asked_ms {
                return false;
            }
            p.asked_ms = now_ms;
            recovery.ready(&p.cfg)
        })?;
        let p = self.slots.swap_remove(at);
        Some((p.item, p.cfg))
    }
}
