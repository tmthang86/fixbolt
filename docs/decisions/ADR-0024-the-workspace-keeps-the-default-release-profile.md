# ADR-0024 — The workspace keeps cargo's default release profile, and `GUIDE.md` tells consumers what LTO is worth

> **Status:** **Accepted — 2026-09-01.** Closes `STATUS.md` open item 13.
>
> **Accepted by standing delegation**, `[2026-08-30]`.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md),
  `DESIGN.md` §6 and §8, `CLAUDE.md` §2 non-negotiable 10,
  [measured-costs.md](../reference/measured-costs.md),
  [plans/2026-09-01-release-profile.md](../plans/2026-09-01-release-profile.md)

## Context

`[measured 2026-09-01]` `Cargo.toml` has no `[profile.release]` section, so every number in
`benches/baselines.tsv` was measured at cargo's defaults: `opt-level = 3`, `lto = false`,
`codegen-units = 16`. Open item 13 asked what the usual knobs are worth here.

Four arms, ten `scripts/bench.sh` runs each, no reboots, `check-machine.sh` reading
`pass 12 fail 0 unknown 1` for every run counted:

| | clean build | syscall-bound cases | pure user-space cases |
|---|---|---|---|
| default | **5.2 s** | — | — |
| `lto = "thin"` | 17.1 s | −2.2% … −3.0% | −8.4% … +1.3% |
| `lto = "fat"` | 15.9 s | **−2.9% … −5.4%** | **−30.8%** … +12.2% |
| `codegen-units = 1` | 5.2 s | −0.1% … −2.2% | −16.6% … +1.8% |
| both | 16.3 s | **−2.9% … −5.6%** | −30.1% … +12.2% |

The prediction written down first — user-space improves, syscall-bound barely moves — was
**directionally right and understated the syscall side**. Those cases gave back 3–6%, more
than "barely".

## Decision

**1. The workspace keeps cargo's default release profile.** No `lto`, no `codegen-units = 1`.

**2. The reason is that this setting does not reach the people it would appear to help.**
Cargo honours `[profile.*]` only from the **top-level package being built**; profile settings
in a dependency are ignored. So a `[profile.release]` here applies to *this* workspace's
benchmarks and `tools/w2w`, and **not** to anybody who depends on `fixbolt-codec`,
`fixbolt-session` or `fixbolt-engine`. Setting it would make this project's published numbers
better while changing nothing for a consumer — which is the shape of a misleading measurement,
and non-negotiable 10 exists to stop exactly that.

**3. And part of the measured gain is an artifact of measuring.** A benchmark is a separate
crate calling into the library, so LTO inlines library internals into the *benchmark loop*.
`presession, read and route an identity` fell 83.4 → 57.7 ns — but in production
`Shards::hand` calls `identity_of` from within the same crate, where it is already inlinable.
Likewise `recv on a quiet socket` fell 2.9% on a case that is ~94% kernel time; 12 ns off ~25 ns
of user-space work is an inlining effect at the bench boundary, not a kernel one.

**How much of the gain survives into a real application was not established**, and this ADR
does not claim a number for it. What was measured is what a benchmark sees.

**4. `GUIDE.md` carries the range so a consumer can decide for their own binary**, where the
profile does apply, with the caveat in decision 3 attached. This is the useful half of the
result and it belongs where a user of the framework reads.

**5. The baselines stay a default-profile measurement.** They are a regression detector for
this project's own code, and keeping the build configuration constant is what makes years of
them comparable. Changing it would invalidate every recorded row for a gain the consumer does
not get.

**6. `codegen-units = 1` is free in build time and is still not adopted**, for the same reason
as the rest: it changes what our benches measure without changing what anybody ships.

**7. PGO and `#[cold]` remain out of scope, and item 13 closes without them.** PGO needs a
profile-generation run and a representative workload — its own plan, worth writing only if
whole-program optimisation is ever shown to matter to a *consumer*. `#[cold]` is a code change,
not a profile one.

## Consequences

**Good**

- The published numbers describe a build a consumer can reproduce with `cargo build --release`
  and nothing else.
- Build time stays **5.2 s** rather than 16, on every CI job and every local iteration. That
  is not the reason for the decision, but it is not nothing.
- The measurement is not wasted: the range is documented for the people the setting actually
  reaches.

**Bad, and named**

- **This project's own binaries are slower than they could be**, `tools/w2w` included, and its
  wire-to-wire figures will carry that. Consistent with decision 5 — a number a consumer cannot
  reproduce is worth less than a slower one they can — but it is a real cost paid on the one
  gate §6 has left open.
- **The decision rests on documented cargo behaviour, not on a measurement.** That profiles do
  not propagate to dependents is stated by cargo and was not tested here. It is the load-bearing
  claim of decision 2 and it is labelled as unverified.
- **The arms were ten runs each**, not the twenty ADR-0016 asks of a baseline. Enough to see
  3–30% effects; not enough to resolve 1%.
- **A consumer who follows `GUIDE.md` gets a number this project has never measured**, because
  the caveat in decision 3 means the bench figures overstate what their application will see.
  The guide says so, which is the best available answer and not a good one.

## Alternatives considered

**Adopt `lto = "fat"` + `codegen-units = 1`.** The naive read of the table, and rejected on
decision 2: the numbers get better, nobody's program does. It would also have cost every
historical baseline its comparability, for a gain measured at the bench boundary.

**Adopt it only for `tools/w2w`.** A per-package profile would make the wire-to-wire gate
faster. Rejected for the same reason and more sharply: §6's wire-to-wire row is the most
product-shaped claim this repository will make, and it is the last place to build differently
from what a reader can reproduce.

**Leave the whole question open.** Rejected. Item 13 has been open since the project began and
the measurement cost two hours; leaving it open after measuring it is how a `[unproven]` label
outlives the thing it was attached to — which is exactly what item 22 just demonstrated.
