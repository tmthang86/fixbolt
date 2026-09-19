# A `--workspace` build and a `-p` build are different artifacts

`[measured 2026-09-19]` on the cloud Linux box, `RUSTFLAGS` pinned to ADR-0049's
`-C llvm-args=-align-all-functions=6` throughout.

## The shape

`scripts/bench.sh` runs each bench target as `cargo bench -p <pkg> --bench <name>`.
`scripts/check-bench-alignment.sh` then reads the alignment flag back off the bench
binaries — the ADR-0049 guard that stops every figure being a measurement of where the
compiler put the code. It discovered those binaries with one `cargo bench --workspace
--no-run`.

Two different invocations, and the assumption nobody wrote down was that they name the
same files. They do not.

## The measurement

`fixbolt-session --bench alloc`, same flags, three invocations:

```
-p fixbolt-session --features fix50sp2      alloc-66cc3ea49f7b020a   <- what bench.sh RUNS
--workspace        --features fix50sp2      alloc-f3c4b7c7d945a11e
--workspace        (no features)            alloc-e674ff459564cb4a   <- what the check READ
```

Three builds, three artifacts. The feature flag is **not** the cause — it only made a
latent divergence total. The cause is that a cargo unit's identity includes whatever
**feature unification** the invocation performs across the packages it selects, so
`--workspace` is a different build from `-p <pkg>` even when flags and features match.

It was already broken before any feature existed. Featureless, the workspace build
matched the per-package build for 17 of 18 bench targets but not for `fixbolt --bench
alloc`:

```
-p fixbolt --bench alloc      alloc-1845cc4723f12ea9
--workspace                   alloc-282bf43cebf5f93e  (and no 1845cc… anywhere in the set)
```

So the guard had been certifying one artifact nobody measured since it was written.
Adding `--features` to the workspace call — the obvious one-line fix, and the one the
manager predicted would work — took that from 1 wrong to 17 wrong.

## Why it matters more than it looks

`bench.sh`'s own header already states the stake: *"Two copies of a codegen flag are two
different builds, and the figures would then come from artifacts the check never saw."*
That sentence was true of the script's own guard.

The guard is not void when it reads the wrong binary — its real question is *"does this
toolchain still honour `-C llvm-args=-align-all-functions=6`"*, and that is a property of
`RUSTFLAGS`, not of the feature set, so it is still answered on 18 binaries. What is lost
is narrower and worth naming exactly: **it no longer certifies the artifacts the published
figures came from.** A weakened guard that still prints `OK` is the hard kind to notice.

Worse, under the reversal the header printed `features  fix50sp2` while building without
them. The output *asserted* the feature set it was not using — the gap does not merely
stay silent, it reads as covered. That is the same shape as the `feature-sets` CI job
sitting next to the hole it looks like it closes
([a-feature-gated-test-is-a-test-ci-never-runs](a-feature-gated-test-is-a-test-ci-never-runs.md)).

## The fix

`bench_binaries()` enumerates **per package**, one `cargo bench -q -p <pkg> --no-run` each,
with the same feature set `bench.sh` runs with. The feature list lives in
`check-bench-alignment.sh` beside the flag, for the reason the flag lives there, and is
exposed as `--features` (csv, for the header) and `--features-map` (per package, for the
enumeration). `bench.sh` asks; it keeps no list.

Result, checked by set equality rather than by exit code: the 18 binaries `bench.sh`
builds are the identical 18 the read-back certifies. **17/18 before, 18/18 now.**

Independent corroboration that the feature is really in them:

| | before | after |
|---|---|---|
| `fixbolt-session/alloc` | 216 own-crate text symbols | **264** |
| `fixbolt-session/validate` | 31 | **42** |

+48 and +11 are the new FIXT cases.

## The rule to take away

**Two cargo invocations that differ only in package selection are two different builds.**
Any check that reads an artifact must build it the same way the thing it is certifying
built it — same flags, same features, and the same `-p` / `--workspace` shape. Comparing
`OK` against `OK` proves nothing; compare the **artifact identifiers**, as a set.

## What guards it

`scripts/check-bench-alignment.sh`, by construction: it now derives its binary list the
same way `bench.sh` derives what it runs, from one definition. Proven by reversal —
disabling the feature pass-through in `bench_binaries()` alone puts the read-back back on
`alloc-e674ff459564cb4a` (216 symbols) while `bench.sh` still ends `OK`, which is exactly
the hole, reproduced on demand.

No gate compares the two sets automatically. That comparison was done by hand, here, and
is the thing to redo if either script's build shape changes.

## Related

- [ADR-0049](../decisions/ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md) — why the
  alignment flag exists and what a figure costs without it.
- [feature-flags-unify-across-a-workspace](feature-flags-unify-across-a-workspace.md) —
  the other half of cargo's unification biting this repository.
- [a-feature-gated-test-is-a-test-ci-never-runs](a-feature-gated-test-is-a-test-ci-never-runs.md)
  — the same family: a check that reads as covering something it never touched.
