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
echo "=== compare-w2w-procedures.sh, WIRE_NIC arm (ADR-0071 decision 2)"

wiredir=$(mktemp -d)
trap 'rm -rf "$wiredir" "$fixdir"' EXIT

# Counterparty rows differ 30% (would be "not reproduced" if read); wire
# rows differ 0.3% (must be "reproduced" — the wire pair is what gets
# published per ADR-0071 decision 2).
cat >"$wiredir/w1.txt" <<'EOF'
  == hft / admin / off: median of 6 qualifying runs (4 disqualified) ==
     min    129083 ns
     p50    232375 ns      (across runs: 232125 .. 232666)
     p99    279792 ns
     p99.9  939812 ns
     spread max/median 1.001
     machine pass 16   fail 0   unknown 0
     pinned  engine cpu6; the generator on thangtran@192.168.77.2 is NOT pinned by this script
     split   as the counterparty sees it — not an acceptor wire figure
     generator  thangtran@192.168.77.2

     -- acceptor wire figure: NIC in -> NIC out at the acceptor, enp9s0 hardware stamps --
     -- median of the same 6 runs; NOT the counterparty table above, and nothing is subtracted --
     wire p50    27050 ns      (across runs: 27034 .. 27178)
     wire p99    31982 ns
     wire p99.9  35374 ns
     window      every request after the logon, leaving out the first 2000
     stamps      hw-rx-missing 0 and hw-tx-missing 26, summed over 6 runs (each run within ADR-0071's rule: RX 0, TX <= 0.1%)
     observer    cpu7
EOF

cat >"$wiredir/w2.txt" <<'EOF'
  == hft / admin / off: median of 6 qualifying runs (4 disqualified) ==
     min    129604 ns
     p50    302088 ns      (across runs: 301500 .. 302666)
     p99    363730 ns
     p99.9  1221756 ns
     spread max/median 1.002
     machine pass 16   fail 0   unknown 0
     pinned  engine cpu6; the generator on thangtran@192.168.77.2 is NOT pinned by this script
     split   as the counterparty sees it — not an acceptor wire figure
     generator  thangtran@192.168.77.2

     -- acceptor wire figure: NIC in -> NIC out at the acceptor, enp9s0 hardware stamps --
     -- median of the same 6 runs; NOT the counterparty table above, and nothing is subtracted --
     wire p50    27131 ns      (across runs: 27040 .. 27210)
     wire p99    32078 ns
     wire p99.9  35480 ns
     window      every request after the logon, leaving out the first 2000
     stamps      hw-rx-missing 0 and hw-tx-missing 26, summed over 6 runs (each run within ADR-0071's rule: RX 0, TX <= 0.1%)
     observer    cpu7
EOF

wireout=$("$here/compare-w2w-procedures.sh" "$wiredir/w1.txt" "$wiredir/w2.txt" 2>&1)
wirerc=$?

same "0" "$wirerc" \
  "exit 0: the wire pair (0.3% at every percentile) reproduces despite the counterparty rows differing 30%"
same "1" "$(printf '%s\n' "$wireout" | grep -c 'wire p50')" \
  "output is labelled 'wire p50', not the counterparty's p50"
same "arms 1 reproduced 1 not-reproduced 0" \
  "$(printf '%s\n' "$wireout" | tail -1)" \
  "the one arm reproduces on the wire figure"

# Reversal: widen the wire rows to a 6% difference (still under the
# counterparty's own 30%) and the arm must flip to "not reproduced" on the
# wire assertion — proving the comparator is reading the wire rows, not the
# counterparty rows, which never moved.
sed -i 's/wire p50    27131 ns.*/wire p50    28769 ns      (across runs: 28700 .. 28820)/' "$wiredir/w2.txt"
wireout_rev=$("$here/compare-w2w-procedures.sh" "$wiredir/w1.txt" "$wiredir/w2.txt" 2>&1)
wirerc_rev=$?
same "1" "$wirerc_rev" \
  "reversal: wire p50 widened to ~6% -> exit 1, not reproduced"
same "arms 1 reproduced 0 not-reproduced 1" \
  "$(printf '%s\n' "$wireout_rev" | tail -1)" \
  "reversal quoted: $(printf '%s\n' "$wireout_rev" | grep 'wire p50')"

echo
echo "=== compare-w2w-procedures.sh, two interval arms of one summary"

ivdir=$(mktemp -d)
trap 'rm -rf "$ivdir" "$wiredir" "$fixdir"' EXIT

cat >"$ivdir/i1.txt" <<'EOF'
  == hft / admin / off: median of 8 qualifying runs (2 disqualified) ==  interval 10us
     min    125979 ns
     p50    232813 ns      (across runs: 230208 .. 233416)
     p99    285083 ns
     p99.9  806270 ns
     spread max/median 1.003
     machine pass 15   fail 1   unknown 0

  == hft / admin / off: median of 7 qualifying runs (3 disqualified) ==  interval 20us
     min    126417 ns
     p50    232875 ns      (across runs: 232542 .. 233167)
     p99    285541 ns
     p99.9  935792 ns
     spread max/median 1.001
     machine pass 16   fail 0   unknown 0
EOF
cp "$ivdir/i1.txt" "$ivdir/i2.txt"

ivout=$("$here/compare-w2w-procedures.sh" "$ivdir/i1.txt" "$ivdir/i2.txt" 2>&1)
same "arms 2 reproduced 2 not-reproduced 0" \
  "$(printf '%s\n' "$ivout" | tail -1)" \
  "the @10us and @20us arms of one summary are two distinct, paired keys"
same "2" "$(printf '%s\n' "$ivout" | grep -c '^== hft/admin/off@')" \
  "both arm headers carry the interval suffix"

echo
echo "=== summary"
echo "ok scripts/check-w2w-compare.sh pass $pass   fail $fail"
[[ "$fail" -eq 0 ]]
