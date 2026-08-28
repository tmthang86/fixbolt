//! What the parser needs to know about the FIX dictionary, and nothing more.
//!
//! A trait rather than a dependency: `codec` must not depend on `dict`, or the
//! hot path would carry a table it may not need and `codec`'s zero-dependency
//! rule would be gone. Every method is an associated function — there is no
//! receiver, no `dyn`, and no vtable on the parse path.

/// The dictionary questions the codec asks.
///
/// The three group functions are **not implemented in step 1**. They are
/// declared here because `Dictionary` is public API and adding a method later
/// is a breaking change — the same reasoning ADR-0004 used for `Role`. Their
/// bodies arrive with the repeating-groups plan.
pub trait Dictionary {
    /// Is this tag a header field? Used when ordering an outbound message
    /// (D3): header fields ascend, then body fields ascend.
    fn is_header(tag: u32) -> bool;

    /// For a DATA field, the tag of the length field that must immediately
    /// precede it. `None` for everything else.
    ///
    /// A DATA value may legally contain `0x01`, so the parser must take the
    /// declared number of bytes instead of scanning for the separator.
    fn data_length_tag(tag: u32) -> Option<u32>;

    /// The tag that ends the first entry of a repeating group and begins each
    /// subsequent one.
    ///
    /// Keyed by `(msg_type, counter)`, never by counter alone: four counters in
    /// FIX 4.4 take different delimiters in different messages. Not implemented
    /// in step 1.
    fn group_delimiter(_msg_type: &[u8], _counter: u32) -> Option<u32> {
        None
    }

    /// Every tag that may appear inside this group. A tag outside the set ends
    /// the group. Not implemented in step 1.
    fn group_members(_msg_type: &[u8], _counter: u32) -> &'static [u32] {
        &[]
    }

    /// Declaration order within the group, delimiter first. Used when writing.
    /// Not implemented in step 1.
    fn group_order(_msg_type: &[u8], _counter: u32) -> &'static [u32] {
        &[]
    }
}

/// A dictionary that knows nothing.
///
/// Lets `codec` be parsed and tested without `dict`. With `NoDict` no field is
/// treated as DATA, so a DATA value containing `0x01` is cut at the wrong byte
/// — which is correct behaviour for a parser that was told nothing, and the
/// reason `data_length_tag` exists.
pub struct NoDict;

impl Dictionary for NoDict {
    #[inline]
    fn is_header(_tag: u32) -> bool {
        false
    }
    #[inline]
    fn data_length_tag(_tag: u32) -> Option<u32> {
        None
    }
}
