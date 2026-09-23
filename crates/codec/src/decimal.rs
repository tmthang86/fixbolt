//! A FIX float read on demand into a 16-byte `Copy` value, and written back.
//!
//! ADR-0120 (which revises ADR-0028 decisions 1 and 2). Nothing here runs
//! unless the caller asks: the parser stores no typed value, the index is
//! unchanged, and a price is read as `as_decimal(view.get(44)?)` exactly the
//! way an integer is read with `as_i64`.
//!
//! No `unsafe`, no allocation, no panic. `benches/alloc.rs` case `decimal`
//! proves the second. The third is proven in three parts, because no single
//! one of them covers it:
//! * clippy `indexing_slicing = "deny"` stops `a[i]` and `a[i..j]`. It does
//!   **not** see a method call such as `split_at`, which panics just as surely
//!   (`docs/reference/split-at-panics-where-no-lint-looks.md`). So `format`
//!   uses `split_at_checked` and slice `get`, which cannot panic, and that
//!   choice is held by review, not by any lint.
//! * Tests drive every split position and both ends of the range:
//!   `format_places_the_point_at_every_position`,
//!   `the_longest_output_is_max_len` and
//!   `a_zero_mantissa_formats_as_zero_whatever_the_exponent` in
//!   `tests/decimal.rs`.
//! * `fuzz/fuzz_targets/decimal.rs` runs arbitrary bytes through
//!   `as_decimal` and every accepted value through `format`.

use crate::index::ConvertError;

/// A decimal number: `mantissa × 10^exponent`.
///
/// Parsed from FIX, the exponent is never positive — `1.50` is
/// `(150, -2)`, `23` is `(23, 0)`. A positive exponent reaches a `Decimal`
/// only through [`Decimal::new`], typically from an SBE decimal, whose sign
/// convention this is (ADR-0120 decision 7): every SBE `int64`/`int32`
/// mantissa with an `int8` exponent is `Decimal::new(m, e)` without loss.
///
/// **Equality is structural**: `1.5 != 1.50`, because they are different
/// bytes on the wire (ADR-0028 decision 4). There is no `Ord` and no
/// arithmetic; across exponents every one of those is a rounding decision, and
/// rounding money is the application's call.
///
/// Sixteen bytes, asserted at compile time in `lib.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decimal {
    mantissa: i64,
    exponent: i8,
}

impl Decimal {
    /// The longest [`Decimal::format`] can write: `-`, 19 digits of
    /// `i64::MIN`, and 127 zeros for the largest positive exponent.
    ///
    /// The longest negative-exponent form is shorter — `-0.` and 128 fraction
    /// digits, 131 bytes.
    pub const MAX_LEN: usize = 147;

    /// `mantissa × 10^exponent`, taken as given. Any pair is valid.
    #[inline]
    #[must_use]
    pub const fn new(mantissa: i64, exponent: i8) -> Self {
        Self { mantissa, exponent }
    }

    /// The integer the value is a multiple of `10^exponent` of.
    #[inline]
    #[must_use]
    pub const fn mantissa(self) -> i64 {
        self.mantissa
    }

    /// The power of ten. `-2` for `1.50`.
    #[inline]
    #[must_use]
    pub const fn exponent(self) -> i8 {
        self.exponent
    }

    /// Write the canonical FIX form into `out` and return the bytes written.
    ///
    /// Infallible: the buffer is sized for the longest case, and every write
    /// goes through an iterator over it, so nothing here can index out of
    /// bounds or panic. The canonical form (ADR-0120 decision 4):
    ///
    /// * `-` only for a negative mantissa — a zero never carries one;
    /// * the integer part with no leading zero other than a single `0`;
    /// * `exponent < 0`: a `.` and exactly `-exponent` fraction digits, the
    ///   trailing zeros kept — `(150, -2)` is `1.50`;
    /// * `exponent > 0`: that many `0`s and no point — `(5, 3)` is `5000`. A
    ///   zero mantissa is written `0` whatever the exponent, since `0000` would
    ///   break the rule above;
    /// * never `+`, never exponent notation.
    ///
    /// The result goes into a `Template` slot as-is. To echo a counterparty's
    /// bytes verbatim, send `view.get(tag)` instead: `002000.00` reads fine
    /// and writes back as `2000.00` (ADR-0120 decision 5).
    #[must_use]
    pub fn format(self, out: &mut [u8; Self::MAX_LEN]) -> &[u8] {
        // The digits of |mantissa|, right-aligned. `unsigned_abs` is what keeps
        // `i64::MIN` from overflowing on the way to its magnitude.
        let mut digits = [b'0'; 20];
        let mut n = self.mantissa.unsigned_abs();
        let mut len = 0usize;
        for slot in digits.iter_mut().rev() {
            // `n % 10 < 10`, so the narrowing loses nothing.
            *slot = b'0' + (n % 10) as u8;
            n /= 10;
            len += 1;
            if n == 0 {
                break;
            }
        }
        let digits = digits
            .get(digits.len().saturating_sub(len)..)
            .unwrap_or_default();

        let mut w = Writer::new(out);
        if self.mantissa < 0 {
            w.put(b'-');
        }
        match self.exponent {
            e if e >= 0 => {
                w.put_all(digits);
                if self.mantissa != 0 {
                    w.repeat(b'0', e.unsigned_abs() as usize);
                }
            }
            e => {
                let frac = e.unsigned_abs() as usize;
                // `split_at` would panic on a bad midpoint and no lint names
                // it; the checked form cannot. `None` is exactly "the digits
                // do not reach past the point".
                let split = len
                    .checked_sub(frac)
                    .filter(|&whole| whole > 0)
                    .and_then(|whole| digits.split_at_checked(whole));
                if let Some((whole, tail)) = split {
                    w.put_all(whole);
                    w.put(b'.');
                    w.put_all(tail);
                } else {
                    w.put(b'0');
                    w.put(b'.');
                    w.repeat(b'0', frac.saturating_sub(len));
                    w.put_all(digits);
                }
            }
        }
        let written = w.written;
        out.get(..written).unwrap_or_default()
    }
}

/// A cursor over the output buffer that cannot write past its end.
///
/// `Decimal::MAX_LEN` is the longest output, so no write is ever dropped;
/// `the_longest_output_is_max_len` proves the bound, and if it were ever wrong
/// the output would come back short rather than panic.
struct Writer<'a> {
    slots: core::slice::IterMut<'a, u8>,
    written: usize,
}

impl<'a> Writer<'a> {
    #[inline]
    fn new(out: &'a mut [u8]) -> Self {
        Self {
            slots: out.iter_mut(),
            written: 0,
        }
    }

    #[inline]
    fn put(&mut self, b: u8) {
        if let Some(slot) = self.slots.next() {
            *slot = b;
            self.written += 1;
        }
    }

    #[inline]
    fn put_all(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.put(b);
        }
    }

    #[inline]
    fn repeat(&mut self, b: u8, n: usize) {
        for _ in 0..n {
            self.put(b);
        }
    }
}

/// Read a FIX float — `PRICE`, `QTY`, `AMT`, `FLOAT`, `PRICEOFFSET`,
/// `PERCENTAGE` — as a [`Decimal`].
///
/// Used as `as_decimal(view.get(44)?)`. It does not know which tag the bytes
/// came from; asking it to read a `STRING` is the caller's mistake.
///
/// **The grammar is the session's, to the byte** (ADR-0120 decision 3):
/// `-? digit* ('.' digit*)?` with at least one digit. Leading zeros, `.5` and
/// `5.` are accepted, because real engines write them.
///
/// # Errors
///
/// * [`ConvertError::NotANumber`] — empty; a `+` (FIX floats carry a minus or
///   nothing, and `14f_IncorrectDataFormat.def` sends `+200.00` expecting a
///   reject); `e`/`E`; whitespace; a second point; a lone `-` or `.`; any
///   other byte.
/// * [`ConvertError::Overflow`] — the digits, leading zeros dropped and
///   trailing zeros **kept** (they are the scale), exceed ±(2^63 − 1); or more
///   than 128 digits follow the point. **Never rounded.** `i64::MIN` is
///   refused, so the range is symmetric.
///
/// A syntax fault wins over an overflow: `99…9x` is `NotANumber`. The whole
/// value is classified before its size is, so this function refuses exactly
/// what `FieldType::accepts` refuses and no more — the tests
/// `agrees_with_the_session_float_syntax_*` hold that, since the two grammars
/// are two pieces of code.
#[inline]
pub fn as_decimal(value: &[u8]) -> Result<Decimal, ConvertError> {
    let (negative, body) = match value.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, value),
    };
    // The magnitude, accumulated positive: its ceiling is `i64::MAX`, which is
    // exactly the range, so `i64::MIN` falls out as an overflow for free.
    let mut magnitude: i64 = 0;
    let mut overflow = false;
    let mut any_digit = false;
    let mut point = false;
    let mut fraction_digits: u32 = 0;
    for &b in body {
        let d = b.wrapping_sub(b'0');
        if d <= 9 {
            any_digit = true;
            if point {
                fraction_digits = fraction_digits.saturating_add(1);
            }
            // Keep scanning after an overflow: a bad byte later still has to
            // win, or `99…9x` would say `Overflow` where the session says the
            // value is malformed.
            if !overflow {
                match magnitude
                    .checked_mul(10)
                    .and_then(|m| m.checked_add(i64::from(d)))
                {
                    Some(m) => magnitude = m,
                    None => overflow = true,
                }
            }
        } else if b == b'.' && !point {
            point = true;
        } else {
            return Err(ConvertError::NotANumber);
        }
    }
    if !any_digit {
        return Err(ConvertError::NotANumber);
    }
    if overflow {
        return Err(ConvertError::Overflow);
    }
    // 128 fraction digits is `-128`, the last `i8`; 129 does not convert.
    let exponent = i8::try_from(-i64::from(fraction_digits)).map_err(|_| ConvertError::Overflow)?;
    let mantissa = if negative { -magnitude } else { magnitude };
    Ok(Decimal::new(mantissa, exponent))
}
