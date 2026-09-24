# An unsubmitted `RecvMulti` SQE can land on a reused descriptor

`[2026-09-24]` phase 4 row 5, the `io_uring` transport plan, named in step 2's commit and
guarded before it could show as a bug — [ADR-0190](../decisions/ADR-0190-the-io-uring-transport-is-reaped-by-the-idle-strategy-and-an-hft-turn-enters-the-kernel-once-without-waiting.md)
decision 1, decision 8.

A `RecvMulti` submission queue entry names a raw file descriptor **number**, not the object
behind it. If a connection is dropped and its socket closed while a `RecvMulti` for it is still
only *queued* — pushed onto the submission queue (`squeue::Entry`) but not yet handed to
`io_uring_enter` — the kernel is free to reuse that descriptor number for a fresh `accept4`
before this module ever submits the stale entry. Submitting it after that arms a receive on
**someone else's socket**: every byte the new connection sends would look, to this module's
ledger, like a receive for the slot that used to own that descriptor number.

## Why it cannot happen here

The descriptor a `RecvMulti` entry names is held open by its `UringTransport` until *after*
`Drop` has flushed the queue. `UringTransport::drop`
(`crates/engine/src/transport/uring.rs`) calls `shutdown(SHUT_RDWR)` first — ending the pending
multishot `recv` and putting the peer's FIN on the wire — then queues an `ASYNC_CANCEL` for that
`recv` and flushes the submission queue; only once that method body returns does the `tcp` field
drop and actually close the descriptor. So a queued `RecvMulti` for a slot is always submitted,
one way or another — as a live receive or as the target of a cancel — before the descriptor it
names can be closed and its number handed to a new connection. `Inner::push`'s `SAFETY` comment
names this explicitly as what makes its `unsafe` sound.

The generation carried in every `user_data` (32 bits beside the slot index) is the second,
independent line of defence: a completion that still arrives for a slot whose generation has
since moved on is discarded, never delivered to whatever now occupies that slot
(ADR-0190 decision 1).

## Guard

- `crates/engine/tests/uring.rs::a_ring_that_runs_out_of_buffers_rearms_and_loses_nothing` —
  drives fast re-arm and drop churn across the submission-queue path this trap sits on.
- `crates/engine/tests/uring.rs::a_late_completion_for_a_dropped_connection_reaches_nobody` —
  drops a connection with bytes already staged, gives its slot to a fresh connection, and
  asserts the new connection's bytes are never contaminated while a stale completion is counted
  (`UringReport::stale > 0`), never delivered.
- The ordering itself — `shutdown`, then cancel, then flush, then close — is enforced by field
  order and an explicit `Drop`, not by a runtime check. There is no dedicated reversal for it in
  the plan's step 7 list: breaking Rust's own field-drop order is not a change step 7's
  reversals can express by editing the transport's logic.
