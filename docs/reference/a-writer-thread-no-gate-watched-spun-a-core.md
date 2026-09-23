# A writer thread no gate watched spun a core

> `[measured 2026-09-23]` — found while planning
> [plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md), fixed
> in its row D5,
> [ADR-0150](../decisions/ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md)
> decision 4.

## What happened

Non-negotiable 4 says a `standard` engine gives the core back when idle, and three scripts prove
it for **the engine thread**: `check-no-kernel-sleep.sh` filters syscalls by the engine's tid,
`check-no-kernel-sleep-by-ctxt.sh` reads the engine thread's context switches, and
`check-standard-gives-the-core-back.sh` measures the engine thread's CPU. All three were green.

Meanwhile the two writer threads beside it waited on an empty ring like this:

- `FileJournal` under `Async`: `std::hint::spin_loop()` and poll again — a busy spin;
- `FileLog`: flush, then `std::thread::yield_now()` — which on a machine with a free core is run
  again at once (ADR-0013 §2: a yield is not a wait).

`crates/engine/tests/writer_idle.rs`, before the fix, on the desk under load (load average ~30 on
16 threads, other sessions compiling): **the journal writer used 457 ms and the message log
writer 429 ms of CPU in 1 s while idle**; with the fix reversed later, 684 ms and 457 ms. On an
idle machine the figure approaches a whole core. After the fix: **0 ms** for both.

The unpinned writers had no name either (`std::thread::spawn`), so an operator looking at `top -H`
saw an anonymous thread of the engine's process burning a core.

## Why no gate saw it

**Every mode gate is scoped to one tid**, and the rule it enforces was written about one thread.
The promise a user hears — *"`standard` gives the core back"* — is about the process. A gate
scoped narrower than the promise is green about a thing the user never asked about. And the mode
scripts had never run with a journal writer or a log at all (`W2W_EXTRA` without
`--journal`/`--log`), so the writers were not even in the process under test.

## The rule now

Both writers share one wait, `ring.rs` `Idle`: spin 1 024 empty polls (a burst is caught with no
syscall), then `std::thread::sleep(1 ms)` per empty poll; any record resets the count. In every
mode, because the writer is not the engine thread. The engine thread **never wakes the writer**
— no `unpark`, no futex on the push side — so its path is byte-for-byte unchanged. A record
pushed to a sleeping writer waits at most 1 ms. The unpinned threads are named `fixbolt-journal`
and `fixbolt-msglog`, as the pinned ones were.

## A second thing the scripts showed, not caused by the fix

With `W2W_EXTRA="--journal file-async --log file"`, **`check-no-kernel-sleep.sh` reports
`2 futex` on the `hft` engine thread** and fails. `strace -f -tt` shows both calls are
`FUTEX_WAIT_BITSET` made **after the engine thread's last `recvfrom`/`sendto`/`accept4`**, each
followed by the `munmap` of a writer's ring (1 052 672 bytes, the journal's 1 MiB ring; 4 198 400
bytes, the log's 4 MiB ring): the `JoinHandle::join` in `FileJournal::close` and `FileLog::close`,
run from `Drop` when the engine is torn down. **It is not D5's**: the same trace of a `w2w` built
from the commit before D5 (`016f2a5`, writers still spinning) reads `FUTEX_WAIT = 2` in 5 of 5
runs, as the D5 build does. The script counts the engine thread's syscalls over its **whole life**,
teardown included, and a teardown that joins a thread always waits in a futex. Non-negotiable 4 is
about the hot path; the join is not on it. `check-no-kernel-sleep-by-ctxt.sh`, which counts only
the timed window, reads `hft voluntary 0` with both writers running.

**Open**: whether the strace script should stop counting at the end of the timed window, or the
engine should join its writers from another thread. That is a decision, not a fix, and it is
recorded for the plan's owner rather than made here.

## The tests that guard it

| Guard | Test | Reversal |
|---|---|---|
| The journal writer sleeps when idle | `writer_idle.rs::an_idle_async_journal_writer_gives_its_core_back` (< 50 ms CPU in 1 s) | `spin_loop` back → *"the journal writer used 684 ms of CPU in 1 s while idle"* |
| The log writer sleeps when idle | `writer_idle.rs::an_idle_message_log_writer_gives_its_core_back` | `yield_now` back → *"the message log writer used 457 ms of CPU in 1 s while idle"* |
| A sleeping writer still drains a burst | `writer_idle.rs::an_async_journal_still_writes_a_burst_after_it_slept` (1 000 of 1 000) | a count that never resets stays **green** — sleeping too early delays records but drops none, so this test guards the waking, not the spin budget |
| The engine thread is unchanged | `check-no-kernel-sleep-by-ctxt.sh` and `check-standard-gives-the-core-back.sh` with `W2W_EXTRA="--journal file-async --log file"` | each script's own red half |

The idle tests also assert the thread was readable across the whole window: a dead writer costs
0 ms and would pass the ceiling on nothing.
