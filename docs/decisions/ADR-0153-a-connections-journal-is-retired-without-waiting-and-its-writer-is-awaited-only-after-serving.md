# ADR-0153 — A departing connection's journal is retired without waiting; its writer is awaited only after serving

- **Status**: **Accepted — 2026-09-23** (manager, owner's standing mandate, with plan Sửa 3). Proposed 2026-09-23. Written by the architect (Opus) for *Sửa 3* of
  [docs/plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md).
  On acceptance it **supersedes the part of
  [ADR-0152](ADR-0152-non-negotiable-4-judges-the-engine-threads-serving-window-and-its-teardown-may-wait-for-its-writers.md)
  that says a writer join happens only at teardown** — its *Context* §3 and the premise of its
  decision 1. ADR-0152's window mechanism (decisions 2–5) stands unchanged: it is what found this.
- **Date**: 2026-09-23
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: [ADR-0150](ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md)
  (the writer, its `STOP` record and its 1 ms idle sleep);
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md) (rule 4, both halves);
  [ADR-0008](ADR-0008-journal-is-a-trait.md) (the `Journal` trait); `DESIGN.md` D7, D8.

## Context

### 1. What row W found — `[measured 2026-09-23]`, worktree `fb-w`, branch `fix/w-serving-window`

With ADR-0152's serving-window markers built into `tools/w2w` and the script windowed as that ADR
says (its reversals red as predicted), `W2W_EXTRA="--journal file-async --log file"
scripts/check-no-kernel-sleep.sh` was red in **2 of 8** runs, and **3 of 12** manual traces read
`inside=1 outside=1`. Only the message log's `futex` is teardown. The journal's is not: trace
order on the engine tid is `recvfrom = 0`, `close(7)`, `futex`, `munmap(…, 1052672)`, then the
`serve-close` marker. Whether it lands inside the window depends on whether the peer's EOF or the
stop request is seen first.

### 2. Why — `[read in repo 2026-09-23]`

- A journal is **per connection**: `Connection` owns its `J` (`crates/engine/src/conn.rs:49`;
  `add_with_journal`).
- When a session ends mid-serving, `turn()` removes it with `self.conns.swap_remove(i)`
  (`crates/engine/src/lib.rs:1351`), **on the engine thread**. `shutdown_finished` does the same
  with `self.conns.clear()` (`lib.rs:1065`), also inside the serving loop.
- Dropping a `FileJournal` runs `close()`, which pushes `STOP` and **joins** the writer
  (`crates/engine/src/journal.rs:770-788`). A join on a running thread is a `futex` wait. Since
  ADR-0150 decision 4 the writer may be in its 1 ms idle sleep, so the wait can last ~1 ms.
- So an `hft` engine thread sleeps in the kernel **while serving** every time a session with a
  `FileJournal` ends — a real breach of non-negotiable 4, pre-existing (the join is older than
  D5), and in `standard` a stall of up to ~1 ms for every other session on that thread.
- ADR-0152 *Context* §3 read the join as teardown-only. That was wrong: it holds for the engine's
  message log (one per engine, dropped after the loop) and not for journals.

### 3. How others keep a hot thread from waiting on a resource's end — `[documented]`

- Chronicle Core's `BackgroundResourceReleaser`: closeable resources are **queued and released on
  a background thread** (or a user thread that calls `releasePendingResources()`), not on the
  thread that let them go (<https://github.com/OpenHFT/Chronicle-Core>, read 2026-09-23).
- Aeron's C media driver hands log-buffer freeing to a separate agent as `FREE_LOG_BUFFER`
  commands, and drains them on close
  (<https://github.com/aeron-io/aeron/pull/2143>, <https://github.com/aeron-io/aeron/issues/2127>,
  read 2026-09-23).
- Rust's `JoinHandle`: dropping it **detaches** the thread; `is_finished` supports a non-blocking
  join (<https://doc.rust-lang.org/std/thread/struct.JoinHandle.html>, read 2026-09-23).

Both engines move the end of a resource's life off the hot thread and keep a place where it is
finally awaited. Both halves are taken here; the queue is not, because the only thing left to do
after `STOP` is to wait, and the writer can do its own ending.

## Decision

1. **The engine never waits for a journal writer while serving, in either mode.** A departing
   connection's journal is **retired**, not closed: the writer is told to stop and nobody waits
   for it on the engine thread.
2. **`fixbolt_session::journal::Journal` gains a defaulted `fn retire(&mut self) {}`** — no
   socket, no clock, no allocation, so the session layer stays pure (non-negotiable 2). `Connection`
   gains a `Drop` that calls `self.journal.retire()`, so **every** path that drops a connection —
   `swap_remove`, `clear`, the engine's own drop — retires first, with no call site to forget.
3. **`FileJournal::retire`** makes no syscall and never spins: it tries to push `STOP` once; if the
   ring is full it sets a stop flag the writer reads when the ring runs dry. It takes the writer's
   `JoinHandle` and drops it (detach), and counts the writer in a process-wide count of
   **retired writers not yet finished**. The writer, after its final flush, decrements that count
   as its last act. A later `Drop` of the retired journal finds no handle and does not join.
4. **The wait moves to after serving.** `fixbolt_engine::journal::wait_for_retired_writers(timeout)
   -> bool` polls that count (sleeping 1 ms between looks) until it is zero or the timeout passes,
   and says which. Every entry point that runs a serving loop (`serve*`, `connect_and_serve*`, the
   shard's serve) calls it **after its loop returns** — teardown, which ADR-0152 decision 1 allows
   to block — with the shutdown grace as the timeout. A caller that drives `Engine` directly must
   call it before exiting; `GUIDE.md` says so, because the compiler cannot.
5. **`FileJournal::close()` and a plain `Drop` of an unretired journal keep their blocking join**,
   for code that is not the engine thread (tests, tools, a caller closing its own journal).

## Options not taken

- **Wake the writer at once on `STOP` so the join is short.** Still a `futex` wait on the engine
  thread; short is not zero. Rejected.
- **A reaper thread fed through a channel.** `std::sync::mpsc` allocates blocks and wakes the
  receiver with a `futex` wake, which the strace check counts. Rejected.
- **Detach with no barrier.** A normal exit would lose what the writer had not reached — the loss
  `Async` accepts on a crash, not on a clean stop. Rejected.
- **Make every `FileJournal` drop non-blocking.** Changes the meaning of `Drop` for every owner,
  and existing tests read files right after a drop. Rejected; retiring is the engine's explicit act.

## Consequences

**Good**

- A session ending mid-serving costs the engine thread no wait, in `hft` (rule 4 holds) and in
  `standard` (no stall for the other sessions).
- The durability `Async` promised on a clean stop is kept, and the place it is paid is named.
- ADR-0152's window stays sound and becomes stricter in effect: the journal's wait is now a defect
  it catches, not teardown it forgives.

**Bad, and these are the price**

- **A caller driving `Engine` directly can lose data at exit** by not calling
  `wait_for_retired_writers`. Only `GUIDE.md` and the rustdoc hold that line.
- **A process-wide counter** is shared by every engine in the process; a serve loop's teardown
  waits for writers another engine retired. Harmless (it only waits longer), stated here.
- **`Journal` grows a method** — additive, defaulted; `CHANGELOG.md` names it. Touches
  `crates/session`, so the 59 acceptance definitions run (non-negotiable 3).
- **`Connection` gains a `Drop`**, which forbids moving fields out of it; any such code must
  change.
- **Freeing a departed connection's memory still happens on the engine thread** (its buffers, a
  `MemJournal`'s slots; a ring if the writer had already exited). That is deallocation and an
  `munmap`, not a sleep; it is pre-existing and out of this ADR's scope.
- The count's 1 ms poll is `[unmeasured]` and teardown-only.
