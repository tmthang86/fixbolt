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
///
/// Held in two halves and joined at run time, with a real SOH between them,
/// for two reasons.
///
/// 1. ADR-0089 §3 says the test looks for a **byte sequence**, and a marker
///    written as one string literal is a *spelling*: it is blind to the
///    backslash-newline continuation that wrapped all four original copies,
///    and to `\u{1}` where the source it compares against wrote `\x01`. Every
///    file's text is folded by [`fold_escapes`] before it is searched, and the
///    marker is built from bytes, so all the spellings Rust accepts for SOH
///    match the one marker.
/// 2. This file is walked like every other `.rs` file under the roots below.
///    A marker written here in one piece would make the test name itself as an
///    offender — so rejoining these two halves fails **loudly**, naming this
///    file, rather than quietly carving out an exception that a real copy
///    could then hide behind.
const MARKER_HEAD: &str = "167=BOO";
const MARKER_TAIL: &str = "10=";

/// The one file allowed to contain the marker.
const SOURCE: &str = "crates/codec/benches/fixture.rs";

/// Every directory of this repository that holds Rust source a benchmark can
/// reach. `crates/` and `tools/` are both workspace members — `tools/w2w` is
/// named by `CLAUDE.md` §2's machine-check table as an item-1 instrument, so a
/// copy under `tools/*/benches/` is exactly as damaging as one under
/// `crates/*/benches/`. The repository-root `benches/` holds no `.rs` today
/// (only `baselines.tsv`); it is walked so that a bench added there later is
/// not invisible from the day it lands.
const ROOTS: [&str; 3] = ["crates", "tools", "benches"];

/// A fifth appearance of the message is refused by a test, not by a page in
/// `docs/reference/` asking the next author to grep. Twice that page was
/// written and twice the correction reached one file of four.
///
/// **What this still cannot see.** It is a grep over source text, and the ADR
/// says so: a copy that renames `167=` or reorders the fields slips past, and
/// so does one that never spells the bytes contiguously — assembled from
/// pieces at run time, written per character (`b"\x31\x36\x37=BOO…"`), built
/// by `concat!`, or pulled in by `include_bytes!` from a non-Rust file. It
/// reads `crates/`, `tools/` and the root `benches/` only: a copy in `fuzz/`
/// or `spikes/` (both outside the workspace, `Cargo.toml` `exclude`) is not
/// looked at. It reads the source, not the bytes the bench actually times —
/// only `the_shared_bench_message_parses_clean_under_full_validation` above
/// judges those, and only for `fixture.rs`. It stops the paste, which is the
/// recurrence that actually happened, four times.
#[test]
fn no_bench_carries_its_own_copy_of_the_shared_message() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("the workspace root")
        .to_path_buf();

    let marker = marker();
    let mut scanned = 0usize;
    let mut per_root: Vec<(&str, usize)> = Vec::new();
    let mut offenders: Vec<String> = Vec::new();

    for dir in ROOTS {
        let mut files: Vec<PathBuf> = Vec::new();
        collect_rs(&root.join(dir), &mut files);
        files.sort();
        per_root.push((dir, files.len()));
        for file in &files {
            let rel = file
                .strip_prefix(&root)
                .unwrap_or(file)
                .to_string_lossy()
                .replace('\\', "/");
            scanned += 1;
            let text = fs::read_to_string(file).unwrap_or_default();
            if rel != SOURCE && contains_marker(&text, &marker) {
                offenders.push(rel);
            }
        }
    }

    // A zero that means nothing is the failure mode this repository keeps
    // meeting: if the walk found no files, "no offenders" is not a result.
    // `[measured 2026-09-20]` the tree holds 201 `.rs` files under `crates/`,
    // 8 under `tools/` and none under `benches/` — 209 scanned. The floor is
    // 100 rather than 209 so that adding or deleting a file does not trip it,
    // while losing a whole root — which is how the walk was blind to
    // `tools/w2w/benches/` — cannot pass: `crates/` and `tools/` must each
    // yield at least one file of their own, and the total must still clear the
    // floor.
    let counted = |name: &str| {
        per_root
            .iter()
            .find(|(d, _)| *d == name)
            .map_or(0, |(_, n)| *n)
    };
    for required in ["crates", "tools"] {
        assert!(
            counted(required) > 0,
            "the walk found no .rs file under {}/{required}; it is not looking where it thinks",
            root.display()
        );
    }
    assert!(
        scanned >= 100,
        "the walk scanned only {scanned} .rs files under {}; it is not looking where it thinks",
        root.display()
    );
    let source = root.join(SOURCE);
    assert!(
        contains_marker(&fs::read_to_string(&source).unwrap_or_default(), &marker),
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

/// The marker as the bytes it is on the wire: `MARKER_HEAD`, SOH, `MARKER_TAIL`.
fn marker() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MARKER_HEAD.as_bytes());
    out.push(1);
    out.extend_from_slice(MARKER_TAIL.as_bytes());
    out
}

fn contains_marker(text: &str, marker: &[u8]) -> bool {
    fold_escapes(text)
        .windows(marker.len())
        .any(|w| w == marker)
}

/// The file's text with the two spellings that hide the marker folded away:
///
/// * a **string-literal line continuation** — a `\` at the end of a line eats
///   the newline and the next line's leading whitespace, so
///   a literal broken after the last body field's SOH, with `10=098\x01"` on
///   the next line, is one unbroken message to `rustc` and two separate lines
///   to a naive grep. That is how all four original copies were wrapped;
/// * every escape Rust accepts for **SOH**: `\x01` and `\u{…}` with any number
///   of leading zeros and `_` separators. A literal SOH byte in the source
///   needs no folding — it is already the byte the marker carries.
///
/// An escaped backslash (`\\`) is stepped over first, so `"…\\"` at the end of
/// a line is not mistaken for a continuation. Nothing else is decoded: this is
/// still a grep, not a Rust lexer, and it is applied to comments and raw
/// strings as readily as to string literals.
fn fold_escapes(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while let Some(rest) = bytes.get(i..) {
        let Some(&b) = rest.first() else { break };
        if b != b'\\' {
            out.push(b);
            i += 1;
            continue;
        }
        if rest.starts_with(br"\\") {
            out.extend_from_slice(br"\\");
            i += 2;
            continue;
        }
        if rest.starts_with(br"\x01") {
            out.push(1);
            i += 4;
            continue;
        }
        if let Some((len, value)) = unicode_escape(rest) {
            if value == 1 {
                out.push(1);
            } else {
                out.extend_from_slice(rest.get(..len).unwrap_or_default());
            }
            i += len;
            continue;
        }
        if let Some(len) = continuation(rest) {
            i += len;
            continue;
        }
        out.push(b'\\');
        i += 1;
    }
    out
}

/// `\u{…}` at the head of `rest`: its length in bytes and the value it spells.
fn unicode_escape(rest: &[u8]) -> Option<(usize, u32)> {
    if !rest.starts_with(br"\u{") {
        return None;
    }
    let mut value: u32 = 0;
    let mut digits = 0usize;
    let mut i = 3usize;
    loop {
        let &b = rest.get(i)?;
        i += 1;
        match b {
            b'}' => break,
            b'_' => continue,
            _ => {
                let digit = char::from(b).to_digit(16)?;
                value = value.checked_mul(16)?.checked_add(digit)?;
                digits += 1;
                if digits > 6 {
                    return None;
                }
            }
        }
    }
    (digits > 0).then_some((i, value))
}

/// A `\` line continuation at the head of `rest`: how many bytes it swallows.
fn continuation(rest: &[u8]) -> Option<usize> {
    let mut i = 1usize;
    if rest.get(i) == Some(&b'\r') {
        i += 1;
    }
    if rest.get(i) != Some(&b'\n') {
        return None;
    }
    i += 1;
    while matches!(rest.get(i), Some(&(b' ' | b'\t' | b'\r' | b'\n'))) {
        i += 1;
    }
    Some(i)
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
