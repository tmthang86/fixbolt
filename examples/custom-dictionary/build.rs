//! Generates the venue's dictionary into `$OUT_DIR/venue.rs` (ADR-0207
//! decisions 3 and 4): `venue.xml` merged onto the FIX 4.4 that `fixbolt-dict`
//! ships, written as a zero-sized type `Venue` whose paths go through the
//! `fixbolt` facade. `src/lib.rs` includes it.
//!
//! A refused overlay stops the build with the generator's own sentence, which
//! names both sides of a conflict. Nothing here panics: the error is returned
//! from `main`, and cargo prints it.

use fixbolt_dict::codegen::{self, Paths, Source};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=venue.xml");
    let overlay = std::fs::read_to_string("venue.xml")?;
    let source = Source::Fix44Overlay(&overlay);
    // What `codegen`'s own documentation recommends: a high custom tag widens
    // every per-tag bitset, so the build says how large the tables came out.
    println!(
        "cargo:warning={}",
        codegen::merged_model(source)?.table_size()
    );
    let rust = codegen::generate(source, "Venue", Paths::facade())?;
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("venue.rs");
    std::fs::write(out, rust)?;
    Ok(())
}
