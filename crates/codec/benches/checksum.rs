//! The FIX checksum on its own — phase 4 row 8, plan
//! `docs/plans/2026-09-27-p4-simd.md` step 1, ADR-0211.
//!
//! A target of its own rather than three more cases in `parse.rs`: new code in
//! an existing binary moves the code already there (ADR-0049 holds a case's
//! layout only to about 4%, and `STATUS.md` item 101 closed on a case moved by
//! nothing but its binary's own layout), so adding here could shift the
//! committed baselines of `parse` and `serialize`. As a
//! separate target, those two binaries do not change by one byte — the plan
//! checks that by sha256.
//!
//! `checksum NewOrderSingle` is the case the row is judged on: every byte of the
//! shared fixture before `10=`. The other two are recorded, not judged —
//! `Heartbeat` is the smallest frame a session sends, and `1 KiB` shows how the
//! loop scales with width.
//!
//! No figure from this file is a claim until a baseline row exists for it,
//! recorded on the `DESIGN.md` §9 machine (plan step 6). Until then every case
//! prints `NO BASELINE`, which is not a pass, and a figure printed on the
//! desktop line is a rehearsal figure, never quoted (non-negotiable 10).
#[path = "harness.rs"]
mod harness;

/// The shared `NewOrderSingle`, ADR-0089: one source, included by path.
#[path = "fixture.rs"]
mod fixture;

use fixbolt_codec::{checksum, format_checksum};
use std::hint::black_box;

/// Every byte of `msg` up to and including the SOH before its `10=` field —
/// exactly what FIX sums. Empty when there is no trailer, so the sum assertion
/// below goes red on a frame that is not one rather than timing nothing.
fn frame_before_trailer(msg: &[u8]) -> &[u8] {
    msg.windows(4)
        .rposition(|w| w == b"\x0110=")
        .and_then(|soh| msg.get(..=soh))
        .unwrap_or_default()
}

fn main() {
    harness::suite(|b| {
        fixture::assert_valid();
        let nos = frame_before_trailer(fixture::NEW_ORDER_SINGLE);

        // The Heartbeat `parse.rs` times, frame only; its trailer was `10=226`.
        let hb: &[u8] = b"8=FIX.4.4\x019=51\x0135=0\x0134=2\x0149=TW44\x01\
52=00000000-00:00:00.000\x0156=ISLD\x01";

        // 1 KiB of printable ASCII, `A..=Z` repeated. A stack array: codec has
        // no `alloc`, and this bench takes none either.
        let mut kib = [0u8; 1024];
        for (byte, letter) in kib.iter_mut().zip((b'A'..=b'Z').cycle()) {
            *byte = letter;
        }

        // **Assert the input, not the output** (ADR-0089), and prove each path
        // is live: the sum of the exact bytes each case times, against a value
        // computed outside this crate (Python `sum(frame) % 256`), so a case
        // that summed a wrong or empty slice cannot print a figure.
        assert_eq!(
            format_checksum(checksum(nos)),
            *b"097",
            "checksum NewOrderSingle: the {}-byte frame before `10=` must sum to 097",
            nos.len()
        );
        assert_eq!(
            format_checksum(checksum(hb)),
            *b"226",
            "checksum Heartbeat: the {}-byte frame must sum to 226",
            hb.len()
        );
        assert_eq!(
            format_checksum(checksum(&kib)),
            *b"176",
            "checksum 1 KiB: A..=Z repeated over 1024 bytes must sum to 176"
        );

        b.bench("checksum NewOrderSingle", || {
            black_box(checksum(black_box(nos)));
        });
        b.bench("checksum Heartbeat", || {
            black_box(checksum(black_box(hb)));
        });
        b.bench("checksum 1 KiB", || {
            black_box(checksum(black_box(&kib)));
        });
    });
}
