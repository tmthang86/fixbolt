# ADR-0152 — Non-negotiable 4 judges the engine thread's serving window; its teardown may wait for its writers

- **Status**: **Accepted — 2026-09-23** (manager, owner's standing mandate, with plan Sửa 2). Proposed 2026-09-23. Written by the architect (Opus) for *Sửa 2* of
  [docs/plans/2026-09-23-phase-3-found-defects.md](../plans/2026-09-23-phase-3-found-defects.md).
  Accepting that revision accepts this ADR.
- **Date**: 2026-09-23
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md),
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md) (rule 4's wording, *"on the hot path"*),
  [ADR-0072](ADR-0072-a-tracer-free-check-that-the-hft-engine-thread-never-sleeps.md) (the
  tracer-free check, which already judges a window),
  [ADR-0150](ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md)
  (the writers, and `close()`, which joins them).

## Context

### 1. What was seen — `[measured 2026-09-23]` by the D5 builder, commit `bc98fcb` on `fix/d1-journal`

`W2W_EXTRA="--journal file-async --log file" scripts/check-no-kernel-sleep.sh` is **red**:
`FAIL: the engine thread slept in the kernel:  2 futex`, also under `--tls ktls`. Both
`FUTEX_WAIT_BITSET` calls come **after** the engine thread's last `recvfrom`/`sendto`/`accept4`,
and each is followed by an `munmap` the size of a ring (1 052 672 B, the journal's; 4 198 400 B,
the message log's): the engine thread dropping `FileJournal` and `FileLog`, whose `close()`
joins the writer thread. A w2w built from `016f2a5` (before D5) reads the same `2 futex` in 5 of
5 runs — **pre-existing**, unseen only because no mode script had ever run with writers
(`STATUS.md` *Not proven*).

The two other mode checks are green on the same binary: `check-no-kernel-sleep-by-ctxt.sh` reads
0 voluntary switches, `check-standard-gives-the-core-back.sh` passes.

### 2. Why one check is red and the other green — `[read in repo 2026-09-23]`

- `check-no-kernel-sleep.sh` runs w2w under `strace -f` and counts **every** syscall the engine
  tid ever made, from thread start to thread exit (`engine_syscalls`, script lines 129-155): setup,
  serving and teardown alike.
- `check-no-kernel-sleep-by-ctxt.sh` reads `voluntary_ctxt_switches`, sampled by w2w's main
  thread **right before and right after the timed window** (ADR-0072 decision 1). It has judged a
  window since it was written.
- Rule 4's own text (`CLAUDE.md` §2; ADR-0013 lines 45, 91) is *"never sleeps in the kernel **on
  the hot path**"*. Teardown is not on it.

### 3. Why the join is right — `[read in repo]`

`FileJournal::close` joins the writer so that *"everything accepted is on disk"*
(`crates/engine/src/journal.rs:770-781`); `FileLog` does the same. Not waiting — detaching the
writer — would let a process exit with accepted messages still in the ring: after a restart
they could not be replayed when the counterparty asks for a resend, only gap-filled. Joining from some other thread only moves the
wait, adds a thread the library must own, and changes nothing a counterparty can see.

No internet search was made: the question is what this repository's own rule covers, and no
outside source decides that.

## Decision

1. **Rule 4 governs the engine thread from the start of its serving loop to that loop's
   return.** Setup before it (opening files, building the engine, printing its tid) and teardown
   after it (dropping the engine, joining writers, closing files) may block, and teardown **must**
   wait for the writers to drain. This is what *"on the hot path"* already said; this ADR makes it
   checkable.
2. **The strace check judges that window and no more.** `tools/w2w`'s engine thread marks it with
   two syscalls nothing else makes — a metadata lookup of two sentinel paths that do not exist,
   `fixbolt-w2w-serve-open` right before the serving loop and `fixbolt-w2w-serve-close` right
   after it returns and **before anything is dropped** (an explicit `drop` after the second
   marker). The script matches the **path string**, not the syscall name (`statx` or
   `newfstatat` depending on the libc), and counts `SLEEPERS` and socket calls only between the
   two markers on the engine tid.
3. **Outside the window is printed, never hidden.** The script prints every sleeper it saw on the
   engine tid outside the window as *"outside the serving window, not judged: …"*.
4. **A missing or doubled marker is a failure**, not a green: *"FAIL: the serving window is not
   marked exactly once — nothing can be judged"*.
5. **Not cut at the last socket call.** A window defined by the events it judges would exclude a
   sleep that happened after the last message but still inside the loop — exactly a regression
   the check exists for.

## Options not taken

- **(b) Detach the writers.** Loses accepted messages at exit (§3). Rejected.
- **(b) Join the writers from another thread.** Moves the wait, adds a library-owned thread, buys
  nothing. Rejected.
- **`strace -e` filters or a time window.** Filters by syscall, not by phase; a time window
  depends on the machine's speed. Rejected.
- **Cut at the last socket call.** §5 above.

## Consequences

**Good**

- The strace check can be run with writers, so rule 4's two machine checks finally cover the
  deployment shape `Async` journals and message logs are (`STATUS.md` *Not proven*).
- The two machine checks for the `hft` half now judge comparable windows (serving loop vs timed
  loop, the second inside the first).
- Teardown's blocking is stated, printed on every run, and justified by durability rather than
  tolerated by accident.

**Bad, and these are the price**

- **Setup is no longer judged by the strace check.** It was judged before by accident and was
  clean; a sleeper added to setup would now be printed but not fail the gate.
- **The window depends on two lines in a tool**, not in the library. A w2w change that moved a
  marker inside the loop, or the engine's drop before the close marker, would change what is
  judged; the script's reversal (plan *Sửa 2*) is the guard.
- **Two syscalls are added to the engine thread**, once each, outside the loop — not on the hot
  path, and visible in the trace by name.
- `CLAUDE.md` §2's machine-check table note for rule 4 should say the strace check judges the
  serving window; that file is the owner's rules file and the manager edits it.
