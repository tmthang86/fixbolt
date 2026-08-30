# ADR-0006 — The mirrored corpus is 50 of 59, not 51

- **Status**: Accepted — 2026-08-30
- **Date**: 2026-08-30
- **Deciders**: Tran Manh Thang
- **Supersedes**: [ADR-0004](ADR-0004-bidirectional-engine.md) **decision 6 only**. Every
  other decision in ADR-0004 stands unchanged, including decision 5 — interop against
  `libquickfix` remains the initiator's primary gate.
- **Related**: [DESIGN.md §7 step 5](../DESIGN.md),
  [plans/2026-08-29-session-initiator.md](../plans/2026-08-29-session-initiator.md)

## Context

ADR-0004 decision 6 makes the mirrored acceptance corpus the initiator's secondary gate, and
gives a mechanical criterion rather than a list:

> a definition mirrors when every one of its `I` lines is something a correct initiator would
> actually send, i.e. it begins with `8=FIX.4.4`, every tag is numeric, and no field is empty.

It then reports **51 of 59**, and names the eight that fail.

Implementing that criterion (`crates/conformance/src/script.rs::mirrors`) reproduced the eight
names exactly — the criterion is right about what it looks at. `[measured 2026-08-30]` the
excluded set computed from the rule equals the ADR's list, name for name, and a test asserts
that rather than a hand-copied array.

**But the criterion looks only at message lines.** A `.def` file also carries directives, and
one of them mirrors into something no initiator does.

`1b_DuplicateIdentity.def` ends:

```
e2,DISCONNECT     # the acceptor is expected to drop the second connection
i1,DISCONNECT     # the harness then hangs up the first
```

Mirrored, `i1,DISCONNECT` becomes `e1,DISCONNECT`: **this engine** is expected to hang up
connection 1. Nothing on the wire asked it to — no message arrived on that connection, no
timer fired, the counterparty did not close. In the original the line is the harness tidying
up after itself, which is not a protocol rule in either direction.

`[measured 2026-08-30]` `1b_DuplicateIdentity.def` is the **only** file in the 59 with an
`iDISCONNECT`. Every other file's disconnect is an `eDISCONNECT`, which mirrors the other way
— into an input, which is exactly right: an initiator whose counterparty hangs up is an
ordinary thing.

The file's subject is also acceptor-side by nature: policing which connection owns an
identity. That is the same reason the other nine are excluded, arrived at from the other end.

## Decision

**The criterion gains a fourth clause, and the number becomes 50.**

A definition mirrors when:

1. every `I` line begins with `8=FIX.4.4`;
2. every tag on an `I` line is numeric;
3. no field on an `I` line is empty; **and**
4. the file contains no `iDISCONNECT` — mirrored, that is this engine closing a connection
   nothing told it to close.

The excluded set is nine: the eight ADR-0004 named, plus `1b_DuplicateIdentity.def`.

**The number stays computed, never listed.** `mirrors()` applies the four clauses and
`crates/conformance/tests/mirror.rs` asserts the set it produces equals the nine names. A
corpus change that alters the set fails a test instead of passing quietly.

## Consequences

**Good**

- The gate is honest about what it covers. A file the initiator cannot pass for a reason that
  has nothing to do with the initiator would otherwise sit red forever, or — worse — be made
  to pass by teaching the session a rule no protocol has.
- The criterion now covers both halves of a `.def` file, messages and directives. It was
  written against the half that was in front of it.

**Bad**

- One fewer file of coverage, and the one lost is the only multi-connection file in the suite.
  Mirrored, the initiator side gets **no** two-connection coverage at all. The acceptor still
  has it, through `1b` and `AlreadyLoggedOn` unmirrored.
- ADR-0004's headline number, quoted in `DESIGN.md` and in the initiator plan, was wrong by
  one from the day it was written. It was wrong in the direction of optimism, which is the
  direction that matters.

**Neutral**

- Nothing about decision 5 changes. Interop against `libquickfix` was already the primary
  gate precisely because mirroring is this project's own reading of a suite written for the
  other direction, and this ADR is an example of that reading needing correction.
