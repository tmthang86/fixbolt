//! `benches/baselines.tsv` as `crates/codec/benches/harness.rs` reads it, tested.
//!
//! # Why this file exists at all
//!
//! ADR-0067 moved the baselines from `include_str!` to a read at run time, and
//! with it the guarantee that a broken file cannot pass quietly moved from the
//! compiler to `read_baselines`. That function lives in a `harness = false`
//! bench, which `cargo test` never runs, so the guarantee had no test. A senior
//! review of PR #72 then showed the parser accepting `nan`: a margin of `nan`
//! printed `baseline 56.2 xNaN = [NaN, NaN]` and exited 0, because every
//! comparison against `NaN` is false and so no case is ever over or under.
//!
//! The harness is pulled in by `#[path]`, exactly as `crates/engine/benches`
//! pull it in, so this tests the one source the bench binaries compile rather
//! than a copy of it. `dead_code` is allowed on that `mod` item only: this crate
//! calls two of the harness's functions and none of `Suite`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
#[path = "../benches/harness.rs"]
mod harness;

use harness::{data_line_parses, load_baselines};

/// A file under cargo's per-target scratch directory, one per test so that the
/// tests can run in parallel.
fn write(name: &str, content: &str) -> String {
    let path = format!("{}/bench_baselines-{name}.tsv", env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(&path, content).unwrap();
    path
}

const HEADER: &str = "# cpu\tcase\tns\tmargin\tn\tdate\tverdict\n#\n";
const GOOD: &str = "AMD Ryzen 7 3700X 8-Core Processor\tring, one way\t56.2\t1.10\t20\t2026-09-05\tpass 12   fail 0   unknown 0";

/// The control: a header, a blank line and a good data line load, and the
/// content comes back whole. Without it, every rejection below also passes
/// against a loader that rejects everything.
#[test]
fn a_good_file_loads_whole() {
    let content = format!("{HEADER}\n{GOOD}\n");
    let path = write("good", &content);
    assert_eq!(load_baselines(&path), Ok(content));
}

/// The second control: the file the bench binaries actually read today.
#[test]
fn the_committed_baselines_file_loads() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../benches/baselines.tsv");
    if let Err(why) = load_baselines(path) {
        panic!("the committed baselines file is rejected: {why}");
    }
}

#[test]
fn a_missing_file_names_its_path() {
    let path = format!(
        "{}/bench_baselines-does-not-exist.tsv",
        env!("CARGO_TARGET_TMPDIR")
    );
    let why = load_baselines(&path).unwrap_err();
    assert!(
        why.starts_with(&format!("cannot read baselines file {path}: ")),
        "{why}"
    );
}

#[test]
fn a_malformed_ns_names_its_line() {
    let bad = GOOD.replace("\t56.2\t", "\t56.2ns\t");
    // Line 4: two header lines, one good line, then the bad one.
    let path = write("malformed-ns", &format!("{HEADER}{GOOD}\n{bad}\n"));
    let why = load_baselines(&path).unwrap_err();
    assert!(
        why.starts_with(&format!("{path}:4: malformed baseline line (ns ")),
        "{why}"
    );
}

/// The defect the review found. `nan` parses as an `f64`, and a band of
/// `[NaN, NaN]` is neither over nor under anything — so it has to be refused
/// here or it is a case that can never go red.
#[test]
fn a_nan_margin_names_its_line() {
    let bad = GOOD.replace("\t1.10\t", "\tnan\t");
    let path = write("nan-margin", &format!("{HEADER}{bad}\n"));
    let why = load_baselines(&path).unwrap_err();
    assert!(
        why.starts_with(&format!("{path}:3: malformed baseline line (margin ")),
        "{why}"
    );
}

/// Every other value that parses as an `f64` and cannot be a baseline or a
/// margin, each named, with the good line as the control beside them.
#[test]
fn values_that_parse_but_cannot_be_a_baseline_are_refused() {
    assert_eq!(data_line_parses(GOOD), Ok(()));
    for (ns, margin, n) in [
        ("nan", "1.10", "20"),
        ("inf", "1.10", "20"),
        ("-56.2", "1.10", "20"),
        ("0", "1.10", "20"),
        ("56.2", "inf", "20"),
        ("56.2", "-1.10", "20"),
        ("56.2", "0.90", "20"),
        ("56.2", "1.10", "0"),
        ("56.2", "1.10", "-3"),
        ("56.2", "1.10", "20.5"),
        ("56.2", "1.10", "twenty"),
    ] {
        let line = format!("cpu\tcase\t{ns}\t{margin}\t{n}\t2026-09-14\tpass 12");
        assert!(
            data_line_parses(&line).is_err(),
            "ns {ns} margin {margin} n {n} was accepted"
        );
    }
    // A four-column line has no `n` to check, and is still a baseline.
    assert_eq!(data_line_parses("cpu\tcase\t56.2\t1.10"), Ok(()));
    // Margin exactly 1.0 is a zero-width band: useless, but not incoherent.
    assert_eq!(data_line_parses("cpu\tcase\t56.2\t1.0\t20"), Ok(()));
}
