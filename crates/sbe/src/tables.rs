//! [`SbeTables<S>`], the `Dict` of `Sbe<S>`'s `codec::Encoding` impl.
//!
//! ADR-0082 decision 4: `SbeTables<S>` implements **`codec::Dictionary` only**.
//! `dict::Tables` is a *session* bound, and `Session<Sbe<S>>` is refused on
//! five type equalities before `Tables` is ever asked, so an impl of it would
//! satisfy a constraint nothing can reach — and `sbe` does not depend on
//! `dict`.
//!
//! What SBE needs to validate a message is `S` itself (its [`Schema`]
//! layouts); `codec::Dictionary`'s questions are tag=value questions, and each
//! has a fixed SBE answer, stated on the method.

use core::marker::PhantomData;

use fixbolt_codec::Dictionary;

use crate::schema::Schema;

/// The dictionary `Sbe<S>` rides on (ADR-0080 decision 1: the dictionary
/// rides the encoding). A marker with no values: its field is private.
pub struct SbeTables<S>(PhantomData<fn() -> S>);

impl<S: Schema> Dictionary for SbeTables<S> {
    /// `false` for every id: SBE has no FIX header of fields to order first —
    /// its header is the fixed 8-byte composite, and a field's position is
    /// its offset in the schema, not an order a writer chooses (D3).
    #[inline]
    fn is_header(_tag: u32) -> bool {
        false
    }

    /// `None` for every id: an SBE field has a fixed width, and `varData`
    /// carries its own length prefix rather than naming a length field.
    #[inline]
    fn data_length_tag(_tag: u32) -> Option<u32> {
        None
    }
}
