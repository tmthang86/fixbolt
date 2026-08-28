//! FIX checksum: every byte before `10=`, summed mod 256, rendered as three
//! digits with leading zeros.
//!
//! Plain byte loop. `reference/measured-costs.md` puts a whole `NewOrderSingle`
//! parse at 139 ns without any of this vectorised, which is already inside the
//! published gate — so SIMD waits until a measurement asks for it.

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
