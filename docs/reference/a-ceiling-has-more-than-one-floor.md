# A size ceiling is only the one you measured; the binding one is somewhere else

> `[measured 2026-09-05]` — found while making the receive buffer the caller's choice,
> [plans/2026-09-05-buffers-the-caller-can-size.md](../plans/2026-09-05-buffers-the-caller-can-size.md).
> **`[to testing-skills]`**

## The claim under test

*"This acceptor cannot handle a message larger than 4 KiB, because its receive buffer is
`[u8; 4096]`."* True, checked against the source, and the fix was built: the buffer size became
a parameter the caller supplies.

The obvious test: stand up an acceptor with a 16 KiB buffer, send a 5 KiB order, **assert the
reply comes back**. It failed. So did 4 KiB, 3 KiB, and 1 KiB — through the 16 KiB buffer, which
was supposed to make all of them work.

```
size=  200  echo=true   reply_len=223
size= 1000  echo=false  reply_len=0
size= 3000  echo=false  reply_len=0
size= 5000  echo=false  reply_len=0
```

The threshold was **between 200 and 1000 bytes**, nowhere near either buffer.

## What was actually in the way

A different constant, two layers down, in a struct nobody had been looking at:

```rust
/// Where an Application writes its reply.
pub(crate) app: [u8; 1024],
```

An application that cannot lay out its reply in 1 KiB returns "no reply". That is not an error
anywhere — it is a legal answer — so the socket simply stays quiet. **The observable for "too
big" and the observable for "nothing to say" are the same silence.**

Had the first test been believed, it would have "confirmed" a 4 KiB inbound ceiling by measuring
a 1 KiB outbound one, and the fix would have shipped with a green test that never touched the
thing it named.

## Why the test was wrong, not the code

The test asked *"did the reply come back?"* to answer *"was the message framed?"* — two events
with three constants between them. The reply crosses the inbound buffer, the application's
scratch buffer, and the write queue; only the first was under test.

The replacement asks a question that crosses **one** boundary: send the big message, then a
small `TestRequest` behind it, and see whether the session answers *that*. A message that is
framed and consumed leaves the sequence intact and the `TestRequest` is answered normally. A
message that cannot be framed is garbage, and garbage takes everything buffered behind it — so
the `TestRequest` disappears too. The two cases differ in **what comes back**, not in how long
it takes, so no timeout separates them and nothing downstream of framing is in the path.

## The general shape

**When a test spans N constants and names one of them, it is measuring `min` over all N.** The
name on the test says which one you believe is smallest, and belief is not measurement.

The check is mechanical and costs one run: **sweep the input size and find where the answer
actually flips.** A threshold that lands anywhere but the constant under test means some other
constant is binding. Here the sweep took thirty seconds and moved the conclusion from "the fix
does not work" to "the fix works and there is a second ceiling nobody had written down".

The corollary is worth as much: **a silence with two causes cannot be an assertion's only
evidence.** "No reply" meant both *unframeable* and *nothing to say*, and the test could not
tell them apart. Pick an observable with one cause, or add a second observable that
discriminates.

## And the third ceiling was in the measuring instrument

Raising the reply scratch to 8 KiB did not move the wall to 8 KiB. It moved it from ~900 bytes
to somewhere between 3 000 and 5 000 — because the **test oracle** lays its reply out in a
`TemplateBuilder::<128, 4096>` of its own. That module's own doc comment says *"this is a
measuring instrument, not a product"*, and the instrument had a range.

Three sweeps, three different constants, none of them the one first suspected:

| Sweep | Wall found | Constant | Whose |
|---|---|---|---|
| 1 | ~900 B | `Outbound::app: [u8; 1024]` | the system |
| 2 | 3–5 KiB | `TemplateBuilder::<128, 4096>` | **the test** |
| 3 | — | `Framer<RX>` | the system, and the one under test all along |

**Check the instrument's range before believing a ceiling.** A harness that cannot express an
input larger than X can never demonstrate a limit above X, and it will report that limit as the
system's.

## The near-miss underneath it

The same work carried a second claim — *"the pre-session buffer and the connection's buffer are
the same size, and that matters"* — with a reversal that came back **green twice**: once with
the constant flipped in one place, once with it flipped in both.

Flipping it in both places was the right technique
([a-reversal-that-must-not-compile](a-reversal-that-must-not-compile.md), the near-miss note) and
still proved nothing, because **the reversal was in the wrong direction.** The invariant is not
*equal*, it is *the pre-session buffer must not exceed the connection's*: a prefix longer than
the receiving buffer is refused, and shrinking the pre-session buffer only makes it safer. Made
larger instead, the guard fires immediately and the logon goes unanswered:

```
REVERSAL PRE>RX: logon answered = false
```

**A reversal has a direction, and an inequality has two.** Flipping a constant to a smaller value
tests nothing when the guard is `>`. Read the guard before choosing the direction — the comment
above it said "matches", which is what sent two reversals the wrong way.


## Postscript: two samples of a flake are not a comparison

`[measured 2026-09-05]` while closing this work, the allocation gate read non-zero on the
development machine. To decide whether the change had caused it, the branch was stashed, `main`
checked out, and the gate re-run: **same array, same index, same value.** Reported as
*"pre-existing, not caused by this change"*.

Run three more times, the same command on the same machine:

```
[… index 21 = 1 …]
[… index 21 = 2 …]
[… index 21 = 2 …]
```

The value is not deterministic. The counter is a relaxed atomic shared with a background writer
thread, and on this platform the writer's own allocation sometimes lands inside the measured
window. **Two agreeing samples of a varying quantity carried no information about causation** —
the conclusion was right and the argument for it was luck.

What actually settled it was CI: the same gate on Linux read **0 for all 24 cases** on the very
commit under test. So the finding is *red on the development machine, green on the machine the
gate runs on* — the mirror image of the more familiar failure, and it needs the same treatment.

**Before comparing a measurement across two versions, sample it twice on one of them.** If the
two samples disagree, a cross-version comparison of single samples cannot establish anything, and
the honest move is to find a deterministic observable or defer to the machine that owns the gate.

The same run also caught a smaller thing worth naming: the failing case was reported by **name**
from memory rather than read from the output, and the name was wrong — index 21 was `log-record`,
not `log-busy`. A result that was recalled rather than observed is not a result.
