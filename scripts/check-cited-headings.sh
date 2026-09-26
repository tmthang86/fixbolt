#!/usr/bin/env bash
# ADR-0206 Context: "DESIGN.md is named by 288 files; its section numbers
# are cited in prose ... about 776 times", and decision 3: "no cited section
# is cut out of its file." A `## N.` (or `### 1a.`) heading in the five
# documents below is a number hundreds of other documents and comments cite
# by that number — `DESIGN.md §8`, `D3`, `SESSION-BEHAVIOUR.md §3` — and a
# heading that quietly disappears (renamed, renumbered, folded into another
# section) breaks every one of those citations silently, because a prose
# citation is not a link `check-links.py` or mdBook would ever flag.
#
# This script asks one question, machine-checkable and no broader: every
# heading matching `^##+ [0-9]+[a-z]?\.` that exists on the base ref, in one
# of DESIGN.md, GUIDE.md, SESSION-BEHAVIOUR.md, CONFIGURATION.md or
# CONFORMANCE.md under docs/, must still exist — same heading line, verbatim
# — somewhere in that file in the working tree. Order does not matter;
# position does not matter; only "is this exact line still here" does.
#
# Base ref: `origin/main` by default. Override with an argument
# (`scripts/check-cited-headings.sh some-branch`) or the
# `CITED_HEADINGS_BASE_REF` environment variable — CI passes the pull
# request's actual base, which is not always `main`. An argument wins over
# the environment variable.
#
# WHAT IT CANNOT SEE:
#   - a heading whose NUMBER survives but whose MEANING changed underneath
#     it (`## 4. The decisions that shape it` rewritten to discuss something
#     else entirely) — this only compares heading text, never body text.
#   - a heading reworded while keeping the same number and the same
#     meaning (a human paraphrase) reads as a removal AND an addition here,
#     because the match is the exact line, not the leading number alone.
#     That is a false red for a legitimate rename, caught by the same
#     mechanism that catches a real deletion: both need a human to update
#     the citations, so both stop the gate on purpose (walk `CLAUDE.md` §4
#     by hand and confirm before overriding).
#   - a heading MOVED from one of these five files into another one (or
#     into a new page): counted as removed from the file it left, even
#     though the words survive somewhere in the repository.
#   - a section deleted and a brand-new, unrelated section added under the
#     same number in the same commit — the two are indistinguishable from a
#     rename by this script; a human reads the diff.
#   - a base ref this checkout has never fetched: `git rev-parse` failing on
#     it is reported as a setup error (exit 2), not a red about headings.
#
# Runs standalone: scripts/check-cited-headings.sh [base-ref]
# Reversal: rename a `## 4.` heading in docs/DESIGN.md (edit the line in
# place) — expect red naming DESIGN.md and the missing heading text; restore
# the line — expect green. See the plan step this script was written for.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

BASE_REF="${1:-${CITED_HEADINGS_BASE_REF:-origin/main}}"

if ! git rev-parse --verify --quiet "${BASE_REF}^{commit}" >/dev/null; then
  echo "check-cited-headings: cannot resolve base ref '${BASE_REF}' in this checkout — fetch it first (e.g. git fetch origin main)" >&2
  exit 2
fi

FILES=(
  "docs/DESIGN.md"
  "docs/GUIDE.md"
  "docs/SESSION-BEHAVIOUR.md"
  "docs/CONFIGURATION.md"
  "docs/CONFORMANCE.md"
)

HEADING_RE='^##+ [0-9]+[a-z]?\.'

fail=0
checked=0

for f in "${FILES[@]}"; do
  # A file that did not exist on the base ref has nothing cited yet to lose;
  # not an error, just nothing to check for that file.
  if ! git cat-file -e "${BASE_REF}:${f}" 2>/dev/null; then
    continue
  fi

  base_headings="$(git show "${BASE_REF}:${f}" | grep -E "${HEADING_RE}" || true)"

  if [[ -z "${base_headings}" ]]; then
    continue
  fi

  if [[ ! -f "${f}" ]]; then
    echo "check-cited-headings: FAIL — ${f}: file existed on ${BASE_REF} with cited headings, and is gone from the working tree" >&2
    fail=1
    continue
  fi

  while IFS= read -r heading; do
    [[ -z "${heading}" ]] && continue
    checked=$((checked + 1))
    if ! grep -Fxq -- "${heading}" "${f}"; then
      echo "check-cited-headings: FAIL — ${f}: heading no longer present: ${heading}" >&2
      fail=1
    fi
  done <<< "${base_headings}"
done

if [[ ${fail} -ne 0 ]]; then
  echo "check-cited-headings: FAIL" >&2
  exit 1
fi
echo "check-cited-headings: ok — ${checked} cited heading(s) checked against ${BASE_REF}"
