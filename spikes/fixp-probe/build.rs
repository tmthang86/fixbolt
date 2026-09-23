//! Generates the probe's layout tables from the schema Artio's referee
//! decodes with (ADR-0140 decisions 1 and 3): `vendor/fixp/binary_entrypoint.xml`,
//! extracted from the pinned `artio-binary-entrypoint-codecs-0.184.jar` and
//! checked against its SHA-256 by `scripts/fixp-spike.sh`. That file is B3's
//! and is never committed, so a missing file is the normal state of a fresh
//! checkout: the build stops and names the script that fetches it.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("fixp-probe build.rs: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let schema = manifest_dir.join("../../vendor/fixp/binary_entrypoint.xml");
    println!("cargo::rerun-if-changed={}", schema.display());

    let xml = std::fs::read_to_string(&schema).map_err(|e| {
        format!(
            "cannot read {} ({e}). It is B3's schema, fetched and SHA-256-checked by \
             scripts/fixp-spike.sh into gitignored vendor/fixp/ — run that script first.",
            schema.display()
        )
    })?;
    let generated = fixbolt_sbe_gen::generate(&xml)
        .map_err(|e| format!("fixbolt-sbe-gen refused {}: {e}", schema.display()))?;

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").map_err(|e| e.to_string())?);
    std::fs::write(out_dir.join("binary_entrypoint.rs"), generated).map_err(|e| e.to_string())
}
