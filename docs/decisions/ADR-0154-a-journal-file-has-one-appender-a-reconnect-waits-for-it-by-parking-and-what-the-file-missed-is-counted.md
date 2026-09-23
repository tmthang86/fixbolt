# ADR-0154 — A journal file has one appender; a reconnect waits for it by parking, never by sleeping; and what the file missed is counted

- **Status**: **Accepted — 2026-09-24** (manager, owner's standing mandate, with plan Sửa 5; the rust-version bump to 1.89 supersedes ADR-0160's MSRV 1.88 when both are on main — whichever merges second reconciles). Proposed 2026-09-24. Written by the architect (Opus) for *Sửa 5* of
  [docs/plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md),
  answering the senior review of PR #103 (HEAD `0f1f8a5`). Amends
  [ADR-0153](ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md)
  (Accepted) by adding what it did not consider — the same file opened again while a retired
  writer still owns it — without changing any of its decisions.
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: ADR-0153; [ADR-0150](ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md)
  (the writer's 1 ms idle sleep); [ADR-0046](ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)
  (a refused `put` is counted); [ADR-0053](ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md)
  (the outbound mark); [ADR-0088](ADR-0088-recovery-reaches-the-sharded-runtime-and-the-journal-crosses-the-channel-with-the-connection.md)
  (recovery off the engine thread, in the shard); `DESIGN.md` D7.

## Context

### 1. Two appenders and a stale reopen — `[measured 2026-09-24]` by the senior review

ADR-0153 lets a departing connection's writer finish on its own. Nothing stops the **same file**
being opened again before it has. Probe (release): `put` seq 1, writer asleep, `mark_out(2)`,
`retire()`, drop, reopen at once → *"reopen saw highest_out=Some(1), wanted Some(2)"*, *"missed 50
of 50; mean retire→writer-done 1072 µs"*; under load in debug, *"reopen saw highest=706, expected
2000"* (19/19). A counterparty that logs back within ~1 ms is resumed from a stale count and is
sent a `MsgSeqNum` it has already seen. And both writers then append to one file; a record and
its CRC are two `write_all` calls (`crates/engine/src/journal.rs:988`, `:994`), so they can
interleave. Before ADR-0153 the join on drop made this impossible — by blocking the engine
thread.

### 2. Where the reopen happens — `[read in repo 2026-09-24]`

The file is opened by the deployment's `Recovery` (`recover`/`fresh`, `crates/engine/src/recovery.rs:131-160`),
e.g. `tools/interop/src/reconnect.rs:183-191`. The engine asks it:

- in `pump` (single-threaded `serve*`), **in the same loop that turns the engine**
  (`crates/engine/src/lib.rs:3446`) — the engine thread, mid-serving, whatever the comment there
  and `Recovery::recover`'s rustdoc say about "the acceptor thread";
- in `dial` (`connect_and_serve*`, `standard` only), `lib.rs:2714`, on the engine thread while it
  holds no connection;
- in the shard runtime, on the acceptor thread, which may block (ADR-0088).

So the fix may not wait on the engine thread in `pump`. That recovery reads a file on the engine
thread in `pump` at all is **pre-existing** and wider than this defect; it is recorded as a
`STATUS.md` open item, not fixed here.

### 3. A full ring is silent — `[measured 2026-09-24]` by the senior review

`FileJournal::put` under `Async` returns `true` when the ring push fails (`journal.rs:~1031`).
Writer asleep, 40 × 60 KB puts all `true` → 28 of 40 on disk. The session's contract
(`crates/session/src/lib.rs:3083`, `:3890`; `crates/session/src/journal.rs:22-40`): `false` means
*"not kept — every future `ResendRequest` covering it gap-fills"*, and is counted as
`puts_refused`. Here the message **is** kept in memory (`get` answers it; a resend while the
process runs replays it). Only the file lacks it, which matters after a restart.

### 4. How others keep one appender — `[documented]`

- `flock(2)` via Rust's `File::try_lock` (stable since 1.89): an exclusive advisory lock,
  `LOCK_EX|LOCK_NB`; `Err(TryLockError::WouldBlock)` if held; released when the file (and every
  duplicate of its descriptor) is closed
  (<https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock>, read 2026-09-24).
- Chronicle Queue: every appender takes a write lock for the duration of a write
  (`TableStoreWriteLock`) so appenders never interleave
  (<https://github.com/OpenHFT/Chronicle-Queue>, <https://github.com/OpenHFT/Chronicle-Queue/blob/ea/src/main/java/net/openhft/chronicle/queue/impl/single/TableStoreWriteLock.java>,
  read 2026-09-24). A lock per write would put a lock on the writer's path; one lock for the
  file's life gives the same exclusion at no per-record cost.

## Decision

1. **One appender per journal file, for the file's whole life.** `FileJournal::open` takes an
   exclusive `File::try_lock` on the journal file **before reading it**. The lock lives with the
   `File`: the writer thread's under `Async` (released when it drops the file after its last
   flush), the journal's under `Fsync`. If it is held, `open` **fails at once** with
   `io::ErrorKind::WouldBlock` naming the path — it never waits. This also refuses a second
   **process** on the same file, a pre-existing hazard. `rust-version` rises from 1.85 to 1.89
   (the pinned toolchain is 1.98; clippy's `incompatible_msrv` would otherwise deny the call).
2. **A recovery can say "not yet".** `Recovery` gains a defaulted
   `fn ready(&mut self, cfg: &Config) -> bool { true }`, and `fixbolt_engine::journal::file_busy(path)
   -> bool` answers it without blocking (open, `try_lock`, close — no sleeping syscall). The engine
   asks `ready` before `recover`.
3. **Not ready parks the connection; it never waits.** In `pump` the settled connection stays in
   the pre-session set, asked again **at most once per millisecond** by the engine's clock (no
   syscall in between); in `dial` the connected transport stays in its slot. A parked connection
   does not count as progress, so `standard` still idles (and re-asks after its wake), and `hft`
   still never sleeps. Parked longer than `LogonTimeout`, it is dropped and counted as timed out.
   The shard's acceptor thread follows the same rule, for one behaviour everywhere.
4. **What the file missed is counted, and `put` stays `true`.** Returning `false` would tell the
   session a message is unreplayable when memory can replay it, and `puts_refused` would lie.
   Instead `FileJournal` counts pushes the ring refused; `fixbolt_session::journal::Journal` gains
   a defaulted `fn unwritten(&self) -> u64 { 0 }`; the engine reports increases as
   **`EventKind::JournalUnwritten { count }`**, the way it reports `JournalRefused`. The
   operator learns the file has holes; the session's counts stay true.
5. **A journal dropped before it became a connection is retired too.** The prefix-too-long
   refusal (`lib.rs:668-673`) drops a `Resumed` journal that never reached a `Connection`; it is
   retired before it is dropped, as ADR-0153 decision 2 does for connections.

## Options not taken

- **`open` waits for the lock (blocking `lock()` or a sleep-poll).** A sleep on the engine thread
  in `pump`, mid-serving. Rejected.
- **An in-process registry of live paths.** Misses a second process, and needs a `Mutex` the
  writer and `open` share. The OS lock does both jobs. Rejected.
- **Hand the retired journal to the next `open` of its path.** Parking a generic `FileJournal<N,
  LEN>` in a process-wide map needs type erasure and an allocation at retire, on the engine
  thread. Rejected.
- **A lock per record, as Chronicle Queue does.** A lock on the writer's per-record path for an
  exclusion the file's lifetime lock already gives. Rejected.
- **`put` returns `false` on a full ring.** §3: memory kept it; the count would mislead.

## Consequences

**Good**

- A reconnect is resumed from a complete file, or not until it is; two appenders are impossible,
  in-process and across processes.
- The engine thread still never waits for a writer (ADR-0153 holds).
- A full ring stops being silent.

**Bad, and these are the price**

- **Public surface grows**: `Recovery::ready`, `Journal::unwritten`, `journal::file_busy`,
  `EventKind::JournalUnwritten`; `FileJournal::open` gains an error case. `CHANGELOG.md` names
  each; `Journal` is in `crates/session`, so the 59 acceptance definitions run.
- **A `Recovery` that opens a `FileJournal` and does not implement `ready` gets `WouldBlock`**
  from `open` on a quick reconnect, and must not read it as "no history". `GUIDE.md` says so;
  the in-repo recovery (`tools/interop`) implements `ready`.
- **Advisory, not mandatory, locking**: a program that ignores `flock` can still write the file.
- **MSRV 1.85 → 1.89.**
- **`standard` re-asks a parked recovery only on its wakes** (≤ its 100 ms poll timeout), so a
  quick reconnect there may wait up to one timeout longer than the writer needed. `[unmeasured]`.
- **`file_busy` costs three syscalls per look**, at most once per millisecond and only while a
  connection is parked. `[unmeasured]`.
- **Recovery still reads a file on the engine thread in `pump`** — pre-existing, not fixed here,
  a `STATUS.md` open item.
