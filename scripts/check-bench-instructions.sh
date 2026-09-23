#!/usr/bin/env bash
# Does `scripts/bench-instructions.sh` implement ADR-0102 decision 1's
# verdict rule -- same-work / work-changed / unstable, at the 0.1% /
# 0.01% thresholds the decision names -- the way the rule is actually
# written, not the way it is remembered?
#
# `spread_pct` / `between_pct` / `classify` are pure
# (bench-instructions.sh:47-79); fed here with
# BENCH_INSTRUCTIONS_SOURCE_ONLY=1, the same shape
# scripts/check-w2w-compare.sh uses on compare-w2w-procedures.sh
# (COMPARE_SOURCE_ONLY=1).
#
# A stub `perf` (built below, never touches a real PMU) then drives the
# whole CLI end to end for five cases: same-work, work-changed, unstable, a
# missing counter, and a workload that exits 1 -- so what is proven is the
# argument parsing and the exit codes too, not only the arithmetic.
#
# No real counters are read here and none of this runs under `sudo`; it is
# safe in CI, where the PMU is usually unavailable (ADR-0102 decision 1).
#
# Reversal: SAME_WORK_THRESHOLD_PCT=0.1 in bench-instructions.sh is the
# number this file's own reversal exercises. Run this script once green,
# then by hand: edit bench-instructions.sh, change `0.1` to `1` on that
# line, re-run this script -- the "work-changed, 0.2% diff" case must go
# red on its own assertion (0.2% <= 1%, so the script now reads
# same-work) and no other case may move. Then restore the `0.1` and
# re-run -- green again. The expected FAIL line is printed by this script
# itself before it runs, in the "=== reversal target" section below, so it
# is on record before the edit is made.
#
# LC_ALL=C: bench-instructions.sh now sets this itself, but this file's own
# `same()` comparisons and awk calls need it too -- a reviewer saw 6 FAIL
# here under LC_ALL=en_DK.utf8 because mawk's printf spelled 0.02 as
# `0,020000`, and the "en_DK locale" case below re-checks this end to end.
export LC_ALL=C
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=bench-instructions.sh
# shellcheck disable=SC1091 # found at runtime via $here; not followed without -x
BENCH_INSTRUCTIONS_SOURCE_ONLY=1 . "$here/bench-instructions.sh"

pass=0
fail=0

same() { # same <want> <got> <what it means>
  if [[ "$2" == "$1" ]]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$3"
  else
    fail=$((fail + 1)); printf 'FAIL  want [%s] got [%s]  %s\n' "$1" "$2" "$3"
  fi
}

echo "=== spread_pct / between_pct (pure)"

same "0.000000" "$(spread_pct 1000000000 1000000000)" "no spread reads 0"
same "0.010000" "$(spread_pct 1000000000 1000100000)" "100 000 over 1e9 is 0.01%"
same "0.020000" "$(spread_pct 1000000000 1000200000)" "200 000 over 1e9 is 0.02%"
same "0.050000" "$(between_pct 1000000000 1000500000)" "500 000 over 1e9 is 0.05%"
same "0.200000" "$(between_pct 1000000000 1002000000)" "2 000 000 over 1e9 is 0.2%"
# between_pct is relative to A, not to the smaller value -- ADR-0102 decision
# 1's own formula, unlike compare-w2w-procedures.sh's diff_pct.
same "0.199601" "$(between_pct 1002000000 1000000000)" "between_pct is relative to its first argument"

echo
echo "=== classify (pure), at the committed thresholds (0.1% / 0.01%)"

same "same-work" "$(classify 0.000000 0.000000 0.050000)" \
  "0.05% between, 0% spread: same-work"
same "work-changed" "$(classify 0.000000 0.000000 0.200000)" \
  "0.2% between, 0% spread: work-changed"
same "unstable" "$(classify 0.020000 0.000000 0.050000)" \
  "arm A's own 0.02% spread: unstable, regardless of the between-arm figure"
same "unstable" "$(classify 0.000000 0.020000 0.050000)" \
  "arm B's own 0.02% spread: unstable"
# Boundary: exactly at a threshold passes it (decision 1 says "<=").
same "same-work" "$(classify 0.010000 0.010000 0.100000)" \
  "spread exactly 0.01% and between exactly 0.1%: still same-work"

echo
echo "=== counter_ok / pct_fully_counted (pure)"

# A finding against a first version of this script: it whitelisted only
# `<not counted>` by name, so `<not supported>` (what perf prints on a
# PMU-less VM) fell through untouched, was coerced by awk's + 0 into 0, and
# both min/max/classify ran to a verdict on that -- same-work if both arms
# read it, work-changed if only one did. counter_ok is a whitelist (a bare
# non-zero integer), not a blocklist, precisely so a third spelling of
# "no count" needs no third case here.
same "0" "$(counter_ok 1000000000 && echo 0 || echo 1)" "a real count passes"
same "1" "$(counter_ok '<not supported>' && echo 0 || echo 1)" \
  "<not supported> (PMU-less VM) fails -- the reviewer's finding"
same "1" "$(counter_ok '<not counted>' && echo 0 || echo 1)" "<not counted> fails"
same "1" "$(counter_ok '' && echo 0 || echo 1)" "an empty field fails"
same "1" "$(counter_ok '0' && echo 0 || echo 1)" "zero fails"
same "1" "$(counter_ok '12abc' && echo 0 || echo 1)" "trailing garbage fails"

same "0" "$(pct_fully_counted '100.00' && echo 0 || echo 1)" "100.00% passes"
same "0" "$(pct_fully_counted '100' && echo 0 || echo 1)" "bare 100 passes"
same "1" "$(pct_fully_counted '50.00' && echo 0 || echo 1)" \
  "50.00% (multiplexed) fails -- the reviewer's finding"
same "1" "$(pct_fully_counted '99.99' && echo 0 || echo 1)" "99.99% fails"
same "1" "$(pct_fully_counted '' && echo 0 || echo 1)" "an empty percent field fails"
same "1" "$(pct_fully_counted '<not supported>' && echo 0 || echo 1)" \
  "<not supported> as a percent field fails"

echo
echo "=== end to end, a stub perf"

FIXDIR=$(mktemp -d)
trap 'rm -rf "$FIXDIR"' EXIT

# Two placeholder "binaries". The stub perf never executes them -- it
# recognises which arm it was called for by basename -- but
# bench-instructions.sh itself checks [ -e "$A" ] / [ -e "$B" ] before
# running anything, so they must exist.
ARM_A="$FIXDIR/arm-a"
ARM_B="$FIXDIR/arm-b"
: >"$ARM_A"
: >"$ARM_B"
chmod +x "$ARM_A" "$ARM_B"

FAKE_PERF="$FIXDIR/fake-perf.sh"
cat >"$FAKE_PERF" <<'FAKE'
#!/usr/bin/env bash
# Stub `perf`: never touches hardware counters. Emits one pre-set
# instructions:u/cycles:u pair per call, picked by $FAKE_PERF_MODE and by
# which arm's placeholder path is the last argument -- so
# bench-instructions.sh's real parsing and classify() run against known
# numbers instead of the desk's PMU.
set -uo pipefail
mode="${FAKE_PERF_MODE:?FAKE_PERF_MODE not set}"
bin="${*: -1}"
arm=$(basename "$bin")

counter_file="${FAKE_PERF_COUNTER_DIR:?FAKE_PERF_COUNTER_DIR not set}/$arm.count"
n=$(cat "$counter_file" 2>/dev/null || echo 0)
echo $((n + 1)) >"$counter_file"

case "$mode" in
  missing_counter)
    echo "<not counted>,,instructions:u,0,0.00,,"  >&2
    echo "<not counted>,,cycles:u,0,0.00,,"        >&2
    exit 0
    ;;
  workload_exit1)
    echo "1000000000,,instructions:u,100000,100.00,," >&2
    echo "400000000,,cycles:u,100000,100.00,,"          >&2
    exit 1
    ;;
  not_supported_both)
    # A PMU-less VM: perf still exits 0, but the counter field itself says
    # so, for both arms.
    echo "<not supported>,,instructions:u,100000,0.00,," >&2
    echo "<not supported>,,cycles:u,100000,0.00,,"        >&2
    exit 0
    ;;
  not_supported_one)
    if [ "$arm" = "arm-a" ]; then
      echo "1000000000,,instructions:u,100000,100.00,," >&2
      echo "400000000,,cycles:u,100000,100.00,,"          >&2
    else
      echo "<not supported>,,instructions:u,100000,0.00,," >&2
      echo "<not supported>,,cycles:u,100000,0.00,,"        >&2
    fi
    exit 0
    ;;
  multiplexed)
    # Real counts, but only 50.00% of the run's time was actually counted --
    # the two events shared the PMU, and this figure is a scaled estimate,
    # not the same execution's instructions and cycles both counted
    # throughout.
    echo "1000000000,,instructions:u,100000,50.00,," >&2
    echo "400000000,,cycles:u,100000,50.00,,"          >&2
    exit 0
    ;;
  same_work)
    if [ "$arm" = "arm-a" ]; then instr=1000000000; else instr=1000500000; fi
    cycles=400000000
    ;;
  work_changed)
    if [ "$arm" = "arm-a" ]; then instr=1000000000; else instr=1002000000; fi
    cycles=400000000
    ;;
  unstable)
    # arm-a's own spread across its runs is 0.02% (> the 0.01% bound);
    # arm-b stays flat. between-arm figure would read same-work on its own.
    if [ "$arm" = "arm-a" ]; then
      case "$n" in
        1) instr=1000000000 ;;
        2) instr=1000200000 ;;
        *) instr=1000100000 ;;
      esac
    else
      instr=1000050000
    fi
    cycles=400000000
    ;;
  *)
    echo "fake-perf.sh: unknown FAKE_PERF_MODE '$mode'" >&2
    exit 90
    ;;
esac

echo "# started on a fake clock" >&2
echo "$instr,,instructions:u,100000,100.00,," >&2
echo "$cycles,,cycles:u,100000,100.00,,"       >&2
exit 0
FAKE
chmod +x "$FAKE_PERF"

# Two separate channels (an rc and a log file), not one packed string: the
# log is bench-instructions.sh's own multi-line output, and a text separator
# sharing a line with it is a trap of its own (cut splits per line, so a
# packed "<rc><sep><log>" only keeps the separator on the log's first line).
LASTOUT="$FIXDIR/last-out.txt"

run_case() { # run_case <mode> <perf command> <extra bench-instructions.sh args...> -> prints rc; log in $LASTOUT
  local mode=$1 perf_cmd=$2
  shift 2
  local countdir rc
  countdir=$(mktemp -d)
  FAKE_PERF_MODE="$mode" FAKE_PERF_COUNTER_DIR="$countdir" \
    PERF="$perf_cmd" "$here/bench-instructions.sh" "$@" >"$LASTOUT" 2>&1
  rc=$?
  rm -rf "$countdir"
  echo "$rc"
}

rc=$(run_case same_work "$FAKE_PERF" -n 3 -c 0 "$ARM_A" "$ARM_B")
same "0" "$rc" "same-work case: exit 0"
same "1" "$(grep -c '^verdict: same-work$' "$LASTOUT")" "same-work case: verdict line"

rc=$(run_case work_changed "$FAKE_PERF" -n 3 -c 0 "$ARM_A" "$ARM_B")
same "3" "$rc" "work-changed case: exit 3"
same "1" "$(grep -c '^verdict: work-changed$' "$LASTOUT")" "work-changed case: verdict line"

rc=$(run_case unstable "$FAKE_PERF" -n 3 -c 0 "$ARM_A" "$ARM_B")
same "2" "$rc" "unstable case: exit 2"
same "1" "$(grep -c '^verdict: unstable$' "$LASTOUT")" "unstable case: verdict line"

rc=$(run_case missing_counter "$FAKE_PERF" -n 2 -c 0 "$ARM_A" "$ARM_B")
same "2" "$rc" "missing/zero counter: exit 2"
same "1" "$(grep -c 'missing, zero, or non-numeric counter' "$LASTOUT")" "missing/zero counter: message printed"

rc=$(run_case workload_exit1 "$FAKE_PERF" -n 2 -c 0 "$ARM_A" "$ARM_B")
same "2" "$rc" "workload exit 1: exit 2"
same "1" "$(grep -c 'workload exited 1' "$LASTOUT")" "workload exit 1: message printed"

# Bad usage: no sudo of its own, an unreadable binary path is a plain exit 2.
rc=$(run_case same_work "$FAKE_PERF" -n 3 -c 0 "$ARM_A" "$FIXDIR/does-not-exist")
same "2" "$rc" "unreadable second binary: exit 2"

# Regression: PERF is a caller-provided COMMAND, not one executable name --
# the real caller passes PERF="sudo -n perf" (ADR-0102 decision 1). A first
# version of this script quoted "$PERF_BIN" as one token and failed with
# "sudo -n perf: command not found" the moment it was run against real
# hardware with a real sudo prefix; every case above used a one-word
# $FAKE_PERF and could not have caught it. This drives the stub through a
# two-word command (`bash <fake-perf.sh>`) the same way sudo -n perf is
# two-plus words, so a re-quoting regression here fails loudly again instead
# of only on the desk. `bash`, not `sh` (dash here): fake-perf.sh below uses
# a bash-only substitution and a literal `sh` prefix would fail on that, not
# on the word-splitting this case exists to catch.
rc=$(run_case same_work "bash $FAKE_PERF" -n 3 -c 0 "$ARM_A" "$ARM_B")
same "0" "$rc" "PERF as a multi-word command (bash <script>): exit 0"
same "1" "$(grep -c '^verdict: same-work$' "$LASTOUT")" "PERF as a multi-word command: verdict line"

# The reviewer's four findings, end to end: `<not supported>` on both arms,
# `<not supported>` on one arm, a multiplexed 50.00% run, and `-n 1`.

rc=$(run_case not_supported_both "$FAKE_PERF" -n 2 -c 0 "$ARM_A" "$ARM_B")
same "2" "$rc" "<not supported> on both arms: exit 2, not same-work rc=0"
same "1" "$(grep -c 'missing, zero, or non-numeric counter' "$LASTOUT")" \
  "<not supported> on both arms: refused by name"

rc=$(run_case not_supported_one "$FAKE_PERF" -n 2 -c 0 "$ARM_A" "$ARM_B")
same "2" "$rc" "<not supported> on one arm: exit 2, not work-changed"
same "1" "$(grep -c 'missing, zero, or non-numeric counter' "$LASTOUT")" \
  "<not supported> on one arm: refused by name"

rc=$(run_case multiplexed "$FAKE_PERF" -n 2 -c 0 "$ARM_A" "$ARM_B")
same "2" "$rc" "multiplexed (50.00%) counters: exit 2"
same "1" "$(grep -c 'multiplexed or unreadable percent-of-time-counted' "$LASTOUT")" \
  "multiplexed (50.00%) counters: refused by name"

# -n 1: rejected before any perf call at all -- an unstable verdict could
# never fire with one run per arm, so accepting -n 1 would silently drop a
# whole verdict, not just narrow the sample.
out=$("$here/bench-instructions.sh" -n 1 -c 0 "$ARM_A" "$ARM_B" 2>&1)
rc=$?
same "2" "$rc" "-n 1: exit 2 (usage), not a same-work/unstable run with one sample"
same "1" "$(printf '%s\n' "$out" | grep -c '^usage:')" "-n 1: usage message printed"

# The locale the reviewer found this under: mawk's printf spells 0.02 as
# `0,020000` under LC_ALL=en_DK.utf8, which used to break every numeric
# comparison downstream (spread_pct, between_pct, classify all read a comma
# where they expected a decimal point). bench-instructions.sh's own
# `export LC_ALL=C` must make this locale irrelevant end to end.
if locale -a 2>/dev/null | grep -qx 'en_DK.utf8'; then
  rc=$(LC_ALL=en_DK.utf8 run_case same_work "$FAKE_PERF" -n 3 -c 0 "$ARM_A" "$ARM_B")
  same "0" "$rc" "en_DK.utf8 caller locale: exit 0, same as LC_ALL=C"
  same "1" "$(grep -c '^verdict: same-work$' "$LASTOUT")" "en_DK.utf8 caller locale: verdict line"
  same "2" "$(grep -c 'spread=0.000000%' "$LASTOUT")" \
    "en_DK.utf8 caller locale: both arms' spread printed with a decimal point, not a comma"
  same "0" "$(grep -c '0,0' "$LASTOUT")" \
    "en_DK.utf8 caller locale: no comma-decimal anywhere in the output"
else
  echo "skip  en_DK.utf8 not installed on this machine (locale -a) -- not a self-test result either way"
fi

echo
echo "=== reversal target"
echo "bench-instructions.sh:43  SAME_WORK_THRESHOLD_PCT=0.1"
echo "Expected FAIL when it reads 1 instead of 0.1, before running:"
echo "  FAIL  want [work-changed] got [same-work]  work-changed case: verdict line"
echo "(0.2% <= 1%, so the same 'work_changed' fixture above reads same-work)."
echo "Run this script again after that one-line edit and confirm exactly that"
echo "line goes red and nothing else does; then restore 0.1 and confirm green."

echo
echo "Second reversal target: bench-instructions.sh's \`export LC_ALL=C\`."
echo "Expected FAIL when that line is removed, before running (en_DK.utf8"
echo "installed; skip otherwise):"
echo "  FAIL  want [1] got [0]  en_DK.utf8 caller locale: verdict line"
echo "and/or:"
echo "  FAIL  want [0] got [>0]  en_DK.utf8 caller locale: no comma-decimal anywhere in the output"
echo "(mawk's printf spells the spread/between figures with a comma under"
echo "LC_ALL=en_DK.utf8 once the script no longer forces LC_ALL=C, corrupting"
echo "classify()'s arithmetic or at minimum the printed figures)."
echo "Restore the line and confirm green again."

echo
echo "ok scripts/check-bench-instructions.sh pass $pass   fail $fail"
[[ "$fail" -eq 0 ]]
