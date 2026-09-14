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
#
# `[2026-09-13]` step 6b of docs/plans/2026-09-04-tls.md, Sửa 6: a third run,
# `--mode hft --tls ktls`, arms this same trace for kTLS — the non-negotiable
# is about the engine thread, not about which transport is under it, and
# nothing before this traced a TLS arm at all. A fourth run, `--tls
# userspace`, asserts nothing about syscalls; it exists only to prove the
# `tls:` read-back line actually distinguishes the two arms, because a line
# that always said "kernel" would make run 3's TLS assertion worthless. A
# binary built without the `tls` feature refuses `--tls ktls` outright
# (tools/w2w/src/main.rs), so that case is SKIPPED, NOT PASSED rather than
# silently read as "no TLS syscalls" — CLAUDE.md §10.
#
# `[2026-09-14]` step A3b of docs/plans/2026-09-04-the-second-linux-desk.md:
# `W2W_EXTRA`, appended to every w2w invocation below, so the same four runs can
# be made with `--wire-timestamps` on. Unset or empty it expands to no words,
# and every command line is the one this script ran before. Set, e.g.
# `W2W_EXTRA="--wire-timestamps --nic lo --observer-core 2"`, it asks whether
# the engine thread's syscall set survives the tap and the error-queue reader.
# Every run must then print the wire arm's own lines, or the script fails
# (`wire_arm_ran`): an arm that is only assumed to have run is the plain arm.
#
# **That arm runs inside a user namespace**, Sửa 2 of the plan (Điều 1, R1):
# `strace` is an unprivileged tracer, and a program exec'd under one is never
# given file capabilities (security/commoncap.c, `cap_bprm_creds_from_file`),
# so a `setcap`'d w2w lands without `CAP_NET_RAW` and refuses to run without
# its tap. So when `W2W_EXTRA` holds `--wire-timestamps` the script runs itself
# again under `unshare -Urn`: root of a new user namespace with its own network
# namespace, `lo` brought up, `CAP_NET_RAW` over that `lo` (af_packet.c,
# `ns_capable`) — no `setcap`, no `sudo`, and the kernel path through that `lo`
# is the path through any other. `FIXBOLT_W2W_USERNS` marks the inner run.
# **When the namespace is refused** — Ubuntu >= 24.04's AppArmor refuses it to
# a process without a profile that allows it — the script says SKIPPED, NOT
# PASSED and exits 2, naming the sysctl; it never reads as green. Reversal:
# `aa-exec -p unconfined -- env W2W_EXTRA=... scripts/check-no-kernel-sleep.sh`
# must exit 2 with that sentence. A namespace has no `enp9s0`: tracing on a real
# NIC is `sudo -n strace -f -u "$USER"` at the desk (docs/hft-playbook.md §6).
set -uo pipefail

# `W2W_EXTRA` with `--wire-timestamps` runs this whole script again inside a user
# namespace — see the header. Before anything else, so the re-run makes its own
# temporary directory and nothing is left behind by `exec`.
if [[ "${W2W_EXTRA:-}" == *--wire-timestamps* && -z "${FIXBOLT_W2W_USERNS:-}" ]]; then
  if ! unshare -Urn true 2>/dev/null; then
    echo "SKIPPED, NOT PASSED: W2W_EXTRA asks for --wire-timestamps, whose arm runs inside a" >&2
    echo "user namespace (unshare -Urn), and this process is not allowed to create one." >&2
    echo "On Ubuntu >= 24.04 AppArmor refuses it to a process without a profile that allows" >&2
    echo "userns. For this boot only: sudo -n sysctl -w kernel.apparmor_restrict_unprivileged_userns=0" >&2
    echo "CLAUDE.md §10: a green result that was inferred rather than observed is not a result." >&2
    exit 2
  fi
  echo "== W2W_EXTRA='${W2W_EXTRA}': running inside a user namespace (unshare -Urn), on its own lo =="
  # shellcheck disable=SC2016 # single-quoted on purpose: expanded by the inner shell.
  exec unshare -Urn env FIXBOLT_W2W_USERNS=1 bash -c \
    'ip link set lo up || { echo "FAIL: could not bring up lo inside the user namespace" >&2; exit 1; }; exec bash "$0" "$@"' \
    "${BASH_SOURCE[0]}" "$@"
fi

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

# `[2026-09-14]` **The wire arm is read back, not assumed from `W2W_EXTRA`** — the
# `ran_mode` lesson again. A w2w that stopped acting on `--wire-timestamps`
# would run every arm above as the plain one, and this script would be green
# about an instrument that never started. So when `W2W_EXTRA` holds the flag,
# each run's output must carry the observer's `wire-timestamps: <nic>, port`
# line and both `hw-rx-missing N of M` and `hw-tx-missing N of M` lines, M above
# zero; on `lo`, which has no hardware clock, N must equal M. With no flag this
# returns 0 and reads nothing.
wire_arm_ran() {
  local out="$1" nic line missing of
  [[ "${W2W_EXTRA:-}" == *--wire-timestamps* ]] || return 0
  nic="$(sed -nE 's/(^|.* )--nic +([^ ]+).*/\2/p' <<<"${W2W_EXTRA}")"
  if ! grep -qE "^wire-timestamps: ${nic}, port [0-9]+$" "${out}"; then
    echo "FAIL: W2W_EXTRA asks for --wire-timestamps and w2w printed no 'wire-timestamps: ${nic}, port' line — the wire arm did not run" >&2
    return 1
  fi
  for line in hw-rx-missing hw-tx-missing; do
    read -r missing of <<<"$(awk -v l="${line}" '$1 == l && $3 == "of" && $2 ~ /^[0-9]+$/ && $4 ~ /^[0-9]+$/ { print $2, $4; exit }' "${out}")"
    if [[ -z "${of:-}" || "${of}" -eq 0 ]]; then
      echo "FAIL: W2W_EXTRA asks for --wire-timestamps and w2w printed no '${line} N of M' line with M above zero — the observer's report did not run" >&2
      return 1
    fi
    if [[ "${nic}" == "lo" && "${missing}" -ne "${of}" ]]; then
      echo "FAIL: ${line} reads ${missing} of ${of} on lo, which has no hardware clock — every stamp must be counted missing" >&2
      return 1
    fi
  done
}

# Syscalls, by name, that the engine thread made during one run.
engine_syscalls() {
  local out="${TMP}/out.$1" tr="${TMP}/tr.$1" tid
  # shellcheck disable=SC2086 # deliberate: $2 carries zero or more w2w flags
  # as one string (e.g. "--mode hft --tls ktls") and must split on spaces.
  # shellcheck disable=SC2086 # W2W_EXTRA likewise: zero or more flags.
  strace -f -o "${tr}" "${BIN}" --messages 300 --warmup 50 --hold-ms 400 ${2:-} ${W2W_EXTRA:-} \
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
  wire_arm_ran "${out}" || return 1
  echo "${tid}" > "${TMP}/tid.$1"
  awk -v t="${tid}" '$1==t {print $2}' "${tr}" | grep -oE '^[a-z_0-9]+' | sort | uniq -c | sort -rn
}

# What `tls:` says in a run's captured output — read back the same way
# `ran_mode` is, inside `engine_syscalls`, never assumed from the flag that was
# passed.
tls_seen() {
  grep -oE '^tls: [a-z]+' "$1" | head -1 | cut -d' ' -f2
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

echo
echo "== TLS arm: hft mode with kTLS must also pass, and read back 'kernel' =="
# `engine_syscalls` itself fails (its "w2w failed:" branch, `return 1`) before
# it ever checks `ran_mode`, because the refusal in tools/w2w/src/main.rs exits
# non-zero before printing the `mode:` line at all. So a missing `tls` feature
# and a real failure both come back here as a non-zero status from the same
# call; what tells them apart is the refusal's own sentence, still sitting in
# the run's captured output (`${TMP}/out.hft`) either way.
ktls="$(engine_syscalls hft "--mode hft --tls ktls")"
ktls_status=$?
if [[ "${ktls_status}" -ne 0 ]]; then
  # shellcheck disable=SC2016 # single-quoted on purpose: a backtick inside
  # double quotes would be a command substitution, not the literal character
  # main.rs's refusal message actually prints.
  if grep -q 'needs `--features tls`' "${TMP}/out.hft" 2>/dev/null; then
    echo "TLS arm SKIPPED, NOT PASSED: this build has no TLS transport (needs \`--features tls\`)." >&2
    echo "CLAUDE.md §10: a green result that was inferred rather than observed is not a result." >&2
    exit 2
  fi
  echo "FAIL: --tls ktls could not be run at all (see above)" >&2
  rc=1
else
  echo "${ktls}" | head -8
  ktls_found="$(echo "${ktls}" | grep -cE " (${SLEEPERS})$" || true)"
  ktls_ran="$(echo "${ktls}" | grep -cE ' (recvfrom|sendto)$' || true)"
  ktls_tls="$(tls_seen "${TMP}/out.hft")"

  if [[ "${ktls_ran}" -eq 0 ]]; then
    echo "FAIL: the engine thread made no socket calls under --tls ktls, so it proved nothing" >&2
    rc=1
  elif [[ "${ktls_found}" -ne 0 ]]; then
    echo "FAIL: the engine thread slept in the kernel under --tls ktls:" >&2
    echo "${ktls}" | grep -E " (${SLEEPERS})$" >&2
    rc=1
  elif [[ "${ktls_tls}" != "kernel" ]]; then
    echo "FAIL: --tls ktls ran tls '${ktls_tls:-<none>}' when 'kernel' was required" >&2
    rc=1
  else
    echo "GREEN ok — --tls ktls: no blocking call, socket calls present, tls: kernel"
  fi

  echo
  echo "== TLS arm: --tls userspace must read back 'userspace' =="
  # No syscall assertion for this run — only the read-back line matters here.
  engine_syscalls hft "--mode hft --tls userspace" >/dev/null || exit 1
  userspace_tls="$(tls_seen "${TMP}/out.hft")"
  if [[ "${userspace_tls}" != "userspace" ]]; then
    echo "FAIL: --tls userspace ran tls '${userspace_tls:-<none>}' when 'userspace' was required" >&2
    rc=1
  else
    echo "GREEN ok — --tls userspace reads back tls: userspace (the read-back line distinguishes arms)"
  fi
fi

exit "${rc}"
