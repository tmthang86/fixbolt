#!/usr/bin/env bash
# Every scripts/check-*.sh that needs bash 4 refuses to run under bash 3.2,
# rather than printing `ok` having checked nothing.
#
# `[measured 2026-09-26]` under macOS's /bin/bash 3.2.57,
# scripts/check-dict-spec-pin.sh exited 0 and printed "ok — pin …, three
# files, two NOTICE copies" while its `declare -A` table of expected sha256
# values had failed to exist, so it compared no hash at all; and
# scripts/check-sudo-names-what-root-can-find.sh exited 0 with "0 sudo lines
# read" because `mapfile` did not exist. Three others failed loudly but with
# an unrelated-looking error. The trap and its costs:
# docs/reference/a-check-script-under-bash-3-2-can-print-ok-having-checked-nothing.md.
#
# Two checks, and neither stands in for the other:
#   1. static: every scripts/check-*.sh whose code (not its comments) uses
#      `declare -A`, `mapfile` or `readarray` carries the guard. A new script
#      that needs bash 4 and forgets the guard fails here, on any machine.
#   2. observed: each of those scripts, run under a real bash 3.2 (the
#      official `bash:3.2` container image), exits 2 carrying the guard's
#      sentence. Reading the guard's text proves nothing about where it sits;
#      running it does — a guard placed after anything that exits first (an
#      argument check, a tool the image lacks, a `set -e` casualty) never
#      fires, and the script then fails or passes for some other reason.
#
# Needs docker for check 2. Without it this script fails and says so: a
# check that cannot run its second half must not report ok about it.
#
# This script itself uses nothing bash 4 has and bash 3.2 lacks, so it runs
# as-is on macOS.
#
# Reversal, `[measured 2026-09-26]` both red on their own line:
#   - delete the guard from check-sudo-names-what-root-can-find.sh → check 1
#     "has no bash 4 guard", and check 2 "exit 0" (it read 0 lines and said ok);
#   - move the guard in check-no-crate-root-allow.sh below its first
#     `mapfile` → check 2 names it: `cargo metadata` exits 2 first, without
#     the sentence.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

# The guard's test, as literal text to find in each script — not to expand.
# shellcheck disable=SC2016
GUARD='"${BASH_VERSINFO[0]:-0}" -lt 4'
SENTENCE='needs bash 4 or newer'
IMAGE='bash:3.2'

needs=()
for f in scripts/check-*.sh; do
  # Not this script: its own pattern and messages name the three words.
  [ "${f}" = "scripts/check-old-bash-is-refused.sh" ] && continue
  # Code only: a comment that mentions `mapfile` is not a use of it.
  if grep -vE '^[[:space:]]*#' "${f}" | grep -qE '(^|[^[:alnum:]_-])(declare -A|mapfile|readarray)([^[:alnum:]_-]|$)'; then
    needs+=("${f}")
  fi
done

if [ "${#needs[@]}" -eq 0 ]; then
  echo "check-old-bash-is-refused: FAIL — found no scripts/check-*.sh using declare -A or mapfile; at least six did on 2026-09-26, so this is a broken search, not a clean tree." >&2
  exit 1
fi

status=0
for f in "${needs[@]}"; do
  if grep -qF "${GUARD}" "${f}"; then
    echo "check-old-bash-is-refused: ok — ${f} carries the bash 4 guard"
  else
    echo "check-old-bash-is-refused: FAIL — ${f} uses declare -A or mapfile and has no bash 4 guard. Copy the guard block from scripts/check-dict-spec-pin.sh to right after its \`set\` line." >&2
    status=1
  fi
done

if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then
  echo "check-old-bash-is-refused: FAIL — docker is not available, so check 2 (each script run under a real bash 3.2) did not run. NOT PASSED." >&2
  exit 1
fi

for f in "${needs[@]}"; do
  out="$(docker run --rm -v "${ROOT}:/w:ro" -w /w "${IMAGE}" bash "${f}" 2>&1)"
  rc=$?
  if [ "${rc}" -eq 2 ] && grep -qF "${SENTENCE}" <<<"${out}"; then
    echo "check-old-bash-is-refused: ok — ${f} under ${IMAGE}: exit 2, refused"
  else
    echo "check-old-bash-is-refused: FAIL — ${f} under ${IMAGE}: exit ${rc}, expected 2 and \"${SENTENCE}\". It said:" >&2
    echo "${out}" | tail -5 >&2
    status=1
  fi
done

if [ "${status}" -ne 0 ]; then
  exit 1
fi
echo "check-old-bash-is-refused: ok — ${#needs[@]} scripts, each guarded and each refused under ${IMAGE}"
