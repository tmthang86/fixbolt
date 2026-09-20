# ADR-0090 — A measurement boot is pre-built, shares one control arm, and a five-step drift is closed one named segment at a time

**Status:** Accepted — 2026-09-20, by the architect under the owner's standing delegation of
technical decisions (`STATUS.md` *Start here — 2026-09-19, night*, "uỷ quyền toàn bộ").
**Date:** 2026-09-20 · **Plan:** [docs/plans/2026-09-20-boot-d.md](../plans/2026-09-20-boot-d.md)
**Answers:** the last sentence of `STATUS.md` item 93 — *whether the band should also read a
cumulative drift is a question for the architect once the five have names* — by saying how the
names are obtained and when the baselines move; and the *needs-desk* half of items 51, 52 and
of ADR-0082's and ADR-0086's unmeasured bands: how one boot pays all of them.
**Changes nothing in:** [ADR-0023](ADR-0023-section-9-records-the-cpu-mitigations.md) (a
`mitigations=off` reading is never a figure), [ADR-0031](ADR-0031-a-baseline-is-a-band.md) (the
band, `n ≥ 20`), [ADR-0049](ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md) (the alignment
flag), [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md) (a wire
figure is a pair; an A/B is one procedure and a difference). This ADR is about *bench* A/Bs on
the §9 desk and about how item 93 is closed.

## Context

Six open items need the §9 desk and the desk is on the desktop grub line. Boot C
(2026-09-18/19) ran ~6 hours and was cut short once by its own rules (C-40); its bisect (C-91b)
found that a 9.5% engine-turn slowdown is five steps of 2–3% each, none of which tripped
ADR-0031's 10% band, and left *"five perf tasks, each with its bisect endpoints named"*. Three
unmeasured bands are stacked on top: PR A's (`Session` generic over the encoding, merged with the
band deliberately unrun — ADR-0082), PR B/C/#86's `validate` changes (ADR-0084/0085/0086), and
the nested-group descent (+2.6 µs / +8% on a laptop, unpublishable).

Three facts shape the decision, all measured on this desk:

1. **One suite run costs ~2 minutes** (`target/boot-c-evidence/c91-timeline.txt`: 6 suite runs
   per 11 m 45 s). A 20-round two-arm A/B over four suites is ~5 hours. Three such A/Bs run one
   after another would be ~13 hours and the last would never run.
2. **C-91 and C-91b ran `cargo bench` without ADR-0049's `RUSTFLAGS`** (`c91.sh`,
   `c91b-judge.sh`), while `bench.sh` exports `-C llvm-args=-align-all-functions=6`. Their
   absolute figures are therefore not the figures `bench.sh --strict` compares against, and
   cannot be the source of a re-recorded baseline.
3. **A build is ~10 minutes of load and heat between two runs.** C-85 refuted the build-slot
   candidate for one shift, but a boot that compiles between arms has a variable nobody wants
   to defend at 02:00.

The literature agrees on the shape: Kalibera & Jones (*Rigorous Benchmarking in Reasonable
Time*, ISMM 2013) — repeat at the level where the variance arises and report the difference with
its uncertainty; Mytkowicz et al. (ASPLOS 2009, via ADR-0068) — vary the setup, interleave, never
trust a tight spread inside one setup. `perf diff` (perf-diff(1)) compares two profiles by symbol
name across different binaries, which is what naming "the function that grew" between two
commits needs.

## Decision

1. **The campaign is one boot, mitigations on, and nothing in it needs `mitigations=off`.**
   Every comparison is against a figure recorded with mitigations in force (`baselines.tsv`, B7,
   boot C), so every arm runs mitigated. Item 51's `mitigations=off` arm is already measured and
   labelled A/B; ADR-0023 and ADR-0068 decision 4 already forbid publishing it as a figure, and
   this ADR adds no arm to it. The owner is told the reboot count — **one** — before the reboot;
   the return to the desktop line is the owner's, not a measurement step.
2. **A measurement boot measures and never compiles.** Every arm is a git worktree built before
   the reboot with the same `RUSTFLAGS` `bench.sh` uses (`scripts/check-bench-alignment.sh
   --flags`) and the features `bench.sh` would pass for that tree (`--features-map` where the
   tree's script knows it, none where it does not), with the alignment read back off the
   binaries in every worktree. A `cargo bench` that rebuilds during the boot invalidates that
   arm's round; the rotation driver proves no rebuild by binary sha256 before and after. Two
   feature sets of one commit are two worktrees, because cargo unifies features within one
   invocation.
3. **One shared control arm serves every A/B of the boot, in one interleaved rotation.** The
   arms run round by round, order reversed on alternate rounds, the machine-quiet row read
   before each arm, a round with any disqualified arm dropped for *all* arms so `n` stays equal.
   Each comparison pairs arms of **the same feature set**; `main` runs both without features
   (to meet the older arms) and with (to meet `bench.sh`). The rotation degrades gracefully: at
   any cut, every comparison has the same `n`; a verdict "in band" is claimed only at `n ≥ 20`
   (ADR-0031) — below that the medians are published as *provisional, n = k*, never as a band
   verdict and never as a baseline.
4. **A five-step drift is closed one named segment at a time, and the baselines move once.**
   For each bisect segment of item 93: the two endpoints are profiled in the boot (`perf
   record`, flat, ≥ 2 per endpoint, plus `perf stat` for IPC), the analysis (`perf diff -c
   delta-abs -s symbol`, then `perf annotate`) happens off the desk, and the segment gets one
   of three verdicts — **fix** (a candidate that recovers the step, built and tested *before*
   the boot as its own arm; if it wins it becomes a PR with a senior review), **accept, named**
   (the growth is a feature's stated cost — e.g. reading `52=` at every precision — and the
   name is written next to the number), or **accept, unnamed** (the profile cannot resolve it
   inside an inlined `turn`; the IPC delta is recorded instead). The `engine turn *` and
   `validate *` baselines **move exactly once**, in one commit after the last verdict, from the
   20-round medians of the with-features `main` control arm of *this* boot — not from C-91b's
   unflagged numbers, not per segment, and never before every segment has a verdict. Until that
   commit the 10% band is the only guard, and that is accepted (consequence below).
5. **What is published from a boot, and as what.** Only the with-features `main` control arm
   can become a baseline line (`n = 20`, verdict `pass 16 fail 0 unknown 0`, date). Every other
   arm — older commits, feature-less builds, A/B branches, endpoints under `perf` — is published
   only as a difference against the control, labelled with its sha, its features and its `n`.
   A machine-state A/B (a flushed ruleset, a stopped daemon) is A–B–A: the two A phases must
   agree within 2% or the arm is recorded as *not isolated*.
6. **Item 52's small case is decided by the measurement, with the rule stated first.** The
   `journal put, 191 bytes, one slot` line is re-recorded at `n = 20` on the run-time table
   (ADR-0067). If `max/median ≤ 1.35` the line joins its neighbours (ladder margin, `n = 20`) and
   the "can the ladder hold a 6 ns case" question closes with that number; if not, the line
   keeps the ladder's top (1.35) at `n = 20`, the case is listed in `DESIGN.md` §6 as *guarded by
   the ladder's top only*, and an absolute-nanosecond floor is a new ADR, not a mid-boot edit.

## Alternatives considered

- **Three sequential A/Bs, most urgent first.** Simpler driver; but ~13 hours, and the second
  and third would be cut in every realistic boot. Rejected on fact 1.
- **Re-record the baselines now from C-91b's numbers.** Available today, no desk needed; but
  they were taken without ADR-0049's flag and at `n = 3–5`, so `bench.sh --strict` would be
  compared against a binary it never measured. Rejected on fact 2.
- **Move each segment's baseline as it is named.** Five small commits instead of one; but a
  half-moved baseline set has a band that means five different things, and the ratchet
  (`check-indexing-debt.sh`'s cousin here is ADR-0031's *Under* rule) would fire on the
  unmoved ones. Rejected.
- **Add a cumulative-drift row to `bench.sh` now** (item 93's last sentence). Tempting and
  desk-free; but it decides what "drift" means before knowing whether the five steps are
  features or defects. Deferred to the architect after the names exist — the plan's *Ngoài phạm
  vi* says so.
- **Build arms during the boot to save disk.** ~13 worktrees is ~20–40 GB against 169 GB free.
  Rejected on fact 3.
- **Publish the interleaved arms as ADR-0068 pairs.** ADR-0068 is for `tools/w2w` wire figures;
  bench baselines have their own rule (`n ≥ 20`, one machine). Interleaving supplies the
  cross-time protection ADR-0068's second procedure supplies for wire figures; the two are not
  merged here.

## Consequences

**Good**

- One reboot, told in advance, pays six debts; a cut at any hour leaves equal-`n` comparisons
  and a written rule for what may be claimed at that `n`.
- The baselines, when they move, come from a binary `bench.sh --strict` will actually measure,
  on the machine and settings the line records.
- Item 93's five segments each end with a verdict and a sentence, not with a wider band.
- The rotation driver (`scripts/ab-rotation.sh`) is a committed procedure; the next boot does
  not rewrite `c91.sh` by hand.

**Bad — and accepted**

- **The boot is 10–12 hours.** Boot C was ~6. The owner loses the desk for a night, and the
  plan's cut list is the only mitigation.
- **Baselines will be re-recorded to slower numbers** for every engine-turn and validate case
  (~+7–10% and ~+3–6%). That is the ratchet accepting debt in one commit. It is written down as
  such, with the five names beside it, and it is better than a band that has been quietly
  guarding a number nobody can reproduce since 2026-09-05 — but it is a loss.
- **Until that commit, the only guard is a 10% band on a case already 9.5% over its line.** A
  further 1% regression would pass unseen for the duration of the campaign.
- **A flat `perf` profile of an inlined `Engine::turn` may name nothing.** "Accept, unnamed" is
  a legal verdict here; the honest price is that segments 1, 2, 4 and 5 may close with an IPC
  delta and no function. `perf annotate` is the last resort and it is manual, judgement work.
- **~13 worktrees and ~20–40 GB of build products live outside the repository** and are
  gitignored by location; the manifest (`../fb-boot-d/MANIFEST.txt`) is the only record of what
  sha and features each binary carries, and it must be copied into `measured-costs.md`'s
  *Settings in force* block or the numbers lose their provenance.
- **The flush arm of item 51 may never run.** It needs the owner off Tailscale for fifteen
  minutes; the default answer is no, and item 51's residue (what of the remaining 5.2 µs is
  netfilter beyond conntrack) stays open with that reason written.
- **A fix candidate is built before it is known to matter.** `ab/parse-utc-fast-path` costs a
  senior developer's time today; if the boot refutes the mechanism, that branch is deleted and
  the time is the price of measuring instead of guessing.

## Sources

- perf-diff(1) — symbol-name matching across binaries, `-c delta-abs|ratio|wdiff`, `-s symbol`
  — <https://man7.org/linux/man-pages/man1/perf-diff.1.html> (read 2026-09-20).
- T. Kalibera, R. Jones, *Rigorous Benchmarking in Reasonable Time*, ISMM 2013 —
  <https://kar.kent.ac.uk/33611/45/p63-kaliber.pdf> (read 2026-09-20): repetition at the level
  where variance arises; effect-size confidence intervals.
- Linux `kernel-parameters.txt`, entry `mitigations=`: *"off — Disable all optional CPU
  mitigations and expose users to all known vulnerabilities"* —
  <https://www.kernel.org/doc/Documentation/admin-guide/kernel-parameters.txt> (read 2026-09-20).
  How others publish it: relative, labelled on/off comparisons only (e.g. Phoronix,
  <https://www.phoronix.com/review/retbleed-benchmark>), which is ADR-0023's rule.
- Search 2026-09-20 for a method of isolating a regression across a multi-commit span beyond
  `git bisect run` with a numeric judge plus endpoint profiling and `perf diff`: nothing further
  found.
- This repository: `target/boot-c-evidence/c91-timeline.txt`, `c91.sh`, `c91b-judge.sh` (the
  durations and the missing flag); `docs/reference/measured-costs.md` *C-91*, *C-91b*, *Item
  51's last arm*; `benches/baselines.tsv` lines 264–268; `crates/session/benches/validate.rs:173-231`
  (the `fix50sp2` block); `scripts/check-bench-alignment.sh --features-map` at `main` and its
  absence at `ece17e7`/`6fbe851` (`git show`), all read 2026-09-20.
