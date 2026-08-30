#!/usr/bin/env bash
# CLAUDE.md §2 non-negotiable 4: THE ENGINE THREAD NEVER SLEEPS IN THE KERNEL.
#
# STATUS.md open item 15. This rule had no machine check for three days and two
# attempts to build one failed — both in ways worth remembering, because both
# reported success:
#
#   * `dtruss` is refused by macOS SIP, so it never ran at all.
#   * reading undefined symbols out of the compiled rlib passed WITH a
#     `thread::sleep` in the loop, because `Engine` and `serve` are generic and
#     are never code-generated into a library.
#
# So this traces a concrete binary — tools/w2w — on Linux, and attributes the
# syscalls to the engine thread by tid rather than to the process, because the
# client on the main thread blocks on purpose and would mask everything.
#
# **It runs the binary twice and REQUIRES THE SECOND RUN TO FAIL.** A guard that
# has only ever been seen passing is not known to work, and this one has two
# predecessors that were exactly that.
#
# `[2026-08-30]` The red half is now `--mode standard`, and it used to be
# `--park` (`sched_yield`). The reason for moving it: **nobody writes
# `sched_yield` into an engine by accident.** A blocking readiness call is what
# an actual regression looks like — somebody reaches for `poll` because it is
# the obvious way to wait — so the red half now trips on the syscall a real
# mistake would make rather than on one nothing would.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ROOT}/target/release/w2w"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

command -v strace >/dev/null || {
  echo "SKIPPED, NOT PASSED: strace is not installed, so nothing was checked." >&2
  echo "CLAUDE.md §10: a green result that was inferred rather than observed is not a result." >&2
  exit 2
}
[[ -x "${BIN}" ]] || { echo "build it first: cargo build -p fixbolt-w2w --release" >&2; exit 2; }

# Syscalls that mean the thread left user space to wait. `accept4`, `recvfrom`
# and `sendto` are the socket path and are non-blocking; they are not here.
SLEEPERS='epoll_wait|epoll_pwait|epoll_pwait2|poll|ppoll|select|pselect6|futex|nanosleep|clock_nanosleep|sched_yield|io_uring_enter'

# Syscalls, by name, that the engine thread made during one run.
engine_syscalls() {
  local out="${TMP}/out.$1" tr="${TMP}/tr.$1" tid
  strace -f -o "${tr}" "${BIN}" --messages 300 --warmup 50 --hold-ms 400 ${2:-} \
    > "${out}" 2>&1 || { echo "w2w failed:" >&2; tail -5 "${out}" >&2; return 1; }
  # **Read the mode back rather than trusting the flag.** `[measured
  # 2026-08-30]` `--mode standard` was once accepted, printed its banner, and
  # ran hft anyway, because the `#[cfg(feature = ...)]` selecting it named a
  # feature the binary's own manifest did not declare. A gate that assumes its
  # own arm ran is a gate that can be green about the wrong binary.
  local ran_mode
  ran_mode="$(grep -oE '^mode: [a-z]+' "${out}" | head -1 | cut -d' ' -f2)"
  if [[ "${ran_mode}" != "$1" ]]; then
    echo "w2w ran mode '${ran_mode}' when '$1' was asked for" >&2
    return 1
  fi
  tid="$(grep -oE 'engine-tid: [0-9]+' "${out}" | head -1 | grep -oE '[0-9]+')"
  [[ -n "${tid}" ]] || { echo "no engine-tid in output" >&2; return 1; }
  echo "${tid}" > "${TMP}/tid.$1"
  awk -v t="${tid}" '$1==t {print $2}' "${tr}" | grep -oE '^[a-z_0-9]+' | sort | uniq -c | sort -rn
}

echo "== GREEN half: hft mode, which is what DESIGN.md D8 describes =="
spin="$(engine_syscalls hft "--mode hft")" || exit 1
echo "${spin}" | head -8
found="$(echo "${spin}" | grep -cE " (${SLEEPERS})$" || true)"

# A count of zero means "did not sleep" only if something separately proves the
# thread RAN. The same rule the allocation benches learned the hard way.
ran="$(echo "${spin}" | grep -cE ' (recvfrom|sendto)$' || true)"

echo
echo "== RED half: standard mode, the same loop blocking on readiness =="
park="$(engine_syscalls standard "--mode standard")" || exit 1
echo "${park}" | head -8
park_found="$(echo "${park}" | grep -cE " (${SLEEPERS})$" || true)"

echo
rc=0
if [[ "${ran}" -eq 0 ]]; then
  echo "FAIL: the engine thread made no socket calls, so it proved nothing" >&2
  rc=1
elif [[ "${found}" -ne 0 ]]; then
  echo "FAIL: the engine thread slept in the kernel:" >&2
  echo "${spin}" | grep -E " (${SLEEPERS})$" >&2
  rc=1
else
  echo "GREEN ok — engine thread made no blocking call; it did make socket calls"
fi

if [[ "${park_found}" -eq 0 ]]; then
  echo "FAIL: --mode standard did NOT trip the check, so the check cannot fail and means nothing" >&2
  rc=1
else
  echo "RED   ok — --mode standard trips it: $(echo "${park}" | grep -E " (${SLEEPERS})$" | tr -s ' ' | paste -sd' ' -)"
fi
exit "${rc}"
