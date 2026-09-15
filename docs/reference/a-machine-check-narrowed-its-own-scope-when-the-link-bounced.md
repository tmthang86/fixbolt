# A machine check narrowed its own scope when the link bounced

> `[measured 2026-09-14]` — boot B, step B0 of
> [plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md), and
> that plan's *Sửa 3*, Điều 4. **`[to testing-skills]`**

## What happened

On the §9 desktop, `enp9s0` (Intel I211, `igb`) cabled straight to a second machine. Read off
`sudo journalctl _COMM=sudo`, `dmesg` and the commands' own output:

| Time | Event |
|---|---|
| 21:28:49.24 | `ethtool -C enp9s0 rx-usecs 0` — **no link change** |
| 21:28:58.387 | `ethtool --set-eee enp9s0 eee off`, run by another session on the owner's instruction — `igb` reinitialises the device |
| 21:28:58 | `scripts/check-machine.sh`, no `FIXBOLT_NIC` → `pass 12 fail 0 unknown 1`, **`§9 satisfied`** |
| 21:29:01 | `dmesg`: `igb … enp9s0: NIC Link is Up 1000 Mbps Full Duplex` |
| a few seconds later | the same command → `pass 15 fail 0 unknown 0` |

The first account, written in the plan's delivery log at 21:31, blamed `ethtool -C`. The sudo
journal refuted it the same evening; the log entry is corrected in place with the wrong sentence
struck rather than deleted.

## Why

Three things lined up, none of them wrong on its own:

- **The NIC was selected by carrier.** With no `FIXBOLT_NIC`, the script took the first interface
  that had a link. For about four seconds nothing had one, so no NIC was selected.
- **The NIC rows exist only when a NIC is selected.** The IRQ affinity row fell back to counting
  lines of `/proc/interrupts` and printed UNKNOWN; the coalescing and `irqbalance` rows were not
  printed at all. Thirteen rows instead of fifteen.
- **`unknown ≤ 1` is satisfied.** So the narrower verdict was still green, and the output said
  nothing about having looked at less.

On `igb`, `--set-eee` ends in `igb_reinit_locked` — the device goes down and comes back up, which
is the four seconds above. `ethtool -A` (pause frames) takes the same kind of path in the driver
source; that one was **not** observed here. `ethtool -C` does not reset the link, and at 21:28:49
it did not.

## The rule

**The scope of a verdict must not depend on something that flickers, and whatever a check did not
look at must say so on a line of its own.** A check that silently judges less when an input is
momentarily absent reports a narrower green as the same green.

Two operating corollaries, both in the plan's runbook since B0:

- after any command that can drop a link (`--set-eee`, `-A`, a cable), wait for `ethtool <nic>` to
  read `Link detected: yes` before a machine check or a discarded run;
- every measuring step names its NIC with `FIXBOLT_NIC`, rather than trusting auto-selection.

## Regression guard

`scripts/check-machine-verdicts.sh`, section `=== pick_nic`, which CI runs. The selection moved
into a pure function, `pick_nic <net-root> <explicit>` in `scripts/check-machine.sh`, defined above
the `MACHINE_SOURCE_ONLY` return. It keeps only physical wired devices — `type` reads 1, a
`device` link exists, no `wireless` and no `phy80211` — and uses carrier **only** to break a tie
between two of them. The machine header now prints the choice on its own line:
`nic       enp9s0 (carrier 1)`, or `nic       none — …` when there is no wired NIC.

The five cases, on a `mktemp` fake of `/sys/class/net`: a wired NIC without carrier is still
picked; wireless is never picked; virtual interfaces (`tailscale0`, a device-less `docker0`, `lo`)
are never picked; carrier breaks a tie and only a tie; `FIXBOLT_NIC` wins without a carrier.

Proven by reversal, 2026-09-14, in a worktree copy: requiring `carrier = 1` again turned case 1
red on its sentence —
`FAIL  want [enp9s0] got []  a wired NIC without carrier is still picked — the §9 rows do not need a cable`,
`pass 31 fail 1` — and restoring it read `pass 32 fail 0`.

**Not guarded:** a NIC that loses its link *during* a measurement. The rows are judged once, before
the runs; a figure taken across a bounce is caught only where a procedure checks for it on its own
(the wire-timestamp arms fail a run with a missing stamp).
