# ADR-0013 — Two modes: `standard` runs anywhere, `hft` buys the microsecond

- **Status**: Proposed — 2026-08-30
- **Date**: 2026-08-30
- **Deciders**: Tran Manh Thang
- **Related**: **re-scopes [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md)
  decisions 1–2**, [ADR-0005](ADR-0005-tls.md), [DESIGN.md §4 D5, D8, §8, §9](../DESIGN.md),
  [plans/2026-08-30-threads-and-affinity.md](../plans/2026-08-30-threads-and-affinity.md),
  **`CLAUDE.md` §2 non-negotiable 4**

## Context

[ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md), accepted earlier the
same day, made latency the tie-breaker: one session per polling thread, `density` a labelled
second shape. It settled a contradiction between two documents. It did **not** ask whether the
engine should be usable by somebody who has no isolated cores, no tuned kernel, and no interest
in burning a core per session.

The owner's decision, 2026-08-30: **it should.** Two modes.

| Mode | For | Buys | Costs |
|---|---|---|---|
| **`standard`** | anybody, any OS, any hardware, a container, a laptop | portability, and **the core back** | the microsecond |
| **`hft`** | a tuned box with isolated cores | the microsecond | a core per polling thread, Linux, and a machine that satisfies §9 |

### Three facts that decide the shape

`[measured / read 2026-08-30]`

1. **The code is already portable.** `grep` for `cfg(target_os` / `cfg(unix` / `cfg(windows`
   across `crates/engine/src/` returns **nothing** — it is pure `std`, and the acceptance gates
   have run on macOS and on Linux. Portability of the *code* is not the problem. The
   *deployment assumption* is.
2. **`wait::Park` is not a `standard` mode.** It is `std::thread::yield_now()`. It yields the
   scheduler; **it does not block**, so it still burns the core it is on. The `Waiting` trait is
   the right seam and `Park` is not the right implementation to hang `standard` on.
3. **`std` has no readiness API.** `Transport` gets `set_nonblocking` and `WouldBlock` from
   `std::net` and nothing else. Blocking until a socket is readable needs `poll`, `epoll`,
   `kqueue` or IOCP — none of which `std` exposes.

### And it collides with a non-negotiable

`CLAUDE.md` §2 rule 4 reads *"The engine thread never sleeps in the kernel on the hot path. No
`epoll_wait`, no futex, no blocking `read`."* It is unconditional, and
`scripts/check-no-kernel-sleep.sh` enforces it by failing on
`epoll_wait|epoll_pwait|poll|ppoll|select|pselect6|futex|nanosleep|clock_nanosleep|sched_yield|io_uring_enter`.

**That list is exactly what `standard` mode must call.** As written, the rule forbids the mode.
This ADR cannot be implemented without amending it, and that is stated here rather than
discovered during the work.

## Decision

**1. Two modes, named in the API and in every published number.**

`standard` is the **default** — what you get if you say nothing. `hft` is opt-in, Linux-only,
and refuses to start when its preconditions are absent (the affinity plan already decides how).

The default flips deliberately: an engine whose out-of-the-box configuration burns a core is
one most people cannot evaluate. **`hft` is the claim; `standard` is the front door.**

**2. `standard` blocks on readiness and gives the core back.**

Not `yield_now`. A real readiness wait with a timeout, so an idle engine costs approximately
nothing and a `Tick` still arrives on time. It is a `Transport`-level concern rather than a
`Waiting` one, because blocking on readiness requires the poller to know the sockets — D5
already makes `Transport` a trait, so the seam exists.

**3. ADR-0012's decisions 1 and 2 are re-scoped to `hft`, not repealed.**

- *"Latency wins over session density"* remains the tie-breaker **inside `hft`**. Inside
  `standard`, the tie-breaker is **portability and the core back**, in that order.
- *"One session per polling thread"* is `hft`'s default shape. `standard` has no such default;
  many sessions on one polled thread is the ordinary way to run it.
- **ADR-0012 decisions 3, 4 and 5 stand unchanged and apply to both.** `density` stays a named
  shape; every figure still names its `N`; §8's budget is still stated end to end. Those are
  about honesty in reporting, not about which mode is being reported on.

**4. Every published figure names its mode as well as its `N` and its machine.**

A `standard`-mode number and an `hft`-mode number are not comparable and must not be quoted as
if they were. This extends ADR-0012 decision 4 rather than replacing it.

**5. `CLAUDE.md` §2 non-negotiable 4 becomes mode-scoped, and gains a second half.**

Proposed wording, for the owner to accept or edit — **this ADR does not change `CLAUDE.md` by
itself**:

> **In `hft` mode, the engine thread never sleeps in the kernel on the hot path.** No
> `epoll_wait`, no futex, no blocking `read`. A blocking call on that thread is a bug, not a
> style choice. **In `standard` mode the engine thread *must* block when idle** — an engine
> that spins by default is unusable on shared hardware. Both halves are machine-checked, and
> the second is not weaker than the first: a `standard` engine that spins is as much a defect
> as an `hft` engine that sleeps. (D8, ADR-0013)

**6. Two gates, because one assertion cannot cover both.** `scripts/check-no-kernel-sleep.sh`
keeps its current meaning for `hft`. A second gate asserts that a `standard` engine, given an
idle socket, **does** enter the kernel and **does not** consume a core — measured as CPU time
over a wall-clock window, not by reading the code.

## Consequences

**Good**

- The engine becomes evaluable by somebody who has not read §9. Today, running it as shipped
  pins a core to 100% and looks broken.
- `hft`'s claims get sharper by having a named opposite. "Under a microsecond" means something
  once there is a mode where it is not being claimed.
- It costs little that is new: `Transport` and `Waiting` are already traits, and the code is
  already `cfg`-free.
- CI stops being a special case. Every test in the repository already uses `Park` because a
  spinning CI machine times out — `standard` makes that the honest default rather than a
  test-only workaround.

**Bad, and stated now**

- **It amends a non-negotiable.** §2's ten rules are referenced across every plan and ADR in
  this repository, and rule 4 has been quoted as unconditional in several of them. Those
  references become imprecise the moment this lands.
- **`standard` needs a readiness API that `std` does not have.** `engine` has **zero external
  dependencies** today. The options — a hand-rolled `poll(2)` through `libc` (POSIX only, small),
  or a cross-platform crate (`polling`, `mio`) — all end that. The choice is not made here;
  it belongs to the plan, with the dependency justified there per `CLAUDE.md` §6.
- **Two modes is two things to test, for ever.** The 59 definitions must pass in both. Every
  hot-path change is now two measurements.
- **The default gets slower.** Somebody who benchmarks fixbolt out of the box will measure
  `standard` and see `epoll`'s 2–5 µs. That is the honest number for that mode and it will be
  quoted against the project.
- **`hft` risks becoming the untested path.** Every gate that is inconvenient to run — a
  spinning loop on a shared runner — will drift toward `standard`. The second gate in decision
  6 exists to make that visible, and it is not enough on its own.

## Open questions

1. **Which readiness mechanism, and which dependency?** `poll(2)` through `libc` is the
   smallest thing that works on Linux and macOS and does not work on Windows. Whether Windows
   is in scope has never been decided.
2. **Does `hft` remain the default for `tools/w2w` and the benchmarks?** They exist to produce
   `hft` numbers, and defaulting them to `standard` would quietly change every published figure.
3. **What happens to `wait::Park`?** It is neither mode — it does not spin honestly and does not
   block honestly — and every test in the repository currently uses it.
4. **Is `density` a third mode or a shape within `standard`?** ADR-0012 named it before this
   ADR existed. Most likely it is simply how `standard` is normally run, and the name is
   redundant — but that is not decided here.
