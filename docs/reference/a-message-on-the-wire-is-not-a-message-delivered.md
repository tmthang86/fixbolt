# A message on the wire is not a message delivered

> `[measured 2026-09-05]` — found while building
> [an-engine-that-can-speak-first](../plans/2026-09-05-an-engine-that-can-speak-first.md),
> by two steps of one gate disagreeing about the same two messages.
> **`[to testing-skills]`**

## The finding

The acceptor had just been given a way to originate a message
([ADR-0048](../decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md)). Its
handler sent two `35=B` News on logon. The seven-step interop gate was then pointed at it,
and read:

```
interop: news         FAIL  0 application messages delivered
interop: resend       ok    35=B with 43=Y replayed at 34=[2, 3], wanted [2, 3]
interop: FAIL 6/7
```

**Both steps are about the same two messages.** `resend` asks the counterparty to replay
numbers 2 and 3, which are exactly the two News, and it found them — correctly numbered,
correctly flagged `43=Y`. So the messages existed, were journalled, went out, and came back.

And `news` is right too: **neither of them was ever delivered to an application.** The News
carried `148=` Headline and nothing else. `FIX44.xml` line 294 makes `LinesOfTextGrp`
`required='Y'` — the `33` NoLinesOfText group, with `58` Text in each entry. A News without it
is refused by the receiving session's dictionary validation and answered with a `35=3`. It
never reaches `on_message`.

Adding the group turned the run to `PASS 7/7` with no other change.

## Why one step could not see what the other did

The two steps assert on different things, and the difference is the whole lesson:

| Step | What it reads | What that proves |
|---|---|---|
| `resend` | the **bytes** that came back off the socket, matched as text | the counterparty's *session layer* handled them |
| `news` | a **counter inside the application** the session delivers to | the counterparty's *application* received them |

A byte-matching assertion is satisfied by a message the counterparty is in the middle of
rejecting. The reject travels back on the same socket, and a test looking for `|35=B|` in what
arrived has already found what it wanted before the `35=3` shows up.

**The gate was well built and still nearly missed it.** `resend` is not a bad step — it is
asking a session-layer question and it got a correct session-layer answer. It is only
misleading when read as evidence about the message's *content*, and content is what a
dictionary rejects on.

## The generalisable shape

**An assertion on transport is not an assertion on acceptance.** Anywhere a protocol has a
validating layer between arrival and use — a schema, a dictionary, a deserialiser, a
constraint check — a test that reads the wire is measuring the layer *before* the one that can
still say no.

The diagnostic is cheap and worth making a habit: for each assertion, name the last component
that had to agree for it to pass. If that component is not the one under test, the assertion
is measuring something else.

Here the two answers were **in the same run, three lines apart, and contradictory**, which is
the luckiest version of this bug. The unlucky version is a suite with only the byte-matching
step, which goes green forever over a message nobody can read.

## What was done

`tools/interop/src/desk.rs` writes the required group, and its rustdoc names the XML line so
the next reader does not re-derive it. The engine-side unit tests
(`crates/engine/tests/originate.rs`) were **not** changed and could not have caught this: they
use a `Loopback` and this repository's own session, and the trap needs a counterparty doing
dictionary validation to appear at all.

**No new gate was added for it**, deliberately. The gate that found it is the gate that keeps
it: `news` asserts delivery, not arrival, and reverting the group in `desk.rs` turns it red.
