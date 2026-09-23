# A readiness probe that opened a file on the engine thread

> `[measured 2026-09-24]` — found while building
> [plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md)
> *Sửa 5* row K, decided in *Sửa 6* row P,
> [ADR-0155](../decisions/ADR-0155-a-recovery-learns-its-writer-let-go-from-the-writer-not-from-the-filesystem.md).

## The trap

[ADR-0154](../decisions/ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md)
parks a reconnect until its journal's retired writer has let go of the file, and asks
`Recovery::ready` again at most once a millisecond. The first answer to `ready` was
`!journal::file_busy(path)`: open the file, `try_lock`, unlock, close. **Logically right, and the
wrong kind of call for where it runs.** In the single-engine `serve*` loops `ready` is asked on
the **engine thread**, beside the sessions it is serving — in `hft`, on the hot path
non-negotiable 4 protects. `open(2)` walks a path: directory locks, dentry and inode allocation,
metadata that may not be in cache. None of that is a *wait* by intent, and any of it can put the
thread to sleep in the kernel when the machine is busy.

"Three syscalls, none of which sleeps" was written in the rustdoc and in ADR-0154's
*Consequences* as `[unmeasured]`. It was a claim about the kernel's common path, not about the
kernel.

## How it showed

The test written for it, `a_parked_reconnect_costs_the_engine_thread_no_wait`, asserted the
`hft` engine thread's `voluntary_ctxt_switches` stayed at **0** while a connection was parked. It
read **1** in one run and **7** in another out of about 300 — the second during the whole
`--features affinity` suite with other worktrees building (load ~1.7). About 200 runs under a
`sched_switch` tracer, with directory churn in `/tmp` and a concurrent build, never caught one.

So two things were wrong at once, and they are different kinds of wrong:

1. **The design** put a filesystem probe on the engine thread, once per millisecond per parked
   connection. Whether it was the cause of those two runs is not proven; that it *can* sleep is
   enough to take it off the thread.
2. **The test** asserted something about the machine. A voluntary switch can come from a major
   page fault or memory reclaim during a parallel build — nothing the code under test does.
   `== 0` in `cargo test` on a loaded machine is flaky by construction. ADR-0072 keeps that
   measurement for a dedicated run on a quiet machine (`scripts/check-no-kernel-sleep-by-ctxt.sh`).

## What holds it now

- **The writer says when it has let go.** `FileJournal::released()` hands out a `Released`: a
  clone of an `Arc<AtomicBool>` allocated at `open`. The writer thread stores `true` after it
  has closed its file (after the lock is gone) and before it lowers the retired-writer count;
  under `Fsync` the journal's `Drop` stores it. `is_released()` is one `Acquire` load.
- **The recovery keeps the handle** of the journal it handed out for each counterparty, and
  `ready` is `is_released()`. `file_busy` stays, for tools and the sharded acceptor thread.
- **Two tests instead of one:**
  - `ready_is_answered_by_the_writer_not_the_filesystem` — deterministic: the handle is taken,
    the journal retired, **the file deleted** while the writer is still writing. A filesystem
    probe now finds nothing and says "free"; the handle says "not yet" until the writer is done.
    Red first against the `file_busy` answer: *"ready said released while the writer was still
    writing — it asked the filesystem"*.
  - `a_parked_reconnect_does_not_slow_the_engine_thread` — asserts what the engine controls: over
    50 ms while a connection is parked the engine thread is **runnable** (on a CPU, or waiting
    for one — `/proc/self/task/<tid>/schedstat`) for at least half the window (a sleep of 1 ms
    per ask read **455 µs of 50 ms**), `ready` is asked at most once per elapsed millisecond
    (asking every turn read **9 515** in 50 ms), the running session is still answered, and
    `recover` runs in the same pass as the `ready` that said yes.

**A turn count is a scheduler measurement too.** The plan first asked for "≥ 10 000 turns in
50 ms", counted by snapshot ping-pong between the test thread and the engine: ~8 400 on an idle
desk in a debug build, and **8** during a parallel `cargo build --release` — both threads were
preempted. Runnable time does not depend on being given a CPU, only on not choosing to sleep,
which is the property.

## The rule

**A question the engine thread asks between turns is answered from memory another thread
wrote, never by a system call.** If the answer lives in the kernel, a thread that may block
fetches it and publishes it. And a test that counts sleeps on a thread asserts about the
machine; assert the property the code owns (whether it stays runnable, how often it asks), and measure
sleeps where the machine is quiet.

## What is not proven

- The cause of the two red runs. The probe was taken off the thread without the tracer ever
  catching it.
- `scripts/check-no-kernel-sleep.sh`'s windowed `strace` does not run through a parked
  connection; no gate traces the engine thread while one is parked.
