//! FIX checksum: every byte before `10=`, summed mod 256, rendered as three
//! digits with leading zeros.
//!
//! Plain byte loop, and it stays one (ADR-0211 records that the compiler
//! already vectorises it). ADR-0211's hand-written SWAR (eight lanes of seven-bit sums) was measured
//! against this loop on the `DESIGN.md` §9 machine, `benches/checksum.rs`, and
//! lost by 3.2× on `checksum NewOrderSingle` (4.5 → 14.5 ns/op, n = 24), so it
//! was reverted. A faster checksum is judged on that bench case, against this loop.

/// Sum of `bytes`, mod 256.
#[inline]
#[must_use]
pub fn checksum(bytes: &[u8]) -> u8 {
    let mut sum: u8 = 0;
    for &b in bytes {
        sum = sum.wrapping_add(b);
    }
    sum
}

/// Render a checksum the way FIX writes it: exactly three digits, zero-padded.
#[inline]
#[must_use]
pub fn format_checksum(sum: u8) -> [u8; 3] {
    [
        b'0' + (sum / 100),
        b'0' + ((sum / 10) % 10),
        b'0' + (sum % 10),
    ]
}
