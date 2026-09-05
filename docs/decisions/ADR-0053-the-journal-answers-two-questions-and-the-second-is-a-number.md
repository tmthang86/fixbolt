# ADR-0053 — The journal answers two questions, and the second one is a number

- **Status**: Proposed — 2026-09-05
- **Date**: 2026-09-05
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0008](ADR-0008-journal-is-a-trait.md) — why the journal is a trait the
  session asks rather than a buffer it owns ·
  [ADR-0017](ADR-0017-the-inbound-count-is-persisted-after-delivery.md) — the same decision for
  the inbound direction, taken two weeks earlier ·
  [ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md) — a session outlives its connection, and
  who decides to resume ·
  [ADR-0039](ADR-0039-a-fresh-journal-is-the-deployments-to-build.md) — the journal on disk ·
  [ADR-0046](ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md) — the ring
  is the whole resend store ·
  [DESIGN.md](../DESIGN.md) §4 D7 (the journal), D1 (the session is pure) ·
  `STATUS.md` item 48 ·
  [a-journal-holds-messages-not-numbering](../reference/a-journal-holds-messages-not-numbering.md) ·
  [plan](../plans/2026-09-05-a-journal-that-knows-the-numbering.md)

## Context

A FIX session numbers **every** message it sends. `Logon`, `Heartbeat`, `TestRequest`, `Reject`,
`SequenceReset` and `Logout` each spend a `34=`, exactly as an `ExecutionReport` does.

This engine's journal holds **application messages only**, and that is correct: the journal is the
store a `ResendRequest` is answered from, and QuickFIX never replays an administrative message —
it fills over it, and so does this session. So `put` is offered application messages and nothing
else.

The two facts together leave a hole nobody had named. `Journal::highest()` answers *"the highest
number I still hold for a replay"*, and every worked example in this repository read it as
*"the highest number this session has spent"*:

```rust
next_out: journal.highest().map_or(1, |h| h + 1)
```

`crates/engine/tests/on_disk.rs:288` did it, `tools/interop/src/reconnect.rs:195` did it, and
`Resumed::next_out`'s own rustdoc said *"usually `journal.highest() + 1`"*. **The two answers
differ by exactly the administrative messages sent after the last application one**, and one
clean logout is enough to differ by one: answering the counterparty's `35=5` spends a number the
journal never hears about.

`[measured 2026-09-05]` A real `libquickfix` said so in its own words when this engine came back
after `SIGTERM`:

```
MsgSeqNum too low, expecting 4 but received 3
```

**And the gate that should have caught it was green for a reason that was not the engine.** The
`SIGKILL` interop scenario ran with `HeartBtInt=30`, chosen so no `Heartbeat` could fall inside
its few-second window (`tools/interop/src/reconnect.rs:232`). It passed because of the
experiment's conditions, not because the numbering was right.

The asymmetry is the tell: **the journal already counts the inbound direction.** `mark_in(seq)`
is called after every message the application has seen (ADR-0017), `highest_in()` reads it back,
and a resumed session's `next_in` is `highest_in() + 1` — correct, never short. Outbound had
`put` for bytes and **nothing for the number**.

## Decision

**The journal answers two separate questions, and only the first one is about bytes.**

> *what can you replay?* — `get`, `highest`, `oldest`, unchanged
>
> *how far have you counted?* — `highest_out`, new

Concretely:

1. **`Journal` gains `mark_out(&mut self, seq: u32)` and `highest_out(&self) -> Option<u32>`,**
   mirroring `mark_in` / `highest_in`. `mark_out` has an empty default body, like `mark_active`:
   a journal that does not survive a restart is not obliged to pretend. `highest_out` has **no**
   default, for the reason `highest` and `highest_in` have none — a journal that holds state must
   not be able to report that it holds none.

2. **`mark_out` is a high-water mark, not a per-message event.** It records *the highest number
   spent so far*; calling it with a number the journal already knows does nothing. A successful
   `put(seq)` raises `highest_out` too, so a `mark_out(seq)` following it writes nothing. The
   invariant is one sentence: **the journal always knows the highest outbound number spent.**

3. **The session tells it from the places that already hold a journal**, using a number it
   already has — `next_out - 1`. No new session state, no new field: `received_with`, `tick_with`
   and `send_application` tell the journal on the way out, on **every** exit path.

4. **Three public entry points spend a number without holding a journal, and each gains a `_with`
   twin**: `logout_now_with`, `begin_logout_with`, `send_sequence_reset_with`. The originals keep
   their meaning for a caller that has nothing to record. `Session::connect` and
   `Session::disconnect*` are **not** in this list — reading them today shows they emit nothing
   (`let _ = emit;`) and therefore spend nothing.

5. **`FileJournal` writes a third mark**, shaped `seq == 0 && len == 4` with the number in the
   payload, distinguished from the activity mark (`len == 8`) and the inbound mark (`len == 0`)
   by exactly the machinery `Reader` already uses. **The format version does not change** —
   see *why not v2* below.

6. **`Resumed::from_journal(journal)` computes all three fields**, so no caller derives
   `next_out` by hand again. `Resumed::next_out`'s rustdoc drops the word *"usually"*.

### Why `mark_out` and not "put the administrative bytes too"

Because they are not replayable and the store would be lying. A `ResendRequest` covering an
administrative number must be answered with a gap fill — that is the protocol, not a limitation —
so bytes kept for it could never legally be sent. Writing them would grow every journal by the
Heartbeat traffic of a whole trading day to hold something nothing may read. **The number is the
only part of an administrative message that outlives it.**

### Why not read the number from `Observer` instead

That was the shape `STATUS.md` item 48 assumed when it said this could not close before item 47,
and it is wrong at the root. An `Observer` knows the number **at the moment somebody asks**. A
`Heartbeat` sent between the last poll and the process dying still leaves the recorded number
short — the same defect, now with a smaller and less reproducible window, which is worse than the
one being fixed. The only source that is both **durable** and **present at the moment `recover`
runs** is the journal itself. So item 48 needs nothing from item 47, and goes first.

### Why the file format stays v1

A record with `seq == 0` is not ambiguous: **`34=0` is not a sequence number FIX can produce**,
which is what let D7 add the inbound mark and then the activity mark without a version bump. The
new mark is the third use of the same reserved space and is told apart by its length, as the
other two are.

**And this is the last time.** The escape works because nothing is published; a fourth shape
would be a format that can only be read by knowing its history. The next record shape lifts the
version to v2.

## Consequences

**Good**

- **A deployment can be left running across a clean logout.** The interop `SIGTERM` scenario goes
  from 3 assertions to 5, and the `known_gap` step — a red pinned in a script because the engine
  could not do this — is deleted.
- **`GUIDE.md` §8c point 5 stops asking the user to do the engine's arithmetic.** It said *"keep
  your own outbound counter beside the journal"*; a constraint the type system cannot enforce is
  now a function that cannot be got wrong.
- **The `SIGKILL` scenario stops being green for the wrong reason.** It gains a run at
  `HeartBtInt=1` with a deliberate `Heartbeat` inside the window, which is what proves the fix is
  about *every* administrative message and not about `35=5`.
- The outbound direction now looks like the inbound one. Two symmetric pairs are easier to hold
  than one pair and one hole.

**Bad, and named**

- **`Journal` is a breaking change for anyone who implements it.** Two new methods, one of them
  without a default. Nothing is published, and `CHANGELOG.md` says so.
- **`tick_with` changes `&J` to `&mut J`**, also breaking, for callers that pass a journal by
  shared reference today.
- **Under `Durability::Fsync` an administrative message now costs a `sync_data`.** A `Heartbeat`
  every second becomes a disk sync every second. This is **the price ADR-0017 already accepted for
  the inbound direction**, arriving on the outbound one, not a new class of cost — and `Async` is
  the default. `CONFIGURATION.md`'s `Durability` row says it.
- **A v1 file written before this exists has no outbound mark**, so `highest_out()` falls back to
  what `highest()` would have said and a session resumed from it is short by exactly as much as
  it is today. Not worse, not better, and silent. The rustdoc says so; there is no way to
  reconstruct a number that was never written.
- **A `Reader` from an older binary reads the new mark as `Message { seq: 0, bytes: [4 bytes] }`.**
  Acceptable only because nothing is published.

**Not decided here**

- Reaching `next_out` through the front door (`STATUS.md` item 47).
- `ResetOnLogon` / `ResetOnLogout` / `ResetOnDisconnect`, which are about *restarting* the
  numbering rather than remembering it (wave B).
- Whether `MessageLog` should carry numbers too. It holds both directions already and cannot
  help here: one writer thread, flushed on close, owned by the engine for the whole run.
