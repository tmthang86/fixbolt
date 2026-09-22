# ADR-0095 — A cumulative drift is read at re-record time, not by a wider band; a segment is named by a rule, not by an eye; and the desk runs `bench.sh --strict` at every measurement boot

- **Status**: Proposed — 2026-09-22. Written for
  [closing-the-open-items-desk-free-then-s9](../plans/2026-09-22-closing-the-open-items-desk-free-then-s9.md);
  becomes *Accepted* at that plan's merge under the owner's standing mandate, and one word from
  the owner reverses it. Nothing here has run except the `perf diff` probe cited in the plan's
  *Những gì đã biết chắc*.
- **Date**: 2026-09-22
- **Deciders**: Tran Manh Thang. Written by the architect (Fable) from `STATUS.md` items 93,
  95 and 97, the boot D section of `docs/reference/measured-costs.md`, and a `perf diff` run
  today over `target/boot-d-evidence/d2-0149b26-1.data` and `d2-792c2e7-1.data`.
- **Related**: [ADR-0031](ADR-0031-a-baseline-is-a-band.md) (the band, `Under` is a report),
  [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md),
  [ADR-0067](ADR-0067-the-baselines-are-read-at-run-time-not-compiled-in.md),
  [ADR-0090](ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  decision 4 (three verdicts, the baselines move once),
  [ADR-0092](ADR-0092-the-rotation-driver-reads-a-row-by-its-shape-and-a-panicking-finish-is-a-verdict-not-a-lost-round.md),
  [ADR-0094](ADR-0094-the-descent-is-priced-by-attribution-inside-one-binary-not-by-a-second-binary.md).

## Context

Item 93 asked the architect one question: *"whether the band should also read a cumulative
drift is a question for the architect once the five have names."* Five steps of 2–3% each, on a
case whose band is 10%, added up to 9.5% (C-91b) and none tripped `bench.sh`. Item 97 then showed
the other half of the story: **eight medians on `main` were over their recorded lines on the §9
desk, and the harness said so twenty times a round** — and no gate saw it, because the CI `bench`
job runs on GitHub's runner (which has no line in `baselines.tsv`) and the desk only runs
`bench.sh --strict` when somebody runs it. The last recorded `--strict` run on the desk before
boot D was 2026-09-15. The detector existed for a week and was not read.

So the question is really two questions, and a third follows from boot D's evidence:

1. Should the band be cumulative — should a line remember where it started, and fire when the
   sum of small, in-band steps crosses something?
2. ADR-0090 decision 4 gives three verdicts for a segment (*fix*, *accept named*, *accept
   unnamed*) but no rule for **when a profile has named a function**. With flat `perf` profiles
   over an inlined `Engine::turn`, a name read by eye from a 0.6% share delta is a guess with a
   symbol beside it.
3. Whatever the band does, it guards nothing on a machine where it is never run.

Facts the decision rests on, all measured today or at boot D:

- `perf diff` on two recordings of two different binaries matches symbols **by name**
  ([perf-diff(1)](https://man7.org/linux/man-pages/man1/perf-diff.1.html)), Rust names come out
  demangled **without** the `::h…` hash, and one pair takes 2 s. The probe pair (`e1-1` ↔ `e2-1`,
  segment (1)) read `scan_fields` −1.77 points, `__memcmp_avx2_movbe` +1.31, `Session::judge`
  +0.61, `FieldType::accepts` +0.51 — shares, not nanoseconds, and no noise floor yet.
- Each endpoint has **two** recordings (`<sha>-1`, `<sha>-2`), so a same-binary pair exists for
  every endpoint and is a noise floor for free.
- Dispersion of the cases in question over n = 20 at boot D: `validate *` on `w1`
  `max/median` 1.008–1.035; engine-turn on `w1` 1.120–1.135 (noisy) but 1.008–1.025 on `w1s`.
- rustc-perf calls a result significant only past `Q3 + 3 × IQR` of **that benchmark's** history
  ([comparison-analysis](https://github.com/rust-lang/rustc-perf/blob/master/docs/comparison-analysis.md));
  MongoDB moved from thresholds to change-point detection over a series
  ([Daly et al. 2020](https://arxiv.org/pdf/2003.00584)). Both say the same thing: the threshold
  is the case's own noise, not a constant.

## Decision

### 1. The band stays per case and per commit; there is no cumulative band in the harness

`benches/verdict.rs` and `harness.rs` do not change. A cumulative band would make the harness
stateful over history — a second file, a second rule, and a second place for the line to be
edited — and it would still fire on nobody, because the only machine with a line never runs it
(context, fact 3). The drift is caught instead by decisions 2 and 4.

### 2. A re-record is a ledger entry, and a line that has drifted past its own band needs an ADR

Every commit that moves a line of `benches/baselines.tsv` records, in its body and in
`docs/reference/measured-costs.md` under the boot that produced the number, **old → new per
line, with the cause named** (the item, the segment verdicts, or the optimisation for an
`Under`). The first number ever recorded for a case on a machine is the line's **origin**, and
measured-costs keeps it beside the current value. **When a re-record would put a line more than
its own margin (×1.10 unless the row says otherwise) from its origin, the commit cites an ADR
that accepts the cost by name** — for the `engine turn *` and `validate *` lines of the
Ryzen 3700X, this ADR's *Consequences* is that citation, with the five segment names of item 93
beside it, and ADR-0090's *Bad* consequence ("baselines will be re-recorded to slower numbers,
~+7–10% and ~+3–6%") is the price already accepted. A re-record without a cause is refused in
review; that is a hand check (`CLAUDE.md` §4 has no machine row for it, and this ADR adds none).

### 3. A segment is *named* by a rule that uses the recording's own noise floor

For a bisect segment with endpoints `a`, `b`, each with recordings `-1` and `-2` (ADR-0090
decision 4: "≥ 2 per endpoint" — this is why):

- Run `perf diff -c delta-abs -s symbol -o 1` for the **four cross pairs** (`a-1↔b-1`,
  `a-2↔b-2`, `a-1↔b-2`, `a-2↔b-1`) and the **two same-binary pairs** (`a-1↔a-2`, `b-1↔b-2`).
- Convert shares to time: `ns = share × (ns/op of that run)`, the `ns/op` read from the run's
  own `.out` file, and `Δns` per symbol per cross pair. The check that the table is not
  nonsense: Σ Δns over all symbols has the sign and the order of magnitude of
  `turn_b − turn_a`.
- The segment is **named** when one symbol has the largest |Δns| in **all four** cross pairs
  with the same sign, and its |Δshare| is at least **twice** its |Δshare| in the larger of the
  two same-binary pairs. Otherwise the segment is **accept, unnamed**, with the IPC of both
  `.stat` files written beside it — never a name read by eye.
- **Fix** is *named* plus a concrete code change someone can state in one sentence (file,
  function, what moves), **and an arm built before the next reboot boundary**. A fix that is
  not built by the boundary is recorded as *accept, named* with the candidate written down; the
  next boot may pick it up. This is ADR-0090's "built before the boot" made a deadline.

Segment (3) already met the stricter bar: an arm, a rule declared before the boot, and 33.1 ns
against a 25 ns line. This rule is for the segments that have only a profile.

For a bisect over a **rotation** (item 95, arms `wa → b1 → b2 → b3 → w1` on one case), a step
is **attributed** to the arm-pair whose median difference is at least **3 × (max/median − 1)**
of that case on both arms of the pair; a step smaller than that on every pair is published as
*not attributable at n = 20* and the item closes as *accept, named per span*.

### 4. The desk runs `bench.sh --strict` at every measurement boot, before and after

`scripts/bench.sh --strict` on the pre-built `main` tree is a **named step of every measurement
boot on the §9 desk**: once after the machine checklist reads clean (a red there is a finding
to open, never a stop), and once more after any re-record in the same boot (which must read
green). The verdict line and every `OVER BASELINE` / `UNDER BASELINE` row go into that boot's
*Settings in force* block in `measured-costs.md`. The tree must be built before the reboot so
the run compiles nothing (ADR-0090 decision 2); a `Compiling` line in the output voids the run.
No script changes: this is a plan-template rule — the boot table of every measurement plan
carries the two `--strict` rows, and a plan without them is not approved.

## Consequences

**Good**

- The harness stays one rule in one file, tested (ADR-0031 decision 4), with no history to
  corrupt.
- A drift is now visible in two places that are read: the re-record commit (old → new, cause)
  and the strict run at each boot. Item 97's failure mode — a red nobody ran — is closed by the
  step, not by a knob.
- "Named" has a definition that a second reader can re-run in two seconds per pair and get the
  same answer; "unnamed" stops being an admission and becomes a verdict with an IPC number.
- The four cross pairs and two same-binary pairs cost nothing more than boot D already paid
  (it recorded two per endpoint).

**Bad — and accepted**

- **The baselines move to slower numbers**, once, in the plan's step S4: `engine turn *` by
  roughly +4…+10% and `validate *` by +3…+16% against their 2026-09-05 origin (boot D's table:
  ×1.108…×1.160 on the eight breached lines). That is ADR-0090's accepted price, now with a
  ledger entry and this ADR as the citation decision 2 demands. The segment (3) fix, if merged
  before the boot, takes ~33 ns back on the turn and the re-record includes it.
- **Decision 2 is a hand check.** A re-record without a cause can be committed; only a reviewer
  reading the diff of `baselines.tsv` beside the commit body catches it. §10 says review of a
  diff catches almost nothing — but a tsv diff is three numbers, and this is the one case where
  the diff *is* the evidence.
- **The naming rule can say "unnamed" for a real cause** when it is spread over several
  callees (a generic that stopped inlining shows as several small deltas). The rule prefers a
  false "unnamed" to a false name; an IPC delta is written either way.
- **Decision 4 adds ~30 minutes to every boot** and one more pre-boot build.
- **The rotation attribution rule (3 × dispersion) is chosen, not derived.** At `max/median`
  1.03 it asks for a 9% step, which the 12.32% of item 95 clears and a 4% step does not; a
  4% step then reads *not attributable*, and that is the honest outcome at this `n`.

## Sources

- [perf-diff(1)](https://man7.org/linux/man-pages/man1/perf-diff.1.html) — symbol-name
  matching across binaries, `-c` defaults; [Kan Liang, "perf diff: Support for different
  binaries"](https://lore.kernel.org/lkml/1416585348-14762-1-git-send-email-kan.liang@intel.com/).
- [rustc-perf, comparison analysis](https://github.com/rust-lang/rustc-perf/blob/master/docs/comparison-analysis.md)
  — significance as `Q3 + 3 × IQR` of the benchmark's own history.
- [Daly, Brown, Ingo, O'Leary, Bradford — Change Point Detection in Software Performance
  Testing (2020)](https://arxiv.org/pdf/2003.00584);
  [Fleming et al. — Hunter (2023)](https://arxiv.org/pdf/2301.03034).
- [Schulz & de Supinski — Practical Differential Profiling](https://www.osti.gov/servlets/purl/914615);
  [Bezemer et al. — differential flame graphs](https://www.researchgate.net/publication/282681970_Understanding_software_performance_regressions_using_differential_flame_graphs).
- The probe: `perf diff -c delta-abs -s symbol -o 1 target/boot-d-evidence/d2-0149b26-1.data
  target/boot-d-evidence/d2-792c2e7-1.data`, 2026-09-22, desk on the desktop grub line, 2.0 s.
