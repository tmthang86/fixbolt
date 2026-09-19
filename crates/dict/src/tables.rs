//! The dictionary questions the **session layer** asks, as one trait.
//!
//! [`fixbolt_codec::Dictionary`] is what the *parser* needs: where the header
//! ends, what a group's members are, which tag carries a length. It lives in
//! `codec` because the parse path cannot depend on anything else. This trait is
//! the other half — what the session's validation pass asks before it decides a
//! `373=` reason — and it lives here because its answers are generated from the
//! XML and one of them returns a [`FieldType`], which is this crate's type.
//!
//! [ADR-0080](../../../docs/decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
//! decision 1: the dictionary rides the encoding. So
//! [`fixbolt_codec::Encoding::Dict`] is bound by `Dictionary` alone — `codec`
//! has zero dependencies and keeps them — and `Session` adds
//! `where E::Dict: Tables` itself. A FIXT 1.1 / FIX 5.0 SP2 session is then the
//! same state machine with a second implementor of this trait (PR B).
//!
//! Every method is an associated function with no receiver, exactly as
//! `Dictionary`'s are: the implementor is a zero-sized marker, the dispatch is
//! static, and nothing on the path allocates or branches on a vtable.

use fixbolt_codec::Dictionary;

use crate::FieldType;

/// What a session asks a dictionary while it validates one message.
///
/// The seven questions below are exactly the ones
/// `crates/session/src/lib.rs`'s validation pass asks; the other three it needs
/// — `is_header`, `group_delimiter`, `group_members` — are already
/// [`Dictionary`]'s, which is why that is a supertrait rather than a second
/// bound written at every call site.
///
/// Each one names the `SessionRejectReason` it answers, because that is the
/// only reason it exists: a question no `373=` code turns on is not a question
/// the session layer may ask.
pub trait Tables: Dictionary {
    /// Whether FIX defines this tag at all — `373=0`, *Invalid tag number*.
    fn is_defined_tag(tag: u32) -> bool;

    /// The header fields every message must carry — `373=1`, together with
    /// [`Tables::required`].
    fn required_header() -> &'static [u32];

    /// The body fields this message type must carry — `373=1`.
    fn required(msg_type: &[u8]) -> &'static [u32];

    /// Whether this message type may carry this tag — `373=2`, *Tag not
    /// defined for this message type*.
    fn allows(msg_type: &[u8], tag: u32) -> bool;

    /// Whether an enumerated field will take this value — `373=5`. `None`
    /// means *the field is not enumerated*, **not** that the value is fine.
    fn enum_allows(tag: u32, value: &[u8]) -> Option<bool>;

    /// The declared type of a field, or `None` for a tag the dictionary does
    /// not define — `373=6` through [`FieldType::accepts`].
    fn field_type(tag: u32) -> Option<FieldType>;

    /// Whether this is a message type the dictionary knows — `373=11`,
    /// *Invalid MsgType*. [`Tables::required`] cannot answer it: it returns
    /// `&[]` both for a type that does not exist and for one that requires
    /// nothing.
    fn is_msg_type(msg_type: &[u8]) -> bool;
}

impl Tables for crate::Fix44 {
    #[inline]
    fn is_defined_tag(tag: u32) -> bool {
        crate::is_defined_tag(tag)
    }

    #[inline]
    fn required_header() -> &'static [u32] {
        crate::required_header()
    }

    #[inline]
    fn required(msg_type: &[u8]) -> &'static [u32] {
        crate::required(msg_type)
    }

    #[inline]
    fn allows(msg_type: &[u8], tag: u32) -> bool {
        crate::allows(msg_type, tag)
    }

    #[inline]
    fn enum_allows(tag: u32, value: &[u8]) -> Option<bool> {
        crate::enum_allows(tag, value)
    }

    #[inline]
    fn field_type(tag: u32) -> Option<FieldType> {
        crate::field_type(tag)
    }

    #[inline]
    fn is_msg_type(msg_type: &[u8]) -> bool {
        crate::is_msg_type(msg_type)
    }
}

#[cfg(test)]
mod tests {
    use super::Tables;
    use crate::{FieldType, Fix44};

    /// The trait answers what the inherent methods answer, for every question.
    ///
    /// Delegation is the whole implementation, so the failure mode worth a test
    /// is a method wired to the wrong generated function — which reads fine and
    /// would only show up as a wrong `373=` code four crates away.
    #[test]
    fn the_trait_and_the_inherent_methods_agree() {
        assert_eq!(
            <Fix44 as Tables>::is_defined_tag(35),
            Fix44::is_defined_tag(35)
        );
        assert!(!<Fix44 as Tables>::is_defined_tag(5000));
        assert_eq!(
            <Fix44 as Tables>::required_header(),
            Fix44::required_header()
        );
        assert_eq!(<Fix44 as Tables>::required(b"D"), Fix44::required(b"D"));
        assert_eq!(<Fix44 as Tables>::allows(b"D", 11), Fix44::allows(b"D", 11));
        assert_eq!(
            <Fix44 as Tables>::enum_allows(54, b"1"),
            Fix44::enum_allows(54, b"1")
        );
        assert_eq!(<Fix44 as Tables>::field_type(34), Fix44::field_type(34));
        assert_eq!(<Fix44 as Tables>::field_type(34), Some(FieldType::SeqNum));
        assert_eq!(
            <Fix44 as Tables>::is_msg_type(b"D"),
            Fix44::is_msg_type(b"D")
        );
        assert!(!<Fix44 as Tables>::is_msg_type(b"ZZZ"));
    }
}
