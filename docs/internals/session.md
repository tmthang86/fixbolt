# `session` — internals

Layer L2 in [DESIGN.md §3](../DESIGN.md#3-crates): the FIX session state machine. Pure — no
socket, no clock, no allocation, no `format!` (CLAUDE.md §2 non-negotiable 2, D1). `Role` is
a type parameter; time arrives as `Input::Tick`.

## Files, and what each keeps

| File | Keeps |
|---|---|
| `lib.rs` | `Session<Role>`, `Input`, the state machine itself, and the `Application` trait a caller implements |
| `clock.rs` | Reading a FIX `UTCTimestamp`, and the milliseconds-since-`0000-01-01` epoch the session counts from (D13) |
| `journal.rs` | The `Journal` trait: two questions the session asks (`keep these bytes at seq n`, `do you still have seq n`) and holds nothing itself |
| `schedule.rs` | `Schedule` — when a session is open and when both ends restart at `34=1`, as UTC arithmetic only ([ADR-0033](../decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)) |
| `out.rs` | Every outbound message this layer builds, each a `Template` sorted by the generated tables — non-negotiable 5, field order never decided at a call site |
| `text.rs` | The exact `58=Text` strings and `373=` codes the 59 acceptance definitions expect, byte for byte |

## Read in this order

1. `lib.rs` — the state machine and `Input`/`Application`, before anything that supports it
2. `clock.rs` — how `Input::Tick` is interpreted
3. `journal.rs` — the seam to persistence, since `engine` supplies the real implementation
4. `schedule.rs` — session-open/session-restart arithmetic
5. `out.rs` — what the machine sends and how it is ordered
6. `text.rs` — the exact wording sent, last, since it is mechanical once the logic above is
   understood

## Tests that guard it

- `tests/score.rs` — the primary gate: the 59 QuickFIX acceptance definitions, run against
  this pure machine (CLAUDE.md §2 non-negotiable 3)
- `tests/logon.rs`, `tests/heartbeat.rs`, `tests/resend.rs`, `tests/sequence_admin.rs`,
  `tests/numbering.rs`, `tests/skew.rs`, `tests/reject.rs`, `tests/goodbye.rs` — one area of
  session behaviour each
- `tests/schedule.rs` — `schedule.rs`
- `tests/text.rs` — `text.rs` against the corpus's own wording
- `tests/mirror.rs`, `tests/initiator.rs` — the mirrored (initiator) corpus,
  [ADR-0076](../decisions/ADR-0076-the-mirrored-ceiling-is-a-classified-count-not-an-estimate.md)
- `benches/alloc.rs` — non-negotiable 1, the session's own hot path
