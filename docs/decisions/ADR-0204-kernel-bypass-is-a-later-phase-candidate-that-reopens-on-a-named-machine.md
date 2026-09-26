# ADR-0204 — Kernel bypass is a later-phase candidate that reopens only on a named machine

- **Status**: **Accepted — 2026-09-26, by the owner's answers to decision 5** (in conversation,
  verbatim: *"1. thuê VPS 2. không 3. thuê — chấp nhận con số đo ở VPS, nếu cần thì viết lại ADR"*).
  **Revised in place 2026-09-26, while Proposed** (`CLAUDE.md` §5; ADR-0002 shows the shape): the
  first text left decision 5's three questions to the owner; the owner answered them the same day —
  (1) a commodity NIC over AF_XDP on a **rented cloud VM**, not a Solarflare card; (2) the counterparty
  is **not the Mac mini**; (3) **rent, do not buy**, and a figure measured on a VM is accepted as
  publishable. Decisions 2, 4 and 5 are rewritten to carry those answers, and the acceptance of VM
  figures, which reverses a sentence of `DESIGN.md` §9, is decided separately in
  [ADR-0205](ADR-0205-a-latency-figure-from-a-cloud-vm-is-published-under-its-own-label-beside-a-same-boot-kernel-twin-and-never-compared-with-the-desk.md).
  The first text's decision 4 (*"A figure is published only from a host that is not a guest"*) is
  withdrawn by that answer.
- **Date**: 2026-09-26
- **Deciders**: Tran Manh Thang. Written by the architect (Opus).
- **Related**: [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  item 2, Q1, Q3, Q5; [ADR-0099](ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md);
  [ADR-0200](ADR-0200-a-bypass-arm-is-judged-from-the-counterparty-against-a-same-boot-kernel-twin-and-its-kill-line-is-arithmetic-written-first.md);
  [ADR-0201](ADR-0201-onload-on-the-i211-runs-without-hardware-flow-filters-one-channel-count-holds-for-the-boot-and-the-control-path-leaves-the-cable.md)
  *Result*; [ADR-0203](ADR-0203-an-item-that-cannot-run-is-dropped-on-its-failing-gates-evidence-in-place-of-a-pair.md);
  [ADR-0205](ADR-0205-a-latency-figure-from-a-cloud-vm-is-published-under-its-own-label-beside-a-same-boot-kernel-twin-and-never-compared-with-the-desk.md);
  [ADR-0141](ADR-0141-fixp-is-built-only-in-the-venues-current-dialect-in-a-role-that-dialect-has-a-referee-for-and-not-before-phase-5.md)
  (phase 5 is not scoped); `PRD.md` §2 *Later phases*, §5 *Kernel bypass*; `DESIGN.md` §9;
  [kernel-bypass-needs-a-machine-this-project-does-not-have](../reference/kernel-bypass-needs-a-machine-this-project-does-not-have.md).

## Context

Phase 4 item 2 (Onload over AF_XDP) was dropped at gate G1 on 2026-09-24: the desk's I211 (`igb`,
`7.0.0-31-generic`) has no `get_rxfh_key_size`, so Onload cannot register it (ADR-0201 *Result*).
ADR-0203 made that drop count as done for phase 4, and nothing in phase 4 changes here.

A drop on a machine is not a finding about bypass: no arm ran, and ADR-0200 decision 4's prediction
was never tested. The owner wants the item kept for later. What stops it is hardware, so the record
must say which hardware, and whether renting is enough. The research is on the reference page named
above; its conclusions:

- Onload's AF_XDP path needs both RSS size operations, n-tuple steering and zero-copy in the driver.
  `ixgbe`, `i40e`, `ice` and `mlx5` have them on every kernel Onload supports (6.1 – 7.0); `igb` and
  `igc` gain the missing key operation only in `v7.3-rc1`, past Onload's range.
- AWS `ena` (`.metal` included) lacks n-tuple steering, and Onload fails on it (issues #62, #337);
  Azure `mana` lacks it too. GCP `gve` has every operation in the driver, and is the only VM driver
  with a report of Onload working (#337, 2026-09-21, an AF_XDP-code contributor, not reproduced
  here). On `gve`, n-tuple steering and the RSS key are **offered by the virtual device per platform**,
  so the driver having the operations is not enough.
- With the Mac mini as the counterparty (~232 µs p50 against a 27–29 µs acceptor window), a 10 %
  counterparty-side gain is arithmetically near-impossible (ADR-0200 decision 4). The owner's answer
  (2) removes the Mac from the item.

Phase 5 is not scoped (ADR-0141 *Consequences*), so this item cannot be assigned to a numbered phase.

## Decision

1. **Kernel bypass is a candidate for a phase after 4, not a phase-4 item.** It is listed in
   `PRD.md` §2 *Later phases*, with no phase number until the owner scopes a phase that includes it.
   Phase 4's exit criterion 3 stays met by ADR-0203's record.
2. **The platform is two rented cloud VMs, and the candidate is GCP with `gve` (gVNIC).** One VM runs
   the acceptor, the other the counterparty, in one zone. AWS (`ena`) and Azure (`mana`) are excluded
   by the reference page's table. Nothing is bought (owner's answer 3); no Solarflare card
   (answer 1 — ADR-0098 Q5's *no `ef_vi` card* carries over to this item).
3. **It reopens only on a named pair of instances that passed the gate.** Before anything is cloned
   or built, the reopening plan quotes, from the acceptor instance: the machine type and zone;
   `uname -r` inside Onload's supported range; `ethtool -i <nic>` naming `gve`; the driver's boot log
   line `FLOW STEERING device option enabled with max rule limit of N` with N > 0; `ethtool -K <nic>
   ntuple on` succeeding; `ethtool -x <nic>` printing an RSS hash key (G0); and `ethtool -L` set to at
   most half the maximum queue count, which `gve` requires before an XDP program attaches. After
   registration, `ss --xdp` must read `zc:1`. A machine type whose device does not offer flow
   steering fails the gate; the plan may try another series, and the series that passed is part of
   every figure's label.
4. **What it reopens as is unchanged**: ADR-0099 (kernel TCP stays the headline; a bypass figure is a
   second, labelled row beside a same-boot kernel twin), ADR-0098 Q3 (Onload only, no TCP stack of
   this project's own), `hft` only, plaintext only. Figures are measured from the counterparty VM
   and published under ADR-0205. ADR-0200 (Deprecated) is the design a reopening starts from; its
   decision 1 (the Mac as counterparty) and decision 4 (the desk's arithmetic) do not carry over, and
   the reopening plan writes its own kill line before the first run (ADR-0098 Q1's rule).
5. **Rented machines, by use.** Development, functional testing and the published measurement all
   run on the same kind of rented pair. CI gets no Onload job: the item carries no engine code, and
   a self-hosted runner on a rented VM is a secret-bearing machine outside this repository.

## Consequences

**Good**

- The item has an owner-visible home and one test that says it can start: a quoted gate on a named
  pair of instances. Nobody reopens it by trying Onload again on the I211 and kernel `7.0`.
- No purchase, and no dependence on the kernel and Onload both moving for the desk's `igb`.
- The Mac's 232 µs round trip no longer sits in the instrument.
- The hardware facts live on one page that can be re-read when drivers change, without editing an
  ADR.

**Bad — and accepted**

- **No date and no phase number.** A candidate with no phase can stay a candidate for good.
- **The platform rests on one report.** GCP support for Onload is one contributor's comment; Onload's
  AF_XDP path is *not at release quality* by its own README; flow steering on `gve` depends on the
  machine series. The gate can fail on every series tried, and then the item has no platform again.
- **A VM figure carries everything ADR-0205 accepts**: not comparable with the desk, neighbours
  measured rather than removed, several §9 rows absent, no NIC timestamps on `gve`.
- **Rented time costs money while it runs**, and a recreated instance is a new machine whose
  figures are re-measured, not reused.

## Sources

The reference page's *Sources* (read 2026-09-26), and in particular Onload `src/lib/efhw/af_xdp.c`
(`master`, `51dbf861c3`); Onload issues
[#62](https://github.com/Xilinx-CNS/onload/issues/62),
[#83](https://github.com/Xilinx-CNS/onload/issues/83),
[#337](https://github.com/Xilinx-CNS/onload/issues/337); Linux commits `1ae67b2b28bc` (`igb`) and
`f243be8edeab` (`igc`), both first in `v7.3-rc1`; Linux `drivers/net/ethernet/google/gve/gve_adminq.c`
(flow steering and RSS as device options); the `gve` driver
[README](https://github.com/GoogleCloudPlatform/compute-virtual-ethernet-linux) (*Receive Flow
Steering*, *XDP*); `DESIGN.md` §9 first row; ADR-0200 decision 4.
