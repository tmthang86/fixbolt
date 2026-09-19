# An oracle absent on disk is not an oracle absent upstream

`[measured 2026-09-19]` — phase 2 plan, row B2, before it was built.
[Plan](../plans/2026-09-19-phase-2-fixt-and-sbe.md) · decided in
[ADR-0082](../decisions/ADR-0082-ten-field-types-one-field-spelled-two-ways-one-empty-component-and-where-the-sp2-oracle-comes-from.md)
decision 3 · guarded by `crates/dict/tests/interop_quickfix_order.rs`'s SP2 sibling, which
**panics** with the fetch instruction when `vendor/quickfix/src/C++/fix50sp2/` is absent

## What happened

Row B2 of the phase 2 plan chose between two oracles for SP2 repeating-group order — QuickFIX's
generated C++ (the 730 / 730 pattern), or the declaration order of the XML the table was
generated from — with the condition *"if `vendor/quickfix-src` (cloned by `interop.sh`) has
generated `src/C++/fix50sp2/`"*. On every machine this project runs on, that reads **false**:

| Fact | Measured |
|---|---|
| QuickFIX ships `src/C++/fix50sp2/` at the pin | **yes** — 160 files, 26 413 279 bytes (`git ls-tree -l -r HEAD`) |
| `vendor/quickfix/src/C++/fix50sp2/` exists after `scripts/fetch-quickfix-assets.sh` | **no** — line 50 sparse-checks-out `/src/C++/fix44/` only |
| `vendor/quickfix-src/` exists after `cargo test` or the CI `test` job | **no** — only `scripts/interop.sh` creates it, after a cmake build of libquickfix |

So the condition would have picked the fallback — a test that compares the table to the file
it was generated from, which `DESIGN.md` D3 already says *"proves stability, not correctness"*
— for a reason that had nothing to do with whether the stronger oracle exists.

## Why it is a trap

The condition tested **where the oracle was not** (a directory another script builds, for
another purpose) instead of **whether the oracle exists** (a `ls-tree` at the pin). A row that
says "if X is on disk, use the strong check, else the weak one" degrades silently: the weak
branch is green, the number it prints looks like the strong branch's number, and nobody reads
the sentence in the test doc that says which branch ran. It is the shape of
[a-test-that-skipped-itself-on-every-machine-that-ran-it](a-test-that-skipped-itself-on-every-machine-that-ran-it.md),
one step earlier — the skip was written into the plan.

## The rule

- **A plan row never tests for an oracle's presence on disk.** It names where the oracle
  comes from (which script, which line, which pin) and the test fails loudly when it is
  absent. The existing `interop_quickfix_order.rs` line 47–60 already does this for `fix44`;
  the SP2 test does the same beside it.
- **`vendor/quickfix-src` is `interop.sh`'s working tree, not a fetch target.** Anything a
  `dict` or `conformance` test reads lives under `vendor/quickfix/`, put there by
  `scripts/fetch-quickfix-assets.sh` at the pinned SHA, and the sparse-checkout list on line
  49–52 of that script is the single place that says what is on disk.
- **Two oracles of different strength are not a fallback pair.** If the strong one can be
  fetched, fetch it and delete the weak branch; if it cannot, say so in the ADR and keep only
  the weak one, labelled.

## What it cost

Measured, not inferred: the 26.4 MB the pattern adds per fresh fetch, from `ls-tree -l`. Not
measured: the seconds it adds to the CI fetch step — ADR-0082 says the plan's delivery log
records that from the first CI run after the change, before and after.
