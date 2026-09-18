#!/usr/bin/env bash
# CLAUDE.md §2 non-negotiable 4, `hft` half — second machine check, ADR-0072.
#
# `scripts/check-no-kernel-sleep.sh` traces `tools/w2w` under `strace -f`, and
# an unprivileged tracer strips file capabilities from the traced process
# (security/commoncap.c, `cap_bprm_creds_from_file`), so that check cannot run
# the arm that opens a raw socket on a real NIC — the one place non-negotiable
# 4 matters most (docs/reference/a-traced-process-gets-no-file-capabilities.md,
# STATUS.md open item 86). This script needs no tracer and no capability: it
# reads `voluntary_ctxt_switches` for the engine thread out of
# `/proc/self/task/<tid>/status`, which `tools/w2w` itself samples on the MAIN
# thread right before and right after the timed window (ADR-0072 decision 1),
# so the read never adds a syscall to the thread it is watching.
#
# **What this cannot see, by design (ADR-0072 Consequences):**
#   * It names no syscall. A red run says only "the engine thread blocked
#     somewhere"; `check-no-kernel-sleep.sh` on `lo` still has to be run to
#     find out where.
#   * A non-blocking syscall that returns at once (a `poll` with timeout 0, a
#     `futex` wake) does not move this counter and is not a sleep — zero
#     voluntary switches is necessary, not sufficient.
#   * `sched_yield` and ordinary preemption land on the INVOLUNTARY counter,
#     not this one (`kernel/sched/core.c`, `__schedule`: `prev->nvcsw` only
#     when the previous task is no longer runnable; a yielding task stays
#     runnable). `--mode yield` is printed here but never judged by this
#     script for exactly that reason — see the delivery log for
#     docs/plans/2026-09-18-closing-the-open-items.md step 2.1: it prints
#     `voluntary 0` on this machine, same as `hft`, which `strace`'s SLEEPERS
#     list (it names `sched_yield` there) still catches on its own gate.
#
# `--mode hft` fails inside `tools/w2w` itself (`assert_eq!` on `voluntary`)
# if the engine thread made even one voluntary switch — that failure IS the
# gate's green/red for `hft`. This script's own reversal, built in exactly as
# ADR-0072 decision 2 asks, is the second run: `--mode standard` must read
# voluntary > 0, because `standard` blocks in `poll` while idle (ADR-0014).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ROOT}/target/release/w2w"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

[[ -x "${BIN}" ]] || { echo "build it first: cargo build --release -p fixbolt-w2w" >&2; exit 2; }

# Runs `w2w --mode $1`, reads the mode back (never trusts the flag — the
# `ran_mode` lesson `check-no-kernel-sleep.sh` already paid for), and prints
# `voluntary <n>` from the `engine-ctxt` line it finds. Only the `hft` run
# gets `--assert-no-voluntary-switches`: it is opt-in in `tools/w2w` itself
# (a `PTRACE_CONT` off a ptrace-stop is its own voluntary switch on the
# tracee, so `--mode hft` under `strace` must not carry it — see
# `scripts/check-no-kernel-sleep.sh`, which runs untouched, unflagged, and
# unaffected). `standard`'s red half is judged below by this script instead.
run_and_read() {
  local mode="$1" out="${TMP}/out.$1" ran voluntary assert_flag=()
  [[ "${mode}" == hft ]] && assert_flag=(--assert-no-voluntary-switches)
  # shellcheck disable=SC2086 # W2W_EXTRA: zero or more flags, split on spaces.
  "${BIN}" --messages 300 --warmup 50 --hold-ms 400 --mode "${mode}" "${assert_flag[@]}" ${W2W_EXTRA:-} \
    > "${out}" 2>&1
  local status=$?
  ran="$(grep -oE '^mode: [a-z]+' "${out}" | head -1 | cut -d' ' -f2)"
  if [[ "${ran}" != "${mode}" ]]; then
    echo "w2w ran mode '${ran:-<none>}' when '${mode}' was asked for" >&2
    tail -5 "${out}" >&2
    return 1
  fi
  voluntary="$(grep -oE '^engine-ctxt voluntary [0-9]+' "${out}" | head -1 | grep -oE '[0-9]+$')"
  if [[ -z "${voluntary}" ]]; then
    echo "no 'engine-ctxt voluntary' line in w2w's output for --mode ${mode}" >&2
    tail -10 "${out}" >&2
    return 1
  fi
  echo "${voluntary} ${status}"
}

rc=0

echo "== GREEN half: hft mode, w2w itself asserts voluntary == 0 =="
read -r hft_voluntary hft_status <<<"$(run_and_read hft)" || exit 1
echo "hft voluntary ${hft_voluntary}"
if [[ "${hft_status}" -ne 0 ]]; then
  echo "FAIL: --mode hft exited ${hft_status} — w2w's own assertion read voluntary != 0" >&2
  tail -5 "${TMP}/out.hft" >&2
  rc=1
elif [[ "${hft_voluntary}" -ne 0 ]]; then
  echo "FAIL: --mode hft printed voluntary ${hft_voluntary}, expected 0, and w2w did not catch it" >&2
  rc=1
else
  echo "GREEN ok — engine thread made 0 voluntary context switches"
fi

echo
echo "== RED half: standard mode blocks in poll while idle, and must read > 0 =="
read -r standard_voluntary standard_status <<<"$(run_and_read standard)" || exit 1
echo "standard voluntary ${standard_voluntary}"
if [[ "${standard_status}" -ne 0 ]]; then
  echo "FAIL: --mode standard exited ${standard_status}, so nothing was measured" >&2
  tail -5 "${TMP}/out.standard" >&2
  rc=1
elif [[ "${standard_voluntary}" -eq 0 ]]; then
  echo "FAIL: --mode standard did NOT trip this check (voluntary 0), so the check cannot" >&2
  echo "      fail and means nothing — ADR-0072 Consequences names the too-short --hold-ms" >&2
  echo "      case this would be" >&2
  rc=1
else
  echo "RED   ok — standard mode trips it: voluntary ${standard_voluntary} > 0"
fi

echo
if [[ "${rc}" -eq 0 ]]; then
  echo "PASS"
else
  echo "FAIL" >&2
fi
exit "${rc}"
