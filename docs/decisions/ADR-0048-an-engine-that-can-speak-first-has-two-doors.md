# ADR-0048: An engine that can speak first has two doors, not one

- **Status:** Proposed
- **Date:** 2026-09-05
- **Plan:** [an-engine-that-can-speak-first](../plans/2026-09-05-an-engine-that-can-speak-first.md)
- **Supersedes nothing.** Extends [ADR-0002](ADR-0002-engine-library-split.md)'s dispatch split
  and [D4](../DESIGN.md); the primitive it builds on is `Session::send_application`, which
  [ADR-0046](ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md) left in
  place.

## Context

`fixbolt::serve` gave an application **no way to originate a message**. Every application
message the engine could send was a *reply*, returned from `Handler::on_message` for a message
that had just arrived. `Admin::Command`'s three variants only move sequence numbers. Nothing
else wrote to a connection.

So the engine could not send an `ExecutionReport` for a fill that lands a second after the
order, could not stream a quote, could not send a `35=j` out of band, and had nothing to say to
a counterparty that was connected and quiet. `STATUS.md` item 46.

**Every gate was blind to it by construction.** The 59 `.def` files, `end_to_end.rs`,
`interop.sh` and `w2w` are all stimulus-then-response, so a capability needing no stimulus was
outside their coordinate system — not failing, not skipped, never named. It surfaced only when
this repository's two interop roles were pointed at each other and the acceptor could not send
the two unprompted `35=B` the initiator role expects:
[an-acceptor-that-can-only-answer](../reference/an-acceptor-that-can-only-answer.md).

**The primitive already existed and was already right.** `Session::send_application` takes a
whole message, rewrites `8=`, `9=`, `34=`, `52=` and `10=`, orders the rest by `Fix44`, keeps a
copy for a resend, and spends the sequence number. `Conn::send_application` wraps it with the
transmit buffer, the backpressure policy and the message log. What was missing was not a
mechanism. It was a **door**.

The one caller of that primitive was the engine's `D::OUT_OF_BAND` block, which is `false` for
`InlineDispatch` — the default, and what every `serve` entry point builds. So the primitive was
reachable only by an application that had already moved itself to another thread.

## Decision

### 1. Two doors, because the two needs are not the same need

| Need | Door | Why not the other one |
|---|---|---|
| Say something the moment a session comes up — the two `35=B`, a subscription, a state dump | **`Handler::on_logon`**, on the engine thread | a queue would make the first message of a session race the logon that caused it |
| A fill that lands later, a quote stream, an out-of-band `35=j` | **`Sender`**, from any thread | the engine thread is not where an application waits for a fill, and a handler that blocks there stops the session layer |

One door would have to serve both, and each shape is wrong for the other half. This is the same
split [ADR-0002](ADR-0002-engine-library-split.md) already made between inline and ring
dispatch, applied to origination rather than to delivery.

### 2. The application never learns a sequence number or a clock

Both doors end at `Session::send_application`, which assigns `34=` and `52=` itself and ignores
whatever the caller wrote there. An application that hands over a stale header cannot corrupt
the stream, because it is never told the two values it would have to get right.

This is not a new rule. It is `Reply`'s rule — `34`, `49`, `52`, `56` are session-owned —
extended to the message nobody asked for.

### 3. `on_logon` is asked repeatedly, and the engine owns the loop

```rust
fn on_logon(&mut self, nth: u32, sender: &[u8], target: &[u8], out: &mut [u8])
    -> Option<Range<usize>>;   // default: None
```

The engine calls it with `nth = 0, 1, 2, …` until it answers `None`, sending each message as it
comes. One message per call, one buffer reused, nothing accumulated.

**Why not hand the application a list to fill.** N messages of unknown length need a buffer
sized for the worst case, which is a constant nobody can pick, or an allocation, which
non-negotiable 1 forbids. Asking again costs one virtual call per message and bounds nothing
badly.

**The loop is bounded** by `MAX_ON_LOGON`, so a handler with a bug that never says `None`
cannot hold the engine thread. Reaching the bound is an event, not a silence.

### 4. `Sender` is not a fourth `Command`

`Command` is `Copy` and fixed-size and rides a queue of `Option<Command>`. A message body is
neither. `Sender` gets its own queue of fixed slots — `ORIGIN_CAPACITY` slots of `ORIGIN_LEN`
bytes, allocated once — and rides the same `Arc<Shared>` that `Admin` and `Observer` already
ride, with the same capability split: an `Observer` cannot send and a `Sender` can.

It copies `Commands` exactly where `Commands` was right: `submit` returns `false` at the call
when the queue is full, so **a lost origination is never silent**; the engine drains with
`try_lock` and never `lock`, so non-negotiable 4 holds; and a relaxed `waiting` load is read
before the lock is attempted, so an engine nobody sends through pays one load per turn rather
than a mutex. `drains()` is what keeps that claim falsifiable.

### 5. A message for a connection that has gone is dropped, on purpose

The same answer the `OUT_OF_BAND` block already gives, for the same reason: the session that
owned the sequence numbers went with it, and sending the message anywhere else would be worse
than not sending it.

### 6. The gate is somebody else's implementation

The two red steps in `scripts/interop.sh`'s acceptor role — `news` and `resend` — are the gate.
They are red today **by design**, and the tool says so. A `libquickfix` initiator is what has to
see them go green.

This is deliberate and it is the whole lesson of item 46: the capability was invisible to every
fixture this repository writes for itself, so the fixture that proves it is not one of them.

## Consequences

**Good.**

- The engine can be deployed as an acceptor by a desk. That is the positioning `DESIGN.md`
  opens with, and it was not true before.
- The two red interop steps go green, and the acceptor's independent opinion covers 7 / 7
  instead of 5 / 7.
- `Session::send_application` stops being reachable only from a thread hop. It was tested and
  correct and, through the default engine, dead.
- One builder serves both doors, so a message written for a reply and a message written for an
  origination differ only in where they are handed over.

**Bad, and priced.**

- **`crates/session` gains surface it does not use.** `Application::on_logon` has a default body
  returning `None` and `Session` never calls it; three `Config` getters exist for a
  `Counterparty` the session layer has no opinion about. The alternative was rewriting six entry
  points stabilised the day before, and the plan's Sửa 1 records the three rejected options. The
  cost is a trait method a reader of `session` cannot find a caller for from inside that crate.
- **An engine thread can now be held by a handler's `on_logon`.** `on_message` already had this
  property and `GUIDE.md` §2 already says so; this adds a second place to say it about.
- **A second bounded queue** on `Shared`, with its own capacity to document, its own full-queue
  answer to explain, and its own allocation-count case to keep.
- **`ORIGIN_LEN` is a fifth ceiling**, after the four ADR-0047 named. A message longer than a
  slot is refused at the call — visibly, unlike `Outbound::app` — but it is one more constant
  whose wrong value fails at a boundary rather than at a compile.
- **`MAX_ON_LOGON` is a number with no measurement behind it.** It is a guard against a bug, not
  a tuning knob, and it is labelled as one.

**Open.**

- Nothing here lets an application originate **before** logon, and nothing should: the session
  refuses it and returns `Link::Up`, which reads as success. Whether that silence deserves a
  counter is left to whoever first wants to send on a session that is not up.
- `on_tick` — a per-turn door for a counterparty that is connected and quiet — is deliberately
  not built. `Sender` answers that need from another thread, and a callback on the `hft` hot
  path needs its own measurement before it earns a place.
