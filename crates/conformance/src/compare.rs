//! Comparing what the engine produced with what the `.def` file expects.
//!
//! The rules come from QuickFIX's `test/Comparator.rb`, recorded in
//! [`docs/reference/quickfix-acceptance-def-format.md`]:
//!
//! 1. split both on SOH;
//! 2. the field **count** must be equal — any extra or missing field fails;
//! 3. the field **order** must be identical, positionally;
//! 4. for a tag listed in `test/definitions/fields.fmt`, the **received** value
//!    is matched against a pattern and the expected value is ignored;
//! 5. every other tag is compared by exact byte equality.
//!
//! # Rule 3 is the expensive one
//!
//! A correct FIX message whose fields are in a different order **fails**. The
//! acceptance suite is not only a test of session behaviour; it silently pins
//! the field ordering of every message the session layer generates. That is why
//! `CLAUDE.md` §2 non-negotiable 5 exists.
//!
//! # A corpus `E` line is not engine output
//!
//! `[measured]` 244 of the 250 `E` lines carry `10=`, and **240 of those are not
//! three digits** — 238 are the literal `10=0` and 2 are `10=7` — so they do
//! not satisfy rule 4. That is
//! not a bug in this comparator: rule 4 matches the **received** value, and the
//! corpus never plays the part of the engine. An `E` line therefore does **not**
//! compare equal to itself, and anything standing in for the engine — the fake
//! session in the runner's own test, for one — has to compute a real checksum
//! the way an engine would. [`crate::script::with_real_checksum`] does that.
//!
//! # `9` is not loosely matched, and that is the point
//!
//! `BodyLength` is absent from `fields.fmt`, so every expected `9=` in the
//! corpus is a hard assertion that the body is byte-for-byte the right length.
//! Get the field order wrong and the length is usually still right — the
//! ordering check is what catches it, and the length check is what catches a
//! field written with the wrong width.
//!
//! [`docs/reference/quickfix-acceptance-def-format.md`]: ../../../docs/reference/quickfix-acceptance-def-format.md

const SOH: u8 = 0x01;

/// The first difference found, or nothing.
///
/// Carries the position so a failure names the field rather than dumping two
/// messages and leaving the reader to diff them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    /// Rule 2.
    FieldCount { expected: usize, actual: usize },
    /// Rule 3 — different tags at the same position.
    Tag {
        at: usize,
        expected: u32,
        actual: u32,
    },
    /// Rule 5 — same tag, different value.
    Value {
        at: usize,
        tag: u32,
        expected: Vec<u8>,
        actual: Vec<u8>,
    },
    /// Rule 4 — a loosely-matched tag whose received value has the wrong shape.
    Shape {
        at: usize,
        tag: u32,
        actual: Vec<u8>,
    },
    /// A field with no `=` in it. Not a FIX field at all.
    Malformed { at: usize, side: Side },
}

/// Which message a [`Mismatch::Malformed`] was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Expected,
    Actual,
}

/// What a loosely-matched tag's value must look like.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `\d{3}` — CheckSum.
    ThreeDigits,
    /// `\d{8}-\d{2}:\d{2}:\d{2}`, milliseconds optional and ignored.
    Timestamp,
}

/// The tags `test/definitions/fields.fmt` matches by pattern:
///
/// ```text
/// 10=\d{3}
/// 42=\d{8}-\d{2}:\d{2}:\d{2}
/// 52=\d{8}-\d{2}:\d{2}:\d{2}|\d{8}-\d{2}:\d{2}:\d{2}[.]\d{3}
/// 60=\d{8}-\d{2}:\d{2}:\d{2}
/// 122=\d{8}-\d{2}:\d{2}:\d{2}
/// ```
///
/// `52`'s second alternative is redundant under an unanchored match, which is
/// why one [`Shape::Timestamp`] serves all four time tags.
///
/// `tests/compare.rs` reads that file and asserts this list against it, so the
/// list is checked rather than believed.
pub const LOOSE_TAGS: &[u32] = &[10, 42, 52, 60, 122];

/// Deliberately a fixed list rather than "anything that looks like a time":
/// `122` OrigSendingTime is in it and `9` BodyLength is not, and guessing would
/// get both wrong.
const fn shape_of(tag: u32) -> Option<Shape> {
    match tag {
        10 => Some(Shape::ThreeDigits),
        42 | 52 | 60 | 122 => Some(Shape::Timestamp),
        _ => None,
    }
}

/// Compare a received message against an expected one.
///
/// # Errors
///
/// The first [`Mismatch`], in field order. Stops at the first difference: the
/// second is usually a consequence of the first.
pub fn compare(expected: &[u8], actual: &[u8]) -> Result<(), Mismatch> {
    let e: Vec<&[u8]> = split(expected);
    let a: Vec<&[u8]> = split(actual);

    if e.len() != a.len() {
        return Err(Mismatch::FieldCount {
            expected: e.len(),
            actual: a.len(),
        });
    }

    for (at, (ef, af)) in e.iter().zip(a.iter()).enumerate() {
        let (etag, eval) = field(ef).ok_or(Mismatch::Malformed {
            at,
            side: Side::Expected,
        })?;
        let (atag, aval) = field(af).ok_or(Mismatch::Malformed {
            at,
            side: Side::Actual,
        })?;

        if etag != atag {
            return Err(Mismatch::Tag {
                at,
                expected: etag,
                actual: atag,
            });
        }

        match shape_of(etag) {
            // Rule 4: the expected value is not consulted at all.
            Some(shape) if !matches_shape(shape, aval) => {
                return Err(Mismatch::Shape {
                    at,
                    tag: etag,
                    actual: aval.to_vec(),
                });
            }
            Some(_) => {}
            None if eval != aval => {
                return Err(Mismatch::Value {
                    at,
                    tag: etag,
                    expected: eval.to_vec(),
                    actual: aval.to_vec(),
                });
            }
            None => {}
        }
    }
    Ok(())
}

fn split(msg: &[u8]) -> Vec<&[u8]> {
    msg.split(|&b| b == SOH).filter(|f| !f.is_empty()).collect()
}

fn field(f: &[u8]) -> Option<(u32, &[u8])> {
    let eq = f.iter().position(|&b| b == b'=')?;
    let (tag, value) = f.split_at(eq);
    if tag.is_empty() || !tag.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut n: u32 = 0;
    for &d in tag {
        n = n.checked_mul(10)?.checked_add(u32::from(d - b'0'))?;
    }
    Some((n, value.get(1..).unwrap_or_default()))
}

/// Unanchored, like Ruby's `=~`, which is what `Comparator.rb` uses.
///
/// It matters: `52=20260828-14:30:59.123` matches
/// `\d{8}-\d{2}:\d{2}:\d{2}` under an unanchored search and would not under an
/// anchored one, and half the corpus's timestamps carry milliseconds while the
/// other half do not.
fn matches_shape(shape: Shape, value: &[u8]) -> bool {
    match shape {
        Shape::ThreeDigits => value.windows(3).any(|w| w.iter().all(u8::is_ascii_digit)),
        Shape::Timestamp => (0..value.len()).any(|i| timestamp_at(&value[i..])),
    }
}

/// `\d{8}-\d{2}:\d{2}:\d{2}` at the start of `s`.
fn timestamp_at(s: &[u8]) -> bool {
    const PATTERN: &[u8] = b"dddddddd-dd:dd:dd";
    if s.len() < PATTERN.len() {
        return false;
    }
    PATTERN.iter().zip(s).all(|(p, &c)| match p {
        b'd' => c.is_ascii_digit(),
        _ => c == *p,
    })
}
