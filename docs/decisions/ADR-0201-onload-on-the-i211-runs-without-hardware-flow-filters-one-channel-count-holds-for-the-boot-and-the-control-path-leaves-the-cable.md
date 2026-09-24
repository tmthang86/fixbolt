# ADR-0201 — Onload on the I211 runs without hardware flow filters, one channel count holds for the whole boot, and the control path leaves the cable

- **Status**: **Deprecated — 2026-09-24, never accepted**: Onload over AF_XDP was dropped at the
  probe's gate G1 before any decision below could be exercised; the *Result* section stands as the
  record of that drop ([ADR-0203](ADR-0203-an-item-that-cannot-run-is-dropped-on-its-failing-gates-evidence-in-place-of-a-pair.md)). **Revised in place 2026-09-24, third time**
  (senior review of PR #111): *Result* corrected — the filter and queue variants never reached the
  RSS check; the errno values are replaced by the `I/O error` lines the evidence holds; Onload issue
  #257 cited as precedent; *Sources* completed. History: Proposed — 2026-09-24, with
  [plans/2026-09-24-p4-bypass-and-s9-boot](../plans/2026-09-24-p4-bypass-and-s9-boot.md). Decision 1
  is confirmed or replaced by that plan's step 6.1 probe, whose output is quoted into this ADR's
  *Result* before it is accepted. **Revised in place 2026-09-24**: *Result so far* added — the first
  probe stopped at the build of Onload `v9.0.2`, and Onload is re-pinned to an untagged commit on its
  `v9_2` release branch; decisions 1–4 are unchanged. **Revised in place 2026-09-24, second time**:
  the probe at the new pin failed G1 at hardware init; *Result* is filled and records **Onload over
  AF_XDP as dropped** on this desk. Decisions 1–4 were never exercised and stand only as the design
  for a reopening; the boot runs at `combined 2` with the generator driven over the cable.
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

## Result so far

- `[2026-09-24]` **Probe attempt 1, G1 failed at the build, before any registration.** Onload `v9.0.2`
  (`9f330e7058`) on `7.0.0-31-generic`:
  `src/lib/efhw/af_xdp.c:375:28: error: passing argument 2 of ‘kernel_bind’ from incompatible pointer
  type … expected ‘struct sockaddr_unsized *’`
  (log `target/p4-probe/onload_install.txt` on the desk, not committed; `onload_install: ERROR: Build
  failed.  Not installing.`). Nothing was installed or loaded; the NIC was not touched. Decisions 1–4
  were therefore not exercised.
- **Re-pin**: upstream fixed the build in `268f1d4c8a` *"ON-17217: Add compat for sockaddr_unsized
  (6.19)"* (2026-02-23), and no tag after `v9.0.2` exists. Onload is pinned to
  **`174b947d0b9b7b77439463afbabf8a7e417b3706`**, the head of the `v9_2` release branch (2026-08-18,
  `versions.env` `ONLOAD_VERSION=9.2.2`), **untagged**. It contains the fix (GitHub compare
  `268f1d4c8a...v9_2`: ahead 269, behind 0) and not the 82 later `master` commits, which include
  ON-17442's datapath-selection rework — the code that decides which NIC is accelerated. The probe
  proves the pin before building: `git merge-base --is-ancestor 268f1d4c8a HEAD`.

## Result

**Dropped, 2026-09-24, at the probe's gate G1 — before any stack existed, so before any decision
here could be tried.** Per ADR-0098 and its Q1, a dropped item counts as done.

| Attempt | Onload | Stopped at | Excerpt (`…` marks elided text) |
|---|---|---|---|
| 1 | `v9.0.2` (`9f330e7058`) | build | `src/lib/efhw/af_xdp.c:375:28: error: passing argument 2 of ‘kernel_bind’ from incompatible pointer type … expected ‘struct sockaddr_unsized *’` |
| 2 | `174b947d0b9b7b77439463afbabf8a7e417b3706` (`v9_2`, untagged; `git merge-base --is-ancestor 268f1d4c8a HEAD` true) | registering `enp9s0` | `` [sfc efhw] af_xdp_rss_get_support: enp9s0 does not support `get_rxfh_key_size` operation `` then `[sfc efrm] ?: ERROR: hardware init failed rc=-95` |

Attempt 2 built and installed (`onload_install: Install complete.`) and its modules loaded. The
**first** registration, with default filters and two queues, failed with the two lines above, and
Onload added the NIC anyway (`[onload] oo_nic_add: ifindex=2 oo_index=0 flags=0 alternate=-1` at
`5123.778185`). Every later write — `unregister`; `register` after
`enable_af_xdp_flow_filters=0` and `ethtool -L enp9s0 combined 1`; `unregister`; `register` —
printed only `sh: 1: echo: echo: I/O error`, and the five copies of the `af_xdp_rss_get_support`
line in the log all carry the timestamp `5123.778141`: **the variants of decision 1 never reached
the RSS check**, because the NIC was left half-registered. That they would fail the same way is
established from Onload's source, not from a run: `af_xdp_rss_get_support` reads only the driver's
`ethtool_ops` (`get_rxfh_indir_size`, `get_rxfh_key_size`), which neither the filter parameter nor
the channel count changes. `onload_tool unload --onload-only` removed the modules
(`oo_nic_remove` at `5218.812979`); `ethtool -L enp9s0 combined 2` restored the channel count. Evidence on the desk, not committed:
`target/p4-probe/{onload_install.txt, onload_install-v9_2.txt, probe-v9_2.txt, after-v9_2.txt}`.

**Why no Onload version fixes it on this kernel.** Onload's `af_xdp_rss_get_support`
(`src/lib/efhw/af_xdp.c`, present at `v9.0.2`, at the `v9_2` pin and on `master`) returns
`-EOPNOTSUPP` when the driver lacks `get_rxfh_indir_size` **or** `get_rxfh_key_size`, and
`af_xdp_nic_init_hardware` returns that error. The desk's `igb` (`7.0.0-31-generic`) has the first and
not the second — `ethtool -x enp9s0`, read before the probe, already printed `RSS hash key: Operation
not supported`. Linux added the operation to `igb` in commits `dfaf57ef99cf`, `1ae67b2b28bc` (*"igb:
expose RSS key via ethtool get_rxfh"*, the commit that adds `get_rxfh_key_size`) and
`e3c94e9782a7`, first in **`v7.3-rc1`** (not in `v7.2`). **Precedent:** Onload issue
[#257](https://github.com/Xilinx-CNS/onload/issues/257) (vmxnet3, 2025-01-14) shows the same two
lines, `af_xdp_rss_get_support: ens160 does not support get_rxfh_key_size operation` and
`hardware init failed rc=-95`, followed by `oo_nic_add`; #270 (virtio-net) reports "hardware init
failed" on another virtual NIC.

**What would reopen it:** a measurement NIC whose driver provides both RSS key operations and AF_XDP
zero-copy — for example `igb` on a kernel ≥ 7.3 — **and** an Onload that builds on that kernel (its
README lists kernels up to 7.0). The check costs no install: `ethtool -x <nic>` must print an RSS
key. Reopening needs a new plan; ADR-0200 holds the measurement design it would use.

## Sources

Read 2026-09-24: <https://docs.kernel.org/networking/af_xdp.html>; Onload `src/lib/efhw/af_xdp.c`,
`src/lib/efhw/af_xdp_bpf.c` (master); Linux `drivers/net/ethernet/intel/igb/igb_ethtool.c` (master);
<https://github.com/Xilinx-CNS/onload/issues/10>, `/issues/139`, `/issues/257`, `/issues/270`;
Onload `src/lib/efhw/af_xdp.c` at `v9.0.2`, at `174b947d0b` and on `master`
(`af_xdp_rss_get_support`); GitHub compare `Xilinx-CNS/onload` `268f1d4c8a...v9_2` (ahead 269,
behind 0); Linux commit `1ae67b2b28bc` *"igb: expose RSS key via ethtool get_rxfh"* (with
`dfaf57ef99cf`, `e3c94e9782a7`), first tag containing it `v7.3-rc1`. On the desk, read-only:
`ethtool -l|-x|-k enp9s0`, `ip -4 -br addr`, `tailscale status`; the probe's own output
`target/p4-probe/{before.txt, onload_install.txt, onload_install-v9_2.txt, probe-v9_2.txt, after-v9_2.txt}`.
