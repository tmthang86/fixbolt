# A journal holds messages, not the session's numbering

> `[measured 2026-09-05]` — found while building
> [reconnect-interop](../plans/2026-09-05-reconnect-interop.md), by a real `libquickfix`
> acceptor refusing a resumed session and saying exactly why.
> `STATUS.md` item 48.

## The finding

`Recovery::recover` hands the engine a `Resumed { next_out, next_in, .. }` when a session comes
back. Every example of it in this repository derives `next_out` the same way:

```rust
let next_out = journal.highest().map_or(1, |h| h + 1);
```

`crates/engine/tests/on_disk.rs` does it, and so did the first version of `tools/interop`'s
reconnect role. **It is wrong for any session that sent an administrative message after its last
application message**, and *every* session does that eventually — a Heartbeat, a TestRequest, an
answer to somebody's Logout.

`Journal::put` is called on the application path and nowhere else
(`crates/session/src/lib.rs:1871`, `:2481`). The journal is the **resend store**: it holds what a
`ResendRequest` might legally ask for, and administrative messages are never replayed, they are
gap-filled. So the journal is a record of *messages*, and reading it as a record of *the
numbering* is off by exactly the administrative messages that came after the last application
one.

## What it looks like when it bites

Two scenarios, the same engine, the same journal derivation. The counterparty is a real
`libquickfix` acceptor holding its own `FileStore`.

**Killed with `SIGKILL` — nobody says goodbye, and the derivation is right:**

```
this engine sent: 35=A 34=1, 35=B 34=2          journal.highest() = 2
                                                next_out = 3
came back at:     35=A 34=3                     accepted
```

**Stopped with `SIGTERM` — the venue says goodbye, this engine answers, and the derivation is
short by one:**

```
this engine sent: 35=A 34=1, 35=B 34=2, 35=5 34=3    journal.highest() = 2, still
                                                     next_out = 3
came back at:     35=A 34=3
the counterparty: 35=5 58=MsgSeqNum too low, expecting 4 but received 3
```

The `35=5` answering the goodbye spent `34=3` and the journal never heard about it. **The
shortfall is exactly the count of administrative messages sent after the last application
message** — one, here — and `scripts/interop.sh`'s `known_gap` assertion pins that number rather
than merely noticing a failure, because a shortfall of a different size would mean the
explanation is wrong.

## The rustdoc already knew, and that was not enough

`Resumed::next_out` says:

> `34=` on the next message this session will send. Usually `journal.highest() + 1`, and
> *usually* is why the engine does not compute it.

The word *usually* is carrying the whole finding, and **no document said what to do about the
other case**, while the only worked example in the repository did the naive thing. A caveat that
is one word long, in a doc comment, next to an example that contradicts it, is a caveat nobody
reads. `GUIDE.md` §8c warns that `NoRecovery` restarts the numbering — the louder, easier
mistake — and said nothing about this quieter one.

## And the number that would be right is unreachable

The session knows its own `next_out`. `Observer` can read it. But
[`connect_and_serve`](../GUIDE.md) builds its engine internally and returns only a `Shutdown` —
no `Observer`, no `Admin`, no `Sender` (`STATUS.md` item 47). So an initiator deployment using
the front door **cannot observe the number it will need**, and there is no other source:

| Source | Holds administrative messages? | Reachable at `recover` time? |
|---|---|---|
| `Journal` | **no** — application only | yes |
| `MessageLog` (`FileLog`) | yes, both directions | **no** — one writer thread, flushed on close, and the log is owned by the engine for the whole run |
| `Observer::snapshot` | yes (`next_out` directly) | **no** — `connect_and_serve` hands out no handle |

So item 38's gate found item 47 from the other side, with a counterparty's own words as the
evidence. That is the part worth carrying: **the gap was not visible from inside this repository
for three days, and one afternoon in front of somebody else's engine printed it in English.**

## What was done about it

Nothing, in the plan that found it — `CLAUDE.md` §1: a fix outside a plan's scope is the drift
Rule Zero exists to stop. It became `STATUS.md` item 48, and the interop gate pins the current
behaviour so the day it changes, somebody has to come back and read this page.

## The rule

**A store that was built to answer one question does not answer a second one just because the
numbers look right.** The journal answers *"what did I send that can be replayed?"*. It was read
as if it answered *"where had I got to?"*, and those two agree right up until the moment they do
not — which is a clean logout, the most ordinary ending there is.
