#!/usr/bin/env bash
# What DESIGN.md §9's isolated core costs, and WHERE it costs it.
#
# `docs/reference/measured-costs.md` measured the isolated core at +36% on
# `Engine::turn` and could not say which of `isolcpus`, `nohz_full` and
# `rcu_nocbs` was responsible: one kernel command line applied all three to the
# same CPUs. This runs `measure-isolation-cost.c` on each core given and prints,
# beside every reading, the isolation state THAT CORE ACTUALLY HAS — read from
# sysfs and /proc/cmdline, never from the argument it was called with.
#
# Reading the two loops:
#
#   user_loop differs between cores    -> the cores are not running at the same
#                                         speed; nothing else in the run may be
#                                         compared, and the reading is discarded
#   user_loop equal, syscall_loop up   -> the cost is kernel entry and exit
#   both up by the same ratio          -> the cost is the clock
#
# The `LOC` column is the local timer tick, sampled across each core's own run.
# It is how a `nohz_full` core that did NOT stop its tick is told apart from one
# that did: single digits mean the tick is off, thousands mean it is on. Without
# it, a `nohz_full` arm that quietly failed to engage reads as "isolation is
# free" — the false green this script exists to make visible.
#
# It READS ONLY and needs no privilege. Changing the kernel command line is root,
# is a reboot, and belongs to the person at the machine.
#
# Usage: scripts/measure-isolation-cost.sh [core ...]     (default: 4 5 6 7)
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/scripts/measure-isolation-cost.c"
BIN="$(mktemp -t measure-isolation-cost.XXXXXX)"
trap 'rm -f "$BIN"' EXIT

CORES=("${@:-}")
[ -z "${CORES[0]:-}" ] && CORES=(4 5 6 7)

command -v taskset >/dev/null || { echo "taskset not found (util-linux)"; exit 2; }
cc -O2 -o "$BIN" "$SRC" || { echo "could not build $SRC"; exit 2; }

r() { cat "$1" 2>/dev/null; }

# in_list <cpu> <kernel cpu list, e.g. "6-7,14-15">  — expands ranges.
in_list() {
  local cpu="$1" spec="${2:-}" part lo hi i
  [ -z "$spec" ] && return 1
  IFS=',' read -ra parts <<<"$spec"
  for part in "${parts[@]}"; do
    case "$part" in
      *-*) lo="${part%-*}"; hi="${part#*-}"
           for ((i = lo; i <= hi; i++)); do [ "$i" = "$cpu" ] && return 0; done ;;
      *)   [ "$part" = "$cpu" ] && return 0 ;;
    esac
  done
  return 1
}

# The value of a kernel command line parameter, or empty.
param() { sed -n "s/.*\b$1=\([^ ]*\).*/\1/p" /proc/cmdline; }

# The LOC (local timer interrupt) count for one cpu.
#
# `$1 == "LOC:"`, NOT `/^LOC:/`: /proc/interrupts right-aligns its first column,
# so the line begins with a space and the anchored pattern matches nothing. It
# printed a delta of 0 for every core, on a boot where two cores were ticking
# three million times and two were not — a guard against a false green that was
# itself a false green, and it read as the reassuring answer.
loc_of() {
  awk -v c="$1" '$1 == "LOC:" { print $(c + 2) }' /proc/interrupts
}

ISOLATED="$(r /sys/devices/system/cpu/isolated)"
NOHZ="$(r /sys/devices/system/cpu/nohz_full)"
NOCBS="$(param rcu_nocbs)"

echo "cmdline   $(cat /proc/cmdline)"
echo "isolated  ${ISOLATED:-<none>}"
echo "nohz_full ${NOHZ:-<none>}"
echo "rcu_nocbs ${NOCBS:-<none>}  (from the command line; nohz_full implies it too)"
echo "smt       $(r /sys/devices/system/cpu/smt/control)"
echo "online    $(r /sys/devices/system/cpu/online)"
echo

printf "%-6s %-9s %-10s %-10s %-12s %s\n" \
       core isolcpus nohz_full rcu_nocbs "ticks(LOC)" "measurement"
for c in "${CORES[@]}"; do
  in_list "$c" "$ISOLATED" && iso=yes || iso=no
  in_list "$c" "$NOHZ"     && nz=yes  || nz=no
  # nohz_full implies rcu_nocbs whether or not the command line says so.
  if in_list "$c" "$NOCBS" || [ "$nz" = yes ]; then nocb=yes; else nocb=no; fi

  before="$(loc_of "$c")"
  out="$(taskset -c "$c" "$BIN")"
  after="$(loc_of "$c")"
  ticks=$(( ${after:-0} - ${before:-0} ))

  printf "cpu%-3s %-9s %-10s %-10s %-12s %s\n" "$c" "$iso" "$nz" "$nocb" "$ticks" "$out"
done

echo
echo "user_loop must agree across every core above. Where it does not, that core's"
echo "syscall_loop says nothing about isolation — CLAUDE.md §10."
