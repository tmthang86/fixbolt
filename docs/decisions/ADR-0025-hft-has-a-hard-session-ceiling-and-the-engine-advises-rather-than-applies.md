# ADR-0025 — `hft` has a hard session ceiling, and the engine advises rather than applies

> **Status:** **Proposed — 2026-09-01.**
>
> **Deliberately not self-accepted under the standing delegation.** The owner is forming this
> decision in conversation now, and decision 1's number can still move — the busy-path
> measurement it rests on has not been taken. Accepting it today would be accepting a number
> before the run that settles it, which is what `CLAUDE.md` §10 exists to stop.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md),
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md),
  [ADR-0014](ADR-0014-standard-mode-blocks-on-poll.md),
  [ADR-0015](ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md),
  [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md),
  `DESIGN.md` §8 and D8, `GUIDE.md` §1a, `PRD.md` §2,
  [measured-costs.md](../reference/measured-costs.md), `STATUS.md` open items 6, 14, 21

## Context

The question that produced this: **can the engine detect the machine at startup and configure
itself — mode, cores, bypass — instead of making the caller do it?**

Everything about that is attractive and most of it has already been decided against, in four
places and for one reason: an engine that picks silently is an engine whose latency claim
nobody can check. What follows separates the part that survives from the part that does not.

### What blocked an auto-tuner, and what dissolved it

An `advise(sessions)` has to compute from two numbers this project does not have:

| | Range | Why it is a range |
|---|---|---|
| the `hft` / `standard` crossover | **N ≈ 4 … 11** | `[measured 2026-08-31]` `Engine::turn` is **448.9 ns** — `crates/engine/benches/turn.rs`, AMD Ryzen 7 3700X, ADR-0021 §9 line, `check-machine.sh` **pass 11 fail 0 unknown 1**, median of 24 qualifying runs; the `epoll`-class wakeup it is weighed against is **2–5 µs from the literature**, never measured here — ADR-0014 open question 1 |
| the L2 cache wall | **N ≈ 9 … 128** | `[measured 2026-08-30]` `Connection` is 53.3 KiB against a 32 KiB `L1d` — `size_of` and a pointer-chase curve on the same AMD Ryzen 7 3700X, [measured-costs.md](../reference/measured-costs.md); **how much of it a message touches is unmeasured**, and the two bounds are 14× apart |

Encoding either range behind an API would ship a guess wearing an authoritative signature — and
once it is `advise(100)` rather than a sentence in `GUIDE.md`, a reader can no longer tell a
guess from a measurement.

**A ceiling of four dissolves both, and not by rounding.** `2000 / 448.9 = 4.46`, so:

| N | sweep | vs 2 µs (pessimistic) | vs 5 µs (optimistic) |
|---|---|---|---|
| 3 | 1 347 ns | wins | wins |
| **4** | **1 796 ns** | **wins** | **wins** |
| 5 | 2 245 ns | **loses** | wins |
| 11 | 4 938 ns | loses | wins |

**Four is the largest N that wins under every reading of the number nobody measured**, and it
is below the pessimistic bound of the cache wall as well. The argument stops being *tune along
a curve* and becomes *stay in the region where the curve's shape cannot change the answer* —
which needs no measurement to be sound.

**The remaining uncertainty runs one way only.** That 448.9 ns is an **idle** turn: it is one `recv`
that finds nothing and never touches the session, the journal or the template. If a busy turn
costs more per session, the true crossover is **below** 4.46, never above. So four is *the
largest ceiling defensible from what is known*, not a proven optimum, and the measurement that
is missing can only lower it.

## Decision

**1. `hft` carries a hard ceiling of four sessions per engine, and it refuses rather than
degrades.** The fifth connection to one `hft` engine is refused with a typed error naming the
ceiling. It does **not** silently become a `standard` engine, and the mode never changes while
the process runs.

Refusal rather than degradation is what keeps three earlier decisions intact:
[ADR-0013](ADR-0013-two-modes-standard-and-hft.md) decision 4 — a `standard` figure and an
`hft` figure are not comparable, so a mode that changes underneath a measurement destroys the
measurement; [ADR-0014](ADR-0014-standard-mode-blocks-on-poll.md) decision 2 — *"an engine that
quietly falls back … looks like it is doing what you asked and it is burning a core"*; and
[ADR-0015](ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md) decision 3 — a failure
stops startup, it never runs degraded.

**The ceiling is provisional and the constant says so.** It is lowered, never raised, without
the busy-path measurement described in the context.

**2. The engine detects and advises. It never applies.** `Machine::probe()` reads what
`check-machine.sh` reads — the §9 rows, the topology, whether this is a guest — and returns a
**printable value**. `advice.suggest(sessions)` returns a `ShardPlan`, a mode and a
`presession::Limits` **that the caller must pass in**. Nothing is applied implicitly.

This is the inverse of a function that already exists: `ShardPlan::validate()` is *"the engine
refuses your plan"*; `suggest()` is *"here is a plan that would validate"*. The decision stays a
value in the caller's hands, so the mode is still visible at the call site, `CLAUDE.md` §2 rule
4's two machine gates still read the mode back out of the binary, and every published number
still names the settings that produced it.

**3. `ADR-0015` decision 1 is untouched: the engine still never picks a core.** A ceiling says
*how many*, never *which*. `suggest()` may propose core ids, and the caller passes them to
`ShardPlan` explicitly, where all five rejections still apply. *"Auto-selection is how a system
that merely looks pinned gets built"* remains the rule; a suggestion the caller has to accept in
their own source is not auto-selection.

**4. `presession::Limits` keeps having no defaults.** `logon_ms` and `pending` are
denial-of-service parameters, not latency parameters, and nothing about a session ceiling
implies either ([ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)
decision 4). `suggest()` may not invent them.

**5. Kernel-bypass detection is out of scope and stays in phase 3.** Onload needs no engine
support at all — `onload ./engine` runs it unchanged over the socket API, so there is nothing to
detect and nothing to build. `ef_vi` would be a second `impl Transport` behind a feature flag
(D5); it does not exist, and nobody here has a NIC that could run it. Detecting a transport that
is not written, for hardware that is not owned, is what `STATUS.md` open item 14 already refuses
on NUMA's behalf.

## Consequences

**Good**

- **The auto-configuration question gets an answer that does not wait on three measurements.**
  Decisions 1, 3, 4 and 5 are sound today; only decision 2's `suggest()` wants numbers.
- **`hft` stops being something anybody can misuse by accident.** `[2026-09-01]` `serve_hft`
  today takes no plan, pins nothing and checks no machine row: it will spin on a laptop, slower
  than `standard` and burning a core, and nothing says so. A ceiling plus decision 2's probe is
  what makes that refusal possible, and it closes the rest of `STATUS.md` open item 21.
- **It tightens the story rather than bending it.** ADR-0012 already decided *latency beats
  density* and *one session per polling thread is the shape this design is optimised for*. Four
  is far closer to that sentence than the thirteen `GUIDE.md` §1a currently implies.
- **Every number keeps naming its mode**, because the mode is chosen once, in the caller's
  source, and never moves.

**Bad, and these are the price**

- **`GUIDE.md` §1a's published arithmetic gets much more demanding.** It currently offers 8
  shards of 13 sessions for a hundred; under this ceiling a hundred `hft` sessions need **25
  isolated cores**. For most gateways the honest answer becomes *use `standard`*, and this
  document should say so rather than let a reader discover it by arithmetic.
- **A hard ceiling in a public API is hard to reverse.** Raising it later is a breaking change
  in behaviour even if the signature holds, and it cannot be raised at all until the busy-path
  turn is measured.
- **Four is derived from one measured number and one borrowed one.** It is the *conservative*
  end of that pairing, which is why it is safe to act on, but it is not a measurement of the
  crossover and this ADR does not claim it is.
- **`suggest()` is a new public API surface that can be wrong quietly.** A caller who passes its
  output through without reading it has re-created the auto-tuner this ADR declines to build —
  the type system cannot prevent that, so `GUIDE.md` must.

## Open questions

1. **What does a busy turn cost per session at N > 1?** Every `engine turn` baseline is
   `idle sessions`; `benches/dispatch.rs` measures the hop at N = 1 and `benches/alloc.rs`
   counts allocations rather than time. **Nothing measures the time of a busy turn at N > 1**,
   and it is what decides whether the ceiling stays at four.
2. **What does a `standard` wakeup cost on the §9 machine?** ADR-0014 open question 1, still
   open. It is the borrowed half of the 4.46.
3. **Is the ceiling per engine or per process?** Stated per engine here, because an engine is a
   shard and the sweep is per shard. A process running 25 shards holds 100 sessions and every
   one of them is inside the ceiling, which is the intended reading and is worth confirming.
4. **Does `standard` want a ceiling too?** It has the same `N × recv` sweep — `pump` is one loop
   for both modes and `Engine::turn` reads every connection unconditionally, so `poll`'s
   readiness answer is discarded. That is a separate finding and wants its own measurement
   before it wants a decision.
