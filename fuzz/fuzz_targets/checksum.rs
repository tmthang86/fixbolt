//! `checksum` must equal the byte-at-a-time sum on every input — ADR-0211,
//! plan `docs/plans/2026-09-27-p4-simd.md` step 3.
//!
//! The SWAR version adds eight lanes at once with no carry between them; the
//! reference below is the loop it replaced. Checked on the input and on each of
//! its first eight suffixes, so every length meets the word loop at every phase.
//!
//! Run: `cargo +nightly fuzz run checksum -- -max_total_time=600`

#![no_main]

use fixbolt_codec::checksum;
use libfuzzer_sys::fuzz_target;

fn byte_loop(bytes: &[u8]) -> u8 {
    let mut sum: u8 = 0;
    for &b in bytes {
        sum = sum.wrapping_add(b);
    }
    sum
}

fuzz_target!(|data: &[u8]| {
    for start in 0..8 {
        let Some(tail) = data.get(start..) else {
            break;
        };
        assert_eq!(
            checksum(tail),
            byte_loop(tail),
            "{} bytes from offset {start}",
            tail.len()
        );
    }
});
