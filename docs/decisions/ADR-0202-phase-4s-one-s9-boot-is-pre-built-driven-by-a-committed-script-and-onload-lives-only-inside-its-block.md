# ADR-0202 — Phase 4's one §9 boot is pre-built, driven by a committed script, and Onload lives only inside its block

- **Status**: Proposed — 2026-09-24, with
  [plans/2026-09-24-p4-bypass-and-s9-boot](../plans/2026-09-24-p4-bypass-and-s9-boot.md).
  **Revised in place 2026-09-24** (senior review of PR #111): Onload over AF_XDP was dropped at the
  probe (ADR-0201 *Result*), so decisions 1–4 are rewritten for a boot that measures the `io_uring`
  A/B (phase-4 row 5) and the SQLite-store `w2w` pair (row 4, step 4b) only; decision 5 is
  unchanged. The superseded text of decisions 1–4 named an Onload block, `FIXBOLT_BYPASS` values, a
  `turn`/`density` rotation of two binaries between the procedures, and Onload's uninstall after
  the boot. Stays Proposed until the preparing PR (7a) merges.
  **Revised in place 2026-09-25, second time** (the driver `scripts/boot-p4.sh` built and rehearsed
  for 7a.1, PR #113): decision 1 now names what the pre-build must also do and who does it — the
  file capability on both `w2w`, never a `nosuid` mount, the driver's `build` subcommand, the Mac's
  manual build recorded in `BUILD-INFO.txt` — and says the store pair's one-binary reading overrides
  the store plan's step 4b, which named two binaries. Same day, after the senior review of PR #113:
  capability checked before and after every arm; the build commit is the merge commit of the
  preparing PR, built after it merges and before the docs-only handoff; the timer refusal; exit 1.
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

1. **One boot serves rows 5 and 4b, and it compiles nothing.** Every binary is built before the
   reboot from the merge commit of the preparing pull request, in worktrees under `../fb-p4-boot/`,
   one per feature set (ADR-0090 decision 2): `uring` (`affinity,io-uring`) and `sqlite`
   (`affinity,sqlite`). Each A/B takes both arms **from one binary**, so the flag is the only
   variable: K and U are the `uring` build without and with `--transport uring`; the `turn`/`density`
   cases `…, kernel` and `…, uring` live in the `uring` bench binary, paired by suffix; the store
   pair is the `sqlite` build with `--journal file-async` and `--journal sqlite-async` — **one
   binary, the two arms differing by exactly the journal flag**; the store plan's step 4b, which
   names a separate control binary for `file-async`, is read through this decision for this boot.
   `BOOT_ROOT=../fb-p4-boot scripts/boot-p4.sh build` makes both worktrees, then sets
   `cap_net_raw,cap_net_admin+ep` on both `w2w` with `sudo -n /usr/sbin/setcap` and reads it back
   with `getcap` — the NIC tap opens `AF_PACKET`, a fresh build carries no file capability, and the
   capability is an extended attribute that does not change the sha256 and is lost on a rebuild —
   then writes `MANIFEST.txt` (sha256 of both `w2w` and both bench binaries) and `BUILD-INFO.txt`
   (commit, rustflags, rustc, bench paths, and the Mac's `mac_head` and `mac_w2w_sha256`). The
   Mac's `w2w` is built by hand at the same commit, before `build`, with the commands in the
   script's header. No binary that needs the capability is built or run from a `nosuid` mount
   (`/tmp`, the scratchpad): the kernel ignores file capabilities there although `getcap` shows
   them. The build commit is the merge commit of the preparing PR on `main`: the PR merges with CI
   green first, the binaries are built from that commit, and only then is the handoff (docs only)
   committed. `run` refuses before anything runs (exit 2) on a missing manifest or build record, a
   missing capability or a `nosuid` mount, a build commit that is not on `origin/main` (outside a
   rehearsal), or a timer due within 12 hours plus the boot's length (naming each); it checks every
   binary's sha256 **and** every `w2w`'s capability before **and** after every arm, and stops
   (exit 3) when either changes or when the Mac's HEAD or `w2w` sha256 differs from
   `BUILD-INFO.txt`. A boot that runs to its end with a failed arm exits 1, naming the arms.
2. **A committed driver runs the boot unattended.** `scripts/boot-p4.sh` runs the blocks below in
   order, reads `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` before each block, stops on a red row, and keeps every output under `target/boot-p4-evidence/` (never
   `/tmp`, which is tmpfs on the desk). Its `sudo -n` lines follow ADR-0093. It is rehearsed on the
   desktop line with `RUNS=2` before the reboot, and that rehearsal's output is labelled *not a
   figure*. The manager launches it and makes no tool call until it exits.
3. **The order, and why.** Each procedure holds every arm the two kill lines read, because the
   `io_uring` kill line's clause (b) needs the idle `turn` at N = 16 **in both procedures**.
   Procedure 1: the `io_uring` w2w block (K, U, U′, S in `hft` with acceptor stamps, then `standard`
   Mac-side), the `turn`/`density` bench from the `uring` binary pinned to the engine core, then the
   store pair (`hft:app` with acceptor stamps, `standard:app` Mac-side). At least thirty minutes pass
   (ADR-0068 decision 1), checked by the driver against the clock, with the machine idle. Procedure
   2 runs the same blocks in reverse order, and the arms inside each block in reverse order — so no
   arm always runs first.
4. **Onload is not on the desk during the boot.** It was uninstalled after the probe
   (`onload_uninstall`, plan step 6.6a); the boot's first action checks that no `onload` or
   `sfc_resource` module is loaded and that `/etc/modprobe.d/onload.conf` is gone. A kernel carrying
   foreign modules is a different machine for every figure taken on it.
5. **The reboot is the handoff.** Before it: the preparing PR is merged with its CI run id named,
   `STATUS.md` *Start here* names the boot's first action and a do-not list, the §9 grub line
   (`/etc/default/grub.fixbolt-s9-bootf-20260923` — never `grub.fixbolt-s9`, which carries
   `nohz_full`) is installed with `update-grub`, and the owner is told the reboot count: one.

## Consequences

**Good**

- One reboot for two items (`io_uring` and the store pair); every arm shares the boot's machine state.
- The boot has no live decision: every block runs in both procedures, in an order written here.
  *(Revised 2026-09-24: before the drop, procedure 2's Onload block was the one live decision.)*
- The desk comes out of the phase with the kernel it went in with.

**Bad — and accepted**

- **A long boot.** Two procedures, each with the `io_uring` arms, a bench run and the store pair, is
  several hours on the desk, unattended; a failure early in the night costs the rest.
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
