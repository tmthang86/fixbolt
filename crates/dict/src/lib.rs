//! FIX 4.4 tables, generated from `vendor/quickfix/spec/FIX44.xml` at build time.
//!
//! Nothing here is hand-written. See `build.rs`, and ADR-0001 for why the XML is
//! data rather than something copied into this repository.

mod field_type;

pub use field_type::FieldType;

include!(concat!(env!("OUT_DIR"), "/fix44.rs"));

/// The FIX 4.4 dictionary, as `codec` sees it.
///
/// A zero-sized type: `Dictionary`'s methods are associated functions, so there
/// is no receiver to pass and nothing on the parse path but a `match`.
pub struct Fix44;

impl nanofix_codec::Dictionary for Fix44 {
    #[inline]
    fn is_header(tag: u32) -> bool {
        is_header(tag)
    }

    #[inline]
    fn data_length_tag(tag: u32) -> Option<u32> {
        data_length_tag(tag)
    }

    #[inline]
    fn group_delimiter(msg_type: &[u8], counter: u32) -> Option<u32> {
        // The head of the member list, never a table of its own: two tables
        // are two things that can disagree about the same group.
        match group_members(msg_type, counter) {
            [first, ..] => Some(*first),
            [] => None,
        }
    }

    #[inline]
    fn group_members(msg_type: &[u8], counter: u32) -> &'static [u32] {
        group_members(msg_type, counter)
    }

    #[inline]
    fn group_order(msg_type: &[u8], counter: u32) -> &'static [u32] {
        // Declaration order already begins with the delimiter, so `order` and
        // `members` are one list read two ways.
        group_members(msg_type, counter)
    }
}

impl Fix44 {
    /// See the module-level `required` — knowingly incomplete, no caller yet.
    #[inline]
    #[must_use]
    pub fn required(msg_type: &[u8]) -> &'static [u32] {
        required(msg_type)
    }

    /// Whether FIX 4.4 defines this tag at all.
    ///
    /// The dictionary's answer to `SessionRejectReason 0`, *Invalid tag
    /// number*. Note that there is **no user-defined range**: QuickFIX's own
    /// header calls 5000..=9999 user-defined, and the acceptance corpus expects
    /// `5000=HI` refused anyway.
    #[inline]
    #[must_use]
    pub const fn is_defined_tag(tag: u32) -> bool {
        is_defined_tag(tag)
    }

    /// Whether an enumerated field will take this value.
    ///
    /// The dictionary's answer to `SessionRejectReason 5`, *Value is incorrect
    /// (out of range) for this tag*. `None` means the field is not enumerated —
    /// **not** that the value is fine. Confusing the two makes `373=5` fire on
    /// nothing, and no acceptance definition would notice.
    #[inline]
    #[must_use]
    pub fn enum_allows(tag: u32, value: &[u8]) -> Option<bool> {
        enum_allows(tag, value)
    }

    /// Whether this message type may carry this tag.
    ///
    /// The dictionary's answer to `SessionRejectReason 2`, *Tag not defined for
    /// this message type*. Header and trailer tags are allowed on every message
    /// — otherwise `52=` would be refused on everything.
    #[inline]
    #[must_use]
    pub fn allows(msg_type: &[u8], tag: u32) -> bool {
        allows(msg_type, tag)
    }

    /// The declared type of a field, or `None` if FIX 4.4 does not define the
    /// tag.
    ///
    /// The dictionary's answer to `SessionRejectReason 6`, *Incorrect data
    /// format for value* — via [`FieldType::accepts`].
    #[inline]
    #[must_use]
    pub const fn field_type(tag: u32) -> Option<FieldType> {
        field_type(tag)
    }

    /// Whether this is one of the 93 FIX 4.4 message types.
    ///
    /// The dictionary's answer to `SessionRejectReason 11`, *Invalid MsgType*.
    /// [`Self::required`] cannot answer it — it gives `&[]` both for a message
    /// type that does not exist and for one that requires nothing.
    #[inline]
    #[must_use]
    pub fn is_msg_type(msg_type: &[u8]) -> bool {
        is_msg_type(msg_type)
    }
}
