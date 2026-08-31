# ADR-0021 — `nohz_full` leaves the §9 checklist; `isolcpus` and `rcu_nocbs` stay

> **Status:** **Accepted — 2026-08-31**, by the owner, on the four-arm table and the
> tail table below.
>
> **Addendum, same day, and it does not revise a decision.** Rebooting into the §9
> line this ADR chose found that `nohz_full` also costs **~45 ns per kernel entry on
> every CPU that is NOT in its list** — `cpu5` carried no flag in any boot and went
> from 501.8 ns per turn to 455.7. Every figure in the tables below therefore carries
> that tax as well, in all four arms equally, so the comparisons and the decision are
> unchanged and the true price of `nohz_full` on the core that has it is **~200 ns per
> kernel entry, not 155**. §5 forbids editing an accepted ADR's substance; this note
> strengthens the case it already made and changes none of it, and the measurement
> itself lives in [measured-costs.md](../reference/measured-costs.md) where a
> measurement belongs. `DESIGN.md` §8's dominant row is now **449 ns**, from 24 fresh
> baseline runs.
>
> Reverses one row of `DESIGN.md` §9 and the gate that checks it. §9 has told
> every reader of this repository to give the engine thread a core with
> `isolcpus`, `nohz_full` and `rcu_nocbs` since the checklist was written, and
> `scripts/check-machine.sh` has been failing machines that did not.

- **Date**: 2026-08-31
- **Deciders**: Tran Manh Thang
- **Related**: `DESIGN.md` §8 and §9, `CLAUDE.md` §2 non-negotiable 10,
  [measured-costs.md](../reference/measured-costs.md),
  [plans/2026-08-31-which-isolation-flag-costs.md](../plans/2026-08-31-which-isolation-flag-costs.md),
  `STATUS.md` open item 22

## Context

`[measured 2026-08-31]` §9's isolated core was found to be **36% slower** at the
one operation §8 says dominates — a non-blocking `read` per session per turn.
Which of the three isolation options caused it was not separated: one kernel
command line applied all three to the same CPUs.

It has now been separated, by putting the three flags on three **different**
CPUs in one boot rather than one flag per boot. AMD Ryzen 7 3700X, Linux
7.0.0-30-generic, SMT off, `performance`, turbo off, `check-machine.sh` reading
`pass 10 fail 0`:

| Core | Flags | bare `getpid` | `Engine::turn`, 1 session |
|---|---|---|---|
| `cpu5` | none | 198.87 ns | 501.8 ns |
| `cpu6` | `isolcpus` | 198.94 ns | **494.8 ns** |
| `cpu7` | `rcu_nocbs` | 198.86 ns | 498.2 ns |
| `cpu4` | `isolcpus` + `nohz_full` | **354.76 ns** | **670.7 ns** |

**`nohz_full` is the entire cost.** `isolcpus` and `rcu_nocbs` are free, and the
`isolcpus`-only core is the fastest of the four. `nohz_full` cannot be tested
alone — the kernel adds `rcu_nocbs` to any CPU that has it — so it is reached by
subtracting the two arms that are free.

The cost is **not** the clock and **not** interrupts. A pure user-space loop runs
at the same speed on all four cores (1.0546–1.0581 ns/iter), and the `nohz_full`
core takes **3743 fewer** timer interrupts per second than the others and is
still 78% slower per kernel entry. It is the context tracking that full dynticks
runs on every entry and exit, which is what the mechanism predicted.

### And what it buys, which had never been measured either

`nohz_full` is bought for jitter, so a decision on median alone would repeat the
error this project has already written down. Every call timed individually,
5 000 000 of them:

| Core | Flags | p50 | p99 | p99.9 | p99.99 | calls > 1 µs | ticks |
|---|---|---|---|---|---|---|---|
| `cpu5` | none | 216 | 224 | 240 | 3720 | 1130 | 1283 |
| `cpu6` | `isolcpus` | 216 | 224 | **224** | 2848 | 1078 | 1281 |
| `cpu7` | `rcu_nocbs` | 216 | 224 | 240 | 3440 | 1120 | 1281 |
| `cpu4` | `isolcpus` + `nohz_full` | 376 | 376 | 384 | **504** | **2** | **2** |

The count of calls over 1 µs tracks the local timer interrupt count **call for
call** — 1130/1283, 1078/1281, 1120/1281 and 2/2. The tail *is* the tick, shown
rather than inferred.

**And the decisive line is the one that reads left to right.** `nohz_full` is
worse at p50, worse at p99, **and worse at p99.9** — 384 ns against 224. It wins
only from **p99.99 outward**, and even there `max` does not follow: over four
runs `cpu4`'s worst call was 852, 2966, 11582 and 14107 ns, so the rare large
excursion survives it.

## Decision

**1. `isolcpus` stays in §9.** It is free — 494.8 ns against 501.8 on an
untouched core, which is inside this bench's own run-to-run spread and if
anything the right way round. Kept on mechanism and on price, **not** on a
benefit measured here: on a quiet machine it removes almost nothing (1078
excursions against 1130) because there is nothing to remove. Its value is
against *other tenants*, and this machine had none. Named as unmeasured, not
implied.

**2. `rcu_nocbs` stays in §9.** Free on the same evidence, and it is what keeps
RCU callbacks off the engine core when the system is doing work.

**3. `nohz_full` is removed from the §9 recommendation.** Not forbidden —
**priced**. §9 gains a line saying what it costs and what it buys, and the
choice belongs to whoever is deploying.

The arithmetic behind the removal, for this engine at `hft`'s N=1:

> A turn is ~500 ns, so a busy engine performs ~2 000 000 kernel entries per
> second per core. `nohz_full` adds **160 ns to every one of them** and removes
> **~1100 excursions of 3 µs**. That is 0.32 s of tax per second of running
> against 0.0033 s of tail removed — **a hundred to one against**.

The tax is certain and uniform; the benefit lands on 0.055% of turns. A design
whose stated gate is `p99 ≤ 50 µs` wire-to-wire (§6) is buying protection from
3–20 µs excursions it already has room for, and paying for it on every message.

**4. `scripts/check-machine.sh` changes with it, and the change is a reversal.**
The row `isolcpus + nohz_full` becomes two rows: `isolcpus + rcu_nocbs` PASSes
as before, and **`nohz_full` PASSes when it is ABSENT** from the engine cores,
reporting the measured price when it is present. A gate that keeps demanding
`nohz_full` after this ADR would be failing machines for being fast.

This matters beyond advice: `benches/baselines.tsv` was recorded **without**
`nohz_full` on the measured cores, so a machine that has it reads 35% over
baseline on four cases and `bench.sh --strict` refuses. `[measured 2026-08-31]`
that is exactly what happened on `cpu4` — the existing gate flagged all four
`turn` cases `OVER BASELINE` without being told anything about isolation.

**5. `nohz_full` is the right answer for a p99.99 objective, and §9 says so.**
It takes p99.99 from 2848 ns to 504 ns, which is a 5.6× improvement and is real.
This repository does not have that objective today. If §6 ever grows a p99.99
row, this ADR is the thing to supersede.

## Consequences

**Good**

- The recommendation is now priced on both sides, which it never was. A reader
  can decide; before, they were told.
- The §8 budget loses its worst row. `Engine::turn` on a §9 machine becomes
  **~500 ns per session, not ~675**, and the "isolated core" penalty leaves the
  table rather than being carried as a permanent 36%.
- A machine set up to the new §9 is faster at p50, p99 **and** p99.9 than one
  set up to the old.
- `check-machine.sh` stops failing correctly-configured machines.

**Bad, and named**

- **A published checklist changes, and anything measured under the old one is
  now measured under a configuration this project no longer recommends.** Every
  number in `measured-costs.md` and `baselines.tsv` was taken on cores without
  `nohz_full`, so the figures are unaffected — but the *label* on them was wrong
  for a day and that has to be said out loud rather than quietly corrected.
- **`isolcpus` is kept on an argument, not on a measurement.** It is free, so the
  cost of being wrong is zero, but this ADR should not be read as having proven
  it earns its place.
- **One machine, one kernel, one CPU vendor.** `nohz_full`'s entry/exit cost is a
  kernel-configuration property (`CONFIG_CONTEXT_TRACKING_USER`,
  `CONFIG_VIRT_CPU_ACCOUNTING_GEN`), not a Ryzen property, but nothing here has
  been run on Intel or on another kernel build.
- **The tail measurement is 5 seconds per arm on an idle box.** Excursions rarer
  than one in 5 000 000 are outside what it can see, and `max` was demonstrably
  unstable across runs.

## Alternatives considered

**Keep `nohz_full` and accept the 36%.** Rejected on the arithmetic in decision
3, and on the p99.9 row: this is not a median-versus-tail trade where the tail
wins somewhere reasonable. `nohz_full` is behind until p99.99.

**Drop `isolcpus` and `rcu_nocbs` too, since none of the three helped here.**
Rejected. They are free, their mechanism is about *other tenants* and this
machine had none, and removing a free defence because a quiet box did not need
it is designing against the easy case.

**Make it a mode split, like ADR-0013's `standard` / `hft`.** Rejected as
premature: a mode is a promise the code has to keep, and this is a line in a
deployment checklist. If a p99.99 objective ever appears in §6 it can become one.
