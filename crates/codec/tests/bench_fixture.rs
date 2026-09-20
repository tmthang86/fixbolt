//! The two guards ADR-0089 puts on the shared bench fixture.
//!
//! `cargo test` never builds a `harness = false` bench, so without this file
//! `crates/codec/benches/fixture.rs` would be checked only by the `bench` CI
//! job. These tests run on every commit.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[path = "../benches/fixture.rs"]
mod fixture;

use std::fs;
use std::path::{Path, PathBuf};

/// The whole point of the fixture: the bytes the benches time are a message the
/// parser accepts, not one it rejects at its last field.
#[test]
fn the_shared_bench_message_parses_clean_under_full_validation() {
    fixture::assert_valid();
}

/// The marker a pasted copy carries. Deliberately the tail of the message —
/// `167=` is its last body field and `10=` its checksum — so a copy that
/// changed the timestamps still trips it.
const MARKER: &str = r"167=BOO\x0110=";

/// The one file allowed to contain [`MARKER`].
const SOURCE: &str = "crates/codec/benches/fixture.rs";

/// A fifth appearance of the message is refused by a test, not by a page in
/// `docs/reference/` asking the next author to grep. Twice that page was
/// written and twice the correction reached one file of four.
///
/// This is a grep, and the ADR says so: a copy that renames `167=` or reorders
/// the fields slips past. It stops the paste, which is the recurrence that
/// actually happened.
#[test]
fn no_bench_carries_its_own_copy_of_the_shared_message() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("the workspace root")
        .to_path_buf();

    let mut scanned = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    for bench in benches_under(&root) {
        let rel = bench
            .strip_prefix(&root)
            .unwrap_or(&bench)
            .to_string_lossy()
            .replace('\\', "/");
        scanned += 1;
        let text = fs::read_to_string(&bench).unwrap_or_default();
        if text.contains(MARKER) && rel != SOURCE {
            offenders.push(rel);
        }
    }

    // A zero that means nothing is the failure mode this repository keeps
    // meeting: if the walk found no benches, "no offenders" is not a result.
    assert!(
        scanned > 4,
        "the walk found only {scanned} bench files under {}; it is not looking where it thinks",
        root.display()
    );
    let source = root.join(SOURCE);
    assert!(
        fs::read_to_string(&source)
            .unwrap_or_default()
            .contains(MARKER),
        "{SOURCE} no longer contains the marker, so this test proves nothing"
    );

    offenders.sort();
    assert!(
        offenders.is_empty(),
        "the shared bench message must live only in {SOURCE}; \
         these carry their own copy: {}",
        offenders.join(", ")
    );
}

/// Every `crates/*/benches/**/*.rs`, `fixture.rs` included.
fn benches_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let crates = match fs::read_dir(root.join("crates")) {
        Ok(d) => d,
        Err(e) => panic!("crates/ must be readable: {e}"),
    };
    for krate in crates.flatten() {
        collect_rs(&krate.path().join("benches"), &mut out);
    }
    out.sort();
    out
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}
