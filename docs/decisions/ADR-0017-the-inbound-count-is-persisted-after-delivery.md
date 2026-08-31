# ADR-0017 — The inbound count is persisted after delivery, not before

- **Status**: **Accepted — 2026-08-31**, chosen by the owner.
- **Date**: 2026-08-31
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0008](ADR-0008-journal-is-a-trait.md),
  [ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md),
  [DESIGN.md §4 D1 and D7](../DESIGN.md),
  [plans/2026-08-30-session-recovery.md](../plans/2026-08-30-session-recovery.md)

## Context

[ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md) made a session able to keep counting across
a restart, and step 4 of the recovery plan built the outbound half: `Session::resume` takes the
numbers, and the engine recovers `next_out` from the journal's highest written sequence.

**The inbound half has no home.** The journal holds messages this end *sent*, because that is
what a `ResendRequest` needs. Nothing records which inbound sequence numbers have been consumed,
so a resumed session cannot know what it has already seen.

Making it durable is easy. **Deciding when to write it is not**, and the plan that asked for the
work stated only one half of the reason:

> `next_in` must be persisted **before** the application sees the message — otherwise an
> ill-timed crash reprocesses a message that was already processed.

That sentence is true and it is half the picture. The two orderings do not trade a failure for
safety; they trade one failure for a different one.

| `next_in` written | An ill-timed crash means | What the counterparty sees |
|---|---|---|
| **before** delivery | the message is **lost** — this end already counted it, so it never asks for a resend | nothing; it believes we received it |
| **after** delivery | the message is **processed twice** after the restart | a `ResendRequest`, answered with the message carrying `43=Y` |

## Decision

**1. `next_in` is persisted after the application has seen the message.**

Concretely, at the end of `Session::received_with`, once `judge` and the held-message drain have
both finished, so it covers a message delivered directly and one released when a gap closed.

**2. The protocol has a mechanism for the failure this chooses, and none for the other.** A
message delivered twice arrives the second time with `PossDupFlag` set, because the second copy
comes from a `ResendRequest` this end issued. An application that cannot tolerate a duplicate has
a flag to key on. A message that was silently skipped leaves nothing to key on anywhere — both
ends believe it was delivered, and the discrepancy surfaces at reconciliation, hours later, as a
break with no trace of its cause.

**3. It is what QuickFIX does**, which matters here because [ADR-0001](ADR-0001-relationship-to-quickfix.md)
makes QuickFIX this project's behavioural oracle: it advances the target sequence number after
the message has been passed to the application, not before.

**4. The `Journal` trait carries it**, with no default method — the same reasoning that
`Journal::highest` was given no default. A default returning `None` would let a journal that is
holding state report that it holds none, and a session resumed from it would silently start over.

## Consequences

**Good**

- The failure mode this accepts is one FIX was designed to express. The one it rejects is
  invisible on the wire.
- It agrees with the oracle, so an interop run against `libquickfix` is testing the same
  contract on both sides rather than two different ones.
- Recovery for a resumed session is now complete: outbound from the journal's highest record,
  inbound from its highest mark.

**Bad, and stated rather than found later**

- **A durable write lands on the inbound path.** Under `Durability::Fsync` that is a `sync_data`
  per inbound message, which is the dominant cost of receiving one. `Async` and `NoJournal` are
  unaffected in kind, and this ADR does not change which tier anybody chooses — but it does mean
  `Fsync` now costs on both directions where it used to cost on one. **Nothing here measures
  that**, and the number belongs in `reference/measured-costs.md` when somebody takes it.
- **Duplicate delivery is now a documented possibility rather than an impossibility**, and it is
  a constraint the type system cannot enforce. `GUIDE.md` has to carry it: an application behind
  this engine must be idempotent per sequence number, or must key on `43=Y`.
- **The window is not closed, only moved.** A crash between the application seeing the message
  and the mark being written still reprocesses. That is inherent — there is no atomic step that
  spans an external application's side effects and this engine's disk — and pretending otherwise
  would be the more dangerous claim.
- **The acceptance corpus cannot see any of this.** No `.def` file restarts a process. Every test
  behind this decision is one this project invented, and a test invented to agree with a rule
  invented in the same session is a guess written down twice. The guard against that is the
  reversal discipline and the fact that the ordering agrees with an external implementation, not
  the fact that the tests pass.

## Alternatives considered

**Write before delivery**, as the plan originally said. Rejected on the asymmetry above: it
converts a recoverable duplicate into an unrecoverable loss, and for order flow those are not the
same size of mistake.

**Make it a policy the embedder picks**, like `Durability`. Rejected as a third configuration
axis on the session layer, another dimension to test in a suite already carrying two modes, and
one more constraint in `GUIDE.md` that the compiler cannot check — for a choice where one option
is defensible and the other is not.

**Write the mark before delivery and a completion record after.** A two-phase mark closes the
window properly and costs a second durable write per message on the hot path, to fix a failure
mode the protocol already has a flag for. Not worth it here; it is what a system without
`PossDupFlag` would have to do.
