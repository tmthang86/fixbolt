# ADR-0103 — The descent's price is exact for its symbol and bracketed for the loop that drives it; ADR-0094's reason for "≥" was wrong

- **Status**: Accepted — 2026-09-23 (by the manager under the owner's standing mandate, at P6 of the closing-phase-2 plan). Proposed 2026-09-23. Written for
  [closing-phase-2](../plans/2026-09-23-closing-phase-2.md); becomes *Accepted* at that plan's
  merge under the owner's standing mandate (2026-09-18), and one word from the owner reverses it.
  It **amends [ADR-0094](ADR-0094-the-descent-is-priced-by-attribution-inside-one-binary-not-by-a-second-binary.md)
  decision 2, condition C3**; ADR-0094 is Accepted and its substance is not edited (`CLAUDE.md`
  §5) — at acceptance its status block gains one line pointing here, as ADR-0095's did for
  ADR-0096 decision 6. Nothing new was measured: every number below is read from boot F's
  existing record.
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang. Written by the architect (Opus) from `STATUS.md` boot F *What
  the boot found* (the follow-up note on item 96) and *Not proven*, `docs/reference/measured-costs.md`
  *Boot F, Item 96 — the descent's inclusive cost*, and `target/boot-f-evidence/d96f-analysis.txt`
  (desk only; its C3 block and the three `perf report` excerpts).
- **Related**: [ADR-0086](ADR-0086-a-group-count-is-asked-at-every-depth-admin-is-two-questions-with-two-names-and-xmlnonfix-is-not-asked-the-appl-ver-id-rule.md)
  decision 1 (the descent), ADR-0094 decisions 1–3,
  [ADR-0096](ADR-0096-a-figure-measured-elsewhere-meets-the-same-band-the-baseline-file-stays-out-of-the-heap-and-a-boot-may-rebuild-on-its-housekeeping-cores.md)
  *Bad* ("item 96's number is still ≥"),
  [reference/perf-dwarf-unwind-fails-on-an-lld-linked-pie](../reference/perf-dwarf-unwind-fails-on-an-lld-linked-pie.md).
- **Answers**: what ADR-0094's "≥" means now that its stated premise is known to be false, and
  how much of the inlined `bad_group_count` loop belongs to the descent — the *Not proven*
  bullet of boot F.

## Context

ADR-0094 decision 2, C3, says: *if the outer call of `bad_nested_count` from `bad_group_count`
was inlined, the first level's own instructions are credited to `bad_group_count`, `children%`
understates, and the write-up says "≥ X ns"*. Boot F read the pinned `validate` binary (sha256
`e34e60f9…`) and found the premise false:

- `objdump -d` shows **two** call sites of `bad_nested_count::<Fixt11Fix50Sp2Tables, 256>` at
  `c9680`: `c7046` inside `validate_with::<…, 256>` (the one outer call) and `c98eb` inside
  `bad_nested_count` itself (the recursion). **No level of the descent is inlined**, so
  `children%` of the symbol covers every level.
- What *is* inlined is `bad_group_count` — into `validate_with`; there is no `bad_group_count`
  symbol. Its loop, which walks the message's groups and makes the call at `c7046`, is
  credited to `validate_with`'s **self** column.
- Read with the `--symfs` workaround, n = 3 (`d96f-analysis.txt`): `bad_nested_count` children
  8.49 / 9.68 / 8.28 %, self 2.86 / 3.31 / 2.85 %; `validate_with::<…, 256>` children 95.72 /
  95.71 / 95.70 %, **self 7.51 / 7.28 / 7.48 %**; the case's ns/op in the profiled runs 82 666.9 /
  82 704.7 / 82 884.6 (median 82 704.7). `validate_with::<…, 256>` is used by the
  `TradeCaptureReport (33 groups)` case alone (its children share equals that case's share of the
  run, 95.7 %).

The boot F write-up kept "≥ 7 021.6 ns" and moved its reason to the driving loop; the reason in
ADR-0094 was never corrected, and the question "how much of that loop is the descent's" was
left open.

### What the search found

- `perf report` charges an inlined function's samples to the function it was inlined into —
  "all Self overhead from the inlined function remains attributed to its caller" — and only an
  `--inline` / `srcline` read with debug line tables splits them
  (<https://www.kdab.com/improved-handling-inlined-frames-linux-perf-report/>,
  <https://man7.org/linux/man-pages/man1/perf-report.1.html>). That is the mechanism C3 assumed
  and it is not in dispute; what was wrong was *which* function got inlined.
- Nothing else external bears on this: it is a question of what this repository's ADR defined
  and what this binary's symbol table shows.

## Decision

### 1. C3's premise is recorded as false for the binary that was measured

For `e34e60f9…`, the descent's first level is a real call (`c7046 → c9680`); ADR-0094's C3
condition ("the first level may be inlined") **did not occur**. C3 remains as a rule for a
future binary where it does occur; the check is the same `objdump` of call sites to the symbol,
now named as C3's pre-read beside C1's `nm`.

### 2. The price, as ADR-0094 decision 1 defined it, is exact, not a lower bound

ADR-0094 decision 1 defines the price as *`bad_nested_count`'s inclusive share × the case
median*. With every level a call, that quantity is measured whole: **7 021.6 ns** (8.49 % ×
82 704.7, n = 3, range 6 844.8 … 8 023.2 ns). It is written without "≥" from now on, still with
ADR-0094 §3's qualifier — cycles *inside* the descent, not cycles a message would save without it.

### 3. The descent together with the loop that drives it is bracketed, not resolved

The part of the inlined `bad_group_count` loop that exists to enter the descent is inside
`validate_with`'s self share, and cannot exceed it. So, from the same three profiles, with no new
run:

| k | lower: `bad_nested_count` children | upper: + `validate_with` self | upper × 82 704.7 ns |
|---|---|---|---|
| 1 | 8.49 % | 16.00 % | 13 232.8 |
| 2 | 9.68 % | 16.96 % | 14 026.7 |
| 3 | 8.28 % | 15.76 % | 13 034.3 |

**The descent including its driver costs between 7 021.6 and 13 232.8 ns** of the
`TradeCaptureReport (33 groups)` case (median shares × median ns/op; 8.5 % … 16.0 % of the case).
The upper bound is loose by design: `validate_with`'s self column holds all of its own work, not
only the loop. It is **not tightened**: splitting `validate_with`'s self samples by address
(`perf annotate` of the inlined loop's basic blocks) is judgement work on one binary's layout,
and no decision in this repository waits on the split — whether PR B's validation cost is bought
back is its own plan (item 95's closing), and that plan would profile its own arms.

### 4. What changes in the record

- `measured-costs.md` *Boot F, Item 96* gains a dated line: exact 7 021.6 ns for the symbol,
  bracket [7 021.6, 13 232.8] ns with the driver, this ADR cited.
- `STATUS.md` *Not proven*, item 96's bullet is struck with this ADR as the reason (the
  manager's edit, in the plan's closing step).
- ADR-0094's status block gains: "C3 amended by ADR-0103 — the premise did not occur in the
  measured binary; the price is exact for its symbol, and the descent with its driving loop is
  bracketed."

## Alternatives considered

- **Edit ADR-0094's C3 text.** Forbidden for an Accepted ADR (`CLAUDE.md` §5).
- **Keep "≥" with the new reason and leave the bracket open.** Rejected: "≥" then describes a
  quantity ADR-0094 never defined (the descent plus its driver), and an open lower bound with no
  upper one is not a number.
- **Tighten the upper bound with `perf annotate`** of `validate_with` on the existing `.data`
  files. Possible off the desk (the files exist; the `--symfs` workaround is recorded). Not
  chosen: no decision needs it, and it is manual attribution by address, the kind of read ADR-0094
  decision 4 already called "a poor verdict" when it is the only one.
- **A `#[inline(never)]` on `bad_group_count` and a new profile.** A code change that moves layout
  and needs a §9 boot; rejected by ADR-0094 decision 4 for the same reason.

## Consequences

**Good**

- ADR-0094's record is correct: its premise is marked false for the measured binary, its number
  is exact for what it defined, and the wider question has a bracket.
- No run, no desk, no build: the item's last bullet closes on data already on disk.

**Bad — and accepted**

- **The bracket is wide** — a factor of 1.9 between its ends. Anyone who needs "what the message
  would save without the descent" still has no number; this ADR says so rather than narrowing it
  by judgement.
- **n = 3 profiles, one binary, one message shape**, unchanged from ADR-0094 §3 and boot F.
- **The evidence file is desk-only** (`target/boot-f-evidence/d96f-analysis.txt`); the bracket's
  inputs are quoted here so the record does not depend on it.

## Sources

- `docs/reference/measured-costs.md` *Boot F, Item 96 — the descent's inclusive cost*.
- `target/boot-f-evidence/d96f-analysis.txt` (desk only): the three `perf report --children`
  excerpts (`validate_with` self 7.51 / 7.28 / 7.48 %), the C2 block, the C3 block with the two
  call sites `c7046` and `c98eb`.
- `STATUS.md` *Start here — 2026-09-23 (boot F)*, *What the boot found* (the note for the
  architect) and *Not proven*.
- ADR-0094 decisions 1–4, *Consequences*.
