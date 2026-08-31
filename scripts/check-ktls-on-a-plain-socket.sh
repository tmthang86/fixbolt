#!/usr/bin/env bash
# STATUS.md open item 10 / ADR-0005 open question 1: can `ktls-core` be driven
# from a plain non-blocking socket, with no async runtime?
#
# This runs the spike in spikes/ktls and reads its output, then traces it and
# attributes syscalls to the thread that drove the offloaded socket — by the tid
# that wrote the steady-state marker, not by the process, because the peer
# thread would otherwise mask everything. Same reasoning as
# check-no-kernel-sleep.sh, which learned it the expensive way.
#
# **It runs the binary twice and REQUIRES THE SECOND RUN TO FAIL.** With
# KTLS_SPIKE_WAIT=poll the steady-state loop calls poll(2) before each read, and
# the trace must show it. A check nobody has seen fail is not known to work.
#
# The spike lives outside the workspace on purpose (see spikes/ktls/Cargo.toml),
# so this script builds it itself. Nothing in the engine depends on it.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SPIKE="${ROOT}/spikes/ktls"
BIN="${SPIKE}/target/release/ktls-spike"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

"${ROOT}/scripts/check-ktls-available.sh" >/dev/null 2>&1 || {
  echo "SKIPPED, NOT PASSED: this kernel cannot offload TLS." >&2
  echo "Run scripts/check-ktls-available.sh for the reason and the fix." >&2
  exit 2
}

command -v cargo >/dev/null || { echo "cargo is not installed" >&2; exit 2; }
if [[ ! -x "${BIN}" ]]; then
  echo "building the spike (outside the workspace, so this is its own build)..."
  ( cd "${SPIKE}" && cargo build --release ) || exit 2
fi

# ---------------------------------------------------------------------------
# 1. The spike's own assertions
# ---------------------------------------------------------------------------
echo "== the spike's own checks =="
"${BIN}" > "${TMP}/plain.out" 2> "${TMP}/plain.err"
plain_rc=$?
grep -E '^(PASS|FAIL|NOTE|SPIKE) ' "${TMP}/plain.out"

rc=0
summary="$(grep -E '^SPIKE ' "${TMP}/plain.out" | head -1)"
if [[ "${plain_rc}" -ne 0 || "${summary}" != *"fail 0" ]]; then
  echo "FAIL: the spike did not pass its own checks (${summary:-no summary}, exit ${plain_rc})" >&2
  rc=1
fi

# ---------------------------------------------------------------------------
# 2. What the thread driving the offloaded socket actually called
# ---------------------------------------------------------------------------
command -v strace >/dev/null || {
  echo
  echo "SKIPPED, NOT PASSED: strace is not installed, so the syscall half checked nothing." >&2
  echo "CLAUDE.md §10: a green result that was inferred rather than observed is not a result." >&2
  exit 2
}

# Syscalls that mean the thread left user space to wait.
SLEEPERS='epoll_wait|epoll_pwait|epoll_pwait2|poll|ppoll|select|pselect6|futex|nanosleep|clock_nanosleep|sched_yield|io_uring_enter'

# Syscall names made between the two markers by the thread that wrote them.
steady_syscalls() {
  local arm="$1" want="$2"
  local out="${TMP}/${arm}.out" err="${TMP}/${arm}.err" tr="${TMP}/${arm}.trace"

  KTLS_SPIKE_WAIT="${3:-}" strace -f -o "${tr}" "${BIN}" > "${out}" 2> "${err}"

  # Read the arm back out of the program rather than trusting the variable we
  # just set. check-no-kernel-sleep.sh carries the incident this rule comes from.
  local ran
  ran="$(grep -oE '^wait: [a-z]+' "${out}" | head -1 | cut -d' ' -f2)"
  if [[ "${ran}" != "${want}" ]]; then
    echo "the spike ran wait '${ran}' when '${want}' was asked for" >&2
    return 1
  fi

  awk -v sleepers="${SLEEPERS}" '
    /write\(2, "MARK steady-state begin/ { tid = $1; inside = 1; next }
    /write\(2, "MARK steady-state end/   { inside = 0 }
    inside && $1 == tid {
      line = $0
      sub(/^[0-9]+[ \t]+/, "", line)
      if (match(line, /^[a-z_0-9]+/)) print substr(line, RSTART, RLENGTH)
    }
  ' "${tr}" | sort | uniq -c | sort -rn
}

echo
echo "== GREEN half: the spin loop the engine actually uses =="
green="$(steady_syscalls green spin "")" || exit 1
echo "${green}"
green_sleep="$(echo "${green}" | grep -cE " (${SLEEPERS})$" || true)"
# Zero blocking calls means "did not block" only if the thread separately proves
# it RAN. The allocation benches learned this rule the hard way.
green_io="$(echo "${green}" | grep -cE ' (read|recvfrom|write|sendto)$' || true)"

echo
echo "== RED half: the same loop with poll(2) in front of the read =="
red="$(steady_syscalls red poll "poll")" || exit 1
echo "${red}"
red_sleep="$(echo "${red}" | grep -cE " (${SLEEPERS})$" || true)"

echo
if [[ "${green_io}" -eq 0 ]]; then
  echo "FAIL: the traced thread made no socket calls in the steady-state region," >&2
  echo "      so an absence of blocking calls proves nothing." >&2
  rc=1
elif [[ "${green_sleep}" -ne 0 ]]; then
  echo "FAIL: the offloaded data path left user space to wait:" >&2
  echo "${green}" | grep -E " (${SLEEPERS})$" >&2
  rc=1
else
  echo "GREEN ok — read/write only; no blocking call on the offloaded data path"
fi

if [[ "${red_sleep}" -eq 0 ]]; then
  echo "FAIL: KTLS_SPIKE_WAIT=poll did NOT trip the check, so the check cannot fail" >&2
  echo "      and the green half above means nothing." >&2
  rc=1
else
  echo "RED   ok — poll arm trips it: $(echo "${red}" | grep -E " (${SLEEPERS})$" | tr -s ' ' | paste -sd' ' -)"
fi

exit "${rc}"
