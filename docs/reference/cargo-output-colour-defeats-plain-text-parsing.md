# cargo output colour defeats plain-text parsing

`.github/workflows/ci.yml` sets `CARGO_TERM_COLOR: always` at the top of the file, for a human
reading a run's log. Every script that parses `cargo`'s (or a cargo plugin's) stdout as plain
text is reading a DIFFERENT string on CI than it is on a terminal that has not set that variable
— cargo wraps words like `Compiling`, `Checking`, `Checked` in ANSI escape codes, and a `grep`,
`sed` or Python regex written and tested against an uncoloured run matches none of them on CI.
The failure is silent in the worst direction: the tool being wrapped still runs and still exits
0 (or whatever it would have exited), so nothing about the underlying check is wrong — only the
wrapper's own reading of it goes blind, and a blind wrapper that no longer sees any input tends
to report "nothing found" rather than crash, which reads as a plausible-looking FAIL rather than
an obviously-broken tool.

**`--color` on the command line overrides `CARGO_TERM_COLOR` in the environment.** That is the
whole fix, every time: pass `--color never` on the exact invocation being parsed, not on some
other cargo call nearby, and not by unsetting the environment variable (a script that unsets
`CARGO_TERM_COLOR` for itself does nothing about `cargo`'s own auto-detection re-enabling colour
when stdout happens to be a TTY locally, which is exactly the environment this repository's own
CI is NOT, but a developer's terminal often IS — `--color never` is unconditional either way).

## Bitten twice

1. **`scripts/check-indexing-debt.sh`** (see the script itself, lines 67–75, dated
   `[measured 2026-09-08]`): `cargo clippy --message-format short` under
   `CARGO_TERM_COLOR: always` wrapped every line in ANSI escapes, the script's own grep matched
   none of them, and the indexing-debt count read **0** on a workspace that actually had 188. The
   zero-guard already in that script is what turned the miscount into a red job instead of a
   silently-passing green one that had stopped counting anything.
2. **`scripts/stranger-check.sh`'s `--from git` mode and `scripts/check-semver-against-tag.sh`**
   (plan [2026-09-23-p3-packaging-and-first-release](../plans/2026-09-23-p3-packaging-and-first-release.md)
   row 8b, senior review of PR #112, findings F1 and F2, `[measured 2026-09-24]`): CI run
   `36015377663`, job "A stranger clones…", failed with
   `FAIL — fixbolt was compiled from <unknown source>` — the source-verification grep in
   `stranger-check.sh` never matched the (coloured) `Compiling fixbolt v0.1.0 (…)` line in the
   build log at all, so the regex that extracts what is inside the parentheses came back empty.
   The same run of `CARGO_TERM_COLOR=always scripts/check-semver-against-tag.sh v0.1.0` on the
   desk read all six published crates as `did not appear in cargo-semver-checks output at all` —
   again a real, successful run of the underlying tool, misread as a total blank by the wrapper
   parsing it.

Two different scripts, two different tools being wrapped (`clippy`, `cargo add` / `cargo build`,
`cargo-semver-checks`), the same one-line environment variable at the top of `ci.yml`, the same
failure shape: a real result silently read as "nothing here."

## Guard

**The reversal that proves this is "run it again with colour forced on."** Every script in this
repository that greps or regex-parses `cargo`'s own output for a specific word (`Compiling`,
`Checking`, `Checked`, `error[…]`, a lint name) now passes `--color never` on that exact
invocation, and its own test suite (or this repository's manual verification, where the script
has no automated test of its own) includes a run under `CARGO_TERM_COLOR=always` that must stay
green:

```sh
CARGO_TERM_COLOR=always scripts/stranger-check.sh --from git --tag v0.1.0        # must stay green
CARGO_TERM_COLOR=always scripts/check-semver-against-tag.sh                     # must stay green (no argument since ADR-0162)
```

Removing the `--color never` flag from either script's cargo invocation, then re-running the
same command above, is the reversal that proves the guard: it reproduces exactly the two FAIL
modes above (`FAIL — fixbolt was compiled from <unknown source>` /
`FAIL semver: <crate> did not appear in cargo-semver-checks output at all`, once per crate).

**What this does NOT guard:** a tool other than `cargo` itself deciding to colour its own output
under some OTHER environment variable this repository has not set (`CLICOLOR_FORCE`, a tool's
own equivalent); a NEW script written later that parses `cargo` output without reading this page
first — there is no lint that greps `scripts/*.sh` for "a `cargo` call whose output is later
grepped, missing `--color never`", so this remains a hand-check, same as `CLAUDE.md` §2's own
table names for rules with no machine-checkable form.

## Sources

- `scripts/check-indexing-debt.sh` lines 67–75, `[measured 2026-08-30]` per its own comment
  (2026-09-08 in *Bitten twice* above is when the fix landed).
- CI run `36015377663` (job "A stranger clones fixbolt from GitHub at the v0.1.0 tag and runs
  it"), senior review of pull request #112, `[measured 2026-09-24]`.
- `cargo --help` / `cargo-semver-checks --help`: both document `--color <auto|always|never>` on
  the command line, and both give the command-line flag precedence over `CARGO_TERM_COLOR` (the
  ordinary CLI-over-environment precedence every `clap`-based Rust CLI follows).
