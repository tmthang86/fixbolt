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
    /// Keep `bytes` as the message numbered `seq`.
    ///
    /// **Only application messages are ever offered.** QuickFIX never replays
    /// an administrative message; it fills over it, and so does this session.
    ///
    /// An implementation may refuse — there is no return value and no error,
    /// because there is nothing the session could do about it. What a refusal
    /// costs is a `SequenceReset` gap fill instead of a replay, which is a
    /// legal answer to every `ResendRequest`.
    fn put(&mut self, seq: u32, bytes: &[u8]);

    /// The message numbered `seq`, if it is still held.
    fn get(&self, seq: u32) -> Option<&[u8]>;
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
    fn put(&mut self, _seq: u32, _bytes: &[u8]) {}

    fn get(&self, _seq: u32) -> Option<&[u8]> {
        None
    }
}
