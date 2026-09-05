# ADR-0049 — Bench builds pin function alignment, and the flag is read back

> **Status:** Accepted (2026-09-05) · **Amends:** [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md)
> decision 2 (how a margin is chosen) and [ADR-0031](ADR-0031-a-baseline-is-a-band.md)
> (which kept ADR-0016's recording procedure unchanged)
> **Closes:** `STATUS.md` open item 41's `encode ExecutionReport (template)` half

## Context

`scripts/bench.sh --strict` is the gate non-negotiable 10 leans on: no performance number
exists without the benchmark that produced it, the machine, and the §9 settings. It had been
red on the §9 desktop since 2026-09-02, and `encode ExecutionReport (template)` was one of
two reasons — 274–283 ns against a baseline of 239.1 recorded from 24 runs with the tightest
margin on the ladder, 1.10.

`STATUS.md` item 41 recorded it as a possible regression and named the leading hypothesis:
[ADR-0044](ADR-0044-a-builder-that-is-not-moved-per-field.md) had changed `TemplateBuilder`
on 2026-09-02, and the only route from there to `encode` was the layout of `Template` itself.
The plan that owns the item wrote down a second, opposite fact: `4396d6d` had modified the
bench harness, but reading its diff showed only the verdict logic and the printing changing,
so *the measuring instrument did not change*.

**That negative fact was wrong, and it was reached by reading a diff.**

## What was measured

All on the §9 desktop, AMD Ryzen 7 3700X, `scripts/check-machine.sh` `pass 12 fail 0
unknown 1`, medians of 5 runs unless stated.

**1. The baseline still reproduces at the commit that recorded it.** Built and run at
`bf798ea`: **240.0 ns** against the recorded 239.1. Neither the machine nor the toolchain
drifted, which had to be excluded before anything else could mean anything.

**2. The jump is one commit, and it is a bench-only commit.**

| Commit | What it is | median ns |
|---|---|---|
| `bf798ea` | recorded the 239.1 baseline | 240.0 |
| `f15c82d` | priced the pre-session stage | 234.4 |
| `54eebe9` | fixed a different benchmark | 240.6 |
| **`4396d6d`** | **a baseline is a band — harness only** | **268.0** |
| `576f924` | ADR-0044, the leading hypothesis | 279.4 |
| `HEAD` | today | 280.4 |

`git show 4396d6d -- crates/codec/src/` is **empty**. The commit `include!`d ~150 lines into
the bench binary and moved the figure **+11.4%**. ADR-0044 is real but is +4.5%, arriving on
a case that was already 12% over its ceiling: the written hypothesis would have explained a
quarter of the gap and been believed for all of it.

**3. Forced alignment collapses the difference.** With `-C llvm-args=-align-all-functions=6`,
`54eebe9` reads 241.6 and `4396d6d` reads 232.6 — the gap is gone. `HEAD` goes 278.9 → 233.0.

**4. And without the knob, because a knob that moves a number is not a cause.** Inert
functions were added to the bench binary — code the encoder never calls, referenced once
through `black_box` so it survives to link:

| Inert functions | unpinned, ns | pinned, ns |
|---|---|---|
| 0 | 278.8 | 235.8 |
| 3 | 245.7 | 238.9 |
| 9 | 281.6 | 229.6 |
| 27 | 264.4 | 230.9 |
| **spread of medians** | **14.6%** | **4.0%** |

Individual unpinned readings span 236.5–292.4 ns with not one line of the code under test
different between them.

**5. It is not only this case.** Under the same flag `library, parse only` moves 138.5 →
160.5, +16% in the *other* direction — and it has never had a baseline on this CPU at all.

## Decision

**1. `scripts/bench.sh` pins function alignment for bench builds**, by exporting
`RUSTFLAGS="-C llvm-args=-align-all-functions=6"` — 6 is log₂(64), one cache line.

Scoped to the script, not to `.cargo/config.toml`: the shipping build must not change, and
the recording procedure already requires baselines to be measured *through `bench.sh`* rather
than per target ([per-machine-baselines](../plans/2026-08-31-per-machine-baselines.md),
Sửa 2).

**2. The flag is read back off the built binaries, by `scripts/check-bench-alignment.sh`,
which `bench.sh` runs before it prints its summary.** A typo cannot survive — rustc refuses
an unknown `llvm-args` and the build dies — but a flag that is *accepted and ignored* by a
later LLVM is silent, and would leave every figure layout-bound under a green gate. The check
counts this workspace's own text symbols on a 64-byte boundary: `[measured 2026-09-05]`
**5 of 23 unpinned against 23 of 23 pinned** for `serialize`. Only own symbols are counted —
`RUSTFLAGS` does not rebuild the precompiled standard library, so a whole-binary count reads
137/629 vs 158/629, a real difference buried under std and far too weak to assert on.

Proven by reversal, `--reversal`, which builds one target with the flag removed and **requires
the check to go red**: `2 of 11`.

**3. `encode ExecutionReport (template)` keeps a margin wider than 1.10 — 1.15** — because
pinning reduces the layout term to 4.0% but does not remove it, and 1.10 is inside that plus
ordinary run-to-run variance. The owner chose both halves on 2026-09-05: pin *and* widen,
rather than either alone.

**4. Every baseline on this CPU is re-recorded under the pinned build.** The measurement
changed, so the numbers it is compared against change with it. Nineteen lines, one machine —
`benches/baselines.tsv` holds no other CPU, so nothing else is invalidated.

## Consequences

**Good**

- The gate now moves when the code under test moves. Before this, `bench.sh --strict` could
  be turned red by adding a function to the harness, and the person reading it would go
  looking for a regression in the encoder — which is exactly what happened, for three days,
  across two documents.
- The band means something again at 1.10 for the cases that hold there, instead of every case
  needing a margin wide enough for layout.
- The read-back is a new machine check, and it is the kind this repository keeps discovering
  it needs: a claim about *how the build was performed*, verified from the artifact.
- The failure is loud in both directions. A missing `nm` fails rather than skips; cargo
  reporting no bench executables fails rather than passes.

**Bad, and named**

- **The benchmarks now measure a binary that is not the one that ships.** Aligning every
  function to 64 bytes is not what a release build does, so the absolute figures are of a
  slightly different program. This is tolerable precisely because §6's figures are compared
  against *themselves over time* on one machine; it would not be tolerable for a number
  published as "what this engine costs". Where an absolute figure is published — `tools/w2w`,
  `DESIGN.md` §8 — it does **not** come through `bench.sh` and is unaffected.
- **`-C llvm-args` is a string handed to LLVM.** It is not a stable rustc interface, and a
  toolchain upgrade may rename or drop `align-all-functions`. Decision 2 is what makes this
  survivable rather than a slow silent rot, but the day it fires, somebody has to find a
  replacement mechanism and every baseline on the machine is re-recorded again.
- **Re-recording nineteen lines in one commit is the shape of the failure mode
  `CLAUDE.md` §10 warns about** — a fixture edited so that new work can pass. What separates
  it here: the two out-of-band cases were each diagnosed *before* any line was touched, the
  plan made 1d depend on 1a–1c for that reason, and neither diagnosis was "the code got
  slower" — one is a benchmark measuring its own address, the other is
  [ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md) deliberately making
  `identity_of` do twice the scans.
- **4.0% residual layout sensitivity remains**, so a real regression to `Template::encode`
  smaller than the 1.15 band is still invisible. The case cannot claim better than that, and
  `DESIGN.md` §6 says so beside it now.
- Bench artifacts no longer share a fingerprint with `cargo test`/`cargo build`, so a bench
  run after a test run rebuilds. Costs wall clock, buys the guarantee that the figures came
  from the checked binaries.

**Neutral**

- `cargo bench` invoked by hand, outside `bench.sh`, produces unpinned figures that no longer
  match the baselines. That is a reason to use the script — which the recording procedure
  already required — and the alignment check names the discrepancy if anyone is confused.

## Alternatives rejected

- **Widen the margin alone, no build change.** Honest and free, and it was the other half of
  the choice put to the owner. Rejected because the required margin is ~1.20–1.25 and a case
  that cannot see a 20% regression in the encoder is not guarding D9's central claim.
- **Re-record 239.1 → ~280 and keep 1.10.** This is the fixture edit with nothing learned:
  the next harness edit re-reds it, and the reason would have to be rediscovered.
- **Drop the case's band, report only.** Loses the gate. `parse` and `encode` are the two
  cases §6 exists for.
- **`codegen-units = 1` for the bench profile**, to make layout deterministic. Does not help:
  layout stays deterministic *for a given source*, and the whole problem is that unrelated
  source changes move it. [ADR-0024](ADR-0024-the-workspace-keeps-the-default-release-profile.md)
  also measured what it costs in build time.

## Open question

The 4.0% residual has not been attributed. It may be loop-body alignment inside a function
rather than function entry alignment, which `-align-all-nofallthru-blocks` would address at a
size cost. Not pursued: 1.15 covers it, and one codegen flag whose failure mode has to be
guarded is already one more than this repository wanted.
