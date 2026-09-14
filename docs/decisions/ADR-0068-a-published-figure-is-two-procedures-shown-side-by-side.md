# ADR-0068 — A published latency figure is two procedures, shown side by side

**Status:** Proposed (architect, 2026-09-14 — *Sửa 3* of
[the-second-linux-desk](../plans/2026-09-04-the-second-linux-desk.md), Điều 2; the owner decides
at Q13 of that section) · **Date:** 2026-09-14 ·
**Plan:** docs/plans/2026-09-04-the-second-linux-desk.md, Sửa 3, Điều 2 and step S2
**Answers:** the *needs a plan* sentence of `STATUS.md` open item 85 — *how a figure is
reproduced before it is published, at every published percentile; a dispersion that reads both
sides; and HEAD and tree state printed by the procedure itself*.
**Changes nothing in:** [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md)
and [ADR-0031](ADR-0031-a-baseline-is-a-band.md) — the bench baselines keep their band and their
`n ≥ 20`; this ADR is about the wire figures `tools/w2w` produces and `DESIGN.md` §8 publishes.
[ADR-0023](ADR-0023-section-9-records-the-cpu-mitigations.md)'s rule that an A/B difference never
stands beside a §8 figure is kept and generalised to every A/B arm (decision 4).

## Context

`scripts/w2w-baseline.sh` runs an arm twenty times and publishes the median of the twenty per-run
p50, p99 and p99.9, with one dispersion column: the largest per-run p50 over that median.
`[measured 2026-09-14]`, on the §9 desk, the identical procedure run twice in one boot moved one
arm's p50 by 15.9% (20 774 → 17 473 ns) while its spread inside procedure 1 read 1.008; two other
arms' p99 moved 7.2% and 18.5%, one of them because procedure 1's per-run p99 fell in two
clusters and the median landed in the upper one. Five runs 13.4% under a median moved the
max-over-median column by exactly as much as a run 0.3% under it would have. Neither summary
printed the commit or whether the tree was clean. The whole reading is
[a-tight-spread-inside-one-procedure-did-not-reproduce-across-two](../reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md);
its *rule* section says what a figure needs, and nothing enforced it. `DESIGN.md` §8 already
shows that day's two procedures side by side as a stop-gap.

Boot B of the plan above is about to take every loopback and NIC figure §8 has waited for. Taking
them with the old procedure would publish one observation per arm, with the same blind spots.

The literature on this is old: Mytkowicz et al. (ASPLOS 2009) show that the environment of a
measurement — link order, environment size, what ran before — moves results by more than the
effects being measured and can flip their sign, and that a tight variance inside one setup says
nothing about the next. Their remedy is to vary the setup and to report the spread across
setups; the cheapest form of that here is to run the whole procedure twice, apart in time, and
show both.

## Decision

1. **A published figure is a pair.** A wire figure in `DESIGN.md` §8, or one that
   `docs/reference/measured-costs.md` presents as a figure rather than as a reading, is the
   median of **two complete runs of the committed procedure** (`scripts/w2w-baseline.sh`, the
   arm's full `RUNS`), on the same commit, the same binary (by sha256), a clean tree and the same
   boot, separated by at least one pass over other work — not less than thirty minutes — each
   with its own machine block. **Both medians are shown, never averaged**, with their
   difference in percent beside them, at every percentile published.
2. **Reproduced means: within 5% at every published percentile.** Two procedures agree when,
   at each of p50, p99 and p99.9, the two medians differ by at most 5% of the smaller. The
   number is provisional and chosen, not measured: on 2026-09-14 the arms nobody doubted moved
   0.2–2.5% at p50 and up to 3.0% at p99.9 between procedures, and the three that did not
   reproduce moved 7.2%, 15.9% and 18.5%. A measured band, if item 85 ever produces one,
   replaces it by a new ADR.
3. **Not reproduced is still published — as a pair, marked, and it meets nothing.** The two
   columns go into the table with the arm marked *not reproduced*; no gate, target or
   *Definition of Done* box reads either column as met; everything that happened between the
   procedures is listed as a candidate, none as a cause. A step that owes a target verdict
   (DESIGN.md §6's NIC row, say) reports *not proven* for that arm.
4. **An A/B difference is one procedure and never a figure.** Arms whose point is a
   difference — `busy_read` 0 ↔ 50, an IRQ pinned onto the engine core, `--wire-timestamps` on
   ↔ off, EEE on ↔ off, `mitigations=off` — run once at `RUNS ≥ 10`, are labelled *A/B*, and
   are published only as a difference beside the pair they modify. This is ADR-0023's rule for
   boot C applied to every A/B.
5. **The procedure records what it measured.** `scripts/w2w-baseline.sh` prints, in its header:
   the commit (`git rev-parse --short HEAD`), the tree state (`git status --porcelain`, count
   and paths), time since boot, the binary's sha256 and mtime — and the generator's binary
   hash when a `GENERATOR_SSH` host runs it; keeps every run's raw output on disk under one
   directory it names in the header; and prints dispersion **from both sides and per
   percentile** — `min/median` and `max/median` for p50, p99 and p99.9 — beside the existing
   `spread` line, which stays so that older summaries still read the same way. The summary
   function is pure and has a regression test, `scripts/check-w2w-baseline-summary.sh`, fed the
   numbers of the 2026-09-14 trap.
6. **Scope.** Wire figures from `tools/w2w`. Bench baselines (`benches/baselines.tsv`) are not
   touched; whether their ladder needs a second procedure is `STATUS.md` item 52's remaining
   half and is not decided here.

## Alternatives considered

- **One procedure and a wider band.** Cheaper by half. But a band bounds the runs inside one
  procedure and nothing else; 1.008 was inside a 15.9% move. Rejected: it is the failure mode
  the trap records.
- **Pool the runs of both procedures into one distribution.** Hides a bimodal p99 the way the
  median of procedure 1 did; and an arm that moved is a result about that arm, which pooling
  erases. Rejected; both columns are kept instead.
- **Three or more procedures.** Better evidence, and a majority when one disagrees. Rejected for
  now on desk time: two already doubles the published arms' cost, and two disagreeing columns
  already say *not reproduced*, which is the only verdict a third would sharpen.
- **Setup randomisation** (Mytkowicz et al.): vary environment size, link order, run order.
  Right in principle, and the order of arms between the two procedures is the one variable this
  ADR asks to keep fixed rather than to vary — a controlled repeat first, randomisation when
  item 85 has a plan of its own. Not rejected; deferred.
- **A separate plan for item 85 before any §8 figure.** The plan would propose this ADR and the
  script change, and boot B would be spent producing raw runs nobody may publish. Rejected on
  Q6's spirit: a day of the §9 machine should end with figures.

## Consequences

**Good**

- Every §8 figure taken from now on carries its own reproduction evidence, at every percentile,
  with the commit and the tree it came from — `CLAUDE.md` §2 non-negotiable 10's *the benchmark
  that produced it* becomes checkable rather than asserted.
- A fast outlier is visible; a bimodal tail is visible as two columns that disagree.
- The A/B arms stop being a temptation: they cannot be a figure by construction.
- Item 85's *no guard* sentence closes with a named test and a named procedure.

**Bad, and accepted**

- **Desk time doubles for every published arm** — on boot B roughly +2½ hours, mostly the
  `--interval` arms. The plan's cut order says what goes first if the boot runs long.
- **5% is a chosen number.** An arm that moves 4.9% is *reproduced* and one that moves 5.1% is
  not, and nothing measured put the line there. The consequence is bounded: the pair is
  published either way; only the label changes.
- **Reproduced is not right.** Two procedures share the boot, the thermal state, the binary's
  memory layout and whatever else a day has in common; Mytkowicz's bias survives a controlled
  repeat. This ADR buys a tolerance statement, not a truth.
- **A table may hold a pair and no verdict** for a long time; §6's NIC row can read *measured,
  not reproduced* after a full day on the cable. That is the honest state, and it is worse to
  read than a number.
- `scripts/w2w-baseline.sh` grows a directory of outputs, a header, a test, and one more
  variable (`W2W_EXTRA`, added in the same step for B5) — more surface for the next trap.

## Sources

- [a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md](../reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md)
  `[measured 2026-09-14]`, and `DESIGN.md` §8 *The round trip under TLS, measured* (the two
  procedures side by side).
- T. Mytkowicz, A. Diwan, M. Hauswirth, P. F. Sweeney, *Producing Wrong Data Without Doing
  Anything Obviously Wrong!*, ASPLOS 2009 —
  <https://users.cs.northwestern.edu/~robby/courses/322-2013-spring/mytkowicz-wrong-data.pdf>
  (searched 2026-09-14): measurement bias from environment size and link order exceeds the
  effect under study and flips its sign; remedies are setup randomisation and causal analysis.
- `scripts/w2w-baseline.sh` lines 500–552 as of `9bee2f2`: one-sided `spread max/median`, p50
  only; no commit, no tree state, no output kept.
