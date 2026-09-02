# A reversal that stayed green, because the protocol had a second legal answer

> `[measured 2026-09-02]` — found running the **control** for
> [scripts/interop.sh](../../scripts/interop.sh), step 2 of
> [plans/2026-09-02-the-initiator-and-its-second-opinion.md](../plans/2026-09-02-the-initiator-and-its-second-opinion.md).
> **`[to testing-skills]`**

## The shape

A request carries a range. The counterparty may answer it in **two** legal ways, and the two
share the field the test was reading.

| This end asks | A conforming counterparty may answer |
|---|---|
| *"resend 2 through 3"* | the two messages, replayed, each marked *this is a repeat* |
| | one *"skip to 4"* placeholder, also marked *this is a repeat* |

The second is what a counterparty sends when it no longer holds the messages, and it is
**correct**. Both shapes carry the same *is-a-repeat* flag, and the first version of this gate
read exactly that flag:

```rust
// the assertion, as first written
let answered = w.read_until(8, |m| m.contains("|43=Y|")).is_some();
```

## The measurement

The reversal was to swap the two ends of the range, so this end asked for **3 through 2** — an
inverted range, a question no correct implementation asks.

```
interop: resend       ok    a message came back with 43=Y
interop: PASS 7/7
```

**The gate stayed green.** The counterparty's own log says why:

```
in   35=2 ... 7=3 16=2                      <- the inverted question
out  35=4 ... 43=Y 36=3 123=Y               <- "skip to 3", the placeholder
```

It could not replay a range that runs backwards, so it fell back to the placeholder — which
carries `43=Y`, which is what the test was looking for. **A legal answer to a question nobody
asked passed a test named for the question.**

With the range the right way round, the same counterparty replays both messages:

```
out  35=B ... 34=2 43=Y 122=<original time> ...
out  35=B ... 34=3 43=Y 122=<original time> ...
```

So the two shapes *are* distinguishable — the test simply was not reading the part that
distinguishes them.

## The fix

Assert on the identity of what came back, not on the flag that both shapes carry:

```rust
// replayed: the sequence numbers of messages that came back as 35=B with 43=Y
assert_eq!(replayed, [2, 3]);
```

Re-run of the same reversal, unchanged:

```
interop: resend       FAIL  35=B with 43=Y replayed at 34=[], wanted [2, 3]
interop: FAIL 6/7
```

**6 / 7, and the one that fell is the one the reversal aimed at.** The previous reading gave
7 / 7 for the same code.

## The lesson, stated without FIX

**When a protocol allows a fallback answer, a test that accepts "an answer" accepts the
fallback — including for requests that are malformed.** The fallback is usually the *cheaper*
path for the counterparty, so a broken request is more likely to reach it than a correct one.
That inverts the test: the worse the request, the likelier it is to be answered in a way the
assertion accepts.

Three things to take from it:

1. **Assert on identity, not on a marker.** *"Something came back with the repeat flag"* is a
   liveness check wearing a correctness check's name. *"Message 2 and message 3 came back"* is
   the claim the step is actually making.
2. **A reversal that stays green is a result, not a nuisance.** This one cost ten minutes and
   bought a real hole. Every reversal in this repository is run for this reason; most of them
   go red and prove the guard, and the ones that do not are the ones worth writing down.
3. **Read the counterparty's log when a reversal is a no-op.** The answer to *"why did it still
   pass?"* was one line at the other end, and no amount of re-reading the test would have
   produced it. `tools/interop/acceptor.cpp` prints every message it sends and receives for
   exactly this.

## Where it is guarded

`tools/interop/src/main.rs`, step 5, and the comment there names this file. The reversal is not
automated — it is in the plan's verification table and in the commit body, which is how every
reversal in this repository is recorded.
