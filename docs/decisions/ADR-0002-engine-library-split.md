# ADR-0002 — Split the engine from the library, with a ring buffer between them

- **Status**: Proposed
- **Date**: 2026-08-27
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0001](ADR-0001-relationship-to-quickfix.md)

## Context

In QuickFIX — C++, Java and Go alike — the application's `fromApp` callback runs **on the
session's own thread**. Everything the application does happens inside the loop that also
sends heartbeats, advances sequence numbers and answers `TestRequest`.

The consequence is well known to anyone who has operated one: an application that blocks —
a database call, a lock, a log flush, a GC pause — stops the session layer too. The
counterparty sees missed heartbeats and disconnects. The bug is in the application; the
symptom is a protocol failure, and the two are hard to tell apart from the outside.

[Artio](https://github.com/artiofix/artio) (Real Logic, Apache-2.0, Java) is the open-source
engine that solves this structurally rather than by asking applications to behave. It splits
into two process types:

- **`FixEngine`** — the gateway. Accepts and manages TCP connections from counterparties,
  owns session lifecycle, and handles persistence and archival through a
  `RecordingCoordinator`.
- **`FixLibrary`** — the application interface. Runs business logic through session handlers,
  in the same JVM or a different one.

They communicate over **Aeron**, giving zero-copy message passing between processes. A
session begins owned by the Engine; a Library then **requests ownership** of it.

Artio is the only engine in the survey ([prior-art.md](../reference/prior-art.md)) with this
property. It is also the only one designed after the problem was widely understood.

## Decision

**Adopt the engine/library split, including Artio's ownership handover. Start in-process,
over an SPSC ring buffer, with the boundary shaped so that moving the library to another
process is a transport swap rather than a rewrite.**

Concretely:

1. `engine` owns the listening socket, the TCP connections, the session state machines
   ([D1](../DESIGN.md#d1--the-session-layer-is-a-pure-state-machine-with-no-io)) and the
   journal. It never calls application code.
2. `library` owns the `SessionHandler` the application implements, and runs on its own
   thread.
3. Between them: a single-producer single-consumer ring buffer carrying framed bytes, not
   Rust references. Bytes cross the boundary, so the boundary can later be shared memory or
   a socket without changing either side's types.
4. A session is **owned by the engine on connect**. A library **requests ownership**; until
   it is granted, the engine answers session-level traffic on its own. Copied from Artio
   deliberately — it is what makes engine-only operation (heartbeats during library restart)
   possible.
5. **In-process first.** One binary, two threads. Cross-process is not built now.

## Consequences

**Good**

- **An application that blocks does not stall the session layer.** Heartbeats keep flowing,
  sequence numbers keep advancing, the counterparty stays connected. This is the property
  the whole ADR exists for.
- A library can be restarted while the engine holds the sessions up — Artio's ownership
  model is precisely what enables that.
- Business logic gets its own thread, so it can be profiled, throttled and reasoned about
  separately from protocol timing.
- The pure session machine ([D1](../DESIGN.md#d1--the-session-layer-is-a-pure-state-machine-with-no-io))
  becomes testable without any of this being built, because it never touched I/O anyway.

**Bad — and these are real**

- **A hop is added to every message.** A ring-buffer handoff is on the order of tens of
  nanoseconds, against a ~139 ns parse — so it is a **meaningful fraction, not a rounding
  error.** For an application that only needs to see messages, this cost is pure overhead
  compared with a direct callback. It is accepted because an application that blocks is a
  worse outcome than one that is 20% slower, but it must be measured, not assumed.
- **The API is harder.** A direct callback hands the application a borrowed view into the
  read buffer. Across a ring buffer, bytes are copied into the ring and the borrow is
  reconstructed on the other side. Zero-copy stops at the boundary by construction.
- **Two threads is more to get wrong** than one: shutdown ordering, backpressure when the
  library falls behind, and what happens when the ring fills. That last one needs an explicit
  policy — block, drop, or disconnect — and none of the three is obviously right.
- **Ownership handover is genuine complexity**, and it is complexity for a capability
  (library restart without dropping sessions) that nobody has asked for yet. This is the
  weakest part of the decision and the first thing to cut if it proves expensive.
- Artio's split is justified partly by the JVM: separating processes isolates GC pauses.
  **Rust has no GC**, so that half of Artio's motivation does not transfer. What remains is
  the blocking-application argument — real, but a smaller prize than it is for Java.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| Direct callback on the session thread, as QuickFIX does | Simplest and fastest, and it is exactly the design whose failure mode motivated this ADR. Retrofitting the split later means rewriting every place that assumed the application runs on the session's thread |
| Split, but cross-process from day one | Buys process isolation nobody has asked for, at the cost of shared-memory transport, a separate lifecycle and a much harder debugging story. In-process first, with the boundary drawn correctly, keeps the option without paying for it |
| Give the application a thread pool behind the callback | Solves blocking without a boundary, but reintroduces ordering problems — FIX messages on a session are ordered, and a pool is not |

## Open questions

1. **What is the ring-buffer handoff cost, measured on this design?** Until it is benchmarked
   against a direct callback on the same machine, the "20% slower" figure above is arithmetic,
   not evidence. This is the first benchmark to write after `codec`.
2. What is the policy when the ring fills — block the engine, drop the message, or disconnect
   the session? Each is defensible; the choice needs a stated rationale.
3. Should ownership handover ship in v1 at all, or wait until something needs it?
