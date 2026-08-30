# Session lifecycle: what the standard says, and what a working engine does

> **What this page is:** research for the three open questions in
> [ADR-0010](../decisions/ADR-0010-a-reconnect-is-not-a-restart.md) — *when does a session end*,
> *what happens when the two ends disagree about sequence numbers at Logon*, and *is a durable
> `next_in` worth its cost*. Both ADRs it serves are `Proposed` and **this page decides
> nothing**; it is the material the decision should be made against.
>
> **No QuickFIX source is reproduced here.** Function names and behaviour are facts about a
> public implementation; the code stays where it is. `CLAUDE.md` §2 rule 9, ADR-0001.

## Method

Two sources, in this order of weight:

1. **QuickFIX's own session implementation, read at `master`** — `Session.cpp`, `Session.h`,
   `SessionState.h`, `MessageStore.h`, `FileStore.cpp`, fetched to a scratch directory outside
   this repository. This is the same "second opinion" role ADR-0001 already assigns it for
   field ordering and the dictionary, and it is stronger than documentation because it is what
   actually runs.
2. **The FIX Trading Community session-layer material and vendor references** for the parts of
   the standard that QuickFIX implements a *choice* within — cited inline.

Where the two disagree, that disagreement is recorded rather than resolved.

---

## Question 1 — When does a session end?

**PRD.md already tracks this**, and not as a new question: *"Session schedules — start/end
time, weekday … entered scope with ADR-0004; **unspecified** … P1, **gap**"*. Answering it
closes a gap the product document already names; it does not add scope.

### What QuickFIX does

A session holds a **creation time** and a **`TimeRange`** — start and end times, optionally
qualified by start and end *days* for a weekly session. On every tick, `Session::next` asks
`checkSessionTime`, and when that is false it calls `reset`, which **sends a Logout, drops the
connection, and resets the state** — sequence numbers to 1, creation time to now. The same
check runs when a responder is attached, so a connection arriving after the boundary is reset
before it can carry traffic. A `NonStopSession` flag opts out of the whole mechanism.

**The predicate is the design decision, and it is not the obvious one.** It does not ask *is
now past the end time*. It asks whether **now and the session's creation time fall in the same
range** — which is what makes a session spanning midnight, or a weekly session running Sunday
evening to Friday evening, work at all. A naive "past EndTime" test breaks on both.

### What the standard and the field say

Sequence numbers are initialised at the start of a session at 1 and increment through it, each
direction independently. Practice varies and the variation is real: **many venues never reset
during the trading day; some reset weekly rather than daily.** There is also a live
disagreement about *which* boundary resets — resetting on logout at `EndTime` versus resetting
when `StartTime` is next reached — with a QuickFIX/n issue arguing the latter is correct.

### What this means for fixbolt

The mechanism costs little and fits D1 exactly: **the boundary is detected on a tick**, and
`Input::Tick` is already how time reaches the pure session layer. No clock, no allocation.
What it needs is a `TimeRange` in the session's configuration and a creation timestamp in its
state.

**It is also the missing opposite of `resume`.** ADR-0010 says so itself: without a session
end, a resumed session's numbers grow for ever and the journal with them. The two belong in
one decision — `resume` without a schedule is a leak, and a schedule without `resume` is what
the code already does.

---

## Question 2 — The journal says 100, the peer asks for 50

FIX 4.4 has a field for exactly this, and it is not in ADR-0010: **`NextExpectedMsgSeqNum`
(789)**, carried on the Logon. It lets each end state, at Logon, the next sequence number it
expects *from the other* — turning a silent mismatch into an explicit negotiation.

### What QuickFIX does with it

On receiving a Logon carrying 789, it compares that value against its own next outbound:

| Peer's `789` versus my next outbound | Action |
|---|---|
| **Lower** | Retransmit the gap after the Logon completes — either real messages, or a `SequenceReset` gap fill when messages are not persisted |
| **Higher** | **Logout with a reason, then disconnect.** The peer expects messages this end never sent |
| Equal | Proceed |

Separately, and independently of 789, an incoming Logon whose own `MsgSeqNum` is **too high**
does not disconnect: the Logon is queued and a resend range is recorded, or the ordinary
too-high path runs.

**The asymmetry is the answer to the question.** *The peer is behind* is recoverable and is
recovered. *The peer is ahead* is not: it believes it holds messages this end has no record of
sending, and no exchange of sequence numbers can reconcile that — so the session is refused
rather than continued on a false basis. ADR-0010 lists "a `ResendRequest`, a `SequenceReset`,
or a refusal" as three defensible options; the working implementation says **it depends on the
direction, and both answers are needed.**

### Where the standard is narrower than it looks

The FIX session-layer material describes acceptor state reconstruction from 789 — set next
inbound from the Logon's `MsgSeqNum`+1 and next outbound from 789, then send a `SequenceReset`
with `GapFillFlag=Y` — but scopes it deliberately: **it is for service offerings where
application-level recovery replaces session-level recovery.** It is not a general licence to
rebuild session state from whatever the peer claims.

### What this means for fixbolt

789 is a FIX 4.4 field and this is a FIX 4.4 engine, so the mechanism is available. It also
**needs `resume` to be worth anything** — an engine that resets to 1 on every connection has
nothing to compare 789 against. That makes it a consequence of ADR-0010 rather than a separate
decision.

**Nothing in the acceptance corpus exercises it.** ADR-0010 already flags that resumption is
untested by the corpus; 789 is the same hole, one level deeper, and the same hand-written
tests would have to carry it.

---

## Question 3 — Is a durable `next_in` worth it?

ADR-0010 poses this as two options: write per message and put a disk on the receive path, or
accept that a restart may re-deliver and require the application to be idempotent. **There is
a third, and it is what QuickFIX actually ships.**

### What QuickFIX pays

Its `MessageStore` interface carries `getNextTargetMsgSeqNum` / `setNextTargetMsgSeqNum` /
`incrNextTargetMsgSeqNum` alongside the sender-side equivalents, so **the inbound number is
first-class, not an afterthought**. `FileStore` implements the increment by rewriting a small
sequence-number file — sender and target together — and **flushing it, on every message in
both directions.**

`[measured 2026-08-30]` reading that write path: it calls `fflush` and **not `fsync`.**

That is the third option, and it is a different durability class from either of ADR-0010's:

| Guarantee | Cost | Survives |
|---|---|---|
| Nothing | zero | nothing |
| **Flush to the OS** — QuickFIX's choice | one `write` per message, no disk sync | **process crash, not power loss** |
| Sync to the platter | `fsync` per message | power loss |

A process crash is the common failure — a panic, an OOM kill, a bad deploy. Power loss on a
colocated server with redundant supply is rarer, and after one, a session that has been down
long enough to lose power is usually re-established by agreement rather than by replay.

### What this means for fixbolt

The mapping is already built. `PRD.md` records the journal's three policies as
`None` / `Async` / `Fsync`, and **QuickFIX's behaviour is `Async` — for the inbound number as
well as the outbound one.** So question 3 is not "should we pay `Fsync` on the receive path";
it is **"should `next_in` be journalled at all, under the policy the caller already chose for
`next_out`"** — and the reference implementation answers yes, at a cost the project has already
decided it can pay in the other direction.

What fixbolt has today is narrower: the journal exposes `highest()`, which answers *what did
this end send*. There is no inbound equivalent.

**The ordering constraint ADR-0010 names is the real cost, and it is not the write.** The
inbound number must be durable **before the application sees the message**, or a restart
re-delivers work already done. That is a hot-path ordering requirement, and it is what deserves
the measurement — not the `write` itself.

---

## Question 4 — 4 MiB per ring, times how many sessions?

[ADR-0011](../decisions/ADR-0011-a-full-ring-disconnects.md) decision 3 raises the default ring
to 4 MiB, pre-faulted per session. `[2026-08-30]` **`PRD.md` states the target as an acceptor
that "holds many sessions on one core" and never quantifies "many".**

So the decision cannot be checked. 4 MiB is unremarkable at ten sessions (40 MiB) and decides
the deployment at a thousand (4 GiB, on a 30 GiB desk box). Three ways forward, and they are
genuinely different decisions rather than shades of one:

1. **Quantify it in `PRD.md`** and check 4 MiB against the number. The product document owes
   this figure for other reasons — every memory and file-descriptor question has the same hole.
2. **Make capacity per-session configuration with 4 MiB as the default**, so a gateway with a
   thousand clients can choose differently without an ADR to reverse this one.
3. **Size it from the measurement instead of from a round number.** The measured fill is 352
   messages at 64 KiB; a capacity stated as *a duration of slack at full rate* rather than a
   byte count would carry its own justification — and ADR-0011's own revision already warns
   that the duration is known only to an order of magnitude.

**None of this touches decisions 1, 2 or 4**, which rest on the count and the argument, not on
the capacity.

---

## Sources

- QuickFIX C++ at `master` — `Session.cpp`, `Session.h`, `SessionState.h`, `MessageStore.h`,
  `FileStore.cpp`. Read, not copied; fetched outside this repository.
- [FIX Session Layer, FIX Trading Community](https://www.fixtrading.org/standards/fix-session-layer-online/)
- [Logon `NextExpectedMsgSeqNum` processing — OnixS FIX dictionary](https://www.onixs.biz/fix-dictionary/fixt1.1/section_logon_msg_nextexpectedmsgseqnum_processing.html)
- [A note on tag 789, `NextExpectedMsgSeqNum`](https://www.quickfixj.org/jira/secure/attachment/10240/FIX%20Tag%20789(NextExpectedMsgSeqNum)%20Description.pdf)
- [Sequence number handling — B2BITS knowledge base](https://kb.b2bits.com/display/B2BITS/Sequence+number+handling)
- [Message sequence numbers — OnixS .NET FIX engine](https://ref.onixs.biz/net-fix-engine-guide/message-sequence-numbers.html)
- [Sequence numbers should reset at StartTime, not EndTime — quickfixn issue 151](https://github.com/connamara/quickfixn/issues/151)
