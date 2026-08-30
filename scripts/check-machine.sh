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

# --- the machine is quiet -----------------------------------------------------
# `[measured 2026-08-30]` This row exists because the rows above were measuring the
# wrong thing. On this project's own desktop the five tuning rows move the ring
# benchmark's median by 0.8%; competing CPU load moves it by 71% — 262 ns to 449 ns
# — and NOTHING in §9 looked at load. The box scored `pass 6` while running an LLM,
# an editor and two Electron apps. A tuned machine that is busy is not a machine you
# can take a latency number from, and until this row existed the script could not say
# so. Guarded by reversal: with eight spinners running it must FAIL.
#
# CPU time over a real window, not `loadavg` — loadavg is a one-minute average and
# says nothing about the second the benchmark ran in.
#
# Both the total AND the per-process attribution are deltas over that window.
# `[measured 2026-08-30]` `ps -eo pcpu` was used here first and named the wrong
# processes: **`%CPU` from `ps` is an average over the process's whole lifetime**,
# not what it is doing now. It reported an LLM at 19% on a machine /proc/stat
# measured as 1% busy, and that number reached the owner as "the machine is loaded"
# before the two were put side by side. An instrument that answers a question
# adjacent to the one asked is the failure this repository keeps finding.
QUIET_WINDOW=${QUIET_WINDOW:-1}
busy_pct=""
# One awk pass over every /proc/<pid>/stat. Done in shell it took seconds, which
# made the sampling window longer than QUIET_WINDOW and reported a single-threaded
# process at 310% of a core — a reading that is impossible on its face, and the
# only reason it was caught.
snap() {
  head -1 /proc/stat
  awk 'FNR==1 {
         a = index($0, "(")
         b = 0; for (i = length($0); i > 0; i--) if (substr($0, i, 1) == ")") { b = i; break }
         if (a == 0 || b == 0) next
         pid  = substr($0, 1, a - 2)
         comm = substr($0, a + 1, b - a - 1)
         n = split(substr($0, b + 2), f, " ")      # f[1] is state, so utime=f[12]
         if (n >= 13) print "p", pid, f[12] + f[13], comm
       }' /proc/[0-9]*/stat 2>/dev/null
}
if [ -r /proc/stat ]; then
  A=$(snap); sleep "$QUIET_WINDOW"; B=$(snap)
  # shellcheck disable=SC2046
  set -- $(echo "$A" | head -1); shift
  a_idle=$(($4 + $5)); a_tot=0; for v in "$@"; do a_tot=$((a_tot + v)); done
  # shellcheck disable=SC2046
  set -- $(echo "$B" | head -1); shift
  b_idle=$(($4 + $5)); b_tot=0; for v in "$@"; do b_tot=$((b_tot + v)); done
  d_tot=$((b_tot - a_tot)); d_idle=$((b_idle - a_idle))
  [ "$d_tot" -gt 0 ] && busy_pct=$(((d_tot - d_idle) * 100 / d_tot))
fi

if [ -z "$busy_pct" ]; then
  row UNKNOWN "machine is quiet" "cannot read /proc/stat" \
    "a latency number needs a quiet machine; find another way to confirm it"
elif [ "$busy_pct" -le 3 ]; then
  row PASS "machine is quiet" "${busy_pct}% CPU busy over ${QUIET_WINDOW}s"
else
  top=$(
    { echo "$A" | awk '$1=="p"{print "a", $2, $3, $4}'
      echo "$B" | awk '$1=="p"{print "b", $2, $3, $4}'; } |
    awk '$1=="a"{was[$2]=$3; nm[$2]=$4}
         $1=="b" && ($2 in was){d=$3-was[$2]; if(d>0) print d, nm[$2]}' |
    sort -rn | head -3 |
    awk -v tot="$d_tot" -v ncpu="$(nproc 2>/dev/null || echo 1)" \
      'tot > 0 {printf "%s %d%% of a core  ", $2, $1 * 100 * ncpu / tot}'
  )
  row FAIL "machine is quiet" "${busy_pct}% CPU busy over ${QUIET_WINDOW}s — ${top:-unattributed}" \
    "close what is running; competing load moved this project's ring median 71%, against 0.8% for every tuning row combined"
fi

# --- kTLS ---------------------------------------------------------------------
# STATUS.md open item 10. This is a kernel feature, not a latency property, so it
# is reported separately from the tuning rows above.
# `[measured 2026-08-30]` this row used `lsmod | grep -q '^tls'`, and under this
# script's own `set -o pipefail` that branch can never be taken: grep -q exits on
# the first match, lsmod dies of SIGPIPE with status 141, and the pipeline reports
# failure exactly when the module IS loaded. The `|| modinfo` fallback hid it here
# — but on a kernel with CONFIG_TLS=y the module is built in, `modinfo` has
# nothing to find, and this row would have reported "no tls module" on the machine
# best equipped to run kTLS. /sys/module/tls covers loaded and built-in alike.
# Guarded by scripts/check-ktls-classify.sh, which holds the same rule.
if [ -d /sys/module/tls ]; then
  row PASS "kTLS (CONFIG_TLS)" "tls module loaded — open item 10 is unblocked here"
elif modinfo tls >/dev/null 2>&1; then
  row PASS "kTLS (CONFIG_TLS)" "tls module on disk, not loaded — 'sudo modprobe tls' away"
else
  row FAIL "kTLS (CONFIG_TLS)" "no tls module, loaded or on disk" \
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
