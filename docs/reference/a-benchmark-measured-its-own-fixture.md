# A benchmark measured its own fixture, three times, before it measured anything

> `[measured 2026-09-02]` — found while adding the event stream,
> [plans/2026-09-02-why-a-connection-ended.md](../plans/2026-09-02-why-a-connection-ended.md)
> step 3. **`[to testing-skills]`**
>
> The sibling of
> [the-guard-measured-a-window-that-excluded-the-thing](the-guard-measured-a-window-that-excluded-the-thing.md),
> and its exact opposite: there the counting window was **too narrow** and left the
> allocation outside it, so a broken build read zero. Here the window was **too wide** and
> swallowed the setup, so a correct build read thirty thousand. **Same edge, both directions,
> and neither is visible in the number alone.**

## The claim

Non-negotiable 1: no heap allocation on the hot path, proven by a counting allocator. A new
event stream writes into a fixed ring from the engine thread, so its case must read **0**.

## What it read

```
events-busy 30000      # 2 000 iterations
events-busy  6000
events-busy  2000
events-busy     0
```

Three wrong numbers, and **not one of them came from the code under test.**

| Reading | What was actually allocating |
|---|---|
| 30 000 | `Loopback::pair()` called **inside** the counting closure — three allocations each |
| 6 000 | still building a pair inside, now through `mem::replace(engine_side, Loopback::pair().1)` |
| 2 000 | one per iteration: each *fresh* pair's `VecDeque` growing on its **first** `send` |
| 0 | pairs built **and warmed** outside the window |

The last one is the interesting one. The fixture had been moved out, the pairs were
pre-built, and the number was still exactly one per iteration — because a `VecDeque`
allocates lazily, so "constructed outside the window" and "warmed outside the window" are
different claims and only the second is the one that matters.

## The diagnostic that was itself a false green

Halfway through, the window was split to find where the allocations were: an `add_only` loop
first, then the full loop. `add_only` read 0 and `events-busy` read 0, and it looked solved.

It was not. The first loop had consumed every `Option` out of the fixture vector, so the
second loop's `let Some(e) = … else { continue }` skipped **every iteration**. The second
number was zero because the second loop did nothing. **A guard that cannot fail reports
success.**

## The fix, and the part that generalises

1. Build the fixture outside the window, and **exercise it once** so lazy allocation happens
   there rather than on the first counted iteration.
2. Give the engine capacity for every connection up front, so its own storage never grows.
3. **Assert, inside the window, that the path under test actually ran** — here, that the
   stream recorded more than zero events across the counted loop.

Step 3 is the one that converts the zero from *"nothing allocated"* into *"nothing allocated
while the thing happened"*, and it is the only one of the three that survives somebody later
refactoring the fixture.

## The lesson

**A counting benchmark reports the window, not the code.** Every allocation inside the
braces is attributed to the thing being measured, and every one outside is invisible — so
the boundary is a claim, and it needs the same scrutiny as the assertion.

A zero from such a benchmark is worth nothing on its own. It is worth something only when
paired with a positive control **inside the same window**, proving the measured path was
live. Without that, "0 allocations" and "0 executions" are the same output.

## What it cost here, separately

`git checkout <one file>` was used twice this session to undo a scratch edit, and both times
it destroyed uncommitted work in the same file — the second time the entire benchmark case
this write-up is about. **Commit before you reverse anything**, or the reversal loop and the
undo share a target.
