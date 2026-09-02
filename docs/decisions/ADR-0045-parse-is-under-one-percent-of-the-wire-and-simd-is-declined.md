# ADR-0045 — Parse is under 1% of the wire, and SIMD/SWAR is declined

- **Status**: Accepted — 2026-09-02
- **Date**: 2026-09-02
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0003](ADR-0003-message-representation.md) — the layout decision that made
  parse cheap in the first place ·
  [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md) — why the parse figure
  is a per-machine baseline and not a target ·
  [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md) — the `N × turn`
  term this decision is dwarfed by ·
  [ADR-0021](ADR-0021-nohz-full-leaves-section-9.md) — the §9 line these figures were taken on ·
  [plan](../plans/2026-08-30-w2w-and-linux-numbers.md) step 8

## Context

`STATUS.md` open item 12 — **SIMD / SWAR for the SOH scan and the checksum** — has been open
since the codec was written, and it has always carried its own condition for being done:

> Do it only when `benches/parse.rs` on the Linux box shows parse on the critical path.

**That condition could not be evaluated until 2026-09-02**, because "the critical path" is a
ratio and this project had only the numerator. `benches/parse.rs` has read **122.6 ns** for a
validated `NewOrderSingle` on the §9 desktop since 2026-08-31 — median of 24 qualifying runs,
`benches/baselines.tsv` — and there was nothing to divide it by: `tools/w2w` had never run on a
machine matching `DESIGN.md` §9, which is phase 1 exit criterion 6 and open item 6.

Two facts already argued against the work and neither was decisive on its own:

- **`matthart1983/nanofix` has NEON and SSE2 SOH scanning and still parses 4–6× slower than
  this codec**, because its 512-entry inline field index does not fit in L1
  ([measured-costs.md](../reference/measured-costs.md) §1). Layout won; SIMD did not. That is
  evidence about *that* implementation, not about what SIMD would do to *this* one.
- **`codec` has a zero-dependency rule**, so `memchr` is out. `core::arch` intrinsics and plain
  8-byte SWAR need no dependency at all, so **the rule is not what decides this** — and saying
  so matters, because a decision that looks like dogma gets reopened by the next reader.

## Decisions

### 1. Parse is not on the critical path. Measured, and the arithmetic is the whole decision.

`[measured 2026-09-02]` on the §9 desktop — AMD Ryzen 7 3700X, Linux 7.0.0-30-generic,
`scripts/check-machine.sh` `pass 12  fail 0  unknown 1`, engine pinned to isolated `cpu6` and
the client to `cpu7`, `hft`, medians of 20 qualifying `scripts/w2w-baseline.sh` runs of 20 000
messages each:

| | Parse | Round trip, p50 | Parse's share |
|---|---|---|---|
| `TestRequest` → `Heartbeat` | 57.3 ns (`Heartbeat`) | **16 010 ns** | **0.36%** |
| `NewOrderSingle` → `ExecutionReport` | 122.6 ns | **19 908 ns** | **0.62%** |

Item 12's own estimate of the gain was **20–40 ns per message**. Against the app round trip
that is **0.10–0.20%** — and the run-to-run spread of the wire figure itself is **0.5%**
of its own median across 20 runs. **The improvement would be smaller than the noise of the only
instrument this project has for measuring whether it helped.** That is the decision: not that
20–40 ns is not worth having, but that this design cannot tell whether it got it.

`STATUS.md` item 12 closes on data, which is what its own condition asked for.

### 2. The size of the declined gain is *not* a measurement, and only its irrelevance is

The 20–40 ns figure comes from item 12's own note and was never measured here. **No SWAR scan
was written, so nothing about what one would cost or save on this codec is known.** What is
measured is the denominator, and the denominator is what settles it: at 16–20 µs a round trip,
*any* plausible parse improvement is under half a percent. Stating this precisely matters
because a reader could otherwise carry "SIMD is worth 20–40 ns here" out of this file as though
it were one of this repository's own numbers. It is not, and it is labelled.

### 3. What would reopen it, named rather than left to judgement

**A transport that removes the kernel term.** `[measured 2026-08-31]` removing the syscall —
Onload, `ef_vi`, DPDK — takes ~420 ns of `Engine::turn`'s 449 down to a memory read
([measured-costs.md](../reference/measured-costs.md)). If the round trip fell from ~16 µs to
~2 µs, parse's share goes from under 1% to roughly 6% and this arithmetic no longer holds. That
is `STATUS.md` item 14 and `PRD.md` §5's permanent non-goal; if that non-goal is ever reversed,
**this ADR is to be re-read before the bypass work is planned, not after.**

Nothing else reopens it. A faster CPU moves both terms. A busier engine moves the `N × turn`
term, which makes parse's share *smaller*, not larger.

### 4. If it is ever done: the shape is fixed here so the next reader does not re-derive it

8-byte SWAR in `codec`, no `memchr` (zero-dependency rule), `core::arch` only behind a
measurement, and `unsafe` only with the comment non-negotiable 8 requires. **Started with
`scripts/bench.sh --strict`**, because this is a same-machine A/B and needs the box quiet rather
than needing a particular box.

## Consequences

### Good

- **`codec` keeps no `unsafe`, no `core::arch`, and one code path.** A SIMD scan is three
  implementations — NEON, SSE2, scalar fallback — each needing its own correctness test against
  the same corpus, and the fallback is the one that never gets exercised on the developer's
  machine. That cost is avoided rather than paid and then measured.
- **The `#![no_std]` goal for `codec` (`CLAUDE.md` §6) stays reachable.** `core::arch` would not
  break it, but a target-feature matrix is one more thing that has to hold on a target nobody
  here compiles for.
- **The decision generalises to every other micro-optimisation of the user-space path**, and
  that is the more useful half. `DESIGN.md` §8's user-space rows total ~0.46 µs against a
  measured 16 µs round trip: **2.9%**. Anything inside those rows is bounded by that, and this
  ADR is the place that arithmetic is written down.

### Bad, and accepted

- **A parse-only benchmark shootout would show a SIMD parser ahead**, and that is how codec
  comparisons are usually published. This project's positioning — *the fastest acceptor that can
  run on kernel TCP* — is a claim about the round trip, and it declines to compete on a number
  it has measured to be irrelevant to that claim. Somebody will quote the parse figure anyway.
- **The measurement is loopback, not NIC to NIC.** `DESIGN.md` §6's wire-to-wire row asks for a
  load generator on a separate machine with `SO_TIMESTAMPING`, and that row is **not** closed by
  this. A real NIC path adds kernel and driver time, so it moves the denominator *up* and this
  decision further in — but that direction is reasoned, not measured, and is labelled as such.
- **`STATUS.md` item 12 is closed with no code written**, so there is no artifact and no test.
  What exists instead is this file and the two figures in it, which is the honest shape for a
  decision not to build something.
