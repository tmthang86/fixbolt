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
//!
//! # Item 99's capacity test lives here too
//!
//! `Suite::figure` (ADR-0096 decision 1) checks a measured figure against a
//! baseline with exactly this file's `verdict` — one comparator, no copy to
//! drift. The `figure_*` tests below prove the classification reaches the
//! tallies `Suite::finish` reads, so an over-band figure fails the run.
//! What `figure` cannot see is whether the baseline file it reads was kept
//! from moving the bench's own heap (ADR-0096 decision 2): the pinned
//! `journal` binary's `one slot` case read 7.4 ns against a 24 226-byte
//! `benches/baselines.tsv` and 12.4 ns against a 24 897-byte one, same
//! binary, same machine, because `load_baselines`'s buffer used to be sized
//! to the file. `String::with_capacity(1 << 20)` is the fix; the plan's F8
//! sweep is what proves it on the desk, and this file asserts the one thing a
//! sweep cannot check from a laptop — that the fixed capacity was requested.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("../benches/verdict.rs");

// ADR-0096 decision 2 puts item 99's capacity test in this file rather than
// `crates/codec/tests/bench_baselines.rs`: it is a fact about
// `load_baselines`'s own allocation, not about the file-parsing rules that
// file exists to guard. Pulled in by `#[path]` exactly as
// `bench_baselines.rs` and `crates/engine/benches/density.rs` pull it in, so
// this tests the one source the bench binaries compile rather than a copy —
// `dead_code` allowed because this file calls only `load_baselines` and the
// `Suite::figure` seam, never `suite` or `Suite::bench`.
#[allow(dead_code)]
#[path = "../benches/harness.rs"]
mod harness;

use harness::{Suite, load_baselines};

/// One baseline line, `100.0 x 1.10` → band `[90.9, 110.0]`, for the
/// `Suite::figure` tests below.
const FIGURE_BASELINES: &str = "TEST CPU\tfigured case\t100.0\t1.10\t20\t2026-09-23\tpass\n";

/// ADR-0096 decision 1, plan boot F row F5 (c): `Suite::figure` — the seam
/// `crates/engine/benches/wakeup.rs` hands its p50s to — must sort a figure
/// into the same three tallies `Suite::bench` does and nothing else, so a
/// wakeup p50 over its band fails the run exactly as a timed case would.
/// The `verdict` tests above prove the classification; these prove the
/// classification reaches the tally `finish` asserts on.
#[test]
fn figure_in_band_counts_nothing_and_finishes_green() {
    let mut s = Suite::for_test("TEST CPU", FIGURE_BASELINES);
    s.figure("figured case", 105.0);
    assert_eq!(s.tallies(), (0, 0, 0));
    s.finish_for_test();
}

#[test]
fn figure_under_band_is_counted_under_and_does_not_fail_the_run() {
    let mut s = Suite::for_test("TEST CPU", FIGURE_BASELINES);
    s.figure("figured case", 80.0);
    assert_eq!(s.tallies(), (0, 1, 0));
    s.finish_for_test();
}

#[test]
fn figure_without_a_line_is_counted_missing_not_passed() {
    let mut s = Suite::for_test("OTHER CPU", FIGURE_BASELINES);
    s.figure("figured case", 105.0);
    assert_eq!(s.tallies(), (0, 0, 1));
    s.finish_for_test();
}

#[test]
#[should_panic(expected = "1 of 1 case(s) over the machine baseline")]
fn figure_over_band_fails_the_run() {
    let mut s = Suite::for_test("TEST CPU", FIGURE_BASELINES);
    s.figure("figured case", 120.0);
    assert_eq!(s.tallies(), (1, 0, 0));
    s.finish_for_test();
}

/// A file under cargo's per-target scratch directory, one per test so the
/// tests can run in parallel — same helper as `bench_baselines.rs`, kept
/// local because `tests/*.rs` files are each their own crate.
fn write_baselines_file(name: &str, content: &str) -> String {
    let path = format!("{}/bench_verdict-{name}.tsv", env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(&path, content).unwrap();
    path
}

/// ADR-0096 decision 2, `STATUS.md` item 99: a fixed `1 << 20`-byte read
/// buffer makes the one long-lived allocation before a bench's timed closures
/// the same size whatever `benches/baselines.tsv`'s length, so the addresses
/// the closures allocate afterwards stop depending on it (whether glibc
/// serves it from `mmap` or brk — see `load_baselines`'s doc; on the desk it
/// is brk). `String::with_capacity` always returns a buffer of *at least* the
/// requested capacity, regardless of the allocator and of how short the file
/// is, so a tiny fixture is enough to prove the request was made.
///
/// A `load_baselines` that reverted to `std::fs::read_to_string` (which sizes
/// its buffer to the file's own length) would still parse this fixture
/// correctly and pass every test in `bench_baselines.rs` while silently
/// reintroducing the trap `docs/reference/
/// recording-a-baseline-changed-the-baseline.md` already paid for once — the
/// capacity is the only thing that would go red.
#[test]
fn a_loaded_file_reads_into_a_fixed_one_mib_buffer() {
    let path = write_baselines_file("capacity", "# cpu\tcase\tns\tmargin\n");
    let content = load_baselines(&path).expect("a well-formed fixture loads");
    assert!(
        content.capacity() >= 1 << 20,
        "load_baselines returned a buffer of capacity {}, want >= {} (1 << 20)",
        content.capacity(),
        1 << 20
    );
}

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
