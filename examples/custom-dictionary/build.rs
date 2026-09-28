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
    // region:generate
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
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    std::fs::write(out_dir.join("venue.rs"), rust)?;
    // endregion:generate

    // For `tests/plain_is_fix44.rs` only — not part of the example, and
    // `src/lib.rs` does not include it. A type generated from an overlay that
    // adds nothing, through the same template and the same facade paths as
    // `Venue`, which that test holds to `Fix44` answer for answer (ADR-0207
    // *Consequences*: that is what carries `Fix44`'s 59 / 59 to a generated
    // type).
    let plain = codegen::generate(
        Source::Fix44Overlay("<fix type='FIX' major='4' minor='4' servicepack='0'></fix>"),
        "Plain",
        Paths::facade(),
    )?;
    std::fs::write(out_dir.join("plain.rs"), plain)?;
    Ok(())
}
