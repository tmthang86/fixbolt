# A feature-gated test is a test CI never runs

`[measured 2026-09-19]`

## The shape

Phase 2's PR B put every new test behind `#![cfg(feature = "fix50sp2")]`, which is correct:
`cargo test --all` on a machine without the feature must still compile and pass, and non-negotiable
6 wants the feature to gate the module, not only the manifest.

Then CI went **green** on a commit whose author had just watched one of those tests fail:

```
$ cargo test -p fixbolt-session --features fix50sp2 --test score_fixt
fix50 57/60 fix50sp1 58/60 fix50sp2 58/60
test result: FAILED. 1 passed; 1 failed;
```

The commit was pushed **deliberately red**, with the reason in its message. CI reported
`conclusion: success`, 14 jobs of 14.

The explanation is one grep:

```
$ grep -n "fix50sp2" .github/workflows/ci.yml
(no match)
```

No job enables the feature. `cargo test --all` and `cargo test --all --no-default-features` both
build the crate **without** it, so the `#![cfg(...)]` at the top of each new test file compiles
those files to nothing. Four test binaries — `dict/tests/fixt.rs`, `dict/tests/fixt_order.rs`,
`session/tests/fixt.rs`, `session/tests/score_fixt.rs` — had never been executed by CI, on any
commit, since the feature was created.

## The half-truth that hides it

`ci.yml`'s `feature-sets` job **does** cover the feature — with `cargo hack clippy` and
`cargo hack doc` over the feature powerset to depth two. So "the new feature passes CI's
powerset" is a true sentence, and it is about **lint and documentation**. Nothing in that job
runs a test. A reader who knows the powerset job exists will assume the feature is covered, and
be right about two of the three things they mean.

That is what makes this worse than a gap nobody thought about: the gap sits **next to** a job
that looks like it closes it.

## Why the usual instinct does not catch it

The habit that catches most of these — run the gate the plan row names — is exactly what does
not help. Every plan row for this work names `cargo test -p <crate> --features fix50sp2`, and
every one of those commands was run, read and quoted. They were run **here**, by hand. The
question nobody asked is *"and which job runs that?"*, because the answer for every other test
in the repository is `cargo test --all`.

## What to do

A feature that gates tests needs its own CI invocation, named in the same commit that creates
the feature. The minimum is one line per crate that has feature-gated tests:

```
- run: cargo test -p fixbolt-dict --features fix50sp2
- run: cargo test -p fixbolt-session --features fix50sp2
- run: cargo test -p fixbolt-conformance --features fix50sp2
```

`cargo hack test --workspace --feature-powerset --depth 2` would cover it generically, and costs
what the powerset costs; the explicit lines cost one job-step each and say which crate is meant.

The check on the check: **make a feature-gated test fail on purpose and confirm CI goes red.**
Until that has been observed once, "CI covers the feature" is a claim about a YAML file, not a
measurement — and this page exists because the YAML file read as though it did.

## What guards it

Nothing, at the time of writing. The fix belongs to plan row B7, which is the row that owns
`.github/workflows/ci.yml` under the parallel-work contract, and it is written down here so it
is not carried in somebody's head until then. A green CI run on this branch **does not** mean any
FIXT test passed.

## Related

- [a-doc-gate-never-opened-the-file-it-was-guarding](a-doc-gate-never-opened-the-file-it-was-guarding.md)
  — the same shape for rustdoc: a job that could not fail on a module the default feature set
  never compiled. That one was found by accident too.
- [a-doc-command-that-exits-zero-is-not-the-doc-gate](a-doc-command-that-exits-zero-is-not-the-doc-gate.md)
  — the sibling about *severity*: the command ran, and its warnings were not errors. Here the
  command never ran at all.
- [CLAUDE.md](../../CLAUDE.md) §10: *"A check proves nothing until something reads it."* This is
  that sentence with the check itself as the thing nobody read.
