#!/usr/bin/env bash
# Does `dispersion()` report both sides, per percentile, the way ADR-0068
# decision 5 asks for a published wire figure?
#
# `median` and `spread` (max/median) already existed; `[measured 2026-09-14]`
# on the DESIGN.md §9 desktop, the `hft`, app, `off` arm's spread column read
# 1.008 while five of its twenty runs sat 13.4-0.3% BELOW the median it was
# supposed to bound — a column that is maximum-over-median cannot register a
# fast outlier at all. `docs/reference/
# a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md`, "A
# spread column that cannot say how far down a run fell", names the trap;
# this test is fed that arm's own twenty numbers, so the guard is provably
# about the run that motivated it, not an invented one.
#
# `[2026-09-15]` also `extra_flag_refusal()`, the pure half of review finding
# F13 (a `W2W_EXTRA` spelled `--journal=X` bypassed the journal identity check).
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=w2w-baseline.sh
# shellcheck disable=SC1091 # found at runtime via $here; not followed without -x
BASELINE_SOURCE_ONLY=1 . "$here/w2w-baseline.sh"

pass=0
fail=0

same() { # same <want> <got> <what it means>
  if [[ "$2" == "$1" ]]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$3"
  else
    fail=$((fail + 1)); printf 'FAIL  want [%s] got [%s]  %s\n' "${1//$'\n'/ }" "${2//$'\n'/ }" "$3"
  fi
}

echo "=== dispersion"

# The 2026-09-14 trap's own numbers: `hft`, app, `off`, procedure 2's twenty
# per-run p50s (measured-costs.md, "TLS on the wire").
trap_p50=(17323 17523 17864 18976 19136 19990 19992 19994 19996 19998 \
          19998 20000 20010 20020 20050 20100 20120 20140 20150 20158)
same "p50  19998 ns      (across runs: 17323 .. 20158)   min/median 0.866   max/median 1.008" \
  "$(dispersion p50 "${trap_p50[@]}")" \
  "a run 13.4% under the median is visible from the min side"

# A symmetric set: both sides sit the same distance from the median.
same "p99  10000 ns      (across runs: 9000 .. 11000)   min/median 0.900   max/median 1.100" \
  "$(dispersion p99 9000 9500 10000 10500 11000)" \
  "a symmetric set reads the same distance on both sides"

# n = 1: nothing to disperse, both ratios are 1.000.
same "p99.9  12345 ns      (across runs: 12345 .. 12345)   min/median 1.000   max/median 1.000" \
  "$(dispersion p99.9 12345)" \
  "n = 1 has no dispersion — both ratios read 1.000"

# n even: the same integer average `median()` already takes today, not a new
# rule invented for dispersion.
same "p50  15 ns      (across runs: 10 .. 20)   min/median 0.667   max/median 1.333" \
  "$(dispersion p50 10 20)" \
  "n even averages the two middle values as an integer, like median() today"

echo
echo "=== extra_flag_refusal"

# `[2026-09-15]` review finding F13: tools/w2w matches `--journal` only as a
# whole word, so `--journal=file-async` ran the default journal while the
# identity check was skipped and the summary still printed the flag. The
# refusal names the token and the two-word form.
same "W2W_EXTRA token '--journal=file-async': tools/w2w reads --journal only as a word of its own and would ignore this spelling, and the identity check would never run — write it as two words, '--journal file-async'" \
  "$(extra_flag_refusal --journal=file-async)" \
  "--journal=X is refused, naming the token and the two-word form"
same "W2W_EXTRA token '--log=file': tools/w2w reads --log only as a word of its own and would ignore this spelling, and the identity check would never run — write it as two words, '--log file'" \
  "$(extra_flag_refusal --log=file)" \
  "--log=X is refused the same way"
# The two-word form's words, one at a time, are what W2W_EXTRA is split into.
same "" "$(extra_flag_refusal --journal)$(extra_flag_refusal file-async)$(extra_flag_refusal --log)$(extra_flag_refusal file)" \
  "the two-word form is not refused"

echo
echo "=== missing_stamp_verdict"

# ADR-0071 decision 1: a run with skipped TX stamps within 0.1% of the timed
# requests is a valid run with a smaller sample, not a FAIL.
same "" "$(missing_stamp_verdict 0 19 20000)" \
  "19 of 20000 tx-missing is within 0.1%, no RX missing: PASS (nothing printed)"
same "hw-tx-missing 21 of 20000 exceeds 0.1%" "$(missing_stamp_verdict 0 21 20000)" \
  "21 of 20000 tx-missing exceeds 0.1%: FAIL"
same "hw-rx-missing 1 of 20000 — any missing RX stamp is a failed run (ADR-0071 decision 1)" \
  "$(missing_stamp_verdict 1 0 20000)" \
  "any missing RX stamp fails outright, even with tx-missing 0"

echo
echo "=== summary"
echo "pass $pass   fail $fail"
[[ "$fail" -eq 0 ]]
