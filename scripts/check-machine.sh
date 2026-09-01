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

# virt_verdict <systemd-detect-virt output> <steal % over the window>
#
# Why this row exists: a guest CANNOT satisfy §9, and it does not fail loudly — it
# fails by the files simply not being there. `governor`, `turbo`, `C-states`, `SMT`
# and NIC IRQ affinity are all HOST properties; a guest that sets its own THP and
# busy_poll can collect `unknown` on the rest and look merely under-configured
# rather than structurally unable. `unknown` is already not a pass, so the script
# does not lie — but it does not say the one thing the reader needs, which is that
# no amount of configuration inside this machine will fix those rows.
#
# Split out so scripts/check-machine-verdicts.sh can exercise it: this repository
# has no VM to run against, and a row that cannot be tested where it matters is how
# scripts/check-ktls-available.sh shipped a wrong answer for a day.
virt_verdict() {
  local virt="$1" steal="$2"
  case "$virt" in
    none)
      if [ "${steal:-0}" -gt 0 ] 2>/dev/null; then
        echo "STEAL_ON_METAL"
        echo "  Bare metal reporting ${steal}% steal. Nothing here explains that;"
        echo "  find out why before publishing a number from this machine."
        return 1
      fi
      echo "BARE_METAL"
      echo "  Bare metal, no steal. §9's host-level rows are settable here."
      return 0
      ;;
    ""|unknown)
      echo "UNKNOWN"
      echo "  Cannot tell whether this is a guest. Treat every §9 row below as"
      echo "  unconfirmed — CLAUDE.md §2 non-negotiable 10."
      return 1
      ;;
    *)
      echo "GUEST"
      echo "  Running under '${virt}'. governor, turbo, C-states, SMT and NIC IRQ"
      echo "  affinity are HOST properties: no configuration inside this machine"
      echo "  can set them, so §9 cannot be satisfied here and no latency figure"
      echo "  from it is publishable. Steal over the window: ${steal:-unknown}%."
      echo "  Use bare metal, or publish nothing but counts and same-machine A/B."
      return 1
      ;;
  esac
}

# Sourced by the verdict test, which wants the functions and none of the probing.
if [ "${MACHINE_SOURCE_ONLY:-0}" = 1 ]; then
  return 0 2>/dev/null || exit 0
fi

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

# --- isolcpus + rcu_nocbs -----------------------------------------------------
#
# This row used to demand `nohz_full` as well, and ADR-0021 reversed that.
# [measured 2026-08-31] `nohz_full` is the whole of the 36% the isolated core was
# costing: 670.7 ns per `Engine::turn` against 494.8 on an `isolcpus`-only core,
# because full dynticks runs context tracking on every kernel entry and this
# engine is nothing but kernel entries. `isolcpus` and `rcu_nocbs` are free.
#
# So this gate now FAILS a machine for HAVING nohz_full, where it used to fail
# one for lacking it. That reversal is the point and not an accident: the
# baselines in benches/baselines.tsv were recorded without it, and a machine
# carrying it reads 35% over on all four `turn` cases.
iso=$(echo "$CMDLINE" | tr ' ' '\n' | grep '^isolcpus=' || true)
nocb=$(echo "$CMDLINE" | tr ' ' '\n' | grep '^rcu_nocbs=' || true)
nohz=$(echo "$CMDLINE" | tr ' ' '\n' | grep '^nohz_full=' || true)
if [ -z "$CMDLINE" ]; then
  row UNKNOWN "isolcpus + rcu_nocbs" "/proc/cmdline not readable" \
    "run outside a restricted container"
elif [ -n "$iso" ] && [ -n "$nocb" ]; then
  row PASS "isolcpus + rcu_nocbs" "$iso $nocb"
else
  row FAIL "isolcpus + rcu_nocbs" "${iso:-no isolcpus}${nocb:+ $nocb}" \
    "add 'isolcpus=N rcu_nocbs=N' to the kernel command line, then reboot"
fi

# --- nohz_full, which §9 no longer asks for -----------------------------------
if [ -z "$CMDLINE" ]; then
  : # already reported unknown above; one unreadable file is one row
elif [ -z "$nohz" ]; then
  row PASS "no nohz_full" "absent — ADR-0021"
else
  row FAIL "no nohz_full" "$nohz" \
    "REMOVE nohz_full from the kernel command line: it adds 160 ns to every kernel entry (+36% on Engine::turn) and is behind at p50, p99 AND p99.9 — see ADR-0021"
fi

# --- CPU speculation mitigations ----------------------------------------------
#
# ADR-0023. [measured 2026-09-01] disabling these makes every syscall this
# engine performs 59-63% cheaper: `engine turn, 1 idle sessions` goes from
# 448.9 ns to 175.2, while thirteen pure user-space benchmarks move -4.1% to
# +4.1% with no direction. All of it is `retbleed`s untrained return thunk plus
# `spec_rstack_overflow`s Safe RET; `vmscape` — the mechanism STATUS.md had
# named for two days — costs nothing.
#
# This row PASSES when the machine IS mitigated, which is the default, the safe
# state, and the state `benches/baselines.tsv` was recorded in. It is NOT advice
# to turn them off. A machine with them off reads ~60% UNDER every syscall-bound
# baseline, which passes — a baseline is a ceiling — so the bench gate cannot
# catch it and something else must.
#
# Read from /sys rather than /proc/cmdline: the command line says what was asked
# for and sysfs says what the kernel is doing, and [measured 2026-09-01] they
# differ — `retbleed=off` also removed `STIBP: always-on` from spectre_v2s line,
# which no reading of the command line would show.
vuln_dir=/sys/devices/system/cpu/vulnerabilities
if [ ! -d "$vuln_dir" ]; then
  row UNKNOWN "CPU mitigations" "$vuln_dir not readable" \
    "run outside a restricted container"
else
  off=$(grep -l '^Vulnerable' "$vuln_dir"/* 2>/dev/null | xargs -r -n1 basename | tr '\n' ' ')
  if [ -z "$off" ]; then
    row PASS "CPU mitigations" "all in force"
  else
    row FAIL "CPU mitigations" "disabled: ${off% }" \
      "these are worth 61% of every syscall here (ADR-0023), so numbers from this machine are NOT comparable to benches/baselines.tsv — remove the mitigation overrides from the kernel command line and reboot"
  fi
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
steal_pct=""
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
  a_idle=$(($4 + $5)); a_steal=${8:-0}; a_tot=0; for v in "$@"; do a_tot=$((a_tot + v)); done
  # shellcheck disable=SC2046
  set -- $(echo "$B" | head -1); shift
  b_idle=$(($4 + $5)); b_steal=${8:-0}; b_tot=0; for v in "$@"; do b_tot=$((b_tot + v)); done
  d_tot=$((b_tot - a_tot)); d_idle=$((b_idle - a_idle)); d_steal=$((b_steal - a_steal))
  [ "$d_tot" -gt 0 ] && busy_pct=$(((d_tot - d_idle) * 100 / d_tot))
  [ "$d_tot" -gt 0 ] && steal_pct=$((d_steal * 100 / d_tot))
fi

# --- virtualisation -----------------------------------------------------------
# Reported before the quiet row because it governs it: on a guest, "quiet" can only
# ever mean "quiet inside this VM", and the neighbours are invisible except as steal.
# `systemd-detect-virt` EXITS 1 WHEN THE ANSWER IS "none" — the good case is
# reported as a failure. `[measured 2026-08-30]` written first as
# `$(systemd-detect-virt || echo unknown)`, which on bare metal ran both halves and
# set VIRT to the two lines "none\nunknown", so this machine reported itself a
# guest. The verdict test could not catch it: it feeds virt_verdict directly and
# never sees how the argument is obtained. Running the real script did.
VIRT=$(systemd-detect-virt 2>/dev/null)
[ -z "$VIRT" ] && VIRT=unknown
vv=$(virt_verdict "$VIRT" "${steal_pct:-0}")
case "$(echo "$vv" | head -1)" in
  BARE_METAL) row PASS "not virtualised" "bare metal, ${steal_pct:-0}% steal" ;;
  GUEST) row FAIL "not virtualised" "guest under '${VIRT}', ${steal_pct:-0}% steal" \
    "measure on bare metal; governor, turbo, C-states, SMT and IRQ affinity are host properties" ;;
  STEAL_ON_METAL) row FAIL "not virtualised" "bare metal but ${steal_pct}% steal — unexplained" \
    "find out what is stealing time before publishing a number from this machine" ;;
  *) row UNKNOWN "not virtualised" "cannot tell (systemd-detect-virt unavailable)" \
    "confirm by hand; every row below is unconfirmed until you do" ;;
esac
# The reasoning only earns its space when the answer is not PASS — every other row
# here prints a fix line and nothing else when it passes.
[ "$(echo "$vv" | head -1)" = BARE_METAL ] || echo "$vv" | tail -n +2 | sed 's/^/     /'

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
