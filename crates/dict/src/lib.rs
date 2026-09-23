//! FIX 4.4 tables, generated from `spec/FIX44.xml` (shipped in this crate, under
//! `NOTICE`) at build time.
//!
//! Nothing here is hand-written. See `build.rs` for the generator; the XML itself
//! is committed inside this crate, under `NOTICE`, as ADR-0001 decision 5 and
//! [ADR-0104](../../../docs/decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)
//! explain.
#![cfg_attr(docsrs, feature(doc_cfg))]

mod field_type;
mod tables;

pub use field_type::FieldType;
pub use tables::Tables;

/// The third-party notice for the QuickFIX-derived dictionaries this crate
/// ships (ADR-0104). Contains the QuickFIX Software License's condition 3
/// acknowledgment ("This product includes software developed by
/// quickfixengine.org…"), its condition 5 naming restriction, and the pinned
/// commit the shipped files come from.
///
/// A binary that links `fixbolt-dict` — directly, or through `fixbolt`, which
/// re-exports this as `fixbolt::NOTICE` — carries QuickFIX-derived tables and
/// owes the licence's conditions 2 and 3.
/// Printing this constant wherever the application lists its third-party
/// notices satisfies condition 3's "in the software itself" clause; see
/// `docs/GUIDE.md` for what a distributor owes beyond that.
pub const NOTICE: &str = include_str!("../NOTICE");

/// FIX 4.4 tag=value under [`fixbolt_codec::Encoding`].
///
/// The encoding a `Session` is generic over by default, and the name
/// `engine` and `library` build their own aliases on. `64` is the
/// [`fixbolt_codec::FieldIndex`] capacity the alias fixes; a caller that wants
/// another writes `TagValue<Fix44, N>` itself, exactly as it used to pick `N`
/// on `Session` (`CLAUDE.md` §6 — the caller picks `N`, no hidden constant).
pub type Fix44TagValue = fixbolt_codec::TagValue<Fix44, 64>;

// `[measured 2026-09-08]` The generated table indexes its own arrays, twice,
// each time under a bound clippy cannot see. **An `#[allow]` written here does
// not reach it** — rustc calls it an unused attribute and the two errors stand,
// because a lint inside an `include!` belongs to the included file. So the
// allow is emitted by `build.rs` onto the two generated functions themselves,
// and this file stays under `indexing_slicing = "deny"` like every other.
// STATUS.md item 55.
include!(concat!(env!("OUT_DIR"), "/fix44.rs"));

/// The FIXT 1.1 / FIX 5.0 SP2 tables, generated from the **pair**
/// `spec/FIXT11.xml` + `spec/FIX50SP2.xml` (shipped in this crate, under
/// `NOTICE`) at build time.
///
/// Behind the `fix50sp2` feature, and behind a module rather than at the crate
/// root: the generated free functions have the same names as FIX 4.4's and two
/// `field_type` at one scope do not compile. `CLAUDE.md` §2 item 6 — the
/// feature gates this `mod`, not only `Cargo.toml`.
///
/// ADR-0080 decision 2 and ADR-0083 say how the two files become one table:
/// header, trailer and the eight admin messages come from the transport file,
/// fields, components, groups and the 156 application messages from the
/// application file, and every message resolves its components against one
/// merged map.
#[cfg(feature = "fix50sp2")]
pub mod fixt11_fix50sp2 {
    include!(concat!(env!("OUT_DIR"), "/fixt11_fix50sp2.rs"));
}

/// The FIX 4.4 dictionary, as `codec` sees it.
///
/// A zero-sized type: `Dictionary`'s methods are associated functions, so there
/// is no receiver to pass and nothing on the parse path but a `match`.
pub struct Fix44;

impl fixbolt_codec::Dictionary for Fix44 {
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

/// The FIXT 1.1 / FIX 5.0 SP2 dictionary, as `codec` and the session see it.
///
/// The same zero-sized shape as [`Fix44`], over the tables in
/// [`fixt11_fix50sp2`]. A `Session` is generic over the encoding and the
/// dictionary rides it (ADR-0080 decision 1), so this type is the whole of what
/// a FIXT 1.1 session needs that a FIX 4.4 one does not.
#[cfg(feature = "fix50sp2")]
pub struct Fixt11Fix50Sp2Tables;

#[cfg(feature = "fix50sp2")]
impl fixbolt_codec::Dictionary for Fixt11Fix50Sp2Tables {
    #[inline]
    fn is_header(tag: u32) -> bool {
        fixt11_fix50sp2::is_header(tag)
    }

    #[inline]
    fn data_length_tag(tag: u32) -> Option<u32> {
        fixt11_fix50sp2::data_length_tag(tag)
    }

    #[inline]
    fn group_delimiter(msg_type: &[u8], counter: u32) -> Option<u32> {
        // The head of the member list, never a table of its own — the same
        // rule `Fix44` follows and for the same reason.
        match fixt11_fix50sp2::group_members(msg_type, counter) {
            [first, ..] => Some(*first),
            [] => None,
        }
    }

    #[inline]
    fn group_members(msg_type: &[u8], counter: u32) -> &'static [u32] {
        fixt11_fix50sp2::group_members(msg_type, counter)
    }

    #[inline]
    fn group_order(msg_type: &[u8], counter: u32) -> &'static [u32] {
        fixt11_fix50sp2::group_members(msg_type, counter)
    }
}

#[cfg(feature = "fix50sp2")]
impl Tables for Fixt11Fix50Sp2Tables {
    #[inline]
    fn is_defined_tag(tag: u32) -> bool {
        fixt11_fix50sp2::is_defined_tag(tag)
    }

    #[inline]
    fn is_defined_tag_for(msg_type: &[u8], tag: u32) -> bool {
        // ADR-0084 decision 1. Two files, two layers: a message of the
        // transport file carries only the transport file's tags, and
        // `999=LegUnitOfMeasure` on a Heartbeat is `373=0` rather than `373=2`
        // — which is what both QuickFIX engines answer, and what
        // `14a_BadField.def` expects in all three FIXT corpora.
        if fixt11_fix50sp2::is_transport_message(msg_type) {
            fixt11_fix50sp2::is_transport_tag(tag)
        } else {
            fixt11_fix50sp2::is_defined_tag(tag)
        }
    }

    #[inline]
    fn required_header() -> &'static [u32] {
        fixt11_fix50sp2::required_header()
    }

    #[inline]
    fn required(msg_type: &[u8]) -> &'static [u32] {
        fixt11_fix50sp2::required(msg_type)
    }

    #[inline]
    fn allows(msg_type: &[u8], tag: u32) -> bool {
        fixt11_fix50sp2::allows(msg_type, tag)
    }

    #[inline]
    fn enum_allows(tag: u32, value: &[u8]) -> Option<bool> {
        fixt11_fix50sp2::enum_allows(tag, value)
    }

    #[inline]
    fn field_type(tag: u32) -> Option<FieldType> {
        fixt11_fix50sp2::field_type(tag)
    }

    #[inline]
    fn is_msg_type(msg_type: &[u8]) -> bool {
        fixt11_fix50sp2::is_msg_type(msg_type)
    }

    #[inline]
    fn is_admin(msg_type: &[u8]) -> bool {
        // `[measured 2026-09-20]` the admin set of the pair is the transport
        // file's own `<messages>`: all 8 of `FIXT11.xml` carry `msgcat='admin'`
        // and all 156 of `FIX50SP2.xml` carry `msgcat='app'`. The generated
        // function still reads `msgcat` across both files rather than aliasing
        // `is_transport_message`, so the day one file changes category the two
        // answers part and `fixt.rs` says so.
        fixt11_fix50sp2::is_admin(msg_type)
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

    /// Header fields every message must carry.
    ///
    /// [`Self::required`] answers for a message body and says so in its own
    /// doc; this is the other half. `14b_RequiredFieldMissing.def` sends a
    /// Heartbeat with no `56=` and expects `373=1` with `371=56`.
    #[inline]
    #[must_use]
    pub const fn required_header() -> &'static [u32] {
        required_header()
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
