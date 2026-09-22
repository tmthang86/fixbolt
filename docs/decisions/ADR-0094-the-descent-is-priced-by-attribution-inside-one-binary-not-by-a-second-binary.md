# ADR-0094 — The ADR-0086 descent is priced by sample attribution inside one binary, not by a second binary

- **Status**: Accepted — 2026-09-22, at the merge of the plan's branch, **under the owner's
  approval of the plan
  [the-detector-and-the-campaign-preconditions](../plans/2026-09-22-the-detector-and-the-campaign-preconditions.md)
  on 2026-09-22** (`bb71859`, one word), whose *Cách làm* states this design in substance and
  links this file. That is the whole basis: **no delegation was granted for this plan
  specifically**, and the owner has not read this text. The general delegation of 2026-08-30
  (`STATUS.md`, *On the delegation itself*) is the precedent ADR-0088, ADR-0090 and ADR-0091
  stand on and is cited here only as that. One word from the owner reverses this status.
  **Nothing here has run**: it designs an experiment for a **future §9 boot**, that plan does
  not run it, and acceptance is of the design, not of a number. Not revised before acceptance.
- **Date**: 2026-09-22
- **Deciders**: Tran Manh Thang. Written by the architect (Fable) from `STATUS.md` item 96,
  ADR-0086's dated note and the layout reference page; the code facts are read from
  `crates/session/src/lib.rs` and `Cargo.toml` at `0629111`.
- **Related**:
  [ADR-0086](ADR-0086-a-group-count-is-asked-at-every-depth-admin-is-two-questions-with-two-names-and-xmlnonfix-is-not-asked-the-appl-ver-id-rule.md)
  decision 1 (the descent, `bad_nested_count`, recursive) and its *Bad* consequence with the
  2026-09-22 note (the cost is measured and not attributable);
  [ADR-0049](ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md)
  (the alignment flag narrows the layout term to ~4% and does not close it);
  [ADR-0090](ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  decision 4 (`perf record` of endpoints is already the boot's tool) and decision 5 (what a
  boot may publish);
  [reference/a-benchmark-that-measures-where-the-compiler-put-it](../reference/a-benchmark-that-measures-where-the-compiler-put-it.md).
  `STATUS.md` open item 96 and *Not proven*; `CLAUDE.md` §2 non-negotiable 10.
- **Answers**: what experiment prices the descent without building a second binary, what it
  can see, what it cannot, and what it retires.

## Context

1. **The two-binary experiment measured the descent plus the layout.** `[measured
   2026-09-22, boot D step D4]` `w1s` (`main` `76e53cb`, `fix50sp2`) → `w3` (`d6f79dc`,
   `ab/validate-no-descent`), n = 20: `validate TradeCaptureReport (33 groups)` 82 071.1 →
   73 182.8 ns, **−10.83%**. The design required four group-free control cases to agree
   within 2%; **three moved 2.9–5.8%** (`validate Heartbeat` +5.78%, `validate TestRequest,
   w2w bytes` +4.35%, `validate NewOrderSingle, w2w bytes` +2.86%). Both arms were built with
   ADR-0049's flag, so this is the residue the flag does not close. The magnitude is known;
   the attribution is not (`STATUS.md` item 96).
2. **The descent has a symbol.** `bad_nested_count<'a, D: Tables, const N: usize>`
   (`crates/session/src/lib.rs:4437`) recurses on itself with `depth + 1` (ADR-0086 decision
   1). A recursive function keeps a symbol of its own however aggressively the outer call is
   inlined into `bad_group_count` (`:4376`): the recursive call site needs an address. It
   carries no `#[inline]` attribute. `Cargo.toml` has **no `[profile]` section**
   `[measured 2026-09-22]`, so bench binaries use cargo's defaults — `strip = "none"`, symbols
   present, no debuginfo — and `perf report` can name the function; `perf annotate` sees
   assembly only.
3. **The instrument exists.** `crates/session/benches/validate.rs:226` — `validate
   TradeCaptureReport (33 groups)` under `fix50sp2`, 10 000 warm-up calls then 7 × 200 000
   timed calls of the same `validate` on the same pre-parsed view (`harness.rs:305-318`).
   Five million calls of one function is a profile with no sampling problem. The same binary
   runs four group-free cases (`validate NewOrderSingle`, `Heartbeat`, `TestRequest, w2w
   bytes`, `NewOrderSingle, w2w bytes`) that never enter the descent.
4. **`ab/validate-no-descent` is deliberately broken** (`STATUS.md` 2026-09-22 *Do not*): it
   is an instrument, not a candidate, and it has now shown it cannot isolate what it was built
   to isolate on this desk.
5. **Root is needed for `perf` on the desk** (`perf_event_paranoid = 4`), so the run goes
   through `sudo` — and through
   [ADR-0093](ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md)
   decision 2: the workload after `--` is the manifest-pinned binary by absolute path.

### What the search found

`[searched 2026-09-22]` — nothing was fetched for this ADR beyond what ADR-0092 and ADR-0093
fetched. The method — attribute sampled cycles to a symbol and multiply by the measured time
per operation — is the ordinary use of `perf report`'s per-symbol overhead, and `--children`
versus `--no-children` (inclusive of callees or self only) is documented in `perf-report(1)`.
The architect did **not** re-fetch that page from this container; the step that runs the
experiment reads `perf report --help` on the desk and quotes the two lines, because the
inclusive/self distinction is the whole measurement. **Not found**: any published cost for a
recursive group-count check in a FIX engine (ADR-0086 said the same).

## Decision

### 1. The price is the descent's inclusive share of one binary's samples, times that binary's median

On the §9 desk, `check-machine.sh` clean, the **manifest-pinned `main` `validate` binary with
`fix50sp2`** (one binary; the arm ADR-0090 decision 5 allows to become a baseline):

```sh
sudo -n perf record -e cycles -F 4999 -o "$EVIDENCE/d96-<k>.data" -- "$VALIDATE_BIN"
```

for k = 1…5 at least, `$VALIDATE_BIN` an absolute path checked against the manifest's sha256
before each run (ADR-0093 decision 1). The bench binary prints its usual rows; the case
median over the five runs is the second factor. Then, off the desk:

```sh
perf report -i d96-<k>.data --children  -s sym --percent-limit 0 | grep bad_nested_count
perf report -i d96-<k>.data --no-children -s sym --percent-limit 0 | grep bad_nested_count
```

**The price of the descent for this message is `children% × (case median)`**, in ns, quoted
with its `self%` beside it, its n, its machine line and the two `perf report` lines
verbatim. There is **one binary**, so there is **no layout term**: every byte the timed loop
executes is where it was when 82 071.1 ns was measured.

### 2. Two conditions must hold before the number is read, and a third bounds it

- **C1 — the symbol is there.** `nm -C "$VALIDATE_BIN" | grep bad_nested_count` shows at
  least one symbol *before* the boot (it is a pre-boot check on the built arm, the way the
  alignment read-back is). If it does not — the compiler turned the recursion into a loop,
  or the monomorphised copy was folded into `bad_group_count` entirely — the experiment does
  not run and this ADR's decision 4 applies.
- **C2 — the share is bounded by the case's own share.** The profile is per process, not
  per case, and the four group-free cases run in the same process; they never enter the
  descent, so every sample credited to `bad_nested_count` must come from the group case's
  share of the run. That share is computable from the same output — the group case's median
  over the sum of the six medians, each case running the same number of calls — and
  `bad_nested_count`'s inclusive share **cannot exceed it**. A larger share means the symbol
  is being credited with time from elsewhere (a mis-resolved frame, a shared callee) and the
  number is not read. A second binary with the group case removed would make the control
  explicit and is not used, for the reason this ADR exists.
- **C3 — the first level may be inlined, and then the number is a lower bound.** If the
  outer call of `bad_nested_count` from `bad_group_count` was inlined, the first level's own
  instructions are credited to `bad_group_count` and only the recursive levels to the
  symbol; `children%` then **understates**. The write-up says *"≥ X ns"* in that case, and
  the sign of the missing part is known.

### 3. What it cannot see, written next to the number

- **Second-order cost on the caller**: what the descent's memory traffic does to the cache
  and branch state of the code that runs *after* it. The two-binary experiment could not see
  this either — it is inside the −10.83% together with the layout term, and attribution
  separates neither. The number this ADR produces is *"cycles spent inside the descent"*,
  not *"cycles the message would save without it"*; the two coincide only if the second-order
  term is small, and that is stated, not assumed.
- **One message shape.** `TradeCaptureReport (33 groups)` is the case that exists. The
  descent's cost is per nested counter per entry; a message with more or deeper nesting is
  not priced by this number and `MAX_GROUP_NESTING = 8` bounds it only in depth.
- **A product of two measurements.** A share and a median, each with its own dispersion;
  the write-up carries `min/med` and `max/med` of both over the five runs, and the price is
  a range, not a point.

### 4. `ab/validate-no-descent` is retired as an instrument; a matched-layout second binary is not attempted

The branch is deleted after the number is recorded (it is broken by design and has had its
one use). If C1 fails, the fallback is **not** a second binary: it is `perf annotate` of
`bad_group_count` on the same profile, reading the inlined block by its assembly — manual,
judgement work, and reported as ADR-0090 decision 4's *"accept, unnamed"* if it cannot be
resolved. A bench-only `#[inline(never)]` on `bad_nested_count` is a code change that moves
layout and would need its own A/B; it is named here as the last resort and is not chosen.

## Alternatives considered

- **A second binary with the descent removed, layout matched harder.** ADR-0049's flag is
  already in force on both arms and the controls still moved 2.9–5.8%; the reference page
  measured 11–24% of a case as layout before the flag and ~4% after. There is no known flag
  that closes the residue, and "try more flags until the controls agree" is a cause accepted
  because a knob moved with it (`CLAUDE.md` §10). Rejected.
- **A run-time switch in the session layer** (`Limits` field, env var) so one binary runs both
  paths. Rejected: a branch on the hot path in production for a benchmark's sake, in a layer
  whose rule is *pure, no allocation, no clock* (`CLAUDE.md` §2 item 2) and whose every field
  is a user-visible constant (`docs/CONFIGURATION.md`). The descent is correct behaviour
  (ADR-0086); a switch to turn correctness off is not a feature.
- **A data arm** — the same binary validating a `TradeCaptureReport` whose nested counters
  are removed, so the descent is entered and exits at once. Kept as a **control**, not as the
  price: it measures a different message (fewer bytes, fewer fields), so its difference from
  the 33-group case is not the descent alone. The step may add such a case to `validate.rs`
  as a bench-only change if the architect and the plan that runs the boot want the extra
  view; it does not replace decision 1.
- **`perf stat` counters** (instructions, cycles) over the whole case. Rejected: no symbol
  resolution — it prices the case, which is already priced.
- **uprobes on `bad_nested_count` entry/return.** Rejected: the probe overhead on a ~µs
  function called millions of times is the measurement.

## Consequences

**Good**

- The descent gets a number with **no layout term**, on the binary whose figure it is a
  share of, with the two `perf report` lines as the artefact — a sentence with something
  behind it (`STATUS.md` 2026-09-22, *What this pull request paid to learn about itself*).
- Nothing is built for the boot beyond what already exists; the pre-boot cost is one `nm`.
- The retired branch stops being a temptation to "just re-run it".

**Bad — and accepted**

- **The number is a share times a median, and a lower bound if the first level is inlined.**
  It is less than the −10.83% would have been if that were trustworthy, and it answers a
  narrower question — *inside the descent* — than *without the descent*. The narrower
  question is the one that can be answered honestly.
- **Second-order effects are unmeasured and stay so.** No experiment in this ADR sees them;
  the write-up says so beside the number.
- **It needs `perf` as root on the desk**, through ADR-0093's gate, and off-desk analysis of
  files that live only in `target/boot-d-evidence/`-shaped directories — the same
  provenance debt ADR-0090 accepted.
- **If C1 fails the fallback is manual** (`perf annotate` on assembly with no source lines),
  and may end *unnamed*. That is a legal verdict under ADR-0090 decision 4 and a poor one.

## Sources

- `STATUS.md` open item 96; ADR-0086 *Bad — and accepted*, the `[measured 2026-09-22]` note;
  `docs/reference/measured-costs.md` *Boot D … The ADR-0086 descent band*.
- `crates/session/src/lib.rs` at `0629111`: `:4340-4356` (`MAX_GROUP_NESTING`), `:4376`
  (`bad_group_count`), `:4431-4437` (`bad_nested_count`, its rustdoc and signature).
  `crates/session/benches/validate.rs:104-226` (the six case names).
  `crates/codec/benches/harness.rs:305-318` (the timing loop).
- `Cargo.toml` at `0629111`: no `[profile.*]` section (`grep -n '^\[profile'` → nothing).
- ADR-0049; `docs/reference/a-benchmark-that-measures-where-the-compiler-put-it.md`.
