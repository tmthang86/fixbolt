# A cost claim needs its own counter, because behaviour tests cannot see cost

> `[measured 2026-09-01 and again 2026-09-02]` — found twice, in the same codebase, in two
> mechanisms built five weeks apart by the same hand. **`[to testing-skills]`**

## The shape

Some claims are about **what** a system does. A test can see those: send this, expect that.

Some claims are about **how much it costs to be able to do it** — *"watching is free while
nobody watches"*, *"the queue is not touched when it is empty"*, *"this allocates once at
startup"*. **No behaviour test can see those**, because the expensive implementation and the
cheap one produce identical output for every input.

So a cost claim is only testable if the system **counts the thing it claims not to do**.

## The first time

A snapshot mechanism claimed: *the engine builds one only when somebody asks; the cost while
nobody is asking is one relaxed load.*

The reversal was to delete the "somebody asked" flag, so the engine built a snapshot every
turn. **Every content assertion stayed green** — the snapshots were correct, they were merely
84 555 of them in 50 ms. A counter of snapshots built was added, and the reversal then failed
on it.

## The second time, in a mechanism explicitly modelled on the first

A command queue in the same module, going the other way. Its first version reached for the
queue's mutex on **every** turn as soon as the observation handle existed. Every test passed:
the same commands were applied, the same outcomes reported.

It was found by a **hand-walk of the project's invariant list**, not by any test — and the
invariant it violated was one the earlier mechanism had already been fixed for.

A count of lock attempts was added. The reversal now reads:

```
left: 1002
 right: 2
```

## Why the second time is the interesting one

The fix for the first case was in the same file, forty lines away, with a comment explaining
exactly why it was there. **The precedent did not transfer.**

That is the part worth generalising: a cost guard protects **one** claim. It is not a lesson
the codebase remembers on your behalf, because nothing about the new mechanism fails when the
guard is missing — including the tests you wrote for it, including the reversals you planned
for it. Every reversal in the plan for the second mechanism was written before this was found,
and **none of them would have caught it**.

## The rule

**Every claim of the form *"X costs nothing when Y is absent"* ships with a counter of X**,
and the counter is asserted in a test that does nothing but be idle.

Concretely:

1. Write the claim as a sentence with a number in it.
2. Ask: *what would an implementation that violates this produce differently?* If the answer
   is "nothing observable", you need a counter.
3. Expose the counter — even where it is API nobody will ever read in production. That is the
   price of the claim being checkable.
4. **Assert it in a test that only idles**, and assert in the same test that the counter is
   not simply stuck at zero.

Step 4's second half matters: a counter wired to nothing also reads zero for a thousand idle
turns.

## What it costs

Public surface that exists only to be asserted on. Both counters here are exactly that, and
they are worth it: without them the two most-repeated performance sentences in the project's
own documentation would be unfalsifiable prose.
