# ADR-0035 — An event is pushed, and a loss is counted

- **Status:** Accepted
- **Date:** 2026-09-02
- **Related:** [ADR-0011](ADR-0011-a-full-ring-disconnects.md),
  [ADR-0027](ADR-0027-the-engine-owes-a-byte-stream-not-an-archive.md),
  [ADR-0030](ADR-0030-one-engine-holds-many-counterparties.md),
  [ADR-0032](ADR-0032-observation-is-a-snapshot-taken-on-request.md)
- **Plan:** [2026-09-02-why-a-connection-ended.md](../plans/2026-09-02-why-a-connection-ended.md)
- **Closes:** `STATUS.md` open item 30 (d)

## Context

`Link::Dropped` is **one bit**. `[verified 2026-09-02]` the session returns it from eighteen
places — a wrong `BeginString`, a wrong identity, a `SendingTime` too far out, a sequence
number already used, a first message that is not a `Logon`, an hour outside the schedule, the
counterparty's own `Logout`, a heartbeat that never came — and the engine adds a few of its
own: a full ring (ADR-0011), a slow consumer (D10), a dead socket. **Nothing at the other end
told them apart.**

Six acceptance definitions expect **no response at all**, so 59/59 is blind to every one of
these. The gate that guards the session layer cannot see the difference between the two most
common production faults.

That cost hours **twice in one working session**, and both write-ups end on the same sentence:

| | |
|---|---|
| A schedule test passed on `max_skew_ms` rather than on the schedule — two time rules, one silent observable | [two-time-rules-share-one-observable](../reference/two-time-rules-share-one-observable.md) |
| A `Logon` refused in silence for a `FieldIndex` too small, while the failure message blamed a registry that did not exist | [silence-before-a-logon-has-many-causes](../reference/silence-before-a-logon-has-many-causes.md) |

The cheapest structural defence is to make the reason observable.

## Decision

### 1. The session records the reason; the engine reads it back

```rust
#[non_exhaustive]
pub enum DropReason { WrongBeginString, NotALogon, /* … */ SlowConsumer }
pub fn last_drop_reason(&self) -> Option<DropReason>;
```

A **fieldless** enum, so D1 holds unchanged: no clock, no allocation, no `format!`. `Link`'s
signature does not change, which would have been an API break reaching every call site for a
diagnostic.

This is the shape ADR-0032 decision 5 already chose for `Session::last_skew_ms`, and it is
reused rather than reinvented for the same reason: the session knows the fact, the engine
knows the thread.

**`From<Refusal> for DropReason` is exhaustive with no `_` arm.** A new refusal that is not
given a name will not compile — which is the only mechanism here that survives the next person.

### 2. Events are **pushed**; snapshots are pulled

ADR-0032 made observation a snapshot **taken on request**, and that is right for a state you
can ask about later. It is wrong for an ending: a snapshot not asked for at the right moment is
a stale number, but an event not recorded is **gone**. So the engine writes an `Event` when a
session's state changes, whether or not anybody is reading.

The cost is one `try_lock` per state change. **State changes are rare** — a logon, a logout, a
disconnect — and never per message; D8 forbids that outright.

### 3. Same channel as the snapshot, not a second one

`Events` lives inside the existing `observe::Shared`, behind the same `Arc`, reached through
the same `Observer`. ADR-0032 paid once for `try_lock`-never-`lock`, a fixed array, and a
handle that is `Send + Sync`; two parallel mechanisms are two things that will disagree.

### 4. `try_lock`, never `lock`, and a full ring is not an error

Non-negotiable 4. A refused lock, or a ring with no room, bumps `lost` and the turn continues.
**An observer may never drop a session** — ADR-0011 decided that a full *output* ring
disconnects because the counterparty would otherwise be lied to; nothing about somebody
watching justifies the same cost.

### 5. A loss that is not counted is worse than no stream at all

`EVENT_CAPACITY = 256` and an `AtomicU64 lost`. An event stream that loses silently is a source
an operator will trust and should not — precisely the failure mode of the two write-ups above.
So the counter has **its own test, which drives the ring past full on purpose** rather than
hoping it never happens.

### 6. `EndedWithoutReason` is a variant, not a fallback to a guess

If an ending reaches the stream with no recorded cause, it says so. Naming one at random —
picking the most likely — is how a diagnostic becomes a lie under load.

### 7. The engine names what the session cannot know

Three endings are the engine's own decisions, and the session has no way to see them:

- `DuplicateIdentity` — ADR-0030's single-logon rule, refused before the session judges
  anything. `Session::disconnect_with` lets the engine supply the reason.
- `SlowApplication` and `SlowConsumer` — D10's backpressure paths send their own `Logout`, so
  they use `note_drop_reason` and leave the disconnect funnel alone.

**And a cause already known is never replaced.** `[measured 2026-09-02]` before that rule,
`disconnect()` overwrote every specific reason with `TransportClosed` — the socket taking the
blame for eighteen different faults, one of which was a policy decision the engine made itself.

## Consequences

**Good**

- An operator on another thread can tell *check your NTP* from *check your venue calendar*.
  Both are silence on the wire; they are different people's problems.
- `[measured 2026-09-02]` `benches/alloc.rs` cases `events-idle` and `events-busy` both read
  **0**, and `events-busy` asserts the stream recorded something inside the counting window, so
  the zero is the engine's path and not an empty measurement.
- The engine's own refusals stopped blaming the network. That was found by a test failing for a
  reason unrelated to its name — see the reference write-up.
- Events are read **while the engine turns**, from another thread, in every test. A test that
  stops the engine first passes against a mechanism that is not thread-safe at all.

**Bad, and named**

- **`try_lock` over `lock` is not proven by any test.** Reversal 3 in the plan — swap one for
  the other — turns nothing red, because a contended blocking acquisition needs a scheduler the
  suite does not control. It is a reading of the code, and it goes to *Not proven*.
- **256 events is a policy with no evidence behind it.** It is one page of ring for a
  once-per-connection event; nothing measured says a busy venue does not overflow it during a
  mass reconnect, which is exactly when an operator most needs the stream.
- **Only three kinds exist.** `LoggedOn`, `Ended`, `EndedWithoutReason`. The plan's own scope
  named gap-detected, resend-issued and reject-sent, and none of them are here: they are
  message-rate, and the line between *rare* and *hot path* has not been drawn for them.
- **`Event` carries `ConnId`, not an identity.** Correlating an ending with a counterparty
  needs a snapshot taken before it, and after the disconnect that correlation is gone.
- **`DropReason` is `#[non_exhaustive]`**, so a downstream `match` needs a `_` arm and gains
  nothing from the exhaustiveness the engine enjoys internally.
- **Nothing exports it.** `Event` is data; JSON, Prometheus and a log line are all somebody
  else's, by ADR-0027's rule that the engine owes a stream and not an archive.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| A field on `Link::Dropped(DropReason)` | An API break at every call site, for a diagnostic. And `Link` is returned on the hot path |
| `tracing` behind a feature flag | Not an audit trail, and D8 forbids logging on the hot path. It was already rejected for item 30 |
| Reuse D10's output ring | ADR-0011: a full ring disconnects. An observer that can drop a session is worse than no observer |
| A callback per event | Runs arbitrary code on the engine thread. Everything non-negotiable 4 exists to prevent |
| Grow the buffer instead of counting losses | Non-negotiable 1, and it only moves the silent loss further out |
| Let `lost` be inferred from a sequence number gap | Requires a reader that never restarts, and hides the loss from the first reader that does |
