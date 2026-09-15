# A tight spread inside one procedure did not reproduce across two

> `[measured 2026-09-14]` on the `DESIGN.md` §9 desktop, while taking the first TLS latency
> figures ([plans/2026-09-04-tls.md](../plans/2026-09-04-tls.md) step 6-M). One arm of a
> twenty-run procedure reported a spread of 1.008, and the identical procedure, run again half
> an hour later in the same boot, moved that arm's median p50 by 15.9%. Two other arms' p99 moved
> by 7% and 18% between the same two procedures.
>
> **`[to testing-skills]`**

## What happened

`scripts/w2w-baseline.sh` runs each arm 20 times — 20 000 timed round trips a run — and
publishes the median of the 20 per-run p50s (and of the per-run p99 and p99.9). Beside it, as its
statement of dispersion, it prints a *spread*: the largest per-run p50 over that median. Eight
arms, one binary, one command, threads pinned to isolated cores, `scripts/check-machine.sh`
reading `pass 12 fail 0 unknown 1` before and after.

The procedure ran twice in one boot, 07:25–07:51 and 07:56–08:22, with the same command and the
same binary — but not with nothing happening in between; what was recorded is listed below. Both
summaries are in [measured-costs.md](measured-costs.md), section *TLS on the wire*, verbatim.

| Arm | procedure 1: median p50 · runs · spread | procedure 2: median p50 · runs · spread | p50 moved |
|---|---|---|---|
| `hft`, admin, `userspace` | 20 774 · 20 639 .. 20 930 · 1.008 | 17 473 · 17 333 .. 17 813 · 1.019 | **−15.9%** |
| the other seven arms | spreads 1.003–1.014 | spreads 1.005–1.012 | −0.2% to −2.5% |

All in nanoseconds, `[measured 2026-09-14]`. **At p50, the two ranges do not overlap**: the
slowest run of procedure 2 (17 813) is 2 826 ns faster than the fastest run of procedure 1
(20 639). Every one of the twenty runs moved, and all eight arms' p50 moved in the same direction.

**At p99 it was not one arm.** `hft`, admin, kTLS read p99 37 345 in procedure 1 and 30 447 in
procedure 2 (−18.5%). Procedure 1's per-run p99 for that arm fell in **two clusters** — 8 runs at
30 928–32 141 and 12 at 37 220–37 841 — and its median landed in the upper one; all 20 runs of
procedure 2 read 29 887–30 699. `hft`, app, kTLS went the other way, 36 409 → 39 029 (+7.2%). The
p50 of both arms moved under 2.6%. The moved `userspace` arm's p99 moved −11.8% and its p99.9
−7.5%.

**And one level shift sat inside a single procedure.** Procedure 1's `standard`, admin, kTLS arm
read p50 29 305–29 506 over runs 1–10 and 28 704–28 965 over runs 11–20. Its spread printed
1.013. Procedure 2's twenty runs read 28 634–28 985 — the level procedure 1 had shifted to.

**A spread column that cannot say how far down a run fell.** In procedure 2 the `hft`, app, `off`
arm printed spread **1.008** while five of its twenty runs put their p50 at 17 323, 17 523,
17 864, 18 976 and 19 136 against a median of 19 998 — the lowest of them 13.4% under it. The
spread is maximum over median. A fast run does move it slightly, by lowering the median — with
those five runs replaced by 20 050 the column reads 1.005 — but **a run 13.4% under the median
and a run 0.3% under it move it by exactly the same amount**. The same summary printed
`(across runs: 17323 .. 20158)`, beside a spread that could not register that range.

**What differed between the two procedures — candidates, none claimed as a cause.** Before
procedure 1: a machine check whose quiet row failed at 07:24:03, the release build (binary
modified 07:24:19), a thrown-away run at 07:25:18, and about seven minutes since boot. Between
procedure 1 and procedure 2: both non-negotiable-4 scripts under `strace` (started 07:50:59 and
07:51:01), a machine check at 07:51:32, two single runs at 07:53, seven single runs at
07:54:28–07:55:19 with an editor session active, and 38 minutes since boot by 07:56:27. Nothing
was varied to separate any of these.

**The numbers, without an interpretation.** The moved arm's median per-run *minimum* went
17 644 → 17 147 (−497 ns), in line with the other seven arms' minima (−255 to −656 ns), while its
median p50 went −3 301 ns. Procedure 1's per-run minima for that arm were 17 523–17 774, every one
of them above the p50 procedure 2 published.

**What made anyone look.** Between the procedures, a single run of the same arm read p50
17 553 against the 20 774 procedure 1 had just published. Seven more single runs followed: the
four of that arm read 17 423–17 613, while the runs of two sibling arms sat only 2–3% under their
procedure-1 medians. None of those single runs had a per-run quiet check, so none is a publishable
figure — they are the reason the whole procedure was run again rather than either number being
published. They are quoted verbatim in [measured-costs.md](measured-costs.md), *TLS on the wire*,
§2. **Had nobody taken a single run afterwards, procedure 1's 20 774 would have been the figure.**

**The commit was not in the output either.** HEAD (`1178f4d`) and a clean tree were read by the
operator's own shell around the build; neither the baseline script nor the binary prints them, so
the two summaries alone do not say which code produced them.

## Why it is easy to walk into

Twenty runs with a spread of 1.008 look like a figure reproduced twenty times. They are one
procedure: the twenty runs share the boot, the time since boot, the thermal state, the memory
layout, what ran just before, and whatever else a sequence of runs has in common, and anything
that holds for a whole procedure can hold at a different value for the next one. A tight figure
inside a procedure says the runs agree with **each other**.

This repository's publication procedure publishes one twenty-run median. Run once at 07:25,
**20 774 ns** would have gone into the design document with 1.008 beside it as its evidence; run
once at 07:56, **17 473** would have. The same command made both. The tail is worse: a median
p99 taken over two clusters of runs lands in whichever cluster holds more than half of them.

And a maximum-over-median column cannot register how far below the median a fast run falls, so
even inside one procedure it under-reports whenever the outliers are fast ones.

## The rule

- **A dispersion figure bounds the procedure it was computed in, not the next one.** A figure is
  reproduced when the whole procedure has run at least twice, separated in time, and the medians
  agree within a stated tolerance — at every percentile that is published, not only the median.
  Until then it is one observation of a median, however many runs are inside it.
- **When two procedures disagree, publish both; do not average them.** An arm that moves while
  its siblings in the same two procedures do not is a result about that arm.
- **Report dispersion from both sides, and per percentile** — the range, or minimum over median as
  well as maximum over median, for the tail figures as well as the median. A one-sided column
  cannot measure a fast outlier, and a p50 column says nothing about a bimodal p99.
- **Name everything that happened between the procedures, and call each a candidate.** Time since
  boot, a build, a traced run, single runs taken to check a figure — all of them moved together
  with the result here, and nothing isolated any one.
- **Make the procedure record what it measured**: the commit, whether the tree was clean, and the
  time since boot, printed by the procedure itself rather than read by whoever ran it.

## What guards it

`[2026-09-14, later]` **Two of the three halves have a guard; the third is a rule.**
[ADR-0068](../decisions/ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)
(accepted) makes a published figure two procedures shown side by side, and step S2 of
[the-second-linux-desk](../plans/2026-09-04-the-second-linux-desk.md) *Sửa 3* changed
`scripts/w2w-baseline.sh` to match:

- **A dispersion that reads both sides.** A pure `dispersion` function prints `min/median` and
  `max/median` for p50, p99 and p99.9 (and the wire columns). `scripts/check-w2w-baseline-summary.sh`,
  run by CI's `script-logic` job, feeds it this entry's own twenty p50s — median 19 998, min/median
  **0.866**, max/median **1.008** — and asserts that *"a run 13.4% under the median is visible from
  the min side"*. Reversal, 2026-09-14: `min/median` removed from the function → red on that sentence,
  `pass 0 fail 4`; restored → `pass 4 fail 0`. The old `spread max/median` line is kept, so records
  from before can still be compared.
- **The procedure records what it measured.** The header prints `commit`, `tree`, `uptime`, the
  `binary` sha256 and mtime (and the generator's over ssh), and `output <dir>`; every run's raw
  output is kept under that directory with a `summary.txt`. **No test asserts those header lines** —
  they were checked once against a fake `w2w` (`bash -x`), and a real run is owed in boot B's build
  slot.
- **Reproduced before it is published** is ADR-0068's rule, applied by whoever publishes: nothing
  runs the procedure twice or compares the two. `DESIGN.md` §8's procedure text is rewritten to it at
  boot B step B2.

## It happened again, 2026-09-15

`[measured 2026-09-15]` boot B of
[plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md), two
full 20-run procedures apart on the same boot (00:58–02:54, 02:54–04:47): **every zero-interval
loopback arm read 5.0–6.7% faster in procedure 2 than procedure 1 at p50** — B2's four arms
(`hft`/`standard` × admin/app, combined process) and B8's two `fixbolt` arms (admin/app, loopback
split against `matthart1983/nanofix`) all moved the same direction, while each procedure's own
in-procedure dispersion stayed tight (B2: min/median ≥ 0.995, max/median ≤ 1.011 in all eight
cells). B4's paced arms (1 ms, 10 ms, 1 s) and B8's `nanofix` arms did **not** show this: they
read within 0.5–5.2% of each other across the same two procedures, tighter than the zero-interval
arms. A third procedure of B2, run 04:48–05:01 with no build in between, read within 0.1% of
procedure 2 for `hft` and 2.0–2.2% above procedure 2 (below procedure 1) for `standard` — **so
procedure 1 was the outlier, not a monotone drift with elapsed time**. Candidate recorded, not a
cause, same as before: procedure 1 started about seven minutes after the build slot (cargo
builds, rustdoc, the Mac rebuild) ended; nothing was varied to isolate it. Full tables and
verdicts: [measured-costs.md](measured-costs.md), *Boot B, 2026-09-15*, sections B5, B7, B8 and
"B2, procedure 3".

## Related

- [recording-a-baseline-changed-the-baseline.md](recording-a-baseline-changed-the-baseline.md)
  — twenty clean runs, and the next run left the band they defined. There the cause was found;
  here it was not.
- [measured-costs.md](measured-costs.md), *The wire, at last* (2026-09-02) — where the spread
  column was introduced, and called the tightest measurement this repository had taken. It was,
  inside one procedure.
