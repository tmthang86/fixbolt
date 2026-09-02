# A corpus of expected outputs cannot be replayed as inputs

> `[measured 2026-09-02]` — found turning the mirrored acceptance corpus around, step 4 of
> [plans/2026-09-02-the-initiator-and-its-second-opinion.md](../plans/2026-09-02-the-initiator-and-its-second-opinion.md).
> **`[to testing-skills]`**

## The shape

A conformance corpus records, per step, *what goes in* and *what should come out*. Running it
from the other side looks like a free second test suite: swap the two, and the expected outputs
become the inputs.

It is not free, and the reason is in how the **comparator** works rather than in the protocol.

An expected output is only ever compared, never sent. So the fields a comparator matches
**loosely** — by shape, by presence, by type — never had to carry a real value. In this corpus:

| Field | What the expected lines carry | What the comparator does with it |
|---|---|---|
| `52` `SendingTime` | `00000000-00:00:00.000`, in **every** expected line | matches by shape |
| `10` `CheckSum` | the literal `0` in 238 of 244 | matches the **received** value |

Both are legal as expectations. Both are **impossible as inputs**: a timestamp in the year 0 is
2 026 years of clock skew, which a correct session refuses before it looks at anything else in
the message, and a two-character checksum is not a checksum.

## The measurement, and why it was invisible for three days

Mirrored, the counterparty's `Logon` is one of those lines. The session refused it and dropped
the link, so **every later line of every file** reported *"expected a message, got silence"* —
196 failures across 50 files, all with the same message, none of them naming the cause.

The score had been **0 / 50** since 2026-08-30 and the number was *correct* — for a different
reason than the one everybody had written down. The recorded reason was that 46 of the 50 files
need the engine to originate a message no state machine can invent, which is true and was the
harder problem. It was fixed first. The score stayed at 0.

```
0 / 50
  10_MsgSeqNumEqual.def:8   expected a message, got silence
  10_MsgSeqNumEqual.def:9   expected a message, got silence
  …
  harness originated: 0×1
```

**`harness originated: 0×1`** is the line that gave it away. The harness had just been taught to
originate on demand and had been asked exactly **once** in fifty files — because everywhere else
the connection was already gone. A counter of *what the test rig itself did* found a defect that
196 assertion failures had not.

With the two fields made real — nothing else changed, no session code touched — the same run:

```
2 / 50
  10_MsgSeqNumEqual.def:8   FieldCount { expected: 10, actual: 8 }
  10_MsgSeqNumEqual.def:9   Value { at: 1, tag: 9, expected: 62, actual: 61 }
  …
  harness originated: 0×48 1×24 2×10 4×10 5×38 app×49
```

179 originations instead of 1, and the failures are now **protocol disagreements with values in
them** instead of one silence repeated.

## The lesson, stated without FIX

**A recorded expectation is not a recorded message.** Anywhere a comparator is lenient, the
recorded value is free to be a placeholder — and it usually is, because nobody had a reason to
make it real. Replaying that recording as input feeds the system under test something no real
peer would ever send.

The three parts worth keeping:

1. **Before reversing a corpus, list every field the comparator treats loosely.** That list is
   exactly the set of fields that may be fiction. It is short, it is knowable up front, and it
   is cheaper to read the comparator than to debug the replay.
2. **Instrument the harness, not only the system.** Every assertion here failed with the same
   message, and the thing that identified the cause was a counter of *how many times the harness
   was asked to act*. When N identical failures tell you nothing, count what the rig did.
3. **A correct number can have a wrong reason, and the wrong reason survives being written
   down.** `0 / 50` was documented with a cause that was real, was the harder problem, and was
   not the blocker. Fixing it changed nothing, and *that* — a fix that moves no number — is the
   signal that the recorded cause is not the whole cause.

## Where it is guarded

`fixbolt_conformance::script::make_receivable`, whose doc comment names both fields and says why
each is substituted. The reversal: removing it takes the mirrored score from 2 / 50 back to
0 / 50 and the drive counts from 179 back to 1 — and `crates/session/tests/mirror.rs` asserts
both numbers, so it goes red on either.

**It substitutes exactly two fields and nothing else.** A message whose `52=` is a *deliberately*
wrong real value — `1d_InvalidLogonBadSendingTime` sends one from 2001 — keeps it, because it is
not the placeholder. A loader that "fixed up" that one too would have quietly deleted the case
the file exists for.
