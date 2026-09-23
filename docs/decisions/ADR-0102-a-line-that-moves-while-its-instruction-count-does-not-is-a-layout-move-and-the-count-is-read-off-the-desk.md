# ADR-0102 — A timed line that moves while its instruction count does not is a layout move, and the count is read off the §9 line

- **Status**: Accepted — 2026-09-23 (by the manager under the owner's standing mandate, at P6 of the closing-phase-2 plan). Proposed 2026-09-23. Written for
  [closing-phase-2](../plans/2026-09-23-closing-phase-2.md); becomes *Accepted* at that plan's
  merge under the owner's standing mandate (2026-09-18), and one word from the owner reverses it.
  Decisions 1–5 state their verdict rules **before** the plan's rows run; the outcomes are
  appended under *Outcome* by the plan's step P5 and change no rule above them.
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang. Written by the architect (Opus) from `STATUS.md` item 101, the
  boot F *Not proven* list, `docs/reference/measured-costs.md` *Boot F, item 99* and *Boot F,
  `--strict` three times*, ADR-0049, ADR-0095 and ADR-0096 decision 2, `git show 6b2833b --
  crates/codec/benches/harness.rs`, and three probes run on the desk on its **desktop** grub line
  (cited under *Context*, fact 4).
- **Related**: [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md),
  [ADR-0031](ADR-0031-a-baseline-is-a-band.md),
  [ADR-0049](ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md) (function
  alignment; its *Open question* on loop-body alignment),
  [ADR-0067](ADR-0067-the-baselines-are-read-at-run-time-not-compiled-in.md),
  [ADR-0093](ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md)
  (sudo in a committed script),
  [ADR-0095](ADR-0095-a-drift-is-read-at-re-record-time-not-by-a-wider-band-a-segment-is-named-by-a-rule-and-the-desk-runs-strict-at-every-boot.md)
  decision 2 (a re-record names its cause) and decision 3 (the rotation attribution rule),
  [ADR-0096](ADR-0096-a-figure-measured-elsewhere-meets-the-same-band-the-baseline-file-stays-out-of-the-heap-and-a-boot-may-rebuild-on-its-housekeeping-cores.md)
  decision 2 (the fixed 1 MiB read buffer).
  [reference/recording-a-baseline-changed-the-baseline](../reference/recording-a-baseline-changed-the-baseline.md),
  [reference/a-benchmark-that-measures-where-the-compiler-put-it](../reference/a-benchmark-that-measures-where-the-compiler-put-it.md).
  `CLAUDE.md` §2 non-negotiable 10, §7, §10 (*a cause accepted because a knob moved with it*).
- **Answers**: `STATUS.md` item 101 (what moved `walk nested group + varData` +10 %), the residue
  of item 99's sweep (max/min 1.12 against a 1.10 bound), and *which commit of PR B carries item
  95's step* — with one instrument, and without a §9 boot.

## Context

1. **Every fix to one layout term so far has been a new layout for another.** Item 52 (a
   baseline line compiled in moved a case 8.2 → 6.3 ns; ADR-0067), item 99 (the length of
   `benches/baselines.tsv` moved `journal put, 191 bytes, one slot` 7.4 → 12.4 ns at run time;
   ADR-0096 decision 2), item 101 (the item 99 fix moved `walk nested group + varData` 153.6 …
   160.3 → 174.2 … 176.1 ns, A/B in boot F, nothing in `crates/sbe` changed). Each cost part of a
   §9 boot to find. The band (×1.10 on most lines) cannot tell a layout move of ~10 % from a code
   change of ~10 %: both read `OVER BASELINE`.
2. **The layout term inside one bench binary is coupled across cases.** `harness::suite` takes
   the whole bench body as one closure, and every `Suite::bench::<F>` is inlined into it: in the
   `sbe` binary there is **one** text symbol, `sbe::harness::suite::<sbe::main::{closure#0}>`,
   holding every case's timed loop. `6b2833b` moved the comparator out of `bench` into a new
   `Suite::figure`, which the compiler kept out of line: that symbol went from **0x4138 = 16 696
   bytes** (pre-`6b2833b` binary, sha256 `89bcc880…`) to **0x2ff4 = 12 276 bytes** plus a
   0x8f5-byte `Suite::figure` (post, sha256 `1719ddc4…`) — `nm -C -S --defined-only`, read by the
   architect 2026-09-23. ADR-0049 pins each **function** start to 64 bytes; it pins nothing
   inside one. So every case's loop moved when a part of the harness no case times was edited.
3. **The walk case touches no heap.** Its message is `nested_buf: [0u8; 128]`, a stack array in
   the `suite` closure (`crates/sbe/benches/sbe.rs`, the `nested_buf` binding), its schema tables
   are statics, and `crates/sbe/benches/alloc.rs` asserts the path allocates nothing. So "the heap
   position after the 1 MiB block" — one of item 101's two candidates — has nothing to act on in
   this case. Glibc's raised mmap threshold was already excluded in boot F
   (`MALLOC_MMAP_THRESHOLD_=131072`: 174.7 … 177.4).
4. **The move reproduces off the §9 line, and counts separate it from work.** `[measured
   2026-09-23, architect probe, desk on the desktop grub line (no isolcpus), mitigations on,
   check-machine not run, taskset -c 6; a diagnostic, not a published figure]`, the two pinned
   `sbe` binaries above, three interleaved pairs of `sudo -n perf stat -x, -e
   instructions:u,cycles:u -- taskset -c 6 <bin>` and one plain run each:

   | arm | `instructions:u` (3 runs) | `cycles:u` | `walk nested group + varData` |
   |---|---|---|---|
   | pre-`6b2833b` | 4 408 882 945 / 4 408 883 023 / 4 408 882 884 | 1.417 / 1.414 / 1.416 G | 151.6 / 158.3 / 157.5 ns |
   | post-`6b2833b` | 4 407 487 817 / 4 407 487 898 / 4 407 487 582 | 1.508 / 1.527 / 1.566 G | 175.5 / 176.8 / 174.9 ns |

   Same-arm spread: 139 and 316 instructions. Between arms: **−1 395 128 instructions
   (−0.032 %)** — the new binary does slightly *less* work — while `cycles:u` rose by 0.09–0.15 G,
   which is the size of +20 ns × 1.41 M timed calls of one case (≈ 0.1 G cycles at 3.6 GHz). The
   timing step is item 101's, reproduced on a machine state boot F did not have.
   A second probe, `setarch x86_64 -R` (ASLR off) with an unused environment variable of `k`
   bytes, `k ∈ {0, 16, 32, 48, 64, 128, 256, 512, 1024, 2048}` — Mytkowicz et al.'s lever, which
   moves the stack — read **151.8 … 160.4** (pre) and **174.2 … 178.4** (post) at every `k`: the
   stack's position does not move either arm. What is left is the binary's own layout (code, or
   the read-only data next to it).
5. **The retired-instruction count is layout-free, and cheap here.** It does not depend on where
   code or data sit, only on which instructions run; the harness runs a fixed 10 000 + 7 × 200 000
   calls per case (`crates/codec/benches/harness.rs`, `Suite::bench`), so a bench process's count
   is a function of the binary alone. Weaver and McKee measured the counter's non-determinism on
   x86 Linux, name ASLR and environment size among its causes, and bring the error under 0.002 %
   with setup care; fact 4's same-arm spread is 3 × 10⁻⁸ of the count. The count needs `perf` and
   a PMU, which the desk has on **either** grub line; GitHub's runners are VMs where the PMU is
   usually not exposed, so it is not a CI gate.
6. **Item 99's residue is a different statistic from the one the fix was about.** The fix claims
   the heap after the read buffer no longer depends on the file's length `k`. F8's bound
   (max/min ≤ 1.10 over 65 single runs, one per `k`) folds each run's own dispersion into that
   claim: 43 of 65 points read 7.4, two read 8.2 and 8.3 (`k = 800`, `224`), not adjacent, no
   step. A single run is not what the line records (a median of 20). In 30 runs of the post-fix
   `journal` binary on the desktop line (15 with ASLR, 15 without) the case read 7.4 … 7.7; that
   says nothing about the two high points' cause and is quoted only so nobody re-runs it hoping
   it does.

### What the search found

- Mytkowicz, Diwan, Hauswirth, Sweeney, *Producing wrong data without doing anything obviously
  wrong!*, ASPLOS 2009 — link order and the size of an unused environment variable move measured
  performance, because the environment sits above the stack and shifts it
  (<https://dl.acm.org/doi/10.1145/1508244.1508275>). Fact 4's second probe is their experiment;
  it moved neither arm here.
- Curtsinger and Berger, *STABILIZER: statistically sound performance evaluation*, ASPLOS 2013 —
  "a single binary constitutes just one sample from the space of program layouts, regardless of
  the number of runs"; it re-randomises code, stack and heap at run time so layout becomes a
  Gaussian term (<https://people.cs.umass.edu/~emery/pubs/stabilizer-asplos13.pdf>,
  <https://github.com/ccurtsinger/stabilizer>). It is an LLVM-pass-plus-runtime research tool
  for C/C++; nothing in the search showed it running on current rustc output.
- iai-callgrind (now Gungraun) — Rust benches by Callgrind instruction counts, "extremely
  accurate and consistent … comparable between different systems"
  (<https://github.com/iai-callgrind/iai-callgrind>, <https://docs.rs/iai-callgrind>). It is a
  dev-dependency plus Valgrind, and runs the code under simulation.
- Weaver and McKee, *Can hardware performance counters be trusted?* (IISWC 2008) and the
  follow-up on non-determinism: retired instructions "in theory should be deterministic", with
  ASLR, environment size and errata as the causes of variation, reduced below 0.002 %
  (<https://web.eece.maine.edu/~vweaver/projects/deterministic/deterministic_counters.pdf>).
- LLVM's alignment knobs: `-align-all-functions`, `-align-all-nofallthru-blocks`,
  `-align-all-blocks`; the last "can cause lot of nops", the middle one "increases the binary
  size" (<https://easyperf.net/blog/2018/01/25/Code_alignment_options_in_llvm>); loop alignment
  lives in `MachineBlockPlacement::alignBlocks()`
  (<https://llvm.org/doxygen/MachineBlockPlacement_8cpp_source.html>). rustc is stabilising
  `-Cmin-function-alignment` and `#[align(N)]` on functions
  (<https://github.com/rust-lang/rust/pull/142824>, <https://github.com/rust-lang/rust/pull/140261>),
  which is ADR-0049's flag in a stable spelling, not a loop-level one.
- **Found nothing** on a Rust bench harness that isolates each case's timed loop in its own
  aligned function to decouple cases' layouts; decision 4 below is reasoned, not borrowed.

## Decision

### 1. The instruction count is the layout-free companion of every timed case

`scripts/bench-instructions.sh A B` runs two bench binaries `n` times each (default 3,
interleaved, pinned to one core) under `perf stat -e instructions:u,cycles:u`, and prints per
arm the min and max of each counter and one verdict:

- **`same-work`** when `|I_B − I_A| / I_A ≤ 0.1 %` and each arm's own spread is ≤ 0.01 %;
- **`work-changed`** when `|I_B − I_A| / I_A > 0.1 %`;
- **`unstable`** when an arm's own spread exceeds 0.01 % (the count is not reading one program).

`0.1 %` is chosen, not derived: it is ~3 × fact 4's between-arm difference of a harness edit that
changed no case's work, and far below the ~2–3 % of a process a +10 % move of one case would add
if it were work. The script reads `perf` as `${PERF:-perf}` and holds no `sudo` of its own
(ADR-0093's gate reads every committed `sudo`); the caller passes `PERF="sudo -n perf"` when
`kernel.perf_event_paranoid` requires it. It refuses (exit 2) when a counter is missing, zero or
`<not counted>`, or when a workload exits non-zero — the trap in
[perf-record-exits-zero-when-sudo-cannot-find-the-workload](../reference/perf-record-exits-zero-when-sudo-cannot-find-the-workload.md)
has a `perf stat` twin. Its verdict logic is tested with a stub `perf` by
`scripts/check-bench-instructions.sh`, in CI's `gates` job; the real counters are read on the
desk only.

### 2. A line that moved while the count did not is re-recorded with the cause *layout*

A timed case that reads `OVER` or `UNDER BASELINE` after a commit that did not change the code the
case times is compared, **before anything else**, by decision 1 between the binary its line was
recorded with and the current one. `same-work`, and no other case of the same binary moved the
opposite way by a comparable amount (a compensating shift would hide inside a net count), makes
**`layout`** a named cause under ADR-0095 decision 2: the line is re-recorded (median of ≥ 20,
margin from the ladder in `benches/baselines.tsv`'s header, a new `n` and date) and the commit
cites this decision with both counts. `work-changed` is not a layout move: the case goes to the
code that changed, as any regression does. For that comparison to exist, **every re-record copies
the binary it measured to `target/baseline-bins/<bench>-<sha256[0..16]>`** (gitignored, desk
only) and names that sha256 in the commit body.

This does not widen any band and does not make any red green by itself: a layout move still
costs a re-record, with a ledger entry; what it stops costing is a boot spent looking for a cause
that is not in the code.

### 3. Item 101 closes on the count and on fact 4, with the class named

Item 101 closes when the plan's step P1 reproduces fact 4 with evidence on disk (n = 5 pairs, all
cases' ns/op, both binaries' `nm -S` of the harness symbols) and decision 1 reads `same-work`
with no other `sbe` case moving the opposite way by more than its own band. The cause recorded is
**the bench binary's own layout, moved by `6b2833b`'s outlining of `Suite::figure` out of the one
function that holds every case's timed loop** — not the heap (fact 3), not the mmap threshold
(boot F), not the stack's position (fact 4, second probe). Whether the sensitive term is the loop's
code alignment or the placement of the statics beside it is **not** resolved and is not pursued:
decision 4 is what would make it stop mattering. The re-record `5576694` (159.5 → 176.5) already
stands with the cause "the item 99 fix"; this decision sharpens that cause and moves no line.

If P1 reads `work-changed` instead, item 101 does **not** close here: the harness change did add
work to a timed loop, which is a bug in `6b2833b`, and it goes to a senior developer as one.

### 4. Decoupling each case's timed loop is the named next fix, built with the next full re-record

The fix that would stop one case's layout depending on the rest of its bench file is structural:
`Suite::bench` calls an `#[inline(never)]` generic `timed::<F>(f: &mut F) -> f64` holding the
warm-up and the best-of-7 loop, so each case's loop is its own monomorphised function, its start
pinned to 64 bytes by ADR-0049's flag, its body a function of that case's closure alone. Editing
the comparator, or adding a case to a bench file, then leaves every other case's timed function
byte-identical except for call and RIP-relative displacements — which is **checkable off the desk,
statically**, by `objdump` of one case's function before and after an unrelated harness edit, and
reversible (today's harness moves it; fact 2).

It is **not built by this plan**, because building it moves every timed line of the Ryzen 7 3700X
row once, and a full re-record needs a §9 boot this session cannot have. It is built by **the
first plan that must re-record the whole row anyway** — a toolchain bump, a harness change, or
phase 4's first measurement boot (ADR-0098) — in the same boot, with this decision as the
re-record's cause. It narrows the coupling; it does not remove layout sensitivity (cross-function
effects on the branch predictor and the instruction cache remain), so decision 2 stays in force
after it.

### 5. Item 99's residue closes on the heap trace; the single-run spread is accepted and named

The claim ADR-0096 decision 2 needed is *the heap after the read buffer does not depend on the
file's length*. That is a property of the allocation sequence, not of a timing, and it is proven
by the plan's step P4: with ASLR off (`setarch x86_64 -R`), the `journal` binary built from the
merged harness returns **identical addresses** for every `malloc`/`realloc`/`posix_memalign`
call at `k ∈ {0, 16, 624, 640, 784, 800, 816, 1024}` bytes of padding, and the same binary built
at `6b2833b^` does **not** (the reversal: its read buffer is sized to the file). With that, the
1.12 of F8 is the case's single-run dispersion and not a function of `k`, and the item's residue
closes.

Accepted, with its cost stated: a single `--strict` run can read `journal put, 191 bytes, one
slot` above its ceiling (F8: 2 of 65 single runs over 8.1 ns). Such a red is re-run once; a second
red is a finding under ADR-0095 decision 4, and a line whose 20-run max/median reaches 1.10 moves
to the next margin on the ladder at its next re-record — by the header's rule, with a new `n`, not
by a hand edit now. If P4's trace differs across `k` on the merged harness, this decision is
struck and the residue stays open with the trace as its evidence.

### 6. Item 95's "which commit of PR B" is answered by the count, not by a bisect on the desk

The step lies between `6fbe851` (`wa`) and `e673e8f` (PR B's merge). The plan's step P3 builds
the default-feature `validate` bench (the four FIX 4.4 cases; every FIXT case is behind
`fix50sp2`) at `6fbe851` and at every first-parent commit of `e673e8f^2` that contains it and
changed `crates/`, plus `29be3bd` and `e673e8f`, and compares each commit with its first parent by
decision 1. The attribution rule, written before the run:

- per iteration, `ΔI = ΔI_total / 1 410 000`, summed over the four cases (all run the same number
  of calls); the time step to explain is boot E's `wa → b1` sum over the four cases:
  `(190.8 − 166.8) + (1012.9 − 950.3) + (239.5 − 220.6) + (1008.6 − 950.3)` = **163.8 ns**
  (Heartbeat, NewOrderSingle, TestRequest w2w, NewOrderSingle w2w; `target/boot-e-evidence/s1m/summary.txt`
  rows `wa` and `b1`, n = 21 each), converted to instructions as `163.8 ns × f × IPC_wa`, with
  `f` = 3.6 GHz (the 3700X's base clock; boost is not pinned, so this is an order, not a
  measurement) and `IPC_wa` = `wa`'s own `instructions:u / cycles:u`;
- a commit **carries the step** when its `work-changed` ΔI explains ≥ 50 % of those instructions;
- the step is **layout** when the whole span's ΔI explains ≤ 10 %;
- between the two, *mixed*, with both named.

Whichever verdict comes out closes the *Not proven* bullet as a number. Whether PR B's cost is
bought back is not this ADR's question; it is a plan of its own, as item 95's closing said.

## Alternatives considered

- **Widen the band of sub-200 ns cases** to hold a 10–12 % layout term. Rejected: ADR-0049
  already measured that a band wide enough (~1.20–1.25) stops guarding the encoder, and the
  ladder in `benches/baselines.tsv` forbids a margin changed without a new `n`. The count keeps
  the band honest instead of wider.
- **Stabilizer-style re-randomisation** (Curtsinger and Berger). Right in principle — one binary
  is one layout sample. Rejected: it is a C/C++ LLVM pass and runtime of 2013 with no sign of
  support for current rustc, and a published latency figure (`DESIGN.md` §8) is one binary's
  number by design; a layout-averaged figure would be a different product.
- **Loop or block alignment flags** (`-align-all-nofallthru-blocks`, `-align-all-blocks`).
  Rejected for now: one more codegen flag whose silent acceptance must be read back (ADR-0049
  decision 2), binary growth, and still a rebuild that moves every line; ADR-0049's *Open
  question* stays open. Decision 4 addresses the coupling the flags would not (a harness edit
  moving a loop's offset inside a shared function).
- **iai-callgrind / Gungraun, or `valgrind --tool=callgrind` directly.** Rejected as the
  instrument: a dev-dependency (a plan per dependency, `CLAUDE.md` §6), Valgrind on the desk and
  in CI, and 20–50× simulation on cases already run 1.41 M times each (the 82 µs FIXT case would
  run for over an hour). `perf stat` on the PMU gives the same count at native speed. Callgrind
  stays the tool for *per-function* counts if a future question needs them.
- **Chase item 101 to the exact instruction or static** (`perf annotate` of both binaries,
  alignment of the walk loop mod 32/64, `.rodata` offsets). Not chosen: it would name one term of
  one binary's layout, and the next edit makes a new one (fact 1). Decision 4 removes the
  coupling instead of naming each instance.
- **Bisect PR B by timing on the §9 desk.** Rejected: a ±10 % layout term per commit is the size
  of the step being bisected, so a timed bisect of ~11 arms would name a commit by a knob that
  moved with it (`CLAUDE.md` §10).

## Consequences

**Good**

- Three open questions close without a §9 boot, on counts anyone can re-run on the desk in
  minutes, with verdict rules written before the runs.
- A future `OVER`/`UNDER` after an unrelated commit costs a two-minute count and one re-record,
  not a boot of investigation; the ledger (ADR-0095 decision 2) gets a cause that is checked, not
  guessed.
- The count is valid on the desktop grub line, so the check does not wait for isolation.

**Bad — and accepted**

- **The count is per process, not per case.** A compensating shift (one case +X instructions,
  another −X) nets to zero; decision 2 guards it only by reading the other cases' timings. A
  per-case count needs either callgrind or decision 4's separate functions.
- **`same-work` does not mean "no slower".** A layout move is a real cost on this binary; this
  ADR decides it is not a *code* regression and records it, it does not make it free. A case that
  a layout move pushes over its line still loses its old number.
- **Layout sensitivity of the suite stays**, measured at up to ~10 % on a sub-200 ns case
  (`walk nested group + varData`) and ~×1.68 before `6b2833b` on a sub-10 ns case. Phase 4's kill
  lines (ADR-0098; 3 % for `io_uring`) are below that term, so any phase 4 A/B must be read with
  its instruction counts beside it, or inside one binary.
- **Decision 4 defers a fix that is known.** Until the next full re-record, a harness edit still
  moves every case of a bench file.
- **The 0.1 % and 0.01 % thresholds are chosen**, from one probe of one pair of binaries. A
  harness edit that adds a loop outside the timed section could exceed 0.1 % with no timed work
  changed; then the verdict reads `work-changed` falsely and costs a look, which is the safe
  direction.
- **Decision 5 accepts that `--strict` can go red once on `one slot` with nothing wrong**, about
  one run in thirty on F8's evidence, and answers it with a re-run rule.
- **`target/baseline-bins/` is desk-only provenance**, like every evidence directory here; a
  wiped `target/` loses the A arm of decision 2 and the next comparison falls back to building
  the recorded commit, which reintroduces a build-time layout term the count does not see but the
  timing does.

## Outcome

`[2026-09-23, plan rows P1–P4, desk on the desktop grub line; the tables are in
[measured-costs](../reference/measured-costs.md) *Desk-free, 2026-09-23*]` The rules above were
applied as written. None was changed after its run.

- **Decision 1 (the instrument).** `scripts/bench-instructions.sh` and its stub self-test
  `scripts/check-bench-instructions.sh` are committed (`5507238`) and run in CI's `gates` job. The
  self-test reads `pass 24 fail 0`. Its reversal (threshold 0.1 % → 1 %) went red on
  `work-changed case: verdict line`, and restoring the threshold made it green again. One bug was
  found and fixed while building it: `PERF="sudo -n perf"` had been taken as one token, and the
  self-test now has a regression case for it.
- **Decision 3 — item 101: closed, *layout*.** Five interleaved pairs: `instructions:u`
  4 408 882 961 … 4 408 883 746 (pre-`6b2833b`) against 4 407 487 868 … 4 407 559 228 (post).
  The script reads **0.031643 %, `same-work`**. `walk nested group + varData` read 152.2 … 161.0
  (pre) and 174.2 … 175.3 ns (post). No other `sbe` case moved the opposite way. `harness::suite`
  went 0x4138 → 0x2ff4 bytes, plus a 0x8f5-byte `Suite::figure`. The environment sweep with ASLR
  off read 152.9 … 160.8 (pre) and 174.2 … 181.3 (post) at every `k`. **Verdict: the binary's own
  layout, moved by `6b2833b`'s outlining of `Suite::figure`; not the heap, the mmap threshold or
  the stack.** No line moved.
- **Decision 5 — item 99's residue: closed, the decision stands.** With ASLR off, the bench
  process's own allocation sizes **and returned addresses** are byte-identical at k = 0, 16, 624,
  784, 800, 816 and 1024. At k = 640 there are nine extra allocations from a reporting path, and
  every allocation that run shares with the others has the same address. The reversal at
  `6b2833b^` requests `0x6141` (24 897 bytes) at k = 0 and `0x6548` (25 928 bytes) at k = 1024,
  and the heap after it moves by 0x400. **Verdict: the heap after the read buffer does not depend
  on the file's length, so F8's 1.12 is single-run dispersion.** Two traps cost the step an
  instrument each. `ltrace` sees nothing of a `BIND_NOW` binary. A `perf record` of `setarch -R`
  also records `setarch`'s own randomised process, which was first misread as "`perf` defeats
  `setarch -R`". Both are in
  [tracing-a-rust-binarys-allocations](../reference/tracing-a-rust-binarys-allocations-ltrace-sees-nothing-and-perf-records-the-wrapper-too.md).
- **Decision 6 — item 95's commit: named.** The target was 163.8 ns × 3.6 GHz × IPC_wa 3.134065 =
  1 848.10 instructions per iteration, with bars at 924.05 and 184.81. **Verdict: `179ab51` (*a
  group member's value is asked after the count*) carries the step: +2 409.01 instructions per
  iteration, 130.4 % of the target.** `d7be83d` takes back −875.01 (−47.4 %, opposite sign).
  `31507b5`, the merge that brought PR B's first five commits onto `wa`, adds −165.93 (−9.0 %).
  Every other commit reads `same-work`. The whole span `wa → b1` is +1 362.07, **73.7 %** of the
  target. That is work, not layout. The remaining ~26 % is not attributed, because the 3.6 GHz in
  the conversion is the base clock, not a measured frequency.
- **Decisions 2 and 4** did not run. They are rules for the next re-record and are unchanged.

## Sources

- `STATUS.md` item 101; *Start here — 2026-09-23 (boot F)*, *What the boot found* and *Not
  proven*; item 95 (closed as a merge-granularity bisect); item 99.
- `docs/reference/measured-costs.md` *Boot F, item 99* (F1/F8 sweeps) and *Boot F, `--strict`
  three times, and the case the fix moved* (`f8-sbe-ab.txt`); *Boot E* S1 (`s1m/summary.txt`).
- `git show 6b2833b -- crates/codec/benches/harness.rs` (`Suite::figure`, the fixed read
  buffer); `crates/codec/benches/harness.rs` `Suite::bench` (10 000 + 7 × 200 000 calls);
  `crates/sbe/benches/sbe.rs` (`nested_buf`, the walk case);
  `git diff 6fbe851 e673e8f -- crates/session/benches/validate.rs` (every added case under
  `#[cfg(feature = "fix50sp2")]`; `harness.rs` unchanged across the span).
- Architect probes 2026-09-23, desktop grub line: `nm -C -S --defined-only` of
  `/home/tmt/Projects/nanofixengine/target/release/deps/sbe-ce2236dc240bf0f7` (sha256
  `89bcc880…`) and `/home/tmt/Projects/fb-strict/target/release/deps/sbe-ce2236dc240bf0f7`
  (sha256 `1719ddc4…`); the `perf stat` pairs and the `setarch -R` environment sweep of fact 4;
  30 runs of `fb-strict`'s `journal-0f5dc4e01fbfa6df` (sha256 `22558546…`), fact 6.
- The web sources under *What the search found*.
