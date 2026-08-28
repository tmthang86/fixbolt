//! Parse and serialise FIX 4.4 in place at the I/O buffer.
//!
//! `no_std` on purpose. That alone proves nothing about allocation — the crate
//! could still pull in `alloc`, and a caller can allocate freely. What proves it
//! is `benches/alloc.rs` with a counting allocator. See the plan, "Bất biến bị
//! đụng tới", row 1.
#![no_std]

pub mod checksum;
pub mod dict;
pub mod group;
pub mod index;
pub mod parse;
pub mod template;
pub mod timestamp;

pub use checksum::{checksum, format_checksum};
pub use dict::{Dictionary, NoDict};
pub use group::{GroupEntry, GroupIter};
pub use index::{ConvertError, FieldEntry, FieldIndex, MessageView, as_char, as_i64, as_u32};
pub use parse::{ParseError, Parsed, SOH, Validation, parse_into, tag_text_at};
pub use template::{EncodeError, Template, TemplateBuilder};
pub use timestamp::{TIMESTAMP_LEN, TimestampCache};

/// `MessageView` is three words, not two.
///
/// `&[u8]` is a fat pointer (16 bytes) plus 8 for the index reference. Over 16
/// bytes means x86-64 SysV and AArch64 pass it indirectly, so hot-path functions
/// taking it by value carry `#[inline]`. If someone adds a field here, this stops
/// compiling rather than quietly costing a spill.
const _: () = assert!(core::mem::size_of::<MessageView<'static, 64>>() == 24);
const _: () = assert!(core::mem::size_of::<FieldEntry>() == 12);
const _: () = assert!(core::mem::align_of::<FieldEntry>() == 4);
