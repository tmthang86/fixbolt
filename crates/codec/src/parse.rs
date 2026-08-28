//! Reading a FIX frame out of a byte buffer, in place.
//!
//! # What this refuses, and what it does not
//!
//! The codec knows **syntax**. It rejects only what it cannot read:
//! a frame it cannot delimit, a tag that is not a number, a DATA length that
//! points outside the frame. Everything readable but wrong — an empty value, an
//! unexpected `BeginString`, `MsgType` in the wrong position — is passed up, and
//! the session decides, because only the session knows what to *answer*.
//!
//! That boundary is not stylistic. `14d` sends `56=` empty and expects a Reject
//! naming tag 56 **with the sequence number consumed**; a parser that refused the
//! frame would leave the session unable to read `34=` and unable to pass.

use crate::checksum::checksum;
use crate::dict::Dictionary;
use crate::index::{FieldIndex, as_u32};

/// The FIX field separator.
pub const SOH: u8 = 0x01;

/// The outcome of a successful read.
///
/// `Incomplete` sits in the `Ok` branch on purpose. TCP delivers bytes, not
/// messages, so "wait for more" is the normal case, not a failure. Folding it
/// into `Err` would make every call site pay to tell it apart from "this session
/// is broken, hang up".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parsed {
    /// A whole message was read. `consumed` bytes may be dropped from the front
    /// of the buffer; anything after them is the next message.
    Complete {
        /// Bytes belonging to this message, including its `10=` field.
        consumed: usize,
    },
    /// Not enough bytes yet. Keep the buffer as it is and read again.
    Incomplete,
}

/// Why a frame could not be read.
///
/// `Copy`, and carries at most one `u32`. No `String`, no allocation — this is
/// an error path on the hot path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// `8=` is not at byte 0, or `9=` does not immediately follow it.
    ///
    /// Without both the message cannot be delimited at all, which is why this
    /// is the codec's business and `MsgType` position is not.
    BadFrameStart,
    /// A tag was not a decimal number, or did not fit in a `u32`.
    ///
    /// `at` is the **byte offset of the tag's first byte**, not a tag value —
    /// there isn't one. The session needs it: `14a_BadField` sends `-1=HI` and
    /// expects a Reject carrying `371=-1` with the sequence number consumed, so
    /// something must hand it the text `-1`. Slice from `at` to the next `=`.
    ///
    /// After this error the index holds every field read **before** the bad one,
    /// which is how the session still reads `34=` and answers. Same reasoning as
    /// D12: refusing outright would make the definition unpassable.
    BadTag {
        /// Byte offset of the first byte of the unreadable tag.
        at: u32,
    },
    /// More fields than `FieldIndex<N>` can hold. Never a truncated success.
    TooManyFields,
    /// A DATA field whose length field is absent, or is not the field
    /// immediately before it.
    MissingLengthField(u32),
    /// A DATA field whose declared length runs past the end of the frame.
    LengthOutOfBounds(u32),
    /// `9=` disagrees with the bytes actually between it and `10=`.
    BadBodyLength,
    /// `10=` is not three digits, or does not match the computed sum.
    BadCheckSum,
    /// A value longer than `u16::MAX`. Surfaced rather than wrapped.
    FieldTooLong(u32),
}

/// Which frame-level checks to run.
///
/// A real parameter, not a documented default: the conformance runner needs to
/// feed frames whose `9=` or `10=` are deliberately wrong and see them reach the
/// session, and a caller behind a checked transport may not want to pay twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Validation {
    /// Check `9=` against the measured body length.
    pub body_length: bool,
    /// Check `10=` against the computed sum.
    pub check_sum: bool,
}

impl Validation {
    /// Check everything. The default.
    pub const ALL: Self = Self {
        body_length: true,
        check_sum: true,
    };
    /// Check nothing at the frame level.
    pub const NONE: Self = Self {
        body_length: false,
        check_sum: false,
    };
}

impl Default for Validation {
    fn default() -> Self {
        Self::ALL
    }
}

#[inline]
fn find_soh(buf: &[u8], from: usize) -> Option<usize> {
    let tail = buf.get(from..)?;
    tail.iter().position(|&b| b == SOH).map(|i| i + from)
}

/// Read a tag and return it with the offset of its value's first byte.
/// `Ok(None)` means the buffer ran out mid-tag.
#[inline]
fn read_tag(buf: &[u8], from: usize) -> Result<Option<(u32, usize)>, ParseError> {
    let bad = ParseError::BadTag { at: from as u32 };
    let mut pos = from;
    let mut n: u32 = 0;
    let mut digits = 0u32;
    loop {
        match buf.get(pos) {
            None => return Ok(None),
            Some(b'=') => {
                if digits == 0 {
                    return Err(bad);
                }
                return Ok(Some((n, pos + 1)));
            }
            Some(&b) => {
                let d = b.wrapping_sub(b'0');
                if d > 9 {
                    return Err(bad);
                }
                n = match n.checked_mul(10).and_then(|x| x.checked_add(u32::from(d))) {
                    Some(x) => x,
                    None => return Err(bad),
                };
                digits += 1;
                pos += 1;
            }
        }
    }
}

/// The tag text that begins at `at`, up to but not including its `=`.
///
/// Pairs with [`ParseError::BadTag`]: it is how the session turns "I could not
/// read that tag" into `371=-1`. Returns `None` if there is no `=` ahead.
#[inline]
#[must_use]
pub fn tag_text_at(buf: &[u8], at: usize) -> Option<&[u8]> {
    let tail = buf.get(at..)?;
    let end = tail.iter().position(|&b| b == b'=')?;
    tail.get(..end)
}

/// Read one message from the front of `buf`, filling `idx`.
///
/// # What `idx` holds afterwards
///
/// The index is cleared on entry and filled as the scan proceeds, so:
///
/// | Outcome | `idx` holds |
/// |---|---|
/// | `Ok(Complete)` | every field of the message |
/// | `Ok(Incomplete)` | the fields read so far. Not useful — the next call refills it |
/// | `Err(BadTag { at })` | **every field before the bad tag.** Specified, and relied on: it is how `14a` reads `34=` and answers |
/// | other `Err` | the fields read before the failure. Do not use them: the frame is not trustworthy |
pub fn parse_into<D: Dictionary, const N: usize>(
    buf: &[u8],
    idx: &mut FieldIndex<N>,
    v: Validation,
) -> Result<Parsed, ParseError> {
    idx.clear();

    // ---- 8= at byte 0. Position only; 8=FIX.3.9 parses fine and the session
    // ---- decides what to think of it (1d, 2i).
    match buf.first() {
        None => return Ok(Parsed::Incomplete),
        Some(b'8') => {}
        Some(_) => return Err(ParseError::BadFrameStart),
    }
    match buf.get(1) {
        None => return Ok(Parsed::Incomplete),
        Some(b'=') => {}
        Some(_) => return Err(ParseError::BadFrameStart),
    }
    let Some(soh1) = find_soh(buf, 2) else {
        return Ok(Parsed::Incomplete);
    };
    idx.push(8, 2, soh1 - 2)?;

    // ---- 9= immediately after. This is what makes the frame delimitable.
    let p = soh1 + 1;
    match buf.get(p) {
        None => return Ok(Parsed::Incomplete),
        Some(b'9') => {}
        Some(_) => return Err(ParseError::BadFrameStart),
    }
    match buf.get(p + 1) {
        None => return Ok(Parsed::Incomplete),
        Some(b'=') => {}
        Some(_) => return Err(ParseError::BadFrameStart),
    }
    let val9 = p + 2;
    let Some(soh2) = find_soh(buf, val9) else {
        return Ok(Parsed::Incomplete);
    };
    let declared_body = match as_u32(&buf[val9..soh2]) {
        Ok(n) => n as usize,
        Err(_) => return Err(ParseError::BadBodyLength),
    };
    idx.push(9, val9, soh2 - val9)?;

    let body_start = soh2 + 1;
    // Where `10=` should begin if `9=` is telling the truth. Used to bound DATA
    // fields even when body-length validation is off, because a DATA length is
    // otherwise unbounded and the parser would wait for bytes that never come.
    let trailer_at = body_start
        .checked_add(declared_body)
        .ok_or(ParseError::BadBodyLength)?;

    let mut pos = body_start;
    loop {
        if pos >= buf.len() {
            return Ok(Parsed::Incomplete);
        }
        let Some((tag, val_start)) = read_tag(buf, pos)? else {
            return Ok(Parsed::Incomplete);
        };

        if tag == 10 {
            let Some(soh) = find_soh(buf, val_start) else {
                return Ok(Parsed::Incomplete);
            };
            idx.push(10, val_start, soh - val_start)?;

            if v.body_length && pos != trailer_at {
                return Err(ParseError::BadBodyLength);
            }
            if v.check_sum {
                let digits = &buf[val_start..soh];
                if digits.len() != 3 {
                    return Err(ParseError::BadCheckSum);
                }
                let want = as_u32(digits).map_err(|_| ParseError::BadCheckSum)?;
                if want != u32::from(checksum(&buf[..pos])) {
                    return Err(ParseError::BadCheckSum);
                }
            }
            return Ok(Parsed::Complete { consumed: soh + 1 });
        }

        let val_len = if let Some(len_tag) = D::data_length_tag(tag) {
            // A DATA value may contain SOH, so its length comes from the field
            // in front of it — which must be exactly the field in front of it.
            let prev = idx.last().ok_or(ParseError::MissingLengthField(tag))?;
            if prev.tag != len_tag {
                return Err(ParseError::MissingLengthField(tag));
            }
            let lo = prev.offset as usize;
            let raw = buf
                .get(lo..lo + prev.len as usize)
                .ok_or(ParseError::MissingLengthField(tag))?;
            let n = as_u32(raw).map_err(|_| ParseError::MissingLengthField(tag))? as usize;
            let end = val_start
                .checked_add(n)
                .ok_or(ParseError::LengthOutOfBounds(tag))?;
            if end > trailer_at {
                return Err(ParseError::LengthOutOfBounds(tag));
            }
            match buf.get(end) {
                None => return Ok(Parsed::Incomplete),
                Some(&SOH) => {}
                Some(_) => return Err(ParseError::LengthOutOfBounds(tag)),
            }
            n
        } else {
            let Some(soh) = find_soh(buf, val_start) else {
                return Ok(Parsed::Incomplete);
            };
            soh - val_start
        };

        idx.push(tag, val_start, val_len)?;
        pos = val_start + val_len + 1;
    }
}
