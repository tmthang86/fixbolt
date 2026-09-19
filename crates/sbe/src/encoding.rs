//! `Sbe<S>`: SBE under `codec`'s [`Encoding`] trait (ADR-0079), for schema `S`.
//!
//! Each associated type is the one ADR-0082 names, and the `const _` at the
//! bottom of this file stops the build if one changes:
//!
//! | Associated type | `Sbe<S>` | Why |
//! |---|---|---|
//! | `View<'a>` | [`SbeView<'a>`] | 24 bytes, `Copy` — the size of `MessageView` (ADR-0081 decision 1) |
//! | `Field` | [`FieldId`] | a schema field id; root-block fields only |
//! | `Dict` | [`SbeTables<S>`] | `codec::Dictionary` only (ADR-0082 decision 4) |
//! | `Scratch` | [`MessageHeader`] | the header `parse` read; all a view needs besides the buffer |
//! | `Template<P, N>` | [`SbeTemplate<S, N>`] | `N` bounds the skeleton's bytes; `P` is unused |
//! | `ParseError`, `EncodeError` | [`SbeError`] | one fieldless error for both, as the reader already had |
//!
//! **No session.** [`Encoding::session_fields`] is `None`: SBE carries no FIX
//! session header (ADR-0078), and `Session<Sbe<S>>` does not compile
//! (ADR-0082 decision 4; the `compile_fail` proof lives in `library`, which
//! sees both crates).
//!
//! **Encode is flat.** [`Encoding::encode`] writes the root block only: slots
//! are `(FieldId, wire bytes)` and a group entry's field has no single
//! position, so every group goes out with count 0 and every `varData` with
//! length 0. ADR-0082 decision 3 keeps the trait at its four operations (no
//! builder is added to it); a message with groups or `varData` is written with
//! the crate's native [`MessageWriter`](crate::MessageWriter). There is no
//! `patch` (the plan, *PR A*, deviation 2): `encode` copies the skeleton and
//! writes the slots in one call.

use core::marker::PhantomData;
use core::ops::Range;

use fixbolt_codec::{Encoding, Parsed, SessionFields, Validation};

use crate::encode::{FieldId, SbeTemplate};
use crate::error::SbeError;
use crate::header::{HEADER_LEN, MessageHeader};
use crate::schema::Schema;
use crate::tables::SbeTables;
use crate::view::SbeView;

/// SBE 1.0 for schema `S`, as a `codec::Encoding` marker. No values: its
/// field is private, as `codec::TagValue`'s is.
pub struct Sbe<S>(PhantomData<fn() -> S>);

impl<S: Schema> Encoding for Sbe<S> {
    type View<'a> = SbeView<'a>;
    type Field = FieldId;
    type Dict = SbeTables<S>;
    type Scratch = MessageHeader;
    type Template<const P: usize, const N: usize> = SbeTemplate<S, N>;
    type ParseError = SbeError;
    type EncodeError = SbeError;

    /// Reads the header at the front of `buf` into `scratch` and measures the
    /// whole message by walking its groups and `varData` over `S`'s layout.
    ///
    /// SBE has no frame of its own, so the message's length comes from the
    /// layout: `Complete { consumed }` is the header, the root block
    /// (`blockLength` from the wire) and every group and `varData`.
    /// `Incomplete` whenever the bytes run out first — a truncated SBE message
    /// is TCP's normal case here, as for tag=value. `v` is ignored: SBE has no
    /// body length or checksum to check.
    ///
    /// # Errors
    /// `WrongSchema` when the header's `schemaId` is not `S::ID`;
    /// `UnknownTemplate` when `S` has no such template — without its layout
    /// the message cannot be measured, so a stream needs framing (SOFH) to
    /// skip it. `scratch` holds the header in both cases.
    #[inline]
    fn parse(
        buf: &[u8],
        scratch: &mut Self::Scratch,
        _v: Validation,
    ) -> Result<Parsed, Self::ParseError> {
        if buf.len() < HEADER_LEN {
            return Ok(Parsed::Incomplete);
        }
        let h = MessageHeader::decode(buf, S::BYTE_ORDER)?;
        *scratch = h;
        if h.schema_id != S::ID {
            return Err(SbeError::WrongSchema);
        }
        let layout = S::message(h.template_id).ok_or(SbeError::UnknownTemplate)?;
        let end = SbeView::new(buf, S::BYTE_ORDER)
            .and_then(|view| view.tail::<S>())
            .and_then(|tail| tail.skip_tail(layout.groups, layout.var_data));
        match end {
            Ok(cursor) => Ok(Parsed::Complete {
                consumed: cursor.position(),
            }),
            Err(SbeError::Truncated) => Ok(Parsed::Incomplete),
            Err(e) => Err(e),
        }
    }

    /// The view `parse` read into `scratch`, over `buf`. Cannot fail: every
    /// read through it is bounds-checked, so a `buf` that does not hold what
    /// the header says reads as absent, not out of bounds.
    #[inline]
    fn view<'a>(scratch: &'a Self::Scratch, buf: &'a [u8]) -> Self::View<'a> {
        SbeView::from_header(buf, *scratch)
    }

    /// The wire bytes of root field `f` — the same bytes
    /// [`crate::FieldRef::bytes`] returns. `None` when the message is not
    /// `S`'s, the template has no such root field, or the message's version
    /// predates it. A field made only of constants is `Some(&[])`: it has no
    /// wire bytes, and its value is in the table.
    ///
    /// A linear scan of the root fields by id: for access by id; a hot path
    /// holds the `&'static FieldLayout` and uses [`crate::Block::field`].
    #[inline]
    fn field<'a>(view: Self::View<'a>, f: Self::Field) -> Option<&'a [u8]> {
        let layout = view.layout::<S>().ok()?.field(f.0)?;
        let field = view.root::<S>().ok()?.field(layout).ok()??;
        Some(field.bytes())
    }

    /// Always `None`: SBE carries no FIX session layer (ADR-0078).
    #[inline]
    fn session_fields<'a>(_view: Self::View<'a>) -> Option<SessionFields<'a>> {
        None
    }

    /// Writes one **flat** message: the template's skeleton with each slot's
    /// wire bytes at its root field's offset. Groups go out with count 0 and
    /// `varData` with length 0 — use [`crate::MessageWriter`] to fill them.
    /// Returns `0..len`: an SBE message has no prefix to right-align.
    ///
    /// # Errors
    /// As [`SbeTemplate::encode`].
    #[inline]
    fn encode<const P: usize, const N: usize>(
        t: &Self::Template<P, N>,
        out: &mut [u8],
        slots: &[(Self::Field, &[u8])],
    ) -> Result<Range<usize>, Self::EncodeError> {
        t.encode(out, slots)
    }
}

/// ADR-0082 pins `Sbe<S>`'s associated types. This proves them at every build
/// — a change to any one of them stops compilation here, not in a caller.
const _: () = {
    struct Probe;
    impl Schema for Probe {
        const ID: u16 = 0;
        const VERSION: u16 = 0;
        const BYTE_ORDER: crate::ByteOrder = crate::ByteOrder::Little;
        fn message(_template_id: u16) -> Option<&'static crate::MessageLayout> {
            None
        }
    }
    fn pinned<E>()
    where
        for<'a> E: Encoding<View<'a> = SbeView<'a>>,
        E: Encoding<
                Field = FieldId,
                Dict = SbeTables<Probe>,
                Scratch = MessageHeader,
                Template<0, 64> = SbeTemplate<Probe, 64>,
                ParseError = SbeError,
                EncodeError = SbeError,
            >,
    {
    }
    let _ = pinned::<Sbe<Probe>>;
};
