# Some claims are only reversible at compile time

> `[measured 2026-09-02]` — found while putting a journal on disk behind the serving loop,
> [plans/2026-09-02-recovery-reaches-the-disk.md](../plans/2026-09-02-recovery-reaches-the-disk.md).
> **`[to testing-skills]`**

## The practice this refines

A guard is proven by reversal, and a house rule that has already paid for itself twice says a
reversal must be **red at an assertion, not at the compiler** — because a test that does not
compile says only that the code has not been written yet, never what the system does today.

That rule is right for a *specification* test. Applied to a *reversal* it is wrong, and this
entry says where the line falls.

## What happened

The claim: *"the serving loop no longer requires the journal type to have a `Default`."*

That constraint had been the whole defect. A file-backed journal has no honest `Default` — it
needs a path — so a single `J: Default` bound, and nothing else, was what stopped the convenient
entry point from ever using a journal on disk.

The reversal was written into the plan as: put the bound back, and **the build must fail**.

```
error[E0599]: no associated function or constant named `default` found for type parameter `J`
    --> crates/engine/src/lib.rs:1264:23
error: could not compile (lib) due to 1 previous error
```

## Why no runnable test could have done it

Ask what a test would compare. Both versions — the one with the bound and the one without —
**behave identically for every type that has a `Default`**, which is every type a test can
conveniently reach for. The versions differ only for types that cannot be written down under the
bound, and a program containing one of those does not compile, so there is nothing to run.

The general shape: **a claim about what a type system permits is falsifiable only by the type
system.** A claim of the form *"X is now possible"* where X is a compile-time property has
exactly one honest reversal — remove the change, and watch the compiler refuse.

The same reasoning covers a whole family: a lifetime that no longer needs to outlive another, a
trait that is now object-safe, a `const fn` that is now callable in a constant, a bound moved
from a caller to an implementation. All of them are green-or-nothing at run time.

### The rule as it now reads

- A **specification** test is red at an assertion. A reversal that fails to compile there means
  the test was written against an API that does not exist.
- A **reversal** is red wherever the claim lives. For a behavioural claim that is an assertion.
  For a claim about what the type system permits it is a compiler error, and demanding an
  assertion instead would mean the claim goes unproven.

Both plans in this pair have one of each, which is what made the distinction visible at all.

## The near-miss beside it: a discriminator read in two places

The same day, a second reversal in the same plan reversed only **half** of what it named.

A record in the file is identified by a sentinel value in its header. Two decoders read it: an
eager scan when the file is opened, and a lazy iterator for reading records back. The reversal —
*"read that record as an ordinary one"* — was applied to the lazy decoder alone:

```
test result: FAILED. 5 passed; 1 failed
```

One test red. It looks like a discriminating reversal, and it was reported as one. Flipping the
eager decoder as well:

```
test result: FAILED. 3 passed; 3 failed
```

**Two of the three tests were not covering the branch that had been broken**, and a reader of the
first result would have concluded they were.

The lesson is cheap to state and easy to miss: **when the thing being reversed is a constant or a
condition that appears in more than one place, the reversal is not done until every place is
flipped.** A partial reversal produces a plausible red and understates what the suite is really
holding. `grep` for the constant is the whole technique.

## What was done about it

The compile-error reversal is recorded in the plan's delivery log **as a compile error**, with
the diagnostic quoted rather than described, and with a sentence saying why an assertion was not
the right shape for that particular claim.
