//! What the generator returns where `build.rs` used to `die` (ADR-0207
//! decision 1).

use std::fmt;

/// Why the generator produced no Rust source.
///
/// Hand-written `Display` and `std::error::Error`, the shape
/// `fixbolt_sbe_gen::Error` already has (ADR-0081): this crate has no
/// `thiserror`, and the generator is not on any hot path. `#[non_exhaustive]`
/// so a variant can be added without a breaking change.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GenError {
    /// The dictionary text is not well-formed XML; carries the parser's
    /// description.
    Xml(String),
    /// The dictionary parsed, but the generator refuses to turn it into
    /// tables. The sentence says why, and is the one `build.rs` prints after
    /// `fixbolt-dict: ` when it stops the build —
    /// `scripts/check-dict-refuses-a-message-without-msgcat.sh` reads three of
    /// them word for word.
    Dictionary(String),
    /// The generator does not produce this output yet.
    Unsupported,
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(msg) => write!(f, "not well-formed XML: {msg}"),
            Self::Dictionary(msg) => f.write_str(msg),
            Self::Unsupported => f.write_str("the generator does not support this yet"),
        }
    }
}

impl std::error::Error for GenError {}

/// `build.rs`'s old `die`, as a value: `return refuse(…)` in any function
/// returning `Result<_, GenError>`.
pub(crate) fn refuse<T>(msg: impl Into<String>) -> Result<T, GenError> {
    Err(GenError::Dictionary(msg.into()))
}
