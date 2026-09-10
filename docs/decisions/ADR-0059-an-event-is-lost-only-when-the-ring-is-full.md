# ADR-0059: An event is lost only when the ring is full, never because somebody was reading

- **Status:** Accepted — 2026-09-09, approved by the owner on the day it was written, after the five-engine survey it asked for
- **Date:** 2026-09-09
- **Plan:** [events-without-a-shared-lock](../plans/2026-09-09-events-without-a-shared-lock.md) —
  this page is that plan's gate
- **Amends** [ADR-0035](ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md). Its decision — *an
  event is pushed, never asked for, and a loss is counted rather than swallowed* — **stands
  entirely**. What changes is one of the two things that can cause a loss, and that thing turns
  out not to have been a decision at all
- **Constrained by** non-negotiable 4 (the engine thread never blocks in `hft`), non-negotiable 1
  (no allocation on the hot path), and [ADR-0007](ADR-0007-spsc-ring-without-unsafe.md), which
  built a lock-free ring in this crate for a different queue

## Context

`STATUS.md` open item 60. Two CI failures, in two crates, both classified as flaky, were the same
defect: **an `Ended` event that never arrived, because it had been thrown away while a reader held
the ring's mutex.**

`EventRing::push` takes `try_lock` and, on failure, drops the event and increments a counter
(`crates/engine/src/observe.rs`). `try_lock` rather than `lock` is correct and is
non-negotiable 4: the engine thread may not block behind an operator's thread. The consequence is
that **a reader polling the stream destroys events by reading**, and no amount of waiting brings
them back — which is why the failing tests consumed their entire five-second budget and saw one
event where they wanted two.

`[measured 2026-09-09]` Reproduced deliberately, by removing the 2 ms sleep between polls so the
reader holds the mutex most of the time: **9 failures in 20 runs**, including the exact signature
CI produced, `saw: [ … Ended(WrongBeginString) ]; events_lost=1`. With the sleep in place the same
test passes 60 times out of 60 on two cores, and CPU starvation does not reproduce it at all —
32 spinning processes pinned to the same two cores leave it at 0.12 s against a 5 s budget. **It
was never a timing problem. It was a loss problem wearing a timeout's clothes.**

### What the survey found, and it is why this ADR exists rather than a test fix

`[researched 2026-09-09]` Four other implementations, each read from source:

| Engine | Mechanism | Loses events? | Blocks the session thread? | Says *why*? |
|---|---|---|---|---|
| QuickFIX **C++** | synchronous virtual callback (`Application::onLogout`) | never | **yes**, for as long as the application takes; `SynchronizedApplication` adds a global mutex | **no** — carries no reason |
| QuickFIX/**J** | `SessionStateListener` multicast, reflection `Proxy`, in a loop on the calling thread | never | **yes** | **no**, though it has nine kinds including `onHeartBeatTimeout` |
| **quickfix-go** | synchronous interface callback | never | **yes** | **no** |
| **nanofix** | atomic counters and gauges only | never (they are counts) | no | **no** — aggregate, not per-connection |
| **fixbolt**, today | bounded ring, `try_lock` push, losses counted | **yes** | no | **yes** — `DropReason`, eighteen sites |

Two conclusions, and the second is the one that moved this decision.

**1. The trade is forced, not careless.** Every engine that never loses an event achieves it by
running the application's callback on the session thread. That is exactly what non-negotiable 4
forbids, and it is the thing this project's positioning is about. fixbolt is the only one of the
five that both keeps the engine thread free and says *why* a connection ended.

**2. But the loss is not the price of that.** It is the price of the **producer and the consumer
sharing one mutex**, which nobody decided — ADR-0035 chose *push rather than poll* and *count
rather than swallow*, and a `Mutex<EventRing>` was how it got built. `crates/engine/src/ring.rs`
in this same crate is a lock-free SPSC queue with `AtomicUsize` head and tail
([ADR-0007](ADR-0007-spsc-ring-without-unsafe.md)), written for dispatch, and it demonstrates that
this crate already knows how to hand data between two threads without either of them taking a
lock.

## Decision

**1. The producer never takes a lock, so reading can no longer destroy an event.** The event ring
becomes a single-producer queue whose `push` advances an atomic index and writes a slot. The
engine thread neither blocks nor loses to contention.

**2. Readers serialise against each other, not against the engine.** `Observer` is `Clone` and
[ADR-0054] made a second reader a real case — `[measured 2026-09-05]` `connect_and_serve` was
draining the caller's ring, which is the same stream read twice. So this is **not** SPSC on the
consumer side and must not be built as though it were. A mutex stays, on the **consumer side
only**: two operator threads calling `events()` at once take turns, and neither of them can make
the engine drop anything. An operator's thread is allowed to block; the engine's is not, and that
asymmetry is the same one [ADR-0036] already made for commands.

**3. A loss still exists, and now means exactly one thing: the ring was full.** `events_lost()`
keeps its name and its contract — *events that never reached a reader* — and becomes a signal
about **capacity and read frequency** rather than about luck. That is what makes it actionable:
"read more often, or with a bigger buffer" is now true advice, which it was not while a loss could
happen at 256 slots free.

**4. `EVENT_CAPACITY` stays 256 and the oldest is still dropped on overflow.** Nothing here argues
for unbounded memory on an engine that must not allocate. ADR-0035's choice stands.

**5. The stream is documented as lossy in `GUIDE.md`, and the survey is the reason.** Four out of
four other implementations never lose an event. A user arriving from any of them will assume
losslessness, and the type system cannot tell them otherwise. This is a `GUIDE.md` constraint by
`CLAUDE.md` §4's definition — one the compiler cannot check.

**6. Tests that consume the stream read `events_lost()`.** Four test files poll it and only one
mentions the counter. A test that treats a lossy stream as lossless cannot distinguish "the engine
did not do it" from "the engine did it and I threw it away" — which is precisely the two hours
item 60 cost. This is a rule about tests, so it lives here rather than in a comment.

[ADR-0054]: ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md
[ADR-0036]: ADR-0036-one-mechanism-two-capabilities.md

## Consequences

**Good.**

- The failure item 60 is about becomes impossible. An operator polling frequently — the sensible
  thing to do — stops being the reason events disappear.
- `events_lost() != 0` becomes a diagnosis instead of a shrug, and the advice attached to it
  becomes correct.
- The engine thread loses a `try_lock` from a path it takes on every connection ending.

**Bad, and named rather than discovered.**

- **A second mechanism to keep sound.** The crate now has two hand-written concurrent queues.
  ADR-0007's is proven by its own tests; this one owes the same and does not inherit it.
- **The consumer-side mutex is still a mutex**, so two operator threads reading at once still take
  turns. That is a deliberate non-goal: nothing here tries to make multi-reader draining
  lock-free, and a reader that starves another reader is that operator's problem.
- **This does not make the stream lossless**, and decision 5 exists because saying "we fixed the
  losses" would be the more comfortable and less true summary. A full ring still drops, and every
  other engine surveyed still drops nothing.
- **`unsafe` is a risk here and is refused.** ADR-0007 built a lock-free ring in this crate with
  no `unsafe` at all; this follows it. If a version requiring `unsafe` is ever proposed it needs
  non-negotiable 8's evidence — a Miri run or a fuzz target — and a new ADR.

## Alternatives considered

**(a) Leave the design and fix only the tests.** Have every test read `events_lost()` and report
it. Cheap, honest, and it would have closed item 60. Rejected as *insufficient rather than wrong*:
it leaves a real deployment defect in place — an operator who polls attentively is punished for it,
and the punishment is silent unless they also read a second counter. The tests should read that
counter anyway, which is decision 6.

**(b) Make `push` block on a full `lock`.** Lossless, one line, and it breaks non-negotiable 4 by
putting the engine thread behind an operator's thread. Refused outright.

**(c) Follow the other four engines: call the operator back on the engine thread.** Never loses,
which is the whole appeal, and it hands an unbounded stall to whatever the application does in
that callback. This is the design fixbolt exists not to have, and ADR-0002's engine/library split
is where that was decided.

**(d) Reuse `ring.rs` as it stands.** It is a **byte** queue — `push(&[&[u8]])` — built for
dispatching message payloads. Events are `Copy` structs and serialising them into bytes to get a
queue this crate already has would trade a real cost for an apparent reuse. What is reused is the
technique and its proof, not the type.

## Sources

- `crates/engine/src/observe.rs`, `EventRing::push`, and `crates/engine/src/ring.rs`.
- [ADR-0035](ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md),
  [ADR-0007](ADR-0007-spsc-ring-without-unsafe.md), [ADR-0036](ADR-0036-one-mechanism-two-capabilities.md).
- The five-engine survey, recorded in [prior-art.md](../reference/prior-art.md).
- `STATUS.md` open item 60, and
  [the-same-commit-went-red-and-green-in-the-same-minute](../reference/the-same-commit-went-red-and-green-in-the-same-minute.md).
