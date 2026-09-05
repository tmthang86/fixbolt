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
Rule Zero exists to stop. It became `STATUS.md` item 48, and the interop gate pinned the
behaviour so the day it changed, somebody had to come back and read this page.

**`[measured 2026-09-05]`, one plan later, that day came.** The journal now answers *both*
questions: `Journal::mark_out` records the highest outbound number spent — a high-water mark, so
a kept `put` raises it and telling it again writes nothing — and `highest_out()` reads it back.
`Resumed::from_journal` does the arithmetic once, so no example derives it by hand any more.
`FileJournal` writes it as a fourth record shape and the format stays v1.
[ADR-0053](../decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md).

**And the third column of the table above turned out not to matter.** The fix was *not* to reach
the live number through a handle — the row this page wrote as the blocker, and the reason item 48
was recorded as *"cannot close before item 47"*. Reading `next_out` from an `Observer` is wrong at
the root: an observer knows the number **when somebody asks**, so a Heartbeat between the last
poll and the process dying leaves the recorded number short anyway — the same defect with a
smaller and less reproducible window. The only source that is both durable and present *at the
moment `recover` runs* is the journal. **The dependency this page asserted was an artefact of the
fix it had imagined**, and writing the blocker down as a fact is what made it look real for four
days.

## Two things the gate found that the code did not

**One: the SIGKILL scenario had been green for a reason that was not the engine.** It ran at
`HeartBtInt=30`, chosen so no Heartbeat could fall inside its few-second window — which is
*exactly* the condition this whole page is about, chosen by the fixture. So the abrupt scenario
could never have shown the defect, and its green said nothing. It now runs a second time at
`HeartBtInt=1` with a deliberate pause before the kill, which guarantees the last number spent
belongs to a message no journal holds bytes for.

**Two: the inbound direction had the same hole, on the same line, and only the gate could see
it.** `Session::received_with` returned as soon as `judge` answered `Link::Dropped`, and the
counterparty's own `Logout` is judged exactly that way — so `journal.mark_in`, which sat after
that return, never ran for it. The number that `Logout` arrived under was consumed and never
recorded; a resumed session expected it again, the counterparty's next message was one too high,
and this end sent a `ResendRequest` for a message it already had.

Nothing in this repository could see that. The unit tests for the inbound mark drive messages
that leave the link **up**; the acceptance corpus compares bytes on the wire, and every byte here
was correct. It surfaced only because fixing the outbound half let the clean-logout scenario run
to its fifth assertion, which read `35=2: 1` where it wanted none. **A gate that gets further
finds things the gate that stopped early could not** — and the thing it found was the mirror image
of the defect that had been blocking it.

## The rule

**A store that was built to answer one question does not answer a second one just because the
numbers look right.** The journal answers *"what did I send that can be replayed?"*. It was read
as if it answered *"where had I got to?"*, and those two agree right up until the moment they do
not — which is a clean logout, the most ordinary ending there is.

**And two more, both about the gate rather than the store — these two generalise past FIX and
are owed upstream:**

**`[to testing-skills]`**

**A fixture value chosen to keep a test readable can be the thing the test is supposed to
detect.** `HeartBtInt=30` was picked so the sequence numbers on the transcript stayed easy to
read. It also guaranteed the defect could not appear. Nothing about the assertion was wrong; the
*setup* had quietly removed the case. When a gate passes, ask which of its inputs would have had
to differ for it to fail — and whether any of them were chosen for convenience.

**A blocker written down as a fact outlives the assumption it rests on.** *"This cannot close
before item 47"* was true of one imagined fix and false of the one that worked, and it stood for
four days as though it were a property of the problem. A recorded dependency should name the fix
it assumes, so the next reader can check whether that is still the fix.
