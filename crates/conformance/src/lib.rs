//! The QuickFIX FIX 4.4 session acceptance definitions, run in process.
//!
//! `DESIGN.md` §7 puts this crate **before** the session layer, on the reasoning
//! in `CLAUDE.md` §10: a check proves nothing until something reads it, and a
//! gate written after the thing it gates gets written to fit what was built.
//!
//! Nothing here is on a hot path — it is a measuring instrument. It still obeys
//! non-negotiable 7 (no `unwrap`, `expect` or `panic!` in a library crate),
//! because the workspace lints do not have an opinion about which crate is
//! which, and because a gate that panics on its own bug reports the wrong
//! failure.

pub mod compare;
pub mod echo;
pub mod runner;
pub mod script;
