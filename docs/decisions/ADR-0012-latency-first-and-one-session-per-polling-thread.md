# ADR-0012 — Latency is the priority, and a polling thread carries one session

- **Status**: Proposed — 2026-08-30
- **Date**: 2026-08-30
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0002](ADR-0002-engine-library-split.md),
  [DESIGN.md §1, §4 D8, §8](../DESIGN.md), [PRD.md §1](../PRD.md),
  [reference/measured-costs.md](../reference/measured-costs.md),
  `STATUS.md` open items 12, 14, 22

## Context

Two sentences in this repository's own documents cannot both be the headline.

`DESIGN.md` §1 positions the engine as **the fastest acceptor that can run on kernel TCP**.
`PRD.md` §1 describes its first user as a gateway wanting **"an acceptor that holds many
sessions on one core and does not stall"**.

Until 2026-08-30 nothing forced a choice, because the cost of the second was never measured.

### The measurement that forces it

`[measured 2026-08-30]` on the §9 machine, pinned to an isolated core. D8 makes an idle turn
**one non-blocking `read` per connection**, so this is the cost of the sweep the engine
actually performs:

```
N=1      703.2 ns/read      703.2 ns/turn
N=2      705.1 ns/read     1410.1 ns/turn
N=16     702.3 ns/read    11237.5 ns/turn
N=256    707.0 ns/read   180988.1 ns/turn
```

**Flat from N=1 to N=256.** The sweep is exactly `N × 703 ns`, and a message arriving just
after its socket was polled waits one whole sweep to be seen — so that is *added latency per
session*, not throughput.

Where it goes:

```
clock_gettime (vDSO, no kernel entry)               22.9 ns
syscall(getpid) — enters and leaves, does nothing  353.8 ns
read(socket) -> EAGAIN                             703.0 ns
```

Against this project's own `parse NewOrderSingle (validated)` at **125.5 ns**: **the syscall
that discovers there is nothing to parse costs 5.6× the parse.**

### Two things in the design are now known to be wrong

1. **§8's table prices busy-poll at `~0`.** It is 703 ns per socket per turn. The row was
   comparing busy-poll's *wakeup* against `epoll`'s and calling the remainder free; the
   remainder is a syscall.
2. **§8's bottom line is "everything this design controls: < 1 µs", and it is measured against
   the user-space path only.** User space is not where the money goes — the vDSO line shows
   user-space work at tens of nanoseconds. **Two sessions on one polling thread exceed the
   entire budget in polling alone**, before any FIX work starts.

`DESIGN.md` §1 already contains the lesson in an earlier form: *"the codec is ~1% of the
wire-to-wire budget … a design that optimises the codec and says nothing about I/O strategy has
optimised the wrong 1%."* The I/O strategy was then chosen, and never priced.

## Decision

**1. This is a low-latency engine. When latency and session density conflict, latency wins.**

Not a slogan — a tie-breaker with teeth. Any change that improves sessions-per-core at the cost
of per-session latency needs its own ADR superseding this one. The reverse does not.

**2. The default deployment shape is one session per polling thread.**

`Engine::turn` keeps its round-robin over every connection — D8's starvation argument is
unchanged and correct — but *the shape the design is optimised for, budgeted for and measured
at* is a single session on an isolated core.

**3. Many sessions per thread remains supported, and is named `density` rather than left
implicit.** It is a real product for a broker gateway, and 90 µs at 128 sessions is
unremarkable there. What changes is that it is a **labelled mode with its own budget**, not the
headline shape, and its budget is stated as `N × 703 ns + the per-message path` rather than
inheriting the latency figures.

**4. Every latency figure this project publishes names its session count**, exactly as
non-negotiable 10 already requires it to name its machine and its §9 settings. A number without
`N` is not a number, for the same reason a number without a machine is not one.

**5. `DESIGN.md` §8's budget is restated end to end, including syscalls.** "Everything this
design controls" stops meaning "the user-space path". The syscall to reach the socket is
something this design chose and can change — by batching it, by removing it, or by carrying
fewer sockets — and a budget that excludes it measures the half that was already cheap.

## Consequences

**Good**

- The two documents stop contradicting each other, and the contradiction is resolved by a
  measurement rather than by preference.
- The open items reorder themselves by size instead of by taste. Item 12 defers SIMD for
  20–40 ns; a syscall is 703 ns. That is not a judgement call any more.
- `density` becomes honest: a gateway operator gets a budget they can plan against instead of
  a latency claim that does not apply to them.
- It gives `tools/w2w` its terms of reference — measure a whole turn including its syscalls,
  at a stated `N`.

**Bad, and stated now rather than discovered later**

- **Every latency figure published before today lacks its `N`.** They were all taken at N=1 in
  benches that hold no socket at all, which is the best case and was never labelled as one.
- **It makes the engine look worse on a metric some buyers care about.** "Many sessions per
  core" is a number that sells; this ADR declines to lead with it.
- **`density` is now a mode with a budget nobody has measured.** The 703 ns figure is a floor
  from a C program, not a reading of `Engine::turn`, and no `N > 1` end-to-end number exists.
- **It does not fix anything.** Naming the cost is not removing it. `recvmmsg`, `io_uring`
  with `SQPOLL`, `mitigations=off` and kernel bypass are all unmeasured here, and this ADR
  chooses none of them.
- **`PRD.md`'s first persona changes**, which is a product change and not a documentation
  tidy-up. It is recorded here because §4 requires the ADR that moved it.

## Open questions

1. **What is `Engine::turn`'s real cost at N=1 and N>1**, end to end, rather than the syscall
   floor a C program measures? `tools/w2w` is the instrument and it has not been pointed here.
2. **How much of the 353.8 ns is mitigations?** Full mitigations are in force, including
   `vmscape`'s IBPB on every syscall return. Unmeasured, needs a reboot, and is a security
   decision rather than a performance one.
3. **Does `io_uring` with `SQPOLL` belong in `Transport`?** It removes the per-socket entry
   entirely and would make `density` cheap — at the cost of a second transport implementation
   and a kernel-version floor. D5 already makes transport a trait, so the shape exists.
4. **Where does `density` stop being a mode and become a second product?** A thread pool
   sharding sessions across cores is a different engine from the one D8 describes.
