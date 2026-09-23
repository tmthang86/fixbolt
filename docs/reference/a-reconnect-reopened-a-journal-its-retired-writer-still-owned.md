# A reconnect reopened a journal its retired writer still owned

> `[measured 2026-09-24]` — found by the senior review of PR #103, fixed under
> [plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md)
> *Sửa 5*, rows K and L,
> [ADR-0154](../decisions/ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md).

## What happened

[ADR-0153](../decisions/ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md)
stopped the engine thread joining a departing connection's journal writer: the journal is
*retired*, the writer finishes on its own time. Correct for the engine thread, and it opened a
window nobody had asked about: **between a session ending and its writer's last flush, the file
is still being written — and a counterparty that reconnects inside that window gets its
session resumed from it.**

The recovery (`Recovery::recover`, deployment code) calls `FileJournal::open`, which reads the
whole file to rebuild the ring and the three counts. Read while the old writer was still
flushing, it saw the file short:

- the reviewer's probe: *"reopen saw highest_out=Some(1), wanted Some(2)"*, 50 reconnects in 50;
- `crates/engine/tests/one_appender.rs::a_reconnect_resumes_from_the_finished_file`, red first:
  *"round 9: Logon went out as 34=18, 34=18 was already sent"*, between 73 and 94 misses in 50
  reconnects over four runs — a `Logon` under a number the counterparty had already seen, and
  a `ResendRequest` for messages it had already sent.

And for as long as both lived, **two writers appended to one file.** A record and its CRC are
two `write_all` calls, so their bytes could interleave. Before ADR-0153 the join in `Drop`
closed the window — by making the engine thread wait, which is what ADR-0153 removed.

The same hazard existed across **processes** from the start: two processes pointed at one
journal path both appended to it. `GUIDE.md` §6b said *"one journal, one process"*; nothing
held it.

## Why it was not seen

Every test that reopened a journal first dropped it plainly (a join) or waited for retired
writers. The window only exists for a retired journal reopened *immediately*, and the serving
loop's own tests never reconnect inside a millisecond. The writer sleeps 1 ms between empty
polls (ADR-0150 decision 4), so a record pushed just before the hang-up can take that long to
reach the file — longer than a loopback reconnect takes.

## What holds it now

1. **One appender per file, by the operating system.** `FileJournal::open` takes an exclusive
   `File::try_lock` (`flock(LOCK_EX|LOCK_NB)` on Linux) **before reading**. The lock lives with
   the `File`: the writer thread's under `Async`, the journal's under `Fsync`. Held → `open`
   fails at once with `ErrorKind::WouldBlock`, naming the path. It never waits.
2. **A recovery can say "not yet".** `Recovery::ready(&mut self, &Config) -> bool`, defaulted to
   `true`. First answered by `journal::file_busy(path)` — open, `try_lock`, unlock, close — which
   turned out to be a system call that can sleep on the engine thread; now answered by the
   writer's own `FileJournal::released()` flag, one atomic load
   ([the second page](a-readiness-probe-that-opened-a-file-on-the-engine-thread.md), ADR-0155).
3. **Not ready parks the connection.** In `pump` (the `serve*` loops) and on the sharded
   acceptor thread it is held beside the pre-session set; in `dial` it stays in the handshake
   slot. Asked again at most once per millisecond, never waited for; dropped after its
   `LogonTimeout`. `hft` keeps spinning while one is parked (test `a_parked_reconnect_does_not_slow_the_engine_thread`).

`WouldBlock` from `open` **does not mean "no history"**. A recovery that does not implement
`ready` and treats the error as "start fresh" would reset a session that has history.

## The second trap, found while fixing the first: a lock outlives its close

`flock` locks belong to the **open file description**, not to the descriptor. Closing a
descriptor releases the lock only if no other copy of that description exists — and one does,
briefly, whenever **any thread of the process spawns a child**: between `fork`/`clone` and
`exec` the child holds a duplicate of every descriptor (`O_CLOEXEC` closes them only at the
`exec`).

`[measured 2026-09-24]` with `file_busy` and the writer releasing by `close` alone,
`one_appender.rs` failed 2 runs in 15 the same way: `ready` (via `file_busy`) said the file was
free, and the `open` that followed on the same thread got `WouldBlock`. In the same binary a
sibling test spawns a child process, and in another run `serve` could not bind a port
`free_port` had just released. `[inferred]` a child's momentary copy of a descriptor that held
the lock — `file_busy`'s own, or the finishing writer's — kept it past the close. Explicit
unlocking and serialising the spawning and port-binding tests went in together; neither
failure was seen again in about 200 runs, so which of the two closed it is not separated.

**The rule: unlock explicitly before closing** (`File::unlock`, `LOCK_UN`), which releases the
lock for every copy of the description at once. `file_busy`, the writer's last act and
`FileJournal`'s `Drop` under `Fsync` all do. A child process that never `exec`s would otherwise
hold a journal's lock for its whole life.

The wire tests in `one_appender.rs` also take turns (a static mutex): a test that spawns a
process beside a test that binds a port is harness noise, not engine behaviour.

## What the file missed, counted

Row L, same review. `put` under `Async` pushed each record to the writer's 1 MiB ring with
`let _ = p.push(..)`: a full ring dropped the record and `put` still answered `true`. The probe:
writer asleep, 40 puts of 60 KB, 28 on disk, nothing counted
(`tests/journal.rs::a_full_ring_is_counted_not_silent`, red first: *"28 of 40 messages reached
the file and the journal counted 0 as unwritten"*). `true` stays — memory holds the message and
a resend while the process runs replays it — but `Journal::unwritten()` now counts every record
the ring refused, and the engine reports increases as `EventKind::JournalUnwritten { count }`.

## Guarded by

- `crates/engine/tests/one_appender.rs`: `a_journal_file_has_one_appender`,
  `a_second_process_cannot_append`, `a_reconnect_resumes_from_the_finished_file`,
  `ready_is_answered_by_the_writer_not_the_filesystem`, `a_parked_reconnect_does_not_slow_the_engine_thread`.
- `crates/engine/tests/journal.rs`: `a_full_ring_is_counted_not_silent`,
  `a_full_journal_ring_is_an_event`, `a_refused_prefix_retires_its_journal` (and its shard
  twin under `affinity`).

## What is still not proven

- The voluntary-switch test this page first named went red in 2 runs of ~300 on a loaded machine;
  it was replaced, and `file_busy` taken off the engine thread — see
  [a-readiness-probe-that-opened-a-file-on-the-engine-thread](a-readiness-probe-that-opened-a-file-on-the-engine-thread.md).
- `standard` re-asks a parked recovery only on its wakes, up to its poll timeout later than the
  writer needed. Unmeasured.
