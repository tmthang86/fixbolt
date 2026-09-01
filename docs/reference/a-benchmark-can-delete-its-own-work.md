# A benchmark can delete its own work, and the obvious test for it passes anyway

> `[measured 2026-09-01]` — found while measuring something else entirely,
> [plans/2026-09-01-release-profile.md](../plans/2026-09-01-release-profile.md).
> **`[to testing-skills]`**
>
> **This page corrects a conclusion this repository published on 2026-08-31**, in
> [measured-costs.md](measured-costs.md)'s *"The instrument was 80% of its own smallest
> reading"*. That section suspected exactly the right thing, ran a test, and drew the wrong
> conclusion — which is what makes it worth writing up rather than quietly fixing.

## The number

`DESIGN.md` §6 published `inline deliver + reply` at **1.3 ns** and §8 carried it as
**0.0013 µs**. The benchmark:

```rust
let mut out = [0u8; 1024];
b.bench("inline deliver + reply", || {
    let r = inline.deliver(0, black_box(msg), 2, stamp, &mut out);
    black_box(r);
});
```

The application behind `deliver` is `Bounce`, which does `out[..n].copy_from_slice(&msg[..n])`
for a **163-byte** message and returns `Some(0..n)`.

**163 bytes in 1.3 ns is 125 GB/s from a single core.** That was on the page for a day and
nobody did the division.

`out` is written by every iteration and read by nobody. Only the returned `Range` escapes
through `black_box`, so the stores are dead and the optimiser removes the copy. Adding one
line — `black_box(&out)` — at the **same harness** and the **same default profile** takes the
case from **1.3 ns to 8.5**. 163 bytes in 8.5 ns is 19 GB/s, which is a number a core can
actually produce.

## How it was found, which was not by looking for it

An unrelated experiment: four release-profile arms, measuring what LTO and
`codegen-units = 1` are worth. Sixteen cases, five arms. Fifteen cases moved by single-digit
percentages. One did this:

| Arm | `inline deliver + reply` |
|---|---|
| default profile | 1.3 ns |
| `lto = "thin"` | 1.3 |
| `lto = "fat"` | 1.3 |
| **`codegen-units = 1`** | **7.4–8.5** |
| **`lto = "fat"` + `codegen-units = 1`** | **7.4–8.6** |

Ten runs per arm, and every run of the first three read **exactly 1.3**.

The first reading of that table is *"`codegen-units = 1` regresses dispatch 6×"* — a plausible,
alarming, and completely wrong conclusion, and the one that would have been published if the
number had been believed. What made it not-believed was the arithmetic above: 1.3 ns cannot
copy 163 bytes, so the question was never *"why did C get slower"* but *"why were the others
so fast"*.

## The test that had already been run, and why it passed

`[measured 2026-08-31]`, when this case first dropped from 6.3 ns to 1.3 after a harness
change, the drop **was** investigated. The write-up says:

> *"A benchmark that gets 5× faster when a function signature changes has, on the face of it,
> stopped measuring. The obvious suspicion is that the optimiser deleted the work. That was
> tested rather than argued about: the closure was made to call `deliver` **twice** per
> iteration, changing nothing else."*

| | ns/op |
|---|---|
| one `deliver` per iteration | 1.3 · 1.3 · 1.3 |
| two `deliver` per iteration | 2.6 · 2.6 · 2.6 |

> *"An exact factor of two, three times running. The work is real and the 1.3 ns is real."*

**The suspicion was right, the experiment was run, and the conclusion was wrong.** Here is the
same test on both versions:

| Variant | 1 call/iter | 2 calls/iter | ratio |
|---|---|---|---|
| elided (no `black_box(&out)`) | 1.3 | 2.6 | **2.00** |
| honest (`black_box(&out)`) | 8.5 | 15.9 | **1.87** |

**Doubling the calls is linear in both.** It has to be: doubling the calls doubles whatever
fraction survives the optimiser. A linearity test measures whether the thing you are timing
scales with the number of times you do it — it says **nothing** about whether the thing you
are timing is the whole operation.

That is the trap, and it is a good one, because the linearity test *feels* like the rigorous
move. It is the response of somebody doing the right thing.

## What the old number actually was

The 2026-08-31 write-up concluded that the previous harness had been adding ~5 ns of its own
overhead — *"the instrument was about 80% of its own smallest reading"*.

`[measured 2026-09-01]` that is refuted by the honest range. The old harness read
**6.3–8.3 ns**. The current harness, with the copy no longer deleted, reads **7.7–8.8 ns over
62 process draws**. Those are the same distribution.

**The old harness was not adding overhead. It was preventing the elision** — its closure sat
behind an indirect call, which kept the stores to `out` alive. Replacing it removed the
indirect call, the optimiser could then see the whole loop, and it deleted the copy. The
5× "speedup" was the work leaving.

And the supporting evidence read the same way round: the write-up noted that the old harness
made this case *trimodal* (6.3 / 7.4 / 8.2) while the new one read 1.3 flat, and concluded the
trimodality "belonged to the measuring apparatus". `[measured 2026-09-01]` with the copy
restored, the case is **bimodal again — 7.7 and 8.5 in roughly equal numbers over 62 draws**.
The modes belonged to the work, not to the instrument. A benchmark measuring nothing has
nothing to be multi-modal about, and *that flatness was the tell* — read at the time as a sign
of a better instrument.

## What is now published

| | before | after |
|---|---|---|
| `DESIGN.md` §6 gate | inline **1.3 ns** vs ring 267.4 | inline **8.5 ns** vs ring 265.9 |
| `DESIGN.md` §8 dispatch row | **0.0013 µs** | **0.0085 µs** |
| inline-vs-ring ratio | 206× | **31×** |
| `benches/baselines.tsv` | 1.3, n=24 | **8.5**, margin 1.10, n=22 |

D4's decision — inline by default, ring for an application that must not run on the engine
thread — is unaffected: 31× is still an enormous gap, and it was never close.

## The audit that followed, and its result

Every other timed closure in the workspace was checked the same way: does anything it writes
escape? Two more candidates had an output nobody read — `encode 1 group, 2 entries` (writes
`out`) and `parse NewOrderSingle` ×3 (writes `idx`).

**Neither moved.** 107.4 → 104.9, 124.5 → 123.1, 117.1 → 113.7, 58.3 → 56.6 — every one inside
its own run-to-run spread. In both, the returned value depends on the writes (an encoded
length, a parse result validated from the index), so the optimiser cannot drop them.

**One case in sixteen.** The `black_box` calls were kept in all three anyway: a benchmark that
is safe because today's optimiser happens not to see through it is safe by luck.

## The generalisation

`[to testing-skills]` — **a linearity test cannot detect that a benchmark is measuring a
constant fraction of its operation**, and it is the test people reach for.

Three defences, in the order they cost:

1. **Divide.** Before anything else, take the number and the work and check that the rate is
   physically possible. 163 bytes in 1.3 ns is 125 GB/s per core; no doubling test was needed
   to know that is wrong, and no doubling test could have said so.
2. **Make the output escape, always.** Not because you suspect elision — because a benchmark
   whose result nobody reads is one optimiser release away from measuring nothing, and the
   day it happens the number gets *better*, which is the direction nobody investigates.
3. **Distrust a benchmark that got faster for a reason you cannot name.** This one improved
   5× after a *mechanical* change to a function signature. That was noticed, was investigated,
   and the investigation stopped at the first test that produced a clean answer.

And the structural one, which is about gates rather than benchmarks: **a ceiling cannot catch
a benchmark that got faster.** `benches/baselines.tsv` compares against `baseline × margin`,
so a case that starts measuring nothing reads far *under* its limit and passes, forever. The
elision here would never have gone red. It was found by a person doing arithmetic in an
experiment about something else.
