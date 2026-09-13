# ADR-0066 — The rustdoc gate denies every warning, not a list of named lints

**Status:** Proposed · **Date:** 2026-09-13 · **Plan:** docs/plans/2026-09-13-what-the-review-left-open.md, steps 1–2
**Supersedes, in part:** [ADR-0065](ADR-0065-every-feature-set-to-depth-two-is-built-linted-and-documented.md)
decision 1 — only the `RUSTDOCFLAGS` value it names. Depth, powerset, `--all-features` and
everything else in ADR-0065 stand unchanged.

## Context

ADR-0065 runs `cargo doc` over every feature set to depth two, and once under
`--all-features`, with
`RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::redundant_explicit_links"`.
Two lints are denied by name; every other rustdoc lint keeps its default level.

STATUS.md item 83: `crates/session/src/clock.rs:57-58` links the public `parse_utc`'s docs to
two private constants. rustdoc's `private_intra_doc_links` is **warn** by default
([rustdoc lints](https://doc.rust-lang.org/rustdoc/lints.html)), so every run of the
`feature-sets` job printed it and passed.

`[measured 2026-09-13]` on the Apple M5 desk, tree `4ea5349`:

- `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::redundant_explicit_links -D rustdoc::private_intra_doc_links" cargo doc --workspace --no-deps`
  → `error: public documentation for parse_utc links to private item LEN_SECONDS`, the same
  for `LEN_MAX`, `exit 101`; the same with `--no-default-features`.
- Every rustdoc warning in the workspace, grouped, is those two and nothing else: host
  default set, host `--no-default-features`, `cargo hack doc --workspace --no-deps
  --feature-powerset --depth 2 --keep-going --target x86_64-unknown-linux-gnu` (32 sets,
  `EXIT=0`), and `cargo doc --workspace --no-deps --all-features --target
  x86_64-unknown-linux-gnu`.
- On a throwaway copy with the two links written as code spans, `RUSTDOCFLAGS="-D warnings"`
  documented the workspace with no diagnostic, default set and `--no-default-features`.
- On the same copy, a bare URL added to `crates/codec/src/lib.rs`'s crate docs
  (`bare_urls`, also warn by default) printed `warning: this URL is not a hyperlink` and
  **finished green under ADR-0065's two flags**, and was `error: this URL is not a hyperlink`
  under `-D warnings`.

So item 83 is one instance, and naming `private_intra_doc_links` closes that instance only:
`bare_urls`, `invalid_html_tags`, `invalid_codeblock_attributes`, `invalid_rust_codeblocks`
and `unportable_markdown` are all warn by default and would each be the next item. That is
the loop ADR-0061 described for text scans — one more pattern per finding.

## Decision

1. **Both rustdoc steps of the `feature-sets` job run under `RUSTDOCFLAGS="-D warnings"`** —
   the `cargo hack doc` powerset and the `--all-features` `cargo doc`. No lint is named.
2. The two lints ADR-0065 named stay denied, because `-D warnings` denies every lint at
   warn level and both are warn by default. They are not repeated in the flag.
3. A rustdoc lint that is **allow** by default (`missing_docs`, `unescaped_backticks`,
   `private_doc_tests`, …) stays allowed. Raising one is a separate decision.

## Consequences

**Good**

- A warning rustdoc prints in that job is a red job. The class "the gate printed it and
  nobody read it" is closed for rustdoc, not just its first instance.
- It matches the clippy half of the same job, which already runs `-D warnings`.

**Bad**

- **A toolchain bump can add a warn-by-default rustdoc lint and turn the job red with no
  commit behind it** — STATUS.md item 19's shape, for rustdoc. The toolchain is pinned
  (`rust-toolchain.toml`), so this arrives only with a deliberate bump, and the red names
  the lint.
- `-D warnings` in `RUSTDOCFLAGS` also denies any rustc warning rustdoc itself emits while
  documenting. The clippy step of the same job already denies those, so nothing new is
  refused today; it is stated so a surprising red from the doc step is not a mystery.
- The desk command in
  [a-linux-only-module-is-invisible-to-a-mac-gate](../reference/a-linux-only-module-is-invisible-to-a-mac-gate.md)
  and the `DESIGN.md` §6 row carry the flag text and must change with it.
