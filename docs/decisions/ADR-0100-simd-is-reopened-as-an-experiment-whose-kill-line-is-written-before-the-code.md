# ADR-0100 — SIMD is reopened as an experiment whose kill line is written before the code

- **Status**: **Proposed — 2026-09-23**, with [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  (phase 4). Accepted or rejected together with it. On acceptance it **supersedes
  [ADR-0045](ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md) decision
  1** (*declined*). ADR-0045 decisions 2, 3 and 4 stand and are used below; its text is not
  edited beyond a status line naming this ADR.
- **Date**: 2026-09-23
- **Deciders**: Tran Manh Thang (chose SIMD for phase 4, 2026-09-23). Written by the architect
  (Opus).
- **Related**: `DESIGN.md` §5 *SIMD delimiter scan and checksum*; `PRD.md` §2 *Phase 3*
  (excluded there); [ADR-0003](ADR-0003-message-representation.md);
  [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md); `CLAUDE.md` §2
  non-negotiables 1, 7, 8, 10; §6 (*`codec` has zero runtime dependencies*)

## Context

ADR-0045 declined SIMD on arithmetic: `[measured 2026-09-02]` parse is 57.3 ns of a 16 010 ns
admin round trip (0.36 %) and 122.6 ns of a 19 908 ns application round trip (0.62 %); item 12's
unmeasured 20–40 ns gain would be 0.10–0.20 %, below the 0.5 % run-to-run spread of the only
instrument that could see it. Its decision 3 named the one thing that reopens it — a transport
that removes the kernel term — and required that ADR be re-read *before* bypass is planned.
ADR-0098 plans `io_uring` and a bypass arm in the same phase, and the owner has chosen SIMD.

What the search found (2026-09-23): an AVX2 FIX parser gained **5 %** over a hand-unrolled loop
on a 166-byte message, and the checksum **~2×** over compiler-vectorised code
(<https://www.klittlepage.com/articles/accelerated-fix-processing-via-avx2-vector-instructions/>);
simdjson-style structural indexing is claimed at ~250 ns per ExecutionReport by NexusFIX
(<https://github.com/StratCraftsAI/NexusFix>), slower than this codec's scalar parse.
`matthart1983/nanofix` has a SIMD SOH scan and parses 4–6× slower than this codec
(`DESIGN.md` §5). Nothing found measures SIMD against a parse that already builds its field index
in one pass.

## Decision

1. **SIMD is reopened as an experiment, not declined and not accepted.** It is built, measured
   and then kept or removed by the line in decision 3, written here before any code.
2. **Shape (ADR-0045 decision 4, unchanged)**: 8-byte SWAR first, in safe Rust, in `codec`, no
   `memchr`, no dependency, `no_std`-clean; for the SOH scan and the checksum. `core::arch`
   (AVX2) is a second arm only if SWAR misses the line **and** the remaining gap is in the
   checksum; that arm needs a plan of its own under non-negotiable 8 — a comment naming its
   proof, a differential fuzz target against the scalar path, compile-time `target_feature`
   gating (no `std` runtime detection inside `codec`).
3. **The kill line.** Kept only if **all** of:
   - `scripts/bench.sh --strict`, same boot, A/B: `parse NewOrderSingle (validated)`,
     `parse Heartbeat (validated)` and a checksum-only case each improve by **≥ 15 %** (the
     checksum case does not exist today; it is added and baselined before any SWAR code);
   - **and either** parse's share of the fastest round trip that survived ADR-0098 items 1–2 is
     **≥ 2 %** (ADR-0045's arithmetic with the new denominator), **or**
     `crates/engine/benches/density.rs` at N = 64 improves by **≥ 3 %**;
   - allocation benches still read 0; the 59 / 59 and FIXT gates are unchanged; the
     differential test agrees with the scalar path on every corpus message and on fuzz input.
   Otherwise the code is removed on the same branch and the numbers go to
   `docs/reference/measured-costs.md`.
4. **Measured last.** The share line depends on the denominator ADR-0098 items 1 and 2 leave
   behind, so SIMD is the last hot-path item of phase 4.

## Consequences

**Good**

- The owner's item is built and judged by a line fixed in advance, instead of by whether the
  code, once written, feels worth keeping.
- ADR-0045's analysis is kept, not overturned: if the transport stays at ~28 µs, the share line
  cannot be met, and the experiment ends where ADR-0045 said it would — now with a measured gain
  instead of item 12's guess.

**Bad — and accepted**

- **The likely result is removal.** The only public parse measurement found is 5 %, under the
  15 % line; the checksum is the plausible survivor. Phase 4 may spend a step to confirm
  ADR-0045.
- **Removal is work too**: a branch that adds and then deletes a SWAR path, with the numbers kept.
- **A 15 % codec gain can be real and still be killed** by the share line — a user who cares
  about CPU per message rather than latency would have kept it. The density clause is the only
  concession to that user.
- **An AVX2 arm, if it ever runs, is the first `unsafe` in `codec`.**

## Sources

ADR-0045; the two URLs above; `DESIGN.md` §5; ADR-0098 *Research*.
