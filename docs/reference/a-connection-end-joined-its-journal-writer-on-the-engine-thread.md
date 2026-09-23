# A connection's end joined its journal writer on the engine thread

> `[measured 2026-09-23]` — found by row W of
> [plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md)
> (*Sửa 3*), fixed in its row J,
> [ADR-0153](../decisions/ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md).

## What happened

[ADR-0152](../decisions/ADR-0152-non-negotiable-4-judges-the-engine-threads-serving-window-and-its-teardown-may-wait-for-its-writers.md)
taught `scripts/check-no-kernel-sleep.sh` to judge only the engine thread's **serving window**,
on the premise that a writer thread's join happens only at teardown, when the engine is dropped.
With `W2W_EXTRA="--journal file-async --log file"` the windowed script was still red **2 runs
in 8** — `FAIL: the engine thread slept in the kernel: 1 futex` — and a hand trace read the
journal's `futex` *inside* the window 3 times in 12.

The premise was wrong. `Engine::turn` removes a finished connection with
`self.conns.swap_remove(i)`. That drops the `Connection`, which drops its `FileJournal`, whose
`Drop` calls `close()`: push `STOP`, then **`join` the writer** — a `futex` wait on the engine
thread, while every other session on it is still being served. The order on the engine's tid:
`recvfrom = 0`, `close(7)`, `futex`, `munmap(…, 1052672)`, then the `serve-close` marker.
Whether it landed inside or outside the window depended only on whether the counterparty's EOF or
the stop request arrived first.

So in `hft` the engine really slept in the kernel mid-serving **every time a session with a
`FileJournal` ended** — rule 4 broken, before the idle writer (row D5) made the join up to ~1 ms
long. In `standard` it was a stall of every other session on the thread.

## Why no gate saw it before

The unwindowed script read every engine-thread `futex` as a failure and was already red with a
writer attached, for teardown joins that are allowed; the red was attributed to teardown as a
whole. Only once the window excluded teardown did the join that was *not* teardown stand out. The
window was right; the premise it was built on was the defect.

## The fix, and what guards it

A departing connection's journal is **retired**, not closed (ADR-0153): `Connection`'s `Drop`
calls `Journal::retire`, which for `FileJournal` pushes `STOP` once (or sets a stop-when-dry flag
if the ring is full), detaches the writer and counts it; no syscall, no spin. The wait moves to
after the serving loop: `journal::wait_for_retired_writers(timeout)`, called by every serve
function once its loop has returned.

`crates/engine/tests/retire.rs`:

- `a_session_with_a_file_journal_ends_without_the_engine_thread_waiting` — 20 sessions with an
  `Async` `FileJournal` end on an `hft` engine; the engine thread's `voluntary_ctxt_switches`
  across those endings must be **0**. Before the fix: 20, one per join.
- `a_standard_engine_waits_no_longer_for_a_file_journal_than_for_a_memory_one` — the same in
  `standard`, against a `MemJournal` baseline. Before the fix: 20 against 0.
- `a_retired_journal_reaches_the_disk_before_the_wait_returns` — 10 000 messages, retired, all on
  disk once the wait returns. With the wait reversed to return at once: 1 158 of 10 000.
- `a_journal_closed_by_its_owner_still_joins` — `close()` keeps its blocking join.

## The lesson

**A drop is a call site.** A `Drop` that blocks runs wherever the value happens to die, and a
`Vec::swap_remove` in the middle of a loop is a place a value dies. A rule about what the engine
thread may do has to be checked against every destructor that can run on it, not only against the
calls written in the loop.
