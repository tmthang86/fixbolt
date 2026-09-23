#!/usr/bin/env bash
# CLAUDE.md §10 — a guard is proven by reversal, not trusted by eye. This is
# scripts/check-dict-spec-pin.sh's reversal: four arms, each stating the
# expected FAIL line before it runs, each mutating one real, tracked file
# that check-dict-spec-pin.sh reads, and each restored before the next arm
# starts.
#
# It mutates the ACTUAL shipped files — crates/dict/spec/*.xml, NOTICE,
# crates/dict/NOTICE — never a scratch copy, because check-dict-spec-pin.sh
# reads those exact paths and a copy elsewhere would prove nothing about it.
# Every mutation is backed up before it happens, and a trap restores from
# that backup on ANY exit, including a failed assertion below — a red run
# here must never leave the tree in a state a later `git status` has to
# puzzle out.
#
# Arm 1: crates/dict/spec/FIX44.xml removed          -> "missing crates/dict/spec/FIX44.xml"
# Arm 2: one byte flipped in crates/dict/spec/FIXT11.xml -> "sha256 mismatch"
# Arm 3: crates/dict/NOTICE gains a trailing line     -> "NOTICE copies differ"
# Arm 4: an extra file, FIX42.xml, added to spec/     -> "unexpected entry: FIX42.xml"
#        then removed again, which must go back to ok.
#
# Runs standalone: scripts/check-dict-spec-pin-reversal.sh
# Needs: python3 (arm 2's single-byte flip; already a build-dependency of
# other scripts in this repository, e.g. scripts/dict-diff.py).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

PIN_SCRIPT="${ROOT}/scripts/check-dict-spec-pin.sh"
SPEC_DIR="${ROOT}/crates/dict/spec"
DECOY="${SPEC_DIR}/FIX42.xml"
BACKUP="$(mktemp -d)"

# Snapshot everything this script will ever touch, once, up front. `restore`
# always copies FROM here — it never tries to "undo" one specific mutation —
# so a script that dies partway through an arm still leaves the tree exactly
# as it found it.
cp "${SPEC_DIR}/FIX44.xml" "${BACKUP}/FIX44.xml"
cp "${SPEC_DIR}/FIXT11.xml" "${BACKUP}/FIXT11.xml"
cp "${SPEC_DIR}/FIX50SP2.xml" "${BACKUP}/FIX50SP2.xml"
cp "${ROOT}/NOTICE" "${BACKUP}/NOTICE.root"
cp "${ROOT}/crates/dict/NOTICE" "${BACKUP}/NOTICE.dict"

restore() {
  cp "${BACKUP}/FIX44.xml" "${SPEC_DIR}/FIX44.xml"
  cp "${BACKUP}/FIXT11.xml" "${SPEC_DIR}/FIXT11.xml"
  cp "${BACKUP}/FIX50SP2.xml" "${SPEC_DIR}/FIX50SP2.xml"
  cp "${BACKUP}/NOTICE.root" "${ROOT}/NOTICE"
  cp "${BACKUP}/NOTICE.dict" "${ROOT}/crates/dict/NOTICE"
  rm -f "${DECOY}"
}
# Fires on every exit path — normal completion, an early `exit` from a
# failed assertion, or a signal — so the tree is never left mutated.
trap 'restore; rm -rf "${BACKUP}"' EXIT

fail=0

assert_fail_contains() {
  local name="$1" needle="$2" out status
  out="$("${PIN_SCRIPT}" 2>&1)"
  status=$?
  if [[ "${status}" -eq 0 ]]; then
    echo "check-dict-spec-pin-reversal: FAIL — ${name}: check-dict-spec-pin.sh exited 0; expected a FAIL carrying: ${needle}" >&2
    echo "${out}" >&2
    fail=1
    return
  fi
  if grep -qF "${needle}" <<<"${out}"; then
    echo "check-dict-spec-pin-reversal: ok — ${name} refused, carrying: ${needle}"
  else
    echo "check-dict-spec-pin-reversal: FAIL — ${name}: expected output to carry: ${needle}" >&2
    echo "${out}" >&2
    fail=1
  fi
}

assert_ok() {
  local name="$1"
  if "${PIN_SCRIPT}" > /dev/null 2>&1; then
    echo "check-dict-spec-pin-reversal: ok — ${name}"
  else
    echo "check-dict-spec-pin-reversal: FAIL — ${name}: check-dict-spec-pin.sh did not pass on a tree that should be clean" >&2
    fail=1
  fi
}

echo "=== arm 0: the untouched tree must be green before any reversal ==="
assert_ok "arm 0 (baseline)"

echo "=== arm 1: EXPECTED FAIL — missing crates/dict/spec/FIX44.xml ==="
rm -f "${SPEC_DIR}/FIX44.xml"
assert_fail_contains "arm 1 (missing file)" "missing crates/dict/spec/FIX44.xml"
restore

echo "=== arm 2: EXPECTED FAIL — sha256 mismatch on crates/dict/spec/FIXT11.xml ==="
python3 - "${SPEC_DIR}/FIXT11.xml" <<'PY'
import sys
path = sys.argv[1]
with open(path, "rb") as f:
    data = bytearray(f.read())
data[100] ^= 0xFF
with open(path, "wb") as f:
    f.write(data)
PY
assert_fail_contains "arm 2 (flipped byte)" "sha256 mismatch"
restore

echo "=== arm 3: EXPECTED FAIL — NOTICE copies differ ==="
printf '\ntampered by check-dict-spec-pin-reversal.sh\n' >> "${ROOT}/crates/dict/NOTICE"
assert_fail_contains "arm 3 (NOTICE differs)" "NOTICE copies differ"
restore

echo "=== arm 4: EXPECTED FAIL — an unexpected file in crates/dict/spec/ ==="
cp "${SPEC_DIR}/FIX44.xml" "${DECOY}"
assert_fail_contains "arm 4 (extra file)" "unexpected entry: FIX42.xml"
rm -f "${DECOY}"
assert_ok "arm 4 restored (extra file removed)"

if [[ "${fail}" -ne 0 ]]; then
  echo "check-dict-spec-pin-reversal: FAIL" >&2
  exit 1
fi
echo "check-dict-spec-pin-reversal: ok — 4 arms, each refused for the reason it claims, tree restored"
