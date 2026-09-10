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

This one is now counted: **one red, one green, same commit.** `[written 2026-09-09]` It was
guessed at here as "a load-dependent readiness race in a test helper". **That guess was wrong**,
and the section below is what it actually was — left visible rather than edited away, because the
distance between the guess and the answer is the useful part.

## `[measured 2026-09-09, later]` It happened again, then the cause turned out not to be timing at all

A second pull request, a different crate, the same signature: **one job, one commit, two runs,
opposite results.** So not one flaky test — a *family*: every test that waits a fixed number of
seconds for an asynchronous system to reach a state.

**The obvious fix, raising the deadline, was refused**, and the measurement was why: the second
test completes in **0.00 s** locally, including pinned to a single core. It blew a **5 s** budget
on CI. A gate that finishes five hundred times inside its budget and then consumes all of it and
sees nothing did not run slightly slow — a step did not happen.

And then the cause: **it was not a timing problem, it was a loss problem wearing a timeout's
clothes.**

The engine pushed events into a ring behind a `try_lock` — correctly, because that thread must
never block behind an operator's. A failed `try_lock` **dropped the event permanently** and only
incremented a counter. The test's wait helper polled that same ring every 2 ms, taking its mutex
each time. Occasionally the two coincided, the event ceased to exist, and the test waited out its
whole budget for something that was never coming.

Every hypothesis before that was about *delay*, and delay was never involved. Which is why:

- 32 spinning processes pinned to the same two cores left it at 0.12 s against a 5 s budget;
- the test passed 60 runs out of 60, and the whole suite 3 of 3, at the runner's core count;
- and **the one piece of distinguishing evidence had been in the first CI log all along** —
  `saw: [ one event ]`, not `saw: []`. A test that timed out on a slow engine would have seen
  nothing. Seeing exactly one of two says the other was **destroyed**, not delayed.

Reproducing it meant inverting the instinct: not adding load, but **removing the politeness**.
With the 2 ms sleep deleted so the reader held the lock almost continuously, it went from
unreproducible to **9 runs in 20**, with the exact signature CI had produced.

## What to take from it, part two

- **Read the failure's own output before theorising about the environment.** The evidence that
  settled it was printed by the very first failing run, and went unexamined while three
  environmental hypotheses were built and killed.
- **"Timed out" is a symptom with at least two causes** — it was slow, or it is never coming.
  They demand opposite investigations, and only the second explains a *partial* result.
- **To reproduce a race, exaggerate the participant you control, not the machine.** Adding CPU
  contention made everything slower and the race no likelier. Making the reader greedier made it
  a coin flip.
- **A guess written down as a guess costs nothing to correct.** This page said "a load-dependent
  readiness race in a test helper". Keeping that sentence visible next to the answer is worth
  more than a page that was right the first time.

`[to testing-skills]`
