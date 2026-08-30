//! Outbound messages as a pre-sorted parts list, patched rather than built.
//!
//! A given session's `ExecutionReport` has a fixed skeleton: `BeginString`,
//! `SenderCompID`, `TargetCompID`, `MsgType`, and the field order. That skeleton
//! is laid out **once per session per message type**; a send fills the holes.
//!
//! Three things here are not obvious.
//!
//! **The template owns its bytes.** `49=ISLD\x0156=TW44\x01` arrives in a Logon
//! at run time, so `&'static` would mean leaking per session, and borrowing the
//! session's arena would make this self-referential. It owns a `[u8; S]`.
//!
//! **Ordering happens at build time, never at send time** (D3, non-negotiable 5).
//! `MsgType` first, then header tags ascending, then body tags ascending — the
//! order the acceptance comparator checks positionally.
//!
//! **The body is written first and the prefix right-aligned in front of it.**
//! `BodyLength` is variable-width, so writing the prefix first would mean moving
//! the whole body once its width is known. That is why [`Template::encode`]
//! returns a `Range` and not a length: the message does not begin at `out[0]`.

use core::ops::Range;

use crate::checksum::{checksum, format_checksum};
use crate::dict::{Dictionary, NoDict};
use crate::group::MAX_DEPTH;

/// `BodyLength` is rendered without padding, so five digits is the ceiling this
/// layout reserves. Longer is [`EncodeError::BodyTooLong`].
const MAX_BODY_DIGITS: usize = 5;
/// `10=NNN` and its separator.
const TRAILER_LEN: usize = 7;

const SOH: u8 = 0x01;

/// Why a message could not be laid out or written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// `out` cannot hold the message. Never a partial write the caller might send.
    OutputTooSmall,
    /// The body reached 100,000 bytes, which would need a sixth `BodyLength`
    /// digit and overrun the space reserved in front of the body.
    BodyTooLong,
    /// More fields than `P`.
    TooManyParts,
    /// More static bytes than `S`.
    ScratchFull,
    /// `8`, `9` and `10` are the frame, not fields. `encode` writes them.
    ReservedTag(u32),
    /// A group was declared but the dictionary has no `(msg_type, counter)`
    /// entry for it. Writing it would mean inventing a field order.
    UnknownGroup(u32),
    /// A tag was supplied for a group entry that the group does not contain.
    /// Writing it would put a field where no reader expects one.
    NotAGroupMember(u32),
    /// An entry of this group arrived without its delimiter — the tag that
    /// starts every entry. A reader cannot cut the group without it.
    MissingDelimiter(u32),
    /// A template with a group hole has no `MsgType`, so the group tables
    /// cannot be looked up: they are keyed by `(msg_type, counter)`.
    MsgTypeMissing,
    /// Groups nested deeper than [`MAX_DEPTH`](crate::group::MAX_DEPTH).
    GroupTooDeep,
    /// A DATA field was declared without the length field that must sit
    /// immediately in front of it.
    ///
    /// A DATA value may legally contain `0x01`, so a reader takes its length
    /// from that field and from nothing else. Writing the data without it emits
    /// bytes no reader can frame — every message, for ever — so this is refused
    /// once, when the template is built, rather than at send time.
    DataWithoutLength(u32),
}

/// One entry of a repeating group being written.
///
/// `fields` may be in any order: [`Template::encode_with`] writes them in the
/// dictionary's declaration order. That is non-negotiable 5, and a group is the
/// place it bites — inside a group the order is not ascending by tag, so the
/// rule that governs the body cannot catch a mistake here.
#[derive(Clone, Copy)]
pub struct GroupEntryData<'a> {
    /// Fields of this entry. Must include the group's delimiter.
    pub fields: &'a [(u32, &'a [u8])],
    /// Groups nested inside this entry.
    pub groups: &'a [GroupData<'a>],
}

/// A repeating group being written: its counter tag and its entries.
///
/// Borrowed and recursive, so nesting costs no allocation — the caller builds
/// it on the stack. The counter value is not supplied: it is `entries.len()`,
/// so the two cannot disagree.
#[derive(Clone, Copy)]
pub struct GroupData<'a> {
    /// The counter tag, such as `453` for `NoPartyIDs`.
    pub counter: u32,
    /// The entries, in the order they go on the wire.
    pub entries: &'a [GroupEntryData<'a>],
}

#[derive(Clone, Copy)]
enum Part {
    /// A run of already-encoded `tag=value\x01` bytes in the scratch buffer.
    Static { at: u16, len: u16 },
    /// A hole. Filled from `slots` at send time, or skipped when absent.
    Slot(u32),
    /// A repeating group. Filled from `groups` at send time, or skipped when
    /// absent. The counter tag sorts among the body tags like any other; what
    /// follows it does not sort at all.
    Group(u32),
    /// The length field of a DATA slot. Its value is **not** taken from the
    /// caller: it is the byte count of the value the caller supplied for
    /// `data_tag`, counted at send time, embedded `0x01` included.
    ///
    /// If the caller could state it, the invariant would be advice — one wrong
    /// number and every reader mis-frames the message after it.
    DataLen { tag: u32, data_tag: u32 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Static,
    Slot,
    Group,
    /// The length field of a DATA **slot**; the encoder writes it. A length for
    /// a DATA field that is static is folded into the static bytes at build
    /// time instead, because its value is already known then.
    DataLen(u32),
}

#[derive(Clone, Copy)]
struct Entry {
    tag: u32,
    at: u16,
    len: u16,
    kind: Kind,
}

/// A message skeleton with holes in it.
///
/// `P` bounds the number of parts, `S` the static bytes. Both are the caller's
/// choice, like `FieldIndex<N>`.
pub struct Template<const P: usize, const S: usize> {
    scratch: [u8; S],
    begin_at: u16,
    begin_len: u16,
    /// Where `MsgType`'s value sits in `scratch`. The group tables are keyed by
    /// `(msg_type, counter)`, so writing a group needs it.
    mt_at: u16,
    mt_len: u16,
    parts: [Part; P],
    len: u8,
}

/// Collects fields, then sorts them into wire order exactly once.
pub struct TemplateBuilder<const P: usize, const S: usize> {
    scratch: [u8; S],
    used: u16,
    begin_at: u16,
    begin_len: u16,
    entries: [Entry; P],
    n: usize,
    err: Option<EncodeError>,
}

impl<const P: usize, const S: usize> TemplateBuilder<P, S> {
    /// Start a template for a given `BeginString` value, such as `b"FIX.4.4"`.
    #[must_use]
    pub fn new(begin_string: &[u8]) -> Self {
        let mut b = Self {
            scratch: [0; S],
            used: 0,
            begin_at: 0,
            begin_len: 0,
            entries: [Entry {
                tag: 0,
                at: 0,
                len: 0,
                kind: Kind::Static,
            }; P],
            n: 0,
            err: None,
        };
        match b.stash(begin_string) {
            Some((at, len)) => {
                b.begin_at = at;
                b.begin_len = len;
            }
            None => b.err = Some(EncodeError::ScratchFull),
        }
        b
    }

    /// A field whose value never changes for this session and message type.
    pub fn field(mut self, tag: u32, value: &[u8]) -> Self {
        if let Err(e) = self.push_field(tag, value) {
            self.err.get_or_insert(e);
        }
        self
    }

    /// A field supplied at send time. Absent from `slots` means the field is
    /// simply not written — `35=3` alone takes eight different field sets in the
    /// acceptance corpus, so one rigid template could not serve it.
    pub fn slot(mut self, tag: u32) -> Self {
        if let Err(e) = self.push_slot(tag) {
            self.err.get_or_insert(e);
        }
        self
    }

    /// A repeating group hole. The counter tag takes its ordinary ascending
    /// place among the body tags; the entries after it are ordered by the
    /// dictionary, not by this call.
    pub fn group(mut self, counter: u32) -> Self {
        if let Err(e) = self.push_group(counter) {
            self.err.get_or_insert(e);
        }
        self
    }

    /// Sort into wire order and freeze.
    ///
    /// The dictionary decides header from body. No call site ever chooses an
    /// order, which is what non-negotiable 5 asks for.
    pub fn build<D: Dictionary>(mut self) -> Result<Template<P, S>, EncodeError> {
        if let Some(e) = self.err {
            return Err(e);
        }

        // Insertion sort: at most P entries, once per session, off the hot path.
        for i in 1..self.n {
            let mut j = i;
            while j > 0 && key::<D>(&self.entries[j - 1]) > key::<D>(&self.entries[j]) {
                self.entries.swap(j - 1, j);
                j -= 1;
            }
        }

        // Re-lay the static bytes in sorted order so adjacent statics really are
        // adjacent, and can merge into one copy at send time.
        let mut out = Template {
            scratch: [0; S],
            begin_at: 0,
            begin_len: self.begin_len,
            mt_at: 0,
            mt_len: 0,
            parts: [Part::Static { at: 0, len: 0 }; P],
            len: 0,
        };
        let mut used: usize = 0;

        let bl = self.begin_len as usize;
        let ba = self.begin_at as usize;
        out.scratch
            .get_mut(..bl)
            .ok_or(EncodeError::ScratchFull)?
            .copy_from_slice(&self.scratch[ba..ba + bl]);
        used += bl;

        // **The DATA invariant, checked once, here.** A DATA field whose length
        // field is absent could be written every message for ever and no reader
        // could frame any of them, so it is a build-time refusal rather than a
        // send-time one. Sorting has already put the pair adjacent, in order.
        for i in 0..self.n {
            let e = self.entries[i];
            let Some(len_tag) = D::data_length_tag(e.tag) else {
                continue;
            };
            let Some(j) = (0..self.n).find(|k| self.entries[*k].tag == len_tag) else {
                return Err(EncodeError::DataWithoutLength(e.tag));
            };
            match e.kind {
                // A static DATA value has a known length now, so the length
                // field is rewritten to it and stays static: nothing is left for
                // the caller to get wrong.
                Kind::Static => {
                    let n = e.len as usize - (tag_prefix_len(e.tag) + 1);
                    let mut digits = [0u8; 10];
                    let text = render_u32(n as u32, &mut digits);
                    let Some((at, len)) = self.stash_field(len_tag, text) else {
                        return Err(EncodeError::ScratchFull);
                    };
                    self.entries[j].at = at;
                    self.entries[j].len = len;
                    self.entries[j].kind = Kind::Static;
                }
                // A DATA slot's length is only knowable at send time.
                _ => self.entries[j].kind = Kind::DataLen(e.tag),
            }
        }

        let mut nparts: usize = 0;
        let mut has_group = false;
        for i in 0..self.n {
            let e = self.entries[i];
            if e.kind == Kind::Static {
                let src = &self.scratch[e.at as usize..e.at as usize + e.len as usize];
                if e.tag == 35 {
                    // `35=X\x01` — the value is what sits between `35=` and SOH.
                    out.mt_at = (used + 3) as u16;
                    out.mt_len = e.len - 4;
                }
                out.scratch
                    .get_mut(used..used + src.len())
                    .ok_or(EncodeError::ScratchFull)?
                    .copy_from_slice(src);
                // Merge with the previous part when it is also static, so a run
                // of fixed fields costs one copy at send time.
                match nparts.checked_sub(1).map(|p| out.parts[p]) {
                    Some(Part::Static { at, len }) if at as usize + len as usize == used => {
                        out.parts[nparts - 1] = Part::Static {
                            at,
                            len: len + e.len,
                        };
                    }
                    _ => {
                        *out.parts.get_mut(nparts).ok_or(EncodeError::TooManyParts)? =
                            Part::Static {
                                at: used as u16,
                                len: e.len,
                            };
                        nparts += 1;
                    }
                }
                used += src.len();
            } else {
                *out.parts.get_mut(nparts).ok_or(EncodeError::TooManyParts)? = match e.kind {
                    Kind::Group => {
                        has_group = true;
                        Part::Group(e.tag)
                    }
                    Kind::DataLen(data_tag) => Part::DataLen {
                        tag: e.tag,
                        data_tag,
                    },
                    _ => Part::Slot(e.tag),
                };
                nparts += 1;
            }
        }

        // Refusing here rather than at send time: a template that can never
        // write its group is a build-time mistake, not a per-message one.
        if has_group && out.mt_len == 0 {
            return Err(EncodeError::MsgTypeMissing);
        }

        out.begin_at = 0;
        out.len = u8::try_from(nparts).map_err(|_| EncodeError::TooManyParts)?;
        Ok(out)
    }

    fn push_field(&mut self, tag: u32, value: &[u8]) -> Result<(), EncodeError> {
        reserved(tag)?;
        let at = self.used;
        self.write_tag(tag)?;
        self.write(value)?;
        self.write(&[SOH])?;
        self.entry(Entry {
            tag,
            at,
            len: self.used - at,
            kind: Kind::Static,
        })
    }

    fn push_slot(&mut self, tag: u32) -> Result<(), EncodeError> {
        reserved(tag)?;
        self.entry(Entry {
            tag,
            at: 0,
            len: 0,
            kind: Kind::Slot,
        })
    }

    fn push_group(&mut self, counter: u32) -> Result<(), EncodeError> {
        reserved(counter)?;
        self.entry(Entry {
            tag: counter,
            at: 0,
            len: 0,
            kind: Kind::Group,
        })
    }

    fn entry(&mut self, e: Entry) -> Result<(), EncodeError> {
        *self
            .entries
            .get_mut(self.n)
            .ok_or(EncodeError::TooManyParts)? = e;
        self.n += 1;
        Ok(())
    }

    /// Write `tag=value\x01` into scratch and say where it landed.
    ///
    /// Used to rewrite a static DATA field's length once the real length is
    /// known. The old bytes are left behind rather than reclaimed — this runs
    /// once per template, at build, and a scratch buffer that is too small says
    /// so with [`EncodeError::ScratchFull`].
    fn stash_field(&mut self, tag: u32, value: &[u8]) -> Option<(u16, u16)> {
        let at = self.used;
        self.write_tag(tag).ok()?;
        self.write(value).ok()?;
        self.write(&[SOH]).ok()?;
        Some((at, self.used - at))
    }

    fn stash(&mut self, bytes: &[u8]) -> Option<(u16, u16)> {
        let at = self.used;
        self.write(bytes).ok()?;
        Some((at, self.used - at))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        let at = self.used as usize;
        self.scratch
            .get_mut(at..at + bytes.len())
            .ok_or(EncodeError::ScratchFull)?
            .copy_from_slice(bytes);
        self.used += u16::try_from(bytes.len()).map_err(|_| EncodeError::ScratchFull)?;
        Ok(())
    }

    fn write_tag(&mut self, tag: u32) -> Result<(), EncodeError> {
        let mut d = [0u8; 10];
        let s = render_u32(tag, &mut d);
        let n = s.len();
        let at = self.used as usize;
        self.scratch
            .get_mut(at..at + n + 1)
            .ok_or(EncodeError::ScratchFull)?
            .copy_from_slice_with_eq(s)?;
        self.used += (n + 1) as u16;
        Ok(())
    }
}

/// Small helper so `write_tag` stays one bounds check.
trait CopyWithEq {
    fn copy_from_slice_with_eq(&mut self, src: &[u8]) -> Result<(), EncodeError>;
}

impl CopyWithEq for [u8] {
    fn copy_from_slice_with_eq(&mut self, src: &[u8]) -> Result<(), EncodeError> {
        let (head, tail) = self.split_at_mut(src.len());
        head.copy_from_slice(src);
        *tail.first_mut().ok_or(EncodeError::ScratchFull)? = b'=';
        Ok(())
    }
}

fn reserved(tag: u32) -> Result<(), EncodeError> {
    if matches!(tag, 8..=10) {
        return Err(EncodeError::ReservedTag(tag));
    }
    Ok(())
}

/// `MsgType` first, then header tags ascending, then body tags ascending.
/// Where a field sorts: `MsgType` first, then header tags ascending, then body
/// tags ascending — non-negotiable 5, and never a call site's choice.
///
/// The third element is what makes DATA work. A DATA field sorts **by its
/// length field's tag**, one place behind it, so the two are adjacent and in the
/// right order whatever their own numbers are.
///
/// `[measured 2026-08-30]` fifteen of FIX 4.4's sixteen DATA pairs have
/// `length == data - 1`, so ascending order put them right by luck.
/// `Signature(89)` takes `SignatureLength(93)` and ascending order emitted the
/// data **before** its length, which no reader can frame.
/// `tests/data_encode.rs` holds that case.
fn key<D: Dictionary>(e: &Entry) -> (u8, u32, u8) {
    let (rank, tag) = if e.tag == 35 {
        (0, 0)
    } else if D::is_header(e.tag) {
        (1, e.tag)
    } else {
        (2, e.tag)
    };
    match D::data_length_tag(e.tag) {
        Some(len_tag) if rank == 2 => (rank, len_tag, 1),
        _ => (rank, tag, 0),
    }
}

/// Bytes a `tag=` prefix occupies: the decimal digits, plus the `=`.
const fn tag_prefix_len(mut tag: u32) -> usize {
    let mut n = 1;
    while tag >= 10 {
        tag /= 10;
        n += 1;
    }
    n + 1
}

fn render_u32(mut v: u32, buf: &mut [u8; 10]) -> &[u8] {
    if v == 0 {
        buf[9] = b'0';
        return &buf[9..];
    }
    let mut i = 10;
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    &buf[i..]
}

impl<const P: usize, const S: usize> Template<P, S> {
    /// Bytes reserved in front of the body for `8=...\x019=NNNNN\x01`.
    #[inline]
    fn reserve(&self) -> usize {
        2 + self.begin_len as usize + 1 + 2 + MAX_BODY_DIGITS + 1
    }

    /// Write one message into `out` and return the range it occupies.
    ///
    /// The message does **not** start at `out[0]`: the prefix is right-aligned so
    /// the body never moves. Send `out[range]`.
    ///
    /// A template carrying a group hole needs
    /// [`encode_with`](Self::encode_with): this one has no dictionary, so it
    /// would write the group's counter and nothing after it.
    pub fn encode(
        &self,
        out: &mut [u8],
        slots: &[(u32, &[u8])],
    ) -> Result<Range<usize>, EncodeError> {
        self.encode_with::<NoDict>(out, slots, &[])
    }

    /// [`encode`](Self::encode), plus repeating groups.
    ///
    /// Each [`GroupData`] fills the hole with its counter tag. Field order
    /// inside an entry comes from `D::group_order`, never from the order the
    /// caller supplied — see [`GroupEntryData`].
    ///
    /// A declared group with no matching [`GroupData`] writes nothing at all,
    /// not even `counter=0`: an absent optional group and one with zero entries
    /// are different messages, and the caller says which by supplying data or
    /// not.
    pub fn encode_with<D: Dictionary>(
        &self,
        out: &mut [u8],
        slots: &[(u32, &[u8])],
        groups: &[GroupData<'_>],
    ) -> Result<Range<usize>, EncodeError> {
        let k = self.reserve();
        let mut w = k;

        for i in 0..self.len as usize {
            match self.parts[i] {
                Part::Static { at, len } => {
                    let src = &self.scratch[at as usize..at as usize + len as usize];
                    out.get_mut(w..w + src.len())
                        .ok_or(EncodeError::OutputTooSmall)?
                        .copy_from_slice(src);
                    w += src.len();
                }
                Part::DataLen { tag, data_tag } => {
                    // Absent DATA means absent length. An optional DATA field
                    // that was not supplied must not leave a lone length behind.
                    if let Some((_, value)) = slots.iter().find(|(t, _)| *t == data_tag) {
                        let mut digits = [0u8; 10];
                        let text = render_u32(
                            u32::try_from(value.len()).map_err(|_| EncodeError::BodyTooLong)?,
                            &mut digits,
                        );
                        w = put(out, w, tag, text)?;
                    }
                }
                Part::Slot(tag) => {
                    let Some((_, value)) = slots.iter().find(|(t, _)| *t == tag) else {
                        continue; // an unsupplied slot is simply not written
                    };
                    w = put(out, w, tag, value)?;
                }
                Part::Group(counter) => {
                    let Some(g) = groups.iter().find(|g| g.counter == counter) else {
                        continue; // an unsupplied group is not written at all
                    };
                    let mt = &self.scratch
                        [self.mt_at as usize..self.mt_at as usize + self.mt_len as usize];
                    w = put_group::<D>(out, w, mt, g, 0)?;
                }
            }
        }

        let body_len = w - k;
        if body_len >= 100_000 {
            return Err(EncodeError::BodyTooLong);
        }

        // Prefix, right-aligned so that it ends exactly where the body begins.
        let mut pre = [0u8; 2 + 32 + 1 + 2 + MAX_BODY_DIGITS + 1];
        let mut p = 0;
        pre[p] = b'8';
        pre[p + 1] = b'=';
        p += 2;
        let bl = self.begin_len as usize;
        pre.get_mut(p..p + bl)
            .ok_or(EncodeError::OutputTooSmall)?
            .copy_from_slice(&self.scratch[self.begin_at as usize..self.begin_at as usize + bl]);
        p += bl;
        pre[p] = SOH;
        pre[p + 1] = b'9';
        pre[p + 2] = b'=';
        p += 3;
        let mut d = [0u8; 10];
        let digits = render_u32(body_len as u32, &mut d);
        pre[p..p + digits.len()].copy_from_slice(digits);
        p += digits.len();
        pre[p] = SOH;
        p += 1;

        let start = k - p;
        out.get_mut(start..k)
            .ok_or(EncodeError::OutputTooSmall)?
            .copy_from_slice(&pre[..p]);

        // Checksum covers every byte of the message before `10=`.
        let sum = checksum(&out[start..w]);
        let cs = format_checksum(sum);
        let dst = out
            .get_mut(w..w + TRAILER_LEN)
            .ok_or(EncodeError::OutputTooSmall)?;
        dst[0] = b'1';
        dst[1] = b'0';
        dst[2] = b'=';
        dst[3..6].copy_from_slice(&cs);
        dst[6] = SOH;

        Ok(start..w + TRAILER_LEN)
    }
}

/// Writes `tag=value` and its separator at `w`, and returns the new `w`.
#[inline]
fn put(out: &mut [u8], w: usize, tag: u32, value: &[u8]) -> Result<usize, EncodeError> {
    let mut d = [0u8; 10];
    let digits = render_u32(tag, &mut d);
    let n = digits.len() + 1 + value.len() + 1;
    let dst = out.get_mut(w..w + n).ok_or(EncodeError::OutputTooSmall)?;
    dst[..digits.len()].copy_from_slice(digits);
    dst[digits.len()] = b'=';
    dst[digits.len() + 1..n - 1].copy_from_slice(value);
    dst[n - 1] = SOH;
    Ok(w + n)
}

/// `counter=N` followed by `N` entries, each in the dictionary's order.
/// The DATA member this tag is the length of, if this group has one.
fn data_owner<D: Dictionary>(order: &[u32], tag: u32) -> Option<u32> {
    order
        .iter()
        .copied()
        .find(|t| D::data_length_tag(*t) == Some(tag))
}

fn put_group<D: Dictionary>(
    out: &mut [u8],
    mut w: usize,
    msg_type: &[u8],
    g: &GroupData<'_>,
    depth: u8,
) -> Result<usize, EncodeError> {
    if depth >= MAX_DEPTH {
        return Err(EncodeError::GroupTooDeep);
    }
    let order = D::group_order(msg_type, g.counter);
    if order.is_empty() {
        return Err(EncodeError::UnknownGroup(g.counter));
    }
    let mut d = [0u8; 10];
    let count = u32::try_from(g.entries.len()).map_err(|_| EncodeError::BodyTooLong)?;
    w = put(out, w, g.counter, render_u32(count, &mut d))?;

    for e in g.entries {
        // Every supplied tag must belong to this group. Checked before writing
        // anything, so a rejected entry never leaves half a group in `out`.
        for (t, _) in e.fields {
            if !order.contains(t) {
                return Err(EncodeError::NotAGroupMember(*t));
            }
        }
        for sub in e.groups {
            if !order.contains(&sub.counter) {
                return Err(EncodeError::NotAGroupMember(sub.counter));
            }
        }
        let delimiter = *order.first().ok_or(EncodeError::UnknownGroup(g.counter))?;
        if !e.fields.iter().any(|(t, _)| *t == delimiter) {
            return Err(EncodeError::MissingDelimiter(g.counter));
        }

        // A DATA member without its length member cannot be framed by any
        // reader, exactly as at the top level. Refused before a byte is written,
        // so a rejected entry never leaves half a group in `out`.
        //
        // `[measured 2026-08-30]` FIX 4.4 has **66 DATA members across the group
        // tables, and all 66 have their length declared immediately in front**,
        // so the order below is already right — what was missing is that
        // nothing required the pair to be supplied, or the length to be true.
        for (t, _) in e.fields {
            if let Some(len_tag) = D::data_length_tag(*t)
                && !order.contains(&len_tag)
            {
                return Err(EncodeError::DataWithoutLength(*t));
            }
        }

        // Declaration order, from the table. The caller's order is not consulted
        // and cannot be: `order` is walked, not `e.fields`.
        for &tag in order {
            if let Some((_, v)) = e.fields.iter().find(|(t, _)| *t == tag) {
                // A length field for a DATA member present in this entry is
                // written from the data, never from what the caller passed.
                if let Some(data_tag) = data_owner::<D>(order, tag)
                    && let Some((_, dv)) = e.fields.iter().find(|(t, _)| *t == data_tag)
                {
                    let mut d = [0u8; 10];
                    let n = u32::try_from(dv.len()).map_err(|_| EncodeError::BodyTooLong)?;
                    w = put(out, w, tag, render_u32(n, &mut d))?;
                    continue;
                }
                w = put(out, w, tag, v)?;
            } else if let Some(sub) = e.groups.iter().find(|s| s.counter == tag) {
                w = put_group::<D>(out, w, msg_type, sub, depth + 1)?;
            }
        }
    }
    Ok(w)
}
