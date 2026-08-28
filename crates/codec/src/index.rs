//! Where the fields are, kept apart from the bytes they point into.
//!
//! [ADR-0003](../../../docs/decisions/ADR-0003-message-representation.md) measured
//! the reason: a 512-entry array living inside the returned struct made parsing
//! 4-6x slower. The index is owned by the caller, reused per connection, and
//! never returned by value.

use crate::parse::ParseError;

/// One field: its tag, where its value starts, and how long that value is.
///
/// 12 bytes at natural alignment 4 — not `align(16)`, which would waste a
/// quarter of every cache line. `len` is `u16`: a longer value is
/// [`ParseError::FieldTooLong`], never a silent wrap.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldEntry {
    /// The FIX tag number.
    pub tag: u32,
    /// Offset of the value's first byte, from the start of the buffer.
    pub offset: u32,
    /// Length of the value in bytes. May be 0 — an empty value is a field, not
    /// an error. See D12 and `14d`.
    pub len: u16,
    _pad: u16,
}

impl FieldEntry {
    const EMPTY: Self = Self {
        tag: 0,
        offset: 0,
        len: 0,
        _pad: 0,
    };
}

/// A reusable index over one message. No lifetime, so it can live in a
/// connection struct and be re-filled on every read.
///
/// `N` is the caller's choice: 64 for order flow, 512 for a market-data
/// snapshot, same code and no runtime cost. Overflow is
/// [`ParseError::TooManyFields`], never truncation.
pub struct FieldIndex<const N: usize> {
    count: u16,
    fields: [FieldEntry; N],
}

impl<const N: usize> Default for FieldIndex<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> FieldIndex<N> {
    /// An empty index. `const`, so it can be embedded in a static or a
    /// pre-faulted pool.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: 0,
            fields: [FieldEntry::EMPTY; N],
        }
    }

    /// Number of fields recorded.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.count as usize
    }

    /// Whether no field has been recorded.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Forget every field. The parser calls this before it writes anything, so
    /// a failed parse never leaves a half-filled index that looks valid.
    #[inline]
    pub fn clear(&mut self) {
        self.count = 0;
    }

    /// The recorded fields, in wire order.
    #[inline]
    #[must_use]
    pub fn entries(&self) -> &[FieldEntry] {
        &self.fields[..self.count as usize]
    }

    /// Borrow `buf` through this index.
    ///
    /// The borrow checker does the safety here: `parse_into` takes `&mut self`
    /// and a view takes `&self`, so an index cannot be re-filled while a view
    /// onto it is alive.
    #[inline]
    #[must_use]
    pub fn view<'a>(&'a self, buf: &'a [u8]) -> MessageView<'a, N> {
        MessageView { buf, idx: self }
    }

    #[inline]
    pub(crate) fn push(&mut self, tag: u32, offset: usize, len: usize) -> Result<(), ParseError> {
        if len > u16::MAX as usize {
            return Err(ParseError::FieldTooLong(tag));
        }
        let slot = self
            .fields
            .get_mut(self.count as usize)
            .ok_or(ParseError::TooManyFields)?;
        *slot = FieldEntry {
            tag,
            // A buffer longer than u32::MAX is not a FIX read buffer.
            offset: offset as u32,
            len: len as u16,
            _pad: 0,
        };
        self.count += 1;
        Ok(())
    }

    #[inline]
    pub(crate) fn last(&self) -> Option<FieldEntry> {
        self.count
            .checked_sub(1)
            .and_then(|i| self.fields.get(i as usize))
            .copied()
    }
}

/// A parsed message: the caller's bytes plus the index into them.
///
/// 24 bytes — `&[u8]` is a fat pointer (16) plus 8 for the index reference.
/// Over 16 bytes means x86-64 SysV and AArch64 pass it **indirectly**, so
/// anything taking it by value on the hot path is `#[inline]`.
#[derive(Clone, Copy)]
pub struct MessageView<'a, const N: usize> {
    buf: &'a [u8],
    idx: &'a FieldIndex<N>,
}

impl<'a, const N: usize> MessageView<'a, N> {
    /// Value of the first field with this tag.
    ///
    /// Linear scan. For the 15-30 fields a FIX message carries, a scan beats a
    /// map and needs neither allocation nor hashing.
    #[inline]
    #[must_use]
    pub fn get(&self, tag: u32) -> Option<&'a [u8]> {
        self.idx
            .entries()
            .iter()
            .find(|e| e.tag == tag)
            .and_then(|e| self.value_of(e))
    }

    /// Value of the first field with this tag at or after `from`, with its
    /// position. The primitive repeating groups will be built on.
    #[inline]
    #[must_use]
    pub fn find_from(&self, from: usize, tag: u32) -> Option<(usize, &'a [u8])> {
        let entries = self.idx.entries();
        entries
            .iter()
            .enumerate()
            .skip(from)
            .find(|(_, e)| e.tag == tag)
            .and_then(|(i, e)| self.value_of(e).map(|v| (i, v)))
    }

    /// The recorded fields. Crate-internal: the public surface is `get`,
    /// `find_from` and `field_at`, and group scanning needs the raw tags.
    #[inline]
    pub(crate) fn entries(&self) -> &'a [FieldEntry] {
        self.idx.entries()
    }

    /// The `i`th field in wire order.
    #[inline]
    #[must_use]
    pub fn field_at(&self, i: usize) -> Option<(u32, &'a [u8])> {
        let e = self.idx.entries().get(i)?;
        self.value_of(e).map(|v| (e.tag, v))
    }

    /// How many fields the message has.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.idx.len()
    }

    /// Whether the message has no fields.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    #[inline]
    fn value_of(&self, e: &FieldEntry) -> Option<&'a [u8]> {
        let start = e.offset as usize;
        self.buf.get(start..start + e.len as usize)
    }
}

/// A value could not be read as the requested type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvertError {
    /// Empty, or a byte that is not a digit where one was required.
    NotANumber,
    /// The digits are a number, but not one that fits.
    Overflow,
    /// Asked for one byte and the value was not exactly one byte long.
    NotOneByte,
}

/// Read an unsigned decimal. No sign, no whitespace, no panic.
#[inline]
pub fn as_u32(value: &[u8]) -> Result<u32, ConvertError> {
    if value.is_empty() {
        return Err(ConvertError::NotANumber);
    }
    let mut n: u32 = 0;
    for &b in value {
        let d = b.wrapping_sub(b'0');
        if d > 9 {
            return Err(ConvertError::NotANumber);
        }
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_add(u32::from(d)))
            .ok_or(ConvertError::Overflow)?;
    }
    Ok(n)
}

/// Read a signed decimal.
#[inline]
pub fn as_i64(value: &[u8]) -> Result<i64, ConvertError> {
    let (neg, digits) = match value.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, value),
    };
    if digits.is_empty() {
        return Err(ConvertError::NotANumber);
    }
    let mut n: i64 = 0;
    for &b in digits {
        let d = b.wrapping_sub(b'0');
        if d > 9 {
            return Err(ConvertError::NotANumber);
        }
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_sub(i64::from(d)))
            .ok_or(ConvertError::Overflow)?;
    }
    if neg {
        Ok(n)
    } else {
        n.checked_neg().ok_or(ConvertError::Overflow)
    }
}

/// Read a one-byte CHAR value, such as `54=1`.
#[inline]
pub fn as_char(value: &[u8]) -> Result<u8, ConvertError> {
    match value {
        [b] => Ok(*b),
        _ => Err(ConvertError::NotOneByte),
    }
}
