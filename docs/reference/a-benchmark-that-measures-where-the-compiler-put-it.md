# A benchmark that measures where the compiler put it

> `[measured 2026-09-05]` — found closing `STATUS.md` open item 41,
> [plans/2026-09-02-the-baselines-and-the-pass-nobody-timed.md](../plans/2026-09-02-the-baselines-and-the-pass-nobody-timed.md)
> step 1c. **`[to testing-skills]`**
>
> The sibling of [a-benchmark-can-delete-its-own-work](a-benchmark-can-delete-its-own-work.md)
> and [a-benchmark-measured-its-own-fixture](a-benchmark-measured-its-own-fixture.md). Those
> two are about a benchmark timing the **wrong work**. This one times exactly the right work
> and still reports a number that is **11–24% a property of the binary rather than of the
> code under test**.

## The claim, and how confident it looked

`encode ExecutionReport (template)` had a per-machine baseline of **239.1 ns** on the §9
desktop, recorded from 24 whole runs with a margin of 1.10 — the tightest band on the ladder,
chosen because the case had been measured to hold inside 7.6% of its own median over 21 runs.

On 2026-09-02 the same case on the same box read **274–283 ns**, six runs of six over the
263.0 ceiling. It stayed there. `STATUS.md` item 41 recorded it as a possible regression with
two hypotheses, and named the leading one: ADR-0044 had changed `TemplateBuilder`, and the
only path from that to `encode` was the layout of `Template` itself.

## What it was

**Nothing in the encoder changed at all.** The jump is one commit,
`4396d6d feat(bench)!: a baseline is a band`, and that commit **touches no library source**:

```
crates/codec/benches/harness.rs    | 69 ++++--     the measuring harness
crates/codec/benches/verdict.rs    | 78 +++++      new, bench-only
crates/codec/tests/bench_verdict.rs| 104 +++++     new, test-only
STATUS.md, DESIGN.md, two ADRs, a plan, scripts/bench.sh
```

`git show 4396d6d -- crates/codec/src/` is **empty**. The encoder is byte-identical across
the jump. What the commit did was `include!("verdict.rs")` into the bench binary — about 150
lines of code that the timed loop never enters — and that moved the figure by **+11.4%**.

## The four measurements, and why the fourth is the one that settles it

All on the §9 desktop, `check-machine.sh` `pass 12 fail 0 unknown 1`, medians of 5 runs.

**1. The baseline commit still reproduces.** Built and run at `bf798ea`, the commit that
recorded 239.1: **240.0 ns**. So neither the machine nor the toolchain drifted; that had to
be excluded first, because it is the explanation that would have made everything else moot.

**2. The bisect lands on a bench-only commit.**

| Commit | What it is | median |
|---|---|---|
| `bf798ea` | recorded the 239.1 baseline | 240.0 |
| `f15c82d` | priced the pre-session stage | 234.4 |
| `54eebe9` | fixed a *different* benchmark | 240.6 |
| **`4396d6d`** | **a baseline is a band — bench harness only** | **268.0** |
| `1873725` | (a docs commit, later) | 267.5 |
| `576f924` | ADR-0044, the leading hypothesis | 279.4 |
| `HEAD` | today | 280.4 |

ADR-0044 is real but small — 267.5 → 279.4, +4.5% — and it arrives on top of a case that was
**already 12% over its ceiling** before it. The hypothesis that had been written down would
have explained a quarter of the gap and been believed for the whole of it.

**3. Forcing uniform function alignment collapses the difference.** With
`-C llvm-args=-align-all-functions=6`, `54eebe9` reads 241.6 and `4396d6d` reads 232.6 — the
27 ns gap is gone and slightly reversed. `HEAD` goes from **278.9 to 233.0**, back onto a
baseline recorded four days and one harness rewrite earlier.

**4. And a knob is not a cause, so here is the same result without the knob.** A knob that
moves a number says something is being waited on and nothing about what — this repository has
already published a wrong cause on exactly that arithmetic. So the fourth measurement adds
**inert code** to the bench binary instead: N functions the encoder never calls, referenced
once through `black_box` so they survive to link.

| Inert functions added | medians of 3, ns |
|---|---|
| 0 | 278.8 |
| 3 | **245.7** |
| 9 | 281.6 |
| 27 | 264.4 |

**236.5 to 292.4 ns across the individual readings, and not one line of the code under test
changed between them.** The same sweep under forced alignment reads 229.6 · 238.9 · 235.8 ·
230.9 — a spread of 4.0% against 14.6%.

## The general shape

**A tight band on a small pure-userspace case can be narrower than the layout noise of its
own binary, and the recording procedure cannot see it.** The procedure here is a good one —
median over ≥ 20 whole runs, margin set to the smallest ladder step ≥ max/median. It measures
**run-to-run variance within one build**. Every one of those 20 runs executes the same bytes
at the same addresses, so the spread it reports is real and complete about scheduling, cache
and frequency, and **says nothing at all** about the variance that appears the next time
anybody adds a function anywhere in that binary.

So the margin was fitted to the wrong distribution. Not carelessly: to the only distribution
the procedure can observe.

The failure mode this produces is specific and expensive. The gate goes red, correctly by its
own rule, on a commit that changed the *harness*. The person reading it looks for the
regression in the code under test, finds a plausible commit — there is nearly always a
plausible commit — and writes it down. Here the plausible commit was real, was in the right
file, and accounted for **4.5 of the 17 percentage points**.

**What makes it survivable is a cheap, decisive discriminator: rebuild the code under test at
the commit that recorded the baseline and run it today.** If it reproduces the baseline, the
code under test is exonerated and the search moves to everything else in the binary — which
is where nobody looks, because nobody thinks of the harness as an input to the measurement.
That one build cost four minutes here and refuted two days of written hypothesis.

## What it does not mean

It does not mean the figure is wrong, and it does not mean the encoder is fine — it means the
**figure is not evidence about the encoder at this resolution**. A regression larger than the
layout spread would still show. One smaller than it cannot, in either direction, and a *real*
12% optimisation to `Template::encode` would be indistinguishable from somebody deleting a
function elsewhere in the bench.

It also does not single out this case as broken. Under the same alignment flag,
`library, parse only` moves 138.5 → 160.5 — **+16% in the other direction** — so at least one
more case in the suite carries the same sensitivity and has never had a baseline recorded on
this CPU at all.

## Related

- [a-benchmark-can-delete-its-own-work](a-benchmark-can-delete-its-own-work.md) — the
  optimiser removing the work, which a ceiling passes forever. That one is why the band has a
  floor; this one is about how wide the band has to be to mean anything.
- [a-scratch-fixture-inherits-the-machine](a-scratch-fixture-inherits-the-machine.md) — a gate
  whose result came from its own scaffolding rather than from the system under test.
- [measured-costs.md](measured-costs.md) — the §9 figures this affects.
