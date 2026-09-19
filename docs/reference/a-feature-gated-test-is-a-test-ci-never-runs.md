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
the feature.

`[corrected 2026-09-19]` This page first suggested three bare lines, one per crate. Two things
were wrong with them and both were found when the fix was built:

* **`cargo test -p fixbolt-conformance --features fix50sp2` does not work.** That crate has no
  `[features]` section at all — the string `fix50sp2` appears in it only inside a doc comment.
  Passing a feature a package does not declare is a hard cargo error.
* **A bare `cargo test` line proves only its own exit status**, which is the thing this family of
  traps keeps defeating. A green command is not evidence that the tests you meant ran.

What was built instead, in the `gates` job: one loop over the crates that **declare** the feature
(`codec`, `dict`, `session`, `engine`), each running the crate's whole `--tests` suite and then
passing the log through `scripts/check-feature-gated-tests-ran.sh` — the script the `tls` job
already uses for the same shape. It asks the build which tests the feature has and then requires
each one to appear as having run: R1 a listed test with no run status, R2 a built binary with no
`Running` line, R3 a test that was ignored. That turns *"the command exited 0"* into *"these named
tests executed"*.

Two details worth keeping:

* The boundary is **crates that declare the feature**, not *crates that have gated tests today*.
  `fixbolt-codec` has none yet; the wider boundary cannot silently reopen when the next one is
  written.
* It runs **each crate's whole suite**, never a list of file names. `crates/dict/tests/group_tables.rs`
  has no `#![cfg]` at the top — it grows one extra case under an inner `#[cfg(feature =
  "fix50sp2")]`, which any file-name list would have missed.

The check on the check: **make a feature-gated test fail on purpose and confirm CI goes red.**
Until that has been observed once, "CI covers the feature" is a claim about a YAML file, not a
measurement — and this page exists because the YAML file read as though it did.

## What guards it

`[changed 2026-09-19]` The `gates` job's step *"The fix50sp2 tests, and proof they were the ones
that ran"*, via `scripts/check-feature-gated-tests-ran.sh`, described above. Measured on the
commit that added it, all four crates accounted for with nothing ignored:

```
fixbolt-codec    14 binaries, 14 Running lines,  87 listed,  87 accounted for, 0 ignored
fixbolt-dict     11 binaries, 11 Running lines,  59 listed,  59 accounted for, 0 ignored
fixbolt-session  22 binaries, 22 Running lines, 169 listed, 169 accounted for, 0 ignored
fixbolt-engine   44 binaries, 44 Running lines, 334 listed, 334 accounted for, 0 ignored
```

and the five FIXT binaries running with real counts rather than the empty-shell result this page
is about — `score_fixt` 2 tests, `wire_fixt` 1, against `running 0 tests` under the default
feature set.

**The check on the check is observed — and it was not synthetic.** This page asked for a
feature-gated test to be broken on purpose and CI watched to see it go red. That was never needed:
the first time this step ever ran, it went red on a **real** defect that `cargo test --all` could
not see — `crates/dict/tests/fixt_order.rs` had four possible answers at one pin, because its
oracle walked `read_dir`
([a-test-oracle-that-reads-read-dir-has-more-than-one-answer](a-test-oracle-that-reads-read-dir-has-more-than-one-answer.md)).
Both directions were watched on the same branch, one commit apart:

```
9ca0608  fix50sp2 step RED   fixt_order.rs:235  left: 64460  right: 64750   (twice)
064d90a  fix50sp2 step GREEN after the fix                 CI run 35449251846
```

That is a stronger observation than breaking something deliberately would have been, because the
failure was one nobody planted and nobody expected. Four numbers had been pinned, quoted in
`docs/reference/`, and **wrong since the commit that introduced them** — invisible to every gate
in this repository, because no gate had ever run that test on a second machine.

A known gap, named rather than left implicit: the script is given `--tests`, so a **doctest**
behind the feature would still run nowhere. There is no `fix50sp2` doctest today. The same gap is
recorded in the script's own header for `tls`.

## Related

- [a-doc-gate-never-opened-the-file-it-was-guarding](a-doc-gate-never-opened-the-file-it-was-guarding.md)
  — the same shape for rustdoc: a job that could not fail on a module the default feature set
  never compiled. That one was found by accident too.
- [a-doc-command-that-exits-zero-is-not-the-doc-gate](a-doc-command-that-exits-zero-is-not-the-doc-gate.md)
  — the sibling about *severity*: the command ran, and its warnings were not errors. Here the
  command never ran at all.
- [CLAUDE.md](../../CLAUDE.md) §10: *"A check proves nothing until something reads it."* This is
  that sentence with the check itself as the thing nobody read.
