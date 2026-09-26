# The indexing ratchet counts only files directly under `src/`

`[measured 2026-09-26]` — **fixed the same day**; the trap below is what the script did until then
(*What was done*).

## The trap

`scripts/check-indexing-debt.sh` holds the workspace's panicking-index debt to a ceiling
(CLAUDE.md §2 non-negotiable 7; STATUS.md item 55). It runs clippy with
`--force-warn clippy::indexing_slicing` and counts the sites whose path matches

```
^crates/[a-z0-9_]+/src/[a-zA-Z0-9_]+\.rs:…
```

— one path segment after `src/`. A file in a **subdirectory** of `src/` never matches, so its
sites are never counted, whatever clippy prints about them. It also runs clippy with the default
features (plus `affinity` on Linux), so a module behind any other feature is never compiled for
the count at all.

Step 18 of plan `2026-09-26-docs-for-embedders` moved the dictionary generator into
`crates/dict/src/codegen/`, six files, behind the `codegen` feature — both blind spots at once.

## What was measured

On a Linux container (OrbStack, aarch64, the `rust:1` image, toolchain 1.98.0 — **not** CI's
x86_64 runner), on the tree of step 19, the script's own two clippy invocations, counted with its
regex and with one that recurses into subdirectories:

| Invocation | Script's regex | Recursive |
|---|---:|---:|
| default + `--features affinity` (what the script runs) | 176 | 176 |
| `--all-features` | 176 | 176 |

Seven sources sit in subdirectories of a `crates/*/src/` today — six in `crates/dict/src/codegen/`
and `crates/engine/src/transport/uring.rs` — and **none has a panicking index**, so the blind spot
costs nothing yet. The ceiling stays 176.

## What was done

**Fixed** in review of PR #119: the pattern now takes any depth after `src/`
(`crates/<crate>/src/(<dir>/)*<file>.rs`). The count stayed 176, so the ceiling did not change.
The other blind spot is still there: the count runs clippy with default features (plus `affinity`
on Linux), so a module behind any other feature is compiled for the count only if something else
compiles it. The generator is: `crates/dict/build.rs` loads `src/codegen/` by `#[path]`, so every
clippy run compiles it, and the reversal below is counted through that load.

The lint still does most of the holding. `indexing_slicing = "deny"` applies to every file of every
crate, so a new index in a file with no per-file `allow` fails clippy before the ratchet is asked.
The ratchet is what holds the 176 old sites, which sit in files that carry such an `allow`. Until
this fix, a file in a subdirectory could have carried one and put debt back where nothing counted
it; `scripts/check-no-crate-root-allow.sh` sees only crate roots, not such a file.

## Regression test

The reversal, `[measured 2026-09-26]` on macOS: a temporary `probe[n.children().count() % 2]` in
`crates/dict/src/codegen/parse.rs`, a file in a subdirectory of `src/`, reads

```
crates/dict/src/codegen/parse.rs:99:13: warning: indexing may panic
check-indexing-debt: 177 indexing/slicing sites in crates/*/src, ceiling 176
check-indexing-debt: FAIL — the debt went UP, 176 -> 177.
```

and the tree restored reads `check-indexing-debt: ok`. Under the old pattern the same edit left
the count at 176. The script runs in the `indexing-debt` CI job on every commit. What is not
guarded is a test that runs this reversal automatically: it was run by hand, as above.

## The transferable half

A counting gate states its scope in a regex that nobody rereads. When code moves, ask the gate
what it can see, by running its own invocation with the pattern widened, before believing that
an unchanged count means unchanged debt.
