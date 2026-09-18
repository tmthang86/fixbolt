# ADR-0072 — A tracer-free check that the `hft` engine thread never sleeps: voluntary context switches

- **Status**: **Accepted — 2026-09-18**, by the plan [closing-the-open-items](../plans/2026-09-18-closing-the-open-items.md), approved by the manager under the owner's 2026-09-18 mandate, with decisions 1 and 4 revised to the gate as built (PR #78 senior review). Proposed the same day.
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: `CLAUDE.md` §2 non-negotiable 4 and its *Machine checks* row 4,
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md), `DESIGN.md` §6 *Mode and machine*,
  [reference/a-traced-process-gets-no-file-capabilities.md](../reference/a-traced-process-gets-no-file-capabilities.md),
  [plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md)
  Sửa 2 Điều 1 option R5, `STATUS.md` open item 86

## Context

Non-negotiable 4's `hft` half is machine-checked by `scripts/check-no-kernel-sleep.sh`, which
runs `tools/w2w` under `strace -f` and asserts the engine thread made zero of `epoll_wait`,
`poll`, `select`, `futex`, `nanosleep`, `sched_yield`. It is a good check with one hole: a
process exec'd under an unprivileged tracer gets no file capabilities, so the arm that opens a
raw socket on a real NIC cannot run under it. The workaround (a user namespace) sees only a
private `lo`. So the one place non-negotiable 4 matters most — the engine on the hardware NIC
at the §9 desk — has no machine check for it.

The kernel already keeps the number this gate wants. `/proc/<pid>/task/<tid>/status` carries
`voluntary_ctxt_switches` and `nonvoluntary_ctxt_switches` per thread (proc_pid_status(5),
*"Number of voluntary and involuntary context switches (since Linux 2.6.23)"*; proc_pid_task(5):
*"various fields in each of the task/tid/status files may be different for each thread"*). A
voluntary switch is counted when the scheduler is entered with the task no longer runnable —
every blocking syscall, a futex wait, a page fault that waits for I/O. A thread that spins on a
non-blocking `recv` and never blocks reads **0**; `sched_yield` and a preemption count on the
involuntary side, which is the noise column, not the sleep column.

## Decision

1. **`tools/w2w` reads the engine thread's two counters before the timed window and after the
   `--hold-ms` idle window** — from `/proc/self/task/<tid>/status`, on the main thread, so the
   engine thread makes no new syscall — and prints `engine-ctxt voluntary <n> involuntary <m>`
   beside `allocs`. `[revised 2026-09-18, PR #78 senior review]` **The assertion is opt-in:
   `--assert-no-voluntary-switches` makes the run fail if voluntary is non-zero, in whatever
   mode is given.** It is opt-in because a tracer inflates the column — under `strace` every
   `PTRACE_CONT` resume is a voluntary switch — so the `strace` gate must never carry the flag,
   and a `standard` run given the flag is the reversal (it must fail). The second sample is
   taken after the hold, not at the end of the timed window, so that a `standard` engine has
   had its idle time to block in and the red half is deterministic.
2. **A new script, `scripts/check-no-kernel-sleep-by-ctxt.sh`, is the second machine check
   for row 4**: it runs `w2w --mode hft` and asserts voluntary reads 0, then runs `--mode
   standard` and asserts voluntary reads **> 0** — the reversal is built in, as the `strace`
   script's is. It needs no tracer and no capability, so it runs on a real NIC with
   `--wire-timestamps`, in CI, and inside the §9 procedure.
3. **The `strace` check stays.** It names *which* syscall slept; the counter only says that
   something did. Both run in CI; the `strace` one keeps its user-namespace arm on `lo`, the
   counter one takes the NIC arm.
4. **`scripts/w2w-baseline.sh` passes `--assert-no-voluntary-switches` to every `hft` arm of a
   combined run and records both counters in every run's output**, so every published `hft`
   figure from now on is gated on *engine thread: 0 voluntary switches through the hold* — the
   sentence `CLAUDE.md` §2 rule 4 asks a figure to state. `standard` arms record the counters
   and are not gated by this flag (their gate is the four-assertion script).

## Consequences

**Good**

- Non-negotiable 4 gets a machine check on the hardware NIC, the place it was blind.
- The check is cheap enough to be always on, so it also covers arms nobody thought to trace:
  TLS, the journal writer, the paced runs.
- A gate that cannot be defeated by the tracer stripping capabilities cannot be silently
  SKIPPED either; there is no exit-2 branch to forget.

**Bad — and accepted**

- **The counter does not name the syscall.** A red run says *it slept*, and the `strace`
  script has to be run on `lo` to find out where. Two gates for one rule is more surface.
- **Zero voluntary switches is necessary, not sufficient**: a syscall that returns at once
  without blocking (a `poll` with timeout 0, a `futex` wake) does not count and is not a sleep,
  so this gate rightly ignores it — but a reader may take *0* for *no syscall*, which it is not.
- **Involuntary switches on an isolated core should also be zero and are not asserted**; they
  are printed. Asserting them would turn a §9 tuning fault into a gate failure on a laptop.
- **The reversal depends on the `standard` engine having blocked at least once before the
  second sample**; sampling after the `--hold-ms` window guarantees an idle stretch in which it
  must, so the red half is deterministic rather than a race with the timed window.
- **Two traps, paid for while building it** (`[measured 2026-09-18]`): **`sched_yield` lands on
  the *involuntary* column** — the task stays runnable, so `__schedule` counts it on `nivcsw` —
  which is why the gate reads the voluntary column only and why a `yield`-style spin cannot be
  caught by it (the `strace` gate names `sched_yield` for that); and **a tracer inflates the
  voluntary column** — under `strace` the tracee stops and is resumed by `PTRACE_CONT` on every
  syscall, each stop a voluntary switch — which is why the flag is opt-in and the `strace`
  script never passes it.

## Sources

- proc_pid_status(5) — <https://man7.org/linux/man-pages/man5/proc_pid_status.5.html>;
  proc_pid_task(5) — <https://man7.org/linux/man-pages/man5/proc_pid_task.5.html> (read
  2026-09-18).
- `kernel/sched/core.c`, `__schedule`: the switch is counted on `prev->nvcsw` when the previous
  task is not runnable and on `prev->nivcsw` otherwise (mainline, read 2026-09-18).
- [a-traced-process-gets-no-file-capabilities.md](../reference/a-traced-process-gets-no-file-capabilities.md)
  `[measured 2026-09-14]`.
