//! Generates `$OUT_DIR/examples_rc4.rs` and `$OUT_DIR/car.rs`, the tables
//! `crates/sbe-gen/tests/generated.rs` checks against the spec's own bytes
//! and against `crates/sbe/src/tests.rs`'s hand-written tables.
//!
//! This is test-only plumbing: nothing in `src/lib.rs` includes these files,
//! so a user of this crate who never runs `cargo test -p fixbolt-sbe-gen`
//! never needs `vendor/sbe-*` at all. When it is absent, the crate still
//! builds — but the file this script writes is a single `compile_error!`
//! naming the missing asset, so any test that tries to `include!` it goes
//! red rather than silently green (`docs/plans/2026-09-19-phase-2-fixt-
//! and-sbe.md`, "Sửa kế hoạch lần 4", point 3).
//!
//! Vendor assets are located from `CARGO_MANIFEST_DIR/../../vendor`, never
//! copied and never read outside `build.rs` and this crate's own tests
//! (ADR-0001's rule for QuickFIX's assets, applied again to SBE's by
//! ADR-0081 decision 3).

// Build scripts are not `crates/*/src`: `scripts/check-indexing-debt.sh`
// counts nothing outside it, and `crates/dict/build.rs` carries the same
// allow for the same reason.
#![allow(clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

// The generator core, loaded by path because `build.rs` cannot `use` the
// crate it is building for. `crates/dict/build.rs` does the same with
// `src/field_type.rs`; the generator is deliberately kept in one file so
// this line stays a single `#[path]`, not a tree of them.
#[path = "src/generator.rs"]
#[allow(dead_code)]
mod generator;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("../../vendor");
    let spec_examples = vendor.join("sbe-spec/v1-0-RC4/resources/Examples.xml");
    let ref_schema = vendor.join("sbe-ref/sbe-samples/src/main/resources/example-schema.xml");
    let ref_common_types = vendor.join("sbe-ref/sbe-samples/src/main/resources/common-types.xml");

    println!("cargo:rerun-if-changed={}", spec_examples.display());
    println!("cargo:rerun-if-changed={}", ref_schema.display());
    println!("cargo:rerun-if-changed={}", ref_common_types.display());
    // The generator core itself: a change to how it flattens/emits must
    // regenerate, the same reasoning `crates/dict/build.rs` gives
    // `src/field_type.rs`.
    println!("cargo:rerun-if-changed=src/generator.rs");

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".to_string()));

    write_generated(&out.join("examples_rc4.rs"), &spec_examples, |path| {
        generate_from(path)
    });
    write_generated(&out.join("car.rs"), &ref_schema, |path| {
        generate_car(path, &ref_common_types)
    });
}

/// Writes `path` from `source` via `generate`, or a `compile_error!` naming
/// what is missing — never silently empty and never silently stale.
fn write_generated(
    path: &Path,
    source: &Path,
    generate: impl FnOnce(&Path) -> Result<String, String>,
) {
    let contents = if !source.exists() {
        compile_error("vendor/sbe-* missing: run scripts/fetch-sbe-assets.sh")
    } else {
        match generate(source) {
            Ok(rust) => rust,
            Err(msg) => compile_error(&msg),
        }
    };
    if let Err(e) = std::fs::write(path, contents) {
        die(&format!("cannot write {}: {e}", path.display()));
    }
}

fn generate_from(source: &Path) -> Result<String, String> {
    let xml = std::fs::read_to_string(source)
        .map_err(|e| format!("cannot read {}: {e}", source.display()))?;
    generator::generate(&xml).map_err(|e| format!("{}: {e}", source.display()))
}

fn generate_car(source: &Path, common_types: &Path) -> Result<String, String> {
    let xml = std::fs::read_to_string(source)
        .map_err(|e| format!("cannot read {}: {e}", source.display()))?;
    let base_dir = source.parent().map(Path::to_path_buf).unwrap_or_default();
    let common_types = common_types.to_path_buf();
    generator::generate_with_includes(&xml, |href| {
        // The only include either fixture schema uses is a same-directory
        // relative href, so a full XInclude resolver is out of scope
        // (ADR-0081 decision 5 lists what phase 2 covers, and a general
        // include resolver is not on it); this closure resolves exactly the
        // one shape `example-schema.xml` needs.
        let candidate = base_dir.join(href);
        if candidate == common_types || candidate.file_name() == common_types.file_name() {
            std::fs::read_to_string(&common_types).ok()
        } else {
            std::fs::read_to_string(candidate).ok()
        }
    })
    .map_err(|e| format!("{}: {e}", source.display()))
}

fn compile_error(msg: &str) -> String {
    format!("compile_error!({msg:?});\n")
}

fn die(msg: &str) -> ! {
    eprintln!("\nfixbolt-sbe-gen build.rs: {msg}\n");
    std::process::exit(1)
}
