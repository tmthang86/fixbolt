# A reversal is only a test if the input is one the two versions disagree about

> `[measured 2026-09-02]` — twice in one session, on unrelated code.
> **`[to testing-skills]`**

## The shape

Break the code, expect red. When it stays green the instinct is *"the guard was redundant"*.
Sometimes it is. More often the **input** was chosen from the region where the broken version
and the correct one give the same answer — so the reversal never asked the question.

Two instances, same day, different mechanisms.

## Instance 1 — a protocol with a second legal answer

Asking a counterparty to resend messages `2..3`. The reversal swapped the ends, so this end asked
for `3..2`. Expected red; got `PASS 7/7`.

The counterparty cannot replay a backwards range, so it fell back to a *"skip to N"* placeholder —
legal, and carrying the same *is-a-repeat* flag the test was reading. **The fallback is what a
malformed request gets**, so the worse the request, the likelier the assertion accepts it.

Fixed by asserting *which messages came back* rather than *that something did*. Same reversal,
unchanged: `FAIL 6/7`.

→ [a-resend-answer-has-two-legal-shapes.md](a-resend-answer-has-two-legal-shapes.md)

## Instance 2 — two checks whose order only matters in part of the input space

A reconnect policy asks two questions: *is the venue open?* and *has the backoff elapsed?* The
engine asks them in that order. The reversal swapped them. Expected red; **all 8 green**.

The test asserted at an instant where the backoff had **already elapsed**. There, both orderings
fall through to the schedule and both answer the same thing. The orderings disagree only while
the backoff is still pending:

| venue shut, backoff due in 500 ms | answer |
|---|---|
| schedule first — what the engine does | `At(now + 30 s)` |
| backoff first | `At(now + 500 ms)` — dials into a shut venue, 29 s early |

Move the assertion 1.5 seconds earlier and the same reversal reads `left: At(…801000)`,
`right: At(…830500)`.

## The lesson, stated generally

**Two implementations agree on most inputs; that is what makes one of them a plausible bug.** A
reversal picks one input. If it picks from the agreement region, the reversal is a no-op and
says nothing — and it says nothing *loudly*, because a green reversal reads like evidence that
the guard was unnecessary.

Before writing the assertion, ask: **what input would the broken version answer differently?**
Then assert at that one. It is a different question from *what input exercises this code*, and
the second one is the one that comes to mind.

Three practical forms:

1. **Ordering bugs** (instance 2) discriminate only where the earlier check would have
   short-circuited. Pick an input where the first check fires and the second would not — or the
   reverse.
2. **Fallback paths** (instance 1) discriminate only on inputs the fallback does *not* cover.
   Where a system has a legal degraded answer, assert on something the degraded answer lacks.
3. **Boundaries.** `>=` versus `>` differs at exactly one value. A test at that value is worth
   ten either side of it. `the_wait_ends_at_its_instant_and_not_a_millisecond_early` asserts at
   `n-1`, `n` and `n+1` for this reason.

## And the corollary that costs the most

**A green reversal is a result to investigate, not a box to tick.** Both of these were logged as
"the guard is redundant" for the length of time it took to look — and both were actually "the
test does not test what its name says". The second reading is the expensive one to miss, because
it leaves a test in the suite that will be trusted later.
