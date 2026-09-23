# ADR-0155 — A recovery learns that its writer let go from the writer, not from the filesystem

- **Status**: **Accepted — 2026-09-24** (manager, owner's standing mandate, with plan Sửa 6). Proposed 2026-09-24. Written by the architect (Opus) for *Sửa 6* of
  [docs/plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md).
  On acceptance it **supersedes
  [ADR-0154](ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md)
  decision 2's second half** — that `journal::file_busy` answers `Recovery::ready`. ADR-0154's
  lock (decision 1), `Recovery::ready` itself (decision 2, first half), parking (decision 3) and
  decisions 4–5 stand.
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: ADR-0154, [ADR-0153](ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md),
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md) (rule 4), `DESIGN.md` D7, D8.

## Context

### 1. What rows K+L saw — `[measured 2026-09-24]` in worktree `fb-k`, uncommitted

`a_parked_reconnect_costs_the_engine_thread_no_wait` (an `hft` engine thread, 0 voluntary context
switches while a connection is parked) read **1** switch once and **7** once in ~300 runs under
parallel builds; ~200 runs under a `sched_switch` tracer caught none. The builder's suspect, not
proven: `journal::file_busy` — `open`, `try_lock`, unlock, `close` — called from
`Recovery::ready` on the engine thread in `pump`, at most once per millisecond while a
connection is parked (`tools/interop/src/reconnect.rs:201-203`, `crates/engine/src/journal.rs:996-1010`
in `fb-k`).

### 2. Is a filesystem syscall per parked millisecond allowed? — `[read in repo]`

No. Rule 4 (`CLAUDE.md` §2) forbids the `hft` engine thread to sleep in the kernel on the hot
path and names a blocking `read` as an example. `open(2)` resolves a path: it takes directory
and inode locks and may wait on I/O for metadata that is not cached — it **can** sleep, and
whether it did is a property of the machine's load, which is exactly the flakiness observed.
A parked reconnect sits beside live sessions in `pump`, so this is the hot path. That
`Recovery::recover` itself reads a file on the engine thread in `pump` is pre-existing, recorded
for `STATUS.md` (ADR-0154 *Consequences*), and is not a licence to add a periodic one.

### 3. Why a test counting voluntary switches is flaky — `[documented]`

A voluntary switch is any block, including a major page fault or reclaim under memory pressure
from a parallel build — events the code under test does not cause. `== 0` over a window on a
loaded machine asserts the machine as much as the code; ADR-0072 keeps that assertion for a
dedicated run on a quiet machine, not for `cargo test`.

## Decision

1. **`Recovery::ready` must answer without a syscall.** Its rustdoc says so, because the engine
   asks it on the engine thread in `pump`.
2. **The writer says when it has let go.** `FileJournal::released(&self) -> Released` hands out a
   cheap `Clone` handle over an `Arc<AtomicBool>` allocated **at open**. The writer thread stores
   `true` (Release) **after it has closed its `File`** — so after the lock of ADR-0154 decision 1
   is gone — and before it lowers the retired-writer count. Under `Fsync`, and on a joined
   `close()`, it is stored when the journal's `File` is dropped. `Released::is_released()` is one
   Acquire load.
3. **A recovery keeps the handle of the journal it handed out** for each counterparty, and
   answers `ready` with `is_released()` (no handle yet → ready). Nothing process-wide is added:
   the recovery already knows its paths, and a registry keyed by path would need a lock or an
   allocation on the engine thread.
4. **`file_busy` stays, off the engine thread.** It is for tools and for the shard's acceptor
   thread; its rustdoc says not to call it from `ready` in `pump`. A second **process** holding
   the file is still refused by `open`'s own `try_lock` (ADR-0154 decision 1) — a deployment
   error that must fail loudly, not park.
5. **The engine test asserts what the engine controls.** No voluntary-switch count in
   `cargo test`. It asserts that while a connection is parked the other session keeps being
   served at spinning speed (turns during the parked window far above what a millisecond sleep
   per ask allows), that `ready` is asked at most once per millisecond, and that the connection is
   admitted within a turn of `is_released()` turning true. The "no filesystem" property is proven
   separately and deterministically (plan *Sửa 6*, test 1).

## Options not taken

- **Keep `file_busy` and accept the syscall.** §2. Rejected.
- **A process-wide registry keyed by path.** Needs a lock or an allocation on the engine thread
  to look up, and the recovery already holds the key. Rejected.
- **Keep counting voluntary switches with a tolerance.** A tolerance that passes 7 also passes a
  sleep per ask on a short window; it no longer bites. Rejected.

## Consequences

**Good**

- Parking costs the engine thread one atomic load per ask, whatever the filesystem is doing.
- The engine test becomes deterministic without losing its reversal.

**Bad, and these are the price**

- **Another public item** (`FileJournal::released`, `Released`), and a recovery must keep one
  handle per counterparty. `GUIDE.md` shows the pattern; `tools/interop` uses it.
- **A user's `ready` can still do a syscall**; only the rustdoc and `GUIDE.md` forbid it. The
  engine cannot check it.
- **The engine test no longer proves "0 voluntary switches"** for parking. That claim rests on the
  mechanism (an atomic load) and the deterministic test; the strace window check does not
  exercise a parked reconnect. Stated, not hidden.
