# Recording a baseline changed the baseline

`[measured 2026-09-05]` on the `DESIGN.md` §9 desktop.

> **`[2026-09-14]` Half of this is closed, half is not.** The compiled-in half is closed by step
> A1 of `docs/plans/2026-09-04-the-second-linux-desk.md`, commit `bdd673f`,
> [ADR-0067](../decisions/ADR-0067-the-baselines-are-read-at-run-time-not-compiled-in.md):
> `harness.rs` now reads `benches/baselines.tsv` at run time, so appending a line changes no
> bench binary (that commit's gate: the `parse` bench binary's sha256 read the same before and
> after a line was appended, and cargo did not rebuild). A malformed file is no longer a build
> failure; `read_baselines` exits non-zero naming the line instead, and
> `crates/codec/tests/bench_baselines.rs` tests that, including a `nan` margin.
> **The margin-ladder half stays open** (`STATUS.md` item 52): whether the ladder rule can mean
> anything for a case this small, whose cross-binary layout swing the within-binary `max/median`
> never sees. The account below is the 2026-09-05 one and describes the file as it was then.

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

## n = 20 on the run-time table

`[measured 2026-09-21, boot D step D3]` **The fix held, and the odd row is gone.**

[ADR-0067](../decisions/ADR-0067-the-baselines-are-read-at-run-time-not-compiled-in.md) moved the
table out of `.rodata` and made `harness.rs` read `benches/baselines.tsv` at run time, so
recording a baseline no longer rebuilds the binary the baseline came from. That removes the
mechanism this page is about — but removing a mechanism is a claim until something measures the
case it used to break.

Twenty runs of `journal put, 191 bytes, one slot` on the §9 desk (`pass 16 fail 0 unknown 0`),
from a binary built **before** the boot and never rebuilt during it, run 1 discarded, 8 s apart,
a quiet row read before each:

```text
7.4 ×17, 7.5, 8.0, 8.1      median 7.4    max/median 1.0946
```

Compare the three numbers this one case has produced on this one desk:

| value | n | what the binary was |
|---|---|---|
| 8.2 | 20 | before seventeen lines were appended to a **compiled-in** table |
| 6.3 | 8 | after they were, and after the value was corrected — a fixed point of the self-reference |
| **7.4** | **20** | a **run-time** table (ADR-0067); the binary does not contain the baseline at all |

So the line no longer needs the `1.35` that was buying cross-binary swing, and it no longer needs
the honest, awkward `n = 8`. It reads `7.4 / 1.10 / n = 20`, the same shape as its neighbours, and
`STATUS.md` item 52's remaining half — *whether the ladder rule can mean anything for a case this
small* — closes: it can, once the case is measured against a binary that does not move when you
write the answer down.

**The lesson that survives is point 3 above, inverted.** The row was allowed to look wrong — a
`1.35` and an `n = 8` among neighbours that all said `1.10 / 20` — for sixteen days and four
merged pull requests, and that is exactly why the boot that could finally re-measure it knew
which single row to go after.

## The second time, at run time

`[measured 2026-09-23, boot F]` **ADR-0067 took the table out of the binary; it did not take the
file's length out of the process.** Between two `--strict` runs of boot E the same pinned
`journal` binary moved `one slot` from 7.4 to 12.4 ns (`STATUS.md` item 99), and the commit
between the runs had lengthened `benches/baselines.tsv` by 671 bytes. Boot F tested that one
variable on the old binary before any `perf record`: 12.4 on the 24 897-byte file, 7.4 on the
24 226-byte one, 12.4 again after restoring it, and a sweep of the file padded by `k = 0 … 1024`
bytes in steps of 16 that read **6.3 … 19.0 ns** — a band of 11.4 … 19.0 between 24 866 and
25 026 bytes, 6.3 … 8.1 either side — while `walking` stayed 8.7 … 8.9 throughout. Tables:
[measured-costs](measured-costs.md) *Boot F, item 99*.

The mechanism is the one this page opened with, moved one layer: `harness.rs` read the file with
`std::fs::read_to_string`, which sizes its buffer to the file, and that buffer is the one
long-lived allocation made before the timed closure allocates its `Store` and its 191-byte
message. The file's length set where the closure's data landed. **Fix** (`6b2833b`, ADR-0096
decision 2): read into a fixed `String::with_capacity(1 << 20)`, so the block has one size
whatever the file says; test `a_loaded_file_reads_into_a_fixed_one_mib_buffer` in
`crates/codec/tests/bench_verdict.rs` asserts the capacity. The sweep is the desk regression,
and on the new binary it read **7.4 … 8.3** — the step is gone, but max/min is **1.12**, over
the 1.10 the plan asked for; two single points (8.2, 8.3) sit over the case's 8.1 ceiling.

**And the fix moved a neighbour.** The same commit shifted `fixbolt-sbe`'s `walk nested group +
varData` from 153.6 … 160.3 to 174.2 … 176.1 ns (same boot, interleaved A/B), with no change in
`crates/sbe` and not through glibc's mmap threshold (`MALLOC_MMAP_THRESHOLD_=131072` read the
same). Its line was re-recorded with that cause (`5576694`), and the question of how a bench's
working set can be made independent of where the harness's own allocations end is `STATUS.md`
item 101.

What this adds to *The general shape*: **an input read at startup is part of the artifact for a
case small enough to see its own addresses** — removing the expectation from the binary moved
the self-reference from link time to run time, and each fix to a layout term is a new layout
for every other case in the process. Point 1 above caught both: the gate was run after the
change and allowed to go red.

## The third time, in the harness's own code

`[measured 2026-09-23, desk on the desktop grub line — a diagnostic; the count is a count]`
**The neighbour did not move through the heap.** `walk nested group + varData` reads a message
held in a stack array and static schema tables, and allocates nothing on its path, so the fixed
1 MiB block of the second time had nothing to act on in that case. What `6b2833b` also did was
move the comparator out of `Suite::bench` into `Suite::figure`, which the compiler kept out of
line. Every `Suite::bench::<F>` is inlined into one function, `harness::suite::<closure>`, which
therefore holds **every case's timed loop**. In the `sbe` binary that function went from 16 696
to 12 276 bytes, and every loop inside it moved. ADR-0049 pins a function's start, not a loop's
offset inside it.

Proven by the layout-free counter, not by timing ([ADR-0102](../decisions/ADR-0102-a-line-that-moves-while-its-instruction-count-does-not-is-a-layout-move-and-the-count-is-read-off-the-desk.md)):
the two binaries retire **the same instructions to 0.032 %**, and the new one retires fewer. The
walk case is still about +10 % slower in its own ns/op. The process as a whole takes ~+8.7 % more
`cycles:u` (1.384 → 1.504 G at the low ends; `cycles:u` is per process, only ns/op is per case).
Moving the stack with ASLR off and an environment sweep moves neither arm ([measured-costs](measured-costs.md) *Desk-free, 2026-09-23*).
The same session closed the second time's open residue. With ASLR off, the bench's own sequence of
allocation sizes and addresses is identical at all eight padding `k` on this harness (one md5
over the list). The first k = 640 recording differed only because the bench panicked on an
`OVER BASELINE` line; it was re-run with `FIXBOLT_BENCH_COUNT_ONLY=1`, which cannot panic. The
sequence moves by the
padding on `6b2833b^`, so the 1.12 was single-run dispersion, not a staircase.

What this adds to *The general shape*: **the harness is code in every case's binary, and one
function holds all of a bench file's timed loops**. So an edit to the harness, or a case added to
a bench file, is a layout change for every case in that file. The band cannot tell that from a
regression; `scripts/bench-instructions.sh` can, and a line that moved while its count did not is
re-recorded with the cause *layout* (ADR-0102 decision 2). The structural fix, a separate
`#[inline(never)]` timed function per case, is named in ADR-0102 decision 4 and waits for the next
full re-record, because building it is itself one more new layout for every line.
