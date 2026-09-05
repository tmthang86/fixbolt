# ADR-0052 — Two of item 49's candidates are retired with numbers, item 14's touched set is measured, and one baseline line disagrees with its neighbours on purpose

> **Status:** Accepted (2026-09-05) · **Amends:** the kernel-bypass Term 2 analysis in
> [measured-costs.md](../reference/measured-costs.md) (the struct size it was built on, and both
> of the bounds it offered) · **Departs from:**
> [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md) and
> [ADR-0031](ADR-0031-a-baseline-is-a-band.md) for exactly one line, with the reason recorded
> **Plan:** [what-is-left-and-what-a-message-touches](../plans/2026-09-05-what-is-left-and-what-a-message-touches.md)

## Context

Two open items were the last things on `STATUS.md` that needed the §9 desktop and could be
measured today.

**Item 49** — `tools/w2w --path app` costs **3 898 ns** more per round trip than `--path admin`,
and committed benchmarks accounted for ~1 094 ns of it. Four candidates were named for the rest
and none had ever been measured. Item 39 was the warning: the *largest named* candidate, the
dictionary pass, had turned out to be 17.4% of what it was nominated to explain.

**Item 14** — how much of a `Connection` a message touches, which decides where the cache wall
is. The section offered two bounds 14× apart, **N ≈ 9** and **N ≈ 128**, both computed from
`size_of::<Connection<..>>()` = 54 600 bytes measured on 2026-08-30.

## What was measured

§9 desktop, AMD Ryzen 7 3700X, alignment pinned (ADR-0049), medians of **20 clean runs out of a
campaign of 22**, `pass 12 fail 0 unknown 1` on every run used. Two runs were discarded by their
own machine verdict.

**The four payload sizes had never been measured.** `strace -f -e trace=sendto` on the release
binary at its default flags, last 2 000 sends of each direction, all identical: administrative
**83 in / 87 out**, application **149 in / 191 out**. The row said "149 in and ~200 out against
79 and ~70" and one of the four was right.

**The struct is not the size the analysis was built on.** `size_of::<Connection<Loopback,
Acceptor, MemJournal<64,512>, 64, 4096, 8192>>()` is **21 456 bytes**, not 54 600, and
`size_of::<MemJournal<64,512>>()` is **32**, not 33 288. [ADR-0046](ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)
boxed the ring on 2026-09-04 and raised it to 4 096 slots; the 2 MiB moved to the heap. Nothing
pointed at the paragraph that depended on it.

| Case | ns |
|---|---|
| `TCP loopback, 8 in 8 out` → `8192 in 8192 out` | 12 528.8 → 14 890.9 |
| `journal put, 191 bytes, walking` | 8.9 |
| `engine turn, 1 busy sessions` → `64 busy sessions` | 1 659.8 → 121 009.1 |
| `engine turn, 1 busy, ring 64 / 512 / 4096` | 1 635.5 / 1 654.8 / 1 657.7 |

## Decisions

**1. Item 49 is narrowed, not closed, and the two priced candidates are retired with numbers
rather than with prose.** The kernel-copy term is **24.5 ns** and `Journal::put` is **8.9 ns**;
together 0.9% and 0.3% of the 2 804 ns they were candidates for. The accounted subtotal moves
from ~1 094 to ~1 128 ns and the remainder from ~2 804 to ~2 770. The item stays open, holding
one candidate — the engine's framing and read-buffer management — and one unmeasured term, the
session's own `Heartbeat` serialise.

**2. The payload term is read off a lever, never off the two sizes it is about.** The real sizes
differ by 170 bytes inside a round trip of ~12 600 ns; across three repetitions their direct
difference read −4, +13 and +46 ns. Both real sizes keep committed cases anyway — non-negotiable
10 wants the bytes a claim is about to be timed somewhere — but the *number* comes from the
8 → 8192 slope, and the slope predicting the real pair is the only evidence the slope is about
payload at all.

**3. Item 14's upper bound is refuted by the shape of the curve, not by arithmetic.** Per-message
cost climbs smoothly — 1.018 at N=8, 1.031 at N=16, 1.067 at N=32, 1.139 at N=64 — with **no step
at the 512 KiB L2 edge**, which a message touching most of its connection would produce at N ≈ 20.
The touched set is **on the order of 2 to 4 KiB**. The bound this project already had written
down, and had no evidence for, is the one that survived.

**4. A 2 MiB allocation is not a cache cost.** Identical work at one session with the ring swept
from 4 KiB to 2 MiB moves 1.5% and **not monotonically**. `put` writes 191 bytes at a predictable
512-byte stride. Only a large *touched* set costs.

**5. One line of `benches/baselines.tsv` carries a value, a sample size and a margin that all
disagree with its neighbours, and the disagreement is the point.** `journal put, 191 bytes, one
slot` was measured at **8.2 ns** over 20 clean runs, then read **6.3** on the binary that
appending those 20-run results produced — the table is `include!`d into `harness.rs`, so
recording a baseline changes the binary the baseline came from. Alignment was pinned and did not
prevent it. The line carries **6.3 / margin 1.35 / n = 8**: 6.3 because it is the binary anything
will ever be compared against, n = 8 because that is what it is, and 1.35 because the ladder fits
`max/median` *within* one binary (1.159) while the swing this line must survive is the
*cross-binary* one (1.30).

## Consequences

**Good.**

- Two candidates are dead with numbers attached, so nobody prices them again. The one that
  remains has almost all of the 2 770 ns, which makes the next step obvious instead of a choice
  among four.
- A stale measurement was found and corrected, and the mechanism that hid it is written down: a
  correct number, invalidated by a refactor a week later, with no document pointing at it.
- Three new bench targets were added and **no pre-existing baseline moved**, which is the direct
  evidence for the "new target, never a new case in an existing binary" rule this plan was built
  around.
- The engine's per-message cost at 64 sessions is now a measured 13.9% above one session, so
  `GUIDE.md`'s density arithmetic has a cache term instead of a gap.

**Bad, and each is a real cost.**

- **`benches/density.rs` is expensive.** The sweep is O(N) and a whole `bench.sh` run went from
  ~4 to ~7.5 minutes, which is why the owner cut N=128 and why the touched-set figure is a range
  rather than a number. Every future baseline campaign pays this.
- **The touched-set figure rests on a model the data partly contradicts.** At N=16 the model
  predicts no excess and 50.8 ns is measured. 2–4 KiB is honest about that; a single number would
  not have been.
- **Departing from the ladder for one line weakens the rule.** ADR-0031 made the margin
  mechanical precisely so it could not be nudged, and this is a nudge with a paragraph attached.
  The alternative — recording 8.2 and watching the case sit permanently `UNDER BASELINE` — was
  worse, but the precedent is now available to anybody who wants a wider band.
- **The absolute figures in `payload.rs` are environment-bound and will be misread.** ~12.5 µs
  for four syscalls is thirty-two times a bare syscall on this box. The module doc and
  `measured-costs.md` both say so where the numbers are, and that will not be enough forever.
- **Item 49 is still 71% unexplained**, and this ADR moved it by one percentage point.

## Alternatives rejected

**Close item 49 as "understood".** Two candidates priced and 71% left is not understanding.
The item stays open with a smaller remainder and a named survivor.

**Subtract the two real payload sizes directly.** It is the obvious reading of the two cases and
it is wrong: the difference changes sign between repetitions. Publishing it would have been a
number with no measurement behind it, which is the failure `CLAUDE.md` §10 names as accepting a
cause because a knob moved with it.

**Record `journal put, one slot` at 8.2 and accept the `UNDER BASELINE` line.** This project
reports rather than fails that state, so it would have printed every run and been ignored — the
exact shape of a check nobody reads.

**Remove the `one slot` case for being inconvenient.** It is the control that separates the copy
from the address, and dropping a case because it went red is the failure mode §10 warns about in
its own words. It stays, with its awkwardness recorded.

**Measure the touched set directly with a memory tracer.** `valgrind` is not installed and
`perf_event_paranoid` is 4; both are changes to the owner's machine. The wall is what the
question is actually about, so the wall was measured instead.
