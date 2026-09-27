# ADR-0212 — An A/B boot stops early only to discard: the micro-benches run a fixed twenty rounds, and the density arm gets one futility look at six

- **Status**: **Accepted — 2026-09-27**, by the manager under the owner's delegation of that day; proposed the same day. Written by the architect (Opus) at the manager's request, after the
  owner asked (2026-09-27, in chat) why a measurement boot takes so long and agreed to a staged
  boot with an early-stop rule. The manager required the rule on disk **before** any timing of the
  boot is seen. Accepting it is the manager's, under the owner's delegation of that day, together
  with the plan revision it comes with ([2026-09-27-p4-simd](../plans/2026-09-27-p4-simd.md),
  *Sửa plan giữa chừng*, *Sửa 1*).
- **Date**: 2026-09-27
- **Deciders**: Tran Manh Thang (by delegation to the manager, 2026-09-27). Written by the architect (Opus).
- **Related**: [ADR-0090](ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  decisions 2, 3, 5 (the pre-built rotation, `n ≥ 20`, provisional below it);
  [ADR-0095](ADR-0095-a-drift-is-read-at-re-record-time-not-by-a-wider-band-a-segment-is-named-by-a-rule-and-the-desk-runs-strict-at-every-boot.md)
  decisions 2 and 4 (re-record ledger, `--strict` before and after);
  [ADR-0102](ADR-0102-a-line-that-moves-while-its-instruction-count-does-not-is-a-layout-move-and-the-count-is-read-off-the-desk.md)
  decisions 1–2 (the instruction count); [ADR-0202](ADR-0202-phase-4s-one-s9-boot-is-pre-built-driven-by-a-committed-script-and-onload-lives-only-inside-its-block.md)
  decisions 1–2 (pre-built, no tool call while the driver runs);
  [ADR-0211](ADR-0211-each-swar-kernel-is-judged-on-its-own-case-and-the-checksum-is-measured-against-a-loop-the-compiler-already-vectorises.md)
  (the per-kernel lines this ADR schedules). **No Accepted ADR's substance is changed**: every
  line, threshold and `n` stays as those ADRs wrote it; this ADR decides only the *order* in which
  the arms are run and one condition under which a run that can no longer keep anything is not
  continued.

## Context

The phase 4 rows 8–9 boot, as first planned, is one `scripts/ab-rotation.sh` rotation of four
arms (`A`, `S`, `C`, `SC`) with every suite in every round, ≥ 20 complete rounds. By boot D's
rate that is 9–11 hours. The owner asked whether it can be cut. Where the hours go decides the
answer, so it is computed first.

1. **Almost all of a round is `density`.** Every timed case in the shared harness is 10 000
   warm-up calls plus best-of-7 × 200 000 timed calls (`crates/codec/benches/harness.rs`,
   `Suite::bench`), i.e. ~1.41 M calls per case. With the *A-desk* medians
   (`measured-costs.md`, `w0`, n = 20) that is:
   - `parse`, three cases, 296.7 ns together → **~0.4 s** per run;
   - `serialize`, four cases, ~255 ns together → **~0.4 s**;
   - `checksum`, three cases, ~19 ns together on the desktop line (rehearsal, not a figure) →
     **< 0.1 s**; `decimal`, two cases, of the same order;
   - `density`, eleven `engine turn` cases, 244 389 ns together → **~345 s ≈ 6 min** per run.

   Per arm and round the driver adds one `check-machine.sh` read (`QUIET_WINDOW` 1 s plus the
   other rows) and `GAP` 8 s. So a round of the four micro-bench suites over four arms is
   **about one minute**, and each `density` run adds **about six**. Cross-check against the one
   rate on record: boot D's rounds 1–12 took 23:36 → 06:50, **~36 min a round**
   (`measured-costs.md` *Boot D*), for four arms carrying `density` (~24 min) and six carrying
   `validate` (whose 33-group case alone is ~115 s by the same arithmetic). The estimate and the
   record agree. **These are estimates from constants, not measurements**; plan step 5's
   rehearsal replaces them (the mtimes of `raw/*.txt` give each suite's duration).
2. **The micro-benches are cheap enough that stopping them early buys minutes, not hours.** Five
   rounds instead of twenty saves ~15 minutes. And `A`'s three `checksum` cases and two
   `decimal` cases must be recorded as baselines from a twenty-round median in this boot anyway
   (plan, boot step 4; ADR-0090 decision 3 forbids a baseline below `n = 20`), while ADR-0090
   decision 3 makes every compared arm share the control's rounds. `A` runs twenty rounds of its
   micro-benches whatever is decided; the other three arms ride along at ~45 s a round.
3. **`density` is where the hours are, and most of it can only matter for a kernel that passed
   its codec line.** ADR-0211 decision 2 reads the density clause **once, on the arm made of the
   kernels that passed their codec line**. A kernel that fails its codec line is removed whatever
   `density` says (ADR-0211 decision 3). Running `density` on every arm before the codec lines are
   read measures, in the likely case (ADR-0211 *Consequences*: "the checksum loses", and the
   plan's prediction that both are removed), eight hours of numbers no verdict reads.
4. **More rounds do not remove the error that matters most here.** Kalibera and Jones model
   benchmark variation at three levels — build, execution, iteration — and put repetition where
   the uncertainty is (["Rigorous benchmarking in reasonable time"](https://dl.acm.org/doi/10.1145/2464157.2464160),
   ISMM 2013). It follows — this is the architect's reading, not the paper's sentence — that
   repeating executions of one binary cannot average out variation that is fixed per build. A code-layout offset is fixed per binary (Mytkowicz et al., cited by ADR-0211;
   ADR-0102's +10 % case with a −0.03 % count). Round 6 and round 20 of the same pair carry the
   same layout bias. This repository answers layout with the instruction count
   (ADR-0102 decision 1; `rustc-perf` makes instructions its default metric "because it has the
   least variation", [collector README](https://github.com/rust-lang/rustc-perf/blob/master/collector/README.md)),
   not with more rounds. Rounds buy down **run-to-run** noise only, and that noise on these cases
   is small and recorded: `max/median` over twenty rounds **1.000–1.028** on `w0` and
   **1.007–1.014** on `wa`'s session-count `engine turn` cases (*A-desk*), with one unexplained
   outlier arm at **1.120–1.135** (`w1`, *The band across PRs B, C, #85 and #86*).
5. **Stopping on a look is safe in one direction only.** Repeatedly testing a running A/B and
   stopping at the first "significant" reading inflates false positives
   (Johari, Koomen, Pekelis, Walsh, ["Peeking at A/B tests"](https://doi.org/10.1145/3097983.3097992),
   KDD 2017); sequential designs exist to make looks honest (Wald's SPRT, *Annals of Mathematical
   Statistics* 16(2), 1945). Group-sequential practice separates a stop *for success* from a stop
   *for futility*: a non-binding futility bound leaves the design's false-positive rate
   controlled: continuing past a crossed lower bound, "Type I error is still controlled"
   ([gsDesign vignette](https://cran.r-project.org/web/packages/gsDesign/vignettes/GentleIntroductionToGSD.html)).
   Here the costly error is **keeping code that does nothing** (ADR-0100's whole reason to exist);
   discarding a kernel that was barely at its line costs one kernel ADR-0045 had already declined.
   So the only early stop worth having is a futility stop, and a keep stays a full fixed-sample
   verdict. ADR-0090 decision 3 already says the same thing in this repository's words: below
   `n = 20` a median is *provisional*, "never a band verdict and never a baseline".
6. **What the tools do.** Criterion.rs defaults to 100 samples, a 0.05 significance level and a
   1 % noise threshold that filters out statistically significant but small changes
   ([`Criterion` docs](https://docs.rs/criterion/latest/criterion/struct.Criterion.html));
   `hyperfine` runs at least 10 runs and at least 3 s ([README](https://github.com/sharkdp/hyperfine));
   Google Benchmark's `compare.py` U test warns that it needs "no less than 9" repetitions
   ([tools.md](https://github.com/google/benchmark/blob/main/docs/tools.md)). None of them stops a
   comparison early; all of them fix `n` and then test. This repository's `n ≥ 20` (ADR-0031,
   ADR-0090) is above all three minimums, and this ADR keeps it for every verdict that keeps code.
7. **The driver cannot run a subset of rounds, resume a round count, or be read mid-run by the
   manager.** `ab-rotation.sh` numbers rounds from 1 in every invocation (a second run into the
   same `EVIDENCE` directory would reuse round numbers), and ADR-0202 decision 2 has the manager
   make no tool call while a driver runs (boot E lost four arm-rounds to the manager's own tool
   calls, `measured-costs.md` *Boot E*, S1). So a stage is **one whole invocation into its own `EVIDENCE` directory**, and a look
   happens only **between** invocations. The script needs no change.

What the architect had seen when writing this: the plan's step-5 **instruction counts**
(`target/simd-evidence/prep/bench-instructions.log`, `perf stat instructions:u`, not a
timing), which the plan requires before the boot anyway. No timing of any arm on the §9 line
existed. The thresholds below are derived from facts 1–7 and from the ADRs cited, not from those
counts.

## Decision

### 1. The boot runs in stages, each one invocation with its own evidence directory

| Stage | Arms and suites | `ROUNDS` | Reads |
|---|---|---|---|
| **0** | — (`check-machine.sh`, the discarded first run, `scripts/bench.sh --strict` on `A`, ADR-0095 decision 4 "before") | — | a red `--strict` is a finding, never a stop |
| **1 — codec** | `A`: `parse,serialize,checksum,decimal`; `S`: `parse,checksum`; `C`: `parse,serialize,checksum`; `SC`: `parse,serialize,checksum` — **no `density`** | **24** (≥ 20 complete needed; four spare rounds cost ~4 min) | ADR-0211 decision 1 (each kernel's codec line) and the share half of decision 2, at `n ≥ 20`, read **once, after the invocation exits** — no interim look |
| **2a — density look** | `A` and `X`: `density` only, where `X` is the arm ADR-0211 decision 2 names (`S`, `C` or `SC`) | **6** | decision 2 below; can only **kill** |
| **2b — density verdict** | the same two arms, same suite, **fresh** `EVIDENCE` directory | **22** (≥ 20 complete needed) | ADR-0211 decision 2's density half at `n ≥ 20`, as written |
| **3** | — (baselines per ADR-0095 decision 2 on the tree that is kept, then `bench.sh --strict` "after", then `check-machine.sh`) | — | ADR-0095 decision 4 |

Stage 1 always runs. Stage 2 runs only when a kernel passed its codec line in stage 1 **and**
the share half fails (fact 3 of ADR-0211 says it will) **and** the pre-boot
`bench-instructions.sh` on the `density` binaries of `A` and `X` reads `work-changed`. If that
count reads `same-work`, ADR-0211 decision 2 already says the density half cannot count, so its
timing is not run: the kernel is removed by ADR-0211 decision 3 with the count as the evidence.
Stages 2a and 2b never run on more than one `X`: ADR-0211 decision 3 forbids a search over
sub-arms.

### 2. The futility look: one look, after six complete rounds, and it can only discard

After stage 2a exits, the manager runs `CONTROL=A scripts/ab-rotation.sh --summary` on its
`runs.txt` and reads, for `engine turn, 64 busy sessions`, `X`'s median `m_X` and `min/median`
`r_X`, and `A`'s median `m_A` and `max/median` `R_A`. With `n = 6` on both:

> **Kill** if `(m_X × r_X) / (m_A × R_A) − 1 > −3.0 %` — `X`'s fastest run of the six is not
> 3 % faster than `A`'s slowest run of the six.

Otherwise, stage 2b runs. If fewer than six rounds of 2a are complete, there is no look: 2b runs.

**Why this bound, in numbers.** In a rotation every round is a pair (`A` and `X` share it,
ADR-0090 decision 3). If `X`'s fastest run is not 3 % below `A`'s slowest, then **every** one
of the six paired rounds has `X` slower than the 3 % line, because each pair's ratio is at least
the fastest-over-slowest ratio. If the kernel's true effect were at the line or better, each
round would land on the far side of the line with probability ≤ ½, so six out of six has
probability **≤ 2⁻⁶ = 1.6 %** (a sign test on six pairs). That is below the 5 % Criterion uses
by default and below its one-sided half. With the tighter spreads recorded for this case family
(fact 4, `wa`'s session-count cases, `max/median` 1.007–1.014 over twenty rounds; six rounds
reach less far into the tails), a kernel with **no** density effect lands at a bound of about
−1.5 % to −2.5 % and is killed at round 6. At `w0`'s wider 1.028 it may not be; a kernel near the
line, or any arm as noisy as `w1` (1.13), is not killed and goes to the full verdict — the
failure is extra hours, never a wrong keep. The margin is therefore not a number read
off one boot and trusted: it is the arm's own spread in the same six rounds, and it widens by
itself on a noisy day.

A **later look adds nothing**, which is why there is one: "all rounds above the line" at round
k > 6 implies it at round 6, so any later futility look can only repeat round 6's answer.

### 3. A keep is a full, fixed-sample verdict on rounds that were not used to decide whether to continue

Stage 2b is a fresh twenty-plus rounds. Its data alone decide ADR-0211 decision 2's density
half and supply any `density` baseline of a kept tree (ADR-0095 decision 2, from a median of
`n ≥ 20`). Stage 2a's six rounds are **not pooled** into 2b: they are published beside it as
*provisional, n = 6* (ADR-0090 decision 3), and they are what the kill, if there was one, rests
on. Keeping them out of the verdict means the keep estimate is not conditioned on having survived
the look — the verdict is exactly the fixed-sample one ADR-0211 wrote.

If stage 1 or stage 2b ends with fewer than 20 complete rounds (a timer, a load), one top-up
invocation runs the missing count into a further directory, and the two are merged with a round
offset exactly as boot D (`d4m/`) and boot E (`s1m/`) did, with the offset and both directories
named in `measured-costs.md`. Below 20 after the top-up, the numbers are *provisional* and
**nothing is kept** — the boot's handoff names what is missing.

### 4. What stage 1 decides, and what it does not

Stage 1 reads ADR-0211 decision 1 exactly as written: each codec line at `n ≥ 20`, the median
against −15 %, no margin added and none removed. A kernel that fails is removed and never
reaches `density`; a kernel that passes goes to stage 2 — a pass of stage 1 is **not** a keep.
The share half is computed from stage 1's parse medians and printed. The "dumb control" cases of
the plan (`decimal`, `SendingTime from the cache…`) are read from stage 1.

### 5. This ADR is the pattern for the next A/B boot, not only this one

Any A/B boot in this repository where one expensive suite is read only for arms that passed a
cheaper line runs the cheap line first at its full `n`, and gives the expensive suite one
futility look after six complete rounds with the bound of decision 2, recomputed for that
suite's own line. A plan that wants a different look count, or a look that can **keep**, needs
a new ADR.

## Alternatives considered

- **Stage 1 at about five rounds with an early decision on the codec line**, as proposed in chat.
  Rejected on fact 2: it saves ~15 minutes, and `A` must run twenty rounds of the same suites
  for its baselines anyway, so the other arms' extra fifteen rounds cost ~11 minutes. A
  kill-or-pass at five rounds would also be a look at a ±15 % line with no spread of its own to
  judge by — and a *pass* at five rounds is exactly the early "yes" fact 5 warns against.
- **A true sequential test (SPRT) on each line.** Rejected: an SPRT needs a likelihood model per
  round, and this harness's runs are multimodal per process ("the modes are drawn per
  process", `harness.rs`, `Baseline::margin`); a wrong model makes the stated error rates
  fiction. The sign bound of decision 2 needs no model beyond independent rounds.
- **One stage-2 invocation of twenty rounds, read at round six and stopped by killing the
  driver.** Rejected: ADR-0202 decision 2 (no tool call while the driver runs); killing the
  script leaves the running `density` binary orphaned and loading the core for up to six more
  minutes, and the pooled verdict would include the rounds that decided to continue. It would
  save six rounds (~75 min) only on the path where a kernel survives the look.
- **Stage 2a's rounds merged into 2b** (twenty-six rounds, or 2b cut to fourteen). Rejected for
  the conditioning reason in decision 3, and because a merge is a hand step; the top-up merge of
  decision 3 is kept only for lost rounds, where there is precedent and no look was taken.
- **A fixed noise margin (e.g. 3 × (max/median − 1), the ADR-0095 decision 3 shape) instead of
  the extremes bound.** Not chosen: that rule attributes a step at `n = 20`; at `n = 6` its
  statistical meaning is unstated. The extremes bound has a stated error probability and uses
  the same two summary columns.
- **Keep the plan as written (one twenty-round rotation of everything).** Rejected by the owner's
  request, and by fact 3: it spends most of its hours on numbers no verdict reads in the likely
  outcome. Its only advantage is `density` figures for all four arms; those are the figures
  ADR-0211 decision 3 says no decision uses.

## Consequences

**Good**

- In the predicted outcome (both kernels fail their codec line) the boot's measuring time falls
  from ~9–11 h to about **1 h** (stage 0 ~15 min, stage 1 ~25 min, stage 3 ~15 min —
  estimates, fact 1). A kernel that passes and is killed at the look adds ~75 min; one that
  survives the look adds ~6 h. Every path is shorter than the original plan's.
- Every verdict that **keeps** code is the same fixed-sample `n ≥ 20` verdict ADR-0211 wrote, on
  data not used to decide whether to continue. The early stop can only lower the chance of
  keeping a kernel, never raise it.
- The false-discard probability is stated (≤ 1.6 % for a kernel at or beyond its line, assuming
  independent rounds) and adapts to the day's own spread.
- No script changes; each stage is a plain `ab-rotation.sh` invocation, so ADR-0090 decision 2's
  refusal to compile and its sha256 manifest apply per stage unchanged.
- `bench.sh --strict` now runs before as well as after, which ADR-0095 decision 4 required and
  the plan's first boot sequence had only once.

**Bad — and accepted**

- **No `density` figures for arms that failed their codec line.** `measured-costs.md` gets their
  codec numbers but not what they would have done to the engine turn. If a later reader wants to
  know, it is a new boot.
- **Two stages are two slices of the machine's day.** Stage 1 and stage 2 do not share rounds;
  each carries its own control `A`, so every comparison is still within one interleaved rotation,
  but a codec figure and a density figure from one arm are an hour or more apart. Boot D's
  rounds 13–20 were the same shape and were recorded, not smoothed over.
- **The sign bound assumes rounds are independent.** A mode that persists across several rounds
  (the harness's per-process modes are drawn per process, not per round, but ADR-0102's layout
  offset is persistent by design) makes six rounds worth less than six independent pairs, and
  the 1.6 % is then optimistic. The bound is conservative in another way (extremes, not pairs),
  and a killed kernel is one ADR-0045 had already declined; the risk is accepted, not removed.
- **Stage 2a costs ~75 min on the path where the kernel survives**, because its rounds are not
  pooled. That is the price of decision 3's clean verdict.
- **A future plan must re-derive the bound for its own line** (decision 5) — a 3 % line with a
  1.13 spread will never be killed early, and that plan pays the full stage.
- The time estimates in this ADR are from constants and one old rate, not a measurement of these
  binaries; the plan's step-5 rehearsal is what the owner is told.

## Sources

- Kalibera, Jones, *Rigorous benchmarking in reasonable time*, ISMM 2013,
  <https://dl.acm.org/doi/10.1145/2464157.2464160> (abstract via <https://kar.kent.ac.uk/33611/>;
  the full-text PDF timed out on 2026-09-27, so only the abstract's claims are used: variation
  arises between builds, executions and iterations, and repetition is placed where it arises).
- Johari, Koomen, Pekelis, Walsh, *Peeking at A/B tests: why it matters, and what to do about it*,
  KDD 2017, <https://doi.org/10.1145/3097983.3097992>; extended as *Always valid inference*,
  Operations Research 70(3), 2022, <https://doi.org/10.1287/opre.2021.2135>.
- Wald, *Sequential tests of statistical hypotheses*, Annals of Mathematical Statistics 16(2),
  117–186, 1945.
- gsDesign, *A gentle introduction to group sequential design* (non-binding futility bounds),
  <https://cran.r-project.org/web/packages/gsDesign/vignettes/GentleIntroductionToGSD.html>.
- Criterion.rs defaults (`sample_size` 100, `significance_level` 0.05, `noise_threshold` 0.01),
  <https://docs.rs/criterion/latest/criterion/struct.Criterion.html>.
- hyperfine, "at least 10 benchmarking runs and … at least 3 seconds",
  <https://github.com/sharkdp/hyperfine>.
- Google Benchmark `compare.py`, U test "no less than 9" repetitions,
  <https://github.com/google/benchmark/blob/main/docs/tools.md>.
- rustc-perf collector, instructions as the default metric,
  <https://github.com/rust-lang/rustc-perf/blob/master/collector/README.md>.
- A search for a published early-stopping or futility rule inside a CPU micro-benchmark harness
  (2026-09-27) found none: the harnesses above all fix `n` first.
- This repository: `crates/codec/benches/harness.rs` (`Suite::bench`, `Baseline::margin`);
  `scripts/ab-rotation.sh` header (*EVIDENCE*, *INTERRUPTIBLE*, *USAGE*) and main loop;
  `measured-costs.md` *Boot D* (rate, *A-desk* medians and spreads), *Boot E* (`s1m/` merge),
  *Boot F* (`--strict` ~10 min).
