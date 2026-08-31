# ADR-0002 — Dispatch is a trait: inline by default, ring buffer as the option

- **Status**: Accepted — 2026-08-27
- **Date**: 2026-08-27
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0001](ADR-0001-relationship-to-quickfix.md),
  [ADR-0011](ADR-0011-a-full-ring-disconnects.md) — *what happens when the ring this ADR
  introduced fills up.* Added as a cross-reference on 2026-08-31; **nothing in the text
  below is changed**, per `CLAUDE.md` §5.

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

**Revised 2026-08-27, same day, after review against the HFT latency budget
([DESIGN.md §8](../DESIGN.md#8-latency-budget-on-kernel-tcp)).** The first draft made the ring
buffer the default. That was wrong for the stated positioning — the fastest acceptor on kernel
TCP — and this section records the reversal rather than hiding it.

**Make dispatch a trait. `InlineDispatch` — the handler runs on the engine thread, zero
hops — is the default. `RingDispatch` — Artio's engine/library split over an SPSC ring — is
the option for applications that may block.**

Concretely:

1. `engine` owns the listening socket, the TCP connections, the session state machines
   ([D1](../DESIGN.md#d1--the-session-layer-is-a-pure-state-machine-with-no-io)) and the
   journal. After the session machine has run, it hands the message to a `Dispatch`.
2. **`InlineDispatch<H>`** calls the application's `SessionHandler` on the engine thread,
   immediately, with the borrowed `MessageView`. No copy, no hop. This is the shape every
   low-latency engine converges on: `recv → parse → decide → encode → send` on one core.
3. **`RingDispatch`** copies framed bytes into an SPSC ring; a library thread consumes them.
   Bytes cross the boundary, not references, so the boundary can later be shared memory or a
   socket without changing either side's types.
4. Under `RingDispatch`, a session is **owned by the engine on connect** and a library
   **requests ownership** — Artio's model, kept because it is what makes engine-only
   operation (heartbeats during a library restart) possible. Under `InlineDispatch` there is
   no ownership question.
5. **In-process only.** Cross-process is not built now; the ring's byte-oriented boundary
   keeps the option.

## Consequences

**Good**

- **The default path has zero handoffs.** An application that answers in nanoseconds pays
  nothing for a capability it does not use.
- **An application that blocks can opt into not stalling the session layer.** Under
  `RingDispatch` heartbeats keep flowing, sequence numbers keep advancing, the counterparty
  stays connected. A simulator serving a QA application wants exactly this.
- A library can be restarted while the engine holds the sessions up — Artio's ownership
  model is precisely what enables that.
- Business logic gets its own thread, so it can be profiled, throttled and reasoned about
  separately from protocol timing.
- The pure session machine ([D1](../DESIGN.md#d1--the-session-layer-is-a-pure-state-machine-with-no-io))
  becomes testable without any of this being built, because it never touched I/O anyway.

**Bad — and these are real**

- **Two dispatchers is two code paths to keep correct**, and the inline one lets a careless
  handler stall the session layer — the very failure this ADR set out to prevent. The
  documentation has to say so at the top of the `SessionHandler` trait, in bold.
- **The ring hop is not "tens of nanoseconds".** The first draft said that. With the consumer
  on another core the cost includes a cache-line transfer of the frame and of the ring's
  indices — realistically 200–500 ns, several times a parse. It is now a published
  benchmark (`benches/dispatch.rs`), not an estimate.
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
| Direct callback **only**, as QuickFIX does | Simplest and fastest, and it is now the default — but as the *only* option it leaves a blocking application with no recourse except a thread pool, which breaks per-session ordering |
| Ring buffer **only** — the first draft of this ADR | Pays 200–500 ns on every message for isolation most latency-sensitive applications do not want. Justified for Artio by JVM GC isolation, which does not transfer to Rust |
| Split, but cross-process from day one | Buys process isolation nobody has asked for, at the cost of shared-memory transport, a separate lifecycle and a much harder debugging story. In-process first, with the boundary drawn correctly, keeps the option without paying for it |
| Give the application a thread pool behind the callback | Solves blocking without a boundary, but reintroduces ordering problems — FIX messages on a session are ordered, and a pool is not |

## Open questions

1. **What is the ring-buffer handoff cost, measured on this design?** `benches/dispatch.rs`
   answers it — inline versus ring, same machine, same message. Until it exists the 200–500 ns
   above is literature, not evidence.
2. What is the policy when the ring fills — block the engine, drop the message, or disconnect
   the session? Each is defensible; the choice needs a stated rationale.
3. Should ownership handover ship in v1 at all, or wait until something needs it?
