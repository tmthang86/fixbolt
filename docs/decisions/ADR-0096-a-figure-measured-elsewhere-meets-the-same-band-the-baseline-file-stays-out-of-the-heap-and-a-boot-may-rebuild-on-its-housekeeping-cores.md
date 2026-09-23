# ADR-0096 — A figure measured outside the harness meets the same band; the baseline file's size stays out of the bench's heap; a measurement boot may rebuild on its housekeeping cores; and item 51 closes at the mitigation tier

- **Status**: Proposed — 2026-09-23. Written for
  [boot-f-closes-the-open-items-and-powers-off](../plans/2026-09-23-boot-f-closes-the-open-items-and-powers-off.md);
  becomes *Accepted* at that plan's merge under the owner's standing mandate (2026-09-18), and
  one word from the owner reverses it. Nothing here has run. Decision 2 is conditional on the
  plan's step F1 confirming the hypothesis it rests on; if F1 refutes it, decision 2 is struck
  in this file with the sweep table cited, and the rest stands.
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang. Written by the architect (Fable) from `STATUS.md` items 51,
  85, 89, 96, 99, 100, the boot E section of `docs/reference/measured-costs.md`, the two strict
  outputs in `target/boot-e-evidence/`, and `git show` of `benches/baselines.tsv` at `9374820`
  and `8153ba0`.
- **Related**: [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md) (what a
  baseline is), [ADR-0031](ADR-0031-a-baseline-is-a-band.md) (the band, one comparator),
  [ADR-0067](ADR-0067-the-baselines-are-read-at-run-time-not-compiled-in.md) (the file is read
  at run time — the first time its size moved a case), [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md),
  [ADR-0069](ADR-0069-the-listener-is-polled-on-a-cadence-in-hft.md), [ADR-0090](ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  (decision 2: a boot never compiles), [ADR-0094](ADR-0094-the-descent-is-priced-by-attribution-inside-one-binary-not-by-a-second-binary.md),
  [ADR-0095](ADR-0095-a-drift-is-read-at-re-record-time-not-by-a-wider-band-a-segment-is-named-by-a-rule-and-the-desk-runs-strict-at-every-boot.md)
  (decision 2: a re-record names its cause; decision 4: strict twice per boot).

## Context

Boot E left `scripts/bench.sh --strict` structurally red on the §9 desk and three facts that the
harness's rules do not cover:

1. **Two cases have no baseline mechanism at all.** `crates/engine/benches/wakeup.rs` measures
   20 000 cross-thread wake-ups per arm and prints its p50 as a `NO BASELINE` case with a
   hard-coded `cases without a baseline: 2` line (lines 59–66, 153–159, 450–467). It cannot use
   `Suite::bench`, which times a closure itself (best-of-7 × 200 000); its figure is a
   percentile of samples it took on two threads. The comparator (`verdict.rs`, ADR-0031
   decision 4) has no entry point for a figure computed elsewhere, so the bench printed the
   words and skipped the band. `bench.sh` runs it unpinned (no `WAKEUP_CORES`); the two
   unpinned readings of boot E agree within 0.2 % (5060 / 5050, 4900 / 4909 ns).

2. **A case moved ×1.68 between two runs of one binary in one boot, and the file it reads
   grew by 671 bytes in between.** `journal put, 191 bytes, one slot` read 7.4 ns in S3 and
   12.4 ns in S4 and on four re-runs; `benches/baselines.tsv` was 24 226 bytes at S3 and
   24 897 bytes at S4 (the re-record added six lines). The harness reads that file with
   `read_to_string` before the bench closure allocates its `Store` and its 191-byte `msg`
   (`harness.rs:267-275`), so every heap address the timed loop touches shifted by the file's
   growth. The case copies the same source into the same slot on every iteration — the shape
   most sensitive to the address relation between source and destination (4K aliasing,
   store-forward checks; Zen 2's `rep movs` fast path also keys on the low bits of both
   pointers). Four other cases moved 5–10 % in the same direction pattern between S3 and S4.
   ADR-0067 already recorded the first instance of this trap (a line appended to the file
   moved a case 8.2 → 6.3 ns) and moved the read to run time, which took the file out of the
   *binary* and left it in the *heap*. Mytkowicz et al. (ASPLOS 2009) is the canonical
   description: environment size and link order move heap and stack and change results by
   whole percents, through load–store overlaps. The perf/NMI suspicion the boot E handoff
   raised found no supporting source: the NMI watchdog occupies one PMU counter while it runs,
   and no source describes a `perf record` that has exited leaving user code slower.

3. **The boot must rebuild before `--strict` can be green.** Both fixes above are code in
   `crates/*/benches/`; the rebuilt binaries are what `--strict` will run. ADR-0090 decision 2
   says a measurement boot "measures and never compiles" and ADR-0095 decision 4 voids a
   strict run that prints `Compiling`. The owner's instruction is to close every open item in
   this boot and power the desk off; a rule written for a boot with a reboot boundary before
   it has no provision for the closing build.

Two more open items end here by decision rather than by measurement: item 89's A/B was measured
at boot C (N ∈ {1, 16, 256}) and boot D (N = 1 vs 16 tiers) and its ADR is still `Proposed`;
item 51's cause is known at one tier (mitigations, −55…58 %, boot C) and the finer split needs
one reboot per arm.

## Decision

### 1. A figure measured outside the harness meets the same band through `Suite::figure`

`crates/codec/benches/harness.rs` gains `Suite::figure(&mut self, name: &str, ns: f64)`: look
up `(cpu, name)` in the baselines already read, run `verdict` (the one comparator), print the
same line `bench` prints (`baseline B xM = [floor, ceiling]`, `OVER BASELINE`, `UNDER
BASELINE`, or `NO BASELINE for '<cpu>'` with the paste-ready line), and count into the same
`over` / `under` / `missing` vectors that `finish` prints as `cases without a baseline: N`.
`Suite::bench` becomes "time best-of-7, then `figure`". `wakeup.rs` includes `harness.rs` by
`#[path]` as `density.rs` does, hands its two p50s to `figure`, and loses its hard-coded lines.
The statistic a `figure` line holds is the case's own — for `wakeup`, the p50 of 20 000
samples of one run — and the tsv line's `baseline` is still the median of `n ≥ 20` whole runs,
`margin` from the ladder, as ADR-0016 decision 3 says. The two `wakeup` lines are recorded
**unpinned**, because that is the procedure `bench.sh --strict` runs; a pinned figure, if one
is ever recorded, is a different case name. p99 and p99.9 are printed and **not banded**: at
20 000 samples the p99.9 is twenty samples, and a band over twenty samples is a wish.

### 2. The baseline file's size stays out of the bench's heap — *conditional on step F1*

`load_baselines` and `cpu_model` read into a `String::with_capacity(1 << 20)`. On glibc an
allocation of 1 MiB is served by `mmap`, outside the brk heap, so the addresses the bench
closure allocates afterwards do not depend on how long the file is. The proof is not the
theory: it is the sweep — the same pinned binary run with the file padded by `k = 0…1024`
bytes in steps of 16 reads a staircase before the fix (F1b) and is flat within ×1.10 after it
(F8). If F1b reads no staircase, this decision is struck here and the hypothesis is recorded
as refuted with the table. If F8 is not flat, the fix is not accepted and the line is not
re-recorded. A `one slot` re-record after a flat F8 is a re-record **with a cause** under
ADR-0095 decision 2 — this decision is its citation — and is not "moving a line to make
`--strict` green". The trap goes into
`docs/reference/recording-a-baseline-changed-the-baseline.md` as its second instance, with the
test `crates/codec/tests/bench_verdict.rs` asserting the capacity, and the sweep as the desk
regression.

### 3. A measurement boot may rebuild on its housekeeping cores, once, when the plan names it

ADR-0090 decision 2 is narrowed, not reversed: its purpose is that no run *measures a machine
that is compiling*, and that is what `Compiling` in a strict output detects. A plan may name
one build step inside a boot, run as `nice taskset -c 0-5 cargo bench --no-run …` (never on
an isolated core), followed by at least ten minutes with no tool call and one discarded run,
and by `scripts/check-machine.sh` reading `fail 0` before the next measurement. Every
measured run after it still prints no `Compiling` line, or is void. A boot that can end at a
reboot boundary keeps building before it; this decision is for a boot that ends at power-off.

### 4. Two rules for published wire figures, pending item 85's probe

(a) `scripts/compare-w2w-procedures.sh` is the comparator of ADR-0068 decision 2 — two
`w2w-baseline.sh` summaries in, one line per published percentile out, exit 1 when any
percentile differs by more than 5 % of the smaller — and `scripts/check-w2w-compare.sh` is
its reversal in the `gates` job (a 6 % pair fails, a 4 % pair passes, at each of p50, p99,
p99.9). ADR-0068's rule is no longer applied by hand. (b) If the plan's probe C-85 confirms
that a procedure run within two minutes of a build differs from one run ten minutes later
while two idle procedures agree, ADR-0068 gains one sentence: the first procedure of a
published figure starts at least ten minutes after the last build, and `w2w-baseline.sh`'s
printed `uptime` and binary mtime are the evidence. If the probe refutes it, item 85 closes as
*comparator built, cause unnamed*, and ADR-0068 does not change.

### 5. Item 51 closes at the mitigation tier; item 89 closes and ADR-0069 is accepted

Item 51's cause is named at the tier that was measured: 55–58 % of a TCP loopback round trip
on this Zen 2 desk is the kernel's CPU mitigations (boot C, `mitigations=off`), 3.3 % is
conntrack (boot B), the remainder is unsplit. `DESIGN.md` §8 keeps its floor as measured with
mitigations on and says so in one sentence beside the figure. Splitting the mitigation term
per mitigation (`retbleed=off`, `spec_rstack_overflow=off`, `spectre_v2=off`) costs one reboot
per arm and is **not an open obligation**; it is a plan the owner may ask for. The flush arm
(Tailscale down, ruleset flushed) runs in the plan only if the controlling session does not
ride `tailscale0`, and its result is recorded either way.

Item 89's default was set by a figure at boot C (ADR-0069 decision 4 is satisfied); the open
half — the mechanism of the +0.6…1.9 % on the admin path — is answered by ADR-0095 decision 3's
naming rule over the boot D records already on disk, *named* or *accept, unnamed with IPC*,
and ADR-0069 moves `Proposed` → `Accepted` in the same pull request.

## Alternatives considered

- **Exempt the two `wakeup` cases from `--strict` by a rule in `bench.sh`.** Rejected: a case
  with no band is a number nobody guards, and the two figures reproduce within 0.2 % — they
  are the easiest cases in the suite to band.
- **A second comparator inside `wakeup.rs`.** Rejected by ADR-0031 decision 4: one source, two
  consumers, no copy to drift.
- **Read the baseline file after the bench closures allocate.** Rejected: the closures run
  inside `f(&mut suite)` and the file is needed by every `bench` call inside it; reordering
  would move the read into the timed section's neighbourhood. A fixed-capacity buffer changes
  nothing else.
- **Page-align the timed buffers in every bench.** Rejected for now: it fixes each bench one
  by one and leaves the next bench to rediscover the trap; the harness-level fix removes the
  variable. A bench that *wants* a chosen source/destination relation can still align its own.
- **Reboot after the build to honour ADR-0090 decision 2 literally.** Rejected: the owner
  wants power-off after this boot; a reboot costs the boot's warm state and ~30 minutes, and
  the rule's purpose is met by decision 3.
- **`--call-graph fp` with `-C force-frame-pointers` for item 96.** Rejected: a rebuild moves
  layout, the very term ADR-0094 exists to avoid; dwarf unwinds the pinned binary through its
  existing `.eh_frame`. LBR is not available on Zen 2 (BRS is Zen 3, LbrExtV2 is Zen 4).
- **Close item 99 by re-recording `one slot` at 12.4.** Rejected: that is the boot E do-not
  list, and a number recorded from one heap layout would be as arbitrary as the other.

## Consequences

**Good**

- `bench.sh --strict` can be green on the desk with every case banded through one comparator,
  and a `wakeup` drift becomes visible the way every other drift is.
- If decision 2 holds, a class of "same binary, different number" surprises is closed at the
  harness, with a sweep that anyone can re-run in ten minutes on any machine with the pinned
  binary. ADR-0067's story gets its second chapter and a test.
- Item 96 gets an inclusive figure without a second binary or a profile change; item 89 and
  item 51 stop being open by drift of attention and become decisions with the evidence beside
  them.
- ADR-0068's rule is run by a script and reversed in CI.

**Bad — and accepted**

- **Decision 2 rests on glibc's mmap threshold.** A 1 MiB allocation goes to `mmap` under the
  default `M_MMAP_THRESHOLD` (128 KiB) and the dynamic threshold never rises above the largest
  freed mmapped chunk, which the harness never frees; another allocator or a changed threshold
  would put the buffer back in the heap. The sweep is the guard, not the argument.
- **The `wakeup` lines are unpinned figures.** They band the procedure `--strict` runs, not
  the pinned wake-up latency `DESIGN.md` §8 quotes; the two are different numbers and the
  case name says which.
- **Decision 3 lets a boot that compiled publish numbers.** The ten quiet minutes, the discard
  run and `check-machine.sh` are what stand between the build and the figure; a thermal or
  page-cache effect that outlives ten minutes would not be caught by them. The strict output's
  `Compiling` count is still read.
- **Item 51 closes without the split the owner might have wanted.** The decision is written so
  that asking for the split is one plan, not a re-litigation.
- **Item 96's number is still "≥".** The first inlined level is not a symbol; only a line-table
  build with an identical `.text` could attribute it, and that is a conditional second tier.
- **Decision 4b is a rule about waiting**, and it is only as good as the probe that created
  it; three procedures in one boot are one data point per condition.

## Sources

- Mytkowicz, Diwan, Hauswirth, Sweeney — *Producing Wrong Data Without Doing Anything
  Obviously Wrong!* (ASPLOS 2009): <https://users.cs.northwestern.edu/~robby/courses/322-2013-spring/mytkowicz-wrong-data.pdf>
- 4K aliasing, with a runnable example: <https://github.com/Kobzol/hardware-effects/blob/master/4k-aliasing/README.md>
- Zen 2–4 `rep movs` slow path keyed on the low bits of both pointers (xen-devel, 2026-01):
  <https://ratatoskr.run/xen-devel/2026/01/15396505/t>
- Criterion.rs analysis (mean/median bootstrap, no tail bands): <https://bheisler.github.io/criterion.rs/book/analysis.html>
- perf-record(1), `--call-graph dwarf`: <https://man7.org/linux/man-pages/man1/perf-record.1.html>
- rustc `-C force-unwind-tables`: <https://doc.rust-lang.org/rustc/codegen-options/index.html>
- Frame pointers vs DWARF, overhead and accuracy: <https://rwmj.wordpress.com/2023/02/14/frame-pointers-vs-dwarf-my-verdict/>,
  <https://linuxvox.com/blog/what-do-the-perf-record-choices-of-lbr-vs-dwarf-vs-fp-do/>
- AMD branch sampling: BRS (Zen 3) <https://lwn.net/Articles/877245/>, LbrExtV2 (Zen 4) <https://lwn.net/Articles/904482/>
- LLVM debug-info invariance is a goal, not a guarantee: <https://github.com/llvm/llvm-project/issues/37076>;
  Cargo profile `debug = "line-tables-only"`: <https://doc.rust-lang.org/cargo/reference/profiles.html>
- NMI watchdog holds a PMU counter (perf stat hint, 2026-06): <https://ratatoskr.run/linux-perf-users/2026/06/17122660/t>;
  <https://github.com/AMDESE/amd-perf-tools>. No source found for a post-exit effect of `perf record` on user code.
- Zen 2 mitigation cost: <https://www.phoronix.com/review/amd-3950x-retbleed>;
  SRSO on Zen 1/2: <https://www.kernel.org/doc/html/latest/admin-guide/hw-vuln/srso.html>
