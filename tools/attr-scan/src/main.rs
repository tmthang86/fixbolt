//! `attr-scan` — print every **inner attribute at a crate root**, as the Rust
//! lexer sees it.
//!
//! This exists because `scripts/check-no-crate-root-allow.sh` used to read a
//! crate root with a regular expression. A regex reads a *spelling*; the rule
//! it enforces — `CLAUDE.md` §2 non-negotiable 7 — is about a *meaning*.
//! `[measured 2026-09-12]` `#![/*x*/allow(clippy::unwrap_used)]` is accepted by
//! `rustc`, silences the whole crate, and got past that regex, past
//! `cargo fmt --all --check`, and past `check-lint-config.sh` (which reads
//! `Cargo.toml`, not source). This tool removes the spelling from the question:
//! comments, whitespace and newlines carry no meaning to a lexer, and a string
//! literal is a string, never an attribute.
//!
//! ## Contract
//!
//! Arguments are file paths. For each one, in order:
//!
//! * stdout gets one line per crate-root inner attribute:
//!   `PATH:LINE<TAB>HEAD<TAB>IDENTS`
//!   - `LINE` is the line of the `#`;
//!   - `HEAD` is the first identifier inside the brackets (`allow`, `warn`,
//!     `doc`, `cfg_attr`, `no_std`, …), or `-` if there is none;
//!   - `IDENTS` is every identifier inside the brackets, space separated,
//!     **recursing through nested groups**, so
//!     `cfg_attr(a, cfg_attr(b, allow(x)))` reveals `allow`.
//! * stderr gets one summary line: `attr-scan: N files, M inner attributes`.
//!
//! **A file it cannot read or cannot lex is a refusal, never a pass**: the line
//! `PATH:0<TAB>LEX_ERROR<TAB><err>` (or `READ_ERROR`) goes to stderr and the
//! process exits **2**. The caller is a gate; a gate that cannot see the file
//! must go red, not green. Diagnostics go to stderr on purpose, so they stay
//! visible when a script captures stdout.
//!
//! ## What "top level" means here
//!
//! Only the outermost token stream is walked. The walk never descends into a
//! `Group` other than an attribute's own bracket, so an attribute written
//! inside `mod x { … }` is *not* reported — that is a per-module `allow`, which
//! is `STATUS.md` item 55's subject, not this one's. An **outer** `#[…]` is not
//! reported either: it annotates the item that follows, not the crate.
//!
//! Doc comments count. `//!` lexes to `#![doc = "…"]`, exactly as it does for
//! `rustc`, so every `lib.rs` in this workspace reports at least one attribute.
//! `scripts/check-no-crate-root-allow.sh` leans on that: a total of **0** means
//! this tool is broken, not that the tree is clean.

use std::env;
use std::fs;
use std::process::ExitCode;

use proc_macro2::{Delimiter, TokenStream, TokenTree};

/// Exit code for "this tool could not answer the question" — distinct from the
/// gate's own 0/1, so a script can tell a refusal from a finding.
const EXIT_REFUSED: u8 = 2;

fn main() -> ExitCode {
    let paths: Vec<String> = env::args().skip(1).collect();

    if paths.is_empty() {
        eprintln!("attr-scan: usage: attr-scan FILE...");
        return ExitCode::from(EXIT_REFUSED);
    }

    let mut attributes = 0usize;

    for path in &paths {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{path}:0\tREAD_ERROR\t{e}");
                return ExitCode::from(EXIT_REFUSED);
            }
        };

        let stream: TokenStream = match source.parse() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{path}:0\tLEX_ERROR\t{e}");
                return ExitCode::from(EXIT_REFUSED);
            }
        };

        attributes = attributes.saturating_add(report_inner_attributes(path, stream));
    }

    eprintln!(
        "attr-scan: {} files, {} inner attributes",
        paths.len(),
        attributes
    );

    ExitCode::SUCCESS
}

/// Walk the top-level tokens of one file and print every `#` `!` `[…]` triple.
/// Returns how many were printed.
fn report_inner_attributes(path: &str, stream: TokenStream) -> usize {
    let mut found = 0usize;
    let mut tokens = stream.into_iter().peekable();

    while let Some(tree) = tokens.next() {
        // A crate-root inner attribute is exactly three tokens: `#`, `!`, and a
        // bracketed group. Compared as TOKENS, not as bytes — `# ! [ … ]` and
        // `#!\n[…]` and `#![/*x*/…]` all lex to the same three.
        let TokenTree::Punct(hash) = &tree else {
            // Not a `#`. Note what does NOT happen here: we do not step into a
            // Group, so `mod x { #![allow(…)] }` is out of scope by
            // construction rather than by a filter someone can forget.
            continue;
        };
        if hash.as_char() != '#' {
            continue;
        }
        let line = hash.span().start().line;

        match tokens.peek() {
            // `#!` — an inner attribute. Keep going.
            Some(TokenTree::Punct(bang)) if bang.as_char() == '!' => {}
            // `#[…]` — an OUTER attribute on the next item, not on the crate.
            // Leave the group in the stream; the next loop turn skips it
            // without descending.
            _ => continue,
        }
        tokens.next();

        let group = match tokens.peek() {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket => g.stream(),
            _ => continue,
        };
        tokens.next();

        let mut idents: Vec<String> = Vec::new();
        collect_idents(group, &mut idents);

        let head = match idents.first() {
            Some(first) => first.as_str(),
            None => "-",
        };

        println!("{path}:{line}\t{head}\t{}", idents.join(" "));
        found = found.saturating_add(1);
    }

    found
}

/// Every identifier inside an attribute's brackets, in source order, descending
/// into nested groups. String literals are literals and contribute nothing —
/// that is the whole difference between this and stripping comments before a
/// regex: `#![doc = "#![allow(clippy::unwrap_used)]"]` yields only `doc`.
fn collect_idents(stream: TokenStream, out: &mut Vec<String>) {
    for tree in stream {
        match tree {
            TokenTree::Ident(ident) => out.push(ident.to_string()),
            TokenTree::Group(group) => collect_idents(group.stream(), out),
            TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
}
