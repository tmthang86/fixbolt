# ADR-0092 — The rotation driver reads a row by its shape, records the harness's verdict per row, and a panicking `finish` is a verdict, not a lost round

- **Status**: Proposed — 2026-09-22
- **Revised 2026-09-22, after the senior review of the plan's branch** — in place, as
  `CLAUDE.md` §5 allows for a `Proposed` ADR: decision 1's fixture sentence said "two temporary
  lines" where three are needed for an in-band row; corrected and the in-band case named. No
  decision changes.
- **Approved by**: nobody yet. Written by the architect for the plan
  [the-detector-and-the-campaign-preconditions](../plans/2026-09-22-the-detector-and-the-campaign-preconditions.md),
  step 0; the owner approved the *scope* of that plan on 2026-09-22, not this text.
- **Date**: 2026-09-22
- **Deciders**: Tran Manh Thang. Written by the architect (Fable) from facts the manager
  reproduced the same day on a cloud container (*Context*, items 1–4); the architect re-read
  `crates/codec/benches/harness.rs` and `scripts/ab-rotation.sh` at `0629111` and did not
  re-run the manager's `awk`.
- **Related**:
  [ADR-0090](ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  decisions 2–4 (the driver this ADR amends; decision 4 is why the baselines do **not** move
  here);
  [ADR-0087](ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md)
  decision 5 (a *count of runs* is not a latency number — the precedent for recording a
  state per run rather than averaging it away);
  [ADR-0031](ADR-0031-a-baseline-is-a-band.md) and
  [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md)
  (the band the harness's verdict comes from);
  [ADR-0067](ADR-0067-the-baselines-are-read-at-run-time-not-compiled-in.md) (why a baseline line can be
  appended without rebuilding a bench binary — the fixture in decision 1 relies on it).
  `STATUS.md` open item 97; `CLAUDE.md` §2 non-negotiable 10, §10.
- **Answers**: why `scripts/ab-rotation.sh --summary` printed rows that were not
  measurements, why a benchmark that panicked twenty times in twenty rounds was recorded as
  twenty complete rounds, and what the driver must write down so that an arm over its own
  recorded baseline cannot hide inside an arm-against-arm table.

## Context

The `file:line` references are `crates/codec/benches/harness.rs` and `scripts/ab-rotation.sh`
at `main` `0629111`.

1. **The harness's report lines are shaped like measurement rows.** `Suite::bench`
   (`harness.rs:305-360`) prints one row per case:
   `{name:<34} {best:>8.1} ns/op   baseline {b:.1} x{m:.2} = [{floor:.1}, {ceiling:.1}]{mark}`
   or, with no baseline for this CPU, `{name:<34} {best:>8.1} ns/op   NO BASELINE for '{cpu}'`
   (`:329-333`, `:361`). It **also** pushes, for every over-band case, the string
   `"{name}: {best:.1} ns/op exceeds {ceiling:.1} ns (baseline …)"` (`:344-348`) and for every
   under-band case `"{name}: {best:.1} ns/op is below …"` (`:353-358`). `finish()` prints the
   under list as one line, `cases under their baseline: N  a | b` (`:398-402`), and then
   `assert!`s with the over list joined by newlines (`:406-411`). Every one of those strings
   contains the literal ` ns/op`. `[measured 2026-09-22, manager, cloud container]`
2. **The driver's extractor cannot tell them apart.** `run_suite()` (`ab-rotation.sh:460-469`)
   selects every line matching `/ ns\/op/`, splits `$1` (the text before ` ns/op`) on spaces,
   calls the last token the figure and the rest the case name. Fed the real output shapes it
   produces **5 rows from 2 measurements**: two phantom cases whose name ends in `:` and repeat
   a real figure, and one whose name is the under-report's counter welded to a case name.
   `[measured 2026-09-22, manager, cloud container]` — the `awk` was run on a synthetic file
   holding the exact strings above. Because a phantom's name differs from the real one, the
   real cases' medians were not corrupted — every boot D median reproduced — but the summary
   gained rows that were not measurements, and boot D's first write-up dismissed the harness's
   real report as "a parser artefact" because of them (`STATUS.md` 2026-09-22 *Do not*).
3. **The driver never reads the bench binary's exit status.** `set -uo pipefail` at `:105`
   (no `-e`), `out=$("$bin" 2>&1)` at `:465`, no `$?` anywhere. The harness **panics in
   `finish()`** on an over-band case — exit 101 — and did so on **every** qualifying round of
   both arms carrying `main` `76e53cb` (`validate` 20 of 20 and 21 of 21, `density` 21 of 21;
   `STATUS.md` item 97). The driver wrote `round N complete` each time. The detector fired
   sixty-two times and the evidence says "complete" sixty-two times.
4. **`ab_summary()` has no column for it** (`:139-195`: arm, case, median, min/med, max/med, n,
   diff%). Two arms can agree with each other while both sit over their recorded lines;
   `STATUS.md` calls this "blind by construction". `scripts/check-ab-rotation.sh` tests
   `ab_summary` with fixtures — and **is not run by CI**: it is absent from
   `.github/workflows/ci.yml` (`grep check-ab-rotation` finds nothing;
   `[measured 2026-09-22, architect]`), while its siblings `check-machine-verdicts.sh`,
   `check-w2w-baseline-summary.sh` and `check-w2w-compare.sh` are in the `script-logic` job.
5. **The harness measures everything before it fails.** Its module docs (`harness.rs:56-63`)
   and `[measured 2026-08-30]`: every case is timed and printed, and only then does `finish`
   assert. So a run that exits 101 from the over-assert has printed **every** figure, and the
   figures are exactly as good as those of a run that exited 0 — the exit is the *verdict*,
   not a failure to measure.
6. **`bench.sh` does not have this problem** and is not changed. It uses `ns/op` only for
   liveness (`grep -q`), reads the two counter lines by their own prefixes, and reads the exit
   code (`bench.sh:151-200`). The driver is the only consumer that extracts figures from rows.
7. **Old binaries are the normal case for this driver.** Every arm of a rotation is a
   manifest-pinned binary built from a worktree **before** the boot (ADR-0090 decision 2), and
   arms `w0`, `wa`, `e1`…`e7` were built from commits weeks old. A fix that changes what the
   harness prints protects only binaries built after the fix.

### What the search found

`[searched 2026-09-22]` — on how other harnesses keep report text out of the data path:

- **Google Benchmark's `compare.py`** reads JSON, never the console table: *"Where
  `<benchmark_baseline>` and `<benchmark_contender>` either specify a benchmark executable
  file, or a JSON output file"*, and when handed an executable it runs it with
  `--benchmark_out` and reads that file
  (<https://github.com/google/benchmark/blob/main/docs/tools.md>, fetched). Its verdict is a
  Mann-Whitney U test per benchmark, not a band.
- **Criterion.rs** compares against a *named saved baseline* (`--save-baseline`, `--baseline`,
  `--load-baseline`), stored on disk by the harness itself; the comparison never parses
  stdout
  (<https://raw.githubusercontent.com/bheisler/criterion.rs/master/book/src/user_guide/command_line_options.md>,
  fetched).
- **What was not found**: any published driver that scrapes a human-readable table *and*
  has the harness's own diagnostics on the same stream. The shape this project has is the
  shape those two projects designed away with a structured channel. The structured channel is
  the right long-term answer and is **not** taken here, for fact 7: the rotation's arms are
  old binaries, and a JSON line they do not print protects nothing this boot needs.

## Decision

### 1. The extractor anchors on the row's shape, and `harness.rs` does not change

`run_suite`'s inline `awk` becomes a pure function `ab_extract <arm> <round>` (stdin → TSV),
sourced under `AB_ROTATION_SOURCE_ONLY=1` like `ab_summary`. A measurement row is exactly:

```text
^<name><spaces><figure> ns/op   baseline <b> x<m> = [<floor>, <ceiling>]<mark>
^<name><spaces><figure> ns/op   NO BASELINE for '<cpu>'
```

where `<mark>` is empty, `  OVER BASELINE` or `  UNDER BASELINE` — the three spaces after
` ns/op` and the word that follows them (`baseline` or `NO`) are the anchor; `<name>` is the
text before the figure with trailing padding trimmed; `<mark>` is the verdict column. Every line without that tail is
not a row: `… ns/op exceeds …`, `… ns/op is below …`, the joined `cases under their baseline:`
line, and the panic body all fail the anchor. The harness's `{name:<34}` padding collapses to
one space for a long name, so the separator is ` +`, not a fixed width.

**`harness.rs` is not touched.** Not because `benches/` is sacred — it is not a public API —
but because of fact 7: the driver runs binaries built before any harness change, and the
parser is the only place a fix protects every arm. One rule, one place (`CLAUDE.md` §1).
The row shape thereby becomes a contract `ab-rotation.sh` depends on; it is pinned by a
fixture **captured verbatim from a real bench binary** (all four row states, the under line and
the panic body in one capture, produced by appending **three** temporary lines to
`benches/baselines.tsv` — one forcing `OVER`, one forcing `UNDER`, and one putting a third
case **in band** (`validate TestRequest, w2w bytes`, its figure read from a first run, margin
1.35), the fourth case left with no line so it reads `NO BASELINE`; ADR-0067 makes that a
run-time change — and reverting them byte-identical), not typed from memory. *(Revised
2026-09-22: this sentence first said "two temporary lines", which cannot produce an in-band
row on a four-case bench — the plan's *Sửa 1* records that the capture at `854fbbb` was made
that way and pinned three of the four states; `85262e4` re-captured with three.)* If the harness's format ever moves, the fixture does
not fail; decision 2's *zero rows* rule does, on the first real run.

### 2. The exit status is read, the harness's verdict is a state of its own, and a run that measured nothing is `FAILED`

`run_suite` records `code=$?`, counts the rows it extracted, and reads the harness's own
`<k> of <m> case(s) over the machine baseline` line from the panic. Each suite run gets a line
in `timeline.txt`:

```text
round N arm X suite S exit E rows R over O under U nobase M  ok|OVER|FAILED
```

- **`ok`**: exit 0 and R ≥ 1.
- **`OVER`**: exit ≠ 0, the harness's verdict line present with `m == R` (every case was
  printed before the assert — fact 5), O ≥ 1. **The round stays `complete`.** These figures
  are measurements; discarding the round would have thrown away all of boot D and the
  finding with it. The state is its own word, never `ok`, so that a grep for `OVER` on a
  timeline answers item 97's question for any boot.
- **`FAILED`**: anything else — a non-zero exit without the verdict line, or with `m ≠ R`, or
  **R = 0 whatever the exit** (a binary that printed no row measured nothing, exactly
  `bench.sh`'s liveness rule; this is also the self-check for decision 1 — a harness format
  drift shows here as `rows 0`). A `FAILED` suite disqualifies the round for every arm, the
  same `round N incomplete` trailer as a busy machine (ADR-0090 decision 3: `n` stays equal),
  because the arm now lacks a sample its siblings have. The per-arm `busy … ok|DISQUALIFIED`
  line and `ab_complete_rounds` are unchanged, so every existing timeline still parses.

### 3. `runs.txt` carries the verdict, `--summary` shows it, and old evidence is re-readable

Each `runs.txt` row gains a fifth column, `in|over|under|none`, read off the row's own mark.
`ab_summary` prints one more column, `over`, as `k/n` — rows over their recorded baseline
among the complete rounds — and a footer line, `over baseline: <P> (arm, case) pairs` or
`over baseline: none`, so the finding cannot be missed and can be grepped. A four-column
`runs.txt` (every one written before this ADR) prints `?` in that column, never crashes.

A new mode, `--reextract <evidence-dir>`, rebuilds `<dir>/runs.reextracted.txt` from
`<dir>/raw/<round>-<arm>-<suite>.txt` with `ab_extract`, **never overwriting `runs.txt`** —
raw stdout was kept whole for every run (`ab-rotation.sh` header, *EVIDENCE*), so boot D's
`d4m/` can be read honestly without a reboot. It is a file-in, file-out mode: no cargo, no
binary, no clock.

### 4. The baselines do not move, and the slowdown is not fixed here

ADR-0090 decision 4 reserves re-recording `benches/baselines.tsv` for the commit after item
93's last segment has a verdict. Items 93, 95 and 97 are three views of one slowdown; this
ADR fixes the **detector's record**, not the slowdown, and moving the line now would write the
slowdown into the detector. `bench.sh --strict` stays red on the §9 desk until then, on
purpose.

## Alternatives considered

- **Change `harness.rs` to drop ` ns/op` from its report strings** (alone, or with decision
  1). Rejected alone by fact 7 — old arms keep printing the old strings — and rejected as an
  addition because a parser that is correct on its own gains nothing from a second fix in a
  second place; a `crates/` step for a driver's convenience is a step nobody needs.
- **A structured (JSON) line from the harness, and the driver reads only that.** The answer
  the two reference projects chose. Deferred, not rejected: it protects nothing already built,
  and this boot's evidence is all pre-built binaries. It becomes worth doing the day a harness
  change is wanted for another reason; the fixture from decision 1 is then the migration test.
- **Treat a non-zero exit as `DISQUALIFIED`, like a busy machine.** Rejected by fact 5: the
  figures are complete and correct, and the state would have erased boot D's largest finding
  by construction — twenty rounds, zero complete.
- **Treat a non-zero exit as `ok` and just add the summary column.** Rejected: a driver that
  cannot tell a panic from a clean exit cannot tell a crash from a verdict either — the
  `FAILED` state and the `rows 0` rule are the whole point of reading the status.
- **Re-record the baselines so the harness stops panicking.** Rejected, decision 4.

## Consequences

**Good**

- `--summary` prints only measurements, and says in one column and one footer whether any
  arm is over its own recorded line — the thing boot D had to be told by a human reading a
  panic message.
- A panic, a crash and a silent binary are three different words in `timeline.txt`, and the
  first keeps the round while the other two drop it. The `n`-equality rule of ADR-0090 is
  unchanged.
- Boot D's evidence can be re-read (`--reextract`) without a boot; the phantom rows disappear
  from the record without editing it.
- `scripts/check-ab-rotation.sh` finally runs in CI, with a fixture that is the harness's real
  output.

**Bad — and accepted**

- **The row shape is now a contract with no test on the harness side.** A future harness
  change breaks the driver, and the first thing that notices is a rotation printing `rows 0`
  and dropping every round — on the desk, during a boot. The fixture pins what was captured,
  not what the harness will print. The mitigation is that `rows 0` cannot be silent; the cost
  is one wasted round at the start of a boot, not a wrong number.
- **`OVER` keeps a round that a stricter reading would drop.** Someone reading a timeline
  must know that `OVER` is a verdict and not a failure; the word is chosen to be different
  from `ok`, and the footer is chosen to be impossible to miss, and that is all.
- **`runs.txt` changes shape** (five columns). Every consumer must accept four; the plan
  names them (`ab_summary` only). Old and new files are distinguishable by column count and by
  nothing else.
- **The slowdown stays**, and `bench.sh --strict` stays red on the desk until item 93 closes.
  This ADR makes that red *recorded*; it does not make it green.
- The capture procedure for the fixture edits `benches/baselines.tsv` on a developer's tree
  and must revert it byte-identical (`git diff --exit-code`); a forgotten revert is a bad
  baseline line, which the harness refuses in every mode (`harness.rs:read_baselines`) —
  loud, not silent.

## Sources

- `crates/codec/benches/harness.rs` at `0629111`: `:30-65` (verdict table), `:305-360`
  (`Suite::bench`), `:379-412` (`finish`).
- `scripts/ab-rotation.sh` at `0629111`: header `:40-120`, `ab_summary` `:134-195`,
  `run_suite` `:452-470`, `:105` (`set -uo pipefail`).
- `scripts/bench.sh` at `0629111`: `:151-200`.
- `.github/workflows/ci.yml` at `0629111`: `script-logic` job, `:107-118`.
- `STATUS.md` 2026-09-22 *Start here* and open item 97.
- Google Benchmark, `docs/tools.md`; Criterion.rs, *Command-Line Options* — both fetched
  2026-09-22, cited above.
