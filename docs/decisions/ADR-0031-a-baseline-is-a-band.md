# ADR-0031 — A baseline is a band, and a figure below it is reported rather than failed

> **Status:** **Accepted — 2026-09-01.** **Amends
> [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md) decision 1**, which
> defined the comparison as *"no regression past this machine's recorded baseline, times this
> case's margin"* — a ceiling. Everything else in ADR-0016 stands: baselines are still data,
> still per (CPU model, case), still the median of `n ≥ 20` runs, and the margin is still
> derived from that case's own measured spread.
>
> **Accepted by standing delegation**, `[2026-08-30]`, and the owner's instruction of
> 2026-09-01 to decide what cannot be decided from the code.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md),
  [ADR-0021](ADR-0021-nohz-full-leaves-section-9.md),
  [ADR-0023](ADR-0023-section-9-records-the-cpu-mitigations.md),
  `DESIGN.md` §6, `CLAUDE.md` §2 non-negotiable 10 and §10,
  [a-benchmark-can-delete-its-own-work.md](../reference/a-benchmark-can-delete-its-own-work.md),
  `STATUS.md` open item 25,
  [plans/2026-09-01-a-baseline-is-a-band.md](../plans/2026-09-01-a-baseline-is-a-band.md)

## Context

`[measured 2026-09-01]` `inline deliver + reply` published **1.3 ns** for a day while doing
**8.5 ns** of work: `out` was written every iteration and read by nobody, so the optimiser
deleted a 163-byte copy. It was found by arithmetic during an unrelated experiment — 163 bytes
in 1.3 ns is 125 GB/s from one core — **not by a gate**.

No gate could have found it. The comparison was `best > baseline * margin`: a ceiling. A case
that stops measuring reads far *under* its limit, passes, and passes **more comfortably every
day**.

**This is the third of three shapes**, and the count is why it earned an ADR rather than a
patch:

| What got faster | What the gate said |
|---|---|
| A machine setting outside §9 ([ADR-0021](ADR-0021-nohz-full-leaves-section-9.md), [ADR-0023](ADR-0023-section-9-records-the-cpu-mitigations.md)) — every number | green |
| A benchmark that stopped measuring — one number | green |
| An allocation guard whose window excluded the operation ([the-guard-measured-a-window…](../reference/the-guard-measured-a-window-that-excluded-the-thing.md)) — zero allocations | green |

The first two are closed. This is the third, and the hardest, because **a naive floor turns
every genuine optimisation into a red gate** — and what people would learn from that is to
widen the margin, which destroys the ceiling too.

## Decision

**1. The comparison is a band: `[baseline / margin, baseline * margin]`.** Three outcomes, not
two — `InBand`, `Over`, `Under`.

**2. `Under` is reported and counted; it is never a build failure.** A figure below the floor
has exactly two causes and **both need the same action from a person**:

| Cause | Correct action |
|---|---|
| A real optimisation | **Re-record the baseline.** Otherwise the ceiling above it is wider than the truth and guards nothing |
| The benchmark stopped measuring | Fix the benchmark |

Making it red would break every genuine speed-up before it could be merged. Making it silent
is the defect. So it takes the shape `NO BASELINE` already has: printed with the case, counted
on its own grep-able line, reported by `scripts/bench.sh`, and **fatal under `--strict`** —
which is what a `DESIGN.md` §9 machine runs during a deliberate measurement session.

**3. The floor uses the same `margin`, and the asymmetry is named rather than hidden.**
`margin` is `max/median` over the `n` runs that produced the baseline: that case's own measured
spread. But the measured value is `best` — a minimum over rounds — and the baseline is a
*median* over runs, so the distribution below the median is the wider one. **This floor will
occasionally report noise.** That is affordable *only* because `Under` is a report, and the way
to resolve a false one is to re-record the baseline, which is the right thing to do anyway. A
separately measured `low_margin` column would need `n ≥ 20` runs on a §9 machine; **it is not
invented here**, and the plan says so in its own scope.

**4. The comparison moves out of the bench harness and gets a test.**
`crates/codec/benches/verdict.rs`, `include!`d by `harness.rs` and by
`crates/codec/tests/bench_verdict.rs`. A `harness = false` bench target is a `main()` that
`cargo test` never runs, so **the rule deciding every timing gate in §6 had no test at all**.
One source, two consumers, no copy to drift.

## Consequences

**Good**

- **Baselines stop going stale, and ADR-0016 listed that as an accepted cost.** Its own
  Consequences said *"a real speed-up leaves the baseline generous until somebody re-records"*.
  Now something asks, every run.
- **The rule that decides §6 is tested.** Four tests, including a 4000-point sweep proving the
  three branches partition the line with no gap between them — a `>` on one side and a `>=` on
  the other would leave a value with no answer, and three sample points would step over it.
- **`[measured 2026-09-01]` proven through the real harness, not only the pure function.**
  Three baselines injected for this CPU: in-band prints the band and passes, over prints
  `OVER BASELINE` and still panics `finish`, under prints `UNDER BASELINE` and does **not**.

**Bad, and these are the price**

- **A knob that can be turned now turns both ways.** ADR-0016 already noted the margin is a
  knob; widening it to silence an `Under` also widens the ceiling. The mitigation is unchanged
  — it sits in a data file where a change shows up in the diff, with `n` and a date beside it.
- **False reports will happen**, for the asymmetry in decision 3. Each costs a re-record.
- **`--strict` is the only place it bites, and `--strict` needs a §9 machine.** On any other
  box `bench.sh` refuses `--strict` before it reaches this check, so **the fatal half of this
  decision is unverified on the machine it was written on** — see the plan's delivery log.
- **A fourth gate turned out to be Linux-only and nobody had noticed.** `[measured 2026-09-01]`
  `bench.sh` used `mapfile`, which is bash 4+; macOS ships bash 3.2, so the script had **never
  run on a development laptop** — it died before measuring anything. Fixed to a `while read`
  loop in the same commit. Its *numbers* are still worthless off a §9 box; its *behaviour* is
  machine-independent and was unverifiable for no reason at all.

## Open questions

1. **Should `low_margin` be its own column?** It needs `n ≥ 20` runs on a §9 machine. Until
   then the floor is conservative in the direction that produces false reports rather than
   false passes, which is the right way round.
2. **Should `bench` take a closure that returns its work's result?** That attacks the *cause* —
   a benchmark whose output nobody reads — rather than the *class*. It touches every bench file
   and belongs in its own plan.
3. **Does CI run `bench.sh --strict` anywhere?** It does not, and on a shared runner pool it
   should not. So on today's infrastructure `Under` is only ever a report, and the fatal half
   waits for a human at a §9 desk.
