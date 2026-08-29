//! The `58=Text` values the 59 definitions expect, and their `373=` codes.
//!
//! `Comparator.rb` compares values byte for byte, so these strings are part of
//! the gate. An engine that writes "Required tag is missing" where QuickFIX
//! writes "Required tag missing" fails a test about sequence numbers, and
//! nothing in the failure says the word "text".
//!
//! # Why this is not called `RejectText`
//!
//! `[measured 2026-08-28]` 17 distinct texts appear on `E` lines, 44 times.
//! Only **12** carry a `373=` and sit on a `Reject (35=3)`. The other five do
//! not:
//!
//! | Text | Carried on |
//! |---|---|
//! | `Incorrect BeginString` | `Logout (35=5)` |
//! | `MsgSeqNum too low, expecting 3 but received 1` | `Logout (35=5)` |
//! | `MsgSeqNum too low, expecting 5 but received 2` | `Logout (35=5)` |
//! | `No Products found for this Class Symbol` | `SecurityDefinition (35=d)` |
//! | `Unsupported Message Type` | `BusinessMessageReject (35=j)` |
//!
//! The two that carry numbers are **Logout reasons**, not reject reasons — the
//! plan said reject, and it was wrong.
//!
//! # No `format!`, no allocation
//!
//! `CLAUDE.md` §2 non-negotiable 2 forbids `format!` in the session layer, and
//! the two numbered texts are exactly the case that tempts it. So the enum is
//! fieldless except for one named variant, and [`SessionText::render`] writes
//! into a caller-supplied buffer. Nothing here allocates and nothing here
//! formats.
//!
//! # Where this belongs eventually
//!
//! In `session`, which does not exist yet. `DESIGN.md` §7 builds the gate first,
//! so it starts here; when the session layer lands the enum moves and this crate
//! keeps only the corpus-derived assertion in `tests/text.rs`. Written to move:
//! no `std`, no dependency on anything in this crate.

/// A `58=Text` value the acceptance definitions expect.
///
/// The variants that pair with a `373=` are listed in code order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionText {
    /// `373=0`
    InvalidTagNumber,
    /// `373=1`
    RequiredTagMissing,
    /// `373=2`
    TagNotDefinedForThisMessageType,
    /// `373=4`
    TagSpecifiedWithoutValue,
    /// `373=5`
    ValueIsIncorrect,
    /// `373=6`
    IncorrectDataFormat,
    /// `373=9`
    CompIdProblem,
    /// `373=10`
    SendingTimeAccuracyProblem,
    /// `373=11`
    InvalidMsgType,
    /// `373=13`
    TagAppearsMoreThanOnce,
    /// `373=14`
    TagSpecifiedOutOfRequiredOrder,
    /// `373=16`
    IncorrectNumInGroupCount,
    /// On a `Logout`, with no `373=`.
    IncorrectBeginString,
    /// On a `BusinessMessageReject`, with no `373=`.
    UnsupportedMessageType,
    /// On a `SecurityDefinition` — an application text, not a session one. It
    /// is here because the comparator does not care which layer produced it.
    NoProductsFound,
    /// On a `Logout`, with no `373=`. **The only variant with fields**, and the
    /// reason [`render`](Self::render) exists instead of a `&'static str`.
    MsgSeqNumTooLow { expecting: u32, received: u32 },
}

impl SessionText {
    /// Every fieldless variant.
    ///
    /// `MsgSeqNumTooLow` is absent because it is not one value: the corpus
    /// carries two instances of it and a session will produce many more.
    pub const ALL: &'static [Self] = &[
        Self::InvalidTagNumber,
        Self::RequiredTagMissing,
        Self::TagNotDefinedForThisMessageType,
        Self::TagSpecifiedWithoutValue,
        Self::ValueIsIncorrect,
        Self::IncorrectDataFormat,
        Self::CompIdProblem,
        Self::SendingTimeAccuracyProblem,
        Self::InvalidMsgType,
        Self::TagAppearsMoreThanOnce,
        Self::TagSpecifiedOutOfRequiredOrder,
        Self::IncorrectNumInGroupCount,
        Self::IncorrectBeginString,
        Self::UnsupportedMessageType,
        Self::NoProductsFound,
    ];

    /// The longest fixed text: `Value is incorrect (out of range) for this tag`.
    ///
    /// A caller sizing a buffer for a `Reject` needs this and not a guess.
    pub const MAX_FIXED_LEN: usize = 46;

    /// The longest any text can be, `MsgSeqNumTooLow` at two ten-digit numbers.
    ///
    /// The corpus only ever shows single digits, so this is the bound that a
    /// real session hits and the corpus never would.
    pub const MAX_LEN: usize = 29 + 10 + 14 + 10;

    /// The `373=SessionRejectReason` this text goes with, if any.
    ///
    /// `[measured]` the corpus uses 12 of FIX 4.4's reasons: `3`, `7`, `8`,
    /// `12`, `15` and `17` never appear. A session that needs one of those has
    /// no expected text to match and no test that covers it.
    #[must_use]
    pub const fn session_reject_reason(self) -> Option<u32> {
        Some(match self {
            Self::InvalidTagNumber => 0,
            Self::RequiredTagMissing => 1,
            Self::TagNotDefinedForThisMessageType => 2,
            Self::TagSpecifiedWithoutValue => 4,
            Self::ValueIsIncorrect => 5,
            Self::IncorrectDataFormat => 6,
            Self::CompIdProblem => 9,
            Self::SendingTimeAccuracyProblem => 10,
            Self::InvalidMsgType => 11,
            Self::TagAppearsMoreThanOnce => 13,
            Self::TagSpecifiedOutOfRequiredOrder => 14,
            Self::IncorrectNumInGroupCount => 16,
            Self::IncorrectBeginString
            | Self::UnsupportedMessageType
            | Self::NoProductsFound
            | Self::MsgSeqNumTooLow { .. } => return None,
        })
    }

    /// The fixed part of this text, or `None` for the one that is not fixed.
    #[must_use]
    pub const fn as_str(self) -> Option<&'static str> {
        Some(match self {
            Self::InvalidTagNumber => "Invalid tag number",
            Self::RequiredTagMissing => "Required tag missing",
            Self::TagNotDefinedForThisMessageType => "Tag not defined for this message type",
            Self::TagSpecifiedWithoutValue => "Tag specified without a value",
            Self::ValueIsIncorrect => "Value is incorrect (out of range) for this tag",
            Self::IncorrectDataFormat => "Incorrect data format for value",
            Self::CompIdProblem => "CompID problem",
            Self::SendingTimeAccuracyProblem => "SendingTime accuracy problem",
            Self::InvalidMsgType => "Invalid MsgType",
            Self::TagAppearsMoreThanOnce => "Tag appears more than once",
            Self::TagSpecifiedOutOfRequiredOrder => "Tag specified out of required order",
            Self::IncorrectNumInGroupCount => "Incorrect NumInGroup count for repeating group",
            Self::IncorrectBeginString => "Incorrect BeginString",
            Self::UnsupportedMessageType => "Unsupported Message Type",
            Self::NoProductsFound => "No Products found for this Class Symbol",
            Self::MsgSeqNumTooLow { .. } => return None,
        })
    }

    /// Write this text into `out` and return how many bytes it took.
    ///
    /// `None` when `out` is too short, and then **nothing is written**: half a
    /// reason on the wire is worse than none. Size a buffer with
    /// [`MAX_FIXED_LEN`](Self::MAX_FIXED_LEN) or [`MAX_LEN`](Self::MAX_LEN).
    #[must_use]
    pub fn render(self, out: &mut [u8]) -> Option<usize> {
        if let Some(s) = self.as_str() {
            let b = s.as_bytes();
            out.get_mut(..b.len())?.copy_from_slice(b);
            return Some(b.len());
        }
        let Self::MsgSeqNumTooLow {
            expecting,
            received,
        } = self
        else {
            return None;
        };
        // Measured to fit before anything is written, so a short buffer is
        // refused rather than half-filled.
        let mut e = [0u8; 10];
        let mut r = [0u8; 10];
        let e = digits(expecting, &mut e);
        let r = digits(received, &mut r);
        const A: &[u8] = b"MsgSeqNum too low, expecting ";
        const B: &[u8] = b" but received ";
        let n = A.len() + e.len() + B.len() + r.len();
        let dst = out.get_mut(..n)?;
        let (head, rest) = dst.split_at_mut(A.len());
        head.copy_from_slice(A);
        let (num, rest) = rest.split_at_mut(e.len());
        num.copy_from_slice(e);
        let (mid, num2) = rest.split_at_mut(B.len());
        mid.copy_from_slice(B);
        num2.copy_from_slice(r);
        Some(n)
    }
}

/// ASCII digits of `v`, right-aligned in `buf`.
///
/// A local copy of what `template.rs` calls `render_u32`. Widening `codec`'s
/// public API for it would be a public API change, which `CLAUDE.md` §1 puts
/// behind its own plan; ten lines is the cheaper answer until this module moves
/// into `session` and the two can be reconciled deliberately.
fn digits(mut v: u32, buf: &mut [u8; 10]) -> &[u8] {
    if v == 0 {
        buf[9] = b'0';
        return &buf[9..];
    }
    let mut i = 10;
    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + u8::try_from(v % 10).unwrap_or(0);
        v /= 10;
    }
    &buf[i..]
}
