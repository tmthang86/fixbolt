# The guard measured a window that excluded the thing it was guarding

> `[measured 2026-09-01]` — found while building the pre-session stage,
> [plans/2026-08-31-pre-session-routing.md](../plans/2026-08-31-pre-session-routing.md)
> step 3. **`[to testing-skills]`**

## The claim

`CLAUDE.md` §2 non-negotiable 1: **no heap allocation on the hot path**, and it is proven
by a counting allocator in `benches/alloc.rs`, never by reading the code. A new
`PendingSet` holds sockets that have not identified themselves. Its claim: *everything is
allocated once, in `new`, to the ceiling the caller named — admitting, turning and taking
allocate nothing.*

## The guard that was written for it

Two cases, and the second was added specifically because the first looked too easy:

```rust
let pending_idle_allocs = count(|| { for _ in 0..100_000 { set.turn(now); } });   // 0
let pending_busy_allocs = count(|| { for _ in 0..10_000  { set.turn(now); } });   // 0, 8 live sockets
```

Both read **0**. Both assert. Both were about to be committed.

## The reversal

The claim is *"allocated once, in `new`"*. So break exactly that:

```rust
-   slots: Vec::with_capacity(limits.pending()),
+   slots: Vec::new(),
```

Now every `admit` past a power of two grows the table and allocates. Re-run:

```
allocations: … pending-idle 0 pending-busy 0
```

**Both cases still read 0, and both still passed.**

## Why

`admit` was called **outside** `count()`. The sockets were put in during setup, and the
counted window contained only `turn`. The guard measured, precisely and truthfully, an
operation that was never the one in question — and the number it printed was the number
that means *everything is fine*.

The busy case was supposed to be the defence against exactly this. It was written down as
*"the empty sweep alone would pass a `PendingSet` that allocated on every `admit`"* — and
then it put the `admit` calls in the setup too, so it inherited the hole it was added to
close. **The reasoning was right and the code did something else**, which is the failure
mode that no amount of reasoning catches.

## The fix

A third case whose window contains the whole per-connection cycle — `admit`, `turn`,
`take` — with the far ends kept alive so the only allocations inside are the set's own:

```
with Vec::with_capacity   …  pending-cycle 0     green
with Vec::new()           …  pending-cycle 7     RED
```

Seven: the reallocations of a `Vec` growing 1, 2, 4, 8, 16, 32, 64.

## The rule

**A guard is not proven by its value. It is proven by the reversal that makes it change.**
This repository already says so — `CLAUDE.md` §7, *break it, see it red, restore it, see
it green*. What this case adds is **which** reversal:

> Reverse the exact sentence the guard claims. Not a nearby one.

The claim was *"allocated once, in `new`"*. A reversal that made `turn` allocate would
have gone red and would have proved nothing about `new`, because `turn` was the only thing
inside the window. The reversal has to attack the clause, and if the guard survives it,
**the window is in the wrong place** — not the claim.

## The generalisation

`[to testing-skills]` — **a measurement window can exclude the operation under test, and
then it reports the passing value for a reason that has nothing to do with the code.**
This is a sibling of *"no events were recorded" and "the recorder was not running" look
identical*, and it is worse in one way: here events *were* recorded, in quantity, from a
real run, of a real operation. The instrument was working perfectly. It was pointed
somewhere else.

It appears wherever a check has a scope: a profiler started after the interesting call, a
transaction counter around the wrong block, a log assertion over the wrong time range, a
mock verified after the code path that would have called it. The number is real; the
window is wrong; and nothing about the output says so.

The defence is mechanical and costs one run: **for every guard, write down the sentence it
proves, then break that sentence and watch.** If the guard does not move, do not adjust
the claim — move the window.

## A fourth instance, twice in one step — `[measured 2026-09-13]`

Step 6b of the `tls` plan (`docs/plans/2026-09-04-tls.md`, commit `da9fe6e`) added a kTLS arm
to `scripts/check-no-kernel-sleep.sh` and `scripts/check-standard-gives-the-core-back.sh`: run
`tools/w2w hft --tls ktls`, and assert the engine is read back as `tls: kernel` rather than
`tls: userspace`, so the two transport arms cannot be mistaken for each other by a script that
only watched syscalls. The claim the reversal needed to prove was **the read-back sentence**
— *"this script refuses a run whose transport identity does not match the arm it asked for."*

The first attempt at that reversal went red, and it was the wrong red. Forcing the engine into
the userspace path under a `--tls ktls` invocation tripped `w2w`'s own **allocation** assertion
— `userspace` allocates, `w2w` asserts `allocs 0` for `ktls`, so the binary panicked on its own
count — *before* either script ever reached the line that reads the transport identity back.
The scripts' own assertions were never exercised; a wholly different guard, one window earlier,
caught the forced fallback first and ended the run before the thing under test ran at all. The
baseline script, `w2w-baseline.sh`, had the same ordering in its first draft for the same
reason: identity was checked **after** the allocation count rather than before it, so a
mismatched arm that also happened to allocate read as an allocation failure, never as an
identity failure.

The fix was the same move this file already names: **reorder the checks so the window that
proves the claim runs first**, then re-run the reversal and read the sentence it actually
produces. Identity now comes before the allocation count in both places, and the second attempt
at the reversal went red on exactly the intended sentence —
`` `--tls ktls` ran tls `'userspace'` when `'kernel'` was required `` — then green once restored.

This is the fourth time this repository has found a red in the wrong place, and the second and
third both happened inside this one step, in the production script and the baseline script
independently, from the same underlying ordering mistake made twice. The generalisation does
not need restating; what this instance adds is that **the same wrong order can be written twice
in one step**, in two different files, because the second one was copied from a shape that
already had the bug.
