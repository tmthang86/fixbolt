//! Walking what follows a fixed block: groups (over their dimension composite),
//! nested groups, and — in `vardata.rs` — `varData`.
//!
//! SBE puts a block's groups and then its `varData` after the block, in schema
//! order, and nested groups depth first inside each entry (SBE 1.0 RC4 "Nested repeating group wire
//! format", "Sequence of message body elements"). Nothing after the first group has a fixed position, so the reader is
//! a cursor that moves forward: [`Cursor::group`] consumes the cursor and
//! returns a [`Group`]; [`Group::finish`] walks what is left of it and hands
//! the cursor back, positioned after the group.

use crate::error::SbeError;
use crate::schema::{GroupLayout, LengthType, VarDataLayout};
use crate::view::Block;
use crate::wire::{self, ByteOrder};

/// A position in a message after a fixed block, where groups and `varData`
/// are read in schema order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor<'a> {
    pub(crate) buf: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) order: ByteOrder,
    pub(crate) version: u16,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(buf: &'a [u8], pos: usize, order: ByteOrder, version: u16) -> Self {
        Self {
            buf,
            pos,
            order,
            version,
        }
    }

    /// Offset of the cursor in the message buffer. After the last group and
    /// `varData` of the root, this is the message's length on the wire.
    #[must_use]
    #[inline]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Opens group `layout` at the cursor.
    ///
    /// When the message's version predates the group (`sinceVersion` > header
    /// `version`) the group is not on the wire: the returned group is absent,
    /// has no entries, and [`Group::finish`] returns the cursor unmoved.
    ///
    /// # Errors
    /// `Truncated` when the dimension is cut off.
    #[inline]
    pub fn group(self, layout: &'static GroupLayout) -> Result<Group<'a>, SbeError> {
        if layout.since_version > self.version {
            return Ok(Group {
                cur: self,
                layout,
                block_length: 0,
                count: 0,
                remaining: 0,
                tail_pending: false,
                present: false,
            });
        }
        let d = layout.dimension;
        let block_length = read_length(self.buf, self.pos, d.block_length, self.order)?;
        let count = read_length(
            self.buf,
            self.pos
                .checked_add(d.block_length.size())
                .ok_or(SbeError::Truncated)?,
            d.num_in_group,
            self.order,
        )?;
        let pos = self.pos.checked_add(d.size()).ok_or(SbeError::Truncated)?;
        Ok(Group {
            cur: Cursor { pos, ..self },
            layout,
            block_length,
            count,
            remaining: count,
            tail_pending: false,
            present: true,
        })
    }

    /// Walks past group `layout` without reading it.
    ///
    /// # Errors
    /// As [`Cursor::group`] and [`Group::finish`].
    #[inline]
    pub fn skip_group(self, layout: &'static GroupLayout) -> Result<Cursor<'a>, SbeError> {
        self.group(layout)?.finish()
    }

    /// Walks past the nested groups and `varData` of a block, per its layout.
    pub(crate) fn skip_tail(
        mut self,
        groups: &'static [GroupLayout],
        var_data: &'static [VarDataLayout],
    ) -> Result<Cursor<'a>, SbeError> {
        for g in groups {
            self = self.skip_group(g)?;
        }
        for v in var_data {
            self = self.var_data(v)?.1;
        }
        Ok(self)
    }
}

/// Reads a length or count of width `t`, widened to `usize`.
pub(crate) fn read_length(
    buf: &[u8],
    at: usize,
    t: LengthType,
    order: ByteOrder,
) -> Result<usize, SbeError> {
    let v = match t {
        LengthType::U8 => u32::from(wire::read_u8(buf, at)?),
        LengthType::U16 => u32::from(wire::read_u16(buf, at, order)?),
        LengthType::U32 => wire::read_u32(buf, at, order)?,
    };
    // Only fails where usize is 16 bits and the value does not fit; then the
    // buffer cannot hold that much either.
    usize::try_from(v).map_err(|_| SbeError::Truncated)
}

/// One repeating group being walked, entry by entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Group<'a> {
    /// At the next entry's block, or — while `tail_pending` — at the tail of
    /// the entry last returned.
    cur: Cursor<'a>,
    layout: &'static GroupLayout,
    block_length: usize,
    count: usize,
    remaining: usize,
    tail_pending: bool,
    present: bool,
}

impl<'a> Group<'a> {
    /// `numInGroup` from the wire; 0 for an absent group.
    #[must_use]
    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }

    /// The entry block length from the wire — the one the reader uses, which
    /// may exceed the layout's when the encoder's schema is newer.
    #[must_use]
    #[inline]
    pub fn block_length(&self) -> usize {
        self.block_length
    }

    /// `false` when the message's version predates the group.
    #[must_use]
    #[inline]
    pub fn is_present(&self) -> bool {
        self.present
    }

    /// The group's layout.
    #[must_use]
    #[inline]
    pub fn layout(&self) -> &'static GroupLayout {
        self.layout
    }

    /// The next entry, or `None` after the last.
    ///
    /// The entry's nested groups and `varData` sit between its block and the
    /// next entry. The caller may walk them from [`Entry::tail`]; whether it
    /// does or not, this call first walks past them by the layout, so entries
    /// are never misaligned. An entry with no nested groups and no `varData`
    /// costs nothing extra.
    ///
    /// # Errors
    /// `Truncated` when an entry block, a nested dimension or a `varData` is
    /// cut off.
    #[inline]
    pub fn next_entry(&mut self) -> Result<Option<Entry<'a>>, SbeError> {
        self.skip_pending_tail()?;
        if self.remaining == 0 {
            return Ok(None);
        }
        let bytes = wire::slice(self.cur.buf, self.cur.pos, self.block_length)?;
        self.remaining -= 1;
        self.cur.pos += self.block_length;
        self.tail_pending = self.has_tail();
        Ok(Some(Entry {
            block: Block::new(bytes, self.cur.order, self.cur.version),
            tail: self.cur,
        }))
    }

    /// Walks past every entry not yet read and returns the cursor after the
    /// group, where the next group or `varData` of the enclosing block starts.
    ///
    /// # Errors
    /// As [`Group::next_entry`].
    #[inline]
    pub fn finish(mut self) -> Result<Cursor<'a>, SbeError> {
        self.skip_pending_tail()?;
        if self.has_tail() {
            while self.next_entry()?.is_some() {}
            self.skip_pending_tail()?;
        } else {
            // Fixed-size entries: skip them all in one bounds-checked step.
            let skip = self
                .block_length
                .checked_mul(self.remaining)
                .ok_or(SbeError::Truncated)?;
            wire::slice(self.cur.buf, self.cur.pos, skip)?;
            self.cur.pos += skip;
            self.remaining = 0;
        }
        Ok(self.cur)
    }

    /// Whether an entry has anything after its block (nested groups, `varData`).
    fn has_tail(&self) -> bool {
        !self.layout.groups.is_empty() || !self.layout.var_data.is_empty()
    }

    fn skip_pending_tail(&mut self) -> Result<(), SbeError> {
        if self.tail_pending {
            self.cur = self
                .cur
                .skip_tail(self.layout.groups, self.layout.var_data)?;
            self.tail_pending = false;
        }
        Ok(())
    }
}

/// One entry of a group: its fixed block, and a cursor at its nested groups
/// and `varData`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry<'a> {
    block: Block<'a>,
    tail: Cursor<'a>,
}

impl<'a> Entry<'a> {
    /// The entry's fixed block, where its fields are read.
    #[must_use]
    #[inline]
    pub fn block(&self) -> Block<'a> {
        self.block
    }

    /// A cursor at the entry's nested groups, then its `varData`.
    #[must_use]
    #[inline]
    pub fn tail(&self) -> Cursor<'a> {
        self.tail
    }
}
