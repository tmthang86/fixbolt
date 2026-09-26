//! The FIX dictionary generator, as a library (ADR-0207).
//!
//! **Skeleton.** Every entry point returns [`GenError::Unsupported`] until the
//! generator moves here from `build.rs` (plan
//! `docs/plans/2026-09-26-docs-for-embedders.md`, step 18);
//! `tests/gen_matches_build.rs` is red on its assertion until then.
//!
//! One generator, three callers (ADR-0207 decision 1). It is loaded twice, the
//! way `fixbolt-sbe-gen` is (ADR-0081): by this crate's `build.rs` through
//! `#[path]`, to write the tables of [`crate::Fix44`] and, behind `fix50sp2`,
//! of the FIXT 1.1 / FIX 5.0 SP2 pair; and as this module, behind the
//! off-by-default `codegen` feature, for a user's own `build.rs`.
//!
//! - [`fix44_tables`] and `fixt11_fix50sp2_tables` (behind `fix50sp2`) are what `build.rs`
//!   writes to `$OUT_DIR/fix44.rs` and `$OUT_DIR/fixt11_fix50sp2.rs`, byte
//!   for byte (decision 2).
//! - [`generate`] is the user's door: an overlay onto the shipped FIX 4.4, or a
//!   whole FIX 4.4 file (decision 3), into one Rust file holding the user's own
//!   zero-sized type (decision 4).

mod error;

pub use error::GenError;

/// What a user's dictionary is made from (ADR-0207 decision 3).
///
/// Both shapes are QuickFIX-format XML text. The first version is FIX 4.4 only
/// (decision 8); `#[non_exhaustive]` so a later plan can add overlays onto the
/// FIXT 1.1 + FIX 5.0 SP2 pair without a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Source<'a> {
    /// A `<fix>` document merged onto the `spec/FIX44.xml` this crate ships.
    /// Its `<header>`, `<messages>`, `<components>` and `<fields>` sections
    /// are each optional; it may add, never remove or retype.
    Fix44Overlay(&'a str),
    /// A complete QuickFIX FIX 4.4 dictionary, generated as-is.
    Fix44Whole(&'a str),
}

/// How the emitted code names the items it implements and uses (ADR-0207
/// decision 4).
///
/// [`Paths::facade`] (the default) goes through `::fixbolt::dict::…`, so a user
/// depends on `fixbolt` at run time and on `fixbolt-dict` only as a
/// build-dependency. [`Paths::direct`] names `::fixbolt_dict` and
/// `::fixbolt_codec`, for this crate's own tests and for a user who drives the
/// engine without the facade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Paths {
    root: Root,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Root {
    #[default]
    Facade,
    Direct,
}

impl Paths {
    /// Paths through the `fixbolt` facade, `::fixbolt::dict::…`.
    #[must_use]
    pub const fn facade() -> Self {
        Self { root: Root::Facade }
    }

    /// Paths straight to `::fixbolt_dict` and `::fixbolt_codec`.
    #[must_use]
    pub const fn direct() -> Self {
        Self { root: Root::Direct }
    }
}

/// Generates one Rust file for the user to `include!`: the tables, a unit
/// struct named `type_name`, and its `impl Dictionary` and `impl Tables`
/// (ADR-0207 decision 4).
///
/// # Errors
///
/// [`GenError::Unsupported`], for every input, until step 18 of the plan.
pub fn generate(source: Source<'_>, type_name: &str, paths: Paths) -> Result<String, GenError> {
    let _ = (source, type_name, paths.root);
    Err(GenError::Unsupported)
}

/// The FIX 4.4 tables this crate's `build.rs` writes to `$OUT_DIR/fix44.rs`,
/// from the dictionary text `xml` (`spec/FIX44.xml` unless
/// `NANOFIX_FIX44_XML` overrides it).
///
/// This crate's own tables, emitted for `crate::`-relative inclusion; a user's
/// dictionary comes from [`generate`].
///
/// # Errors
///
/// [`GenError::Unsupported`] until step 18 of the plan.
pub fn fix44_tables(xml: &str) -> Result<String, GenError> {
    let _ = xml;
    Err(GenError::Unsupported)
}

/// The FIXT 1.1 / FIX 5.0 SP2 tables this crate's `build.rs` writes to
/// `$OUT_DIR/fixt11_fix50sp2.rs`, from the **pair** of dictionary texts
/// (ADR-0080 decision 2, as narrowed by ADR-0083).
///
/// # Errors
///
/// [`GenError::Unsupported`] until step 18 of the plan.
#[cfg(feature = "fix50sp2")]
pub fn fixt11_fix50sp2_tables(transport: &str, app: &str) -> Result<String, GenError> {
    let _ = (transport, app);
    Err(GenError::Unsupported)
}
