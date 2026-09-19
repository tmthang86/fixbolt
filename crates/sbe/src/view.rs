//! [`SbeView`], the borrowed view of one SBE message, and [`Block`], a fixed
//! block (the root block or one group entry) that fields are read from.

use crate::error::SbeError;
use crate::group::Cursor;
use crate::header::{HEADER_LEN, MessageHeader};
use crate::schema::{Element, FieldLayout, MessageLayout, Presence, Primitive, Schema, Value};
use crate::wire::{self, ByteOrder};

/// One SBE message, borrowed from the caller's buffer: 24 bytes, `Copy`.
///
/// `buf` starts at the message header. `block_offset` is where the root block
/// starts inside `buf` — the header length, [`HEADER_LEN`], for the only header
/// this crate supports. The header's `blockLength` is **not** cached: the three
/// identity fields and the offset fill the 8 bytes beside the fat pointer, and
/// the view is pinned at 24 bytes (ADR-0081 decision 1, the size of
/// `MessageView`). It is re-read from `buf` — one bounds-checked 2-byte load —
/// by [`SbeView::root`] and [`SbeView::tail`], the only two places that need it.
///
/// Construction checks that `buf` holds the whole root block, so a truncated
/// root is an error at [`SbeView::new`], not at the first field read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbeView<'a> {
    buf: &'a [u8],
    block_offset: u16,
    template_id: u16,
    schema_id: u16,
    version: u16,
}

/// Pinned: a field added here stops the build rather than quietly pushing the
/// view past three words (the same pin as `fixbolt_codec::MessageView`).
const _: () = assert!(core::mem::size_of::<SbeView<'static>>() == 24);

impl<'a> SbeView<'a> {
    /// Decodes the header at the start of `buf` in byte order `order`.
    ///
    /// Accepts any template and any schema id: an unknown template is still
    /// delimited by `blockLength` (see [`SbeView::root_end`]), and which schema
    /// the caller decodes with is checked by [`SbeView::layout`].
    ///
    /// # Errors
    /// `Truncated` when `buf` is shorter than the header plus the root block
    /// length the header announces.
    #[inline]
    pub fn new(buf: &'a [u8], order: ByteOrder) -> Result<Self, SbeError> {
        let h = MessageHeader::decode(buf, order)?;
        wire::slice(buf, HEADER_LEN, usize::from(h.block_length))?;
        Ok(Self {
            buf,
            // HEADER_LEN is 8; the cast is to the field's width, not a narrowing.
            block_offset: HEADER_LEN as u16,
            template_id: h.template_id,
            schema_id: h.schema_id,
            version: h.version,
        })
    }

    /// [`SbeView::new`] in `S`'s byte order.
    ///
    /// # Errors
    /// As [`SbeView::new`].
    #[inline]
    pub fn decode<S: Schema>(buf: &'a [u8]) -> Result<Self, SbeError> {
        Self::new(buf, S::BYTE_ORDER)
    }

    /// The header's `templateId`.
    #[must_use]
    #[inline]
    pub fn template_id(&self) -> u16 {
        self.template_id
    }

    /// The header's `schemaId`.
    #[must_use]
    #[inline]
    pub fn schema_id(&self) -> u16 {
        self.schema_id
    }

    /// The header's `version`: which schema version encoded the message.
    #[must_use]
    #[inline]
    pub fn version(&self) -> u16 {
        self.version
    }

    /// Where the root block starts in [`SbeView::bytes`].
    #[must_use]
    #[inline]
    pub fn block_offset(&self) -> usize {
        usize::from(self.block_offset)
    }

    /// The buffer the view was built on, from the header onwards.
    #[must_use]
    #[inline]
    pub fn bytes(&self) -> &'a [u8] {
        self.buf
    }

    /// The header's `blockLength`, read from the buffer in `order`.
    ///
    /// # Errors
    /// `Truncated` only if `buf` no longer holds a header, which a view built
    /// by [`SbeView::new`] always does.
    #[inline]
    pub fn block_length(&self, order: ByteOrder) -> Result<u16, SbeError> {
        wire::read_u16(self.buf, 0, order)
    }

    /// Offset in [`SbeView::bytes`] of the first byte after the root block:
    /// where the first group or `varData` starts, and — for a message with
    /// neither, or one whose template is unknown — where the next thing on the
    /// transport starts. This is how an unknown template is skipped by
    /// `blockLength` (ADR-0081 decision 4); what follows the root block of an
    /// unknown template cannot be walked without its layout, so a framed
    /// transport (SOFH) must say where the message ends.
    ///
    /// # Errors
    /// As [`SbeView::block_length`].
    #[inline]
    pub fn root_end(&self, order: ByteOrder) -> Result<usize, SbeError> {
        let len = usize::from(self.block_length(order)?);
        self.block_offset()
            .checked_add(len)
            .ok_or(SbeError::Truncated)
    }

    /// The layout of this message in schema `S`.
    ///
    /// # Errors
    /// `WrongSchema` when the header's `schemaId` is not `S::ID`;
    /// `UnknownTemplate` when `S` has no such template.
    #[inline]
    pub fn layout<S: Schema>(&self) -> Result<&'static MessageLayout, SbeError> {
        if self.schema_id != S::ID {
            return Err(SbeError::WrongSchema);
        }
        S::message(self.template_id).ok_or(SbeError::UnknownTemplate)
    }

    /// The root block, exactly as long as the header's `blockLength` says —
    /// longer than `S`'s layout when the encoder's schema is newer, and the
    /// extra bytes are never read (SBE 1.0 RC4 §5, "Block size").
    ///
    /// # Errors
    /// `Truncated` when the buffer does not hold the block, which a view built
    /// by [`SbeView::new`] rules out.
    #[inline]
    pub fn root<S: Schema>(&self) -> Result<Block<'a>, SbeError> {
        let len = usize::from(self.block_length(S::BYTE_ORDER)?);
        Ok(Block {
            bytes: wire::slice(self.buf, self.block_offset(), len)?,
            order: S::BYTE_ORDER,
            version: self.version,
        })
    }

    /// A cursor at the first byte after the root block, where the root's
    /// groups and then its `varData` are walked in schema order.
    ///
    /// # Errors
    /// As [`SbeView::root_end`].
    #[inline]
    pub fn tail<S: Schema>(&self) -> Result<Cursor<'a>, SbeError> {
        Ok(Cursor::new(
            self.buf,
            self.root_end(S::BYTE_ORDER)?,
            S::BYTE_ORDER,
            self.version,
        ))
    }
}

/// A fixed-length block: the root block of a message or one group entry. Its
/// length is the one the wire announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block<'a> {
    bytes: &'a [u8],
    order: ByteOrder,
    version: u16,
}

impl<'a> Block<'a> {
    pub(crate) fn new(bytes: &'a [u8], order: ByteOrder, version: u16) -> Self {
        Self {
            bytes,
            order,
            version,
        }
    }

    /// The block's bytes.
    #[must_use]
    #[inline]
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The field `field` of this block, or `None` when the message's version
    /// predates it (`sinceVersion` > header `version`, ADR-0081 decision 4).
    ///
    /// # Errors
    /// `FieldOutsideBlock` when the field's version says the encoder knew it
    /// but the block on the wire is too short to hold it.
    #[inline]
    pub fn field(&self, field: &'static FieldLayout) -> Result<Option<FieldRef<'a>>, SbeError> {
        if field.since_version > self.version {
            return Ok(None);
        }
        // A field made only of constants has no wire bytes and no position to
        // check: its values come from the table.
        let bytes = if field.len == 0 {
            &[][..]
        } else {
            wire::slice(
                self.bytes,
                usize::from(field.offset),
                usize::from(field.len),
            )
            .map_err(|_| SbeError::FieldOutsideBlock)?
        };
        Ok(Some(FieldRef {
            bytes,
            layout: field,
            order: self.order,
        }))
    }

    /// The first element of `field` — the whole value of a simple field.
    /// `None` when the field is absent (version) or null (optional).
    ///
    /// # Errors
    /// As [`Block::field`] and [`FieldRef::element`].
    #[inline]
    pub fn value(&self, field: &'static FieldLayout) -> Result<Option<Value<'a>>, SbeError> {
        match self.field(field)? {
            Some(f) => f.element(0),
            None => Ok(None),
        }
    }
}

/// One field inside a block: its wire bytes and its layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldRef<'a> {
    bytes: &'a [u8],
    layout: &'static FieldLayout,
    order: ByteOrder,
}

impl<'a> FieldRef<'a> {
    /// The field's bytes on the wire (`len` bytes from its offset).
    #[must_use]
    #[inline]
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The field's layout.
    #[must_use]
    #[inline]
    pub fn layout(&self) -> &'static FieldLayout {
        self.layout
    }

    /// Element `index` of the field (0 for a simple field). `None` when it is
    /// optional and holds its null value.
    ///
    /// # Errors
    /// `NoSuchElement` for an index past the table; `FieldOutsideBlock` when
    /// the element reaches past the field's `len` (a table defect).
    #[inline]
    pub fn element(&self, index: usize) -> Result<Option<Value<'a>>, SbeError> {
        let e = self
            .layout
            .elements
            .get(index)
            .ok_or(SbeError::NoSuchElement)?;
        read_element(self.bytes, e, self.order)
    }

    /// The element named `name` (a composite member's dotted path). A linear
    /// scan; `NoSuchElement` when absent from the table.
    ///
    /// # Errors
    /// As [`FieldRef::element`].
    #[inline]
    pub fn member(&self, name: &str) -> Result<Option<Value<'a>>, SbeError> {
        let e = self.layout.element(name).ok_or(SbeError::NoSuchElement)?;
        read_element(self.bytes, e, self.order)
    }
}

/// Reads element `e` of a field whose wire bytes are `field`.
fn read_element<'a>(
    field: &'a [u8],
    e: &Element,
    order: ByteOrder,
) -> Result<Option<Value<'a>>, SbeError> {
    let null = match e.presence {
        Presence::Constant(v) => return Ok(Some(v)),
        Presence::Required => None,
        Presence::Optional { null } => Some(null),
    };
    let at = usize::from(e.offset);
    let v = if e.length == 1 {
        read_scalar(field, at, e.primitive, order)
    } else {
        wire::slice(field, at, e.wire_len()).map(Value::Array)
    }
    .map_err(|_| SbeError::FieldOutsideBlock)?;
    Ok(match null {
        Some(n) if is_null(v, n) => None,
        _ => Some(v),
    })
}

fn read_scalar(
    b: &[u8],
    at: usize,
    p: Primitive,
    order: ByteOrder,
) -> Result<Value<'static>, SbeError> {
    Ok(match p {
        Primitive::Char => Value::Char(wire::read_u8(b, at)?),
        Primitive::Int8 => Value::Int(i64::from(i8::from_ne_bytes([wire::read_u8(b, at)?]))),
        Primitive::Int16 => Value::Int(i64::from(wire::read_i16(b, at, order)?)),
        Primitive::Int32 => Value::Int(i64::from(wire::read_i32(b, at, order)?)),
        Primitive::Int64 => Value::Int(wire::read_i64(b, at, order)?),
        Primitive::UInt8 => Value::UInt(u64::from(wire::read_u8(b, at)?)),
        Primitive::UInt16 => Value::UInt(u64::from(wire::read_u16(b, at, order)?)),
        Primitive::UInt32 => Value::UInt(u64::from(wire::read_u32(b, at, order)?)),
        Primitive::UInt64 => Value::UInt(wire::read_u64(b, at, order)?),
        Primitive::Float => Value::Float(f64::from(wire::read_f32(b, at, order)?)),
        Primitive::Double => Value::Float(wire::read_f64(b, at, order)?),
    })
}

/// Whether wire value `v` is the null value `null` (see [`Presence::Optional`]).
fn is_null(v: Value<'_>, null: Value<'static>) -> bool {
    match (v, null) {
        (Value::Float(a), Value::Float(n)) => (a.is_nan() && n.is_nan()) || a == n,
        (Value::Array(bytes), Value::Char(c)) => bytes.iter().all(|&b| b == c),
        (Value::Array(_), _) => false,
        (a, n) => a == n,
    }
}
