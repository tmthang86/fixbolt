# A reversal that removed the guard's label, not the guard

> `[measured 2026-09-12]` Third of three near-miss reversals on one branch, and the
> only one committed by the person checking somebody else's work rather than by the
> person writing it.
>
> **`[to testing-skills]`**

## What happened

A settings parser had to refuse one configuration line on a build compiled without an
optional feature — *refused cleanly*, never *parsed and ignored*, because the quiet
outcome was a server listening in plaintext on a port an operator believed was
encrypted. The refusal was one conditional statement carrying a compile-time
attribute that restricted it to the featureless build:

```rust
#[cfg(not(feature = "tls"))]
if enabled {
    return Err(/* refused: this build cannot do it */);
}
```

A reviewer set out to prove the guard by reversal. The reversal was: delete the
`#[cfg(not(feature = "tls"))]` line, rebuild, expect the test to go red.

The test stayed **green**, and the reviewer's first reading was that the guard was
broken.

## Why it stayed green

The attribute is not the guard. It is a *label saying when the guard applies*. Deleting
it leaves the refusal compiled **unconditionally** — and the test in question runs in
exactly the build where the refusal was supposed to apply. So the refusal still fired,
from the same line, for the same input, and the assertion still passed.

The reversal changed the program in a way that was invisible to the only test watching.
Read off the result alone it says *"the guard is fine"*. Read off what actually changed
it says nothing at all: **a reversal that cannot change the observed behaviour is not
evidence about the behaviour.**

The reversal that means something deletes the whole refusal:

```
assertion `left == right` failed: a build without the `tls` feature must refuse this at
parse time rather than serve the port in plaintext; instead: Ok(true)
  left: None
 right: Some(NeedsFeature)
test result: FAILED. 38 passed; 1 failed
```

## The general shape

**When a guard is a conditional, there are two things to break, and only one of them is
the guard.** Breaking the condition asks *"is this restricted to the right cases?"*.
Breaking the body asks *"does it do anything at all?"*. They are different questions with
different answers, and the second is the one a reversal is usually written to answer.

A condition-only reversal is worth running **only** where some test exercises the case
the condition excludes. If every test lives on one side of it, removing the condition is
a no-op by construction, and a green result was determined before the run started.

Three ways this hides:

1. **It looks like a smaller, safer edit.** Deleting one line above a block feels more
   surgical than deleting the block — and it is exactly the deletion that proves least.
2. **A green reversal reads as reassurance.** A red reversal invites the question *"red
   at what?"*. A green one invites no question, so nobody asks whether it could ever have
   been red.
3. **The reviewer is not the author.** The author knows the refusal is one statement with
   a label on it. A reviewer reading the diff sees an attribute and a block, and the
   attribute is the part that names the thing being tested.

## What to do instead

- **Before running a reversal, write down what the program will now do differently, and
  which test observes it.** If the answer to the second half is "none", the reversal is a
  no-op and the result is worthless — pick a different one.
- **Delete the behaviour, not its scope.** Scope reversals are a separate exercise, and
  they need a test on the other side of the scope before they mean anything.
- **Predict the failure sentence, not just the colour.** This is the same discipline that
  caught the sibling cases of this family, and it is the only thing that separates *the
  guard works* from *the reversal could not fail*.

## Siblings

- [a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for](a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for.md)
  — the reversal went red, on an assertion that could not have failed for the reason it
  exists.
- [a-matcher-excluded-the-separator-every-real-name-uses](a-matcher-excluded-the-separator-every-real-name-uses.md)
  — the guard itself was inert, and its own reversal passed.

All three were found on the same day, on one branch, and none was found by reading code.
Each was found by writing down the expected failure first and then comparing it with what
actually happened.
