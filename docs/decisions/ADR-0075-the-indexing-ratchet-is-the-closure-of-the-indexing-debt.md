# ADR-0075 — The indexing ratchet is the closure of the indexing debt

- **Status**: **Accepted — 2026-09-18**, by the plan [closing-the-open-items](../plans/2026-09-18-closing-the-open-items.md), approved by the manager under the owner's 2026-09-18 mandate. Proposed the same day.
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: `CLAUDE.md` §2 non-negotiable 7 and *Machine checks* row 7;
  `scripts/check-indexing-debt.sh`; [ADR-0061](ADR-0061-the-scratch-fixture-gate-is-a-regex-and-says-so.md);
  [plans/2026-09-13-the-residue-of-an-obligation.md](../plans/2026-09-13-the-residue-of-an-obligation.md)
  §G (owner's Q5); `STATUS.md` open item 55

## Context

`clippy::indexing_slicing` is `deny` at the workspace level, and 184 sites in `crates/*/src`
carry it under a ceiling that `scripts/check-indexing-debt.sh` holds in both directions — up is
a regression, down is a ceiling somebody forgot to lower. The floor is not zero: `crc32`'s
masked table lookup and a `const fn` table both index by construction and carry a scoped
`#[allow]` naming the proof. The owner decided on 2026-09-13 (Q5) to keep the item as a
standing ratchet rather than pay it down, because the two files holding 74 of the 184 sites
(`crates/session/src/lib.rs` 42, `crates/codec/src/template.rs` 32) are the hot path, and
every edit there is a non-negotiable 1 and 3 change with a Criterion band to re-prove on the
§9 machine.

An open item that is kept open on purpose, with a gate, is a decision wearing a to-do's
clothes. This ADR gives it the right clothes.

## Decision

1. **Item 55 is closed by this ADR.** The ratchet **is** the guard for the half of
   non-negotiable 7 no lint could see; `CLAUDE.md` §2's machine-check row 7 already names it.
   The ceiling is a number in a script and is the only place it lives.
2. **The debt is paid file by file, only when a file is open for another reason.** A step that
   edits `crates/session/src/lib.rs` or `crates/codec/src/template.rs` for its own purpose
   lowers the ceiling by the sites it removes and quotes the script's new count; no plan is
   opened to pay the debt alone. A step's brief that touches a listed file says so.
3. **The floor is recorded**: the `crc32` lookup and the `const fn` table stay, each with its
   scoped `#[allow]` and proof comment, and the script's ceiling can never read below their
   count. If a third site of that kind is needed it is named in the same way in the same commit.
4. **`tests/`, `benches/` and `tools/` stay out of scope**, as today.

## Consequences

**Good**

- The open table stops carrying a row whose only next action is *don't*.
- The rule for paying the debt is one that costs no extra desk time: hot-path files get their
  Criterion re-proof when they are edited anyway.

**Bad — and accepted**

- **184 panicking sites remain in library code**, under a ceiling, and this ADR makes that the
  accepted state rather than a queue. A `[i..j]` that panics on a malformed frame is exactly
  the class of defect four `dispatch` tests caught on 2026-09-06; the ratchet only stops the
  count growing.
- **"When the file is open for another reason" may be never** for a stable file, and the
  count can sit at 184 indefinitely. That is the trade the owner chose.
- **A future reviewer may read the ceiling as a target of 0** and open the plan this ADR
  declines; the script's header should point here.

## Sources

- `scripts/check-indexing-debt.sh` (ceiling 181 at the time of the owner's Q5; read the
  script for today's number — the number is not repeated here on purpose).
- `STATUS.md` item 55 `[measured 2026-09-08]`, `[2026-09-13]`.
