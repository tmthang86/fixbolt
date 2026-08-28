//! The `.def` corpus, borrowed from `nanofix-conformance`.
//!
//! This file used to hold its own copy of the loader. It no longer does: the
//! conformance crate owns it, and two loaders that disagree about how to read
//! the same corpus is exactly the failure `CLAUDE.md` §4 means by *one rule,
//! one place*. They **did** disagree — this crate's copy substituted a 21-byte
//! `<TIME>` everywhere, and the corpus's own `9=` values say an `I` line's is
//! 17. See `nanofix_conformance::script::FIXED_TIME_IN`.
//!
//! What is left here is a shape adapter, because these tests were written
//! against `DefLine` and rewriting them to prove the same things a second way
//! would be work with no evidence in it.

// Each test binary compiles this module separately and uses a different part of
// it, so every field looks dead to at least one of them.
#![allow(dead_code)]

use std::path::PathBuf;

use nanofix_conformance::script::{Kind, Step, scenarios};

pub const SOH: u8 = 0x01;

/// The timestamp an `E` line's `<TIME>` becomes — engine output, with
/// milliseconds.
pub const FIXED_TIME: &str = nanofix_conformance::script::FIXED_TIME_OUT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `I` — sent to the engine under test.
    In,
    /// `E` — expected back from it.
    Expect,
}

#[derive(Debug, Clone)]
pub struct DefLine {
    pub file: String,
    pub line_no: usize,
    pub direction: Direction,
    /// Session number from an `I1,` / `E2,` prefix, when present.
    pub session: Option<u32>,
    /// Wire bytes: `<TIME>` substituted, `9=` and `10=` computed if absent.
    pub wire: Vec<u8>,
    pub had_body_length: bool,
    pub had_checksum: bool,
}

/// The 59 FIX 4.4 acceptance definitions.
pub fn definitions_dir() -> PathBuf {
    nanofix_conformance::script::definitions_dir()
}

/// Every `I` and `E` line across the 59 files, normalised.
///
/// Panics rather than skipping when `vendor/` is missing. A suite that quietly
/// runs on zero real messages is worse than one that fails.
pub fn load_all() -> Vec<DefLine> {
    let mut out = Vec::with_capacity(600);
    for s in scenarios().unwrap_or_else(|e| panic!("{e}")) {
        for step in s.steps {
            let direction = match step.kind {
                Kind::Send(_) => Direction::In,
                Kind::Expect(_) => Direction::Expect,
                _ => continue,
            };
            let Some(m) = Step::message(&step) else {
                continue;
            };
            out.push(DefLine {
                file: step.file.clone(),
                line_no: step.line_no,
                direction,
                session: step.session,
                wire: m.wire.clone(),
                had_body_length: m.had_body_length,
                had_checksum: m.had_checksum,
            });
        }
    }
    out
}
