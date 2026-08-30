# ADR-0010 — A reconnect and a restart are different events, and the code cannot tell them apart

- **Status**: Proposed — 2026-08-30
- **Date**: 2026-08-30
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0008](ADR-0008-journal-is-a-trait.md),
  [DESIGN.md §4 D1 and D7](../DESIGN.md),
  [plans/2026-08-30-session-recovery.md](../plans/2026-08-30-session-recovery.md)

## Context

`STATUS.md` open item 16 said a journal was written and never read back. That half is now
fixed: `FileJournal::open` reads the file, `Journal::highest` says what it holds, and a torn
tail is dropped rather than half-read. A journal can now answer *what did this end send*.

**Nothing uses the answer**, and the reason is a question this project has never decided.

`Session::connect` resets both counters to 1, unconditionally:

```rust
self.next_out = 1;
self.next_in = 1;
```

Its comment names the file that requires it. That file is real, and so are two more:

`[measured 2026-08-30]` across the 59 acceptance definitions, **three files reconnect** —
`2i_BeginStringValueUnexpected` (2 connects), `2k_CompIDDoesNotMatchProfile` (3) and
`2o_SendingTimeValueOutOfRange` (2). **Every one of those seven connects expects `34=1` back**,
and **not one of their Logons carries `141=Y`**. Only one file in the whole corpus mentions
`141=` at all.

So the corpus is unambiguous: in its world, every `iCONNECT` starts the numbering again.

### Why the corpus cannot settle this

**The corpus is the harness, not the protocol.** FIX 4.4 numbers a *session*, not a
*connection*: a counterparty that drops and redials continues where it left off, and
`ResetSeqNumFlag(141=Y)` on a Logon is the explicit way to say otherwise. QuickFIX itself
persists sequence numbers across a reconnect in a deployment and resets them at a configured
time of day. Its acceptance harness starts each `iCONNECT` from a clean store, which is why
these files read the way they do — and that is a property of how the tests are run, not a
statement about what an acceptor owes a counterparty.

**The result is that the corpus and a real deployment demand opposite behaviour, and this
code cannot tell which it is in**, because `connect` is the only entry point and it is called
in both cases.

## Decision

**1. A new connection and a new session become different things.**

- `Session::resume(cfg, next_out, next_in)` constructs a session already carrying numbers,
  intended to be called once at startup from what the journal reports.
- `connect` stops resetting unconditionally. It resets when the session is genuinely new, and
  keeps the counters when it is resuming.

**2. `141=Y` becomes the counterparty's way to force a reset**, which is what FIX says it is,
and the only thing that resets a resumed session's numbers from the wire.

**3. The acceptance gate keeps its meaning by construction, not by exception.** The runner
builds a session per scenario; within a scenario a second `iCONNECT` is a second connection to
a session that never persisted anything, so it resets exactly as the corpus expects. No file
needs an exemption and none is granted.

## Consequences

**Good**

- A restart resumes rather than restarting the count, which is what a counterparty and an
  audit both assume — and what `Durability::Fsync` has been paying for since it shipped.
- `141=Y` starts meaning what the specification says instead of being unimplemented.
- The distinction is made explicit in the API, so a caller cannot get it by accident.

**Bad, and stated now rather than discovered later**

- **`connect` gains a condition, and non-negotiable 3 is the gate that finds out.** A session
  change that does not run 59/59 is not done, and this one touches the branch three files
  exercise directly.
- **Nothing in the corpus tests resumption**, so the new path is guarded by hand-written tests
  only — the same weakness `tests/reject.rs` and `tests/resend.rs` already carry, and it is
  worse here because sequence numbers are what a counterparty reconciles on.
- **This ADR does not decide when a session ends.** Real deployments reset at a configured time
  of day. Without that, a resumed session's numbers grow for ever and the journal with them.
- **`next_in` is not addressed.** Recovering what this end *sent* is the easy half; recovering
  what it *accepted* needs a durable write on the receive path, ordered before the application
  sees the message, or a restart replays work already done.

## Open questions

1. **When does a session end?** A time-of-day reset is what QuickFIX does. Nothing here has it,
   and without it "resume" has no opposite.
2. **What happens when the journal and the counterparty disagree at Logon** — the journal says
   the next outbound is 100 and the peer asks for 50? A `ResendRequest`, a `SequenceReset`, or a
   refusal are all defensible and they have very different failure modes.
3. **Is `next_in` durable enough to be worth it?** Writing it per message puts a disk on the
   receive path. The alternative is accepting that a restart may re-deliver, which the
   application must then be idempotent against — a requirement that belongs in the public API's
   documentation if it is chosen.
