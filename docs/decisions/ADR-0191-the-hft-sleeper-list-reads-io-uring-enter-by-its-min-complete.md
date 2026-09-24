# ADR-0191 — The `hft` sleeper list reads `io_uring_enter` by its `min_complete`

- **Status**: Accepted — 2026-09-24, by the manager under the owner's delegation of 2026-09-18.
  Written by the architect (Opus) for phase 4 row 5
  ([plan](../plans/2026-09-24-p4-io-uring-transport.md)).
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the owner or the manager.
- **Related**: `CLAUDE.md` §2 non-negotiable 4 and its *Machine checks* row;
  `DESIGN.md` §6 (*The engine thread never sleeps in the kernel (`hft`)*), D8;
  [ADR-0072](ADR-0072-a-tracer-free-check-that-the-hft-engine-thread-never-sleeps.md) (the
  tracer-free second check); [ADR-0152](ADR-0152-non-negotiable-4-judges-the-engine-threads-serving-window-and-its-teardown-may-wait-for-its-writers.md)
  (only the serving window is judged); [ADR-0190](ADR-0190-the-io-uring-transport-is-reaped-by-the-idle-strategy-and-an-hft-turn-enters-the-kernel-once-without-waiting.md)
  decision 3 (why `hft` enters the kernel through `io_uring_enter`).

## Context

`scripts/check-no-kernel-sleep.sh` judges non-negotiable 4's `hft` half by tracing `tools/w2w`
with `strace -f` and failing on any **name** in `SLEEPERS`; `io_uring_enter` is one of them
(line 116). That was right while nothing here used `io_uring`: every `io_uring_enter` an engine
could make was a mistake. ADR-0190 decision 3 makes one deliberate: `hft` over `io_uring` enters
the kernel once per idle turn with `min_complete = 0` and `IORING_ENTER_GETEVENTS`, which runs the
ring's deferred work and **returns without waiting** (`io_uring_enter(2)`: `GETEVENTS` waits for
`min_complete` events). The same syscall with `min_complete ≥ 1` is exactly the wait `standard`
must make. Unlike `poll` with a timeout of 0 — which the list also names and which nobody here
calls — this is a syscall whose sleeping is decided by one argument.

The rule being checked is *"never sleeps in the kernel"* — not *"never enters it"*: `hft` has
always made a non-blocking `recvfrom` per socket per turn and a `sendto` per reply, and the check
has always allowed them (`DESIGN.md` §6: *"`accept4`, `recvfrom`, `sendto` and zero of …"*).

## Research

- [`io_uring_enter(2)`](https://man7.org/linux/man-pages/man2/io_uring_enter.2.html): the
  signature is `io_uring_enter(fd, to_submit, min_complete, flags, sig, sz)`; with `GETEVENTS`
  the call waits for `min_complete` completions; it may return `EINTR` *"while waiting for
  events"*. With `min_complete` 0 there is nothing to wait for.
- `strace` prints the six arguments positionally, the flags decoded (e.g.
  `io_uring_enter(5, 0, 0, IORING_ENTER_GETEVENTS, NULL, 8) = 0`); under `-f`, a call interrupted
  by another thread's output is split into an `<unfinished ...>` line carrying the arguments and a
  `<... io_uring_enter resumed>` line carrying the result.
- [ADR-0072](ADR-0072-a-tracer-free-check-that-the-hft-engine-thread-never-sleeps.md): voluntary
  context switches are the tracer-free referee — a call that returns at once moves nothing there;
  one that waits does.

**Searched, found nothing** on another project gating `io_uring_enter` by argument; the
alternatives were reasoned, not borrowed.

## Decision

1. **`io_uring_enter` is judged by its third argument.** `engine_syscalls` emits it as
   `io_uring_enter_nowait` when `min_complete` parses as `0`, `io_uring_enter_wait` when it parses
   as anything else, and `io_uring_enter_unparsed` when it cannot be parsed. `SLEEPERS` names
   `io_uring_enter_wait` and `io_uring_enter_unparsed` in place of `io_uring_enter`. **An argument
   the script cannot read fails the run** — the gate fails closed.
2. **The classification is made on the line that carries the arguments** — a whole call or its
   `<unfinished ...>` half; a `<... resumed>` half is not counted a second time.
3. **Every existing run is unchanged.** No current arm makes an `io_uring_enter`, so the renaming
   moves nothing in them; the kernel arms' green and red halves read exactly as today.
4. **New runs, each read back, never assumed**: `--mode hft --transport uring` must pass **and**
   show `io_uring_enter_nowait` > 0 (the ring path ran); `--mode standard --transport uring` must
   trip it with `io_uring_enter_wait`; `--mode hft --transport uring --uring-arm sqpoll` runs only
   where a core for the SQ thread is given (`FIXBOLT_SQPOLL_CORE`), and otherwise prints
   *SKIPPED, NOT PASSED*. Each run must print w2w's `transport: uring …` line with a completion
   count above zero, or it fails — an arm only assumed to have run is the kernel arm.
5. **Proven by reversal**, the expected sentence written before running: `UringSpin` changed to
   `min_complete = 1` must fail the green half with
   `FAIL: the engine thread slept in the kernel:` naming `io_uring_enter_wait`; a trace line
   doctored to an unparseable third argument must fail with `io_uring_enter_unparsed`.
6. **`DESIGN.md` §6's row and `CLAUDE.md` §2's *Machine checks* note are updated in the same
   commit** (`CLAUDE.md` §4: a gate's measurement changed).

## Alternatives considered

- **Keep `io_uring_enter` a sleeper by name and allow `hft` over `io_uring` only with SQPOLL.**
  Rejected: that makes the one arm the owner said must never be a default (Q8) the only legal
  `hft` arm, and it burns a second core for every engine.
- **Exempt `io_uring_enter` entirely and rely on the voluntary-switch count.** Rejected: ADR-0072
  itself says zero voluntary switches is *"necessary, not sufficient"*, and a wait that happens to
  find a completion already posted does not switch — a `min_complete = 1` bug would pass on a busy
  run and be seen only on a quiet one.
- **Decode the flags too** (fail only on `GETEVENTS` with `min_complete ≥ 1`). Rejected as
  needless: without `GETEVENTS`, `min_complete` is ignored, so a non-zero value with no
  `GETEVENTS` is odd but not a sleep — and failing on it costs nothing, because no code here
  writes it.

## Consequences

**Good**

- The rule is checked as written — *sleeps*, not *enters* — for the one syscall where the two
  differ by an argument.
- The gate fails closed on a trace it cannot read.

**Bad**

- **The gate now depends on `strace`'s argument format**, not only on syscall names. A future
  `strace` that prints `io_uring_enter` differently turns every uring arm into
  `io_uring_enter_unparsed` — red, not green, which is the safe direction, but a red that is
  about the tracer.
- **A sleeper hidden behind a non-waiting `io_uring_enter` is invisible to this script** — for
  example work the kernel does inside the call that blocks on a lock. The ctxt check (ADR-0072)
  still sees a voluntary switch if it sleeps; neither sees a spin inside the kernel.
- The SQPOLL arm is judged on the desk only; a runner without a core to spare skips it, loudly.
