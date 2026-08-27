# nanofix — Design

> **This document is deliberately mostly empty.** The design has not been settled. Writing
> a confident architecture here before the decisions are made would produce exactly the
> stale document [CLAUDE.md §4](../CLAUDE.md) forbids.
>
> What is settled lives in [decisions/](decisions/). What is known about the landscape
> lives in [reference/prior-art.md](reference/prior-art.md).

## 1. Scope

**In**

- FIX 4.4, tag=value encoding.
- **Acceptor (server) side.** This is the gap in the Rust ecosystem and the reason the
  project exists.
- A session layer that passes the 59 FIX 4.4 acceptance definitions from QuickFIX.
- Low latency as a measured, gated property — not an aspiration.

**Out, for now**

- FIX 5.0 / FIXT 1.1. Add only when something needs it.
- FAST, SBE, FIXML.
- Kernel bypass (DPDK, OpenOnload). Not until an ordinary TCP path has been measured and
  found to be the limit.
- Initiator side beyond what the acceptance tests require.

## 2. Settled decisions

| Decision | Where |
|---|---|
| Clean-room engine; QuickFIX assets taken as data and as a test oracle, not as source | [ADR-0001](decisions/ADR-0001-relationship-to-quickfix.md) |
| Codec parses and serialises in place at the I/O buffer, `hffix`-style, no heap on the hot path | [ADR-0001](decisions/ADR-0001-relationship-to-quickfix.md) §Decision 4 |

## 3. Open design questions

Each of these needs an answer before the crate it governs can be planned.

| # | Question | Governs |
|---|---|---|
| 1 | What is the latency target, at what percentile, measured how? | Everything. Without a number there is no way to tell a good design from a bad one |
| 2 | Threading model: thread-per-connection, thread-per-core, or a single reactor? | The whole I/O layer |
| 3 | Runtime: `std::net` + threads, `tokio`, or `monoio`/io_uring? | Dependency surface and the latency floor |
| 4 | How are messages represented to the application — borrowed view into the buffer, or an owned decoded struct? | The public API, and whether zero-copy survives contact with users |
| 5 | Is the message store durable, and if so how, given that `Sync()`-per-write is what caps QuickFIX? | Session recovery, and the tail of the latency distribution |
| 6 | Code generation from `spec/FIX44.xml`: build script, or a checked-in generated crate? | Build times, and whether users need the XML |

## 4. Non-negotiables

Carried forward from the reasoning in ADR-0001; each will get a test or a lint before it is
called enforced.

1. **No heap allocation on the parse or serialise hot path.** Proven by a benchmark that
   counts allocations, not by inspection.
2. **The acceptance definitions are the session layer's gate.** A session change that has
   not run them is not done.
3. **No performance claim without a committed benchmark that produced it.** Every number in
   this repository names the command that generated it, or is marked as someone else's.
