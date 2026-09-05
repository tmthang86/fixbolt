//! Where the messages this end has already sent are kept, so a
//! `ResendRequest` can be answered.
//!
//! # Why it is a trait and not a buffer
//!
//! `DESIGN.md` D1 says the session layer is pure: no socket, no clock, no
//! allocation. A journal is none of those things — it is a **file**, and D7
//! says which file and how often it is flushed is the *user's* policy, not the
//! session's. So the session asks two questions and holds nothing:
//!
//! > keep `34=n`, and these are exactly its bytes
//!
//! > do you still have `34=n`?
//!
//! D1's original sketch had the session emit an `Action::Store(seq, bytes)`
//! instead. A trait is the same information with a name and a return value —
//! `get` has to answer, and an emitted action cannot.
//! [ADR-0008](../../../docs/decisions/ADR-0008-journal-is-a-trait.md)
//! records the difference and why.

/// The messages this end has sent, by sequence number.
pub trait Journal {
    /// Keep `bytes` as the message numbered `seq`. `true` if it was kept.
    ///
    /// **Only application messages are ever offered.** QuickFIX never replays
    /// an administrative message; it fills over it, and so does this session.
    ///
    /// An implementation may refuse — a message longer than a slot, a journal
    /// that keeps nothing — and what a refusal costs is a `SequenceReset` gap
    /// fill instead of a replay, which is a legal answer to every
    /// `ResendRequest`. There is still no *error*: there is nothing the
    /// session could do differently.
    ///
    /// `[2026-09-04]` **It used to return nothing, and the argument for that
    /// was one step short.** The session can do nothing about a refusal and can
    /// *count* it, which is
    /// [ADR-0046](../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)
    /// decision 5's second silence: without this, an acceptor whose replies are
    /// longer than its slots gap-fills every resend for the rest of its life
    /// and nothing on this side ever says so.
    fn put(&mut self, seq: u32, bytes: &[u8]) -> bool;

    /// The message numbered `seq`, if it is still held.
    fn get(&self, seq: u32) -> Option<&[u8]>;

    /// The highest sequence number this journal holds, or `None` when it holds
    /// nothing.
    ///
    /// **This is what a restart needs and nothing else provides.** Until it
    /// existed a journal was written and never read back, so
    /// `Durability::Fsync` bought an audit trail rather than a recovery
    /// mechanism — a cost that looks like a guarantee.
    ///
    /// There is deliberately **no default implementation**. A default returning
    /// `None` would let a journal that does hold messages report that it holds
    /// none, and a session resuming from it would silently start again at 1.
    fn highest(&self) -> Option<u32>;

    /// The lowest sequence number this journal can still answer [`Self::get`]
    /// for, or `None` when it holds nothing.
    ///
    /// **This is what separates *"never sent"* from *"fell out of the ring"*,
    /// and only the second is worth telling an operator about.** A session
    /// answering a `ResendRequest` gap-fills both, identically and legally; with
    /// this it can also count the second, which is
    /// [ADR-0046](../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)
    /// decision 1.
    ///
    /// **No default**, for the reason [`Self::highest`] has none: a default
    /// returning `None` would let a journal that does hold messages report that
    /// it holds none, and the counter built on it would read zero forever.
    fn oldest(&self) -> Option<u32>;

    /// Record that inbound sequence number `seq` has been consumed.
    ///
    /// **Called after the application has seen the message, never before** —
    /// [ADR-0017](../../../docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md).
    /// Writing it first would mean an ill-timed crash *loses* the message, since
    /// this end has already counted it and will not ask for a resend; writing it
    /// afterwards means the message is delivered twice, and the second copy
    /// arrives with `43=Y` because it comes from a `ResendRequest` this end
    /// issued. FIX has a flag for the second failure and nothing for the first.
    ///
    /// The window is moved, not closed: a crash between the application seeing
    /// the message and this call still reprocesses. There is no atomic step
    /// spanning an external application's side effects and this engine's disk,
    /// and **an application behind this engine must be idempotent per sequence
    /// number** — `GUIDE.md` carries that, because the type system cannot.
    ///
    /// No return value, for the same reason [`Self::put`] has none.
    fn mark_in(&mut self, seq: u32);

    /// Record that the session was alive at `at_ms`, on the engine's clock.
    ///
    /// **A default no-op, so a journal that does not survive a restart is not
    /// obliged to pretend.** Only a durable journal has anything useful to do
    /// here.
    ///
    /// It exists because the sequence numbers cannot imply it: `next_out = 9`
    /// says nothing about whether a trading day has ended since 9 was reached,
    /// and [ADR-0033]'s boundary reset needs that instant to compare against
    /// after a restart. `[2026-09-02]` before this, the caller had to keep it
    /// somewhere of its own — `STATUS.md` item 32 (c).
    ///
    /// **Not called per message.** The engine records it when a session logs on
    /// and when an ordered shutdown says goodbye; anything more frequent is a
    /// write on the hot path, which D8 forbids.
    ///
    /// [ADR-0033]: ../../../docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md
    fn mark_active(&mut self, at_ms: u64) {
        let _ = at_ms;
    }

    /// The latest instant [`Self::mark_active`] was given, if any.
    ///
    /// [`None`] means *"this journal does not know"* — an in-memory journal, or
    /// a durable one written before this existed. It is **not** the same as
    /// *"the session was never active"*, and a caller must treat it as the
    /// former: guessing a boundary from an absent instant is how a session
    /// silently restarts its numbering.
    fn last_active(&self) -> Option<u64> {
        None
    }

    /// The highest inbound sequence number this journal has been told about, or
    /// `None` if it has been told about none.
    ///
    /// A resumed session's `next_in` is this plus one. **No default**, for the
    /// reason [`Self::highest`] has none: a journal holding state must not be
    /// able to report that it holds none.
    fn highest_in(&self) -> Option<u32>;

    /// Record that outbound sequence numbers up to and including `seq` have
    /// been spent.
    ///
    /// **A high-water mark, not an event.** It is told *the highest number
    /// spent so far*, so being told a number it already knows must do nothing,
    /// and a [`Self::put`] that was kept raises this too — a `mark_out`
    /// following a successful `put` of the same number therefore writes
    /// nothing. The invariant is one sentence: **the journal always knows the
    /// highest outbound number spent.**
    ///
    /// # Why this exists beside `put`
    ///
    /// `put` is offered application messages only, because the journal is the
    /// store a `ResendRequest` is answered from and an administrative message
    /// is never replayed — it is filled over. But a `Logon`, a `Heartbeat` and
    /// a `Logout` each spend a `34=` all the same, so **`highest()` is not the
    /// highest number this session has sent** and a restart deriving one from
    /// the other is short by exactly the administrative messages that followed
    /// the last application one.
    ///
    /// `[measured 2026-09-05]` One clean logout was enough: a real
    /// `libquickfix` refused the resumed session with *"MsgSeqNum too low,
    /// expecting 4 but received 3"*.
    /// [ADR-0053](../../../docs/decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md).
    ///
    /// **A default no-op, like [`Self::mark_active`]**: a journal that does not
    /// survive a restart is not obliged to pretend.
    fn mark_out(&mut self, seq: u32) {
        let _ = seq;
    }

    /// The highest outbound sequence number this journal knows to have been
    /// spent — from [`Self::put`] and [`Self::mark_out`] together — or `None`
    /// if it knows of none.
    ///
    /// A resumed session's `next_out` is this plus one, which is the whole
    /// reason it exists. **No default**, for the reason [`Self::highest`] has
    /// none: a journal holding state must not be able to report that it holds
    /// none, because a session resuming from it would silently start again at 1.
    ///
    /// A durable journal written before [`Self::mark_out`] existed answers
    /// whatever its kept messages say and is **short by the same amount it was
    /// before** — not worse, not better. There is no way to reconstruct a
    /// number that was never written.
    fn highest_out(&self) -> Option<u32>;
}

/// A journal that keeps nothing. `DESIGN.md` D7's `None`.
///
/// The default for tests and simulators, and what [`super::Session::received`]
/// uses so that a caller with no journal at all has the same API. A session
/// wired to this one answers every `ResendRequest` with a gap fill, which is
/// legal — and loses nothing that was not already lost, because a journal that
/// does not survive a restart could not have replayed after one either.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoJournal;

impl Journal for NoJournal {
    /// Always `false`. A journal that keeps nothing refuses everything, and
    /// saying so is more useful than a `true` that is not true.
    fn put(&mut self, _seq: u32, _bytes: &[u8]) -> bool {
        false
    }

    fn oldest(&self) -> Option<u32> {
        None
    }

    fn highest(&self) -> Option<u32> {
        None
    }

    fn mark_in(&mut self, _seq: u32) {}

    fn highest_in(&self) -> Option<u32> {
        None
    }

    /// Always `None`. A journal that keeps nothing has counted nothing, and a
    /// caller resuming from it has nothing to resume — which is the truth.
    fn highest_out(&self) -> Option<u32> {
        None
    }

    fn get(&self, _seq: u32) -> Option<&[u8]> {
        None
    }
}
