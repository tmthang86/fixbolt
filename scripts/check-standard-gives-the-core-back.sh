#!/usr/bin/env bash
# CLAUDE.md §2 non-negotiable 4, SECOND HALF: IN `standard` MODE THE ENGINE
# THREAD MUST BLOCK WHEN IDLE, AND GIVE THE CORE BACK.
#
# ADR-0013 decision 6 asked for this and said how: CPU time over a wall-clock
# window, "not by reading the code". Until this script existed rule 4 was
# half-enforced and CLAUDE.md's own machine-checked list said so.
#
# ## Why one number is not enough
#
# **CPU near zero is passable by an engine that is broken in three different
# ways**, and each of them still answers every message and passes all 59
# acceptance definitions:
#
#   * the engine thread DIED, and a dead thread uses no CPU at all;
#   * the engine is woken by its own 100 ms timeout rather than by the data, so
#     it blocks beautifully and is 100 ms slower per message;
#   * the run never reached the mode it was asked for, which has already
#     happened once here — see
#     docs/reference/feature-flags-unify-across-a-workspace.md.
#
# So this asserts FOUR things, and all four have to hold:
#
#   1. the mode the binary REPORTS is the one that was asked for;
#   2. the engine thread's CPU over the window is under the ceiling;
#   3. the thread is alive and its scheduler state is `S` (sleeping) and not
#      `R` (running) for most of the window;
#   4. the round-trip p50 is far below the poll timeout, which is the only
#      assertion that can tell "woken by the data" from "woken by the clock".
#
# **And it requires its own RED half to fail.** `--mode hft` must trip it, and
# so must `--mode yield` — the strategy this repository has been *claiming*
# fails both gates since it was renamed, without ever showing it.
#
# The syscall evidence lives in the other gate rather than here:
# scripts/check-no-kernel-sleep.sh already traces this binary and requires
# `--mode standard` to trip its sleeper list. Duplicating strace here would also
# distort the very CPU figure this script exists to measure.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ROOT}/target/release/w2w"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

[[ "$(uname -s)" == "Linux" ]] || {
  echo "SKIPPED, NOT PASSED: /proc is Linux-only, so nothing was measured." >&2
  echo "CLAUDE.md §10: a green result that was inferred rather than observed is not a result." >&2
  exit 2
}
[[ -x "${BIN}" ]] || { echo "build it first: cargo build -p fixbolt-w2w --release" >&2; exit 2; }

HZ="$(getconf CLK_TCK)"
WINDOW_S=3
# `standard` wakes about ten times a second on its timeout plus once per
# message. Ten times that is still nothing, and a spinning engine is 100%, so
# nothing sits near this line by accident.
CEILING_PCT=5
# The poll timeout is 100 ms. A p50 anywhere near it means the engine is being
# woken by its own clock; two orders of magnitude below it means the data.
P50_CEILING_NS=1000000
SAMPLES=20

# Run w2w in one mode and report: mode-as-run, engine-thread CPU percent over a
# wall-clock window, share of samples found sleeping, and the p50.
measure() {
  local mode="$1"
  local out="${TMP}/out.${mode}" pid tid t0 t1 c0 c1 sleeping=0 alive=0

  "${BIN}" --mode "${mode}" --messages 300 --warmup 50 --hold-ms $((WINDOW_S * 1000 + 2000)) \
    > "${out}" 2>&1 &
  pid=$!

  # Wait for the engine thread to announce itself.
  for _ in $(seq 1 100); do
    tid="$(grep -oE '^engine-tid: [0-9]+' "${out}" 2>/dev/null | head -1 | grep -oE '[0-9]+')"
    [[ -n "${tid:-}" ]] && break
    sleep 0.1
  done
  [[ -n "${tid:-}" ]] || { echo "no engine-tid from --mode ${mode}" >&2; kill "${pid}" 2>/dev/null; return 1; }

  local stat="/proc/${pid}/task/${tid}/stat"
  [[ -r "${stat}" ]] || { echo "cannot read ${stat}" >&2; kill "${pid}" 2>/dev/null; return 1; }

  # utime + stime, fields 14 and 15 AFTER the comm field. The comm field is
  # parenthesised and may itself contain spaces, so everything is counted from
  # the LAST ')' rather than from the start of the line.
  cpu_ticks() {
    local line rest
    line="$(cat "${stat}" 2>/dev/null)" || return 1
    rest="${line##*) }"
    # shellcheck disable=SC2086
    set -- ${rest}
    # $1 is state; utime is the 12th field after it, stime the 13th.
    # BRACES ARE NOT OPTIONAL: `$12` is `${1}2`, which under `set -u` becomes
    # an unbound variable named after whatever state the thread was in.
    echo "$(( ${12} + ${13} ))"
  }
  thread_state() {
    local line rest
    line="$(cat "${stat}" 2>/dev/null)" || return 1
    rest="${line##*) }"
    echo "${rest%% *}"
  }

  c0="$(cpu_ticks)" || return 1
  t0="$(date +%s.%N)"
  for _ in $(seq 1 "${SAMPLES}"); do
    sleep "$(echo "${WINDOW_S} / ${SAMPLES}" | bc -l)"
    local st
    st="$(thread_state)" || continue
    alive=$((alive + 1))
    [[ "${st}" == "S" ]] && sleeping=$((sleeping + 1))
  done
  t1="$(date +%s.%N)"
  c1="$(cpu_ticks)" || { echo "the engine thread vanished during the window" >&2; return 1; }

  wait "${pid}"

  local ran_mode p50 pct
  ran_mode="$(grep -oE '^mode: [a-z]+' "${out}" | head -1 | cut -d' ' -f2)"
  # `awk`, not `grep -oE '[0-9]+'`. `[measured 2026-08-30]` the first version
  # was the latter, and it reported **50** for every mode — the "50" in the
  # LABEL `p50`, which is the first run of digits on the line. So assertion 4,
  # the one that is supposed to be the only thing able to tell "woken by the
  # data" from "woken by the clock", was comparing a constant against its
  # ceiling and passing unconditionally. It passed in all three arms, which is
  # precisely why nothing looked wrong.
  p50="$(awk '/^ +p50 +[0-9]+ ns/ { print $2; exit }' "${out}")"
  pct="$(echo "scale=2; 100 * (${c1} - ${c0}) / ${HZ} / (${t1} - ${t0})" | bc -l)"

  echo "${ran_mode:-none} ${pct} ${sleeping} ${alive} ${p50:-0}"
}

# Judge one mode's measurement.
#
#   0  the mode gave the core back
#   1  it did not — a POLICY verdict, and the only thing a red half may show
#   2  the measurement itself did not happen
#
# **1 and 2 are kept apart on purpose.** `[measured 2026-08-30]` the first
# version of this script returned 1 for both, and a typo that broke every
# measurement made all three arms report what looked like the right answer: the
# green half failed and both red halves "tripped". A red half that goes red
# because the harness is broken proves exactly as much as a green half that is
# green because nothing ran — which is CLAUDE.md §10, applied to the half of the
# gate that is supposed to be the safety net.
judge() {
  local want="$1" ran="$2" pct="$3" sleeping="$4" alive="$5" p50="$6" ok=0

  printf '  mode reported   %s\n' "${ran}"
  printf '  engine CPU      %s%%   (ceiling %s%%)\n' "${pct}" "${CEILING_PCT}"
  printf '  found sleeping  %s of %s samples\n' "${sleeping}" "${alive}"
  printf '  round trip p50  %s ns   (ceiling %s ns)\n' "${p50}" "${P50_CEILING_NS}"

  [[ "${ran}" == "${want}" ]] || { echo "  !! the binary ran '${ran}', not '${want}'"; return 2; }
  (( alive >= SAMPLES / 2 )) || { echo "  !! the engine thread was not there to measure"; return 2; }
  [[ -n "${pct}" && -n "${p50}" ]] || { echo "  !! the measurement produced no numbers"; return 2; }

  [[ "$(echo "${pct} < ${CEILING_PCT}" | bc -l)" == "1" ]] || { echo "  -> it did not give the core back"; ok=1; }
  (( sleeping * 2 > alive )) || { echo "  -> it was running, not sleeping"; ok=1; }
  (( p50 > 0 && p50 < P50_CEILING_NS )) || { echo "  -> it was woken by its own clock, not by the data"; ok=1; }
  return "${ok}"
}

rc=0

echo "== GREEN half: standard mode must block and give the core back =="
read -r ran pct sleeping alive p50 <<<"$(measure standard)" || exit 1
judge standard "${ran}" "${pct}" "${sleeping}" "${alive}" "${p50}"
case $? in
  0) echo "GREEN ok — standard blocks, stays alive, and is woken by the data" ;;
  1) echo "FAIL: standard mode does not satisfy non-negotiable 4's second half" >&2; rc=1 ;;
  *) echo "FAIL: the green half could not be measured, so nothing was checked" >&2; exit 2 ;;
esac

# A check that has only ever been seen passing is not known to work. Two of this
# repository's guards for the OTHER half of rule 4 were exactly that.
for red in hft yield; do
  echo
  echo "== RED half: ${red} must trip this check =="
  read -r ran pct sleeping alive p50 <<<"$(measure "${red}")" || exit 1
  judge "${red}" "${ran}" "${pct}" "${sleeping}" "${alive}" "${p50}"
  case $? in
    0) echo "FAIL: ${red} PASSED this check, so the check cannot fail and means nothing" >&2; rc=1 ;;
    1) echo "RED   ok — ${red} trips it on the policy, as it must" ;;
    *) echo "FAIL: ${red} could not be measured, so its red is not evidence of anything" >&2; exit 2 ;;
  esac
done

exit "${rc}"
