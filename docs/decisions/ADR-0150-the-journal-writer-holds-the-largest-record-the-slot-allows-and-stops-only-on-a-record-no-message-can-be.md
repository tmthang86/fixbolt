# ADR-0150 — The journal writer holds the largest record the slot allows, and stops only on a record no message can be

- **Status**: **Accepted — 2026-09-23** (manager, owner's standing mandate, with the phase-3-found-defects plan). Proposed 2026-09-23. Written by the architect (Opus) for defect D1 of
  [docs/plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md).
  Accepting that plan accepts this ADR. **Revised in place 2026-09-23, still Proposed**, at the
  manager's request: decision 4 and *Context* §5 added for defect D5 (the writer threads'
  idle wait); decisions 1–3 unchanged.
- **Date**: 2026-09-23
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: `DESIGN.md` D7;
  [ADR-0037](ADR-0037-reading-a-journal-is-not-recovering-from-one.md) (a thread that is not the
  engine thread may allocate);
  [ADR-0046](ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md) (a refused
  `put` is a gap fill, and it is counted);
  [ADR-0110](ADR-0110-a-secret-is-masked-in-the-message-log-and-leaves-only-its-number-in-the-journal-file.md)
  decision 4 (the writer decides what reaches the file); `crates/engine/src/msglog.rs` (the
  message log's writer, which already solved both halves of this).

## Context

### 1. The defect — `[read in repo 2026-09-23, main 2f0a0dc]`, reproduced by a reviewer

`FileJournal<N, LEN>` under `Durability::Async` hands each accepted message to a writer thread
through `crate::ring` as one record: 4 bytes of sequence number, 4 bytes of length, the message
(`crates/engine/src/journal.rs:854-855`). The writer (`write_loop`, `journal.rs:792-835`) pops
into a **fixed `[0u8; 4096]` buffer**.

`ring::Consumer::pop` (`crates/engine/src/ring.rs:183-222`) answers a record longer than the
buffer by **dropping it and returning `Some(0)`**, documented as *"so a caller that cares can tell
it apart from an empty queue"*. `write_loop` reads `Some(0)` as **its stop signal** — the empty
record `close()` pushes (`journal.rs:770-781`). So the first message longer than
4096 − 8 = 4088 bytes stops the writer thread for good. `MemJournal::put` accepted it (it only
refuses `bytes.len() > LEN`, `journal.rs:153`), `FileJournal::put` returned `true`, and every
later message still enters the ring and is never read. Nothing is counted and no event is
raised.

Reproduction (the reviewer's): `FileJournal<8, 8192>`, `Async`; put a 4200-byte message, then a
small one; close and reopen → `get(1) = None`, `get(2) = None`, `highest_out = None`.

Only deployments that raise `LEN` above 4088 can reach it — the default `SLOT_LEN` is 512
(`journal.rs:63`). `Durability::Fsync` writes inline and is not affected.

### 2. A second defect at the same boundary — `[read in repo 2026-09-23]`, not reproduced yet

`MemJournal::put` stores the length as `u16::try_from(bytes.len()).unwrap_or(0)`
(`journal.rs:173`), and `get` answers only `slot.len > 0` (`journal.rs:198`). With `LEN` above
65 535 a longer message is **reported kept (`put` returns `true`) and then answered as absent**:
the refusal ADR-0046 says must be counted is not counted. Nothing bounds `LEN` today.

### 3. The pattern already in the repository — `[read in repo]`

The message log's writer (`msglog.rs:408-460`) met the same ring and decided both halves
differently: its buffer is sized to the largest record it can be sent (`WRITER_BUF =
REC_HEADER + MAX_RECORD`, `msglog.rs:85`), allocated once on the writer thread, and its stop
signal is a **one-byte `STOP` record** (`msglog.rs:94`) that no real record can be, so
`Some(0)` means only *"a record was dropped"* and is counted.

### 4. Prior art — `[documented]`

QuickFIX/J's `FileStore.set` writes `message.getBytes(...)` whole, with no size cap or fixed
buffer (<https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-core/src/main/java/quickfix/FileStore.java>,
read 2026-09-23): a journal that accepts a message writes all of it. No engine found that bounds
a stored message by its writer's buffer rather than by its store.

### 5. Both writer threads burn a core when idle — `[read in repo 2026-09-23]` (defect D5)

`write_loop` answers an empty ring with `std::hint::spin_loop()` and polls again
(`journal.rs:832`), in every mode: an idle `Async` journal keeps one core at 100 %. The message
log's writer answers it with `std::thread::yield_now()` (`msglog.rs:428`), which
[ADR-0013](ADR-0013-two-modes-standard-and-hft.md) decision 2 already rules is **not** a way to
give a core back — on an idle machine a yielding thread is rescheduled at once. Non-negotiable 4
names the engine thread only, and the mode scripts watch the engine thread's tid only
(`scripts/check-no-kernel-sleep.sh` header), so no gate sees either writer. Neither writer knows
the engine's mode: `FileJournal::open` and `FileLog::open` take no wait strategy, and the same
journal type is built for `hft` and `standard` engines alike (`tools/w2w`'s `--journal
file-async`).

## Decision

1. **The writer's buffer is sized by the slot, not by a literal.** `write_loop` pops into a
   buffer of `RECORD_HEADER + LEN` bytes, allocated **once, on the writer thread, before its
   loop** (ADR-0037: not the engine thread). Since `MemJournal::put` refuses anything longer than
   `LEN` before the ring is touched, every record the ring can carry fits — a record too long for
   the writer is impossible by construction, not merely unlikely.
2. **The stop signal is a record no message can be.** `close()` pushes a one-byte `STOP` record,
   as the message log does. A journal record is never shorter than `RECORD_HEADER` (8) bytes, so
   one byte cannot be mistaken for one. `Some(0)` from `pop` means *"the ring dropped a record"*
   and the writer **continues**; it never stops on it.
3. **A message longer than `u16::MAX` is refused by `MemJournal::put`**, returning `false` and
   so counted as ADR-0046 requires, rather than kept with a zero length. `LEN` stays a free const
   generic; the bound is on the message, stated in `CONFIGURATION.md` beside `SLOT_LEN`.

4. **A writer with nothing to write sleeps, in every mode.** Both writers (`journal.rs`
   `write_loop`, `msglog.rs` `write_loop`) share one idle rule, kept in `ring.rs` beside the
   queue it waits on: after **1 024 consecutive empty polls** (each a `spin_loop` hint — a burst
   is still caught without a syscall), the thread calls `std::thread::sleep(1 ms)`; any record
   resets the count. The engine thread **never wakes the writer** — no `unpark`, no futex on the
   push side — so the engine's path is byte-for-byte unchanged in both modes. Mode-free on
   purpose: the rule is safe for `hft` because the writer is not the engine thread and its ring
   bounds the risk (below), and it needs no new constructor argument. The unpinned writers are
   named `fixbolt-journal` and `fixbolt-msglog`, as the pinned ones already are, so a test and an operator can find it.

   **The bound, `[derived, not measured]`**: in 1 ms the journal's 1 MiB ring
   (`journal.rs:711`) fills only above ~1 GB/s of records; the message log's ring is
   `DEFAULT_CAPACITY` = 4 MiB (`ring.rs`) or the caller's size. A FIX engine at 1 M msg/s of
   200-byte records writes ~200 MB/s — five times under. A full ring is already a message not
   journalled (gap fill), so an overflow would be visible as today, not a new failure.

## Options not taken

- **Refuse, in `FileJournal::put`, any message longer than 4088 under `Async`.** Honest about
  the limit, but it makes `LEN` a lie for exactly the deployment that raised it, and the two
  durabilities would keep different messages. Rejected.
- **Stream an oversized record in chunks.** `ring.rs`'s module contract is *"records, not
  bytes"*; chunking would change the ring for `RingDispatch` and the message log too, to serve a
  case decision 1 makes impossible. Rejected.
- **Change `Consumer::pop` to return a three-way enum** (record / dropped / empty). Cleaner in
  isolation, but it edits all three consumers (`dispatch.rs:300`, `dispatch.rs:381`,
  `msglog.rs:422`) for no behaviour this ADR needs. Left for whoever next touches the ring.
- **(D5) Let the engine thread `unpark` the writer on each push.** One futex syscall per journalled
  message on the engine thread — forbidden on the `hft` hot path by non-negotiable 4 and a cost
  `standard` does not need. Rejected.
- **(D5) A wait strategy per writer, chosen by the caller** (spin for `hft`, block for
  `standard`). Correct but a public-API change on two constructors for a thread non-negotiable 4
  does not govern; the mode-free sleep satisfies both. Reopen if a measurement shows the 1 ms
  wake costs an `hft` deployment something.
- **A stack buffer `[u8; RECORD_HEADER + LEN]`.** Needs `generic_const_exprs`, which is unstable.

## Consequences

**Good**

- A message the in-memory ring accepted reaches the file, whatever `LEN` is — the two halves of
  `FileJournal` agree again.
- The writer can no longer be stopped by data. Only `close()` stops it.
- The journal and the message log now stop their writers the same way; one pattern to read.
- The `u16` hole is closed with a refusal that is counted, not a silent empty slot.
- (D5) An idle `Async` journal and an idle message log give their cores back, in `standard`
  as the mode promises and in `hft` without touching the engine thread.

**Bad, and these are the price**

- **One allocation of `8 + LEN` bytes per `FileJournal` under `Async`**, on the writer thread,
  at start. At `LEN = 512` it is smaller than the 4 KiB stack buffer it replaces; at
  `LEN = 65 535` it is 64 KiB. Not on the engine thread, so non-negotiable 1 is not in play, and
  `benches/alloc.rs` (which counts the engine thread's path) must stay green unchanged.
- **Messages longer than 65 535 bytes cannot be journalled at all** — they become gap fills on a
  resend. FIX messages that long exist only with large `RawData`/`XmlData` payloads; a deployment
  that needs them needs a different slot representation and a new ADR.
- **Defence in depth is by construction, not by a counter.** `Some(0)` is unreachable after
  decision 1 and is only `continue`d over; if a future change made it reachable, the loss would be
  one record, not the rest of the file — but it would still be silent. A counter was judged not
  worth a public accessor for an unreachable case.
- **(D5) An idle-then-busy writer notices its first record up to ~1 ms late** (plus timer
  slack). Nothing waits on the disk under `Async` except `close()`, so no latency figure moves;
  but a crash in that millisecond loses what the writer had not reached — the loss `Async`
  already states, widened by up to 1 ms.
- **(D5) An `hft` deployment that pinned the writer to its own core loses a spinning writer.**
  Its engine thread is unchanged; the writer's core now idles. The constants (1 024 polls, 1 ms)
  are `[unmeasured]` and stated here so they can be argued with.
- **Files written before the fix that lost records stay lost.** Nothing here recovers them.
