#!/usr/bin/env bash
# Does scripts/ab-rotation.sh's pure summary — ab_summary / ab_median /
# ab_complete_rounds — compute what boot D's rotation actually needs: the
# C-91 pair reproduced exactly (1 660.2 -> 1 817.2 = +9.5%), and a round
# with one arm DISQUALIFIED dropped for every arm's n, not just that arm's?
#
# ab_summary is pure: two file paths and a control name in, a table out. No
# cargo, no clock, no /proc — sourced here with AB_ROTATION_SOURCE_ONLY=1,
# the same shape scripts/check-w2w-compare.sh uses on
# compare-w2w-procedures.sh (COMPARE_SOURCE_ONLY=1) and
# scripts/check-w2w-baseline-summary.sh uses on w2w-baseline.sh
# (BASELINE_SOURCE_ONLY=1).
#
# The reversal target lives in `same "... +9.5% ..."` below: change 9.5 to
# 8.5 and this file must go red on exactly that assertion, nothing else —
# scripts/ab-rotation.sh:459's diff formula is genuinely being checked, not
# a test that always agrees with whatever the code prints.
#
# What this file does NOT cover: whether the driver actually refuses to
# build, and whether a real ROUNDS=1 rehearsal leaves the binaries
# untouched. Those need real cargo, real binaries and real files, and are
# proven by hand against the live script — plan *Cách kiểm chứng*, P1 row.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=ab-rotation.sh
# shellcheck disable=SC1091 # found at runtime via $here; not followed without -x
AB_ROTATION_SOURCE_ONLY=1 . "$here/ab-rotation.sh"

pass=0
fail=0

same() { # same <want> <got> <what it means>
  if [[ "$2" == "$1" ]]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$3"
  else
    fail=$((fail + 1)); printf 'FAIL  want [%s] got [%s]  %s\n' "${1//$'\n'/ }" "${2//$'\n'/ }" "$3"
  fi
}

fixtures="$(mktemp -d)"
trap 'rm -rf "$fixtures"' EXIT

echo "=== ab_median"

same "1660.2" "$(printf '%s\n' 1660.2 | ab_median)" \
  "a single value is its own median"
same "1738.7" "$(printf '%s\n' 1660.2 1817.2 | ab_median)" \
  "median of two values is their mean, not truncated (unlike w2w-baseline.sh's integer median)"
same "1817.2" "$(printf '%s\n' 1660.2 1817.2 1817.2 | ab_median)" \
  "median of three is the middle one, sorted"

echo
echo "=== ab_complete_rounds"

cat >"$fixtures/timeline.txt" <<'EOF'
round 1 arm w0 busy 1% ok
round 1 arm wa busy 1% ok
round 1 complete
round 2 arm w0 busy 1% ok
round 2 arm wa busy 1% ok
round 2 complete
round 3 arm w0 busy 9% DISQUALIFIED
round 3 arm wa busy 1% ok
round 3 incomplete
EOF

same "1
2" "$(ab_complete_rounds "$fixtures/timeline.txt")" \
  "round 3 carries no 'complete' trailer — one disqualified arm keeps the whole round out"

echo
echo "=== ab_summary — the C-91 pair (measured-costs.md, C-91: 1660.2 -> 1817.2 = +9.5%)"

# w0 is CONTROL and never even attempts round 3 (it was the one disqualified
# there). wa DID run its round-3 suite — 9999.9, an outlier that would drag
# its median and its diff% far off the real numbers if it were counted. The
# plan's own rule (P1 row; ADR-0090 decision 3; trap table "Vòng có một arm
# bị loại...driver đánh dấu cả vòng incomplete; summary bỏ vòng đó cho mọi
# arm") says round 3 is dropped for wa too, even though wa itself was never
# DISQUALIFIED — only w0 was.
cat >"$fixtures/runs.txt" <<'EOF'
w0	1	parse NewOrderSingle (35 fields)	1660.2
wa	1	parse NewOrderSingle (35 fields)	1817.2
w0	2	parse NewOrderSingle (35 fields)	1660.2
wa	2	parse NewOrderSingle (35 fields)	1817.2
wa	3	parse NewOrderSingle (35 fields)	9999.9
EOF

out=$(ab_summary "$fixtures/runs.txt" "$fixtures/timeline.txt" w0)

same "1" "$(printf '%s\n' "$out" | grep -c '^w0 ')" \
  "one row for the control arm"
same "1" "$(printf '%s\n' "$out" | grep -c '^wa ')" \
  "one row for the other arm"
same "$(printf '%s\n' "$out" | grep '^w0 ')" \
  "$(printf '%s\n' "$out" | awk '/^w0 / { printf "%s", $0 }')" \
  "sanity: the control row parses as one line"

w0_line=$(printf '%s\n' "$out" | grep '^w0 ')
wa_line=$(printf '%s\n' "$out" | grep '^wa ')

# The case name ("parse NewOrderSingle (35 fields)") carries its own spaces,
# so the five trailing columns (median, min/med, max/med, n, diff%) are read
# relative to NF, never by a fixed column number.
same "1660.2" "$(printf '%s\n' "$w0_line" | awk '{print $(NF-4)}')" \
  "control median is exactly the C-91 baseline figure, unmoved by anything in round 3"
same "2" "$(printf '%s\n' "$w0_line" | awk '{print $(NF-1)}')" \
  "control n is 2, not 3 — round 3 never had a control row to begin with"
same "1817.2" "$(printf '%s\n' "$wa_line" | awk '{print $(NF-4)}')" \
  "REVERSAL TARGET (median): the outlier round-3 sample (9999.9) is excluded, not blended in"
same "2" "$(printf '%s\n' "$wa_line" | awk '{print $(NF-1)}')" \
  "REVERSAL TARGET (n): wa's own round-3 run is dropped even though wa itself was never DISQUALIFIED — w0 was"
same "+9.5%" "$(printf '%s\n' "$wa_line" | awk '{print $NF}')" \
  "REVERSAL TARGET (diff%): 1817.2 vs control 1660.2 is +9.5%, C-91's own number — change this file's 9.5 to 8.5 and this line must go red"

echo
echo "=== ab_extract — ADR-0092 decision 1, fixture captured from a real bench binary"

# Captured 2026-09-22 by the developer (sonnet), step 1 of
# docs/plans/2026-09-22-the-detector-and-the-campaign-preconditions.md, on a
# cloud container. Command:
#   RUSTFLAGS="$(scripts/check-bench-alignment.sh --flags)" \
#     cargo bench -p fixbolt-session --bench validate --no-run \
#     --message-format=json
#   then two temporary lines appended to benches/baselines.tsv (one case
#   forced OVER, one forced UNDER) and the resolved binary run once directly
#   — reverted straight after with `git checkout benches/baselines.tsv`,
#   `git diff --exit-code benches/baselines.tsv` exiting 0.
# CPU: Intel(R) Xeon(R) Processor @ 2.80GHz
# Binary: target/release/deps/validate-56b06784da3bc3fc
#   sha256 4593d0717bdf72df1ca2cf5cb66a2a7bab156b06da4c88433b84888711f3f5ff
# Pasted verbatim, not typed from memory — the trap the plan names twice.
cat >"$fixtures/harness-raw.txt" <<'EOF'
machine   Intel(R) Xeon(R) Processor @ 2.80GHz
validate NewOrderSingle              1219.3 ns/op   baseline 1.0 x1.10 = [0.9, 1.1]  OVER BASELINE
validate Heartbeat                    246.1 ns/op   baseline 999999.0 x1.10 = [909090.0, 1099998.9]  UNDER BASELINE
validate TestRequest, w2w bytes       311.0 ns/op   NO BASELINE for 'Intel(R) Xeon(R) Processor @ 2.80GHz'
    to record, after check-machine.sh reads fail 0, append to benches/baselines.tsv (median of N>=20 runs, margin from the ladder in that file's header):
    Intel(R) Xeon(R) Processor @ 2.80GHz	validate TestRequest, w2w bytes	311.0	<margin>	<n>	<date>	<verdict>
validate NewOrderSingle, w2w bytes   1215.2 ns/op   NO BASELINE for 'Intel(R) Xeon(R) Processor @ 2.80GHz'
    to record, after check-machine.sh reads fail 0, append to benches/baselines.tsv (median of N>=20 runs, margin from the ladder in that file's header):
    Intel(R) Xeon(R) Processor @ 2.80GHz	validate NewOrderSingle, w2w bytes	1215.2	<margin>	<n>	<date>	<verdict>
cases without a baseline: 2  validate TestRequest, w2w bytes, validate NewOrderSingle, w2w bytes
cases under their baseline: 1  validate Heartbeat: 246.1 ns/op is below 909090.0 ns (baseline 999999.0 / 1.10) — re-record the baseline, or the benchmark stopped measuring

thread 'main' (883) panicked at crates/session/benches/../../codec/benches/harness.rs:405:9:
1 of 4 case(s) over the machine baseline:
validate NewOrderSingle: 1219.3 ns/op exceeds 1.1 ns (baseline 1.0 x 1.10)
stack backtrace:
   0: __rustc::rust_begin_unwind
   1: core::panicking::panic_fmt
   2: validate::harness::suite::<validate::main::{closure#0}>
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
EOF

extracted=$(ab_extract w1 7 <"$fixtures/harness-raw.txt")

same "4" "$(printf '%s\n' "$extracted" | wc -l)" \
  "REVERSAL TARGET (rows): the harness's OVER/UNDER report lines and the panic body are not measurement rows"
same "0" "$(printf '%s\n' "$extracted" | cut -f3 | grep -c ':$')" \
  "no case name ends with ':' — the phantom rows the old awk welded a colon onto are gone"
same "over
under
none
none" "$(printf '%s\n' "$extracted" | cut -f5)" \
  "verdict column reads over/under/none/none: validate NewOrderSingle (forced OVER), validate Heartbeat (forced UNDER), the two cases with no recorded baseline for this CPU"

echo
echo "=== summary"
echo "pass $pass   fail $fail"
[[ "$fail" -eq 0 ]]
