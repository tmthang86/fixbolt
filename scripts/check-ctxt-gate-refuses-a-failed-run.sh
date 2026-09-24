#!/usr/bin/env bash
# Does scripts/check-no-kernel-sleep-by-ctxt.sh report a w2w that FAILED TO RUN
# as a failure — rather than reading the empty result as `voluntary 0` and
# printing GREEN?
#
# `[2026-09-24]` It did not: `read -r a b <<<"$(run_and_read hft)" || exit 1`
# binds `|| exit 1` to `read`, which succeeds on an empty here-string, so a w2w
# that crashed in `hft` mode left both values empty, `[[ "" -ne 0 ]]` read
# false, and the gate printed `GREEN ok` and could PASS. Senior review of PR
# #109, L1; docs/reference/a-shell-read-heredoc-swallows-a-failed-functions-exit-status.md.
#
# Pure: no cargo, no real w2w. Three fake w2w binaries (bash scripts written
# below) stand in through the gate's `W2W_BIN`:
#
#   healthy         hft prints `engine-ctxt voluntary 0` and exits 0; standard
#                   prints 300 and w2w's own assertion line and exits 101.
#                   The gate must PASS — this case keeps the harness from
#                   being a check that is always red.
#   crash-hft       hft exits 101 with nothing on stdout. The gate must exit 1
#                   and name the hft half.
#   crash-standard  standard exits 101 with nothing on stdout. The gate must
#                   exit 1 and name the standard half.
#
# WHAT IT CANNOT SEE: anything about a real engine thread. The real gate run
# (`scripts/check-no-kernel-sleep-by-ctxt.sh` against target/release/w2w) is
# what proves non-negotiable 4; this proves only that its failure path reads.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
gate="${here}/check-no-kernel-sleep-by-ctxt.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

fake="${TMP}/w2w"
cat >"${fake}" <<'FAKE'
#!/usr/bin/env bash
mode=""
while [[ $# -gt 0 ]]; do
  [[ "$1" == --mode ]] && mode="$2"
  shift
done
case "${FAKE_W2W}:${mode}" in
  crash-hft:hft | crash-standard:standard)
    echo "thread 'main' panicked: a fake w2w that fails to run" >&2
    exit 101
    ;;
esac
echo "mode: ${mode}"
if [[ "${mode}" == hft ]]; then
  echo "engine-ctxt voluntary 0"
  exit 0
fi
echo "engine-ctxt voluntary 300"
echo "standard: engine thread made 300 voluntary context switches, expected 0"
exit 101
FAKE
chmod +x "${fake}"

rc=0
# case, the exit the gate must give, a line its output must hold.
check() {
  local name="$1" want_exit="$2" want_line="$3" out status
  out="$(FAKE_W2W="${name}" W2W_BIN="${fake}" bash "${gate}" 2>&1)"
  status=$?
  if [[ "${status}" -ne "${want_exit}" ]]; then
    echo "FAIL ${name}: the gate exited ${status}, expected ${want_exit}" >&2
    echo "${out}" | sed 's/^/    /' >&2
    rc=1
  elif ! grep -qF -- "${want_line}" <<<"${out}"; then
    echo "FAIL ${name}: the gate exited ${status} but never said: ${want_line}" >&2
    echo "${out}" | sed 's/^/    /' >&2
    rc=1
  elif [[ "${name}" == crash-hft ]] && grep -qF "GREEN ok" <<<"${out}"; then
    echo "FAIL ${name}: the gate printed GREEN ok for a hft run that never ran" >&2
    rc=1
  else
    echo "ok   ${name}: exit ${status}, \"${want_line}\""
  fi
}

check healthy 0 "PASS"
check crash-hft 1 "FAIL: --mode hft produced no result"
check crash-standard 1 "FAIL: --mode standard produced no result"

exit "${rc}"
