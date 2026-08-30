# ADR-0007 — The dispatch ring is built from `AtomicU8`, not from `unsafe`

- **Status**: Accepted — 2026-08-30
- **Date**: 2026-08-30
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0002](ADR-0002-engine-library-split.md),
  [DESIGN.md §4 D4](../DESIGN.md), [plans/2026-08-30-engine.md](../plans/2026-08-30-engine.md)

## Context

[ADR-0002](ADR-0002-engine-library-split.md) makes `RingDispatch` — the application on its own
thread, behind an SPSC ring — the option for an application that may block. The engine plan
names the type and says nothing about how the ring is built, because the question did not come
up until the code did.

There are three ways to build one, and each crosses a line the plan did not authorise:

| Way | What it costs |
|---|---|
| `UnsafeCell<Box<[u8]>>` + `unsafe impl Sync`, copied with `copy_from_slice` | `unsafe`. Non-negotiable 8 wants a plan and a comment naming what proves it sound — a Miri run, a fuzz target, a test |
| A crate (`rtrb`, `crossbeam-queue`) | A dependency. `CLAUDE.md` §6 says every crate outside `codec` justifies each dependency in its plan, and this plan states the opposite: *"`std` is enough … so no dependency ADR is needed"* |
| `Box<[AtomicU8]>`, copied a byte at a time | Safe Rust, no dependency, and a slower copy |

The third is not obviously worse. The ring exists for an application that has already accepted
a thread hop; the same ADR-0002 that introduces it says the buyer is *"a QA app that may stall
for 40 ms"*. A copy measured in hundreds of nanoseconds is not what that application is
counting.

## Decision

**Build the ring from `Box<[AtomicU8]>` with `Release`/`Acquire` index publication. No
`unsafe`, no dependency. Publish the cost.**

Records rather than bytes: a `push` is one whole message or none at all, and a `pop` returns
one whole message or nothing. Half a FIX message on the application thread is worse than no
message.

The buffer is allocated once, in `ring::pair`, and never again — non-negotiable 1 holds for
both ends, and `crates/engine/benches/alloc.rs` counts it.

## Consequences

**Good**

- The whole engine is still `unsafe`-free outside a benchmark's allocator. That is a real
  property for a library whose readers will grep for `unsafe` before they read anything else.
- No dependency, so `--no-default-features` and the zero-dependency posture are untouched.
- The cost is a published number rather than an assumption: `[measured 2026-08-30]`
  **128.0 ns one way** and **242.5 ns for the round trip**, on a 163-byte `NewOrderSingle`,
  Apple M5, macOS 25.6, unpinned. Against `InlineDispatch`'s **2.7 ns**.
  `crates/engine/benches/dispatch.rs` asserts ceilings of 260 / 500 / 15 ns.

**Bad, and stated plainly**

- **The ring hop is ~50× the inline call, and roughly 0.8 ns per byte of that is the
  byte-at-a-time copy.** A `memcpy` ring would be a small multiple of the message's cache
  footprint instead — call it one order of magnitude cheaper. Anyone quoting this engine's
  ring hop is quoting a number that a different implementation would beat.
- The number is a laptop number. The one that decides is Linux, at `tools/w2w`.
- Two `AtomicUsize` cache lines are shared between the two threads and are not padded apart,
  so the producer's tail and the consumer's head can share a line and bounce. Not measured;
  named here so that it is not discovered as a surprise.

**Reversal, if the number ever matters**

A new ADR, an `unsafe` ring behind the same `push`/`pop` API, and a Miri run named in the
comment that authorises it — non-negotiable 8's actual requirement. Nothing above the
`ring` module changes, because nothing above it knows how the bytes move. **The point of
writing this down is that the reversal is cheap and the current choice is not a trap.**
