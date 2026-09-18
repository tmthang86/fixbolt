#!/usr/bin/env bash
# Does `scripts/compare-w2w-procedures.sh` implement ADR-0068 decision 2 —
# "reproduced" means within 5% of the smaller value at every published
# percentile — the way the 2026-09-14 trap and the day's own reproducing
# arms actually read?
#
# `diff_pct` / `pct_verdict` are pure (compare-w2w-procedures.sh:20-33);
# fed here with COMPARE_SOURCE_ONLY=1, the way
# scripts/check-w2w-baseline-summary.sh sources w2w-baseline.sh with
# BASELINE_SOURCE_ONLY=1. The threshold in the "B2, 4.7%" case below is the
# number this file's reversal changes (5 -> 4): everything else in this file
# is expected to stay green when that happens.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=compare-w2w-procedures.sh
# shellcheck disable=SC1091 # found at runtime via $here; not followed without -x
COMPARE_SOURCE_ONLY=1 . "$here/compare-w2w-procedures.sh"

pass=0
fail=0

same() { # same <want> <got> <what it means>
  if [[ "$2" == "$1" ]]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$3"
  else
    fail=$((fail + 1)); printf 'FAIL  want [%s] got [%s]  %s\n' "$1" "$2" "$3"
  fi
}

# The threshold ADR-0068 decision 2 sets: 5% of the smaller value. Reversal
# target: change this to 4 and re-run — the "B2, 4.7%" case below must go
# red, on its own verdict assertion, nothing else.
THRESHOLD=5

echo "=== diff_pct"

# The 2026-09-14 trap that ADR-0068 exists to catch: one arm's p50 moved
# 20 774 -> 17 473 ns between two identical procedures (ADR-0068 Context).
same "18.892" "$(diff_pct 20774 17473)" \
  "the trap arm's diff% (20774 vs 17473)"
same "18.892" "$(diff_pct 17473 20774)" \
  "diff_pct is symmetric in its arguments"

# The arms nobody doubted moved 0.2-2.5% at p50 between procedures
# (ADR-0068 decision 2).
same "0.200" "$(diff_pct 10000 10020)" "a 0.2% arm"
same "2.500" "$(diff_pct 10000 10250)" "a 2.5% arm"

echo
echo "=== pct_verdict"

same "not reproduced" "$(pct_verdict "$(diff_pct 20774 17473)" "$THRESHOLD")" \
  "the trap arm does not reproduce at threshold $THRESHOLD"
same "reproduced" "$(pct_verdict "$(diff_pct 10000 10020)" "$THRESHOLD")" \
  "a 0.2% arm reproduces"
same "reproduced" "$(pct_verdict "$(diff_pct 10000 10250)" "$THRESHOLD")" \
  "a 2.5% arm reproduces"

# "B2, 4.7%": the reversal target. At threshold 5 this reproduces; changing
# THRESHOLD above from 5 to 4 must turn this one line red and none of the
# others, because 4.700 <= 5 but 4.700 > 4.
same "reproduced" "$(pct_verdict "$(diff_pct 10000 10470)" "$THRESHOLD")" \
  "B2, 4.7%, reproduces at threshold $THRESHOLD"

echo
echo "=== compare-w2w-procedures.sh, whole file"

fixdir=$(mktemp -d)
trap 'rm -rf "$fixdir"' EXIT

cat >"$fixdir/s1.txt" <<'EOF'
  == hft / admin / off: median of 9 qualifying runs (1 disqualified) ==
     min    15830 ns
     p50    16070 ns      (across runs: 15930 .. 16091)
     p99    21050 ns
     p99.9  22753 ns
     spread max/median 1.001
     dispersion p50  16070 ns      (across runs: 15930 .. 16091)   min/median 0.991   max/median 1.001
     dispersion p99  21050 ns      (across runs: 20469 .. 21381)   min/median 0.972   max/median 1.016
     dispersion p99.9  22753 ns      (across runs: 21972 .. 27022)   min/median 0.966   max/median 1.188
     machine pass 16   fail 0   unknown 0
     pinned  engine cpu6, client cpu7

  == hft / app / off: median of 9 qualifying runs (1 disqualified) ==
     min    17383 ns
     p50    20774 ns      (across runs: 20138 .. 20388)
     p99    25188 ns
     p99.9  27202 ns
     spread max/median 1.009
     dispersion p50  20774 ns      (across runs: 20138 .. 20388)   min/median 0.996   max/median 1.009
     dispersion p99  25188 ns      (across runs: 24546 .. 25489)   min/median 0.975   max/median 1.012
     dispersion p99.9  27202 ns      (across runs: 26491 .. 44124)   min/median 0.974   max/median 1.622
     machine pass 16   fail 0   unknown 0
     pinned  engine cpu6, client cpu7

  == standard / admin / off: median of 9 qualifying runs (0 disqualified) ==
     min    9000 ns
     p50    12000 ns      (across runs: 11800 .. 12200)
     p99    15000 ns
     p99.9  16000 ns
     spread max/median 1.001
     machine pass 16   fail 0   unknown 0
     pinned  engine cpu6, client cpu7
EOF

cat >"$fixdir/s2.txt" <<'EOF'
  == hft / admin / off: median of 10 qualifying runs (0 disqualified) ==
     min    15835 ns
     p50    16080 ns      (across runs: 15971 .. 16191)
     p99    20719 ns
     p99.9  22798 ns
     spread max/median 1.007
     dispersion p50  16080 ns      (across runs: 15971 .. 16191)   min/median 0.993   max/median 1.007
     dispersion p99  20719 ns      (across runs: 20299 .. 21371)   min/median 0.980   max/median 1.031
     dispersion p99.9  22798 ns      (across runs: 21881 .. 27402)   min/median 0.960   max/median 1.202
     machine pass 16   fail 0   unknown 0
     pinned  engine cpu6, client cpu7

  == hft / app / off: median of 8 qualifying runs (2 disqualified) ==
     min    17403 ns
     p50    17473 ns      (across runs: 20198 .. 20439)
     p99    25198 ns
     p99.9  26956 ns
     spread max/median 1.010
     dispersion p50  17473 ns      (across runs: 20198 .. 20439)   min/median 0.999   max/median 1.010
     dispersion p99  25198 ns      (across runs: 24557 .. 25368)   min/median 0.975   max/median 1.007
     dispersion p99.9  26956 ns      (across runs: 26480 .. 42020)   min/median 0.982   max/median 1.559
     machine pass 16   fail 0   unknown 0
     pinned  engine cpu6, client cpu7

  == standard / app / off: median of 9 qualifying runs (0 disqualified) ==
     min    9000 ns
     p50    13000 ns      (across runs: 12800 .. 13200)
     p99    16000 ns
     p99.9  17000 ns
     spread max/median 1.001
     machine pass 16   fail 0   unknown 0
     pinned  engine cpu6, client cpu7
EOF

out=$("$here/compare-w2w-procedures.sh" "$fixdir/s1.txt" "$fixdir/s2.txt" 2>&1)
rc=$?

same "1" "$rc" \
  "exit 1: one arm (hft/app/off, p50 18.9%) does not reproduce"
same "1" "$(printf '%s\n' "$out" | grep -c 'unpaired (present in only one summary):')" \
  "unpaired section printed once"
same "1" "$(printf '%s\n' "$out" | grep -c '^    standard/admin/off$')" \
  "standard/admin/off (s1 only) listed as unpaired"
same "1" "$(printf '%s\n' "$out" | grep -c '^    standard/app/off$')" \
  "standard/app/off (s2 only) listed as unpaired"
same "arms 2 reproduced 1 not-reproduced 1" \
  "$(printf '%s\n' "$out" | tail -1)" \
  "final line: 2 paired arms, 1 reproduced, 1 not"

echo
echo "=== summary"
echo "ok scripts/check-w2w-compare.sh pass $pass   fail $fail"
[[ "$fail" -eq 0 ]]
