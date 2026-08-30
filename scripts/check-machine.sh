#!/usr/bin/env bash
# Read the DESIGN.md §9 checklist off the running machine and say, row by row,
# whether it is actually in force.
#
# CLAUDE.md §2 non-negotiable 10: no performance number without the benchmark,
# the machine, AND the §9 settings in force. Until this script existed, §9 was a
# table of things somebody was supposed to have done — there was no way to tell a
# tuned box from an untuned one except by asking the person who set it up.
#
# It READS ONLY. Applying these is root, machine-specific, and belongs to the
# person sitting at the box; each FAIL prints the command that fixes it.
#
# Exit 1 if any row FAILS, so `scripts/bench.sh --strict` can refuse to publish a
# number from a machine that is not set up. `unknown` is NOT a pass: a container
# that cannot read /sys must not look like a tuned host.
set -uo pipefail

pass=0
fail=0
unknown=0

row() { # row PASS|FAIL|UNKNOWN name value fixcmd
  case "$1" in
    PASS) pass=$((pass + 1)); mark="PASS " ;;
    FAIL) fail=$((fail + 1)); mark="FAIL " ;;
    *) unknown=$((unknown + 1)); mark="? ? ?" ;;
  esac
  printf "%s  %-22s %s\n" "$mark" "$2" "$3"
  if [ "$1" != PASS ] && [ -n "${4:-}" ]; then
    printf "       %-22s fix: %s\n" "" "$4"
  fi
}

# Read a file, or print nothing if it is not there / not readable.
r() { cat "$1" 2>/dev/null; }

echo "=== machine"
uname -srm
if [ -r /proc/cpuinfo ]; then
  echo "cpu       $(grep -m1 '^model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')"
  echo "cores     $(nproc 2>/dev/null || echo unknown)"
else
  echo "cpu       $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown)"
  echo "cores     $(getconf _NPROCESSORS_ONLN 2>/dev/null || echo unknown)"
fi
echo "rustc     $(rustc --version 2>/dev/null || echo 'not on PATH')"
echo

if [ "$(uname -s)" != Linux ]; then
  echo "=== DESIGN.md §9"
  row UNKNOWN "everything" "not Linux — §9 is a Linux checklist" \
    "run this on the Linux box; a number from here is not a §9 number"
  echo
  echo "=== summary"
  echo "pass $pass   fail $fail   unknown $unknown"
  exit 1
fi

CMDLINE=$(r /proc/cmdline)

echo "=== DESIGN.md §9"

# --- isolcpus + nohz_full -----------------------------------------------------
iso=$(echo "$CMDLINE" | tr ' ' '\n' | grep '^isolcpus=' || true)
nohz=$(echo "$CMDLINE" | tr ' ' '\n' | grep '^nohz_full=' || true)
if [ -n "$iso" ] && [ -n "$nohz" ]; then
  row PASS "isolcpus + nohz_full" "$iso $nohz"
elif [ -z "$CMDLINE" ]; then
  row UNKNOWN "isolcpus + nohz_full" "/proc/cmdline not readable" \
    "run outside a restricted container"
else
  row FAIL "isolcpus + nohz_full" "${iso:-no isolcpus}${nohz:+ $nohz}" \
    "add 'isolcpus=N nohz_full=N rcu_nocbs=N' to the kernel command line, then reboot"
fi

# --- CPU frequency governor ---------------------------------------------------
gov=$(r /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
if [ -z "$gov" ]; then
  row UNKNOWN "governor" "cpufreq not exposed" "run on bare metal, not in this container"
elif [ "$gov" = performance ]; then
  row PASS "governor" "$gov"
else
  row FAIL "governor" "$gov" \
    "sudo cpupower frequency-set -g performance   (or write 'performance' to scaling_governor)"
fi

# --- turbo --------------------------------------------------------------------
nt=$(r /sys/devices/system/cpu/intel_pstate/no_turbo)
boost=$(r /sys/devices/system/cpu/cpufreq/boost)
if [ "$nt" = 1 ] || [ "$boost" = 0 ]; then
  row PASS "turbo" "off"
elif [ -z "$nt" ] && [ -z "$boost" ]; then
  row UNKNOWN "turbo" "neither intel_pstate/no_turbo nor cpufreq/boost is readable" \
    "on AMD: echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost"
else
  row FAIL "turbo" "on" \
    "echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo   (Intel) or boost=0 (AMD)"
fi

# --- C-states -----------------------------------------------------------------
# The kernel command line is what actually holds this; per-state disable files
# are the fallback for a box that cannot be rebooted.
if echo "$CMDLINE" | grep -qE 'intel_idle\.max_cstate=0|processor\.max_cstate=[01]|idle=poll'; then
  row PASS "C-states" "capped on the kernel command line"
elif [ -z "$CMDLINE" ]; then
  row UNKNOWN "C-states" "/proc/cmdline not readable" "run outside a restricted container"
else
  row FAIL "C-states" "not capped" \
    "add 'intel_idle.max_cstate=0 processor.max_cstate=1 idle=poll' to the kernel command line"
fi

# --- SMT ----------------------------------------------------------------------
smt=$(r /sys/devices/system/cpu/smt/control)
case "$smt" in
  off | forceoff | notsupported) row PASS "SMT / hyperthreading" "$smt" ;;
  "") row UNKNOWN "SMT / hyperthreading" "not exposed" "check in BIOS" ;;
  *) row FAIL "SMT / hyperthreading" "$smt" "echo off | sudo tee /sys/devices/system/cpu/smt/control" ;;
esac

# --- transparent huge pages ---------------------------------------------------
thp=$(r /sys/kernel/mm/transparent_hugepage/enabled)
if [ -z "$thp" ]; then
  row UNKNOWN "transparent hugepages" "not exposed" "run on bare metal"
elif echo "$thp" | grep -q '\[never\]'; then
  row PASS "transparent hugepages" "never"
else
  row FAIL "transparent hugepages" "$thp" \
    "echo never | sudo tee /sys/kernel/mm/transparent_hugepage/enabled"
fi

# --- busy poll ----------------------------------------------------------------
bp=$(sysctl -n net.core.busy_poll 2>/dev/null)
if [ -z "$bp" ]; then
  row UNKNOWN "net.core.busy_poll" "sysctl unavailable" "run on the host"
elif [ "$bp" -gt 0 ] 2>/dev/null; then
  row PASS "net.core.busy_poll" "$bp"
else
  row FAIL "net.core.busy_poll" "$bp" "sudo sysctl -w net.core.busy_poll=50 net.core.busy_read=50"
fi

# --- kTLS ---------------------------------------------------------------------
# STATUS.md open item 10. This is a kernel feature, not a latency property, so it
# is reported separately from the tuning rows above.
if [ -d /proc/sys/net/ipv4 ] && { lsmod 2>/dev/null | grep -q '^tls' || modinfo tls >/dev/null 2>&1; }; then
  row PASS "kTLS (CONFIG_TLS)" "tls module present — open item 10 is unblocked here"
else
  row FAIL "kTLS (CONFIG_TLS)" "no tls module" \
    "sudo modprobe tls; if that fails the kernel lacks CONFIG_TLS — see scripts/check-ktls-available.sh"
fi

# --- IRQ affinity -------------------------------------------------------------
# Reported, not judged: which core the NIC may interrupt is a decision about
# which core the engine runs on, and this script does not know that.
nic_irqs=$(grep -ciE 'eth|enp|ens|eno|mlx|sfc' /proc/interrupts 2>/dev/null)
: "${nic_irqs:=0}"
row UNKNOWN "NIC IRQ affinity" "$nic_irqs NIC interrupt line(s) — steer them AWAY from the engine core" \
  "see /proc/interrupts, then write a mask to /proc/irq/<n>/smp_affinity"

echo
echo "=== summary"
echo "pass $pass   fail $fail   unknown $unknown"
if [ "$fail" -gt 0 ] || [ "$unknown" -gt 1 ]; then
  echo
  echo "This machine is NOT set up to DESIGN.md §9. Numbers measured here are"
  echo "usable for counts and for A/B comparisons against themselves, and are NOT"
  echo "publishable as latency figures — CLAUDE.md §2 non-negotiable 10."
  exit 1
fi
echo "§9 satisfied. Latency numbers from this machine carry their settings."
