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
use crate::dict::Dictionary;

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
}

#[derive(Clone, Copy)]
enum Part {
    /// A run of already-encoded `tag=value\x01` bytes in the scratch buffer.
    Static { at: u16, len: u16 },
    /// A hole. Filled from `slots` at send time, or skipped when absent.
    Slot(u32),
}

#[derive(Clone, Copy)]
struct Entry {
    tag: u32,
    at: u16,
    len: u16,
    is_static: bool,
}

/// A message skeleton with holes in it.
///
/// `P` bounds the number of parts, `S` the static bytes. Both are the caller's
/// choice, like `FieldIndex<N>`.
pub struct Template<const P: usize, const S: usize> {
    scratch: [u8; S],
    begin_at: u16,
    begin_len: u16,
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
                is_static: true,
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

        let mut nparts: usize = 0;
        for i in 0..self.n {
            let e = self.entries[i];
            if e.is_static {
                let src = &self.scratch[e.at as usize..e.at as usize + e.len as usize];
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
                *out.parts.get_mut(nparts).ok_or(EncodeError::TooManyParts)? = Part::Slot(e.tag);
                nparts += 1;
            }
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
            is_static: true,
        })
    }

    fn push_slot(&mut self, tag: u32) -> Result<(), EncodeError> {
        reserved(tag)?;
        self.entry(Entry {
            tag,
            at: 0,
            len: 0,
            is_static: false,
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
fn key<D: Dictionary>(e: &Entry) -> (u8, u32) {
    if e.tag == 35 {
        (0, 0)
    } else if D::is_header(e.tag) {
        (1, e.tag)
    } else {
        (2, e.tag)
    }
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
    pub fn encode(
        &self,
        out: &mut [u8],
        slots: &[(u32, &[u8])],
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
                Part::Slot(tag) => {
                    let Some((_, value)) = slots.iter().find(|(t, _)| *t == tag) else {
                        continue; // an unsupplied slot is simply not written
                    };
                    let mut d = [0u8; 10];
                    let digits = render_u32(tag, &mut d);
                    let n = digits.len() + 1 + value.len() + 1;
                    let dst = out.get_mut(w..w + n).ok_or(EncodeError::OutputTooSmall)?;
                    dst[..digits.len()].copy_from_slice(digits);
                    dst[digits.len()] = b'=';
                    dst[digits.len() + 1..n - 1].copy_from_slice(value);
                    dst[n - 1] = SOH;
                    w += n;
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
