# ADR-0054 — The handles are made before the engine, and the engine adopts them

- **Status**: Proposed — 2026-09-05
- **Date**: 2026-09-05
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0036](ADR-0036-one-mechanism-two-capabilities.md) — one cell, two
  capabilities, which this makes three reachable from the front door ·
  [ADR-0048](ADR-0048-an-engine-that-can-speak-first-has-two-doors.md) — door 2 is a handle ·
  [ADR-0038](ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md) — the only thing that ends
  a serving loop ·
  [ADR-0047](ADR-0047-the-four-buffer-sizes-are-the-callers-through-a-second-function.md) —
  its decision 2 is the one this contradicts, and only in part ·
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md) — both modes have front doors ·
  [DESIGN.md](../DESIGN.md) §3, §4 D15 · `STATUS.md` items 47, 30, 46 ·
  [plan](../plans/2026-09-05-handles-through-the-front-door.md)

## Context

`fixbolt::serve` builds its `Engine` inside itself and returns a `Shutdown` — after everything
has already ended. `Engine::observer()`, `Engine::admin()` and `Engine::sender()` all need a
`&mut Engine`, and a caller who came through the front door never holds one.

Read straight off the code, that is three capabilities missing from the library's whole public
surface:

- **Nothing can be seen.** No snapshot, no event stream, no `events_lost`. `STATUS.md` item 30
  — observing a running engine — is reachable only by somebody who builds an `Engine` by hand.
- **Nothing can be administered.** The 3 a.m. phone call (`Admin::set_next_out`) cannot be
  placed.
- **Nothing can be stopped cleanly.** `serve` returns when `shutdown_finished()` answers
  `Some`, and the only thing that starts that is `Admin::shutdown(grace)` (ADR-0038). So
  `docs/GETTING-STARTED.md:186` — *"`serve` returns when an operator stops the engine through
  `Admin::shutdown`"* — **described something nobody could do through the API that page
  teaches**. `tools/interop`'s acceptor role runs until something kills it, for the same
  reason.

The hole is older than item 46 and was found while closing it: every test of `observe` drives an
`Engine` directly, so no test ever asked *"and through `serve`?"*. That is the shape ADR-0034
already has a name for — a layer finished, and the layer above it never questioned.

## Decision

**The handles are made first, by the caller, and the engine adopts them.** No callback, no
second family of functions.

**1. `Handles` is a public newtype over the same `Arc<Shared>` the three methods create
lazily.**

```rust
pub struct Handles(Arc<observe::Shared>);   // Send + Sync + Clone
impl Handles {
    pub fn new() -> Self;            // ONE allocation, here, never on a turn
    pub fn observer(&self) -> Observer;
    pub fn admin(&self) -> Admin;
    pub fn sender(&self) -> Sender;
}
```

Nothing new is shared and no new mechanism is introduced: this is ADR-0036's one cell, handed
out before the engine exists rather than after.

**2. `Engine::adopt(&mut self, h: &Handles) -> bool` takes a cell rather than making one.** It
returns `false` and changes nothing if the engine already has a cell, because **two cells on one
engine are two truths**: the engine publishes into one and the operator reads the other, and
every symptom of that is silence. The three existing methods are untouched and keep working for
anyone driving an `Engine` directly — and after an `adopt`, they hand out handles onto the
adopted cell, because `get_or_insert_with` finds it already there.

**3. All ten front-door functions take `handles: Handles` as their last parameter** —
`serve`, `serve_with`, `serve_with_recovery`, `serve_with_recovery_with`, `serve_hft`,
`serve_hft_with`, `serve_hft_with_recovery`, `serve_hft_with_recovery_with`,
`connect_and_serve`, `connect_and_serve_with`.

**4. Not `Option<Handles>`.** One `Handles::new()` is one allocation at startup. Making it
mandatory is what makes *"you can stop this engine"* true for every caller rather than for the
ones who read far enough — and the documentation has been promising it since 2026-09-03.

**5. `dial` stops draining the caller's event ring.** It reads `LoggedOn` out of the ring today
to reset the reconnect ladder. Two readers on one cell **share** events rather than each seeing
them (`Observer::events` drains), so with the caller's own cell in place, every `LoggedOn`
`dial` consumed would be one the caller never sees. `Engine` gains `logons() -> u64`, a counter
incremented where the session is already found to have come up, and `dial` compares the number
across a turn instead of reading the stream. **Nothing new happens on a turn**: the increment
sits inside the `!was_on && is_logged_on()` branch that already exists.

**6. `shard::serve_sharded_hft(_with)` is out of scope and stays as it is.** N engines are N
cells, and a `ConnId` is unique only within one shard, so one `Handles` across a fan of shards
is a design question about identity and not a parameter. Named here as open.

### Why this and not the alternatives

| Shape | Why not |
|---|---|
| **A callback** — `on_ready(Handles)` on `Application` | Blocked by the layering. `Application` is a `fixbolt_session` trait and non-negotiable 2 keeps that crate pure; it cannot name `engine::Handles`. A parallel engine-side trait would be a second application interface for one instant. |
| **Six more twin functions** — `serve_observed`, … | Sixteen functions for one idea, and the twin axis is already spent on the four consts (ADR-0047 decision 2). |
| **Return the handles** — `serve` gives them back | It cannot: they would arrive when `serve` returns, which is after the engine has stopped. This is the whole defect. |
| **A `Serve` builder** | The better long-term shape, and **deferred, not rejected** — see below. |
| **Spawn the engine on a thread and hand back a join handle** | Changes who owns the thread, which is `GUIDE.md`'s central promise (`serve` runs on the caller's thread, pinned where the caller pinned it). A much larger decision than the one being taken. |

### What this does to ADR-0047, and what it does not

ADR-0047 decision 2 says *"the originals keep their exact signatures"*. **That half is dropped
here, deliberately.** Its other four decisions — four consts, `APP` last, `RX` sizing the
pre-session buffer, no default changes — are untouched and still hold. ADR-0047 is **not
superseded**: a decision about *how buffer sizes reach the engine* is not overturned by a
decision about *what a serving loop returns a handle on*.

The reason the signature can move at all is on the first page of `CLAUDE.md`: **nothing is
published**. That is a one-time budget, and this ADR spends part of it.

### The `Serve` builder, deferred with its condition written down

`Serve::new(addr, table, app).limits(l).capacity(64).log(f).handles(h).run()` would collapse ten
functions into one and make the next parameter free. It is not taken now because it is a second
public API for the same thing while this one is still moving, and because the const parameters
would have to live on the builder's type, which is exactly the shape ADR-0047 measured and
found the language fights.

**It is reopened the first time an eleventh parameter is wanted.** That is the condition; it is
written here so the next person does not re-derive it.

## Consequences

**Good**

- **`GETTING-STARTED.md:186` becomes true.** A clean stop through the front door is the first
  time in this repository that a serving loop ends without a signal.
- **Items 30 and 46 reach the library's users**, not only the engine's. The three capabilities
  stop being reachable-in-principle.
- **`tools/interop`'s acceptor role stops depending on `kill`**, so its transcript can print a
  real `Shutdown` and the gate can assert on it.
- **The two sources of `next_out` can be compared for the first time.** A resumed session's
  number from `Resumed::from_journal` and the live number from `Observer::snapshot` are now both
  reachable in one process — the assertion ADR-0053 recorded as unreachable.

**Bad, and named**

- **Ten breaking signature changes**, one of them contradicting an ADR from the same week.
  Every test, tool, example and documented snippet that calls a front door changes with them.
- **A caller who wants none of this still passes a `Handles`.** One allocation at startup and
  one relaxed load per turn, which `benches/alloc.rs` already measures for the `admin-idle`
  case; it is now the default rather than opt-in.
- **`dial` no longer reads its own events**, so a future ending that should reset the ladder has
  to remember to move the counter. A stream is self-describing and a counter is not.
- **`shard` is left behind**, and the asymmetry is now visible in the public API: ten functions
  take handles and two do not.

**Not decided here**

- The `Serve` builder (deferred, with its reopening condition above).
- One `Handles` across N shards, and what `ConnId` means there.
- Whether `Handles` should be able to say *"which engine am I attached to"* — it cannot today,
  and an unadopted `Handles` is silent rather than an error.
