#!/usr/bin/env bash
# Every file in docs/decisions/ carries a distinct ADR-NNNN number, the
# filename shape `ADR-NNNN-<slug>.md`, and a first heading line (`# ADR-NNNN
# — <title>`) whose number matches the filename.
#
# WHY THIS EXISTS.
#
# docs/reference/two-branches-can-take-the-same-adr-number-without-a-conflict.md
# is the story: two branches each allocated ADR-0082 for a different subject,
# git merged the two different filenames without a conflict, and nothing in
# CI noticed — `cargo test`, `cargo clippy`, `cargo doc` and
# `scripts/check-links.py` all pass on a tree holding two ADR-0082s, because
# every link still resolves to a file that exists. The shared resource is the
# *number*, and the number is not a file, so git's merge machinery cannot see
# the collision at all. This script is the check that reads the number.
#
# The H1 check exists for the same shape of hole: two ADRs written the same
# day and renumbered late (two architects racing for the same number) can
# carry a filename that was updated and an H1 that was not — `scripts/
# check-links.py` and every other gate stay green, because the H1's number is
# prose, not a link and not a filename, so nothing else reads it.
#
# WHAT THIS SCRIPT CANNOT SEE.
#
# It reads one tree — the working copy it is pointed at. It cannot see a
# collision with a sibling branch that has not merged into that tree yet; the
# cross-branch check is still the manual `for b in $(git branch -r ...)` loop
# in the reference doc above, run by hand before allocating a new number.
# It checks only the H1 against the filename — it cannot see prose that
# still cites an old number after a renumber anywhere else: another ADR's
# *Related* list, this ADR's own body, a plan's delivery log, a pull request
# body — none of that is the first heading line, so grep for the bare string
# separately, as the reference doc says.
set -euo pipefail

# Plain indexed arrays rather than an associative array (`declare -A`) so
# this runs under bash 3.2 (macOS's shipped /bin/bash) as well as the bash 4+
# CI runs on, without needing a newer bash on the path.

DIR="${1:-docs/decisions}"

if [[ ! -d "${DIR}" ]]; then
  echo "check-adr-numbers: no such directory: ${DIR}" >&2
  exit 2
fi

numbers=()
files=()
bad=0
total=0
h1_checked=0

for path in "${DIR}"/ADR-*; do
  [[ -f "${path}" ]] || continue
  total=$((total + 1))
  name="$(basename "${path}")"

  if [[ ! "${name}" =~ ^ADR-[0-9]{4}-.+\.md$ ]]; then
    echo "check-adr-numbers: ${path}: does not match ADR-NNNN-<slug>.md" >&2
    bad=1
    continue
  fi

  number="${name:0:8}"

  prior=""
  for i in "${!numbers[@]}"; do
    if [[ "${numbers[$i]}" == "${number}" ]]; then
      prior="${files[$i]}"
      break
    fi
  done

  if [[ -n "${prior}" ]]; then
    echo "check-adr-numbers: ${number} used by both ${prior} and ${path}" >&2
    bad=1
  else
    numbers+=("${number}")
    files+=("${path}")
  fi

  # The filename number and the first heading line's number must agree —
  # a renumber that moved one and not the other is exactly the hole
  # docs/reference/two-branches-can-take-the-same-adr-number-without-a-conflict.md
  # describes for filenames, restated for the H1.
  h1="$(head -n 1 "${path}")"
  h1_checked=$((h1_checked + 1))

  if [[ "${h1}" =~ ^#\ (ADR-[0-9]{4})([:\ ]|$) ]]; then
    h1_number="${BASH_REMATCH[1]}"
    if [[ "${h1_number}" != "${number}" ]]; then
      echo "check-adr-numbers: ${path}: filename says ${number} but the H1 says ${h1_number}" >&2
      bad=1
    fi
  else
    echo "check-adr-numbers: ${path}: first heading line does not start with '# ADR-NNNN'" >&2
    bad=1
  fi
done

if [[ "${bad}" -ne 0 ]]; then
  exit 1
fi

distinct="${#numbers[@]}"
echo "ok - ${total} ADRs, ${distinct} distinct numbers, ${h1_checked} H1s checked"
