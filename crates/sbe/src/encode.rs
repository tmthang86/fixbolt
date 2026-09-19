//! Writing SBE messages over the same tables the reader walks (plan step C4).
//!
//! Two ways in, both table-driven, neither allocating:
//!
//! - [`MessageWriter`], the native writer: the header and root block first,
//!   then groups (dimension, then per entry a block, its nested groups and its
//!   `varData`) and `varData` (length prefix, then the bytes), in schema order
//!   (SBE 1.0 RC4, "Sequence of message body elements"). Groups and entries are
//!   written inside closures — [`MessageWriter::group`], [`GroupWriter::entry`]
//!   — so that when a closure returns the writer knows the entry or group is
//!   done and can close it. That is what keeps every message this writer
//!   produces walkable without a stack on the heap: a group or `varData` the
//!   caller never wrote is written **empty** (count 0, length 0) when the
//!   writer passes it, and `numInGroup` is rewritten after each entry, so the
//!   count on the wire is always the number of entries written.
//! - [`SbeTemplate`], a flat message skeleton laid out once and filled per
//!   send from `(FieldId, bytes)` slots — the shape `codec::Encoding::encode`
//!   takes (D9). It writes the root block only; see its docs.
//!
//! **Byte order** is the schema's, for the header, every field, every
//! dimension and every length prefix (SBE 1.0 RC4, "Message header schema").
//!
//! **Initial block contents.** A block (root or entry) is zeroed, then every
//! optional element is set to its null value, before the caller writes into
//! it: a field the caller never sets reads back as absent when it is
//! optional and as zero when it is required.
//!
//! **After any `Err`** the bytes already written are unspecified; start the
//! message again. Nothing here panics or reads or writes out of bounds: every
//! access is a bounds-checked `get`/`get_mut` (CLAUDE.md §2 rule 7).

use core::marker::PhantomData;
use core::ops::Range;

use crate::error::SbeError;
use crate::header::{HEADER_LEN, MessageHeader};
use crate::schema::{
    Element, FieldLayout, GroupLayout, LengthType, MessageLayout, Presence, Primitive, Schema,
    Value, VarDataLayout,
};
use crate::wire::{self, ByteOrder};

// --- primitives ---------------------------------------------------------------

/// `len` bytes at `at`, mutably, or `BufferTooSmall`.
#[inline]
fn slice_mut(out: &mut [u8], at: usize, len: usize) -> Result<&mut [u8], SbeError> {
    let end = at.checked_add(len).ok_or(SbeError::BufferTooSmall)?;
    out.get_mut(at..end).ok_or(SbeError::BufferTooSmall)
}

/// Writes a length or count `v` of width `t`, or `LengthOverflow` when it
/// does not fit the width.
#[inline]
fn write_length(
    out: &mut [u8],
    at: usize,
    t: LengthType,
    v: usize,
    order: ByteOrder,
) -> Result<(), SbeError> {
    match t {
        LengthType::U8 => wire::write_u8(
            out,
            at,
            u8::try_from(v).map_err(|_| SbeError::LengthOverflow)?,
        ),
        LengthType::U16 => wire::write_u16(
            out,
            at,
            u16::try_from(v).map_err(|_| SbeError::LengthOverflow)?,
            order,
        ),
        LengthType::U32 => wire::write_u32(
            out,
            at,
            u32::try_from(v).map_err(|_| SbeError::LengthOverflow)?,
            order,
        ),
    }
}

/// Whether `v` fits a length or count of width `t`.
#[inline]
fn fits(t: LengthType, v: usize) -> bool {
    match t {
        LengthType::U8 => u8::try_from(v).is_ok(),
        LengthType::U16 => u16::try_from(v).is_ok(),
        LengthType::U32 => u32::try_from(v).is_ok(),
    }
}

/// Writes group `g`'s dimension at `at`: its schema `blockLength`, then
/// `count`.
#[inline]
fn write_dimension(
    out: &mut [u8],
    at: usize,
    g: &GroupLayout,
    count: usize,
    order: ByteOrder,
) -> Result<(), SbeError> {
    let d = g.dimension;
    write_length(out, at, d.block_length, usize::from(g.block_length), order)?;
    let count_at = at
        .checked_add(d.block_length.size())
        .ok_or(SbeError::BufferTooSmall)?;
    write_length(out, count_at, d.num_in_group, count, order)
}

/// Writes one scalar `v` of primitive `p` at `at` in `dst`.
fn write_scalar(
    dst: &mut [u8],
    at: usize,
    p: Primitive,
    v: Value<'_>,
    order: ByteOrder,
) -> Result<(), SbeError> {
    let bad = |_| SbeError::BadValue;
    match (p, v) {
        (Primitive::Char, Value::Char(c)) => wire::write_u8(dst, at, c),
        (Primitive::Int8, Value::Int(i)) => wire::write_u8(
            dst,
            at,
            u8::from_ne_bytes(i8::try_from(i).map_err(bad)?.to_ne_bytes()),
        ),
        (Primitive::Int16, Value::Int(i)) => {
            wire::write_i16(dst, at, i16::try_from(i).map_err(bad)?, order)
        }
        (Primitive::Int32, Value::Int(i)) => {
            wire::write_i32(dst, at, i32::try_from(i).map_err(bad)?, order)
        }
        (Primitive::Int64, Value::Int(i)) => wire::write_i64(dst, at, i, order),
        (Primitive::UInt8, Value::UInt(u)) => {
            wire::write_u8(dst, at, u8::try_from(u).map_err(bad)?)
        }
        (Primitive::UInt16, Value::UInt(u)) => {
            wire::write_u16(dst, at, u16::try_from(u).map_err(bad)?, order)
        }
        (Primitive::UInt32, Value::UInt(u)) => {
            wire::write_u32(dst, at, u32::try_from(u).map_err(bad)?, order)
        }
        (Primitive::UInt64, Value::UInt(u)) => wire::write_u64(dst, at, u, order),
        // The reader widens `float` to `f64`; narrowing a value it produced
        // gives back the same `f32` bit for bit (NaN stays NaN, ±inf stays
        // ±inf). A finite value past `f32::MAX` has no `f32`: `as` would
        // write ±inf, a different value, so it is refused
        // (`a_finite_double_too_big_for_a_float_is_bad_value`).
        (Primitive::Float, Value::Float(f)) => {
            if f.is_finite() && f.abs() > f64::from(f32::MAX) {
                return Err(SbeError::BadValue);
            }
            wire::write_f32(dst, at, f as f32, order)
        }
        (Primitive::Double, Value::Float(f)) => wire::write_f64(dst, at, f, order),
        _ => Err(SbeError::BadValue),
    }
}

/// Writes element `e` of field `f` into `block` — `None` meaning the
/// element's null value.
fn write_element(
    block: &mut [u8],
    f: &FieldLayout,
    e: &Element,
    value: Option<Value<'_>>,
    order: ByteOrder,
) -> Result<(), SbeError> {
    let null = match e.presence {
        // Not on the wire. Accept the schema's own value (what the reader
        // hands back) or nothing; anything else would be silently dropped.
        Presence::Constant(c) => {
            return match value {
                None => Ok(()),
                Some(v) if v == c => Ok(()),
                Some(_) => Err(SbeError::BadValue),
            };
        }
        Presence::Required => None,
        Presence::Optional { null } => Some(null),
    };
    let field = slice_mut(block, usize::from(f.offset), usize::from(f.len))
        .map_err(|_| SbeError::FieldOutsideBlock)?;
    let at = usize::from(e.offset);
    let outside = |err| match err {
        SbeError::BufferTooSmall => SbeError::FieldOutsideBlock,
        other => other,
    };
    if e.length == 1 {
        let v = value.or(null).ok_or(SbeError::BadValue)?;
        return write_scalar(field, at, e.primitive, v, order).map_err(outside);
    }
    let n = e.wire_len();
    match (value, null) {
        (Some(Value::Array(b)), _) if b.len() == n => wire::write_slice(field, at, b),
        (Some(_), _) => Err(SbeError::BadValue),
        // A char array is null when every byte is the null char.
        (None, Some(Value::Char(c))) => wire::fill(field, at, n, c),
        (None, _) => Err(SbeError::BadValue),
    }
    .map_err(outside)
}

/// Zeroes the block at `start`, then sets every optional element to null.
fn init_block(
    out: &mut [u8],
    start: usize,
    len: usize,
    fields: &[FieldLayout],
    order: ByteOrder,
) -> Result<(), SbeError> {
    let block = slice_mut(out, start, len)?;
    block.fill(0);
    for f in fields {
        for e in f.elements {
            if matches!(e.presence, Presence::Optional { .. }) {
                write_element(block, f, e, None, order)?;
            }
        }
    }
    Ok(())
}

/// The block of `len` bytes at `start` of `out`, for field writes.
#[derive(Debug, Clone, Copy)]
struct BlockAt {
    start: usize,
    len: usize,
}

impl BlockAt {
    fn put_bytes(&self, out: &mut [u8], field: &FieldLayout, bytes: &[u8]) -> Result<(), SbeError> {
        if bytes.len() != usize::from(field.len) {
            return Err(SbeError::WrongLength);
        }
        let block = slice_mut(out, self.start, self.len)?;
        wire::write_slice(block, usize::from(field.offset), bytes)
            .map_err(|_| SbeError::FieldOutsideBlock)
    }

    fn put_element(
        &self,
        out: &mut [u8],
        field: &FieldLayout,
        index: usize,
        value: Option<Value<'_>>,
        order: ByteOrder,
    ) -> Result<(), SbeError> {
        let e = field.elements.get(index).ok_or(SbeError::NoSuchElement)?;
        let block = slice_mut(out, self.start, self.len)?;
        write_element(block, field, e, value, order)
    }
}

// --- the tail: groups then varData, in schema order ---------------------------

/// Progress through the groups and `varData` that follow one block.
#[derive(Debug, Clone, Copy)]
struct Tail {
    groups: &'static [GroupLayout],
    var_data: &'static [VarDataLayout],
    next_group: usize,
    next_var: usize,
}

impl Tail {
    const fn new(groups: &'static [GroupLayout], var_data: &'static [VarDataLayout]) -> Self {
        Self {
            groups,
            var_data,
            next_group: 0,
            next_var: 0,
        }
    }

    /// Writes an empty group for every group not yet written before `upto`.
    fn empty_groups_until(
        &mut self,
        out: &mut [u8],
        pos: &mut usize,
        order: ByteOrder,
        upto: usize,
    ) -> Result<(), SbeError> {
        while self.next_group < upto {
            let g = self
                .groups
                .get(self.next_group)
                .ok_or(SbeError::UnknownField)?;
            write_dimension(out, *pos, g, 0, order)?;
            *pos += g.dimension.size();
            self.next_group += 1;
        }
        Ok(())
    }

    /// Writes an empty `varData` for every one not yet written before `upto`.
    fn empty_var_data_until(
        &mut self,
        out: &mut [u8],
        pos: &mut usize,
        order: ByteOrder,
        upto: usize,
    ) -> Result<(), SbeError> {
        while self.next_var < upto {
            let v = self
                .var_data
                .get(self.next_var)
                .ok_or(SbeError::UnknownField)?;
            write_length(out, *pos, v.length, 0, order)?;
            *pos += v.length.size();
            self.next_var += 1;
        }
        Ok(())
    }

    /// Writes the dimension of group `id` (count 0 for now), after closing
    /// every earlier group, and returns its layout and where its count sits.
    fn open_group(
        &mut self,
        out: &mut [u8],
        pos: &mut usize,
        order: ByteOrder,
        id: u16,
    ) -> Result<(&'static GroupLayout, usize), SbeError> {
        let idx = self
            .groups
            .iter()
            .position(|g| g.id == id)
            .ok_or(SbeError::UnknownField)?;
        if idx < self.next_group || self.next_var > 0 {
            return Err(SbeError::OutOfOrder);
        }
        self.empty_groups_until(out, pos, order, idx)?;
        let g = self.groups.get(idx).ok_or(SbeError::UnknownField)?;
        write_dimension(out, *pos, g, 0, order)?;
        let count_at = *pos + g.dimension.block_length.size();
        *pos += g.dimension.size();
        self.next_group = idx + 1;
        Ok((g, count_at))
    }

    /// Writes `varData` `id`: every group not yet written (empty), every
    /// earlier `varData` not yet written (empty), then its length and bytes.
    fn var_data(
        &mut self,
        out: &mut [u8],
        pos: &mut usize,
        order: ByteOrder,
        id: u16,
        bytes: &[u8],
    ) -> Result<(), SbeError> {
        let idx = self
            .var_data
            .iter()
            .position(|v| v.id == id)
            .ok_or(SbeError::UnknownField)?;
        if idx < self.next_var {
            return Err(SbeError::OutOfOrder);
        }
        let v = self.var_data.get(idx).ok_or(SbeError::UnknownField)?;
        if !fits(v.length, bytes.len()) {
            return Err(SbeError::LengthOverflow);
        }
        self.empty_groups_until(out, pos, order, self.groups.len())?;
        self.empty_var_data_until(out, pos, order, idx)?;
        write_length(out, *pos, v.length, bytes.len(), order)?;
        let start = *pos + v.length.size();
        wire::write_slice(out, start, bytes)?;
        *pos = start + bytes.len();
        self.next_var = idx + 1;
        Ok(())
    }

    /// Writes everything not yet written, empty.
    fn close(&mut self, out: &mut [u8], pos: &mut usize, order: ByteOrder) -> Result<(), SbeError> {
        self.empty_groups_until(out, pos, order, self.groups.len())?;
        self.empty_var_data_until(out, pos, order, self.var_data.len())
    }
}

// --- the native writer ----------------------------------------------------------

/// Writes one SBE message into a caller's buffer: header, root block, then
/// groups and `varData` in schema order.
///
/// ```
/// # use fixbolt_sbe::{ByteOrder, Element, FieldLayout, MessageLayout, MessageWriter,
/// #     Presence, Primitive, Schema, SbeError, SbeView, Value};
/// static QTY: FieldLayout = FieldLayout {
///     id: 38, name: "Qty", offset: 0, len: 4, since_version: 0,
///     elements: &[Element { name: "", offset: 0, primitive: Primitive::UInt32,
///         length: 1, presence: Presence::Required }],
/// };
/// static MSG: MessageLayout = MessageLayout {
///     template_id: 1, name: "M", block_length: 4, since_version: 0,
///     fields: core::slice::from_ref(&QTY), groups: &[], var_data: &[],
/// };
/// struct S;
/// impl Schema for S {
///     const ID: u16 = 7;
///     const VERSION: u16 = 0;
///     const BYTE_ORDER: ByteOrder = ByteOrder::Little;
///     fn message(id: u16) -> Option<&'static MessageLayout> { (id == 1).then_some(&MSG) }
/// }
///
/// let mut out = [0u8; 64];
/// let mut w = MessageWriter::new::<S>(&mut out, 1)?;
/// w.put(&QTY, Value::UInt(100))?;
/// let len = w.finish()?;
/// assert_eq!(len, 12);
/// let view = SbeView::decode::<S>(&out[..len])?;
/// assert_eq!(view.root::<S>()?.value(&QTY)?, Some(Value::UInt(100)));
/// # Ok::<(), SbeError>(())
/// ```
#[derive(Debug)]
pub struct MessageWriter<'b> {
    out: &'b mut [u8],
    /// End of what has been written so far.
    pos: usize,
    order: ByteOrder,
    layout: &'static MessageLayout,
    tail: Tail,
}

impl<'b> MessageWriter<'b> {
    /// Starts message `template_id` of schema `S` at the front of `out`:
    /// writes the header (`blockLength` from the layout, `schemaId` `S::ID`,
    /// `version` `S::VERSION`) and the initial root block.
    ///
    /// # Errors
    /// `UnknownTemplate` when `S` has no such message; `BufferTooSmall` when
    /// `out` cannot hold the header and root block; `FieldOutsideBlock` when a
    /// field of the table reaches past its block (a table defect).
    pub fn new<S: Schema>(out: &'b mut [u8], template_id: u16) -> Result<Self, SbeError> {
        let layout = S::message(template_id).ok_or(SbeError::UnknownTemplate)?;
        let order = S::BYTE_ORDER;
        MessageHeader {
            block_length: layout.block_length,
            template_id,
            schema_id: S::ID,
            version: S::VERSION,
        }
        .encode(out, order)?;
        let len = usize::from(layout.block_length);
        init_block(out, HEADER_LEN, len, layout.fields, order)?;
        Ok(Self {
            out,
            pos: HEADER_LEN + len,
            order,
            layout,
            tail: Tail::new(layout.groups, layout.var_data),
        })
    }

    /// The layout of the message being written.
    #[must_use]
    #[inline]
    pub fn layout(&self) -> &'static MessageLayout {
        self.layout
    }

    /// Bytes written so far, header included.
    #[must_use]
    #[inline]
    pub fn position(&self) -> usize {
        self.pos
    }

    fn root(&self) -> BlockAt {
        BlockAt {
            start: HEADER_LEN,
            len: usize::from(self.layout.block_length),
        }
    }

    /// Copies `bytes` — the field's wire bytes, in the schema's byte order —
    /// into root field `field`. May be called at any time before
    /// [`MessageWriter::finish`]: the root block's position never moves.
    ///
    /// `field` is trusted to be a field of this message, as on the read side
    /// ([`crate::Block::field`]); a wrong one writes where it says, never
    /// outside the root block.
    ///
    /// # Errors
    /// `WrongLength` when `bytes` is not `field.len` long;
    /// `FieldOutsideBlock` when the field reaches past the root block.
    #[inline]
    pub fn put_bytes(&mut self, field: &FieldLayout, bytes: &[u8]) -> Result<(), SbeError> {
        self.root().put_bytes(self.out, field, bytes)
    }

    /// Writes `value` as element 0 of root field `field` — the whole value of
    /// a simple field.
    ///
    /// # Errors
    /// As [`MessageWriter::put_element`].
    #[inline]
    pub fn put(&mut self, field: &FieldLayout, value: Value<'_>) -> Result<(), SbeError> {
        self.put_element(field, 0, Some(value))
    }

    /// Writes element `index` of root field `field`; `None` writes the
    /// element's null value. The inverse of [`crate::FieldRef::element`]: what
    /// that returns, this accepts, constants included.
    ///
    /// # Errors
    /// `NoSuchElement` for an index past the table; `BadValue` when the value
    /// does not fit the element (see [`SbeError::BadValue`]);
    /// `FieldOutsideBlock` when it reaches past the root block.
    #[inline]
    pub fn put_element(
        &mut self,
        field: &FieldLayout,
        index: usize,
        value: Option<Value<'_>>,
    ) -> Result<(), SbeError> {
        self.root()
            .put_element(self.out, field, index, value, self.order)
    }

    /// Writes root group `layout`, entry by entry, through `f`. Every root
    /// group before it that was not written is written empty first.
    ///
    /// # Errors
    /// `UnknownField` when the message has no group with `layout.id`;
    /// `OutOfOrder` when that group, a later one, or any `varData` was
    /// already written; `BufferTooSmall`; and whatever `f` returns.
    pub fn group<F>(&mut self, layout: &GroupLayout, f: F) -> Result<(), SbeError>
    where
        F: FnOnce(&mut GroupWriter<'_>) -> Result<(), SbeError>,
    {
        let (g, count_at) = self
            .tail
            .open_group(self.out, &mut self.pos, self.order, layout.id)?;
        f(&mut GroupWriter {
            out: &mut *self.out,
            pos: &mut self.pos,
            order: self.order,
            layout: g,
            count_at,
            count: 0,
        })
    }

    /// Writes root `varData` `layout`: its length prefix, then `bytes`. Every
    /// root group, and every earlier root `varData`, not yet written is
    /// written empty first.
    ///
    /// # Errors
    /// `UnknownField`, `OutOfOrder` as [`MessageWriter::group`];
    /// `LengthOverflow` when `bytes` is longer than the prefix can say;
    /// `BufferTooSmall`.
    pub fn var_data(&mut self, layout: &VarDataLayout, bytes: &[u8]) -> Result<(), SbeError> {
        self.tail
            .var_data(self.out, &mut self.pos, self.order, layout.id, bytes)
    }

    /// Closes the message — every root group and `varData` not yet written
    /// is written empty — and returns its length: the message is
    /// `out[..len]`.
    ///
    /// # Errors
    /// `BufferTooSmall`.
    pub fn finish(mut self) -> Result<usize, SbeError> {
        self.tail.close(self.out, &mut self.pos, self.order)?;
        Ok(self.pos)
    }
}

/// One repeating group being written; entries are added by
/// [`GroupWriter::entry`].
#[derive(Debug)]
pub struct GroupWriter<'w> {
    out: &'w mut [u8],
    pos: &'w mut usize,
    order: ByteOrder,
    layout: &'static GroupLayout,
    count_at: usize,
    count: usize,
}

impl GroupWriter<'_> {
    /// The group's layout.
    #[must_use]
    #[inline]
    pub fn layout(&self) -> &'static GroupLayout {
        self.layout
    }

    /// Entries written so far — the `numInGroup` on the wire.
    #[must_use]
    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }

    /// Appends one entry: writes its initial block, runs `f` on it, then
    /// closes the entry's nested groups and `varData` (empty where `f` wrote
    /// none) and rewrites `numInGroup`.
    ///
    /// # Errors
    /// `LengthOverflow` when one more entry does not fit `numInGroup`'s width;
    /// `BufferTooSmall`; and whatever `f` returns. On any error the entry is
    /// undone: the write position goes back to where the entry started and
    /// the count on the wire does not include it, so the group — and the
    /// message — can still be continued and finished as if the entry had
    /// never been attempted (bytes past the position are left as written and
    /// are overwritten by whatever comes next). Guarded by
    /// `a_failed_entry_is_rewound_and_the_message_still_walks` in
    /// `tests/encoding.rs`.
    pub fn entry<F>(&mut self, f: F) -> Result<(), SbeError>
    where
        F: FnOnce(&mut EntryWriter<'_>) -> Result<(), SbeError>,
    {
        let start = *self.pos;
        let r = self.write_entry(start, f);
        if r.is_err() {
            *self.pos = start;
        }
        r
    }

    fn write_entry<F>(&mut self, start: usize, f: F) -> Result<(), SbeError>
    where
        F: FnOnce(&mut EntryWriter<'_>) -> Result<(), SbeError>,
    {
        let next = self.count.checked_add(1).ok_or(SbeError::LengthOverflow)?;
        if !fits(self.layout.dimension.num_in_group, next) {
            return Err(SbeError::LengthOverflow);
        }
        let len = usize::from(self.layout.block_length);
        init_block(self.out, start, len, self.layout.fields, self.order)?;
        *self.pos = start + len;
        let mut e = EntryWriter {
            out: &mut *self.out,
            pos: &mut *self.pos,
            order: self.order,
            block: BlockAt { start, len },
            tail: Tail::new(self.layout.groups, self.layout.var_data),
        };
        f(&mut e)?;
        e.tail.close(e.out, e.pos, e.order)?;
        write_length(
            self.out,
            self.count_at,
            self.layout.dimension.num_in_group,
            next,
            self.order,
        )?;
        self.count = next;
        Ok(())
    }
}

/// One group entry being written: its block, then its nested groups and
/// `varData`, with the same rules as the root's.
#[derive(Debug)]
pub struct EntryWriter<'w> {
    out: &'w mut [u8],
    pos: &'w mut usize,
    order: ByteOrder,
    block: BlockAt,
    tail: Tail,
}

impl EntryWriter<'_> {
    /// As [`MessageWriter::put_bytes`], on this entry's block.
    ///
    /// # Errors
    /// As [`MessageWriter::put_bytes`].
    #[inline]
    pub fn put_bytes(&mut self, field: &FieldLayout, bytes: &[u8]) -> Result<(), SbeError> {
        self.block.put_bytes(self.out, field, bytes)
    }

    /// As [`MessageWriter::put`], on this entry's block.
    ///
    /// # Errors
    /// As [`MessageWriter::put_element`].
    #[inline]
    pub fn put(&mut self, field: &FieldLayout, value: Value<'_>) -> Result<(), SbeError> {
        self.put_element(field, 0, Some(value))
    }

    /// As [`MessageWriter::put_element`], on this entry's block.
    ///
    /// # Errors
    /// As [`MessageWriter::put_element`].
    #[inline]
    pub fn put_element(
        &mut self,
        field: &FieldLayout,
        index: usize,
        value: Option<Value<'_>>,
    ) -> Result<(), SbeError> {
        self.block
            .put_element(self.out, field, index, value, self.order)
    }

    /// As [`MessageWriter::group`], for a group nested in this entry.
    ///
    /// # Errors
    /// As [`MessageWriter::group`].
    pub fn group<F>(&mut self, layout: &GroupLayout, f: F) -> Result<(), SbeError>
    where
        F: FnOnce(&mut GroupWriter<'_>) -> Result<(), SbeError>,
    {
        let (g, count_at) = self
            .tail
            .open_group(self.out, self.pos, self.order, layout.id)?;
        f(&mut GroupWriter {
            out: &mut *self.out,
            pos: &mut *self.pos,
            order: self.order,
            layout: g,
            count_at,
            count: 0,
        })
    }

    /// As [`MessageWriter::var_data`], for this entry's `varData`.
    ///
    /// # Errors
    /// As [`MessageWriter::var_data`].
    pub fn var_data(&mut self, layout: &VarDataLayout, bytes: &[u8]) -> Result<(), SbeError> {
        self.tail
            .var_data(self.out, self.pos, self.order, layout.id, bytes)
    }
}

// --- the flat template ----------------------------------------------------------

/// A root-block field of an SBE message, named by its schema `id` (by
/// convention the FIX tag). The `Field` of `Sbe<S>`'s `codec::Encoding` impl.
///
/// Root-block fields only: a field inside a group entry has no single
/// position in the message, so it cannot be named by id alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(pub u16);

/// A **flat** SBE message laid out once — header, root block with its null
/// and preset values, and every group (count 0) and `varData` (length 0) the
/// template has, written empty — and copied then patched per send (D9).
///
/// `N` bounds the skeleton's bytes, the caller's choice like
/// `codec::Template`'s `S`. `S` is the schema.
///
/// **What it cannot write.** Only root-block fields are slots. A message whose
/// groups or `varData` must carry something is written with
/// [`MessageWriter`]; through a template those are always empty on the wire.
pub struct SbeTemplate<S, const N: usize> {
    bytes: [u8; N],
    len: usize,
    layout: &'static MessageLayout,
    schema: PhantomData<fn() -> S>,
}

impl<S, const N: usize> core::fmt::Debug for SbeTemplate<S, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SbeTemplate")
            .field("template_id", &self.layout.template_id)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl<S, const N: usize> Clone for SbeTemplate<S, N> {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes,
            len: self.len,
            layout: self.layout,
            schema: PhantomData,
        }
    }
}

impl<S: Schema, const N: usize> SbeTemplate<S, N> {
    /// Lays out message `template_id` of `S`.
    ///
    /// # Errors
    /// `UnknownTemplate` when `S` has no such message; `BufferTooSmall` when
    /// the skeleton does not fit `N` bytes.
    pub fn new(template_id: u16) -> Result<Self, SbeError> {
        let mut bytes = [0u8; N];
        let w = MessageWriter::new::<S>(&mut bytes, template_id)?;
        let layout = w.layout();
        let len = w.finish()?;
        Ok(Self {
            bytes,
            len,
            layout,
            schema: PhantomData,
        })
    }

    /// The layout of the templated message.
    #[must_use]
    #[inline]
    pub fn layout(&self) -> &'static MessageLayout {
        self.layout
    }

    /// Length of the message on the wire, header included.
    #[must_use]
    #[inline]
    pub fn wire_len(&self) -> usize {
        self.len
    }

    /// The skeleton as it stands: what [`SbeTemplate::encode`] writes when
    /// given no slots.
    #[must_use]
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or_default()
    }

    fn root(&self) -> BlockAt {
        BlockAt {
            start: HEADER_LEN,
            len: usize::from(self.layout.block_length),
        }
    }

    fn field(&self, f: FieldId) -> Result<&'static FieldLayout, SbeError> {
        self.layout.field(f.0).ok_or(SbeError::UnknownField)
    }

    /// Presets root field `f` to `bytes` (its wire bytes) in the skeleton —
    /// a value every message from this template carries.
    ///
    /// # Errors
    /// `UnknownField`, `WrongLength`, `FieldOutsideBlock`.
    pub fn set_bytes(&mut self, f: FieldId, bytes: &[u8]) -> Result<(), SbeError> {
        let field = self.field(f)?;
        self.root().put_bytes(&mut self.bytes, field, bytes)
    }

    /// Presets element `index` of root field `f`; `None` is null. As
    /// [`MessageWriter::put_element`].
    ///
    /// # Errors
    /// `UnknownField`, and as [`MessageWriter::put_element`].
    pub fn set_element(
        &mut self,
        f: FieldId,
        index: usize,
        value: Option<Value<'_>>,
    ) -> Result<(), SbeError> {
        let field = self.field(f)?;
        self.root()
            .put_element(&mut self.bytes, field, index, value, S::BYTE_ORDER)
    }

    /// Copies the skeleton to the front of `out` and writes each slot's bytes
    /// — the field's wire bytes, in the schema's byte order — at its field's
    /// offset. Returns the range of `out` the message occupies, `0..len`.
    ///
    /// Every slot is checked before anything is written, so a bad slot never
    /// leaves a half-written message in `out`. A field given twice takes the
    /// later value.
    ///
    /// # Errors
    /// `UnknownField` for an id that is not a root field; `WrongLength` when
    /// a slot's bytes are not its field's length; `FieldOutsideBlock` when a
    /// field reaches past the root block (a table defect); `BufferTooSmall`
    /// when `out` is shorter than the message.
    pub fn encode(
        &self,
        out: &mut [u8],
        slots: &[(FieldId, &[u8])],
    ) -> Result<Range<usize>, SbeError> {
        let block_len = usize::from(self.layout.block_length);
        for &(id, bytes) in slots {
            let f = self.field(id)?;
            if bytes.len() != usize::from(f.len) {
                return Err(SbeError::WrongLength);
            }
            if usize::from(f.offset) + usize::from(f.len) > block_len {
                return Err(SbeError::FieldOutsideBlock);
            }
        }
        if out.len() < self.len {
            return Err(SbeError::BufferTooSmall);
        }
        wire::write_slice(out, 0, self.as_bytes())?;
        let root = self.root();
        for &(id, bytes) in slots {
            root.put_bytes(out, self.field(id)?, bytes)?;
        }
        Ok(0..self.len)
    }
}
