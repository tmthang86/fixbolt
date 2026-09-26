//! Generates `$OUT_DIR/fix44.rs` from the QuickFIX FIX 4.4 XML dictionary, and
//! — behind the `fix50sp2` feature — `$OUT_DIR/fixt11_fix50sp2.rs` from the
//! **pair** `FIXT11.xml` + `FIX50SP2.xml` (ADR-0080 decision 2, as narrowed by
//! ADR-0083).
//!
//! The dictionaries ship inside this crate, at `spec/`, byte-identical to
//! QuickFIX at a pinned commit and held there under `NOTICE`
//! ([ADR-0104](../../docs/decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md));
//! `NANOFIX_FIX44_XML`, `NANOFIX_FIXT11_XML` and `NANOFIX_FIX50SP2_XML`
//! override the location for a caller who brings their own copy. No network
//! access and no external toolchain are needed to build this crate
//! (`CLAUDE.md` §2 non-negotiable 6). When a dictionary is absent the build
//! fails loudly and says so. It never falls back to a stub: a dictionary that
//! silently becomes empty is a parser that silently stops validating.
//!
//! Traps this generator is written against are recorded in
//! `docs/reference/fix44-dictionary-traps.md` and
//! `docs/reference/fixt-dictionary-traps.md`. Four matter here:
//!   * a DATA field's length field is NOT `tag - 1` — Signature(89) takes
//!     SignatureLength(93). Matching is by name, with one named exception
//!     (ADR-0083 decision 5).
//!   * `<message>` may be self-closing (`XMLnonFIX`), so "has children" is not
//!     the same as "exists".
//!   * one field is spelled `DATA` in one file and `XMLDATA` in the other, so
//!     the pair build compares the `FieldType` **variant**, never the XML
//!     spelling (ADR-0083 decision 2).
//!   * `<component name='MsgTypeGrp' />` is empty in `FIXT11.xml` and full in
//!     `FIX50SP2.xml`, so components merge across the pair and an empty
//!     declaration loses to a full one (ADR-0083 decision 4).
//!
//! The generator itself lives in `src/codegen/`, and this script loads it by
//! `#[path]` — the shape `crates/sbe-gen` uses for its generator (ADR-0081),
//! and ADR-0207 decision 1: one generator, loaded here to write this crate's
//! own tables and exposed behind the `codegen` feature for a user's
//! `build.rs`. What stays here is what only a build script does: find the
//! file, honour the `NANOFIX_*_XML` overrides, print the `cargo:` lines, and
//! turn a refusal into a failed build.

use std::path::{Path, PathBuf};

/// `NANOFIX_FIX44_XML` overrides the location, for packagers and for CI runs
/// that place the asset elsewhere. The default is the copy shipped inside
/// this crate at `spec/FIX44.xml` (ADR-0104) — a relative path resolved
/// against the package root, which is where cargo runs a build script.
const OVERRIDE: &str = "NANOFIX_FIX44_XML";
const DEFAULT: &str = "spec/FIX44.xml";

/// The transport half of a FIXT 1.1 session: header, trailer and the eight
/// admin messages. Same override pattern as `NANOFIX_FIX44_XML`.
#[cfg(feature = "fix50sp2")]
const FIXT_OVERRIDE: &str = "NANOFIX_FIXT11_XML";
#[cfg(feature = "fix50sp2")]
const FIXT_DEFAULT: &str = "spec/FIXT11.xml";

/// The application half: fields, components, groups and 156 messages.
#[cfg(feature = "fix50sp2")]
const SP2_OVERRIDE: &str = "NANOFIX_FIX50SP2_XML";
#[cfg(feature = "fix50sp2")]
const SP2_DEFAULT: &str = "spec/FIX50SP2.xml";

fn main() {
    println!("cargo:rerun-if-env-changed={OVERRIDE}");
    let path = spec_path(OVERRIDE, DEFAULT);
    println!("cargo:rerun-if-changed={}", path.display());
    // The type table is emitted from `FieldType::from_xml`, so editing that
    // file must regenerate. Without this line a new variant compiles into the
    // crate and never reaches the generated table.
    println!("cargo:rerun-if-changed=src/field_type.rs");
    // The generator is no longer this file, so editing it must regenerate too.
    // A directory is scanned recursively.
    println!("cargo:rerun-if-changed=src/codegen");

    let text = read_spec(&path, "FIX 4.4 dictionary", OVERRIDE);
    let doc = parse_spec(&path, &text);
    write_generated("fix44.rs", &or_die(codegen::fix44_from_document(&doc)));

    // ---- the FIXT 1.1 / FIX 5.0 SP2 pair, behind `fix50sp2` ---------------
    // **Read only when the feature is on.** `CLAUDE.md` §2 item 6: with the
    // feature off nothing below is even compiled, so a machine that never
    // fetched `FIXT11.xml` is not asked whether it exists. A build script sees
    // its package's features as `cfg`s exactly as the library does, so this
    // gate and the one on `codegen::pair_from_documents` are the same switch
    // (`[measured 2026-09-26]` a build script printing under
    // `#[cfg(feature = "x")]` printed with `--features x` and not without).
    #[cfg(feature = "fix50sp2")]
    pair();
}

#[cfg(feature = "fix50sp2")]
fn pair() {
    println!("cargo:rerun-if-env-changed={FIXT_OVERRIDE}");
    println!("cargo:rerun-if-env-changed={SP2_OVERRIDE}");
    let transport_path = spec_path(FIXT_OVERRIDE, FIXT_DEFAULT);
    let app_path = spec_path(SP2_OVERRIDE, SP2_DEFAULT);
    println!("cargo:rerun-if-changed={}", transport_path.display());
    println!("cargo:rerun-if-changed={}", app_path.display());

    let transport_text = read_spec(&transport_path, "FIXT 1.1 dictionary", FIXT_OVERRIDE);
    let app_text = read_spec(&app_path, "FIX 5.0 SP2 dictionary", SP2_OVERRIDE);
    let transport = parse_spec(&transport_path, &transport_text);
    let app = parse_spec(&app_path, &app_text);
    let mut warnings = Vec::new();
    let generated = codegen::pair_from_documents(&transport, &app, &mut warnings);
    // Printed before the result is looked at, so a warning is not lost when a
    // later refusal stops the build.
    for w in &warnings {
        println!("cargo:warning={w}");
    }
    write_generated("fixt11_fix50sp2.rs", &or_die(generated));
}

/// Where a dictionary lives: the override if it is set, the copy shipped in
/// `spec/` otherwise.
fn spec_path(var: &str, default: &str) -> PathBuf {
    match std::env::var(var) {
        Ok(p) => PathBuf::from(p),
        Err(_) => PathBuf::from(default),
    }
}

/// Reads a dictionary, or dies saying exactly what is missing.
fn read_spec(path: &Path, what: &str, var: &str) -> String {
    if !path.exists() {
        die(&format!(
            "{what} not found at {}\n\n  This file ships inside the fixbolt-dict crate, at\n\
             crates/dict/spec/ (docs/decisions/ADR-0104-the-published-dictionary-is-\n\
             quickfixs-xml-shipped-with-a-notice.md) — a missing copy means a broken\n\
             checkout or a broken package, not something to fetch.\n\
             Set {var} to build against a dictionary of your own instead.",
            path.display()
        ));
    }
    match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => die(&format!("cannot read {}: {e}", path.display())),
    }
}

fn parse_spec<'i>(path: &Path, text: &'i str) -> roxmltree::Document<'i> {
    match roxmltree::Document::parse(text) {
        Ok(d) => d,
        Err(e) => die(&format!("{} is not well-formed XML: {e}", path.display())),
    }
}

fn write_generated(name: &str, body: &str) {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));
    if let Err(e) = std::fs::write(out.join(name), body) {
        die(&format!("cannot write {name}: {e}"));
    }
}

// The same file `src/field_type.rs` compiles into the crate. Included by path
// rather than copied, because "which XML type name is which variant" is one
// rule and `CLAUDE.md` §4 gives it one place. `build.rs` cannot `use` the crate
// it builds, so this is how the two stay in step. `src/codegen/` names it as
// `crate::field_type`, which is this module here and the crate's own private
// module there.
#[path = "src/field_type.rs"]
#[allow(dead_code)]
mod field_type;

// The generator, loaded the way `crates/sbe-gen/build.rs` loads its own. Its
// public surface is for a user's `build.rs` and goes unused here.
#[path = "src/codegen/mod.rs"]
#[allow(dead_code)]
mod codegen;

/// A refusal from the generator stops the build with its sentence, exactly as
/// the `die` calls that were here did.
fn or_die(generated: Result<String, codegen::GenError>) -> String {
    match generated {
        Ok(text) => text,
        Err(e) => die(&e.to_string()),
    }
}

fn die(msg: &str) -> ! {
    eprintln!("\nfixbolt-dict: {msg}\n");
    std::process::exit(1)
}
