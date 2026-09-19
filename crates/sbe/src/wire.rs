//! Bounds-checked primitive reads and writes in either byte order.
//!
//! Every function here takes the slice and an offset and returns `Err` rather
//! than indexing: `a[i..j]` panics, and CLAUDE.md §2 rule 7 forbids a panic in
//! a library crate (`indexing_slicing = "deny"`).

use crate::error::SbeError;

/// The byte order a schema declares in `<messageSchema byteOrder=…>`. The
/// message header and every length prefix use the same order as the body
/// (SBE 1.0 RC4, "Message header schema").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    /// `byteOrder="littleEndian"`, the SBE default.
    Little,
    /// `byteOrder="bigEndian"`.
    Big,
}

/// `N` bytes at `at`, or `Truncated`.
#[inline]
pub(crate) fn array<const N: usize>(buf: &[u8], at: usize) -> Result<[u8; N], SbeError> {
    let end = at.checked_add(N).ok_or(SbeError::Truncated)?;
    let bytes = buf.get(at..end).ok_or(SbeError::Truncated)?;
    <[u8; N]>::try_from(bytes).map_err(|_| SbeError::Truncated)
}

/// `len` bytes at `at`, or `Truncated`.
#[inline]
pub(crate) fn slice(buf: &[u8], at: usize, len: usize) -> Result<&[u8], SbeError> {
    let end = at.checked_add(len).ok_or(SbeError::Truncated)?;
    buf.get(at..end).ok_or(SbeError::Truncated)
}

macro_rules! read_fn {
    ($name:ident, $t:ty, $n:literal) => {
        #[inline]
        pub(crate) fn $name(buf: &[u8], at: usize, order: ByteOrder) -> Result<$t, SbeError> {
            let b = array::<$n>(buf, at)?;
            Ok(match order {
                ByteOrder::Little => <$t>::from_le_bytes(b),
                ByteOrder::Big => <$t>::from_be_bytes(b),
            })
        }
    };
}

read_fn!(read_u16, u16, 2);
read_fn!(read_u32, u32, 4);
read_fn!(read_u64, u64, 8);
read_fn!(read_i16, i16, 2);
read_fn!(read_i32, i32, 4);
read_fn!(read_i64, i64, 8);
read_fn!(read_f32, f32, 4);
read_fn!(read_f64, f64, 8);

#[inline]
pub(crate) fn read_u8(buf: &[u8], at: usize) -> Result<u8, SbeError> {
    buf.get(at).copied().ok_or(SbeError::Truncated)
}

/// Writes `v` at `at` in `order`, or `BufferTooSmall`.
#[inline]
pub(crate) fn write_u16(
    out: &mut [u8],
    at: usize,
    v: u16,
    order: ByteOrder,
) -> Result<(), SbeError> {
    let end = at.checked_add(2).ok_or(SbeError::BufferTooSmall)?;
    let dst = out.get_mut(at..end).ok_or(SbeError::BufferTooSmall)?;
    let src = match order {
        ByteOrder::Little => v.to_le_bytes(),
        ByteOrder::Big => v.to_be_bytes(),
    };
    dst.copy_from_slice(&src);
    Ok(())
}
