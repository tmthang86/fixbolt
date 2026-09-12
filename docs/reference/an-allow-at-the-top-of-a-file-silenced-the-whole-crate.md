# The lint was denied, the debt was annotated, and the check protected nothing

`[measured 2026-09-08]` · Rust, `clippy` · **`[to testing-skills]`**

## The plan, which was reasonable

A lint worth having fires 207 times in code that already exists. Cleaning it all
in one change is a diff nobody can review, so the standard move is a **ratchet**:

1. set the lint to `deny` for everybody,
2. give each file that already violates it an `allow`, with its count written down,
3. add a script that counts the violations anyway — with `--force-warn`, which
   overrides an `allow` — and fails if the total goes up.

New code in a clean file is stopped by (1). New code in a dirty file is stopped by
(3). That was the design, and it is a good one.

## What actually shipped

Step 2 was done by putting this at the top of each offending file:

```rust
#![allow(clippy::indexing_slicing)]
```

`#!` is an **inner** attribute. At the top of a module file it applies to that
module. At the top of `lib.rs` it applies to **the entire crate** — every
submodule, including the clean ones.

Three of the 21 offending files were `lib.rs`. So three whole crates — the
biggest three, holding most of the code — had the lint silently turned off. The
ratchet's second half still worked, because `--force-warn` overrides an allow. The
*first* half, the deny that was supposed to stop new code, was inert across most
of the codebase.

## How it was found, and how it was nearly not

The reversal was: paste a new violation into a file with no `allow` of its own and
watch the lint reject it.

```
$ # append `fn probe(v: &[u8]) -> u8 { v[0] }` to a clean module
$ cargo clippy -p <crate> --lib ; echo $?
0
```

Exit 0. No warning. The file had no `allow`; its crate root did.

The near-miss is worth recording. The **first** attempt at this reversal was run
inside a shell line containing an unquoted backtick pair in an `echo`. The shell
tried to execute the word between them, printed `deny: command not found`, and the
grep that followed counted `0`. **The number 0 was correct and meant nothing** —
it was the count from a command that had already gone wrong. It was caught only by
reading the whole output rather than the number that was being looked for.

So the sequence was: a broken reversal produced the same answer as the real
defect, and the real defect was confirmed only on the second, correctly quoted
attempt.

## The fix

Replace the crate-root `#![allow]` with attributes scoped to the items that need
them: 13 functions in one `lib.rs`, 2 in another. For generated code pulled in by
`include!`, note that **an `#[allow]` written next to the `include!` does not reach
inside it** — rustc reports it as an unused attribute and the errors stand. The
allow has to be emitted by the generator, onto the generated functions.

After that the reversal bites in all three crates:

```
crates/engine/src/clock.rs      -> 1 error from the deny
crates/session/src/schedule.rs  -> 1 error
crates/codec/src/checksum.rs    -> 1 error
```

## Two more ways the same annotation pass was short, both found by CI

**A feature-gated file is invisible to a run that does not turn the feature on.**
The 21 files were enumerated from one `clippy` run under **default** features.
Three sites in a file behind an off-by-default feature were never in that list,
never annotated, and never counted by the ceiling — and the first CI run, which
lints that feature separately, went red on them. The ceiling had been *measured*
under one feature set and *believed* for all of them.

**And the gate itself was broken by an environment variable it had never been run
under.** The CI workflow sets `CARGO_TERM_COLOR: always` at the top of the file.
With colour forced on, `cargo`'s `--message-format short` lines arrive wrapped in
ANSI escapes, so the script's `grep '^crates/...: warning: ...'` matched **none of
188 sites and counted 0**. A sibling gate added in the same change — a crate-list
comparison — went red on the same run for the same reason, its two identical
lists differing only in escape codes.

The count of 0 was caught only because the script refuses to treat 0 as a pass:

```
check-indexing-debt: FAIL — 0 sites counted. clippy emitted nothing, which is
  a broken invocation, not a clean workspace.
```

Without that clause, a ceiling of 188 would have been "met" for ever by a check
that had stopped looking. **The guard was written on general principle, before
there was anything to catch, and it caught something on the first real run.**

## The generalisation

> **An opt-out written at the top of a file has a scope, and the scope is a
> property of the language, not of the line's position.** A suppression meant for
> one file that lands on a whole compilation unit produces exactly the observable
> a working ratchet produces — clean output, a stable count — while protecting
> nothing.
>
> The check is not "did I write the suppressions", it is: **introduce a fresh
> violation into a file that was supposed to be protected, and confirm the gate
> rejects it.** Do it in each unit the suppressions touch, not once. A ratchet's
> counter can be perfectly correct while its enforcing half is switched off, and
> the counter is the half everybody looks at.
>
> Two more, from the same change:
>
> **A baseline measured under one build configuration is a claim about that
> configuration only.** Optional features, target platforms and cfg flags each
> hide files from the tool that enumerates them. Enumerate under every
> configuration the project actually builds, or state in the baseline which one
> it is.
>
> **A gate that parses another tool's output must be run under the environment
> that will run it.** Colour, locale, terminal width and verbosity all rewrite
> that output, and the failure mode is a count of zero rather than an error.
> **Refuse to treat an empty result as a pass** — it is the cheapest clause in
> any gate that greps, and the only thing standing between "nothing is wrong"
> and "nothing was looked at".

**Guarded since 2026-09-12 by `scripts/check-no-crate-root-allow.sh`**, which refuses any inner
`allow`/`expect` at a crate root — and any `warn` that lowers a lint the workspace denies —
across every `lib`/`bin` target under `crates/`, taken from `cargo metadata` rather than from
file names. The paste-a-violation reversal above is run once more as R-A4 of
[its plan](../plans/2026-09-12-crate-root-allow-scratch-fixture-and-tls-4c.md), tying the textual
check to the compiler effect: `[measured 2026-09-12]` with the crate-root `#![allow]` in place a
fresh `v[0]` in a clean submodule of `dict` compiled at **exit 0** with no diagnostic, and without
it the same code is **exit 101, `error: indexing may panic`**.
