# The same job, on the same commit, failed and passed at once

`[measured 2026-09-09]` Observed on PR #56 while closing `STATUS.md` item 59.

## What happened

A force-push re-triggered CI. One check went red:

```
thread 'an_operator_stops_the_front_door_and_serve_comes_back' panicked at
crates/library/tests/end_to_end.rs:334:
the premise: a session is up, so the shutdown has something to say goodbye to
```

The test connects to an acceptor, sends a `Logon`, and asserts a `Logon` comes back. It got
nothing — not a timeout, which has its own message, but an empty read: the acceptor closed the
socket or returned end-of-file.

The change under test had nothing to do with sessions or shutdown. So, in order:

1. **Ruled the change out by construction.** The test stamps its `Logon` at a width the change
   provably does not touch — the code path for it is identical before and after, by inspection of
   the one `match` that changed.
2. **Ruled it out by running it.** 8 consecutive local runs, all green.
3. **Then found the thing that settles it.** This repository triggers two workflow runs per push.
   The **same job**, on the **same commit**, in the **same minute**, ran twice:

   ```
   fail  .../runs/34362977166/job/102504603733
   pass  .../runs/34362986989/job/102504692571
   ```

Two runs of one job on one commit disagreeing is not evidence about the change. It is a
measurement of the *test*.

## The shape

The first two steps are the ones a careful person does, and **neither of them can distinguish a
flake from a real defect that happens not to reproduce locally.** "It passes on my machine eight
times" is compatible with a genuine race that needs a slower, more contended host — which is
precisely what CI is.

What settled it was a **second execution of the same job on the same input**. Nothing about the
code, nothing about the diff, nothing about reasoning: two samples of one experiment.

And that second sample existed **by accident**. Nobody arranged it; the repository happens to fire
two workflow runs per push, which is usually just noise and duplicated minutes. This time the
duplication was the entire evidence.

## What to take from it

- **A test that fails once is not yet a defect and not yet a flake.** Both hypotheses fit one red
  run. The cheapest thing that separates them is re-running the identical job, and it should be
  the *first* move, before reading the diff for suspects.
- **Local repetition is weak evidence about a race.** The failing environment is slower and more
  contended than the developer machine by construction, so passing locally is close to what a
  genuine load-dependent bug predicts.
- **Duplicate CI runs are an asset, not only waste.** Where two runs of one job exist, every push
  carries a free nondeterminism check. Where they do not, a single red run cannot be classified at
  all without spending a re-run.
- **Say which it was.** A red result that is quietly re-run until it goes green, with nothing
  written down, is how a real intermittent defect survives for months: each individual person who
  met it concluded "flake" and moved on, and nobody ever counted.

This one is now counted: **one red, one green, same commit** — in a test helper, not in the code
it exercises. It has not been fixed, and this note is the record that it is open rather than
explained away.

## `[measured 2026-09-09, an hour later]` It happened again, in a different test, and the obvious fix is refused

A second pull request, a different test, the same signature: **one job, one commit, two runs,
opposite results.** Different test file, different assertion, same shape — a helper that waits on a
**wall-clock deadline** for something to be observable.

So it is not one flaky test. It is a *family*: every test that waits a fixed number of seconds for
an asynchronous system to reach a state can be starved on a loaded runner, and each of them will
be met once, individually, by someone who concludes "flake" and moves on.

**The obvious fix — raise the deadline — is refused, and the measurement is why.** Locally the
second test completes in **0.00 s**, including pinned to a single core. The deadline it blew
through on CI is **5 s**. A gate that finishes five hundred times inside its budget and then
consumes the entire budget and sees nothing did not run slightly slow: **a step did not happen at
all.** Raising the number would move the score without touching the cause, which is exactly the
failure mode of accepting a cause because a knob moved with it.

Three hypotheses were checked and two are dead:

- *The observer's drain replaces the buffer rather than appending, so events seen in different
  polls cannot accumulate.* **Refuted by reading it**: it appends.
- *The change under test caused it.* **Refuted**: the feature involved is off by default and the
  test does not touch it; and the duplicate run passed on the identical tree.
- *A readiness race between the harness connecting and the server actually serving.* **Not
  refuted, and not confirmed** — it fits the shape but nothing here has demonstrated it.

The cause is **not found**, and that is written down rather than rounded to a diagnosis that sounds
plausible.

## What to take from it, part two

- **A timeout that is blown by 500× is not a timeout problem.** Compare the failing budget against
  the normal duration before touching either. If the ratio is large, the deadline is not the
  constraint and changing it only hides the next occurrence.
- **"Flake" is a classification, not an explanation.** Proving nondeterminism (two runs, one
  commit) is cheap and worth doing immediately. Finding the cause is a separate job, and skipping
  it should be recorded as skipped.
- **One instance is an anecdote; two with the same signature is a category.** The second occurrence
  is what turns "that test is flaky" into "wall-clock waits in test helpers are a defect class in
  this repository", which is a much more actionable statement.

`[to testing-skills]`
