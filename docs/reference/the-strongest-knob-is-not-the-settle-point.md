# The strongest knob is not the settle point

> `[measured 2026-09-02]` — found while building the offline journal reader,
> [plans/2026-09-02-what-the-journal-can-answer.md](../plans/2026-09-02-what-the-journal-can-answer.md).
> **`[to testing-skills]`**

## The shape

A test needs to be sure the system has finished before it looks. There is usually a **correct
settle point** — the mechanism's own guarantee that the work is done — and there is usually a
**strong knob** nearby that feels safer.

Reaching for the knob instead of the settle point produces a test that is correct, slow, and
about the wrong thing. Slow enough, and it stops being run at all, which is the failure that
matters.

## What happened

A durable store had two modes: one that hands each write to a background thread, and one that
`fsync`s before returning. A test needed 5 000 records on disk before reading them back.

`fsync` was chosen, without much thought, because it sounded like the safe choice.

```
test result: FAILED. 1 passed; 1 failed; finished in 39.94s
```

**Forty seconds for one test.** The whole rest of the suite ran in under four.

The actual settle point was three lines away, in the type's own documentation: the store
**joins its writer thread on drop**, and the test already dropped it before reading. The drop
was the guarantee. `fsync` added nothing the test needed and 5 000 disk syncs it did not.

```
test result: FAILED. 1 passed; 1 failed; finished in 0.04s
```

Same test. Same red. Same reason. **A thousand times faster.**

## Why the slow version is worse than merely slow

A forty-second test is a test somebody will eventually mark ignored, move to a nightly job, or
quietly delete — and the reasoning will be *"it is slow and it never fails."* That is how
coverage is lost: not by deciding a check is worthless, but by finding it expensive at a
moment when it happens to be green.

The cost is also paid every time the suite runs during development, which is when the check is
most useful.

## The rule

**Name the settle point before choosing the knob.** Ask: *what, exactly, guarantees the work
is done when my test looks?* Then use that, and only that.

- If the mechanism has an explicit settle point — a join, a flush that returns, a handle whose
  `Drop` waits, a completion callback — that is the answer, and it is usually both faster and
  more precise than the strong knob.
- If you cannot name one, that is a finding about the system, not a licence to reach for the
  biggest hammer and hope. A test that settles by sleeping, or by maximum durability, is a test
  whose passing you cannot explain.
- The strong knob still deserves **its own** test, where the durability is the subject rather
  than the scaffolding. One such test costing forty seconds is a different trade from every
  test costing it.

## The tell

If a test's runtime is dominated by a setting you chose for safety rather than by the thing
under test, you picked scaffolding, not a settle point.
