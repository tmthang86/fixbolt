//! Who is on the other end, before there is a session to ask.
//!
//! `DESIGN.md` D8 gives an `Engine` one `Config` and therefore one FIX
//! identity, which is how it answers *"is this identity already logged on"* —
//! it counts the connections **it** holds. `[measured 2026-08-31]` splitting
//! those connections across shards took the acceptance corpus from **59 to
//! 57**, failing exactly `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def`,
//! because there was nothing left to count.
//!
//! The fix is not a cleverer assignment. Assignment happens at `accept`, and
//! the `Logon` that says who the connection belongs to has not arrived yet. So
//! something has to own the socket until it does, and read the identity off it
//! — [ADR-0020].
//!
//! # This module reads bytes. It is not a second session layer
//!
//! ADR-0020 decision 2. `35=`, `49=` and `56=` come off the buffer by direct
//! scan, the way the engine's own `Logon` check already did. **No dictionary,
//! no parse, nothing from `fixbolt_session` but `Config`.** A stage that had to
//! ask the session a question would be designed wrong, because the session it
//! would ask does not exist yet — which is the entire reason this stage is
//! here.
//!
//! It does not frame, either: [`crate::frame::Framer`] already cuts a stream
//! into messages and carries the one rule the corpus taught
//! (`2m_BodyLengthValueNotCorrect.def`). Everything here takes **one complete
//! message** and answers a question about it. Two copies of a framing rule are
//! two rules that will disagree.
//!
//! [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md

/// The two sides a FIX message names, borrowed from the buffer it arrived in.
///
/// **In wire order.** `sender` is `49=` and `target` is `56=` exactly as they
/// appear in the incoming message, which for an acceptor means `sender` is the
/// counterparty. Both connections from one counterparty therefore carry the
/// same pair, which is what lets a router send them to the same shard and lets
/// the single-logon rule count them together again.
///
/// It borrows. Nothing here allocates — `CLAUDE.md` §2 non-negotiable 1 — and a
/// caller that needs to keep an identity past the buffer copies it deliberately
/// rather than by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity<'a> {
    /// `49=`, the SenderCompID as it appears on the wire.
    pub sender: &'a [u8],
    /// `56=`, the TargetCompID as it appears on the wire.
    pub target: &'a [u8],
}

/// The value of the first field whose tag is `tag`, which must include the `=`.
///
/// Fields, not bytes: the scan walks SOH-separated fields and matches the tag
/// at the **start** of one. A search for `49=` anywhere in the message would
/// read a `Text` field containing `49=` as the sender, which is a value a
/// counterparty controls — see
/// `tests/presession.rs::a_field_value_that_looks_like_an_identity_is_not_one`.
///
/// A trailing field with no SOH is not a field. `msg` is a message
/// [`crate::frame::Framer`] has already cut, so the last one is `10=…` and
/// terminated; a caller passing something else gets the same answer for the
/// same reason.
fn field_value<'a>(msg: &'a [u8], tag: &[u8]) -> Option<&'a [u8]> {
    let mut at = 0;
    while at < msg.len() {
        let end = msg[at..].iter().position(|b| *b == 1).map(|e| e + at)?;
        if let Some(value) = msg[at..end].strip_prefix(tag) {
            return Some(value);
        }
        at = end + 1;
    }
    None
}

/// Both sides of one complete message, or `None` if it does not name both.
///
/// Answers a different question from [`is_logon`] on purpose: a caller that
/// wants to route needs the identity, and a caller deciding whether to accept
/// the connection at all needs the message type. Fusing them would make a
/// non-`Logon` indistinguishable from a `Logon` that named nobody, and those
/// two are dropped for different reasons.
#[must_use]
pub fn identity_of(msg: &[u8]) -> Option<Identity<'_>> {
    Some(Identity {
        sender: field_value(msg, b"49=")?,
        target: field_value(msg, b"56=")?,
    })
}

/// Is this complete message a `Logon`?
///
/// Read off the raw bytes rather than parsed: the engine has no dictionary and
/// wants none. `35=` is the third field of a well-formed message and the
/// session refuses anything else, so a scan for it is enough here.
#[must_use]
pub fn is_logon(msg: &[u8]) -> bool {
    field_value(msg, b"35=") == Some(b"A")
}
