//! The format of the file `codegen::generate` writes (ADR-0207 decision 4).
//!
//! One source, reached two ways: as `codegen`'s own module, where the
//! generator writes the number into every file it emits, and as the hidden,
//! **always compiled** `fixbolt_dict::codegen_format`, which the emitted file
//! checks at compile time (a re-export of this module when `codegen` is on,
//! this file loaded by `lib.rs` when it is off). A user's `build.rs` runs the host copy of
//! `fixbolt-dict` (with `codegen`) and their binary links the target copy
//! (without it); only the second declaration exists in the target copy, and
//! both read this one line.

/// Bumped whenever the text `generate` writes changes in a way a
/// `fixbolt-dict` at another value would not compile, or would read wrongly:
/// a table's shape, a function's signature, an item's name or path.
pub const FORMAT_VERSION: u32 = 1;
