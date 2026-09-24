#!/usr/bin/env bash
# ADR-0190 decision 7, phase 4 row 5: **a blocked `io_uring` is refused at
# startup, named, and never a fallback** — under the sysctl, on a real kernel.
#
# `crates/engine/tests/uring.rs` proves the seccomp half on the test's own
# thread with no privilege. The sysctl half needs root, so it is this script
# and not an `#[ignore]` test: a test that CI never runs is a test no gate
# accounts for (R3 of `scripts/check-feature-gated-tests-ran.sh`).
#
# It sets `kernel.io_uring_disabled=2`, runs `tools/w2w --transport uring`,
# and requires **exit non-zero with `UringRefused::Disabled`'s sentence**
# (`kernel.io_uring_disabled = 2`) — an exit 0 would mean the run carried on
# over `read(2)` under the `uring` label, or the refusal went unnamed. The old
# value is **always restored** (`trap` on EXIT, INT and TERM) and read back
# afterwards; a value that did not come back is a FAIL, printed.
#
# **Desk only.** It needs passwordless `sudo -n` (the owner's desktop has it;
# `CLAUDE.md` global, "The desktop has passwordless sudo"), a kernel with the
# sysctl (6.6+), and a `w2w` built with `--features io-uring`. Missing any of
# them is SKIPPED, NOT PASSED, exit 2 — never green. It is not run by CI:
# changing a machine-wide sysctl on a shared runner is not this script's to do.
#
# **What it cannot see:** the value 1 (`kernel.io_uring_group`), whose
# refusal depends on this user's groups; the classification of 1 is covered by
# the pure table in `the_refusal_is_classified_by_errno_and_sysctl`.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ROOT}/target/release/w2w"
SYSCTL=/proc/sys/kernel/io_uring_disabled
TMP="$(mktemp -d)"

skip() {
  echo "SKIPPED, NOT PASSED: $*" >&2
  echo "CLAUDE.md §10: a green result that was inferred rather than observed is not a result." >&2
  rm -rf "${TMP}"
  exit 2
}

[[ "$(uname -s)" == "Linux" ]] || skip "io_uring is Linux-only"
[[ -r "${SYSCTL}" ]] || skip "${SYSCTL} does not exist (kernel older than 6.6)"
# By absolute path: root resolves a bare name against `secure_path`, never this
# PATH (ADR-0093 decision 2, `scripts/check-sudo-names-what-root-can-find.sh`
# R1); `/usr/bin/true` is coreutils on every merged-/usr Debian or Ubuntu.
sudo -n /usr/bin/true 2>/dev/null || skip "root is not available here without a password; this is a desk-only script"
[[ -x "${BIN}" ]] || skip "build it first: cargo build --release -p fixbolt-w2w --features io-uring"

old="$(cat "${SYSCTL}")"
echo "kernel.io_uring_disabled before: ${old}"

restore() {
  sudo -n sysctl -q -w "kernel.io_uring_disabled=${old}" >/dev/null 2>&1
  echo "kernel.io_uring_disabled after restore: $(cat "${SYSCTL}")"
  rm -rf "${TMP}"
}
trap restore EXIT
trap 'exit 130' INT TERM

# The binary must be able to run the arm at all before the sysctl moves, or a
# refusal below could be the build's rather than the kernel's.
if ! "${BIN}" --mode hft --transport uring --messages 10 --warmup 2 >"${TMP}/before.out" 2>&1; then
  # shellcheck disable=SC2016 # the literal backticks main.rs prints.
  if grep -q 'needs `--features io-uring`' "${TMP}/before.out"; then
    skip "this w2w has no io_uring transport; build it with --features io-uring"
  fi
  echo "FAIL: --transport uring does not run even with io_uring enabled:" >&2
  tail -5 "${TMP}/before.out" >&2
  exit 1
fi
grep -E '^transport:' "${TMP}/before.out"

sudo -n sysctl -q -w kernel.io_uring_disabled=2 || { echo "FAIL: could not set the sysctl" >&2; exit 1; }
echo "kernel.io_uring_disabled set: $(cat "${SYSCTL}")"

timeout 60 "${BIN}" --mode hft --transport uring --messages 10 --warmup 2 >"${TMP}/under.out" 2>&1
status=$?
echo "w2w --transport uring under the sysctl exited ${status}; its lines naming io_uring:"
grep -E 'io_uring|refused' "${TMP}/under.out" | head -5

rc=0
if [[ "${status}" -eq 0 ]]; then
  echo "FAIL: w2w exited 0 with io_uring disabled — it ran something other than the ring, or said nothing" >&2
  rc=1
elif [[ "${status}" -eq 124 ]]; then
  echo "FAIL: w2w hung for 60 s instead of refusing" >&2
  rc=1
elif ! grep -q 'kernel.io_uring_disabled = 2' "${TMP}/under.out"; then
  echo "FAIL: w2w failed, but not with UringRefused::Disabled's sentence (kernel.io_uring_disabled = 2)" >&2
  tail -5 "${TMP}/under.out" >&2
  rc=1
else
  echo "ok — refused at startup, named: Disabled { sysctl: 2 }"
fi

restore
trap - EXIT
if [[ "$(cat "${SYSCTL}")" != "${old}" ]]; then
  echo "FAIL: kernel.io_uring_disabled did not come back to ${old}" >&2
  rc=1
fi
exit "${rc}"
