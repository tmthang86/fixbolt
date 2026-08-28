//! Repeating groups, read off the flat [`FieldIndex`](crate::FieldIndex).
//!
//! Nothing is built and nothing is allocated: a [`GroupIter`] is a pair of
//! positions into the index the parser already filled, and a [`GroupEntry`] is a
//! narrower pair. A message with no groups pays nothing, because none of this
//! runs unless it is called.
//!
//! # Where a group ends
//!
//! There is no end marker on the wire. A group ends at the first tag that is not
//! one of its members — and **a nested group's members are not members of the
//! group around it**. So a scan that does not step over nested regions stops
//! inside the first nested group and reports the outer one as one entry long.
//! `[measured 2026-08-28]` 235 of the 731 group positions in FIX 4.4 hold a
//! nested group, so that is 32% of them, not a corner. Guarded by
//! `a_nested_group_does_not_truncate_the_one_around_it` in `tests/groups.rs`.
//!
//! # What this does not decide
//!
//! [`GroupIter::declared`] is what the counter field says and
//! [`GroupIter::counted`] is what is actually there. They are reported
//! separately and never reconciled here. Whether a mismatch is a
//! `Reject 373=16` is the session layer's call, and it needs the counter tag to
//! put in `371=`, which it already has.

use core::marker::PhantomData;

use crate::dict::Dictionary;
use crate::index::{FieldEntry, MessageView, as_u32};

/// How deep group nesting may go before the scan stops descending.
///
/// `[measured 2026-08-28]` FIX 4.4's deepest chain is 4 —
/// `552 → 78 → 756 → 806` in TradeCaptureReport. The cap is not for FIX 4.4; it
/// bounds the recursion no matter what a future generated table says, so a
/// malformed dictionary cannot turn a parse into a stack overflow. Reaching it
/// ends the group rather than descending further, which under-reads rather than
/// over-reads.
pub(crate) const MAX_DEPTH: u8 = 8;

/// Entries of one repeating group, at one level.
///
/// Created by [`MessageView::group`] or [`GroupEntry::group`]. It **is** an
/// `Iterator`: the plan expected a lending one and it turned out not to be —
/// a [`GroupEntry`] borrows the message (`'a`), not the iterator, so the
/// standard trait fits and the caller gets `for`, `count` and the rest.
pub struct GroupIter<'a, D: Dictionary, const N: usize> {
    view: MessageView<'a, N>,
    msg_type: &'a [u8],
    delimiter: u32,
    members: &'static [u32],
    declared: Option<u32>,
    counted: u16,
    end: u16,
    pos: u16,
    _d: PhantomData<D>,
}

/// One entry of a repeating group: a window into the index.
#[derive(Clone, Copy)]
pub struct GroupEntry<'a, const N: usize> {
    view: MessageView<'a, N>,
    start: u16,
    end: u16,
}

impl<'a, D: Dictionary, const N: usize> GroupIter<'a, D, N> {
    /// The value of the counter field, or `None` when it is not a number.
    ///
    /// `Option` rather than a bare `u32` because "the count says 3 and there
    /// are 2" and "the count field is garbage" are different rejects, and a
    /// sentinel `0` would merge them with a legitimately empty group.
    #[inline]
    #[must_use]
    pub fn declared(&self) -> Option<u32> {
        self.declared
    }

    /// Entries actually found on the wire.
    #[inline]
    #[must_use]
    pub fn counted(&self) -> u32 {
        u32::from(self.counted)
    }

    #[inline]
    fn advance(&mut self) -> Option<GroupEntry<'a, N>> {
        let entries = self.view.entries();
        let from = usize::from(self.pos);
        let limit = usize::from(self.end);
        if from >= limit || entries.get(from).map(|e| e.tag) != Some(self.delimiter) {
            return None;
        }
        let end = entry_end::<D>(
            entries,
            self.msg_type,
            self.delimiter,
            self.members,
            from,
            limit,
            0,
        );
        self.pos = end as u16;
        Some(GroupEntry {
            view: self.view,
            start: from as u16,
            end: end as u16,
        })
    }
}

impl<'a, D: Dictionary, const N: usize> Iterator for GroupIter<'a, D, N> {
    type Item = GroupEntry<'a, N>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.advance()
    }

    /// Exact, and free: the entry count was computed when the group was opened.
    /// This is what is on the wire, which is not necessarily
    /// [`declared`](GroupIter::declared).
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(usize::from(self.counted)))
    }
}

impl<'a, const N: usize> GroupEntry<'a, N> {
    /// Value of the first field with this tag **inside this entry**.
    ///
    /// Fields of a nested group are inside this entry too, so a tag that a
    /// nested group also uses answers with whichever comes first. Reach the
    /// nested one through [`group`](Self::group) instead.
    #[inline]
    #[must_use]
    pub fn get(&self, tag: u32) -> Option<&'a [u8]> {
        let (start, end) = (usize::from(self.start), usize::from(self.end));
        (start..end).find_map(|i| match self.view.field_at(i) {
            Some((t, v)) if t == tag => Some(v),
            _ => None,
        })
    }

    /// A group nested inside this entry.
    #[inline]
    #[must_use]
    pub fn group<D: Dictionary>(
        &self,
        msg_type: &'a [u8],
        counter: u32,
    ) -> Option<GroupIter<'a, D, N>> {
        // Scoped to this entry, so reading NoAllocs off side 2 cannot answer
        // with side 1's — the flat index holds both.
        open::<D, N>(
            self.view,
            msg_type,
            counter,
            usize::from(self.start),
            usize::from(self.end),
            false,
        )
    }
}

impl<'a, const N: usize> MessageView<'a, N> {
    /// A **top-level** group of this message.
    ///
    /// Group regions are stepped over while searching, so asking a
    /// TradeCaptureReport for `NoAllocs(78)` — which exists only inside
    /// `NoSides(552)` — gives `None` rather than side 1's copy. Nested groups
    /// are reached through [`GroupEntry::group`].
    ///
    /// `None` also when the counter tag is absent, or when the dictionary does
    /// not declare a group under `(msg_type, counter)`. A counter present with
    /// value `0` is a group with no entries, not `None`.
    #[inline]
    #[must_use]
    pub fn group<D: Dictionary>(
        &self,
        msg_type: &'a [u8],
        counter: u32,
    ) -> Option<GroupIter<'a, D, N>> {
        open::<D, N>(*self, msg_type, counter, 0, self.len(), true)
    }
}

/// Finds `counter` in `[from, limit)` and scans the group it opens.
///
/// `top_level` steps over any group region met on the way, so a counter that
/// belongs to a group rather than to the message itself is not mistaken for one.
fn open<'a, D: Dictionary, const N: usize>(
    view: MessageView<'a, N>,
    msg_type: &'a [u8],
    counter: u32,
    from: usize,
    limit: usize,
    top_level: bool,
) -> Option<GroupIter<'a, D, N>> {
    let entries = view.entries();
    let delimiter = D::group_delimiter(msg_type, counter)?;
    let members = D::group_members(msg_type, counter);

    let mut i = from;
    let at = loop {
        if i >= limit {
            return None;
        }
        let tag = entries.get(i)?.tag;
        if tag == counter {
            break i;
        }
        i += 1;
        if top_level && let Some(d) = D::group_delimiter(msg_type, tag) {
            let m = D::group_members(msg_type, tag);
            i = region_end::<D>(entries, msg_type, d, m, i, limit, 1).0;
        }
    };

    let declared = view.field_at(at).and_then(|(_, v)| as_u32(v).ok());
    let start = at + 1;
    let (end, counted) = region_end::<D>(entries, msg_type, delimiter, members, start, limit, 0);
    Some(GroupIter {
        view,
        msg_type,
        delimiter,
        members,
        declared,
        counted,
        end: end as u16,
        pos: start as u16,
        _d: PhantomData,
    })
}

/// End of a run of entries starting at `from`, and how many there were.
fn region_end<D: Dictionary>(
    entries: &[FieldEntry],
    msg_type: &[u8],
    delimiter: u32,
    members: &[u32],
    from: usize,
    limit: usize,
    depth: u8,
) -> (usize, u16) {
    let mut i = from;
    let mut counted: u16 = 0;
    while i < limit && entries.get(i).map(|e| e.tag) == Some(delimiter) {
        i = entry_end::<D>(entries, msg_type, delimiter, members, i, limit, depth);
        counted = counted.saturating_add(1);
    }
    (i, counted)
}

/// End of the single entry that starts at `from`, where `entries[from]` is the
/// delimiter.
///
/// Stops at the next delimiter, or at the first tag outside `members`. A member
/// that is itself a group counter has its whole region stepped over, which is
/// the difference between reading a nested message and truncating it.
fn entry_end<D: Dictionary>(
    entries: &[FieldEntry],
    msg_type: &[u8],
    delimiter: u32,
    members: &[u32],
    from: usize,
    limit: usize,
    depth: u8,
) -> usize {
    let mut i = from + 1;
    while i < limit {
        let Some(tag) = entries.get(i).map(|e| e.tag) else {
            break;
        };
        if tag == delimiter || !members.contains(&tag) {
            break;
        }
        i += 1;
        if depth < MAX_DEPTH
            && let Some(d) = D::group_delimiter(msg_type, tag)
        {
            let m = D::group_members(msg_type, tag);
            i = region_end::<D>(entries, msg_type, d, m, i, limit, depth + 1).0;
        }
    }
    i
}
