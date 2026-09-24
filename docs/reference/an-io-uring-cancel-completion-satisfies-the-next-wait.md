# An `io_uring` cancel's completion satisfies the next wait

`[2026-09-24]` found by step 6 of
[the `io_uring` transport plan](../plans/2026-09-24-p4-io-uring-transport.md) (phase 4 row 5),
recorded as [ADR-0190](../decisions/ADR-0190-the-io-uring-transport-is-reaped-by-the-idle-strategy-and-an-hft-turn-enters-the-kernel-once-without-waiting.md)
Revision 2, R5.

**A waiting `io_uring_enter(min_complete = 1)` returns at once if any completion is already in
the CQ — including the completion of a cancel you submitted for your own bookkeeping.** An engine
that cancels its unfired polls *after* a wait and lets the next wait submit those cancels wakes
itself every turn: a `standard` engine that spins, which non-negotiable 4 forbids, with every
message still answered and all 59 definitions still green.

## The mechanism

1. `UringBlock` arms a one-shot `POLL_ADD` per extra source (listener, waker pipe) before each
   wait, and queues an `ASYNC_CANCEL` for every one that did not fire right after it (ADR-0190 R2).
2. As first built, those cancel SQEs sat in the SQ until the **next** turn's
   `io_uring_enter(to_submit, 1, GETEVENTS | EXT_ARG)`. That call submits them first; the cancel
   and the cancelled poll (`-ECANCELED`) each post a completion; `io_uring_enter(2)`: *"If
   min_complete is a non-zero value, the kernel will still return immediately if any completion
   events are available."*
3. The wait returns without sleeping, the turn finds nothing to do, re-arms the polls, cancels
   them again — and the next wait is satisfied the same way. Each turn feeds the next.

## How it showed, and what the kernel arm read

`[measured 2026-09-24]` `scripts/check-no-kernel-sleep.sh` over one `tools/w2w` run: the
`standard` + uring red half printed **7 385 `io_uring_enter_wait`** where the kernel arm's
`standard` red half prints **~345 `poll`**. The red half still passed — a wait *is* a sleeper name
— so the gate stayed green; the symptom was the **count**, twenty times the kernel arm's, read by a
person. `scripts/check-standard-gives-the-core-back.sh` would have caught it on CPU; the strace
count caught it first.

## The fix

Submit the cancels and reap their completions **in the turn that queued them**, with one extra
`io_uring_enter(n, 0, GETEVENTS)` that does not wait, before returning from `idle`
(`Inner::reap_standard`, `crates/engine/src/transport/uring.rs`). After it the same gate read
**356 `_wait` against 345 `poll`**. A completion that still arrives late wakes one wait once and
cannot feed itself, because each turn cancels only its own polls. Cost: one non-waiting syscall per
`standard` idle turn that had an unfired poll.

The general form: **anything you submit for bookkeeping produces a completion, and a completion
is a wake.** Reap it in the turn that caused it, or it becomes the next turn's reason to wake.

## Guards

- **`crates/engine/tests/uring.rs::standard_with_a_quiet_listener_waits_out_its_timeout_every_turn`**
  — red first on the built code with *"turn 1 returned after 6.051µs against a 50 ms timeout"*;
  green after the fix. This is the assertion that fails.
- **`scripts/check-no-kernel-sleep.sh`, `standard` + uring red line** — prints the
  `io_uring_enter_wait` count beside the kernel arm's `poll` count. **It is read, not asserted**:
  the script requires the red half to trip, not how often. A count far above the kernel arm's is
  this trap's signature.
- **`scripts/check-standard-gives-the-core-back.sh`, uring arm** — engine-thread CPU and sleeping
  state over the window.
