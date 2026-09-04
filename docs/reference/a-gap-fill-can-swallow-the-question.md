# A gap fill can swallow the question the test was waiting for

> `[measured 2026-09-04]` — the **first run** of
> [tools/interop/initiator.cpp](../../tools/interop/initiator.cpp) against this engine's
> acceptor, step 2 of
> [plans/2026-09-03-acceptor-interop.md](../plans/2026-09-03-acceptor-interop.md).
> `FAIL 6/7`, and the failing step was correct behaviour on both sides.
> **`[to testing-skills]`**

## The step, as written

Open a sequence gap on purpose and prove the peer survives it:

1. jump this end's outbound sequence number forward by three, saying nothing;
2. send a `TestRequest` carrying `112=QF-TR-2` — now numbered three ahead;
3. expect the peer to notice and ask for what it missed (`35=2`);
4. expect an answer to `112=QF-TR-2`, because *a peer that merely noticed the gap and then
   dropped the link would not answer*.

Step 4 is the load-bearing one. "A `35=2` arrived" says nothing about whether the session is
still usable afterwards, which is the thing being tested.

## What happened

```
out 35=1 34=10 112=QF-TR-2               the gap-causing TestRequest
in  35=2 34=6  7=7 16=0                  the peer asks for 7 onward — correct
out 35=4 34=7 43=Y 36=11 123=Y           the gap fill: 7 THROUGH 10
```

**`36=11` covers sequence 10, and sequence 10 is the `TestRequest` itself.** The test's own
engine, answering the resend request automatically, told the peer to skip the message the test
was waiting for a reply to. The peer discarded it — which is the only correct thing to do with
a message the counterparty has just declared administratively absent — and the step went red
against behaviour that was right on both sides.

Everything after it in the transcript shows the session was fine: heartbeats continued in both
directions for another eight seconds, and the logout exchange completed.

## The shape

**The stimulus that creates the fault can be consumed by the recovery from that fault.**

The test used one message for two jobs — *cause the gap* and *ask the question* — and the
recovery mechanism is entitled to erase everything in the gap, question included. Whether it
does is a property of the peer's engine, not of the system under test, so the step's result
depended on a third party's implementation choice.

The fix is to separate the jobs: the gap-causing message is spent, and survival is proven by a
**fresh** message sent *after* the recovery completed.

```
out 35=1 34=10 112=QF-TR-2               spends itself opening the gap
in  35=2       7=7 16=0                  assertion 1: the peer asked
out 35=4       36=11 123=Y               recovery, whatever it swallows
out 35=1       112=QF-TR-3               assertion 2's question, after recovery
in  35=0       112=QF-TR-3               the session is usable
```

Both assertions now name something the recovery cannot retract. `interop-acceptor: PASS 7/7`.

## How to spot this class before it costs a run

A step that asserts on **a reply to the message that broke things** is the tell. If the protocol
has any recovery that can skip, batch, discard, or replace pending work — gap fills, retries
with a new id, queue drains, idempotency dedupe — the broken message is exactly the one that
recovery is allowed to make disappear. Ask the question again afterwards; it costs one message.
