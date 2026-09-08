# A valid field, refused for its width, answered with silence

`[measured 2026-09-08]` — wave B plan 3, half A.
[Plan](../plans/2026-09-04-timestamp-micros.md) · guarded by
`crates/session/tests/skew.rs::a_microsecond_sending_time_is_read_and_the_link_survives`
and `crates/session/tests/application.rs::a_microsecond_orig_sending_time_is_read_rather_than_reported_missing`

`[to testing-skills]`

## What happened

`52=SendingTime` reaches a FIX 4.4 acceptor as `YYYYMMDD-HH:MM:SS` or the same with `.sss` —
17 or 21 bytes. Those are the two widths the specification documents, the two the 59-file
acceptance corpus contains, and the two this engine's parser accepted:

```rust
if s.len() != LEN_SECONDS && s.len() != LEN_MILLIS {
    return None;
}
```

A European venue under MiFID II RTS 25 keeps its clock synchronised to the microsecond and
stamps `52=` with six fractional digits: 24 bytes. That is a **valid, ordinary, widely
deployed** timestamp. Through the code above it is `None` — indistinguishable from a field
that is absent, or garbled, or names the 32nd of December.

What the session then does with a `None` is where it stops being a parsing decision:

```
52= 24 bytes  →  parse_utc = None  →  time_ok = false
              →  state == AwaitingLogon  →  Refusal::BadSendingTime
              →  hang up, sending nothing
```

The silence is correct for what it was built for. Before a `Logon` completes there is no
session to answer with, so a wrong clock earns a dropped connection rather than a Reject
naming the fault — QuickFIX's `1c` and `1d` definitions both assert exactly that. Applied to
a *right* clock it means the counterparty gets no logon, no Reject, no `58=`, no byte at all,
and no way to tell this apart from a firewall.

`122=OrigSendingTime` went through the same function and failed differently: `373=1
RequiredTagMissing` naming tag 122 — the engine telling a venue a field is missing while the
venue can see it in its own log.

## Why no gate could see it

Five separate gates were green over this for the length of the project:

| Gate | Why it was silent |
|---|---|
| The 59 acceptance definitions | 0 of them carry a stamp wider than 21 bytes. `59 / 59` is a true statement about a corpus that never asks the question |
| `cargo test --all` — 589 tests | Every timestamp in every fixture was 17 or 21 bytes, because every fixture was built from the corpus |
| Interop against a real `libquickfix` | The other engine was configured, by default, to send milliseconds. Two implementations agreeing about a case neither one exercises is not a second opinion |
| `clippy`, the lint set behind non-negotiable 7 | The code is correct Rust doing exactly what it says |
| The parser's own unit tests | They enumerated the widths the parser rejects. Every entry in that list was something that *should* be rejected — the list was written from the implementation, so it could only ever agree with it |

The last row is the transferable one. **A test suite built from the specification you
implemented cannot find the part of the world the specification left out.** The corpus, the
fixtures and the negative-case list all descend from the same reading of FIX 4.4; they
disagree with the code about nothing, because they were derived from it.

What did find it was a question asked from outside: *what does a real counterparty actually
put on this wire in 2026?* — which is a question about deployments, not about the
specification, and no green test can raise it.

## The generalisable lesson

**A parser that returns one failure for every reason has thrown away the information the
caller needs to answer well.** `parse_utc` returned `Option<u64>`: `None` meant *"absent"*,
*"garbled"*, *"impossible date"* and *"a precision I was never taught"* all at once. The
caller could not treat the fourth differently from the first three even if somebody had
thought to — the type made them the same fact. This is the same shape as
[three-outcomes-collapsed-into-one-none](three-outcomes-collapsed-into-one-none.md), two
files away in this directory, found in the same week for the same reason. A collapsed
failure type is not a small infelicity; it is a decision, taken silently, that nobody
downstream can revisit.

**And when a rejection is silent, its blast radius is the whole of what silence can mean.**
The hang-up was designed for a case where naming the fault would be worse. Reused for a case
nobody had considered, it produced the least diagnosable outcome the protocol has. A refusal
path is worth auditing not for whether it fires when it should, but for what it costs when
the premise underneath it turns out to be wrong.

## Two more things this cost, both found by running rather than reading

**A fixture wrong in two ways looks like a passing test that is about the wrong thing.**
The first version of the `122=` test stamped it `.123456` while leaving `52=` at `.000` — so
`122=` named an instant *after* the message that carried it, which is
`2f_PossDupOrigSendingTimeTooHigh`, `373=10` and a Logout. The test went red on the link
assertion and not the one it was written for. It was rewritten to `.000456`, which truncates
into the same millisecond as `52=`, and that turned an accident into an assertion: the test
now also fails if the sub-millisecond part is ever kept or rounded up instead of dropped.

**A reversal is what proved the widths were live.** Deleting `24` from the accepted widths
turns four tests red, each at its own assertion, with `Dropped` where `Up` was expected and
the literal `373=1 371=122 58=Required tag missing` printed by the failure message. Deleting
`27` separately turns the nanosecond assertions red and leaves the rest green. Two reversals
rather than one, because a single reversal that removes both widths cannot tell a live
nanosecond arm from a dead one.

## What is still not proven

Half A reads these widths. **It does not send them** — this engine still writes 21 bytes, and
whether it should write more is `ADR-0057`, unwritten. And there is **no capture of a real
venue** sending a microsecond `52=` in this repository: the behaviour is built from the
specification and from what MiFID II requires, which is one step better than a guess and one
step short of evidence. `CLAUDE.md` §7 asks for real captures over invented messages; here
there is no capture, and this paragraph is what that costs.
