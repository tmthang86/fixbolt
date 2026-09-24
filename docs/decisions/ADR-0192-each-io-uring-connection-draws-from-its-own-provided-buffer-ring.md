# ADR-0192 — Each `io_uring` connection draws from its own provided-buffer ring

- **Status**: Accepted — 2026-09-24, built in PR #110 with its red-first test
  (`a_connection_nobody_reads_cannot_starve_another`) and its reversal (every slot on group 0).
  Proposed 2026-09-24. Written by the architect (Opus) for phase 4 row 5, from design
  finding L3 of the senior review of PR #110. **Supersedes the shared-pool bullet of
  [ADR-0190](ADR-0190-the-io-uring-transport-is-reaped-by-the-idle-strategy-and-an-hft-turn-enters-the-kernel-once-without-waiting.md)
  decision 2** (*"one ring-mapped provided-buffer group per `Uring`"*); every other part of
  ADR-0190 stands. Accepted by the owner or by the manager under the owner's delegation.
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the owner or the manager.
- **Related**: ADR-0190 decisions 2 (receive shape), 5 / R3 (parked connections are not listed),
  7 / R4 (ring sizing checked at startup), 10 (row 7); `DESIGN.md` D5, D10 (backpressure);
  [plan](../plans/2026-09-24-p4-io-uring-transport.md) *Sửa 2*.

## Context

ADR-0190 decision 2 gave each `Uring` **one** provided-buffer group shared by every connection's
multishot `recv`, and justified running dry as *"TCP backpressure, never loss"*. That is true of
bytes, and false of **fairness**: the kernel hands the next free buffer to whichever armed `recv`
has data, so one connection whose bytes are not read can hold every buffer in the pool, and every
other connection's multishot then ends with `-ENOBUFS` and cannot be re-armed until that one
connection is read (`rearm`, `crates/engine/src/transport/uring.rs:1269-1290`, re-arms only while
`kernel_owned > 0`).

`[measured 2026-09-24]` the reviewer's probe, 4 buffers: connection A sends 64 KiB and is not
read, B sends 10 bytes — *"B got 0 of 10 bytes in 1 s (ended Idle); report … bytes: 16384,
enobufs: 2, rearms: 0"*.

**Why it is not reachable today, and why that is not enough.** The shipped entry points
(`serve_uring`, `serve_hft_uring`) run with `NoRecovery` and `Engine::turn` reads every connection
once per turn, so no connection's staged bytes go unread for longer than a turn. It becomes
reachable the moment `io_uring` meets anything that stops reading one connection while serving
the others: a connection parked by `Recovery::ready` (ADR-0154 — neither read nor listed, ADR-0190
R3), and any future read policy that skips a connection. Under a flood it is already a
**latency** coupling today: a busy connection keeps the shared pool near empty, and a quiet
connection's first message waits for a re-arm the busy one races it for. With the kernel
transport, one connection's unread bytes wait in **its own** socket buffer and cost the others
nothing; this transport must not be weaker than that.

## Options considered

- **A. A per-connection cap on buffers held, in the shared pool.** When a connection holds `cap`
  staged buffers, cancel its multishot; re-arm it when it drains below. Rejected: the kernel keeps
  handing that connection buffers between the moment the cap is reached and the moment the cancel
  runs, so the cap is soft by however many completions are in flight; it adds a cancel and a
  re-arm SQE per crossing, on the hot path, and a threshold nobody can measure the right value of.
- **B. Refuse `io_uring` + `Recovery` until a fix exists; record the limit.** Rejected as the
  answer, kept as a fallback: it leaves the flood-time latency coupling in the shipped arm and
  makes the next feature (`Recovery` over the ring) pay for a debt this one created.
- **C. One provided-buffer ring per connection slot, all registered at construction.** Chosen.
  Isolation by construction, the exact semantics of the kernel arm: a connection that is not read
  runs out of **its own** buffers, its multishot ends with `-ENOBUFS`, and its remaining bytes wait
  in its own socket until the engine reads it. No other connection can see it.

## Decision

1. **Each connection slot owns a provided-buffer ring** with its own buffer-group id (the slot
   index; `bgid` is 16 bits and `connections` is already a `u16`). Every ring is registered
   (`IORING_REGISTER_PBUF_RING`) **when `Uring` is built**, not at accept: no `io_uring_register`
   happens inside the serving window. A slot's multishot `recv` selects only from its own group.
2. **`UringConfig` is `connections × buffers_per_connection × buffer_len`.** The field that was
   `buffers` (pool size) becomes `buffers_per_connection` (a power of two, at least 2, at most
   the kernel's per-ring maximum); `buffer_len` is unchanged. **No hidden default** (`CLAUDE.md`
   §6): the value `tools/w2w` and the entry points' rustdoc examples use is **8 × 4 096 bytes per
   connection** — twice the engine's default `RX` (4 096) in flight, so one reply's worth of
   pipelining never runs a connection dry — stated in `docs/CONFIGURATION.md`. The whole region is
   still one page-aligned allocation, pre-faulted at construction; its size is printed by
   `UringReport` (`buffer_bytes`) so a large `connections` is visible, not discovered.
3. **Per-connection `-ENOBUFS` is backpressure on that connection only**: its multishot is
   re-armed when **it** has a buffer back (the engine read it), not when the pool does. `rearm`
   iterates the slots whose own ring has a free buffer.
4. **Accounting is per slot.** The ownership ledger (ADR-0190's U5/U6 proof) holds per slot: each
   buffer id of slot *s* is in exactly one place — *s*'s kernel ring, or *s*'s staged list. The
   bounds check on a completion becomes `bid < buffers_per_connection` for the slot named by its
   `user_data`, with the generation check first.
5. **`UringReport` gains `enobufs_slots`** (how many distinct slots ran dry) beside the existing
   `enobufs` count, so a probe can tell *one* connection starved of its own buffers from many.

## Consequences

**Good**

- A connection nobody reads cannot delay or starve another — the property the kernel arm has, now
  held by construction rather than by today's read policy. `io_uring` + `Recovery` stops being a
  latent defect.
- No new SQE or cancel on the hot path; re-arm logic gets simpler (per slot, no pool-wide race).
- No `io_uring_register` in the serving window.

**Bad**

- **Memory scales with `connections`**, not with a pool size: at 8 × 4 KiB it is 32 KiB per
  connection slot — 8 MiB pre-faulted per engine thread at 256 slots, plus the pending ceiling —
  against one pool before. Stated in `CONFIGURATION.md` and printed by `UringReport`.
- **`connections` buffer rings registered at startup** — `connections` extra `io_uring_register`
  calls before serving, and `connections` ring tails in memory; the idle turn does not touch them,
  a busy turn touches only the slots that received.
- **A connection receiving faster than one turn's read** now runs dry of its own 8 buffers where
  the shared pool might have absorbed a burst; its bytes wait in its socket (as with the kernel
  arm) and it is re-armed after the next read. The `density` busy-loop case is where that shows.
- `UringConfig`'s public shape changes (`buffers` → `buffers_per_connection`) — a breaking change
  to an API introduced in this same pull request and never tagged, so `cargo semver-checks`
  against `v0.1.0` does not see it.

## What the senior developer builds (plan *Sửa 2*)

1. Red first, in `crates/engine/tests/uring.rs`:
   `a_connection_nobody_reads_cannot_starve_another` — the reviewer's probe as a test
   (`buffers_per_connection = 4`; A sends 64 KiB and is never read; B sends 10 bytes; B must
   receive all 10 within 1 s; A's slot is the only one in `enobufs_slots`). Expected FAIL on the
   shared pool: *"B got 0 of 10 bytes in 1 s"*.
2. The per-slot rings (decisions 1–5) in `crates/engine/src/transport/uring.rs`; `UringConfig`
   renamed field and validation; `UringReport::{buffer_bytes, enobufs_slots}`.
3. Every existing test green unmodified except where it constructs a `UringConfig` (the field
   name) — `a_ring_that_runs_out_of_buffers_rearms_and_loses_nothing` must still show
   `enobufs > 0` and `rearms > 0` with its buffers per connection; the ledger check runs per slot.
4. Reversal: point every slot's `RecvMulti` at group 0 → the new test goes red with the sentence
   above.
5. `tools/w2w` passes 8 × 4 096; `buffers_are_resident_before_the_first_message` measures the new
   total.
