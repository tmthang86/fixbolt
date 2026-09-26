# ADR-0204 — Kernel bypass is a later-phase candidate that reopens only on a named machine

- **Status**: **Proposed — 2026-09-26.** The owner decided on 2026-09-26, in conversation, that
  kernel bypass is recorded for a later phase rather than left closed by phase 4's drop; this ADR
  writes down how. It is accepted when the owner approves the text.
- **Date**: 2026-09-26
- **Deciders**: Tran Manh Thang. Written by the architect (Opus).
- **Related**: [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  item 2, Q3, Q5; [ADR-0099](ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md);
  [ADR-0200](ADR-0200-a-bypass-arm-is-judged-from-the-counterparty-against-a-same-boot-kernel-twin-and-its-kill-line-is-arithmetic-written-first.md)
  decision 4; [ADR-0201](ADR-0201-onload-on-the-i211-runs-without-hardware-flow-filters-one-channel-count-holds-for-the-boot-and-the-control-path-leaves-the-cable.md)
  *Result*; [ADR-0203](ADR-0203-an-item-that-cannot-run-is-dropped-on-its-failing-gates-evidence-in-place-of-a-pair.md);
  [ADR-0141](ADR-0141-fixp-is-built-only-in-the-venues-current-dialect-in-a-role-that-dialect-has-a-referee-for-and-not-before-phase-5.md)
  (phase 5 is not scoped); `PRD.md` §2 *Later phases*, §5 *Kernel bypass*; `DESIGN.md` §9;
  [kernel-bypass-needs-a-machine-this-project-does-not-have](../reference/kernel-bypass-needs-a-machine-this-project-does-not-have.md).

## Context

Phase 4 item 2 (Onload over AF_XDP) was dropped at gate G1 on 2026-09-24: the desk's I211 (`igb`,
`7.0.0-31-generic`) has no `get_rxfh_key_size`, so Onload cannot register it (ADR-0201 *Result*).
ADR-0203 made that drop count as done for phase 4, and nothing in phase 4 changes here.

A drop on a machine is not a finding about bypass: no arm ran, and ADR-0200 decision 4's prediction
was never tested. The owner wants the item kept for later. What stops it is hardware, so the record
must say which hardware, and whether renting instead of buying is enough. The research is on the
reference page named above; its conclusions:

- Onload's AF_XDP path needs both RSS size operations, n-tuple steering and zero-copy in the driver.
  `ixgbe`, `i40e`, `ice` and `mlx5` have them on every kernel Onload supports (6.1 – 7.0); `igb` and
  `igc` gain the missing key operation only in `v7.3-rc1`, past Onload's range.
- AWS `ena` (`.metal` included) lacks n-tuple steering, and Onload fails on it (issues #62, #337);
  Azure `mana` lacks it too. GCP `gve` and `idpf` have every operation, and one contributor reported
  Onload working on both (#337, 2026-09-21), not reproduced here.
- `DESIGN.md` §9 already rules that a guest cannot measure. A dedicated bare-metal host can, within
  the §9 rows, but its path to the peer is a provider's switch fabric, not a cable.
- With the Mac mini as the counterparty (~232 µs p50 against a 27–29 µs acceptor window), a 10 %
  counterparty-side gain is arithmetically near-impossible (ADR-0200 decision 4). A peer is part of
  the machine requirement, not only the acceptor's NIC.

Phase 5 is not scoped (ADR-0141 *Consequences*), so this item cannot be assigned to a numbered phase.

## Decision

1. **Kernel bypass is a candidate for a phase after 4, not a phase-4 item.** It is listed in
   `PRD.md` §2 *Later phases*, with no phase number until the owner scopes a phase that includes it.
   Phase 4's exit criterion 3 stays met by ADR-0203's record.
2. **It reopens only on a named machine.** A reopening plan starts by naming the acceptor host, its
   NIC and driver, its kernel, and its peer, and by quoting G0 on that machine (`ethtool -x <nic>`
   prints an RSS hash key) and `ethtool -K <nic> ntuple on` succeeding. Without that quote there is
   nothing to plan. The reference page's *Minimum* and *Recommended* configurations are the
   candidates; the page, not this ADR, holds the hardware facts.
3. **What it reopens as is unchanged**: ADR-0099 (kernel TCP stays the headline; a bypass figure is a
   second, labelled row beside a same-boot kernel twin), ADR-0098 Q3 (Onload only, no TCP stack of
   this project's own), `hft` only, plaintext only. ADR-0200 and ADR-0201, deprecated, are the
   measurement design a reopening starts from, not a design it is bound to.
4. **Rented machines, by use.** Development and functional testing may run on a rented machine
   whose NIC driver passes the reference page's table, a VM included. CI needs no Onload job,
   because the item carries no engine code. A figure is published only from a host that is not a
   guest and passes `scripts/check-machine.sh`, and its row names the path to the peer.
5. **Not decided here, left to the reopening plan and the owner**: whether to buy a native
   Solarflare card (ADR-0098 Q5 said no for phase 4 only); whether the counterparty stays the Mac
   mini, and with it ADR-0200's instrument and kill line; which machine is bought or rented.

## Consequences

**Good**

- The item has an owner-visible home and one test that says it can start: a quoted G0 on a named
  machine. Nobody reopens it by trying Onload again on the I211 and kernel `7.0`.
- The hardware facts live on one page that can be re-read when prices or drivers change, without
  editing an ADR.
- Renting for development is allowed early, so work can start before a purchase, while §9 keeps the
  rented VM out of any published number.

**Bad — and accepted**

- **No date and no phase number.** A candidate with no phase can stay a candidate for good; the
  record says so rather than inventing a phase.
- **The cheapest path is outside this project's control.** The desk's `igb` works once both the
  kernel (≥ 7.3) and Onload (> 7.0 support) move, and neither has a date.
- **The reference page rests partly on others' reports**: GCP support is one contributor's comment,
  and Onload's AF_XDP path is *not at release quality* by its own README. A reopening re-checks, and
  can still end at a gate.
- **A peer is part of the cost.** The recommended shape is two hosts with matching NICs, so the
  purchase is two cards, not one; the minimum shape keeps the Mac and inherits a DROP prediction.
- **Decision 4 lets a figure come from a bare-metal rental** whose path to the peer passes a shared
  switch fabric; such a figure is a different instrument from the desk's C-40 and is labelled as such,
  not compared with it.

## Sources

The reference page's *Sources* (read 2026-09-26), and in particular Onload `src/lib/efhw/af_xdp.c`
(`master`, `51dbf861c3`); Onload issues
[#62](https://github.com/Xilinx-CNS/onload/issues/62),
[#83](https://github.com/Xilinx-CNS/onload/issues/83),
[#337](https://github.com/Xilinx-CNS/onload/issues/337); Linux commits `1ae67b2b28bc` (`igb`) and
`f243be8edeab` (`igc`), both first in `v7.3-rc1`; `DESIGN.md` §9 first row; ADR-0200 decision 4.
