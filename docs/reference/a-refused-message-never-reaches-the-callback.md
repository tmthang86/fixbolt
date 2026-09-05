# A refused message never reaches the callback you are watching

> `[measured 2026-09-05]` — found while building
> [reconnect-interop](../plans/2026-09-05-reconnect-interop.md), by an assertion that read
> *"nothing reached the second acceptor"* about a message that had reached it.
> **`[to testing-skills]`**

## The finding

The gate stands a counterparty up, kills it, starts it again, and asks: **did the engine under
test come back?** The counterparty is a real `libquickfix` acceptor whose whole transcript is
written from its application callbacks — `fromAdmin`, `fromApp`, `toAdmin`, `toApp` — so every
message it handles appears as `acceptor: in` or `acceptor: out`.

The assertion was the obvious one:

```bash
logon_in="$(grep -E '^acceptor: in ' "${A2}" | grep -F '|35=A|' | head -1)"
```

It read `FAIL  nothing reached the second acceptor`. The engine **had** come back — twenty-two
times, on its backoff ladder — and the transcript proved it, on lines the assertion was not
looking at:

```
acceptor: out 35=5 ... 58=MsgSeqNum too low, expecting 4 but received 3
```

You cannot answer *"your sequence number is wrong"* to a message that did not arrive. The Logon
arrived, the counterparty's **session layer** judged it, refused it, and answered it — and
`fromAdmin` was never called, because a session that rejects a message does not hand it to the
application. The gate was watching the application seam and the event happened one layer below it.

## Why the failing message is the one you are least likely to see

This is not a quirk of QuickFIX. It is the shape of every layered protocol implementation: the
session layer exists precisely to keep malformed, mis-sequenced and unauthenticated traffic away
from the application. So the **application callbacks are a filtered view, and what they filter out
is exactly the failure cases** — which are what a gate about failure is trying to observe.

The consequence is worse than a missing line, because the same silence has two causes:

| What actually happened | What an application-seam transcript shows |
|---|---|
| nothing connected at all | nothing |
| something connected and was refused | nothing |

Two different results, one observation. A gate that cannot tell them apart will report the wrong
cause with complete confidence — and here it did, in the first draft.

## What told them apart

Nothing was added to the counterparty. **A second observation point already existed in the same
file**: what the counterparty *sent*. A refusal is itself an outbound message, and it names both
numbers:

```bash
arrived="$(grep -c -E 'MsgSeqNum too low|^acceptor: in .*\|35=A\|' "${A2}")"
```

*Arrived* is now *"accepted, or refused"* — a union of the two seams — and the two causes are
distinguishable again. The reversals confirm it reads what it claims: a policy that will not
redial inside the deadline leaves **zero** of those lines and `no_resend` **green**, while a
recovery that comes back with the wrong number leaves twenty-two of them and `no_resend` **red**.
Two different failures, two different signatures.

## The rule

**Before asserting "X never arrived", ask what the observation point can see.** An assertion
written against an application callback is an assertion about *accepted* traffic, and reads as an
assertion about *all* traffic. The failure it is most likely to be pointed at is the one it is
structurally blind to.

The cheap check: for every "nothing happened" assertion, name the thing that would have been
written **if something had happened and been rejected**. If the answer is "the same nothing", the
assertion has two causes and needs a second seam before it means anything.
