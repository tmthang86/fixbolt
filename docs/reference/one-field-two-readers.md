# One field, two readers, and only a stranger could see the second

`[measured 2026-09-09]`

## What happened

Half A of [timestamp-micros](../plans/2026-09-04-timestamp-micros.md) closed on 2026-09-08. Its
job was: *a valid microsecond `52=SendingTime` must not be answered with a silent hang-up*. It
widened `fixbolt_session::clock::parse_utc` from two widths to four — 17, 21, 24 and 27 bytes —
added three unit tests, ran the 59 acceptance definitions, ran `cargo test --all` at 594 passed
0 failed, and was closed with the evidence quoted.

**It was still broken, and every gate in this repository said otherwise.**

On 2026-09-09, half B pointed a real `libquickfix` initiator at this engine's acceptor with
both ends configured `TimestampPrecision=6`. The session logged on. Then:

```text
in  |8=FIX.4.4|9=119|35=3|34=18|49=FIXBOLT|52=20260909-06:15:27.872514|56=QFMIC|
     45=12|58=Incorrect data format for value|371=52|372=4|373=6|10=058|
```

A `Reject` naming tag 52. Then another, per Heartbeat. Per SequenceReset. Per Logout. The
counterparty's timestamps were valid, this engine had been taught to read them, and it was
refusing every one of them after the Logon.

## Why

**`52=` has two readers in this codebase and half A widened one of them.**

| Reader | What it is for | Where |
|---|---|---|
| `session::clock::parse_utc` | turn the stamp into a number so the **skew** can be measured | `crates/session/src/clock.rs:63` |
| `dict::FieldType::UtcTimestamp` | decide whether the value **is a `UTCTIMESTAMP` at all** | `crates/dict/src/field_type.rs:164` |

The second delegates to a `time` helper that read:

```rust
if v.len() != 8 && v.len() != 12 {
    return false;
}
```

`HH:MM:SS`, or `HH:MM:SS.sss`. Nothing wider. So a 24-byte stamp parsed fine, measured a
sensible skew, and was then rejected by the dictionary as a malformed value — `373=6`,
*Incorrect data format for value*.

The two readers had agreed for as long as there was only one pair of widths. Widening one of
them is what made the disagreement expressible, and nothing named the pair.

## Why nothing here could see it

Three separate blind spots lined up, and each one is ordinary on its own.

1. **The new test drove a `Logon`.** Half A's test asserted *the link survives and the skew is
   measured* — both true. The reject fires on messages **after** the Logon, and the test never
   sent one.
2. **The 59 acceptance definitions carry no stamp wider than 21 bytes.** They cannot see this
   feature at all, which half A recorded honestly and which is exactly why their green said
   *nothing regressed* and could not say *this works*.
3. **`dict`'s own tests asserted the widths `dict` had.** `field_types.rs` checked
   `20040415-12:00:00` and `20040415`. A test written from the implementation's set of accepted
   values can never find a value missing from that set.

So the gap was invisible to a unit test, invisible to the corpus, and invisible to the
dictionary's own suite. **What found it was somebody else's engine**, configured for the feature
and driven over a real socket, with the raw bytes read rather than a scoreboard.

## The generalisable shape

**`[to testing-skills]`**

> A value that crosses a system is often read by more than one component, for different
> questions. Widening what one of them accepts is a change to a *contract between components*
> that no single component's tests can see: each side's suite is written from its own set of
> accepted values, so a value missing from both sets is missing from both suites.
>
> The three checks that would normally catch a regression all pass here **for reasons unrelated
> to the change**: the new unit test exercises the one code path where the second reader is not
> consulted; the end-to-end corpus contains no example of the new value, so its green means
> *nothing broke*, not *this works*; and the second component's tests enumerate the values it
> already accepts.
>
> What found it was an **external implementation configured for the feature**, driven over the
> real transport, **judged on the bytes rather than on a pass/fail line**. The step that mattered
> was making the harness print its transcript on a run that *succeeded* — it printed it only on
> failure, so the evidence for a passing run did not exist.
>
> The cheap generalisation: when you widen what one reader accepts, **grep for the other readers
> of the same value before you close the change**, and add the new value to a test that crosses
> the seam rather than to a test that lives on one side of it.

## The fix, and the test that holds it

`crates/dict/src/field_type.rs` now reads `matches!(v.len(), 8 | 12 | 15 | 18)` — the same four
widths `parse_utc` reads, **and the two readers agreeing is the point**.

`crates/dict/tests/field_types.rs::a_microsecond_timestamp_is_a_timestamp` asserts all four on
both `UTCTIMESTAMP` and `UTCTIMEONLY`, and asserts that a width which is neither — `.8725`,
`.`, eleven digits — is still refused: this widens the set, it does not remove the check.

`scripts/interop.sh` §4h is the gate that found it and now guards it. It asserts, on
QuickFIX's own transcript of the wire:

- at least one 24-byte `52=` **from** this engine (the direction ADR-0057 built),
- at least one **from** `libquickfix` (so the oracle really was configured),
- **zero** 21-byte `52=` — because QuickFIX accepts widths from 17 to 27, a run where this
  engine silently stayed at milliseconds logs on and passes every step gate, and the only thing
  that separates the two outcomes is a count of what is *not* there,
- **zero** `35=3` naming tag 52 — the assertion this page exists for.

`[measured 2026-09-09]` it reads `24-byte 52= — 12 from fixbolt, 10 from libquickfix`,
`21-byte 52= — 0`, `35=3 naming tag 52 — 0`. Proven by reversal: setting this engine's
`TimestampPrecision` back to `3` gives `0 from fixbolt` and `12` millisecond stamps, and the
gate fails.

## A second thing this cost, worth writing down

**The first version of §4h died with no message at all.** It globbed for a QuickFIX file log
that is never written — `tools/interop/initiator.cpp` installs its own `RawLogFactory`, so
`FileLogPath` is ignored — and under `set -o pipefail` a glob that matches nothing made the
assignment fail, `set -e` ended the script, and the run exited 1 having printed the scenario's
header and nothing else.

A gate that dies silently is indistinguishable from a gate that did not run.
`docs/reference/reading-the-output-you-grepped-for.md` is about a filter that could not show a
surprise; this is one turn worse — the surprise was that there was nothing to filter.

## And a fact about the indexing ratchet, found the same day

`scripts/check-indexing-debt.sh` counts `clippy::indexing_slicing`, and **that lint does not
fire on a constant index into a fixed-size array**: `self.buf[18]` where `buf: [u8; 27]` is
proven in bounds at compile time and cannot panic. Only *computed* indices and *computed* slice
bounds are counted.

This matters when reading the number. Rewriting a function from `self.buf[18]` to a slice
pattern changes the count by zero — and `[measured 2026-09-09]` writing the same function with
a computed index and a computed slice took it from **184 to 186**, which the ratchet caught in
CI-equivalent form on the first run. The ceiling is a real guard; it is a guard about
*unprovable* indexing, which is the kind that panics.
