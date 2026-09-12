# ADR-0061 — The scratch-fixture gate is a regex, and says so

**Status:** Accepted · **Date:** 2026-09-12 · **Plan:** docs/plans/2026-09-12-an-obligation-nothing-checks.md §B

## Context

`scripts/check-scratch-fixtures.sh` guards the class behind
`docs/reference/a-scratch-fixture-inherits-the-machine.md`: a gate that builds a
throwaway crate outside the tree, where `rust-toolchain.toml` does not reach. It is a
line-by-line regex over `bash`. `[measured 2026-09-12]` a senior review got past it four
ways, each a false green: a `cp` inside a heredoc body; a `cp` copying out of the scratch
dir rather than into it; `read -r TMP < <(mktemp -d)`; a bare `TMP=/tmp`. STATUS.md item
69 asked whether to keep the regex or parse `bash`.

The plan that built the sibling gate (`check-no-crate-root-allow.sh`) replaced a regex
with the Rust lexer, because its adversary was legal Rust that ordinary code writes.

## Decision

The gate stays a regex. Its four known false greens are named in its own header, in
`DESIGN.md` §6 and in `CLAUDE.md` §2, and no further regex is added for them.

## Why

- `shellcheck -f json1` — the only parser already on CI — emits diagnostics, not a parse
  tree. `[measured 2026-09-12]` on a file containing all four bypasses it reports two
  unused variables and nothing else.
- Every other parser (`shfmt --to-json`, `bashlex`, `tree-sitter-bash`) is a new binary
  or package on the runner and the desk, a JSON walker, and a version skew — for a gate
  whose adversary is an accidental fixture. `[measured 2026-09-12]` no script in
  `scripts/` writes any of the four shapes (`grep`: 0 hits).
- A real parser still cannot see `eval`, `source`, a `cd` behind a function, or a
  `$(…)` — the list the header already carries. It moves the frontier; it does not
  remove it. A static gate over a shell does not beat an author who is trying.

## Consequences

- Good: no new dependency; the header states what the gate reads and what it cannot,
  and a reader is not told a spelling list is a meaning.
- Bad: the four false greens stay. A fixture written in one of those four ways passes.
  The day a real script in `scripts/` does, this ADR is the pointer: extend the script
  in the same commit (`CLAUDE.md` §4), or supersede this ADR with a parser.
- Bad: item 69's question can be reopened by the fifth spelling. The answer is the same
  until the adversary changes from an accident to a person.

## Alternatives rejected

- `shellcheck -f json1` as a parser — measured, no AST.
- `shfmt --to-json` — rejected in docs/plans/2026-09-12-gates-that-match-a-meaning.md §E
  as one more binary; that reason still holds and json1's emptiness does not change it.
- `bashlex` / `tree-sitter-bash` — same cost, one language further from the scripts.
- `bash`'s own parser through `declare -f` — not measured; its output is text again,
  with heredoc bodies rendered in it, so the regex loop would restart one layer down.
