# ADR-0032 — Observation is a snapshot taken on request

- **Status:** Accepted
- **Date:** 2026-09-01
- **Supersedes:** nothing. **Related:** [ADR-0011](ADR-0011-a-full-ring-disconnects.md),
  [ADR-0025](ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md),
  [ADR-0027](ADR-0027-the-engine-owes-a-byte-stream-not-an-archive.md)
- **Plan:** [2026-09-01-operability.md](../plans/2026-09-01-operability.md), steps 1–2
- **Answers:** `STATUS.md` open item 30 (b) and (f)

## Context

`[verified 2026-09-01]` an `Engine`'s entire observable surface was three numbers —
`connections()`, `refused_connections()`, `sources_missing()` — none of them about a
session, and **all three readable only from the engine's own thread**. There was no way for
an operator to ask whether a session was logged on, what sequence numbers it held, or why a
counterparty had gone quiet.

The last of those is the sharp one. `Config::max_skew_ms` refuses a message whose `52=` is
too far from the engine's clock, and the refusal is **silent by protocol** — before a
`Logon` there is no session to answer with. On a box whose NTP has drifted, a counterparty
simply stops working and nothing anywhere says why. That is a production outage with no
first place to look.

Three constraints bound any answer:

1. **D8** — the engine thread does no work it does not have to, and never blocks in the
   kernel on the hot path (non-negotiable 4).
2. **Non-negotiable 1** — nothing on the hot path allocates.
3. **D1** — the session layer stays pure: no clock, no socket, no allocation.

## Decision

### 1. The engine publishes only when somebody has asked

`Observer::request()` sets a flag; the engine's `turn` reads it, and builds a snapshot only
if it is set. **The cost of being observable, while nobody is observing, is one relaxed
load per turn** — and on an engine whose `observer()` was never called, not even that: the
field is `Option::None`.

The alternative — publish every turn — was rejected because it pays for an operator who is
not there, on the one path the whole design exists to keep empty.

### 2. `try_lock`, never `lock`, and a refusal leaves the request standing

The engine may not block, so it never waits for the reader's mutex. If the reader holds the
cell at that instant the engine skips publishing and **leaves `wanted` set**, so the next
turn does it: the reader is not starved and the engine is never stopped.

A seqlock over `UnsafeCell` would avoid the mutex entirely.
[ADR-0007](ADR-0007-spsc-ring-without-unsafe.md) already settled this house's preference —
safe first, `unsafe` only when a measurement asks — and no measurement has asked.

### 3. Not the ring

D10's ring is the application path, and [ADR-0011](ADR-0011-a-full-ring-disconnects.md)
says a full ring **disconnects the session**. An operator asking a question must not be able
to drop a connection. Two mechanisms, two purposes, and this one does not get to share that
risk.

### 4. Fixed size, and *"there were more"* is a fact, not a failure

`Snapshot` is `[SessionSnapshot; 64]` plus a `truncated` flag — non-negotiable 1 forbids the
`Vec`. `hft` carries a hard ceiling of four sessions
([ADR-0025](ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md));
`standard` carries none, so an engine may legitimately hold more sessions than one snapshot
can describe. It reports that it did.

Sixty-four is sixteen times `hft`'s ceiling. A `standard` engine holding more than that has
an operator problem a longer array would not solve.

### 5. The measured clock skew is recorded on refusal, not only on acceptance

`Session::last_skew_ms()` is written **before** the `SendingTime` verdict, for every inbound
message whose `52=` parses. Recording it only on acceptance would have made it `None` in
exactly the case it exists to explain.

The session layer stays pure: this is an `Option<i64>` on a struct, computed from `now_ms`,
which already arrives as `Input::Tick`. No clock read, no allocation.

### 6. The health probe is a pure function on the snapshot

`Snapshot::healthy()` — at least one session, every one logged on, and both
should-be-zero counters at zero. Not a second mechanism with its own I/O: a health endpoint
and an operator's `Debug` print read the same data and **cannot disagree**.

Truncation is deliberately not part of it. More sessions than the array can list is a
reporting limit, not a sick engine.

## Consequences

**Good**

- An operator can answer, from another thread, while the engine runs: is this session
  logged on, what sequence numbers does it hold, is output backed up, and **how far is our
  clock from theirs**.
- `[measured 2026-09-01]` `benches/alloc.rs` cases `observe-idle` and `observe-asked` both
  read **0**. Being watched allocates nothing; watching on every single turn allocates
  nothing.
- The mechanism is one `Arc<Shared>`, allocated once in `observer()`. Steps 3–6 of the
  operability plan — live sequence-number administration, ordered shutdown, the event
  stream — reuse it rather than inventing a second channel.
- `Snapshot` is plain `Copy` data. Whoever wants Prometheus, JSON or a log line writes it;
  this crate does not pick a metrics format.

**Bad, and named**

- **A snapshot is a moment old by the time it is read**, always. `request()` returns the
  most recent published one and asks for a fresh one; it does not wait. A caller wanting a
  snapshot taken *after* its call asks twice. This is deliberate — blocking in either
  direction is the coupling the design exists to avoid — but it means the numbers an
  operator sees are never exactly now.
- **The cost when somebody *is* asking has not been measured on the §9 machine.** The
  allocation half is proven; the nanosecond half is not. The plan's reversal 2 — make
  `publish` unconditional and watch `benches/turn.rs` slow down measurably — is the
  measurement that would close it, and it needs Linux. **Until then, "one relaxed load"
  covers the idle path only.**
- **64 is a number with an argument behind it, not a measurement.** A `standard` deployment
  with more counterparties than that gets a truthful but partial answer, and would want a
  paged interface this design does not have.
- `MAX_SESSIONS` slots make `Snapshot` about 2.5 kB. It is copied on every publish and on
  every read. That is fine on the stack at this size and would not be at ten times it.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| Publish every turn into a double buffer | Pays for an absent observer on the hot path. D8. |
| Deliver events through D10's ring | ADR-0011: a full ring disconnects. An operator's question must not drop a session. |
| A seqlock over `UnsafeCell` | Faster and needs `unsafe`. ADR-0007's preference stands until a measurement asks. |
| `tracing` behind a feature flag | The engine never logs on the hot path, and a log line is not a state you can query. |
| A `Vec<SessionSnapshot>` | Allocates. Non-negotiable 1. |
| The operator reads `Engine` directly under a lock | Blocks the engine thread. Non-negotiable 4. |
