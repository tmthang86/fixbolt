# A tight spread inside one procedure did not reproduce across two

> `[measured 2026-09-14]` on the `DESIGN.md` §9 desktop, while taking the first TLS latency
> figures ([plans/2026-09-04-tls.md](../plans/2026-09-04-tls.md) step 6-M). One arm of a
> twenty-run procedure reported a spread of 1.008, and the identical procedure, run again half
> an hour later in the same boot, moved that arm's median by 15.9%.
>
> **`[to testing-skills]`**

## What happened

`scripts/w2w-baseline.sh` runs each arm 20 times — 20 000 timed round trips a run — and
publishes the median of the 20 per-run p50s. Beside it, as its statement of dispersion, it
prints a *spread*: the largest per-run p50 over that median. Eight arms, one binary, one
command, threads pinned to isolated cores, `scripts/check-machine.sh` reading
`pass 12 fail 0 unknown 1` before and after.

The procedure ran twice in one boot, 07:25–07:51 and 07:56–08:22, with nothing changed between
them. Both summaries are in [measured-costs.md](measured-costs.md), section *TLS on the wire*,
verbatim.

| Arm | procedure 1: median p50 · runs · spread | procedure 2: median p50 · runs · spread | moved |
|---|---|---|---|
| `hft`, admin, `userspace` | 20 774 · 20 639 .. 20 930 · 1.008 | 17 473 · 17 333 .. 17 813 · 1.019 | **−15.9%** |
| the other seven arms | spreads 1.003–1.014 | spreads 1.005–1.012 | −0.2% to −2.5% |

All in nanoseconds, `[measured 2026-09-14]`. **The two ranges do not overlap**: the slowest run
of procedure 2 (17 813) is 2 826 ns faster than the fastest run of procedure 1 (20 639). Every
one of the twenty runs moved, and all eight arms moved in the same direction.

**A second, smaller one sat inside procedure 2.** The `hft`, app, `off` arm printed spread
**1.008** while five of its twenty runs put their p50 at 17 323, 17 523, 17 864, 18 976 and
19 136 against a median of 19 998 — the lowest of them 13.4% under it. The spread is maximum
over median, so **a run below the median cannot move it**. The same summary line printed
`(across runs: 17323 .. 20158)`, beside a spread that said nothing about that range.

**What differed between the two procedures, as far as anything recorded:** time since boot
(about 7 minutes against 38) and that procedure 1 was the first after the reboot, behind one
thrown-away run. **Neither is claimed as a cause** — nothing was varied to test either, and
both moved together with the result.

**What made anyone look.** Between the procedures, a single run of the same arm read p50
17 553 against the 20 774 procedure 1 had just published. Seven more single runs followed: the
four of that arm read 17 423–17 613, while the runs of two sibling arms sat only 2–3% under their
procedure-1 medians. Those single runs had no per-run quiet check, so they are not publishable
figures — they are the reason the whole procedure was run again rather than either number being
published. They are quoted verbatim in [measured-costs.md](measured-costs.md), *TLS on the wire*,
§2. **Had nobody taken a single run afterwards, procedure 1's 20 774 would have been the figure.**

**One descriptive fact, which makes it less strange and no less unexplained.** In procedure 1
that arm's median per-run *minimum* was 17 644 ns — within 1% of the p50 procedure 2 then
published. The faster level was present inside every run of procedure 1; the median of the
distribution did not sit on it.

## Why it is easy to walk into

Twenty runs with a spread of 1.008 look like a figure reproduced twenty times. They are one
procedure: the twenty runs share the boot, the time since boot, the thermal state, the memory
layout and whatever else a sequence of runs has in common, and anything that holds for a whole
procedure can hold at a different value for the next one. A tight figure inside a procedure
says the runs agree with **each other**.

This repository's publication procedure publishes one twenty-run median. Run once at 07:25,
**20 774 ns** would have gone into the design document with 1.008 beside it as its evidence; run
once at 07:56, **17 473** would have. The same command made both.

And a maximum-over-median column under-reports even inside one procedure whenever the outliers
are fast ones.

## The rule

- **A dispersion figure bounds the procedure it was computed in, not the next one.** A figure is
  reproduced when the whole procedure has run at least twice, separated in time, and the medians
  agree within a stated tolerance. Until then it is one observation of a median, however many
  runs are inside it.
- **When two procedures disagree, publish both; do not average them.** An arm that moves while
  its siblings in the same two procedures do not is a result about that arm.
- **Report dispersion from both sides** — the range, or minimum over median as well as maximum
  over median. A one-sided column cannot see a fast outlier.
- **Name what differed between the procedures, and call it a candidate.** Time since boot and
  "the first procedure after a restart" moved together with the result here, and nothing
  isolated either one.

## What guards it

**No regression test, and no gate.** `scripts/w2w-baseline.sh` still publishes from one
procedure and still prints a one-sided spread, and `DESIGN.md` §8's procedure still publishes
from one twenty-run median. `STATUS.md` open item **85** is where changing that is tracked.
Until then, `DESIGN.md` §8's TLS table shows both procedures side by side rather than one.

## Related

- [recording-a-baseline-changed-the-baseline.md](recording-a-baseline-changed-the-baseline.md)
  — twenty clean runs, and the next run left the band they defined. There the cause was found;
  here it was not.
- [measured-costs.md](measured-costs.md), *The wire, at last* (2026-09-02) — where the spread
  column was introduced, and called the tightest measurement this repository had taken. It was,
  inside one procedure.
