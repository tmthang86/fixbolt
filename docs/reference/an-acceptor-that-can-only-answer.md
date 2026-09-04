# An acceptor that can only answer

> `[measured 2026-09-04]` — found by the assembly check in step 1 of
> [plans/2026-09-03-acceptor-interop.md](../plans/2026-09-03-acceptor-interop.md),
> before the C++ counterparty it was written for existed.
> **`[to testing-skills]`**

## The finding

`fixbolt::serve` gives an application **no way to originate a message**. Every application
message this engine can send is a *reply*, returned from `Handler::on_message` for a message
that just arrived.

| Path | What it can send | Line |
|---|---|---|
| `Handler::on_message(&mut self, msg, reply) -> Answer` | one message, in answer to `msg` | `crates/library/src/app.rs:117` |
| `Admin::Command` | `SetNextOut`, `SetNextIn`, `SendSequenceReset` — sequence numbers, nothing else | `crates/engine/src/observe.rs:649` |
| the session layer | the seven administrative types, on its own schedule | `crates/session/src/lib.rs` |

Nothing else writes to a connection. So this engine cannot send an `ExecutionReport` for a fill
that lands a second after the order, cannot stream a quote, cannot send a `35=j`
`BusinessMessageReject` out of band, and cannot say anything at all to a counterparty that is
merely connected and quiet.

## How it surfaced, and why nothing before it could

The gate being built drives a `libquickfix` initiator into this engine's acceptor. Before
writing the C++ side, the tool's **two roles were pointed at each other** — this repository's
initiator into this repository's acceptor — purely to check the new role was wired up.

Five of the seven steps passed. Two could not:

```
interop: news         FAIL  0 application messages delivered
interop: resend       FAIL  35=B with 43=Y replayed at 34=[], wanted [2, 3]
```

Both steps need the counterparty to send two `35=B` News **on logon, unprompted**. The C++
acceptor does that in three lines of `onLogon`. The fixbolt acceptor has no `onLogon`, and no
other door either.

**Every existing gate is blind to this by construction:**

| Gate | Why it cannot see it |
|---|---|
| the 59 `.def` acceptance definitions | every one is *stimulus → response*. A definition for "the acceptor speaks first" cannot be written in that format |
| `crates/library/tests/end_to_end.rs` | sends an order, reads the reply |
| `scripts/interop.sh` (initiator role) | fixbolt is the initiator there; the *C++* side is the one that originates |
| `tools/w2w` | measures a round trip, which is by definition a reply |

The whole test estate is request/response, so the request/response API looked complete.

## The generalisable shape

**A component whose tests all drive it from the outside cannot show you what it is unable to
start.** Every fixture supplies a stimulus, so every capability that needs no stimulus is
outside the coordinate system — not failing, not skipped, simply never named.

Pointing two of your own components at each other finds these cheaply, because each one's
expectations become the other's requirements. The check above is not a gate and proves nothing
about correctness; what it produced was a **list of things one side expected and the other
could not do**, which is the thing no single-sided fixture generates.

## What was done here

Nothing, in the plan that found it. It is a public-API change in two crates and needs an ADR of
its own — fixing it inside a plan scoped to a test tool is how a branch stops being reviewable.
It is `STATUS.md` open item 46, and the plan's step-1 self-check now **expects those two steps
red** and says why, so the expectation is written down rather than remembered.

## And then it was fixed

`[2026-09-05]` [ADR-0048](../decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md)
and [an-engine-that-can-speak-first](../plans/2026-09-05-an-engine-that-can-speak-first.md).
Two doors: `Handler::on_logon` on the engine thread, and a `Sender` from any thread. The two
red steps are green and **the exemption is gone from `tools/interop`**, so a red there is a red
again.

**What that does not change is the lesson above**, which is why this page keeps its marker: the
capability was missing for as long as the test estate was shaped the way it was, and the shape
is the finding. The fix took a day; noticing was the expensive part, and it happened by
accident — the assembly check was run to see whether a new role was wired up, not to look for
this.

One more thing arrived with the fix and is written up separately, because it is a different
mistake: the door opened and the first two messages through it were **still** refused, by the
counterparty's dictionary, while a neighbouring step in the same gate went green on those very
bytes — [a-message-on-the-wire-is-not-a-message-delivered](a-message-on-the-wire-is-not-a-message-delivered.md).
