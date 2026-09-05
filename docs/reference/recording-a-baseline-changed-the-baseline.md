# Recording a baseline changed the baseline

`[measured 2026-09-05]` on the `DESIGN.md` §9 desktop.

A benchmark case was measured over **twenty clean whole-suite runs** and read **8.2 ns**,
tightly: eighteen of the twenty between 8.1 and 8.3, one 7.7, one 9.5. The number was written
into `benches/baselines.tsv` along with sixteen others. The very next run of
`scripts/bench.sh --strict` went **red on that case**: 6.4 ns, under the floor of a band that had
been computed from the twenty runs an hour earlier.

Re-run on the binary that now exists, it reads **6.3, eight times, never anything else**. Its two
siblings in the same file and the same binary did not move: `191 bytes, walking` stayed 8.9 and
`87 bytes, walking` 5.3–5.4.

## The cause is the file itself

`benches/baselines.tsv` is `include!`d into `harness.rs` so that a missing baseline is a build
failure rather than a silently unchecked run. That is a good property and it is not the problem.

The problem is what it implies: **the table is part of the binary**, so appending seventeen lines
to it produced a different binary — different size, different layout — and a small case moved
**23%**. The act of recording the measurement invalidated the measurement, for that case.

Alignment was pinned throughout. `RUSTFLAGS="-C llvm-args=-align-all-functions=6"` was in force
and `scripts/check-bench-alignment.sh` read it back off all sixteen bench binaries. **ADR-0049's
flag did not prevent this**, which is the second thing worth knowing: pinning function alignment
removes one layout term, not layout sensitivity.

## It is a fixed point, not a chase

The obvious fear is a loop: correct the value, the file changes, the value changes, correct
again. Measured rather than feared — the recorded number was changed from 8.2 to 6.3 and the case
rebuilt and re-run three times. It reads **6.3**. Editing three bytes of a value does not move
code the way adding seventeen rows does, so the correction converges immediately.

## What was recorded, and why each field disagrees with its neighbours

```
journal put, 191 bytes, one slot    6.3    1.35    8    2026-09-05
```

* **6.3, not 8.2.** 6.3 is the binary that exists, which is the only binary anything will ever be
  compared against. 8.2 belonged to a binary that stopped existing the moment it was written
  down.
* **n = 8, where every neighbouring line says 20.** Eight runs of one target, not a campaign.
  Writing 20 would have been a lie of exactly the kind the column exists to prevent.
* **margin 1.35, where the ladder gives 1.20.** The ladder fits `max/median` *within one binary*,
  here 1.159. The swing this line has to survive is the *cross-binary* one, measured at 1.30.
  Same reasoning, and the same hole, as `encode ExecutionReport (template)` carrying 1.15 against
  a measured 1.037 —
  [a-benchmark-that-measures-where-the-compiler-put-it.md](a-benchmark-that-measures-where-the-compiler-put-it.md).

## Why it was caught

Only because the strict gate ran **after** the baselines were written and was allowed to fail.
Had the campaign ended at "twenty runs agree, write them down", 8.2 would have been in the
repository, every subsequent run would have read 6.3–6.4, and the case would have sat permanently
`UNDER BASELINE` — a state this project deliberately reports rather than fails, so it would have
been printed and ignored for as long as anybody could stand it.

## The general shape

`[to testing-skills]`

**A test's own recorded expectations can be an input to the thing it measures.** Where the
expected values live inside the artifact under test — compiled in, bundled, baked into an image,
templated into a config the binary reads at startup — writing them down changes it, and the
number you recorded is a number from a system that no longer exists.

This is not the same as a flaky benchmark, and treating it as one leads nowhere: the case was
*extremely* stable, twenty runs inside 2.5%, and it is stable at the new value too. Stability
across repetitions says nothing about stability across rebuilds, and a campaign that repeats the
same binary can only ever measure the first kind.

Three things follow, and none of them costs much:

1. **Run the gate after recording, on the artifact the recording produced, and require it to
   pass.** Deriving the expectation and asserting it are two different acts and must happen in
   that order, against two different builds. A suite that records and then declares success
   without re-running has verified nothing.
2. **Expect the small cases to move.** The three cases here shared a file, a binary and a
   campaign. The two around 5–9 ns with more code under them held; the smallest, whose whole body
   is one call and one address, did not. The finer the measurement, the more of it is layout.
3. **When a recorded value disagrees with its neighbours, say so in the record.** The `n = 8` and
   the `1.35` above are more useful to the next reader than a tidy row would have been, precisely
   because they do not match. A file where every row looks the same cannot tell you which row you
   should not trust.
