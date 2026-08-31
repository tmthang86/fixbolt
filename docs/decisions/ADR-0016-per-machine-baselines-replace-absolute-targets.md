# ADR-0016 — Per-machine baselines replace the absolute timing targets

> **Status:** Accepted · **Date:** 2026-08-31
> **Supersedes:** the absolute timing targets published in `DESIGN.md` §6. It does not
> supersede any earlier ADR.
> **Amends:** nothing in `CLAUDE.md` §2. This decision *enforces* non-negotiable 10; it does
> not relax it.
>
> **Accepted by standing delegation.** `[2026-08-30]` the owner delegated plan-writing and
> plan approval to the agent working in this repository, and `[2026-08-31]` chose the shape of
> this decision directly — lower the target to something reachable, keyed on a per-machine
> baseline, across the whole of §6 rather than the serialise row alone, with the absolute
> target column removed rather than merely lowered. Nobody read the reasoning below on the
> owner's behalf.

## Context

`DESIGN.md` §6 published one absolute nanosecond figure per timing gate. Two independent
findings say that column had stopped carrying information.

**The serialise target of 60 ns was never a measurement of this engine.** `DESIGN.md` §4, D9,
states its own provenance: *"This is how the fastest commercial engines reach tens of
nanoseconds per serialise, and it is why the published serialise target in §6 is 60 ns, not
150."* It is a figure read about other people's software. The 150 ns parse target is a
different kind of number entirely — §6 anchors it to 139 ns measured here, on an Apple M5, on
2026-08-27. One column held both, and only the second kind can gate anything.

No machine has come close to 60 ns: **93.8** (Apple M5) · **177.6–199.4** (shared Xeon
container) · **240.5** (§9 desktop, `[measured 2026-08-31]`). The plan
[serialise-and-the-60ns-target](../plans/2026-08-31-serialise-and-the-60ns-target.md)
then measured where the time goes and closed the question: about **31 ns** is spent before the
first variable field is written — 51% of the whole 60 ns target on a message carrying nothing
— plus ~7 ns per field in `put`. Removing the slot scan *entirely* still leaves ~116 ns. The
fix that `STATUS.md` open item 11 had proposed was written, measured and reverted: predicted
−36 ns, measured **+5.2 ns**.

**The regression ceilings had the same disease, from the other direction.** `STATUS.md` open
item 20 measured `ring, one way` on three machines: **260.9 ns** (Ryzen 7 3700X), **270.7–272.9**
(EPYC 9V74), **327.2–331.1** (EPYC 7763) — a **21% spread between two machines of the same
vendor**, against ~1% within either. The single 260 ns ceiling sat **0.3% below the fastest of
the three**. That item's own conclusion was already written: *"a per-machine baseline is
viable, keyed on the CPU model that `scripts/check-machine.sh` now prints with every figure; a
single absolute ceiling across the pool is not."*

So §6 was publishing targets that could not be met and asserting ceilings that no machine
passed. Both failure modes end the same way: a gate that is always red is a gate somebody
switches off.

## Decision

1. **The timing rows of `DESIGN.md` §6 no longer carry an absolute target.** The gate is
   *"no regression past this machine's recorded baseline, times this case's margin."*

2. **Baselines are data, not code.** `benches/baselines.tsv`, one line per (CPU model, case),
   carrying the baseline in ns, the margin, the sample size, the date, and the
   `scripts/check-machine.sh` verdict of the run that produced it. The CPU model string is the
   key, in the exact spelling `check-machine.sh` prints — so the machine block that already
   travels with every figure *is* the lookup key.

3. **A baseline is the median of N ≥ 20 whole suite runs**, recorded only from a machine
   reading `pass 10 fail 0`. Not one run: the same box gives 267.2–335.7 ns for `ring, one
   way`.

4. **The margin is per case, derived from that case's own measured spread** — the smallest
   step of `1.10 · 1.15 · 1.20 · 1.25 · 1.30 · 1.35` that is at least the max/median observed
   over those N runs, with 1.10 as the floor. **A single margin was tried first and does not
   work**: see Consequences.

5. **A CPU with no line in the file yields `NO BASELINE`, which is not a pass.** It prints as
   its own state, `scripts/bench.sh` counts it on its own summary row, and `bench.sh --strict`
   — what a §9 machine runs — treats one as fatal. Without `--strict` it is reported, because
   CI runs on a shared pool whose CPUs deliberately have no baseline.

6. **Ambition moves to a `Stretch` row that is explicitly not a gate**, carrying a number this
   project measured — for serialise, the ~116 ns floor of the current `Part` shape — rather
   than one borrowed from somebody else's product.

7. **The `INVARIANT` benchmarks are untouched.** Allocation counts and message counts are the
   same answer on every machine; a per-machine baseline would be meaningless and their
   failures stay fatal everywhere.

## Alternatives considered

**Keep 60 ns and record the gap.** This is what §6 did for four months across three machines,
and the gap only widened. `CLAUDE.md` §7 already names the failure mode: *"a target that lives
in a comment is a wish."* A published target that no machine has ever met is the same wish
with a table around it.

**Lower the absolute target to ~150 ns.** Reachable-looking, since step 2 of the serialise plan
showed where ~31 ns of fixed cost and ~7 ns per field sit. Rejected: 150 ns would still be
unmet on the §9 desktop today (240.5), on the container (177.6–199.4) and met only on the M5.
It replaces a target no machine meets with a target one machine meets, and keeps the property
that made the number useless — that one figure means different things on different silicon.

**Set the absolute target just above the §9 desktop, ~250 ns.** Green on the reference machine
on day one. Rejected for the same reason in reverse: the M5 would clear it by 2.6× and never
report a regression, and the CI pool straddles it. This is precisely the disease the
per-machine key exists to cure.

## Consequences

**Good**

- Every published figure is now this project's own measurement, on a named CPU, with the
  machine verdict beside it. That is `CLAUDE.md` non-negotiable 10 made mechanical instead of
  remembered.
- A regression is detectable on any machine that has a baseline, including ones far slower or
  faster than the reference — which an absolute ceiling could never do.
- `[measured 2026-08-31]` The mechanism found a real defect on its first run. See below.
- `STATUS.md` open items 11 and 20 both close: 11's question is answered, 20's conclusion is
  the thing implemented.

**Bad, and named**

- **There is no single headline number any more.** "How fast is it?" now requires naming a
  machine. For a library that positions itself on latency this is a real cost in
  communication, and it is accepted because the alternative is a number that is wrong
  everywhere except one desk.
- **A machine with no baseline is only weakly guarded.** Without `--strict` it reports rather
  than fails, so CI on a shared pool watches nothing but the invariant benchmarks and the
  liveness check. That is a deliberate hole: a red CI on shared hardware is a red CI nobody
  reads. The guard that remains is `--strict` on the §9 box.
- **Baselines go stale.** A real speed-up leaves the baseline generous until somebody
  re-records it. There is no automation for that and this ADR does not invent one.
- **The margin is a knob, and a knob can be turned.** The mitigation is that it sits in a data
  file beside the `n`, date and verdict that justify it, so a hand-nudge is a visible diff and
  a margin off the published ladder is visibly off it. This is honesty by construction, not by
  enforcement, and it is the weakest part of this decision.
- **One margin would have been simpler and it does not work.** `[measured 2026-08-31]` over 21
  runs, nine of twelve cases held inside 7.6% of their own median while `inline deliver +
  reply` spanned 32%. A single margin wide enough for the worst case would have let `encode
  ExecutionReport` drift 241.6 → 326 ns unnoticed. Per-case margins are more file to maintain,
  and they are the price of the gate meaning anything on the narrow cases.

**What this decision does not claim**

- It does not make the engine faster. Five of twelve cases were over their old ceiling on the
  §9 desktop and every one of them is still exactly as slow; what changed is that §6 now
  describes them truthfully instead of failing against a number from another machine.
- It does not settle *why* `ring, one way` has a second mode at +24%. Open item 20 refuted
  five hypotheses — L3 placement, SMT, governor/boost, thermal, competing load — and this ADR
  budgets for the mode rather than explaining it.

## The defect the mechanism found on its first run

`[measured 2026-08-31]` Removing the `ceiling_ns` parameter from `Suite::bench` moved `inline
deliver + reply` from **6.3 ns to 1.3 ns**, reproducibly, with the timed loop byte-identical.
The case was checked for the obvious explanation — that the work had been optimised away — by
doubling the calls inside the closure: it read **2.6 ns**, an exact factor of two, three times
running. The work is real; the old figure was not.

So roughly **5 of the old 6.3 ns were the harness's own overhead**, and the three discrete
clusters that case produced across 21 runs — 6.3, 7.4, 8.2, with nothing in between — were
modes of the *measuring apparatus*, not of the engine. The constant's own doc comment had said
*"the harness's own overhead is the same order"* since 2026-08-30 and nobody had measured it.

This is recorded in [`reference/measured-costs.md`](../reference/measured-costs.md) and is
marked `[to testing-skills]`: an instrument that contributed 80% of its smallest reading, while
a comment beside it predicted exactly that and was never checked.
