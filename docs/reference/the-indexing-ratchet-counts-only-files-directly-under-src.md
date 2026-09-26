# The indexing ratchet counts only files directly under `src/`

`[measured 2026-09-26]`

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

## What holds those files today

Not the ratchet: the lint. `indexing_slicing = "deny"` applies to every file of every crate, and
the generator is compiled under it on **every** clippy run whatever the features, because
`crates/dict/build.rs` loads `src/codegen/` by `#[path]` and the file-wide allow `build.rs` used
to carry is gone. `uring.rs` is linted in the `feature-sets` job's powerset. A new index in either
place fails clippy, which is stronger than the ratchet — the ratchet exists for the 176 old sites
that carry a per-file `allow`, and a subdirectory file with such an `allow` would be invisible to
it.

## Regression test

**None yet — this is an open item.** Deciding whether the ratchet should recurse (and under
which features it should count) is a separate decision, and changing the regex or the ceiling was
deliberately left out of step 19. Until it is decided, a per-file `#![allow(clippy::indexing_slicing)]`
added to a file under a subdirectory of `src/` would put debt back where no gate counts it;
`scripts/check-no-crate-root-allow.sh` sees only crate roots, not that file.

## The transferable half

A counting gate states its scope in a regex that nobody rereads. When code moves, ask the gate
what it can see, by running its own invocation with the pattern widened, before believing that
an unchanged count means unchanged debt.
