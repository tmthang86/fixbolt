# ADR-0067 — The baselines are read at run time, and the binary no longer contains them

**Status:** Accepted (owner, 2026-09-13 — by approving
[the-second-linux-desk](../plans/2026-09-04-the-second-linux-desk.md), *Cửa sổ A*, step A1;
recorded 2026-09-14) · **Date:** 2026-09-14 ·
**Plan:** docs/plans/2026-09-04-the-second-linux-desk.md, A1, and its delivery log of
2026-09-14 (the number is 0067, not the 0062 the plan's text says; only the number changed)
**Closes:** the second half of `STATUS.md` open item 52 — *whether the table should stop being
compiled in*. The first half — whether the ladder rule can mean anything for cases this
small — stays open.
**Supersedes:** the sentence in `crates/codec/benches/harness.rs` that documents `BASELINES`
as *"compiled in rather than read at runtime: a missing file is then a build failure and not a
silently unchecked run."* Nothing in [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md)
or [ADR-0031](ADR-0031-a-baseline-is-a-band.md) changes; ADR-0016 decision 2 said
*baselines are data, not code*, and this is what it takes to make that true of the binary.

## Context

`crates/codec/benches/harness.rs:81` reads

```rust
const BASELINES: &str = include_str!("../../../benches/baselines.tsv");
```

so every bench binary in the workspace carries `benches/baselines.tsv` in its `.rodata`.
`harness.rs` is not one binary's module: it is `#[path]`-included by thirteen bench targets
across `codec`, `session`, `engine` and `library`. Every append to the table rebuilds all of
them.

`[measured 2026-09-05]`, on the `DESIGN.md` §9 desktop, the property that bought became a
defect ([recording-a-baseline-changed-the-baseline](../reference/recording-a-baseline-changed-the-baseline.md),
`STATUS.md` item 52): `journal put, 191 bytes, one slot` read **8.2 ns** over twenty clean
runs, eighteen of them inside 8.1–8.3. Seventeen lines were appended to the table, and the next
`scripts/bench.sh --strict` went **red on that case at 6.4** — under the floor of a band
computed an hour earlier. On the binary that now existed it read **6.3, eight times, never
anything else**, while its two siblings in the same binary did not move. Correcting the value
converged at once (edit three bytes, rebuild, 6.3), so it was a fixed point rather than a
chase; the table carries `6.3 / 1.35 / n = 8` beside neighbours that all say `n = 20`, and
[ADR-0052](ADR-0052-two-candidates-are-retired-with-numbers-and-a-baseline-disagrees-with-its-neighbours.md)
records that disagreement as deliberate.

**Alignment was pinned throughout.** [ADR-0049](ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md)'s
`-C llvm-args=-align-all-functions=6` was in force and read back off all sixteen bench
binaries. That ADR had already shown, on `encode ExecutionReport (template)`, that a commit
touching only the harness — `include!`-ing ~150 lines the timed loop never enters — moved a
figure **+11.4%** with the encoder byte-identical. Item 52 is the third appearance of that hole
and the first that is self-referential: **the act of recording a measurement changed the
binary the measurement came from.**

What the shift *was* is not established. Item 52 records it as observed with the table compiled
in, and nothing more. The literature names layout as the candidate:

- Mytkowicz, Diwan, Hauswirth, Sweeney, *Producing wrong data without doing anything obviously
  wrong!*, ASPLOS 2009 — changing the size of an unused environment variable changes the
  program's performance *"frequently by about 33% and once by almost 300%"*, because it moves
  the stack and *"affects the alignment of local variables in various hardware structures"*;
  link order alone yields conclusions differing *"on average 2% for Core 2 and 8% for
  Pentium 4"*, and for one benchmark *"we may think we have a 7% slowdown when in fact we have
  a 8% speedup"*. Their mechanism: *"link order affects the alignment of code, causing
  conflicts within various hardware buffers (e.g. caches) and hardware heuristics (e.g. branch
  prediction)."*
- Curtsinger and Berger, *Stabilizer: Statistically Sound Performance Evaluation*, ASPLOS 2013
  — *"A single binary constitutes just one sample from the space of program layouts, regardless
  of the number of runs."* Their remedy is to re-randomise code, stack and heap layout at run
  time so the layout term becomes Gaussian noise; under it, LLVM's `-O3` over `-O2` on SPEC
  CPU2006 is *"indistinguishable from random noise"*.

Twenty runs of one binary are twenty samples of one layout. A 23% move on a 6–8 ns case
whose whole body is one call and one address is inside what both papers describe. That is a
candidate, labelled as such; this ADR does not claim it as the cause.

**What sibling harnesses do with a stored baseline** (searched 2026-09-14):

| Harness | Where the baseline lives | If it is missing |
|---|---|---|
| Criterion.rs | on disk under `target/criterion`, written by `--save-baseline <name>`, read by `--baseline <name>` at run time; `critcmp` reads the same files | compares against the previous run instead |
| gungraun (was iai-callgrind) | on disk, `--save-baseline` / `--baseline` / `--load-baseline` | *"creates a new baseline with the current results instead of comparing"* |
| divan | no baseline feature found in the crate docs or README | — |
| CodSpeed | server side, the run recorded on merge to `main` | no comparison until one is recorded |
| `cargo bench` (libtest) | none | — |

None of them compiles the expectation into the artefact under test, and none treats a missing
one as fatal — gungraun goes furthest the other way and silently creates it. This project's
property is stricter than any of theirs and is kept: a missing baseline is loud. What changes
is only *where* the table sits relative to the binary.

## Decision

1. **`harness.rs` reads `benches/baselines.tsv` at run time.** The path is fixed at compile
   time — `concat!(env!("CARGO_MANIFEST_DIR"), "/../../benches/baselines.tsv")` — and the
   contents are not. `CARGO_MANIFEST_DIR` is that of the crate whose bench includes
   `harness.rs` by `#[path]`; every consumer lives at `crates/<name>/`, so `/../../benches/`
   resolves to the workspace table from each of them. A consumer placed anywhere else fails at
   run time under decision 2, not silently.

2. **A missing, unreadable or malformed file is a non-zero exit, in every mode** — with and
   without `bench.sh --strict`, in CI as on a §9 desk. This keeps the one property
   `include_str!` bought: forgetting the table cannot be silent. It was a build failure; it is
   now a run failure that ends the process before any figure is compared. *Malformed* means the
   file does not parse as the TSV its own header describes; it does not mean "no line for this
   CPU" — that is still `NO BASELINE`, ADR-0016 decision 5, and stays reported rather than
   fatal outside `--strict`, because the CI pool's CPUs deliberately have none.

3. **New property: appending a line to `benches/baselines.tsv` changes not one byte of any
   bench binary.** The record and the artefact are decoupled: a campaign's results can be
   written down and the very binary that produced them re-run against them. The check is
   mechanical — hash every bench binary, append a line, rebuild, hash again, equal — and A1's
   delivery log, not this ADR, holds the run of it.

4. **The rustdoc on `BASELINES` says this**, and the *compiled in* sentence is gone. A comment
   asserting run-time behaviour names what proves it (`CLAUDE.md` §4): the test that reverses
   decision 2 is named there.

5. **At boot B, `bench.sh --strict` runs three times on the new binary.** A case outside its
   band is re-measured at `n = 20` and its true `n` recorded; a case inside its band is left
   alone, because ADR-0031's band exists to absorb exactly this drift. The full 28-line
   re-record is **not** done unless a case is red — the plan's Q6.

## Alternatives considered

**Keep `include_str!` and live with the fixed point.** Item 52 showed one correction
converges. But `harness.rs` is shared by thirteen bench targets in four crates, so every
append rebuilds every bench binary, and any small case in any of them may be the next one to
move. The self-reference is structural, not a one-off.

**A `build.rs` writing the table into `OUT_DIR`.** Named and rejected in item 52: the
generated file is a compiler input too, so the binary still changes with the table. It moves
the problem, at the price of a build script the `codec` crate does not otherwise need.

**A path from an environment variable or the working directory.** `cargo bench` does not
promise a working directory to a `harness = false` binary, and an environment variable can
point a run at a stale copy and pass. Both make "the baseline this run compared against"
depend on the shell, which `scripts/check-machine.sh` cannot see. `CARGO_MANIFEST_DIR` is
fixed at build time and names one file.

**Layout randomisation, Stabilizer-style.** The literature's remedy for the whole class. It is
a compiler-and-runtime system, not a change to a bench harness, and it would replace every
baseline in the table with a distribution that has not been measured. Out of scope; the hole
ADR-0049 describes stays open and is not what this ADR claims to close.

## Consequences

**Good**

- Recording a baseline no longer changes the thing it is a baseline for. The seventeen-line
  append that produced item 52 would leave every bench binary byte-identical.
- ADR-0016 decision 2 — *baselines are data, not code* — is now true of the artefact, not
  only of the file format. Editing the table does not trigger a rebuild of thirteen targets.
- Loudness is kept: a missing or corrupt table ends the run non-zero in every mode, where the
  sibling harnesses would compare against the previous run or create the baseline.
- Boot B compares one stable binary three times against one file, and the plan can say which
  cases moved for reasons other than the table.

**Bad**

- **A bench binary now depends on filesystem state at run time.** Copied to another machine,
  run from a checkout at a different path, or run after the tree is moved, it looks for the
  build-time path and exits non-zero. Loud by design, but it means the bench binaries are not
  relocatable, and `cargo bench` on the §9 desk must run from the checkout that built them.
- **The table the binary was compared against is no longer recoverable from the binary.**
  With `include_str!`, the binary was the record; now only git is. A published figure must
  name the commit of `benches/baselines.tsv` it was judged against — non-negotiable 10 already
  demands the committed benchmark, and this is one more thing that word covers.
- **The file can differ from the one at build time.** That is the point — edit, re-run, no
  rebuild — and it is also a way to run a stale binary against a fresh table without noticing.
  Nothing here detects it; the delivery log's habit of naming the commit for both is what does.
- CI and a fresh clone must have the file. It is committed, so they do; a `.gitignore` or a
  sparse checkout that drops `benches/` would turn every bench job red at once, which is the
  intended failure.
- **This does not close the layout hole.** ADR-0049's finding stands — a harness-only code
  change moved a figure 11.4% with alignment pinned. What is removed is the instance where
  the record itself was the change. The first half of item 52, whether a per-case ladder
  margin can mean anything for a 6 ns case, stays open and is not answered here.
- One more line in `harness.rs` that can fail — a file read — and one more test to keep
  green, in a file thirteen targets include.

## Sources

- Mytkowicz, Diwan, Hauswirth, Sweeney. *Producing wrong data without doing anything obviously
  wrong!* ASPLOS 2009. <https://dl.acm.org/doi/10.1145/1508244.1508275>; PDF read at
  <https://users.cs.northwestern.edu/~robby/courses/322-2013-spring/mytkowicz-wrong-data.pdf>
- Curtsinger, Berger. *Stabilizer: Statistically Sound Performance Evaluation.* ASPLOS 2013.
  <https://dl.acm.org/doi/10.1145/2451116.2451141>; PDF read at
  <https://people.cs.umass.edu/~emery/pubs/stabilizer-asplos13.pdf>
- Criterion.rs, *Command-Line Options* — `--save-baseline`, `--baseline`, `--load-baseline`:
  <https://bheisler.github.io/criterion.rs/book/user_guide/command_line_options.html>;
  `critcmp`, which reads the saved baselines from `target/criterion`:
  <https://github.com/BurntSushi/critcmp> (the README fetch timed out on 2026-09-14; the
  `target/criterion` location is from the search summary, not read first-hand)
- gungraun, *CLI and environment: basics* — baseline flags and the create-if-missing rule:
  <https://github.com/gungraun/gungraun/blob/main/docs/src/cli_and_env/basics.md>
- divan crate docs and README, searched for "baseline": <https://docs.rs/divan/latest/divan/>,
  <https://github.com/nvzqz/divan> — nothing found
- CodSpeed, *Writing Benchmarks with bencher (libtest)* — the baseline is the run recorded on
  merge: <https://codspeed.io/docs/benchmarks/rust/bencher>
