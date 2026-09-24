# ADR-0203 — An item that cannot run is dropped on its failing gate's evidence in place of a pair

- **Status**: **Accepted — 2026-09-24, by the manager under the owner's delegation of 2026-09-18**
  (one word from the owner reverses it). It **supersedes, for items that never run, the part of
  [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  *Decision* that defines "Killed" by "the pair that killed it", and exit criterion 3's "if killed,
  the negative pair in `measured-costs.md`"**. ADR-0098 is Accepted and its substance is not edited
  (`CLAUDE.md` §5); its status block gains one line pointing here. Raised by the senior review of
  PR #111 (finding M4).
- **Date**: 2026-09-24
- **Deciders**: Tran Manh Thang (delegation of 2026-09-18). Written by the architect (Opus).
- **Related**: ADR-0098 item 2 and exit criterion 3;
  [ADR-0099](ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md);
  [ADR-0201](ADR-0201-onload-on-the-i211-runs-without-hardware-flow-filters-one-channel-count-holds-for-the-boot-and-the-control-path-leaves-the-cable.md)
  *Result*; [plans/2026-09-24-p4-bypass-and-s9-boot](../plans/2026-09-24-p4-bypass-and-s9-boot.md)
  step 6.1.

## Context

ADR-0098 wrote each phase-4 item's kill line before its code and defined a killed item as one whose
code is removed, **whose killing pair is recorded in `measured-costs.md`**, and whose admitting ADR is
marked with the result; its exit criterion 3 asks, for a killed bypass item, for "the negative pair".
The owner's answer to the bypass plan's Q1 (2026-09-24) was *measure*, not *drop on arithmetic*.

Item 2 (kernel bypass, Onload over AF_XDP) never reached a measurement. On the desk's I211 (`igb`,
`7.0.0-31-generic`) Onload's hardware init refuses the NIC: `af_xdp_rss_get_support: enp9s0 does not
support get_rxfh_key_size operation`, `hardware init failed rc=-95` (ADR-0201 *Result*). No AF_XDP
stack can exist on this kernel, so no arm can run, so no pair can be produced — not now, not by
another boot, not by another Onload version. The same failure is on record elsewhere
(Onload issue [#257](https://github.com/Xilinx-CNS/onload/issues/257), vmxnet3, 2025-01-14).

ADR-0098's definition was written for an item that runs and loses. Read literally, it leaves an item
that cannot run neither kept nor killed.

## Decision

1. **An item whose entry gate fails is dropped on that gate's evidence.** When a phase-4 item cannot
   produce either arm of its pair because a written gate before the measurement fails, the record
   that stands in for the pair is: the gate's name and criterion as written before the attempt; the
   verbatim lines of the failure; the commit, version and kernel it ran on; the path of the evidence
   on the desk; and **why no other attempt on the same machine can pass** (the cause, from the
   source, not from retries).
2. **The rest of "Killed" holds unchanged**: code (if any) removed on the same branch; the entry in
   `docs/reference/measured-costs.md` written, headed as *dropped at a gate, no pair*; the admitting
   ADR marked with the result; the item counts as done (ADR-0098 Q1).
3. **What is not allowed by this ADR**: dropping an item that *can* run because its arithmetic
   predicts a loss (that is Q1's option B, which the owner declined for item 2); dropping on a gate
   whose cause is a fixable choice of this project (a version, a flag, a configuration) — that is a
   retry, bounded by the plan, not a drop.
4. **For item 2**: the gate is G1 of the bypass plan's step 6.1 ("no Onload stack can be built,
   even with filters off and one queue"); the cause is that Onload's `af_xdp_rss_get_support` reads
   only the driver's `ethtool_ops` and this kernel's `igb` has no `get_rxfh_key_size`, which Linux
   added in `1ae67b2b28bc` (first in `v7.3-rc1`). Exit criterion 3 is met by that record.

## Consequences

**Good**

- An item that cannot run has a defined end, recorded with evidence, instead of an exit criterion
  that can never be met.
- The owner's Q1 answer stands: the item was not dropped on a prediction; it was attempted and
  stopped at a gate written before the attempt.

**Bad — and accepted**

- **No number exists for the bypass item.** A reader looking for the negative pair ADR-0099 made
  room for finds a gate failure instead; the arithmetic prediction (ADR-0200 decision 4) is never
  tested.
- A "why no other attempt can pass" argument is reasoning from source, and source changes; the
  record names the kernel commit that would change it, and a reopening is a new plan.
- The line between "cannot run" and "fixable by a retry" is a judgement; decision 3 draws it, and a
  reviewer may still dispute a particular case.

## Sources

ADR-0098 *Decision* (the definition of "Killed") and *Exit criteria* row 3; ADR-0201 *Result*;
`target/p4-probe/probe-v9_2.txt` on the desk; <https://github.com/Xilinx-CNS/onload/issues/257>;
Linux commit `1ae67b2b28bc` *"igb: expose RSS key via ethtool get_rxfh"*.
