# ADR-0202 — Phase 4's one §9 boot is pre-built, driven by a committed script, and Onload lives only inside its block

- **Status**: Proposed — 2026-09-24, with
  [plans/2026-09-24-p4-bypass-and-s9-boot](../plans/2026-09-24-p4-bypass-and-s9-boot.md).
  **Note, 2026-09-24 — the Onload block will not run.** Onload over AF_XDP was dropped at the probe
  (ADR-0201 *Result*). Decisions 1, 2 and 5 stand for the boot, which now measures the `io_uring`
  A/B (row 5) and the SQLite-store `w2w` pair (row 4, step 4b). Decision 3's order loses its Onload
  block: procedure 1 is the `io_uring` block then the store pair, the bench rotation fills the gap,
  procedure 2 reverses both. Decision 4 is carried out early: Onload is uninstalled from the desk
  before the boot, and the boot checks that no `onload` or `sfc_resource` module is loaded.
- **Date**: 2026-09-24
- **Deciders**: Tran Manh Thang. Written by the architect (Opus).
- **Related**: [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  §6 order 4 (items 1 and 2 may share a boot if both arms are pre-built);
  [ADR-0090](ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  decisions 1–3; [ADR-0093](ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md);
  [ADR-0068](ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md) decision 1;
  [ADR-0200](ADR-0200-a-bypass-arm-is-judged-from-the-counterparty-against-a-same-boot-kernel-twin-and-its-kill-line-is-arithmetic-written-first.md),
  [ADR-0201](ADR-0201-onload-on-the-i211-runs-without-hardware-flow-filters-one-channel-count-holds-for-the-boot-and-the-control-path-leaves-the-cable.md).

## Context

Rows 5 (`io_uring`) and 6 (Onload) of phase 4 each need a §9 boot; the scope plan budgets **one**.
A reboot ends the manager's session (the owner's rule: one session, one pull request), so the boot's
work is split between a session that prepares it and a session that runs it, joined only by what is
on disk. Boots D–F showed the costs: a manager tool call during a rotation costs an arm-round
(memory, boot E); the first run after a reboot is disqualified by gnome-shell settling; package
timers take rounds unless stopped; the desk comes back without its runtime toggles.

Onload is out-of-tree kernel modules from a tree whose AF_XDP support "is a community-supported work
in progress that is not currently at release quality"
(<https://github.com/Xilinx-CNS/onload/blob/master/README.md>, which lists kernels 6.1–7.0 — the desk
runs `7.0.0-31-generic`, the last listed). Loaded, it is part of the kernel every later figure on
the desk is taken on.

## Decision

1. **One boot serves both rows, and it compiles nothing.** Every binary is built before the reboot
   from the merge commit of the preparing pull request, in worktrees under `../fb-p4-boot/`, one per
   feature set (`io-uring` on and off are two worktrees — ADR-0090 decision 2), with sha256 in
   `MANIFEST.txt`; the Mac's `w2w` is built at the same commit and its sha256 recorded. The boot
   checks the hashes before and after.
2. **A committed driver runs the boot unattended.** `scripts/boot-p4.sh` runs the blocks below in
   order, reads `scripts/check-machine.sh` before each block with the `FIXBOLT_BYPASS` value that
   block needs, stops on a red row, and keeps every output under `target/boot-p4-evidence/` (never
   `/tmp`, which is tmpfs on the desk). Its `sudo -n` lines follow ADR-0093. It is rehearsed on the
   desktop line with `RUNS=2` before the reboot, and that rehearsal's output is labelled *not a
   figure*. The manager launches it and makes no tool call until it exits.
3. **The order, and why.** Procedure 1: the `io_uring` block (control and `io_uring`, `hft` with
   acceptor stamps, then `standard`), then the Onload block (twin, then Onload). Between the
   procedures, the `turn.rs`/`density.rs` rotation for `io_uring` fills ADR-0068's thirty-minute
   gap. Procedure 2: the Onload block first (Onload, then twin), then the `io_uring` block reversed
   — so no arm always runs first. Procedure 2's Onload block runs **only if procedure 1 met every
   clause** (ADR-0200 decision 3).
4. **Onload is present only inside its block.** Installed before the boot (the probe needs it), it
   must not load at boot: the boot's first `check-machine.sh` runs with `FIXBOLT_BYPASS=absent` and
   FAILs on a loaded `onload` or `sfc_resource` module. The Onload block loads the modules
   (`onload_tool reload`), registers the NIC, runs, unregisters, and unloads. The `io_uring` block
   runs with the modules absent. **After the boot, whatever the verdict, Onload is uninstalled**
   (`onload_uninstall`): the desk measures, it does not deploy, and a KEEP is published through
   `docs/hft-playbook.md`, not through a desk that carries the modules.
5. **The reboot is the handoff.** Before it: the preparing PR is merged with its CI run id named,
   `STATUS.md` *Start here* names the boot's first action and a do-not list, the §9 grub line
   (`/etc/default/grub.fixbolt-s9-bootf-20260923` — never `grub.fixbolt-s9`, which carries
   `nohz_full`) is installed with `update-grub`, and the owner is told the reboot count: one.

## Consequences

**Good**

- One reboot for two hot-path items; every `io_uring` and Onload arm shares the boot's machine state.
- The boot's only live decision is procedure 2's Onload block, and it is made by a script from a
  rule written here.
- The desk comes out of the phase with the kernel it went in with.

**Bad — and accepted**

- **A long boot.** Two procedures of up to twelve arm-sets of twenty runs plus a bench rotation is
  four to six hours on the desk, unattended; a failure early in the night costs the rest.
- **The Onload modules are built and loaded on the owner's desktop before the boot** (the probe),
  from a tree not at release quality. Secure Boot is off on the desk
  (`mokutil --sb-state`, read 2026-09-24), so an unsigned module loads; a crash in it is a crash of
  the owner's desktop.
- `onload_uninstall` after a KEEP means a later re-measurement repeats the install.
- A driver script is one more file that must be right the first time it runs on the §9 line; the
  rehearsal proves its plumbing, not its figures.

## Sources

ADR-0090, ADR-0093, ADR-0068; `STATUS.md` *Start here — 2026-09-23 (boot F)*; the desk's
`/etc/default/grub*` files, read 2026-09-24 (`grub.fixbolt-s9-bootf-20260923`:
`isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`; `grub.fixbolt-s9`: adds
`nohz_full=6,7,14,15`; `/etc/default/grub` identical to `grub.fixbolt-desktop-20260922`);
<https://github.com/Xilinx-CNS/onload/blob/master/README.md>, `DEVELOPING.md`.
