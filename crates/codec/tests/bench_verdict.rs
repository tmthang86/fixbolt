//! The rule that decides every timing gate in `DESIGN.md` §6, tested.
//!
//! Step 1 of [a-baseline-is-a-band], and it is written to be **red**: today
//! `verdict` has two branches and the third — *below the floor* — does not
//! exist, so a case that stopped measuring anything reads far under its limit
//! and passes.
//!
//! # Why this file exists at all
//!
//! `crates/codec/benches/harness.rs` is built with `harness = false`, which
//! makes it a `main()` that `cargo test` never runs. The comparison inside it
//! therefore had **no test**, while deciding whether every timing gate in this
//! repository is green. `benches/verdict.rs` is that comparison on its own, and
//! it is `include!`d by both — one source, two consumers, no copy to drift.
//!
//! # Why the *in band* case is not decoration
//!
//! A test of three branches that only asserts the two interesting ones passes
//! against a `verdict` that answers `Over` and `Under` for everything. The
//! ordinary case is the control, and it is the same lesson as
//! `docs/reference/silence-before-a-logon-has-many-causes.md`.
//!
//! [a-baseline-is-a-band]: ../../../docs/plans/2026-09-01-a-baseline-is-a-band.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("../benches/verdict.rs");

/// The numbers are the real ones from the defect that produced this plan.
///
/// `[measured 2026-09-01]` `inline deliver + reply`: baseline 8.5 ns, margin
/// 1.35 — the widest on the ladder, because that case draws three discrete
/// clusters — and it published **1.3 ns** while doing 8.5 ns of work.
const BASELINE: f64 = 8.5;
const MARGIN: f64 = 1.35;

/// The control. An ordinary measurement passes, and must.
#[test]
fn a_figure_inside_the_band_is_a_pass() {
    assert_eq!(verdict(BASELINE, BASELINE, MARGIN), Verdict::InBand);
    assert_eq!(
        verdict(BASELINE * 1.30, BASELINE, MARGIN),
        Verdict::InBand,
        "just under the ceiling is still a pass"
    );
    assert_eq!(
        verdict(BASELINE / 1.30, BASELINE, MARGIN),
        Verdict::InBand,
        "just over the floor is still a pass"
    );
}

/// A regression. This branch already existed and must keep working.
#[test]
fn a_figure_over_the_ceiling_is_a_regression() {
    assert_eq!(
        verdict(BASELINE * 1.36, BASELINE, MARGIN),
        Verdict::Over,
        "{BASELINE} x {MARGIN} is the ceiling"
    );
}

/// **The specification.** A benchmark that stopped measuring reads far under
/// its baseline, and a ceiling cannot see it.
///
/// 1.3 against a baseline of 8.5 is 6.5x faster than the machine has ever done
/// this work. Under a ceiling-only rule it is not merely a pass — it is the
/// most comfortable pass in the file, and it gets more comfortable every day
/// the baseline is not re-recorded.
#[test]
fn a_figure_under_the_floor_is_neither_a_pass_nor_a_regression() {
    assert_eq!(
        verdict(1.3, BASELINE, MARGIN),
        Verdict::Under,
        "1.3 ns/op against a baseline of 8.5 is the real defect from open item \
         25: `out` was written every iteration and read by nobody, so the \
         optimiser deleted a 163-byte copy. 163 bytes in 1.3 ns is 125 GB/s \
         from one core. A ceiling passes it forever."
    );
    assert_eq!(
        verdict(BASELINE / 1.36, BASELINE, MARGIN),
        Verdict::Under,
        "the floor is baseline / margin, the same margin as the ceiling"
    );
}

/// The band is closed at both ends, and nothing falls between the branches.
#[test]
fn every_figure_lands_in_exactly_one_branch() {
    // A sweep rather than three points: a `verdict` with a gap — say `>` on one
    // side and `>=` on the other — would leave a value with no answer, and a
    // three-point test would step over it.
    for i in 1..=4000u32 {
        let best = f64::from(i) * 0.01;
        let v = verdict(best, BASELINE, MARGIN);
        let expected = if best > BASELINE * MARGIN {
            Verdict::Over
        } else if best < BASELINE / MARGIN {
            Verdict::Under
        } else {
            Verdict::InBand
        };
        assert_eq!(v, expected, "at {best:.2} ns/op");
    }
}
