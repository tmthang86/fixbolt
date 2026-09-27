//! Finding the next SOH, eight bytes at a time — ADR-0211, plan
//! `docs/plans/2026-09-27-p4-simd.md` step 2.
//!
//! SWAR ("SIMD within a register"): eight bytes are loaded as one `u64`, XORed
//! with SOH in every lane so an SOH becomes a zero lane, and the classic
//! has-a-zero-byte test flags the zero lanes in their high bits. Plain integer
//! arithmetic, no intrinsics, no raw pointers: `as_chunks` and `get` do the
//! slicing, so nothing here can index out of bounds or panic.
//!
//! **Why the answer is the first SOH on every architecture.** The subtraction
//! borrows upward, so a lane can be flagged falsely — but only a lane *above* a
//! real zero lane, never below one. The word is loaded little-endian (lane 0 is
//! the lowest-addressed byte on every target, big-endian included), and the
//! lowest set bit is taken (`trailing_zeros`), so the lane reported is always a
//! real SOH and always the first.
//!
//! What proves it, rather than this paragraph:
//! - `tests` below, against the byte loop this replaced: every length 0..=40,
//!   every `from`, seven background bytes, SOH at each position with and without
//!   a second one right after, and the borrow trap on its own;
//! - Miri on x86-64 and on `s390x-unknown-linux-gnu` (big-endian) running those
//!   tests (plan step 2 gate);
//! - `fuzz/fuzz_targets/parse.rs` property 4: every non-DATA field `parse_into`
//!   reports holds no SOH and ends at one.

use crate::parse::SOH;

/// `0x01` in every lane.
const ONES: u64 = u64::from_ne_bytes([0x01; 8]);
/// The high bit of every lane.
const HIGHS: u64 = u64::from_ne_bytes([0x80; 8]);
/// SOH in every lane: XOR with it turns each SOH into a zero lane.
const SOH_LANES: u64 = u64::from_ne_bytes([SOH; 8]);

/// The offset of the first SOH at or after `from`, or `None` — the same
/// contract as `buf.get(from..)?.iter().position(|&b| b == SOH)`, including
/// `None` for any `from` past the end.
#[inline]
pub(crate) fn find_soh(buf: &[u8], from: usize) -> Option<usize> {
    let tail = buf.get(from..)?;
    let (words, rest) = tail.as_chunks::<8>();
    for (i, word) in words.iter().enumerate() {
        let x = u64::from_le_bytes(*word) ^ SOH_LANES;
        let hits = x.wrapping_sub(ONES) & !x & HIGHS;
        if hits != 0 {
            return Some(from + i * 8 + (hits.trailing_zeros() / 8) as usize);
        }
    }
    // The last `len % 8` bytes, one at a time.
    let done = from + words.len() * 8;
    rest.iter().position(|&b| b == SOH).map(|j| done + j)
}

#[cfg(test)]
mod tests {
    use super::find_soh;
    use crate::parse::SOH;

    /// The loop `find_soh` replaced, word for word: the reference every case
    /// below is compared against.
    fn byte_loop(buf: &[u8], from: usize) -> Option<usize> {
        let tail = buf.get(from..)?;
        tail.iter().position(|&b| b == SOH).map(|i| i + from)
    }

    /// Bytes chosen for the carries and borrows they provoke in the word
    /// arithmetic: `0x00` borrows, `0x02` is SOH with one bit moved, `0x7F` /
    /// `0x80` / `0x81` straddle the high bit the test reads, `0xFE` / `0xFF` are
    /// the top of the range.
    const BACKGROUNDS: [u8; 7] = [0x00, 0x02, 0x7F, 0x80, 0x81, 0xFE, 0xFF];

    /// Five whole words, so every lane of every word position and every length
    /// of the tail after the last whole word is covered.
    #[cfg(not(miri))]
    const MAX_LEN: usize = 40;
    /// Under Miri, three whole words: still an SOH in every lane of more than
    /// one word, and every tail length after two. `[measured 2026-09-27]` 40
    /// took `real 15m0.3s` on x86-64 Miri, at the plan's 15-minute bound; the
    /// plan shrinks the length range there, never the background bytes.
    #[cfg(miri)]
    const MAX_LEN: usize = 24;

    /// Every `from` from 0 to one past the end, against the reference.
    fn every_from(buf: &[u8]) -> usize {
        let mut n = 0;
        for from in 0..=buf.len() + 1 {
            assert_eq!(
                find_soh(buf, from),
                byte_loop(buf, from),
                "buf {buf:02x?} from {from}"
            );
            n += 1;
        }
        n
    }

    #[test]
    fn find_soh_matches_the_byte_loop_on_every_short_input() {
        let mut cases = 0usize;
        for bg in BACKGROUNDS {
            for len in 0..=MAX_LEN {
                // No SOH at all.
                let arr = [bg; MAX_LEN];
                cases += every_from(arr.split_at(len).0);
                // One SOH at each position, then a second right after it.
                for at in 0..len {
                    for second in [false, true] {
                        let mut arr = [bg; MAX_LEN];
                        if let Some(b) = arr.get_mut(at) {
                            *b = SOH;
                        }
                        if second && let Some(b) = arr.get_mut(at + 1) {
                            *b = SOH;
                        }
                        cases += every_from(arr.split_at(len).0);
                    }
                }
            }
        }
        // Liveness: the loops above ran every case they name, not some.
        let per_background: usize = (0..=MAX_LEN).map(|l| (1 + 2 * l) * (l + 2)).sum();
        assert_eq!(cases, BACKGROUNDS.len() * per_background);
    }

    /// The borrow trap: SOH turns into a zero lane, `0x00` bytes above it turn
    /// into `0x01` lanes, and subtracting one from each borrows through all of
    /// them — so every lane above the SOH can read as a match. Only the lowest
    /// one is real.
    #[test]
    fn soh_followed_by_nul_reports_the_soh() {
        assert_eq!(find_soh(b"a\x01\0\0\0\0\0\0", 0), Some(1));
        assert_eq!(find_soh(b"\0\0\x01\0\0\0\x01\0", 0), Some(2));
        for at in 0..24 {
            let mut arr = [0u8; 24];
            if let Some(b) = arr.get_mut(at) {
                *b = SOH;
            }
            assert_eq!(find_soh(&arr, 0), Some(at), "SOH at {at} over NUL");
        }
    }

    #[test]
    fn two_adjacent_sohs_report_the_first() {
        for at in 0..23 {
            let mut arr = [b'x'; 24];
            for b in arr.iter_mut().skip(at).take(2) {
                *b = SOH;
            }
            assert_eq!(find_soh(&arr, 0), Some(at), "SOHs at {at} and {}", at + 1);
            assert_eq!(find_soh(&arr, at + 1), Some(at + 1), "from the second");
        }
    }

    #[test]
    fn from_past_the_end_is_none() {
        let buf = b"8=FIX\x01";
        // The control: without it a function that always says `None` passes.
        assert_eq!(find_soh(buf, 5), Some(5));
        assert_eq!(find_soh(buf, 6), None, "from == len");
        assert_eq!(find_soh(buf, 7), None, "from == len + 1");
        assert_eq!(find_soh(buf, usize::MAX), None, "from == usize::MAX");
    }
}
