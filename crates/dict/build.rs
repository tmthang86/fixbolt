//! Generates `$OUT_DIR/fix44.rs` from the QuickFIX FIX 4.4 XML dictionary.
//!
//! The dictionary is not in this repository — ADR-0001 keeps it in gitignored
//! `vendor/`. When it is absent the build fails loudly and names the script that
//! fetches it. It never falls back to a stub: a dictionary that silently becomes
//! empty is a parser that silently stops validating.

use std::path::PathBuf;

/// `NANOFIX_FIX44_XML` overrides the location, for packagers and for CI runs
/// that place the asset elsewhere.
const OVERRIDE: &str = "NANOFIX_FIX44_XML";
const DEFAULT: &str = "../../vendor/quickfix/spec/FIX44.xml";

fn main() {
    println!("cargo:rerun-if-env-changed={OVERRIDE}");

    let path = match std::env::var(OVERRIDE) {
        Ok(p) => PathBuf::from(p),
        Err(_) => PathBuf::from(DEFAULT),
    };
    println!("cargo:rerun-if-changed={}", path.display());

    if !path.exists() {
        eprintln!();
        eprintln!(
            "nanofix-dict: FIX 4.4 dictionary not found at {}",
            path.display()
        );
        eprintln!();
        eprintln!("  run scripts/fetch-quickfix-assets.sh");
        eprintln!();
        eprintln!("It is not committed on purpose: the QuickFIX licence's attribution");
        eprintln!(
            "clause would come with it. See docs/decisions/ADR-0001-relationship-to-quickfix.md."
        );
        eprintln!("Set {OVERRIDE} to use a copy from somewhere else.");
        eprintln!();
        std::process::exit(1);
    }
}
