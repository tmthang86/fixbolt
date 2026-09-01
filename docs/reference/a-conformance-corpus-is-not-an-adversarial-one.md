# A conformance corpus is not an adversarial one

> `[measured 2026-09-01]` — found while building the pre-session stage,
> [plans/2026-08-31-pre-session-routing.md](../plans/2026-08-31-pre-session-routing.md)
> step 2. **`[to testing-skills]`**

`CLAUDE.md` §7 says **real captures over invented messages**, and gives the reason: *a
hand-written packet proves the parser handles a packet nobody sends.* That rule is right
and this page does not weaken it. It records the one place it does not reach, with the
measurement that found the edge.

## The code

A new stage reads a counterparty's identity — `49=` and `56=` — off a `Logon`, before any
session exists, by scanning SOH-separated fields and matching the tag at the **start** of
one:

```rust
if let Some(value) = msg[at..end].strip_prefix(tag) { return Some(value); }
```

## The corpus it was tested against

The QuickFIX FIX 4.4 server acceptance definitions: **289 messages** sent to the engine,
across 59 files. It is not a soft target. It contains, deliberately:

| Shape | Where |
|---|---|
| reversed comp IDs — `49=WT` instead of `TW44` | `1c_InvalidSenderCompID.def` |
| a comp ID that is not the configured one | `2k_CompIDDoesNotMatchProfile.def` |
| an **empty** `56=` | same |
| a required header field simply missing | `14b_RequiredFieldMissing.def` |
| a **corrupted tag** — `garbled9=TW`, `49garbled=TW` | `2d_`, `3c_GarbledMessage.def` |
| a `9=` that lies about the body length, both too long and too short | `2m_BodyLengthValueNotCorrect.def` |

Five of the 289 name no identity at all, and the tests assert that count rather than spot
check, precisely so a reader that got more lenient would be noticed.

## What the corpus did not catch

Two reversals, each a plausible way to write the same function:

| Reversal | Corpus result |
|---|---|
| match the tag anywhere **inside a field** rather than at its start | **289 of 289 green** |
| ignore field boundaries entirely and scan the whole message | **289 of 289 green** |

Both were caught by **one** message, and it was hand-built:

```
… 49=TW44 ␁ 58=49=EVIL56=EVIL ␁ 56=ISLD …
```

`58=` is `Text` — free-form, and its value belongs to the counterparty. A reader that
searches for `49=` rather than for a field beginning `49=` reports the sender as `EVIL`,
and in the stage this code lives in, that is the value a shard is chosen by.

## Why the corpus was always going to miss it

A conformance corpus encodes **what the specification says about errors**. Its malformed
messages are the ones a *correct-but-buggy implementation* produces: a truncated header, a
wrong length, a mistyped tag. Every one of those is damage to the message's **structure**.

The case above damages nothing. It is a well-formed message, with a valid body length and
a correct checksum, whose *contents* are chosen to be read wrongly. No conformance suite
has a reason to contain it, because no conforming implementation would send it — and that
is exactly the population a conformance suite is drawn from.

**The corpus covers the protocol's error space. It does not cover the attacker's input
space, and those are different sets.**

## The rule that follows, without weakening §7

1. **The real corpus stays the primary gate.** It is what proved the reader against 289
   messages somebody actually sends, and it is what caught the first draft of the *test*
   asserting a false thing — the assumption that every corpus `Logon` is `TW44`/`ISLD`
   went red on the real bytes.
2. **Add hand-built cases only where an untrusted party controls the bytes**, and name
   which party and which field. Here: a free-text field, and a router that chooses a
   destination from a neighbouring one.
3. **Build them by mutating a real message.** The test above inserts one field into a
   corpus `Logon`, so the length, the checksum, the header and everything around the
   adversarial part are still bytes a counterparty sent. What is invented is one field,
   and it is the field under test.
4. **Prove the hand-built case earns its place**: break the code, and check that the real
   corpus stays green while the invented case goes red. If both go red, the invented case
   is redundant and should go. `[measured]` here 289 stayed green, twice.

## The generalisation

`[to testing-skills]` — **a corpus drawn from correct participants cannot cover inputs
chosen by a hostile one, and the gap is invisible because the corpus is large.** 289 real
messages passing feels like coverage; the number is what makes it convincing, and the
number is drawn entirely from one population.

The shape appears wherever a suite of real recordings is the safety net: production
traffic replayed at a service, a captured protocol trace, a corpus of real user documents,
a golden-file suite grown from real runs. Each is excellent at the failures that *happen*
and structurally blind to the inputs that are *chosen*.

The cheap defence is not a fuzzer and not a second corpus. It is a single question asked
once per parser: **which bytes here does an untrusted party get to choose, and what would
they choose them to look like?** Then one mutated-real message per answer, each proven by
the reversal in point 4.
