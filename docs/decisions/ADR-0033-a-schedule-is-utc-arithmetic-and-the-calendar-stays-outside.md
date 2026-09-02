# ADR-0033 — A schedule is UTC arithmetic, and the calendar stays outside

- **Status:** Accepted
- **Date:** 2026-09-02
- **Related:** [ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md) (a reconnect is not a
  restart), D1 (the session layer is pure), D13 (the epoch is 0000-01-01)
- **Plan:** [2026-09-02-session-schedules.md](../plans/2026-09-02-session-schedules.md)
- **Answers:** `PRD.md` §2 *Session schedules*, a Phase-1 gap named three times and never
  planned

## Context

A FIX session does not run forever. It opens at 08:00, closes at 17:00, and the next morning
**both ends begin again at `34=1`**. That is protocol, not operations: the two sides must
*agree* on when the counting restarts, and an end that gets it wrong spends the next morning
in a sequence-number dispute with its counterparty.

`[verified 2026-09-02]` this engine had no notion of it. `Session::new` resets and
`Session::resume` does not ([ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md)); *which to
choose* was the embedder's problem and nothing told them **when**. `GUIDE.md` §9 said so
plainly: *"It has no session schedule."*

The hard part is not the window. It is the **calendar**. A venue says *"17:00
America/New_York"*, and resolving that needs an IANA database: a dependency, which allocates,
in the layer non-negotiable 2 calls **pure** (D1).

### What the prior art does

| | |
|---|---|
| **QuickFIX** (C++/J/n/go) | Schedule *inside* the engine. `StartTime`, `EndTime`, `StartDay`, `EndDay`, `Weekdays`, `TimeZone`, `NonStopSession`. Each endpoint carries its own `TimeZone`, and `[read 2026-09-02]` `DefaultSessionSchedule.java` **does not address DST transitions explicitly**. Reset is decided by `isSameSession(t1, t2)` |
| **Artio** | **No schedule in core.** `SessionScheduler` lives in `artio-samples/` — an example, not a component. The engine exposes `resetSequenceNumber()` and leaves *when* to the embedder |

So the two references disagree about where the line goes, and the one that pulled the
calendar in admits it does not handle the hard case.

## Decision

### 1. The session shape is in; the Gregorian calendar is out

`Schedule` is **arithmetic on the millisecond timeline, expressed in UTC**. No zone name, no
database, no daylight saving. `daily`, `weekly`, `always`, an optional weekday mask, and a
fixed UTC offset.

A caller wanting local time resolves it with their own zone library and **rebuilds the
`Schedule` when the offset changes**. `Schedule::with_utc_offset_ms` covers the fixed-offset
case and its rustdoc says at length that it is **not** DST support.

This lands between the two prior arts, and the reason is the invariant rather than taste:
`same_session` is a protocol rule and needs tests, so it belongs here; *which zone* is
deployment data, and importing a database to hold it would break D1 for a convenience.

### 2. Reset is decided by comparison, never by an alarm

`Schedule::same_session(a, b)` — *do these two instants fall in the same interval* — which is
how QuickFIX decides it too.

An engine that slept through midnight gets no alarm. A process that starts at 06:00 gets no
alarm. Each has exactly two facts: the last instant it remembers, and now. **The moment a
reset matters most is the moment nobody was running to hear a bell**, so a mechanism that
depends on being awake at 00:00 is a mechanism that fails when it counts.

### 3. `always()` is the default and is exactly neutral

Every session built before this existed behaves as if it carries `Schedule::always()`: one
session, no boundary, no reset. `[measured 2026-09-02]` making `same_session` return `true`
unconditionally turns five schedule tests red and leaves **59/59 green** — which is the
proof, because it shows the acceptance corpus cannot see a schedule at all and therefore
cannot have been perturbed by one.

### 4. A reset cannot be decided from the numbers alone

`Session::resume(cfg, out, in)` carries the numbers and asserts nothing about the calendar,
so it **never** resets on a boundary. `Session::resume_at(cfg, out, in, last_active_ms)`
carries the numbers *and when they were last touched*, and only that one can compare.

This is not a convenience split. Knowing `next_out = 41` says nothing about whether a
boundary has passed since 41 was reached; without the instant, any answer is a guess. So the
engine must persist the instant beside the numbers, and until it does (step 4 of the plan,
unbuilt) **a restart across a boundary still gets it wrong**.

### 5. An instant inside no interval is never the same session as anything

Including another such instant. An engine that cannot place what it remembers therefore
**resets**.

The direction is chosen on asymmetric cost: resetting when the counterparty did not is a
`Logon` argument, visible immediately. *Failing* to reset when they did is a silent
divergence that surfaces as mis-numbered messages later.

### 6. The closing `Logout` carries no `58=`

FIX makes the text optional; QuickFIX sends none here either; and there is no session-level
text for *"we are closed"* that would not be invented. The counterparty therefore learns
nothing about **why** — recorded as a consequence, and it is `STATUS.md` item 30 (d)'s job,
not this one's.

## Consequences

**Good**

- The gap `PRD.md` named three times is closed for the session layer: a venue's hours, a
  weekday filter, a week-long window, and a session that runs past midnight all work.
- Nothing was added to the dependency list, and `benches/alloc.rs` gains `schedule-open` and
  `schedule-shut`, both **0**.
- `Schedule` is `Copy` and sits in `Config`, so the counterparty registry gives
  **per-counterparty schedules for free** — the registry already returns a `Config`.
- The reset rule is testable without a clock, a socket or a sleep, because it is a comparison
  of two numbers.

**Bad, and named**

- **A fixed offset is wrong for half the year in any zone that observes DST.** A `Schedule`
  built from `America/New_York`'s winter offset resets at the wrong hour all summer, and the
  wrong hour is the one a counterparty is least forgiving about. The type cannot detect this;
  only `GUIDE.md` warns.
- **A restart across a boundary is still wrong**, because nothing persists `last_active_ms`
  yet. `Session::resume` — what the engine calls today — never resets. Step 4 of the plan.
- **The corpus cannot see any of this.** All 59 definitions run inside one interval, so the
  primary gate is blind to every rule here; `crates/session/tests/schedule.rs` is the only
  thing holding them.
- **No `ResetSeqTime`.** QuickFIX allows the reset to happen at an hour other than the close;
  here the reset is tied to the interval boundary. Addable later, not addressed.
- **The weekday constant rests on one unit test.** `[measured 2026-09-02]` changing `+ 5` to
  `+ 6` in `Weekday::from_days_since_year_zero` leaves every weekday case in
  `tests/schedule.rs` green, because those tests *find* a Monday by probing seven days rather
  than naming one — deliberately, so they do not depend on which day the corpus falls on. The
  price of that independence is that they cannot see the constant.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| An IANA database in `session` (QuickFIX's shape) | A dependency that allocates, in the layer non-negotiable 2 calls pure. And the reference implementation that took this route does not handle DST explicitly, so the cost buys less than it looks |
| No schedule at all, `reset_sequence_numbers()` as API (Artio's shape) | `same_session` is a protocol rule two ends must agree on. Leaving it entirely to embedders means every embedder reimplements a rule the corpus cannot check |
| An alarm at the boundary | Misses every case where the process was not running at the boundary, which is when a reset matters most |
| `Schedule` in `engine` rather than `session` | The reset changes `next_out`/`next_in`, which are the session's. Splitting the decision from the state is how the two disagree |
| Store a *count* of boundaries crossed | Needs something to have been awake for each. The comparison needs only two instants |
