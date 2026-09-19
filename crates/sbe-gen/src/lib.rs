//! Turns an SBE 1.0 XML schema into the `&'static` tables `fixbolt_sbe::
//! Schema` reads (ADR-0081 decisions 1 and 2).
//!
//! A user brings their own venue schema by listing this crate under their
//! own `[build-dependencies]` and calling [`generate`] or
//! [`generate_with_includes`] from their own `build.rs`, the way
//! `crates/dict` turns the QuickFIX FIX 4.4 XML into tables at *this*
//! repository's build time. Neither this repository's schemas nor a venue's
//! ever need to be committed here.
//!
//! ```no_run
//! fn main() -> Result<(), fixbolt_sbe_gen::Error> {
//!     let xml = std::fs::read_to_string("my-venue-schema.xml").unwrap();
//!     let generated = fixbolt_sbe_gen::generate(&xml)?;
//!     let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
//!     std::fs::write(out.join("schema.rs"), generated).unwrap();
//!     Ok(())
//! }
//! ```
//!
//! This crate is not on any hot path and is not a dependency of
//! `fixbolt-sbe` itself — ADR-0081 decision 1 keeps `sbe` at zero runtime
//! dependencies. `roxmltree` is this crate's one normal dependency, exactly
//! as `crates/dict` uses it as a build dependency.
//!
//! Scope, error shape and the generated source's naming rule are documented
//! on [`generate`], [`generate_with_includes`] and [`Error`], and in detail
//! on the `generator` module the two are re-exported from.

mod generator;

pub use generator::{Error, generate, generate_with_includes};
