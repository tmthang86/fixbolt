# ADR-0011 — A full ring ends the connection, and the ring is sized so that is rare

- **Status**: Proposed — 2026-08-30
- **Date**: 2026-08-30
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0002](ADR-0002-engine-library-split.md),
  [ADR-0007](ADR-0007-spsc-ring-without-unsafe.md),
  [DESIGN.md §4 D4 and D10](../DESIGN.md),
  [plans/2026-08-30-ring-full-policy.md](../plans/2026-08-30-ring-full-policy.md)

## Context

`STATUS.md` open item 5 has been open since ADR-0002: **what should happen when the application
behind the ring falls behind?** It is not the question `DESIGN.md` D10 answered. D10 asked *the
consumer on the wire is slow* and shipped `Backpressure::{Disconnect, Queue, Block}` for the
socket. This is the other side of the engine.

Today's answer is a counter. `RingDispatch::refused()` goes up, nothing reads it, and its own
doc comment says the question is still open. **A message counted there is a message the session
accepted, numbered, journalled and acknowledged by sequence number, that the application never
saw.** For an order flow that is not backpressure; it is loss, and silent loss.

### The measurement that changes the question

`[measured 2026-08-30]` `crates/engine/benches/ring_full.rs`, Linux 6.18 x86_64, 4 vCPU
container, `cargo 1.98.0`. A stalled application — a consumer that never drains — against the
capacity `benches/dispatch.rs` measures the hop at:

| | |
|---|---|
| Ring capacity | **65 536 bytes** (`1 << 16`) |
| Message | 149 bytes + a 32-byte header |
| Messages accepted before the first refusal | **352** |
| Time to fill at this end's full rate | **56.7 µs** (160 ns per message) |

The benchmark asserts both that the ring **refused** and that it **accepted first**: a ring that
rejected everything from the first message would print a plausible-looking number, and that is
the same shape as a benchmark reporting zero allocations for a path that never ran.

**56.7 microseconds is the whole slack.** ADR-0002 justified the ring on the grounds that *an
application that stalls does not stall the session layer*, and the engine plan's own reasoning
priced the hop's 240 ns against an application that "may stall for milliseconds". At this
capacity a one-millisecond stall overflows the ring roughly **eighteen times over**. The ring as
sized does not buy what it was bought for.

That is why the capacity is part of this decision and not a tuning detail left for later.

## Decision

**1. A full ring ends the connection, by default.** `Backpressure::Disconnect` is already D10's
default for the socket and the reasoning carries: a counterparty that is told the session is
gone can resend, reconnect and reconcile by sequence number. One that is silently missing a
message it was acknowledged for cannot, and will not find out until a reconciliation break.

**2. The refusal is never silent, whatever the policy.** `refused()` becoming non-zero must be
observable from outside the engine — the counter alone is a struct field nobody reads. This is
the part that is not negotiable even if the policy below is revisited.

**3. The default capacity rises to `1 << 22` (4 MiB).** At the measured rate that is roughly
**3.6 ms** of slack rather than 56.7 µs, which is the order the ring was chosen for. The cost is
4 MiB of resident memory per ring, pre-faulted at startup like every other buffer.

**4. `Block` is not offered here.** On the socket side D10's `Block` spins until there is room,
which is defensible because the peer is draining. Spinning until an *application thread* drains
makes the engine thread's progress depend on code the engine does not control, and
non-negotiable 4's gate (`scripts/check-no-kernel-sleep.sh`) cannot distinguish a spin that
finishes from one that does not.

## Consequences

**Good**

- The failure is loud and recoverable. A disconnect is a protocol event both ends understand;
  a dropped message is not.
- The number that decides is published, with its machine and its benchmark, rather than assumed.
- Sizing and policy are decided together, so the ring is not left justified by a stall duration
  it cannot actually absorb.

**Bad, and stated rather than discovered later**

- **4 MiB per ring is a real cost** and it scales with connection count if a deployment ever
  gives each connection its own ring. Nothing here measures the memory pressure that creates.
- **A disconnect on ring overflow turns a slow application into an outage.** An application that
  pauses for 4 ms under GC, or under a lock, now drops the session rather than lagging. That is
  the right trade for order flow and the wrong one for a market-data fan-out, and this ADR
  chooses for the former.
- **The measurement is from a shared 4 vCPU container**, not a machine matching `DESIGN.md` §9.
  It bounds a count and a duration under saturation rather than a latency, which is why it is
  usable here — but the fill rate on a faster machine is faster, so 3.6 ms is an upper estimate.
- **No real application has ever stalled against this ring.** Both the policy and the capacity
  are chosen from one synthetic saturation run and from reasoning about order flow.

## Open questions

1. **Should the capacity be per-connection or shared?** Everything above assumes one ring for
   the engine. 4 MiB × 256 connections is not a default anyone would want.
2. **How does the refusal reach the outside?** A counter, a callback, a log line behind the
   `tracing` feature — the engine may not log on the hot path, so this needs its own shape.
3. **Is 3.6 ms enough?** It is three orders above the 56.7 µs measured and one order above the
   "milliseconds" ADR-0002 assumed. Nobody has measured a real application's worst pause.
