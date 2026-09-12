# A reversal that goes red proves *something* failed, not that your assertion works

> `[measured 2026-09-10]` and `[measured 2026-09-12]` — twice on one branch, two
> different mechanisms, both while proving a TLS handshake driver.
> **`[to testing-skills]`**

## The shape

A reversal is the standard proof that a guard works: break the code, see red, restore, see
green. The ritual is sound. What it quietly does **not** establish is *which* assertion caught
the break — and that is usually the thing you actually wanted to know, because you wrote the
reversal to validate one specific line.

Red is the exit status of the whole test. It is not a receipt for the assertion you had in mind.

Two ways this goes wrong, and they are opposites:

- **The test dies before it gets there.** An earlier assertion, or a setup step, fails first.
  The line you were validating never executed.
- **The test dies somewhere else, because your line cannot tell the two states apart.** It runs,
  it passes in *both* the broken and the healthy version, and a different assertion is doing all
  the work. This is the dangerous one: you conclude the guard is proven and delete the scaffolding
  that was really holding it up.

The companion to [a-reversal-needs-an-input-where-the-answers-differ.md](a-reversal-needs-an-input-where-the-answers-differ.md).
There the **input** was chosen from a region where the two versions agree, and the reversal stayed
green. Here the reversal goes red — and the **output** the assertion reads is one that both
versions produce anyway.

## Instance 1 — three red reversals, and none of them reached the assertion

`[measured 2026-09-10]` A non-blocking TLS handshake driver, behind an acceptor. The test's
meaning lives in one assertion at the end: *the session came up*. Everything before it only
establishes that nothing crashed.

Three reversals were run against it. All three went red:

```
ConnectionReset
UnexpectedEof
WouldBlock
```

All three failed **at the socket**, during setup, before the final assertion ran. So the set was
unanimous, it looked thorough, and it said exactly nothing about the line that separates *it
served* from *it did not crash*. A fourth reversal had to be invented purely to reach that line.

**Three red results, zero coverage of the assertion they were written for.**

## Instance 2 — the assertion could not have caught it, and the reversal still went red

`[measured 2026-09-12]` Same driver. The claim under test: *a slice of work with nothing to do
reports "pending" and returns, rather than waiting.* The predicted reversal: put the peer's first
flight on the wire before the first slice runs, so the slice has real work — then the "pending"
assertion should go red.

It went red. On a different assertion:

```
assertion `left == right` failed: the acceptor wrote something in answer to a socket nobody had written to
  left: 646
 right: 0
```

The predicted assertion **stayed green**, and that was measured rather than reasoned about, by
replacing the other assertion with a print:

```
R2 probe: first = Pending, acceptor answered 647 bytes
test a_handshake_completes_without_the_acceptor_ever_blocking ... ok
```

The reason is simple once seen and invisible before: the driver reports *pending* in **two**
different situations — when there is nothing to do, and when there is plenty to do but the peer
has not answered yet. It is not a discriminator. The assertion that carried the test's meaning
was a second one, added almost as a footnote: *the acceptor cannot have written a reply to a
socket nobody wrote to.*

Had the reversal been read off its exit status, the conclusion would have been "the pending
assertion is proven" — about a line that cannot fail for the reason it is written for.

## Why it is worth a file

Both instances were **predicted wrong by the same person who wrote the test**, in the same week,
on the same driver. In instance 2 the prediction appeared in a written plan, was approved, and was
still wrong. What caught it was not review: it was printing the value instead of asserting on it.

## What to do instead

- **Predict the assertion, not the colour.** Write down *which* assertion you expect to fail, in
  the plan or the commit message, before running the reversal. Then compare. A red on a different
  line is a finding, not a pass.
- **Read the failure message, never the exit status.** The message names the assertion. `FAILED`
  does not. (Same family as
  [reading-the-output-you-grepped-for.md](reading-the-output-you-grepped-for.md): the filter
  became the result.)
- **When a reversal goes red somewhere else, replace your assertion with a print and run it
  again.** If it prints the healthy value under the broken code, the assertion is decoration.
  This costs one run and is the only step here that produces evidence rather than an argument.
- **Ask what else produces the value you are asserting on.** A status that means "not finished
  yet" is produced by *every* unfinished state, including the ones you are trying to exclude.
  Prefer an observable that only the healthy path can produce — in instance 2, *bytes written* was
  that observable, and *pending* never could have been.
- **If a reversal fails during setup, it has not tested anything.** Get it to the assertion, or
  write a different reversal.
