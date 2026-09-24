# ADR-0201 — Onload on the I211 runs without hardware flow filters, one channel count holds for the whole boot, and the control path leaves the cable

- **Status**: Proposed — 2026-09-24, with
  [plans/2026-09-24-p4-bypass-and-s9-boot](../plans/2026-09-24-p4-bypass-and-s9-boot.md). Decision 1
  is confirmed or replaced by that plan's step 6.1 probe, whose output is quoted into this ADR's
  *Result* before it is accepted.
- **Date**: 2026-09-24
- **Deciders**: Tran Manh Thang. Written by the architect (Opus).
- **Related**: [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  item 2; [ADR-0200](ADR-0200-a-bypass-arm-is-judged-from-the-counterparty-against-a-same-boot-kernel-twin-and-its-kill-line-is-arithmetic-written-first.md);
  `DESIGN.md` §9 (IRQ affinity, coalescing, EEE rows); `docs/hft-playbook.md`.

## Context

An AF_XDP socket receives from **one** NIC queue: "if you bind to queue 0, you are NOT going to get
any traffic that is distributed to queues 1 through 7", and the traffic must be steered there by
limiting the device to one queue or by NIC filters (<https://docs.kernel.org/networking/af_xdp.html>).

**Found, 2026-09-24:**

1. **Onload steers with ethtool n-tuple rules.** `src/lib/efhw/af_xdp.c` `af_xdp_filter_insert()`
   converts each socket's filter to an `ethtool_rx_flow_spec` and calls the driver's `set_rxnfc`
   with `ETHTOOL_SRXCLSRLINS`, the queue taken from the VI's `dmaq_id`. The module parameter
   `enable_af_xdp_flow_filters` (default `1`) turns this into a no-op that returns a magic id.
2. **`igb` refuses every rule that is not an Ethernet rule.** `drivers/net/ethernet/intel/igb/igb_ethtool.c`,
   `igb_add_ethtool_nfc_entry`: `if ((fsp->flow_type & ~FLOW_EXT) != ETHER_FLOW) return -EINVAL;` —
   it matches EtherType, VLAN priority and MAC addresses only. A TCP 4-tuple rule, which is what an
   Onload listening or connected socket asks for, cannot be installed on this NIC. Onload issue #10
   (Intel 82599) shows what that looks like: `oof_socket_add_full_hw: ... FILTER TCP ... failed (-95)`
   and the connection fails (<https://github.com/Xilinx-CNS/onload/issues/10>).
3. **The desk's I211 has two queues and RSS splits across them** `[read 2026-09-24]`:
   `ethtool -l enp9s0` → combined 2 (max 2); `ethtool -x enp9s0` → indirection entries 0–63 to
   queue 0, 64–127 to queue 1; `ntuple-filters: off` (not fixed). A TCP flow lands on either queue by
   hash. Onload issue #139 got AF_XDP working on AWS only after `ethtool -L <if> combined 1`
   (<https://github.com/Xilinx-CNS/onload/issues/139>).
4. **With no filters, the XDP program takes every IPv4 TCP and UDP frame on its queue.**
   `src/lib/efhw/af_xdp_bpf.c`: broadcast, non-IPv4 VLAN, IPv6 and non-TCP/UDP frames return
   `XDP_PASS`; everything else is `bpf_redirect_map(&xsks_map, ctx->rx_queue_index, XDP_PASS)` — to
   the Onload stack whenever an XSK is in the map for that queue. What the stack does with a
   segment for a socket it does not own was not found documented.
5. The desk reaches the Mac today over the cable (`ssh thangtran@192.168.77.2`, memory of
   2026-09-14) — through `enp9s0`, the queue decision 1 hands to Onload. Both machines are also on
   Tailscale (`tmt-b450-i-aorus-pro-wifi` 100.99.156.121; `thangs-mac-mini` 100.108.2.110, offline
   at the time of reading) and the desk has Wi-Fi (`wlp7s0` 192.168.31.125).

## Decision

1. **Filters are tried once, in the probe, and turned off when `igb` refuses them.** The step 6.1
   probe registers `enp9s0` with Onload's defaults and reads `dmesg` for filter failures. On a
   refusal (the expected outcome, fact 2) Onload runs with
   `/sys/module/sfc_resource/parameters/enable_af_xdp_flow_filters = 0` and the NIC at
   **`combined 1`**, so the one queue Onload binds is the only queue there is. Both values are
   `DESIGN.md` §9 rows for a bypass boot, read back by `scripts/check-machine.sh`.
2. **One channel count for the whole boot, every arm.** The channel change bounces the link and
   renumbers the NIC's IRQs; doing it once, before the boot's first `check-machine.sh`, means the
   IRQ-affinity, coalescing and EEE rows are read after it and every arm — `io_uring` control,
   `io_uring`, the Onload twin, Onload — runs on the same NIC configuration. The count in force is
   printed on a row, so no figure can be quoted without it.
3. **During a bypass boot nothing but the measured connection crosses `enp9s0`.** The generator is
   driven over ssh on the Mac's **non-cable** address (Tailscale or Wi-Fi, whichever the pre-boot
   check finds reachable), for **both** arms so the arms stay identical (ADR-0200 decision 1); the
   generator itself still connects to `192.168.77.1` over the cable. Nothing else on the desk may
   use the cable while an arm runs (no `git push` to the Mac, no rsync).
4. **Registration is bracketed by read-backs.** Register before the Onload arm, unregister after it;
   the twin runs only after `check-machine.sh` with `FIXBOLT_BYPASS=twin` reads no XDP program on
   the NIC.

## Consequences

**Good**

- The configuration is the one that works on this NIC rather than the one Onload's defaults assume;
  a filter failure cannot turn into a silent kernel run (ADR-0200 decision 2 catches that anyway).
- Every arm of the boot shares one NIC state, so the `io_uring` A/B and the Onload A/B are each one
  variable.

**Bad — and accepted**

- **This is not a deployable shape.** With filters off and one queue, Onload owns every TCP and UDP
  frame the NIC receives while its stack lives; on a machine where that NIC carries anything else,
  the other traffic goes to a stack that does not own it. `docs/hft-playbook.md` must say that on
  `igb` Onload-over-AF_XDP means a NIC dedicated to it.
- `combined 1` is not the configuration of the published §6 NIC figure (combined 2). No figure from
  this boot is compared across boots; everything in it is an A/B against its own control.
- The control path over Wi-Fi or Tailscale adds that interface's interrupts to the desk during every
  run of both arms — the same in both, on the housekeeping cores.
- The probe's decision is a fact about Onload at one tag and `igb` at one kernel; either may change.

## Result

*(To be filled from the step 6.1 probe: the `dmesg` lines at registration with default filters, and
whether a stack came up with `enable_af_xdp_flow_filters=0` and `combined 1`.)*

## Sources

Read 2026-09-24: <https://docs.kernel.org/networking/af_xdp.html>; Onload `src/lib/efhw/af_xdp.c`,
`src/lib/efhw/af_xdp_bpf.c` (master); Linux `drivers/net/ethernet/intel/igb/igb_ethtool.c` (master);
<https://github.com/Xilinx-CNS/onload/issues/10>, `/issues/139`, `/issues/70`. On the desk,
read-only: `ethtool -l|-x|-k enp9s0`, `ip -4 -br addr`, `tailscale status`.
