# ADR-0074 — Kernel bypass, `io_uring` and the Logon hop stay unmeasured, by decision

- **Status**: Proposed — 2026-09-18
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: `PRD.md` §5 (permanent non-goals), §2 *Phase 3*;
  [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md) open question 3
  (`io_uring`); [ADR-0023](ADR-0023-section-9-records-the-cpu-mitigations.md);
  [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md);
  `DESIGN.md` §8; `STATUS.md` open items 14, 22 (residue), 24 (residue)
- **Closes**: the three sentences that kept three closed items in the open table — item 14's
  *"Onload first, `ef_vi` second, DPDK never"* note; item 22's *"still open here is
  `recvmmsg`/`io_uring` with `SQPOLL`"*; item 24's *"the wall-clock latency a `Logon` gains from
  the channel hop … is not measured"*

## Context

Three items are closed on their own terms and still sit in the open table because each ends
with a sentence about something it did not measure. None of the three is phase-1 or phase-2
work; each is a candidate that only a NIC figure can rank. This ADR writes the ranking down
so the sentences can be struck.

**Bypass.** PRD §5 excludes kernel bypass; the engine's positioning is *on kernel TCP*. Item 14
recorded the order if that is ever reversed. The search of 2026-09-18 changes one fact in it:
OpenOnload no longer needs a Solarflare NIC to run at all — *"OpenOnload can accelerate
applications on non-Solarflare network adapters with support for AF_XDP"*, registered with
`echo <ifname> > /sys/module/sfc_resource/afxdp/register`, zero-copy only where the driver
implements the AF_XDP primitives, and *"the AF_XDP support is a community-supported work in
progress that is not currently at release quality"* (Xilinx-CNS/onload README). The I211's
`igb` driver has native XDP but not zero-copy AF_XDP; so an Onload-on-I211 run would be copy
mode — a smoke test of the socket API under Onload, not a latency figure.

**`io_uring` / `recvmmsg`.** Item 22 measured the mitigation surcharge on every syscall and
left the syscall itself as the next lever. `io_uring` with `SQPOLL` replaces the engine's
`recv`/`send` with a kernel thread polling a shared ring — a second spinning thread, which is
the density trade ADR-0012 already made once. `recvmmsg` batches receives, which helps a busy
socket and does nothing for one message in flight. Neither can be ranked against the
engine's real floor until that floor is a NIC figure (item 40, ADR-0071) rather than a
loopback one.

**The Logon hop.** ADR-0020's pre-session stage hands a socket to an engine over a channel
after the `Logon`; the wall-clock cost of that hop is not on the message path and is not in
§8. Item 24 noted it as unmeasured.

## Decision

1. **Bypass stays excluded (PRD §5 unchanged) and its order stands: Onload first, `ef_vi`
   second, DPDK never**, for the reasons item 14 gives. Added: **the first Onload run does not
   need Solarflare hardware** — `onload ./w2w` on the I211 in AF_XDP copy mode is a valid
   smoke test that the engine's socket usage survives Onload (`accept4`, non-blocking `recv`,
   `SO_TIMESTAMPING` on a `dup`), and it is the *only* Onload figure this project may take
   without a Solarflare/AMD X2-class NIC. It is not a latency figure and is not published as
   one. It is phase 3.
2. **`io_uring` (`SQPOLL` or not) and `recvmmsg` are not tried before the NIC figure exists
   at interval 0** (ADR-0071 decision 2). When it exists, the first `io_uring` experiment is a
   `w2w` A/B under ADR-0068 decision 4 — one procedure, a difference, never a figure — and it
   gets its own plan. ADR-0012 open question 3 is answered *deferred, with the trigger named*.
3. **The Logon hop is not measured, and `DESIGN.md` §8 says why in one sentence**: the
   pre-session stage is off the message path; a session's first message pays the hop once,
   and that cost is bounded by the `presession` bench (426.2 ns per socket sweep, ~84 ns to
   route) plus one cross-thread wake, which §9 already forbids from being on the engine core.
   It becomes a measurement only if a user reports Logon latency as a problem.
4. **Items 14, 22 and 24 leave the open table.** Their rows are struck through in full; the
   text stays.

## Consequences

**Good**

- Three rows stop pretending to be work. The trigger for each (a NIC figure; a user report)
  is named, so nobody re-opens them by re-reading the same sentence.
- The Onload fact found today saves a future reader the assumption that phase 3 begins with a
  hardware purchase.

**Bad — and accepted**

- **`io_uring` may be a real lever and this defers finding out.** The mitigation measurement
  (item 22) says every syscall the engine performs costs ~60% more than it needs to; a ring
  that halves the syscall count would be worth measuring today. The reason to wait is that a
  loopback figure cannot rank it against driver and IRQ cost, and a wrong ranking is the more
  expensive mistake.
- **The Logon hop is a latency a counterparty can see** — the first message after Logon — and
  this ADR says it is not the engine's headline. A venue with strict logon SLAs would disagree.
- **An Onload copy-mode smoke test is easy to misread as a number**; decision 1 forbids it and
  a forbidding sentence is weaker than a gate.

## Sources

- Xilinx-CNS/onload README, *AF_XDP* section —
  <https://github.com/Xilinx-CNS/onload/blob/master/README.md> (read 2026-09-18).
- `STATUS.md` items 14, 22, 24; `DESIGN.md` §8; ADR-0012 open question 3; ADR-0020.
- `crates/engine/benches/presession.rs` `[measured 2026-09-01]` 426.2 ns per socket, 84.0 ns
  to read both comp IDs and pick a shard.
