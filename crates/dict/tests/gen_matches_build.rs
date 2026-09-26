//! The public `codegen` door writes what `build.rs` writes, byte for byte.
//!
//! **What this proves:** the two paths into one generator agree —
//! `codegen::fix44_tables(&str)` and `codegen::fixt11_fix50sp2_tables`, which
//! parse the text themselves, against `build.rs`, which reads the file, honours
//! the `NANOFIX_*_XML` overrides, parses, and calls the crate-internal
//! `fix44_from_document` / `pair_from_documents`. A difference in what either
//! path adds around the generator — the parsing, the order of the pair, a
//! warning that became output — turns this red.
//!
//! **What it does not prove:** that the generator's output is unchanged.
//! `build.rs` loads the same `src/codegen/` by `#[path]`, so a change to what
//! the generator emits moves both sides at once and this stays green
//! (`[measured 2026-09-26]` in review of PR #119: editing the `@generated`
//! header in `emit.rs` left this green while `fix44.rs`'s sha256 moved).
//! `tests/generated_is_pinned.rs` pins the output itself.
//!
//! The XML read here is the shipped `spec/` copy. A build run with
//! `NANOFIX_FIX44_XML` (or the FIXT overrides) set generates from another
//! file, and these tests then compare against the wrong input — they are
//! meant for the default build.
#![cfg(feature = "codegen")]
#![allow(clippy::panic)]

use fixbolt_dict::codegen::{self, GenError};

const BUILT_FIX44: &str = include_str!(concat!(env!("OUT_DIR"), "/fix44.rs"));
const SHIPPED_FIX44: &str = include_str!("../spec/FIX44.xml");

/// Fails naming the first line where `generated` and `built` part, rather
/// than printing two files of several hundred kilobytes.
fn assert_same(what: &str, generated: Result<String, GenError>, built: &str) {
    let generated = match generated {
        Ok(text) => text,
        Err(e) => panic!(
            "{what}: the generator returned Err({e:?}) — \"{e}\"; build.rs wrote {} bytes",
            built.len()
        ),
    };
    let first_difference = generated
        .lines()
        .zip(built.lines())
        .enumerate()
        .find(|(_, (g, b))| g != b)
        .map(|(i, (g, b))| format!("line {}:\n  gen:      {g}\n  build.rs: {b}", i + 1));
    assert_eq!(
        first_difference, None,
        "{what}: the generator and build.rs differ"
    );
    assert_eq!(
        generated.len(),
        built.len(),
        "{what}: same lines as far as the shorter goes, different lengths"
    );
}

#[test]
fn generated_fix44_equals_the_build_output() {
    assert_same(
        "fix44.rs",
        codegen::fix44_tables(SHIPPED_FIX44),
        BUILT_FIX44,
    );
}

#[cfg(feature = "fix50sp2")]
#[test]
fn generated_pair_equals_the_build_output() {
    const BUILT_PAIR: &str = include_str!(concat!(env!("OUT_DIR"), "/fixt11_fix50sp2.rs"));
    const SHIPPED_FIXT11: &str = include_str!("../spec/FIXT11.xml");
    const SHIPPED_FIX50SP2: &str = include_str!("../spec/FIX50SP2.xml");
    assert_same(
        "fixt11_fix50sp2.rs",
        codegen::fixt11_fix50sp2_tables(SHIPPED_FIXT11, SHIPPED_FIX50SP2),
        BUILT_PAIR,
    );
}
