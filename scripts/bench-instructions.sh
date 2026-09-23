#!/usr/bin/env bash
# ADR-0102 decision 1: the retired-instruction count is the layout-free
# companion of a timed bench case. This script runs two bench binaries `n`
# times each, interleaved (A, B, A, B, ...) and pinned to one core, under
# `perf stat -e instructions:u,cycles:u`, and prints one verdict:
#
#   same-work     |I_B - I_A| / I_A <= 0.1%   AND each arm's own spread <= 0.01%
#   work-changed  |I_B - I_A| / I_A >  0.1%
#   unstable      an arm's own spread  >  0.01% (the count is not reading one
#                 program, so the between-arm comparison above is not trusted)
#
# I_A and I_B are each arm's MINIMUM instructions:u across its n runs — the
# harness's own best-of-N convention (crates/codec/benches/harness.rs,
# Suite::bench), least sensitive to a stray preemption.
#
# 0.1% is not derived, ADR-0102 decision 1 says why: ~3x the between-arm
# noise of a harness edit that changed no case's work (fact 4), far below the
# ~2-3% a +10% move of one case would add to a whole-process count.
#
# Usage: scripts/bench-instructions.sh [-n N] [-c CPU] A B
#   -n N   runs per arm, interleaved A,B,A,B,... (default 3)
#   -c CPU taskset core to pin both arms to (default 0)
#
# Reads perf as ${PERF:-perf} and holds no `sudo` of its own -- ADR-0093's
# gate reads every committed `sudo`; the caller passes PERF="sudo -n perf"
# when kernel.perf_event_paranoid requires it for an unprivileged process.
#
# Refuses (exit 2) when a counter is missing, zero, or `<not counted>`, or
# when a workload exits non-zero -- perf-record-exits-zero-when-sudo-cannot-
# find-the-workload.md has a `perf stat` twin, and a silent zero would read
# as "no work" instead of "did not run".
#
# Exit codes: 0 same-work, 3 work-changed, 2 unstable or any hard error
# (missing/zero/<not counted> counter, workload exit != 0, bad usage).
#
# The verdict math (spread_pct, between_pct, classify) is pure and sourced
# with BENCH_INSTRUCTIONS_SOURCE_ONLY=1 by
# scripts/check-bench-instructions.sh -- same shape as
# scripts/compare-w2w-procedures.sh (COMPARE_SOURCE_ONLY=1).
set -uo pipefail

# Not derived -- see the header comment above (ADR-0102 decision 1).
SAME_WORK_THRESHOLD_PCT=0.1
UNSTABLE_SPREAD_PCT=0.01

# (max - min) / min, as a percentage. Pure.
spread_pct() { # spread_pct <min> <max>
  awk -v mn="$1" -v mx="$2" '
    BEGIN {
      if (mn == 0) { print "inf"; exit }
      printf "%.6f", ((mx - mn) / mn) * 100
    }'
}

# |B - A| / A, as a percentage -- asymmetric, relative to A. This is
# ADR-0102 decision 1's own formula and is deliberately not
# compare-w2w-procedures.sh's diff_pct (relative to the smaller value): a
# different question, a different file. Pure.
between_pct() { # between_pct <I_A> <I_B>
  awk -v a="$1" -v b="$2" '
    BEGIN {
      if (a == 0) { print "inf"; exit }
      d = b - a
      if (d < 0) d = -d
      printf "%.6f", (d / a) * 100
    }'
}

# classify <spread_a_pct> <spread_b_pct> <between_pct> -> one of
# same-work / work-changed / unstable. Pure.
classify() {
  awk -v sa="$1" -v sb="$2" -v bt="$3" \
      -v uns="$UNSTABLE_SPREAD_PCT" -v same="$SAME_WORK_THRESHOLD_PCT" '
    BEGIN {
      if (sa + 0 > uns + 0 || sb + 0 > uns + 0) { print "unstable"; exit }
      if (bt + 0 > same + 0) { print "work-changed"; exit }
      print "same-work"
    }'
}

# Sourced by check-bench-instructions.sh's pure-function cases, which want
# spread_pct / between_pct / classify and none of the CLI or perf-running
# below -- same guard shape as compare-w2w-procedures.sh:59.
if [ "${BENCH_INSTRUCTIONS_SOURCE_ONLY:-0}" = 1 ]; then
  # shellcheck disable=SC2317 # reachable when sourced
  return 0 2>/dev/null || exit 0
fi

usage() { echo "usage: $(basename "$0") [-n N] [-c CPU] A B" >&2; exit 2; }

N=3
CPU=0
while getopts ":n:c:" opt; do
  case "$opt" in
    n) N=$OPTARG ;;
    c) CPU=$OPTARG ;;
    *) usage ;;
  esac
done
shift $((OPTIND - 1))

[[ "$N" =~ ^[0-9]+$ ]] && [ "$N" -ge 1 ] || usage
[[ "$CPU" =~ ^[0-9]+$ ]] || usage
[ $# -eq 2 ] || usage
A=$1
B=$2

for f in "$A" "$B"; do
  [ -e "$f" ] || { echo "cannot find binary: $f" >&2; exit 2; }
done

PERF_BIN=${PERF:-perf}
command -v taskset >/dev/null 2>&1 || { echo "taskset not found (util-linux)" >&2; exit 2; }

# One measured run: prints "<instructions> <cycles>" on stdout on success, or
# writes a diagnostic to stderr and returns non-zero. The workload's own
# stdout is discarded here -- this script counts instructions, it does not
# read a case's ns/op (that is P1's stdout capture, done by hand).
run_one() { # run_one <bin>
  local bin=$1 out rc instr cycles v
  # $PERF_BIN is deliberately unquoted: the caller passes PERF="sudo -n perf"
  # as a space-separated command, not a single executable name (the same
  # shape scripts/check-sudo-names-what-root-can-find.sh reads off ADR-0093's
  # committed `sudo` lines), so it must word-split here.
  # shellcheck disable=SC2086
  out=$($PERF_BIN stat -x, -e instructions:u,cycles:u -- taskset -c "$CPU" "$bin" 2>&1 1>/dev/null)
  rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "workload exited $rc: $bin" >&2
    printf '%s\n' "$out" >&2
    return 1
  fi
  instr=$(printf '%s\n' "$out" | awk -F, '$3 == "instructions:u" {print $1}' | tail -n1)
  cycles=$(printf '%s\n' "$out" | awk -F, '$3 == "cycles:u" {print $1}' | tail -n1)
  for v in "$instr" "$cycles"; do
    case "$v" in
      '' | *'<not counted>'* | 0)
        echo "missing or zero counter for $bin:" >&2
        printf '%s\n' "$out" >&2
        return 1
        ;;
    esac
  done
  echo "$instr $cycles"
}

min_max() { # min_max <values...> -> "min max" on stdout
  printf '%s\n' "$@" | sort -n | awk 'NR == 1 { min = $1 } { max = $1 } END { print min, max }'
}

instrs_a=()
instrs_b=()
echo "interleaving $N pairs: A=$A  B=$B  cpu=$CPU  perf=\"$PERF_BIN\""
i=1
while [ "$i" -le "$N" ]; do
  line_a=$(run_one "$A") || exit 2
  instrs_a+=("${line_a%% *}")
  line_b=$(run_one "$B") || exit 2
  instrs_b+=("${line_b%% *}")
  echo "  pair $i: A instructions:u=${line_a%% *}   B instructions:u=${line_b%% *}"
  i=$((i + 1))
done

read -r min_a max_a <<<"$(min_max "${instrs_a[@]}")"
read -r min_b max_b <<<"$(min_max "${instrs_b[@]}")"

spread_a=$(spread_pct "$min_a" "$max_a")
spread_b=$(spread_pct "$min_b" "$max_b")
between=$(between_pct "$min_a" "$min_b")
verdict=$(classify "$spread_a" "$spread_b" "$between")

echo
echo "A ($A): min=$min_a max=$max_a spread=${spread_a}%"
echo "B ($B): min=$min_b max=$max_b spread=${spread_b}%"
echo "between |I_B - I_A| / I_A: ${between}%"
echo "verdict: $verdict"

case "$verdict" in
  same-work) exit 0 ;;
  work-changed) exit 3 ;;
  unstable) exit 2 ;;
esac
