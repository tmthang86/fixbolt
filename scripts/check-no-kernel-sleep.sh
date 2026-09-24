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
#
# `[2026-09-23]` ADR-0152, row W of docs/plans/2026-09-23-phase-3-found-defects.md
# (Sửa 2): **only the serving window is judged.** Non-negotiable 4 governs the
# engine thread from the start of its serving loop to that loop's return;
# setup before it and teardown after it may block, and teardown must, to let
# the `--journal file-async` and `--log file` writers drain. tools/w2w marks
# the window with two lookups of paths that do not exist,
# `/fixbolt-w2w-serve-open` and `/fixbolt-w2w-serve-close` (its module note
# "The serving window"); `serving_window` finds them on the engine tid **by the
# path string**, never by the syscall's name (`statx` or `newfstatat`, by
# libc), and every count below — sleepers and socket calls alike — is taken
# strictly between them. A marker missing or seen twice is a FAIL: nothing can
# be judged. A sleeper outside the window is PRINTED, "outside the serving
# window, not judged:", and never fails the run. **What this no longer sees:**
# a sleeper added to setup or teardown fails nothing — it is only printed; and
# the window is two lines in a tool, so a w2w change that moved either marker
# moves what is judged. It is not cut at the last socket call: a window defined
# by what it judges would miss a sleep after the last message.
#
# `[2026-09-24]` ADR-0191, phase 4 row 5: **`io_uring_enter` is judged by its
# third argument, `min_complete`.** The `hft` arm over `io_uring` enters the
# kernel once per idle turn with `min_complete = 0`, which returns without
# waiting; a `min_complete` of 1 or more (with `GETEVENTS`) is a wait.
# `engine_syscalls` names each call `io_uring_enter_nowait`, `_wait`, or
# `_unparsed` — on the line that carries the arguments, a whole call or its
# `<unfinished ...>` half, never on a `<... resumed>` half — and `SLEEPERS`
# lists `_wait` and `_unparsed`: an argument this script cannot read FAILS the
# run (the gate fails closed). Every kernel and TLS run is unchanged, because
# none of them makes an `io_uring_enter`. Two runs are added: `--mode hft
# --transport uring` must pass **and** show `io_uring_enter_nowait` above zero
# and a `transport: uring ... cqes=` line above zero (the ring path ran);
# `--mode standard --transport uring` must trip it with `io_uring_enter_wait`.
# The SQPOLL arm runs only where `FIXBOLT_SQPOLL_CORE` names the SQ thread's
# core (`FIXBOLT_SQPOLL_ALLOW_UNISOLATED=1` adds `--allow-unisolated`, and says
# so); otherwise it prints SKIPPED, NOT PASSED. A binary built without
# `--features io-uring` refuses `--transport uring`, and this script then exits
# 2, SKIPPED, NOT PASSED — never green. **What it cannot see:** a wait inside
# the SQPOLL kernel thread, which is not the engine thread; and a `strace`
# whose argument format changes — that reads `_unparsed` and fails, it does
# not pass.
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
#
# ADR-0191: `io_uring_enter` by name is no longer here. `NAME_OF` below splits
# it by `min_complete`, and the two names that can mean a wait are listed.
SLEEPERS='epoll_wait|epoll_pwait|epoll_pwait2|poll|ppoll|select|pselect6|futex|nanosleep|clock_nanosleep|sched_yield|io_uring_enter_wait|io_uring_enter_unparsed'

# An awk function: the syscall's name for one trace line, with ADR-0191's
# split of `io_uring_enter` by its third argument. Every other line gives `$2`,
# from which the callers cut the name exactly as before — a `<... resumed>`
# half starts with `<` and is cut to nothing, so a split call is counted once,
# on the line that carries its arguments.
# shellcheck disable=SC2016 # awk's own `$0`/`$2`, not the shell's.
NAME_OF='
function name_of(   line, args, n, part, mc) {
  line = $0
  sub(/^[0-9]+ +/, "", line)
  if (line !~ /^io_uring_enter\(/) return $2
  args = line
  sub(/^io_uring_enter\(/, "", args)
  n = split(args, part, ",")
  mc = (n >= 3) ? part[3] : ""
  gsub(/^ +| +$/, "", mc)
  if (mc ~ /^[0-9]+$/) return (mc + 0 == 0) ? "io_uring_enter_nowait" : "io_uring_enter_wait"
  return "io_uring_enter_unparsed"
}'

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

# The two marker paths tools/w2w looks up around its serving loop (`SERVE_OPEN`
# and `SERVE_CLOSE` in tools/w2w/src/main.rs). Matched with the closing quote
# strace prints after a path, so a longer path that merely starts the same way
# is not a marker.
SERVE_OPEN='/fixbolt-w2w-serve-open"'
SERVE_CLOSE='/fixbolt-w2w-serve-close"'

# Prints "<open line> <close line>" — the trace's line numbers of the engine
# tid's two markers — or fails, naming how many of each it saw. Exactly one
# of each, open first, or nothing can be judged.
serving_window() {
  local tr="$1" tid="$2" window n_open n_close open close
  window="$(awk -v t="${tid}" -v o="${SERVE_OPEN}" -v c="${SERVE_CLOSE}" '
    $1 == t && index($0, o) { no++; lo = NR }
    $1 == t && index($0, c) { nc++; lc = NR }
    END { print no + 0, nc + 0, lo + 0, lc + 0 }' "${tr}")"
  read -r n_open n_close open close <<<"${window}"
  if [[ "${n_open}" -ne 1 || "${n_close}" -ne 1 || "${open}" -ge "${close}" ]]; then
    echo "FAIL: the serving window is not marked exactly once — nothing can be judged" >&2
    echo "      (engine tid ${tid}: ${n_open} serve-open, ${n_close} serve-close marker(s); tools/w2w must make one of each, open first)" >&2
    return 1
  fi
  echo "${open} ${close}"
}

# Syscalls, by name, that the engine thread made inside its serving window
# during one run. Sleepers outside the window are printed, not returned.
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
  local open close outside
  read -r open close <<<"$(serving_window "${tr}" "${tid}")"
  [[ -n "${close:-}" ]] || return 1
  # ADR-0152 decision 3: printed, never hidden, never judged.
  outside="$(awk -v t="${tid}" -v a="${open}" -v b="${close}" "${NAME_OF}"'$1==t && (NR<a || NR>b) {print name_of()}' "${tr}" \
    | grep -oE '^[a-z_0-9]+' | grep -xE "${SLEEPERS}" | sort | uniq -c | sort -rn | tr -s ' ' | paste -sd',' - || true)"
  if [[ -n "${outside}" ]]; then
    echo "outside the serving window, not judged:${outside} (${2:-})" >&2
  fi
  awk -v t="${tid}" -v a="${open}" -v b="${close}" "${NAME_OF}"'$1==t && NR>a && NR<b {print name_of()}' "${tr}" \
    | grep -oE '^[a-z_0-9]+' | sort | uniq -c | sort -rn
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
echo "== io_uring arm (ADR-0191): hft over the ring must pass, entering without waiting =="
# Whether this run's output carries `transport: uring ... cqes=<n>`, n > 0 —
# the ring path ran, read back from the engine and never assumed from the flag.
uring_ran() {
  grep -qE '^transport: uring .*cqes=[1-9][0-9]*( |$)' "$1"
}
# Written before anything runs: the refusal a build without the feature prints.
uring_refused() {
  # shellcheck disable=SC2016 # the literal backticks main.rs prints.
  grep -q 'needs `--features io-uring`' "$1" 2>/dev/null
}
ur="$(engine_syscalls hft "--mode hft --transport uring")"
ur_status=$?
if [[ "${ur_status}" -ne 0 ]]; then
  if uring_refused "${TMP}/out.hft"; then
    echo "io_uring arm SKIPPED, NOT PASSED: this build has no io_uring transport (needs \`--features io-uring\`)." >&2
    echo "CLAUDE.md §10: a green result that was inferred rather than observed is not a result." >&2
    exit 2
  fi
  echo "FAIL: --mode hft --transport uring could not be run at all (see above)" >&2
  tail -5 "${TMP}/out.hft" >&2
  rc=1
else
  echo "${ur}" | head -8
  ur_found="$(echo "${ur}" | grep -cE " (${SLEEPERS})$" || true)"
  ur_nowait="$(echo "${ur}" | awk '$2 == "io_uring_enter_nowait" { print $1 }')"
  ur_ran="$(echo "${ur}" | grep -cE ' (recvfrom|sendto)$' || true)"
  if ! uring_ran "${TMP}/out.hft"; then
    echo "FAIL: --transport uring printed no 'transport: uring ... cqes=' line above zero — the ring path did not run" >&2
    grep -E '^transport:' "${TMP}/out.hft" >&2
    rc=1
  elif [[ "${ur_ran}" -eq 0 ]]; then
    echo "FAIL: the engine thread made no socket calls under --transport uring, so it proved nothing" >&2
    rc=1
  elif [[ "${ur_found}" -ne 0 ]]; then
    echo "FAIL: the engine thread slept in the kernel:" >&2
    echo "${ur}" | grep -E " (${SLEEPERS})$" >&2
    rc=1
  elif [[ -z "${ur_nowait}" || "${ur_nowait}" -eq 0 ]]; then
    echo "FAIL: --transport uring made no io_uring_enter_nowait inside the serving window — the reaping turn did not run" >&2
    rc=1
  else
    echo "GREEN ok — hft over io_uring: no blocking call, io_uring_enter_nowait ${ur_nowait}, $(grep -E '^transport:' "${TMP}/out.hft")"
  fi

  echo
  echo "== io_uring arm (ADR-0191): standard over the ring must trip it, with io_uring_enter_wait =="
  urs="$(engine_syscalls standard "--mode standard --transport uring")" || exit 1
  echo "${urs}" | head -8
  if ! uring_ran "${TMP}/out.standard"; then
    echo "FAIL: --mode standard --transport uring printed no 'transport: uring ... cqes=' line above zero" >&2
    rc=1
  elif ! echo "${urs}" | grep -qE ' io_uring_enter_wait$'; then
    echo "FAIL: --mode standard --transport uring did NOT trip the check with io_uring_enter_wait, so the ring's wait is invisible to it" >&2
    rc=1
  else
    echo "RED   ok — standard over io_uring trips it: $(echo "${urs}" | grep -E " (${SLEEPERS})$" | tr -s ' ' | paste -sd' ' -)"
  fi

  echo
  echo "== io_uring arm: SQPOLL (hft), only where FIXBOLT_SQPOLL_CORE names the SQ thread's core =="
  if [[ -z "${FIXBOLT_SQPOLL_CORE:-}" ]]; then
    echo "SQPOLL arm SKIPPED, NOT PASSED: FIXBOLT_SQPOLL_CORE is not set, so it was not run."
  else
    sq_flags="--mode hft --transport uring --uring-arm sqpoll --sqpoll-core ${FIXBOLT_SQPOLL_CORE}"
    if [[ "${FIXBOLT_SQPOLL_ALLOW_UNISOLATED:-}" == 1 ]]; then
      sq_flags="${sq_flags} --allow-unisolated"
      echo "(FIXBOLT_SQPOLL_ALLOW_UNISOLATED=1: the SQ core is not held to isolcpus — not a DESIGN.md §9 run)"
    fi
    sq="$(engine_syscalls hft "${sq_flags}")" || { echo "FAIL: the SQPOLL arm could not be run (see above)" >&2; exit 1; }
    echo "${sq}" | head -8
    if ! grep -qE '^transport: uring arm=sqpoll .*cqes=[1-9]' "${TMP}/out.hft"; then
      echo "FAIL: the SQPOLL arm printed no 'transport: uring arm=sqpoll ... cqes=' line above zero" >&2
      rc=1
    elif echo "${sq}" | grep -qE " (${SLEEPERS})$"; then
      echo "FAIL: the engine thread slept in the kernel under SQPOLL:" >&2
      echo "${sq}" | grep -E " (${SLEEPERS})$" >&2
      rc=1
    else
      echo "GREEN ok — SQPOLL arm: no blocking call on the engine thread, $(grep -E '^transport:' "${TMP}/out.hft")"
    fi
  fi
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
