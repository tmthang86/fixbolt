# A reversal can fail by hanging, and a green-or-red table cannot say so

> `[measured 2026-09-02]` — found while building an ordered shutdown,
> [plans/2026-09-02-an-ordered-shutdown.md](../plans/2026-09-02-an-ordered-shutdown.md).
> **`[to testing-skills]`**

## The practice this refines

A guard is proven by reversal: break the thing it guards, watch the test go red, restore it,
watch the test go green. Reversals are written into a plan **before** the work, as a table of
*"break this → that test must fail."*

That table has an unstated assumption: **that a broken guard produces a failing test.** For a
whole class of guards it does not.

## What happened

The guard: an ordered shutdown gives up after a deadline, because a counterparty that has
already died never answers.

The planned reversal: remove the deadline and wait for the answer for ever.

The expected result: a test called *"a silent counterparty does not hold the shutdown open"*
goes red.

**What actually happened:** that test did go red — and a *different* test, one that drives the
engine's own run loop, never returned at all. The suite ran until it was killed at 600 seconds.

## Why this is worth writing down

Three things follow, and none is obvious from the practice as usually stated.

**1. The reversal table needs a third outcome.** *Red*, *green*, and **does not terminate**. A
plan that only writes "must be red" describes a result the reversal cannot produce, and whoever
runs it later will assume they broke the harness rather than that they proved the guard.

**2. A hang is a *stronger* result than a red test, and a worse one to run.** It says the
property is load-bearing for liveness, not merely for correctness — you cannot even find out
whether the other assertions hold. But it costs a wall-clock timeout every time, and it will
sit in CI as a job that "sometimes takes twenty minutes" if it is ever committed by accident.

**3. Run it with a kill switch, from the start.** Not after it hangs once. The reversal is
*expected* to be pathological; treating it like an ordinary test run is how a session loses ten
minutes to a process nobody is watching.

## The rule

When a guard is about **giving up** — a deadline, a retry limit, a maximum queue depth, a
bound on a loop — write its reversal down as:

> break it → **the suite must hang**, and be killed. A pass here is the failure.

and run it under an explicit timeout. Then say in the delivery log that it was killed and at
what, because *"killed at 600 s"* is the observation, and *"it did not pass"* is not the same
claim.

## The tell

If the guard's job is to stop something happening for ever, its reversal is not a failing test.
It is the thing itself, happening for ever.
