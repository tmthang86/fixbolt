#!/usr/bin/env bash
# ADR-0068 decision 2's comparison, done by machine instead of by hand
# (item 85): two `scripts/w2w-baseline.sh` summaries of the *same* arms, and
# for each arm present in both, whether p50, p99 and p99.9 agree within 5% of
# the smaller value — "reproduced" per ADR-0068 decision 2 — or not.
#
# The comparison itself (`diff_pct`, `pct_verdict`) is pure: two numbers and
# a threshold in, a percentage and a word out. It is sourced with
# COMPARE_SOURCE_ONLY=1 by scripts/check-w2w-compare.sh, the same shape
# scripts/check-w2w-baseline-summary.sh uses on w2w-baseline.sh
# (BASELINE_SOURCE_ONLY=1) and check-machine-verdicts.sh uses on
# check-machine.sh.
#
# Usage: scripts/compare-w2w-procedures.sh <summary1> <summary2> [threshold%]
#
# Each summary is a scripts/w2w-baseline.sh summary.txt: blocks that open
# with a line `  == <mode> / <path> / <tls>[/<interval>]: median of N
# qualifying runs ... ==` followed by plain `p50`, `p99`, `p99.9` lines (the
# `dispersion p50 ...` lines are a different first field and are not read
# here).
set -uo pipefail

# Percentage difference between two numbers, relative to the SMALLER of the
# two — the denominator ADR-0068 decision 2 names, not the mean and not
# always the first argument. Pure.
diff_pct() { # diff_pct <a> <b>
  awk -v a="$1" -v b="$2" '
    BEGIN {
      if (a < b) { small = a; big = b } else { small = b; big = a }
      if (small == 0) { print "inf"; exit }
      printf "%.3f", ((big - small) / small) * 100
    }'
}

# reproduced when the difference is at most the threshold (ADR-0068 decision
# 2: "within 5% at every published percentile"). Pure.
pct_verdict() { # pct_verdict <diff_pct> <threshold>
  awk -v d="$1" -v t="$2" 'BEGIN { print (d + 0 <= t + 0) ? "reproduced" : "not reproduced" }'
}

# One percentile line: both values, the diff, the verdict. Prints, and its
# exit status is the verdict (0 reproduced, 1 not) so a caller can fold it
# into the arm's overall verdict without re-parsing its own printf. Pure
# apart from that print.
compare_one() { # compare_one <label> <v1> <v2> <threshold>
  local label=$1 v1=$2 v2=$3 threshold=$4 d v
  d=$(diff_pct "$v1" "$v2")
  v=$(pct_verdict "$d" "$threshold")
  printf '    %-6s %10s ns   %10s ns   diff %6s%%   %s\n' "$label" "$v1" "$v2" "$d" "$v"
  [ "$v" = "reproduced" ]
}

# Sourced by the pure-function test, which wants diff_pct / pct_verdict /
# compare_one and none of the file parsing or the CLI below — same guard
# shape as w2w-baseline.sh:214 (BASELINE_SOURCE_ONLY).
if [ "${COMPARE_SOURCE_ONLY:-0}" = 1 ]; then
  # shellcheck disable=SC2317 # reachable when sourced
  return 0 2>/dev/null || exit 0
fi

usage() { echo "usage: $(basename "$0") <summary1> <summary2> [threshold%]" >&2; exit 2; }

[ $# -ge 2 ] || usage
SUMMARY1=$1
SUMMARY2=$2
THRESHOLD=${3:-5}

[ -r "$SUMMARY1" ] || { echo "cannot read $SUMMARY1" >&2; exit 2; }
[ -r "$SUMMARY2" ] || { echo "cannot read $SUMMARY2" >&2; exit 2; }

# Pull "<arm> <percentile> <value>" triples out of one summary. The arm key
# is "<mode>/<path>/<tls>[/<interval>]" with the " / " separators collapsed,
# so it can be used as an associative-array key with no embedded spaces.
extract() { # extract <file>
  awk '
    $1 == "==" {
      line = $0
      sub(/^ *== */, "", line)
      sub(/:.*/, "", line)
      gsub(/ \/ /, "/", line)
      gsub(/ /, "", line)
      arm = line
      next
    }
    arm != "" && ($1 == "p50" || $1 == "p99" || $1 == "p99.9") {
      print arm, $1, $2
    }
  ' "$1"
}

declare -A V1 V2
declare -A ARMS1 ARMS2
while read -r arm pct val; do
  V1["$arm|$pct"]=$val
  ARMS1["$arm"]=1
done < <(extract "$SUMMARY1")
while read -r arm pct val; do
  V2["$arm|$pct"]=$val
  ARMS2["$arm"]=1
done < <(extract "$SUMMARY2")

reproduced=0
not_reproduced=0
unpaired=()

# Stable order: every arm from summary 1 first (in the order it appears),
# then any arm summary 2 has that summary 1 does not.
mapfile -t order1 < <(extract "$SUMMARY1" | awk '!seen[$1]++ {print $1}')
mapfile -t order2 < <(extract "$SUMMARY2" | awk '!seen[$1]++ {print $1}')
declare -A printed
all_arms=()
for a in "${order1[@]}" "${order2[@]}"; do
  [ -n "${printed[$a]:-}" ] && continue
  printed[$a]=1
  all_arms+=("$a")
done

for arm in "${all_arms[@]}"; do
  if [ -z "${ARMS1[$arm]:-}" ] || [ -z "${ARMS2[$arm]:-}" ]; then
    unpaired+=("$arm")
    continue
  fi
  echo "== $arm =="
  arm_ok=1
  for pct in p50 p99 p99.9; do
    v1=${V1["$arm|$pct"]:-}
    v2=${V2["$arm|$pct"]:-}
    if [ -z "$v1" ] || [ -z "$v2" ]; then
      printf '    %-6s missing in one summary\n' "$pct"
      arm_ok=0
      continue
    fi
    if ! compare_one "$pct" "$v1" "$v2" "$THRESHOLD"; then
      arm_ok=0
    fi
  done
  if [ "$arm_ok" = 1 ]; then
    reproduced=$((reproduced + 1))
  else
    not_reproduced=$((not_reproduced + 1))
  fi
  echo
done

if [ ${#unpaired[@]} -gt 0 ]; then
  echo "unpaired (present in only one summary):"
  for arm in "${unpaired[@]}"; do
    echo "    $arm"
  done
  echo
fi

echo "arms $((reproduced + not_reproduced)) reproduced $reproduced not-reproduced $not_reproduced"
[ "$not_reproduced" -eq 0 ]
