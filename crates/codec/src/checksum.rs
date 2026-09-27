//! FIX checksum: every byte before `10=`, summed mod 256, rendered as three
//! digits with leading zeros.
//!
//! Eight bytes at a time (SWAR), ADR-0211: each word is added into an
//! accumulator **lane by lane**, every lane its own sum mod 256 with no carry
//! into its neighbour, so there is no length at which it overflows. ADR-0211
//! also records the catch: the compiler already vectorises the plain byte loop,
//! so this is judged against that loop on the `DESIGN.md` §9 machine, on its
//! own bench case (`benches/checksum.rs`), and kept only if it wins there.
//!
//! What proves it: `tests` below against the byte loop (every length 0..=300 at
//! every start 0..8, and 64 KiB of `0xFF` for the carry trap), Miri on x86-64
//! and on big-endian `s390x-unknown-linux-gnu`, and `fuzz/fuzz_targets/checksum.rs`.

/// The low seven bits of every lane.
const LOW7: u64 = u64::from_ne_bytes([0x7F; 8]);
/// The high bit of every lane.
const HIGH: u64 = u64::from_ne_bytes([0x80; 8]);

/// Sum of `bytes`, mod 256.
#[inline]
#[must_use]
pub fn checksum(bytes: &[u8]) -> u8 {
    let (words, rest) = bytes.as_chunks::<8>();
    let mut acc: u64 = 0;
    for word in words {
        // Byte order does not matter: every byte lands in some lane, and the
        // lanes are summed together at the end.
        let w = u64::from_ne_bytes(*word);
        // Per lane: the low seven bits add to at most 0xFE, so nothing crosses
        // into the next lane; the high bit is then the XOR of both high bits and
        // that carry — a byte add mod 256 in every lane at once.
        acc = ((acc & LOW7) + (w & LOW7)) ^ ((acc ^ w) & HIGH);
    }
    let mut sum: u8 = 0;
    for b in acc.to_ne_bytes() {
        sum = sum.wrapping_add(b);
    }
    // The last `len % 8` bytes, one at a time.
    for &b in rest {
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

#[cfg(test)]
mod tests {
    use super::checksum;

    /// The loop `checksum` replaced, word for word: the reference.
    fn byte_loop(bytes: &[u8]) -> u8 {
        let mut sum: u8 = 0;
        for &b in bytes {
            sum = sum.wrapping_add(b);
        }
        sum
    }

    /// A 64-bit LCG (Knuth's MMIX constants), top byte out: `codec` has no
    /// random-number dev-dependency, and the bytes only need to be varied and
    /// reproducible.
    fn lcg_bytes<const N: usize>() -> [u8; N] {
        let mut out = [0u8; N];
        let mut x: u64 = 0x2545_F491_4F6C_DD1D;
        for b in &mut out {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *b = (x >> 56) as u8;
        }
        out
    }

    const MAX_LEN: usize = 300;

    #[test]
    fn checksum_matches_the_byte_loop_on_every_length_to_300() {
        let bytes = lcg_bytes::<{ MAX_LEN + 8 }>();
        let mut cases = 0usize;
        // Every start 0..8, so no length meets the word loop at one phase only.
        for start in 0..8 {
            let from = bytes.split_at(start).1;
            for len in 0..=MAX_LEN {
                let buf = from.split_at(len).0;
                assert_eq!(checksum(buf), byte_loop(buf), "start {start} len {len}");
                cases += 1;
            }
        }
        assert_eq!(cases, 8 * (MAX_LEN + 1), "every case ran");
    }

    /// The carry trap: `0xFF + 0xFF` overflows a lane on the first add. A lane
    /// sum that leaked its carry into the next lane, or dropped the top lane's,
    /// is wrong here within two words and stays wrong.
    #[test]
    fn checksum_of_64_kib_of_0xff() {
        let ff = [0xFFu8; 64 * 1024];
        // n bytes of 0xFF sum to -n mod 256.
        assert_eq!(checksum(&ff), 0, "65536 x 0xFF");
        assert_eq!(checksum(ff.split_at(65_528).0), 8, "65528 x 0xFF");
        assert_eq!(checksum(ff.split_at(65_535).0), 1, "65535 x 0xFF");
    }
}
