# ADR-0200 — A bypass arm is judged from the counterparty against a same-boot kernel twin, on one instrument, and its kill line is arithmetic written before the boot

- **Status**: Proposed — 2026-09-24, with
  [plans/2026-09-24-p4-bypass-and-s9-boot](../plans/2026-09-24-p4-bypass-and-s9-boot.md).
  **Revised in place 2026-09-24** (Proposed, so revised rather than superseded — `CLAUDE.md` §5): the
  owner answered that plan's Q1 and Q2 in conversation the same day. Q1 = **measure** (option A), so
  decision 4's prediction is tested by the boot, not taken as the result. Q2 = the three readings the
  plan recommended, now written into decisions 3 and 5 as the owner's: (a) p99 no worse = within
  ADR-0068's 5 %; (b) both paths in both procedures; (c) the corpus over Onload-accelerated loopback.
  Before this revision decisions 3 and 5 stated the same readings as the architect's proposal,
  awaiting Q2.
- **Date**: 2026-09-24
- **Deciders**: Tran Manh Thang. Written by the architect (Opus) for rows 6 and 7 of
  [plans/2026-09-23-phase-4-scope](../plans/2026-09-23-phase-4-scope.md).
- **Related**: [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  item 2 (the kill line this ADR makes computable — its substance is not edited),
  [ADR-0099](ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)
  decisions 2 and 4, [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)
  decisions 2 and 4, [ADR-0071](ADR-0071-a-skipped-tx-stamp-is-a-missing-sample-not-a-failed-run.md),
  [ADR-0201](ADR-0201-onload-on-the-i211-runs-without-hardware-flow-filters-one-channel-count-holds-for-the-boot-and-the-control-path-leaves-the-cable.md),
  [ADR-0202](ADR-0202-phase-4s-one-s9-boot-is-pre-built-driven-by-a-committed-script-and-onload-lives-only-inside-its-block.md);
  `docs/reference/measured-costs.md` *C-40*.

## Context

ADR-0098 item 2 keeps Onload over AF_XDP only if, on one §9 boot, `hft`, two procedures, the
**generator-side** p50 improves by ≥ 10 % against the kernel arm, p99 is no worse, a zero-copy bind
is confirmed, and the 59-definition socket corpus is 59 / 59 under `onload`. It chose the generator
side because the acceptor's hardware stamps may not survive a userspace stack. Four things were not
yet written down: which instrument each arm uses, how a run proves it really went through Onload,
the arithmetic of each clause, and what the numbers already on record say about the line.

**Found by reading, 2026-09-24:**

1. **The acceptor's stamps do not survive, by construction.** `tools/w2w --wire-timestamps` stamps RX
   through an `AF_PACKET` tap and TX through the engine socket's error queue. An XDP program runs in
   the driver before any `skb` exists, and a frame it redirects to an XSK never reaches the tap; a
   frame Onload sends goes out through the XSK TX ring, not through the kernel socket that carries
   `SO_TIMESTAMPING`. The kernel mechanisms that would carry stamps through AF_XDP — the XDP RX
   metadata kfunc and XSK TX metadata — are implemented by `igc` and `stmmac`, not `igb`
   (<https://docs.kernel.org/networking/xsk-tx-metadata.html>,
   <https://docs.kernel.org/networking/xdp-rx-metadata.html>); on the desk's
   `7.0.0-31-generic`, `/proc/kallsyms` has no `igb` function for either `[read 2026-09-24]`.
   The only Onload hardware-timestamp use found is `EF_RX_TIMESTAMPING` on a Solarflare NIC
   (<https://github.com/Xilinx-CNS/onload/issues/51>); nothing found says it works over AF_XDP.
2. **Onload can fall back to the kernel without saying so.** Onload issue #337 (and #62, #83, #10,
   same signature) reports stacks failing with `rc=-95` and sockets "silently" degrading to the
   kernel path (<https://github.com/Xilinx-CNS/onload/issues/337>). `EF_NO_FAIL` (default `1`) is
   the option that hides a failed accelerated socket behind a kernel one; `0` makes the failure an
   error, and Onload's own text says to use it "to find out when they are not" accelerated
   (`src/include/ci/internal/opts_citp_def.h`, read 2026-09-24).
3. **`EF_AF_XDP_ZEROCOPY` defaults to `1`** ("Support for zerocopy is required") and Onload binds
   with `XDP_ZEROCOPY` when it is set (`src/include/ci/internal/opts_netif_def.h`,
   `src/lib/efhw/af_xdp.c`); a bind with `XDP_ZEROCOPY` "will force the socket into zero-copy mode or
   fail" (<https://docs.kernel.org/networking/af_xdp.html>). The kernel reports the mode of a bound
   XSK through `xsk_diag`: `ss --xdp -a -e` prints `zc:1` from `XDP_DU_F_ZEROCOPY`
   (iproute2 `misc/ss.c`, `xdp_show_umem`; the desk has iproute2 6.19.0 and
   `CONFIG_XDP_SOCKETS_DIAG=m`).
4. **Loopback is not accelerated by default.** `EF_TCP_SERVER_LOOPBACK` and
   `EF_TCP_CLIENT_LOOPBACK` both default to `0`, "not accelerated" (`opts_netif_def.h`).
   `crates/engine/tests/wire.rs` binds `127.0.0.1:0` and drives both ends in one process, so a plain
   `onload cargo test --test wire` runs the 59 definitions on the kernel's loopback and proves
   nothing about Onload.
5. **The generator-side figure is mostly the Mac.** `measured-costs.md` *C-40* (boot C, 2026-09-18,
   `hft`, interval 0, over the cable): acceptor wire p50 **27 050 ‖ 27 114 ns** (admin) and
   **28 894 ‖ 28 878 ns** (app); the counterparty (Mac mini, unpinned, macOS stack both ways) read
   **p50 ~232 µs** in both arms. The acceptor's whole NIC-in → NIC-out window is about 12 % of what
   the Mac measures.

## Decision

1. **One instrument for both arms: the counterparty's table.** Both the kernel twin and the Onload
   arm are `scripts/w2w-baseline.sh` split runs (`LISTEN` on the desk, `--connect` on the Mac), and
   **neither** runs `--wire-timestamps`: the observer thread and its tap are a variable
   (ADR-0068 decision 4 lists `--wire-timestamps` on ↔ off as an A/B that moves figures), and the
   Onload arm cannot have them (fact 1). The two arms share the desk's `w2w` binary by sha256, the
   Mac's `w2w` binary by sha256, the NIC's channel count (ADR-0201 decision 2), the ssh control
   path (ADR-0201 decision 3), `hft`, interval 0, `MESSAGES`, `WARMUP`, `RUNS = 20`, and both
   paths (`admin`, `app`). The twin runs with the NIC unregistered and **no XDP program attached,
   read back** (`scripts/check-machine.sh`, `FIXBOLT_BYPASS=twin`).
2. **Every Onload run proves it went through Onload, or it FAILs the arm.** Not disqualified —
   failed, because a run that did not go where it says is the instrument lying, not noise
   (the `hw-rx-missing` precedent in `w2w-baseline.sh`):
   - the listen half runs with `EF_NO_FAIL=0`, `EF_AF_XDP_ZEROCOPY=1`, `EF_USE_HUGE_PAGES=0` and
     the `latency` profile, and the header prints every `EF_*` in force and the stack's own
     `onload_stackdump` read of its options;
   - once the listen half prints `listening:`, `ss --xdp -a -e` must show an XSK on the NIC's
     `ifindex` with `zc:1`;
   - the kernel's TCP segment counters (`/proc/net/snmp` `Tcp: InSegs`, `OutSegs`), read before
     the generator starts and after the listen half exits, must move by **less than 1 % of the
     run's requests** — a run through the kernel moves them by at least one segment per request;
   - `w2w`'s own `allocs 0` assertion stays on both halves: an allocation by Onload's preloaded
     library on the engine thread is an allocation on the hot path (non-negotiable 1), and it kills
     the arm rather than being subtracted.
3. **The kill line, as arithmetic.** Per path (`admin`, `app`) and per procedure, with `K` the
   twin's and `O` the Onload arm's median-of-runs counterparty figures:
   - p50 clause: `(K_p50 − O_p50) / K_p50 ≥ 0.10`;
   - p99 clause: `O_p99 ≤ K_p99 × 1.05` — "no worse" read as "not worse by more than the
     reproduction band" (ADR-0068 decision 2's 5 %), the same band ADR-0098 item 1 names for
     `io_uring`'s p99 — **the owner's reading, Q2 (a), 2026-09-24**;
   - zero-copy clause: every Onload run passed decision 2's `zc:1` read;
   - corpus clause: decision 5.
   **Kept** only if every clause holds for **both paths in both procedures** (**the owner's reading,
   Q2 (b), 2026-09-24**). **Dropped** as soon as
   one procedure fails a clause: a failing A/B is one procedure, labelled A/B, and published as a
   difference (ADR-0068 decision 4) — the second procedure is not run for a result it cannot
   change. The verdict is printed by `scripts/bypass-verdict.sh` from the four
   `w2w-baseline.sh` summaries, a pure function with its own fixture test
   (`scripts/check-bypass-verdict.sh`), so the kill line is a command, not a reading.
4. **The prediction is written before the boot: DROP, on the p50 clause.** From fact 5: a 10 % gain
   on ~232 µs is **~23.2 µs**, which is **86 %** of the admin window (27 050 ns) and **80 %** of the
   app window (28 894 ns) — everything between the acceptor's two NIC stamps. Onload over AF_XDP
   leaves in that window: the arrival of the whole request frame on 1 GbE (the RX stamp is taken at
   the start of the frame); the NIC's DMA and interrupt, since AF_XDP zero-copy still runs the
   driver's NAPI; a `sendto()` kick per transmit when the TX ring needs wakeup
   (<https://docs.kernel.org/networking/af_xdp.html>, `XDP_USE_NEED_WAKEUP`); and the engine's own
   work (app − admin alone is 1.8 µs). The best AF_XDP round trips found anywhere are 6.5–9.7 µs on
   100 GbE / 40 GbE cards **with** busy polling, and zero-copy without polling "behaved badly" on
   the Intel driver (<https://arxiv.org/html/2402.10513v1>); one public Onload-on-AF_XDP A/B was
   **16 % slower** than the kernel (<https://github.com/Xilinx-CNS/onload/issues/139>). This is a
   prediction, not a result: the boot measures it, and the record says whether the measurement
   confirmed it. The owner chose to measure (plan Q1 = A, 2026-09-24) rather than record the drop
   on this arithmetic without a boot.
5. **"59 / 59 under `onload`" means the corpus runs through Onload's own TCP** (**the owner's
   reading, Q2 (c), 2026-09-24**). The wire test binary,
   pre-built, runs under `onload` with `EF_TCP_SERVER_LOOPBACK=1`, `EF_TCP_CLIENT_LOOPBACK=1` (both
   ends are in one process, so one stack) and `EF_NO_FAIL=0`, by
   `scripts/check-wire-under-onload.sh`, which also reads the kernel's `Tcp: PassiveOpens` before
   and after and FAILs if it rose by as many connections as the corpus opens. This proves Onload's
   TCP and socket semantics against the 59 definitions; the AF_XDP datapath is proven by decision
   2's runs, which carry a real Logon, `35=1`/`35=0` and `35=D`/`35=8` over the cable with every
   reply's `35=` checked by the generator. A split conformance harness across the cable is not
   built (plan *Ngoài phạm vi*).
6. **`standard` under Onload is not measured and is documented as unsupported** (ADR-0099
   decision 4 allows exactly this). Onload's spinning (`EF_POLL_USEC`) inside a blocking call is the
   `standard` defect non-negotiable 4 names, and turning it off to pass
   `check-standard-gives-the-core-back.sh` would measure a configuration nobody runs for latency.

## Consequences

**Good**

- The kill line is a command with a fixture, written before the boot; a reader can re-run it on the
  published summaries.
- A silent fallback to the kernel — the failure every public Onload-on-AF_XDP report shows — cannot
  produce a figure: `EF_NO_FAIL=0`, the `zc:1` read and the segment counters each catch it
  independently.
- The prediction is on record before the number exists, so a DROP is not a surprise explained after
  the fact, and a KEEP would be a finding against it.
- Dropping on the first failing procedure saves the second procedure's boot time without weakening
  a KEEP, which still needs both.

**Bad — and accepted**

- **The instrument is 88 % Mac.** The counterparty's unpinned macOS stack sits in both arms; its
  dispersion is part of both medians, and a real gain of a few µs on the desk is invisible beside
  it. That is the instrument ADR-0098 chose; this ADR does not replace it, it states its size.
- The twin is not the published §6 configuration: no observer, and the channel count ADR-0201 sets.
  Its numbers are an A/B arm, never a figure.
- Decision 3's p99 and two-path readings and decision 5's corpus reading are interpretations of an
  accepted kill line, confirmed by the owner (plan Q2, 2026-09-24) but written here, not in
  ADR-0098, whose substance is not edited; a reader of ADR-0098 alone does not see them.
- The segment-counter guard is a count bound over the whole machine, not a per-connection proof;
  other TCP traffic on the desk during a run could raise it, never lower it, so it can only produce
  a false FAIL.
- `sudo -n ss` in a committed script is one more root line; ADR-0093's rule applies to it.

## Sources

Read 2026-09-24: <https://github.com/Xilinx-CNS/onload/blob/master/README.md>;
<https://github.com/Xilinx-CNS/onload/issues/337>, `/issues/139`, `/issues/10`, `/issues/5`, `/issues/51`;
Onload `src/lib/efhw/af_xdp.c`, `src/lib/efhw/af_xdp_bpf.c`,
`src/include/ci/internal/opts_netif_def.h`, `opts_citp_def.h` (master, last commit 2026-09-21);
<https://docs.kernel.org/networking/af_xdp.html>, `xsk-tx-metadata.html`, `xdp-rx-metadata.html`;
iproute2 `misc/ss.c`; <https://arxiv.org/html/2402.10513v1>. In the repository:
`docs/reference/measured-costs.md` *C-40*; `tools/w2w/src/main.rs` module doc *Wire timestamps*;
`crates/engine/tests/wire.rs:205`. On the desk, read-only: `grep -E 'igb.*(xsk|_zc|metadata)'
/proc/kallsyms`, `ss -V`, `grep XDP_SOCKETS /boot/config-$(uname -r)`.
**Searched, found nothing:** a published latency figure for Onload over AF_XDP on any `igb` NIC; any
statement that Onload yields hardware timestamps over AF_XDP.
