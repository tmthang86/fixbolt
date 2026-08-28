//! FIX 4.4 tables, generated from `vendor/quickfix/spec/FIX44.xml` at build time.
//!
//! Nothing here is hand-written. See `build.rs`, and ADR-0001 for why the XML is
//! data rather than something copied into this repository.

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
}

impl Fix44 {
    /// See the module-level `required` — knowingly incomplete, no caller yet.
    #[inline]
    #[must_use]
    pub fn required(msg_type: &[u8]) -> &'static [u32] {
        required(msg_type)
    }
}
