# An `Async` journal record longer than its writer's buffer stopped the writer for good

> `[measured 2026-09-23]` — found by the phase 3 review, fixed under
> [plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md) row D1,
> [ADR-0150](../decisions/ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md)
> decisions 1–3.

## What happened

`FileJournal<8, 8192>` under `Durability::Async`: `put(1, <4 200 bytes>)`, then `put(2, <small>)`,
`close`, reopen the same file. **`get(1) = None`, `get(2) = None`, `highest_out() = None`** — and
both `put` calls had answered `true`.

Three facts, each correct on its own, made one silent data loss:

1. The writer thread (`journal.rs` `write_loop`) read the ring into a **fixed `[0u8; 4096]`**.
2. `ring::Consumer::pop` handed a record longer than the buffer **drops it and returns
   `Some(0)`**, so the queue does not wedge — documented, and right for a queue.
3. `write_loop` read **`Some(0)` as its stop signal**, because `close()` sent an empty record.

One value, two meanings: *"a record was dropped"* and *"stop"*. A message longer than
4 096 − 8 bytes stopped the writer thread; every later record sat in the ring until the process
exited, and the in-memory ring went on answering resends, so nothing on the wire showed it.

Only a deployment that raised `LEN` above 4 088 could reach it — the default `SLOT_LEN` is 512 —
and `Durability::Fsync` writes inline, so it was never affected.

## The second defect in the same place

`MemJournal::put` stored the length as `u16::try_from(len).unwrap_or(0)`, and `get` returns only
when `len > 0`. With `LEN > 65 535`, a message longer than 65 535 bytes was reported kept by `put`
and then answered as absent by `get` — a refusal nobody counted, where ADR-0046 requires every
refusal to be counted.

## The rule

- **A sentinel must be a value the data can never be.** A journal record is never shorter than
  `RECORD_HEADER` (8) bytes, so the stop signal is now a one-byte `STOP` record, as the message
  log's already was (`msglog.rs` `STOP`). `Some(0)` from `pop` means *dropped* and the writer
  continues.
- **A consumer's buffer is sized by what the producer may send**, not by a literal: the writer's
  buffer is `RECORD_HEADER + max(LEN, 8)`, allocated once on the writer thread (ADR-0037).
- **An `unwrap_or(0)` on a length is a refusal in disguise.** It is now an explicit `false`.

## The tests that guard it

| Guard | Test |
|---|---|
| Buffer sized by `LEN` | `crates/engine/tests/journal.rs::an_async_journal_keeps_a_message_longer_than_four_kilobytes_and_all_that_follow` — reversal (buffer back to 4 096) reads `(None, true, Some(2))` |
| `Some(0)` does not stop the writer | `crates/engine/src/journal.rs` `writer_tests::a_record_the_writer_cannot_hold_does_not_stop_it` — calls `write_loop` with a small buffer on purpose, since the first fix makes `Some(0)` unreachable through `FileJournal`; reversal reads an empty file |
| Longer than `u16::MAX` is refused | `crates/engine/tests/journal.rs::a_message_longer_than_a_u16_is_refused_not_kept_empty` — reversal reads `(true, false)` |

**Why the second test is not through the public API:** once the buffer holds the largest record,
no journal record reaches `Some(0)`, so a writer that went back to stopping on it would leave
every public-API test green. A guard whose case the fix makes unreachable has to be tested below
the fix.

## A cost the fix paid, found one step later

Moving the buffer from a stack array to a `Vec` made it an allocation, and `benches/alloc.rs`
counts with a **global** allocator. Case `mark-out-file-async` opens a `FileJournal` under
`Async` and counts at once, so when the writer thread started late its startup allocations
landed inside the window. `[measured 2026-09-23, desk at load average ~30]` HEAD with this fix
alone read `mark-out-file-async 1` in 1 run of 6. With D5's named writer, 2 in 2 runs of 4. **Not
an engine-thread allocation, but a gate that goes red by chance is a gate nobody believes.**
`FileJournal::open` now returns only after the writer holds its buffer (a one-slot
`sync_channel`, at startup). After that, 8 runs of 8 read 0 at the same load, with the bench
unchanged. The rule: **a thread a constructor spawns finishes allocating before the constructor
returns**, or every counting window that opens after it races that thread.
