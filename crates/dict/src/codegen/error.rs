//! What the generator returns where `build.rs` used to `die` (ADR-0207
//! decision 1).

use std::fmt;

/// Why the generator produced no Rust source.
///
/// Hand-written `Display` and `std::error::Error`, the shape
/// `fixbolt_sbe_gen::Error` already has (ADR-0081): this crate has no
/// `thiserror`, and the generator is not on any hot path. `#[non_exhaustive]`
/// so the variants that carry a refusal's sentence can be added without a
/// breaking change.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GenError {
    /// The generator does not produce this output.
    ///
    /// Until the generator moves out of `build.rs` (plan
    /// `docs/plans/2026-09-26-docs-for-embedders.md`, step 18) every entry
    /// point returns this.
    Unsupported,
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => {
                f.write_str("fixbolt-dict: the generator does not support this yet")
            }
        }
    }
}

impl std::error::Error for GenError {}
