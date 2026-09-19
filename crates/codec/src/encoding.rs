//! One trait over the encodings, and the tag=value implementation of it.
//!
//! [ADR-0079](../../../docs/decisions/ADR-0079-one-view-per-encoding-and-one-trait-over-them.md)
//! decides that each encoding keeps **its own view type** — `MessageView` stays
//! exactly what it is — and that what is shared is the *shape of access*: parse a
//! frame into a view without copying (D2), read a field by a generated identifier
//! (D3), patch an outbound template (D9), and hand the session a small `Copy`
//! value. This module is that shape, and nothing else.
//!
//! **Static dispatch only.** Every method is an associated function with no
//! receiver, so there is no vtable and no `dyn Encoding` on any path a message
//! takes. Every one of them is `#[inline]` and, for [`TagValue`], forwards
//! unchanged to the function it replaces: [`parse_into`], [`MessageView::get`],
//! [`FieldIndex::view`] and [`Template::encode_with`]. The trait adds a name,
//! not a branch — which is why `benches/alloc.rs` counts a `parse via Encoding`
//! case beside the direct one and both must read 0.
//!
//! # Three places this differs from the plan's sketch
//!
//! The sketch in `docs/plans/2026-09-19-phase-2-fixt-and-sbe.md` (*PR A*) is a
//! sketch; row **A1** of *Chia việc* is the spec, and it requires `parse_into`,
//! `MessageView` and `FieldIndex` to be unchanged and `TagValue` to *delegate* to
//! them. Three things follow from that, each visible in the signatures below.
//!
//! 1. **`parse` returns the existing [`Parsed`], and [`Encoding::view`] is its
//!    own method** rather than `Parsed<View>` carrying the view. Making `Parsed`
//!    generic would change the existing public API, and the split is load-bearing
//!    anyway: after [`ParseError::BadTag`] the index holds every field read
//!    *before* the bad one, and `14a_BadField` passes only because the session
//!    still builds a view over it and reads `34=`. A view reachable only out of
//!    the `Ok` branch could not serve that.
//! 2. **One `encode` rather than `patch` then `encode`.** A [`Template`] is
//!    immutable by design (D9): it is laid out once per session and message type,
//!    and the holes are filled from `slots` at send time. There is no mutable
//!    template to patch between the two calls.
//! 3. **Two error types, not one.** `parse` fails with [`ParseError`] and
//!    `encode` with [`EncodeError`], exactly as the functions they forward to do,
//!    so a caller's `match` arms are the ones it already has.
//!
//! Repeating groups are deliberately absent: ADR-0079's four shared operations do
//! not include writing one, and `Template::encode_with`'s `GroupData` is a
//! tag=value shape that has no meaning under SBE. A caller that writes groups
//! uses the concrete `Template` API.

use core::marker::PhantomData;
use core::ops::Range;

use crate::dict::Dictionary;
use crate::index::{FieldIndex, MessageView};
use crate::parse::{ParseError, Parsed, Validation, parse_into};
use crate::template::{EncodeError, Template};

/// The header fields the session layer reads, borrowed out of one message.
///
/// ADR-0079 decision 3: the session needs *these* from a view and nothing else,
/// so it can stay generic over the encoding and still be pure. Every field is
/// `Option` because a message that is missing one is a message the session must
/// answer, not one the codec may refuse — the same boundary `parse.rs` draws.
///
/// The values are raw bytes, unparsed. Converting `34=` to a number is the
/// session's job and its errors are the session's (`as_u32` is right here in
/// `codec` when a caller wants it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionFields<'a> {
    /// `8` — `BeginString`.
    pub begin_string: Option<&'a [u8]>,
    /// `35` — `MsgType`.
    pub msg_type: Option<&'a [u8]>,
    /// `34` — `MsgSeqNum`.
    pub msg_seq_num: Option<&'a [u8]>,
    /// `49` — `SenderCompID`.
    pub sender_comp_id: Option<&'a [u8]>,
    /// `56` — `TargetCompID`.
    pub target_comp_id: Option<&'a [u8]>,
    /// `52` — `SendingTime`.
    pub sending_time: Option<&'a [u8]>,
    /// `43` — `PossDupFlag`.
    pub poss_dup_flag: Option<&'a [u8]>,
    /// `122` — `OrigSendingTime`.
    pub orig_sending_time: Option<&'a [u8]>,
}

/// What every encoding must offer the session, the engine and the application.
///
/// Implementors are **marker types**, never values: every method is an
/// associated function, so `E` is chosen at compile time and monomorphised away
/// (ADR-0079 decision 2). [`TagValue`] is the one implementation in this crate;
/// SBE brings its own, with its own view.
pub trait Encoding {
    /// The borrowed view over one parsed message. `Copy`, and small — 24 bytes
    /// for [`MessageView`], which `lib.rs` asserts at compile time.
    type View<'a>: Copy;

    /// How a field is named: the tag for tag=value, a schema field id for SBE.
    type Field: Copy;

    /// The lookup tables this encoding validates against
    /// ([ADR-0080](../../../docs/decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
    /// decision 1: the dictionary rides the encoding, it is not a second type
    /// parameter). The bound is `codec`'s own [`Dictionary`] — the richer
    /// `dict::Tables` bound belongs where `dict` is visible, because `codec` has
    /// zero dependencies and must keep them.
    type Dict: Dictionary;

    /// The memory the caller keeps per connection and re-fills per message:
    /// [`FieldIndex<N>`] for tag=value. Owned by the caller, never returned by
    /// value (ADR-0003).
    type Scratch: Default;

    /// The outbound skeleton with holes in it (D9). `P` and `S` are the caller's
    /// capacity choices, exactly as on [`Template`]; an encoding that has no use
    /// for them ignores them.
    type Template<const P: usize, const S: usize>;

    /// Why a frame could not be read.
    type ParseError: Copy;

    /// Why a message could not be written.
    type EncodeError: Copy;

    /// Read one message from the front of `buf` into `scratch`.
    ///
    /// Same contract as [`parse_into`], including what `scratch` holds after an
    /// error: see [`Encoding::view`].
    fn parse(
        buf: &[u8],
        scratch: &mut Self::Scratch,
        v: Validation,
    ) -> Result<Parsed, Self::ParseError>;

    /// Borrow `buf` through `scratch`.
    ///
    /// Separate from [`Encoding::parse`] on purpose: after
    /// [`ParseError::BadTag`] the scratch holds every field read before the bad
    /// tag, and the session reads `34=` out of it to answer. A view that only
    /// existed on success could not do that.
    fn view<'a>(scratch: &'a Self::Scratch, buf: &'a [u8]) -> Self::View<'a>;

    /// The value of one field, or `None` if the message does not carry it.
    fn field<'a>(view: Self::View<'a>, f: Self::Field) -> Option<&'a [u8]>;

    /// The session header fields, or `None` for an encoding that carries no FIX
    /// session layer at all (SBE: ADR-0078).
    fn session_fields<'a>(view: Self::View<'a>) -> Option<SessionFields<'a>>;

    /// Write one message: the template's fixed bytes, with `slots` filling its
    /// holes, into `out`.
    ///
    /// Returns the range `out` is occupied by — **not** a length. The prefix is
    /// right-aligned in front of the body so the body never moves, so the
    /// message does not begin at `out[0]`.
    fn encode<const P: usize, const S: usize>(
        t: &Self::Template<P, S>,
        out: &mut [u8],
        slots: &[(Self::Field, &[u8])],
    ) -> Result<Range<usize>, Self::EncodeError>;
}

/// FIX tag=value under [`Encoding`]: dictionary `D`, index capacity `N`.
///
/// A marker, with no values and no size — `PhantomData` and nothing else, and
/// its field is private so none can be built. `TagValue<Fix44, 64>` is the FIX
/// 4.4 encoding; `dict` names it `Fix44TagValue`.
pub struct TagValue<D, const N: usize>(PhantomData<D>);

impl<D: Dictionary, const N: usize> Encoding for TagValue<D, N> {
    type View<'a> = MessageView<'a, N>;
    type Field = u32;
    type Dict = D;
    type Scratch = FieldIndex<N>;
    type Template<const P: usize, const S: usize> = Template<P, S>;
    type ParseError = ParseError;
    type EncodeError = EncodeError;

    #[inline]
    fn parse(
        buf: &[u8],
        scratch: &mut Self::Scratch,
        v: Validation,
    ) -> Result<Parsed, Self::ParseError> {
        parse_into::<D, N>(buf, scratch, v)
    }

    #[inline]
    fn view<'a>(scratch: &'a Self::Scratch, buf: &'a [u8]) -> Self::View<'a> {
        scratch.view(buf)
    }

    #[inline]
    fn field<'a>(view: Self::View<'a>, f: Self::Field) -> Option<&'a [u8]> {
        view.get(f)
    }

    #[inline]
    fn session_fields<'a>(view: Self::View<'a>) -> Option<SessionFields<'a>> {
        Some(SessionFields {
            begin_string: view.get(8),
            msg_type: view.get(35),
            msg_seq_num: view.get(34),
            sender_comp_id: view.get(49),
            target_comp_id: view.get(56),
            sending_time: view.get(52),
            poss_dup_flag: view.get(43),
            orig_sending_time: view.get(122),
        })
    }

    #[inline]
    fn encode<const P: usize, const S: usize>(
        t: &Self::Template<P, S>,
        out: &mut [u8],
        slots: &[(Self::Field, &[u8])],
    ) -> Result<Range<usize>, Self::EncodeError> {
        // `&[(Self::Field, &[u8])]` *is* `&[(u32, &[u8])]` here, so `slots` goes
        // through untouched: no copy, no collect, nothing to allocate.
        t.encode_with::<D>(out, slots, &[])
    }
}
