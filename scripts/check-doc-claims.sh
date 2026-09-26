#!/usr/bin/env bash
# ADR-0206 decision 8's house rules for the new explanation pages, and
# ADR-0104 decision 7 ("QuickFIX" appears as a sourced fact, never an
# endorsement, a compatibility badge or a comparison): both are prose rules
# nobody's memory holds forever, so this script holds three of them by grep
# instead of by review alone (docs/plans/2026-09-26-docs-for-embedders.md,
# step 14).
#
# SCOPE. Every `docs/**/*.md` EXCEPT `docs/plans/` (Vietnamese, addressed to
# the owner, internal — CLAUDE.md §6) and `docs/decisions/` (ADRs record
# history and may quote a banned phrase in order to say it is banned), plus
# `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md` and `SECURITY.md` at the
# repository root, each scanned only if it exists today.
#
# THREE RULES:
#
#   (a) BANNED CLAIMS — "Jane Street", "Jump Trading", "Tower Research", or
#       "98.7" (case-insensitive): docs/reference/prior-art-for-embedders.md
#       §5, uncited, unverifiable, or contradicted by a primary source.
#
#   (b) FUTURE-LICENCE WORDING — "commercial licen[cs]e", "enterprise
#       edition", "pricing" (case-insensitive): ADR-0206 decision 8, a page
#       states the licence the code is under TODAY and nothing about future
#       licensing, commercial editions or pricing.
#
#   (c) A MARKETING COMPARISON — a line naming another FIX engine (QuickFIX
#       and its ports, fix8, Chronicle, OnixS, B2BITS, FIX Antenna, Rapid
#       Addition, Esprow, FerrumFIX, IronFix) together with a comparative
#       word, case-insensitive: ADR-0206 decision 8's third bullet, another
#       engine's number belongs only on the prior-art page, labelled as that
#       vendor's claim, never beside a fixbolt number, and the "why fixbolt"
#       page has "no 'faster than X'".
#
# `docs/reference/prior-art-for-embedders.md` IS SKIPPED FOR ALL THREE
# RULES. It is the one page whose job is to hold these phrases: §5 quotes
# the banned claims in order to ban them (a); §3 records other engines' own
# commercial licence and pricing models as prior art, not fixbolt's (b); and
# it is the "prior-art page" ADR-0206 decision 8 names as the only place a
# vendor's own number may sit (c).
#
# WORD LIST FOR (c) DROPS "than" ON PURPOSE. `[measured 2026-09-26]` the
# obvious list — faster, slower, quicker, better, worse, beats, outperform,
# superior, inferior, than — hit 8 lines on the tree this script was written
# against. 6 of the 8 were ordinary English with no engine comparison in
# sight: "rather than on steps" (CONFORMANCE.md), "is worth more than it
# sounds" (CONFIGURATION.md), "is stricter than the QuickFIX run" (twice,
# CONFORMANCE.md and DESIGN.md), "rather than a registered session" and
# "rather than its documentation" (both prior-art.md). Dropping "than" alone
# took 8 hits to 2 — both the same sourced fact quoted in two places
# (DESIGN.md §1 and measured-costs.md §4): "fix8, 68% faster than QuickFIX",
# each time labelled "the vendor's or project's own claim" and never a
# fixbolt number. That fact is still a hit, allow-listed below by its exact
# text rather than by exempting either file, so a NEW comparison added to
# either file is still caught.
#
# ALLOWLIST is exact substrings, each verified legitimate by hand on
# 2026-09-26 and named here with the reason. It stays short on purpose: a
# rephrased version of any line below is a NEW hit and stops the gate again.
#   (b) "pricing calculator"          — hft-playbook.md: a link to GCP's own
#                                        cost estimator, nothing about
#                                        fixbolt's licence.
#   (b) "pricing the"                 — a-loopback-write-costs-...md:
#                                        "pricing" used as a verb (costing a
#                                        syscall count), not a business term.
#   (b) "licensing models or pricing" — PRD.md's *Not in phase 5* list: an
#                                        explicit deferral, the opposite of a
#                                        promise.
#   (c) "68% faster than QuickFIX"    — see above.
#
# WHAT IT CANNOT SEE:
#   - a banned claim or comparison split across two lines, or paraphrased
#     instead of quoted verbatim — this is same-line substring matching
#     only, the shape ADR-0206 decision 8's own examples are stated in.
#   - a banned name or phrase inside a URL or a code span reads the same as
#     one in prose; a link to a vendor's own pricing page still trips (b)
#     and needs a human to read it, same as any other hit.
#   - a rule violation inside docs/plans/ or docs/decisions/: both are out
#     of scope by design, not missed by accident.
#   - ARCHITECTURE.md, CONTRIBUTING.md, SECURITY.md before they exist: each
#     is scanned only once present, so nothing before that commit is judged.
#   - the allowlist growing without a comment explaining the new entry: nothing
#     enforces that this header stays in sync with the array below it.
#
# Reversal (one bait line per rule, in a scratch file, expect each red naming
# its rule; delete the line, expect green):
#   (a) echo 'Jump Trading uses fixbolt.' >> docs/zz-claims-scratch.md
#   (b) echo 'A commercial licence is planned for a future release.' >> docs/zz-claims-scratch.md
#   (c) echo 'fixbolt is faster than QuickFIX in every benchmark.' >> docs/zz-claims-scratch.md
#
# Runs standalone: scripts/check-doc-claims.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

EXEMPT_FILE="docs/reference/prior-art-for-embedders.md"

BANNED_CLAIMS='Jane Street|Jump Trading|Tower Research|98\.7'
FUTURE_LICENCE='commercial licen|enterprise edition|pricing'
ENGINES='QuickFIX/J|QuickFIX/n|QuickFIX/Go|QuickFIX|fix8|Chronicle|OnixS|B2BITS|FIX Antenna|Rapid Addition|Esprow|FerrumFIX|IronFix'
COMPARATIVE='faster|slower|quicker|better|worse|beats|outperform|superior|inferior'

ALLOWLIST_B=(
  'pricing calculator'
  'pricing the'
  'licensing models or pricing'
)
ALLOWLIST_C=(
  '68% faster than QuickFIX'
)

# is_allowlisted <content> <allowlist-array-name>
is_allowlisted() {
  local content="$1"
  local arr_name="$2"
  local item
  eval "local arr=(\"\${${arr_name}[@]}\")"
  for item in "${arr[@]}"; do
    case "${content}" in
      *"${item}"*) return 0 ;;
    esac
  done
  return 1
}

FILES=()
while IFS= read -r f; do
  FILES+=("${f}")
done < <(find docs -type f -name '*.md' \
    -not -path 'docs/plans/*' -not -path 'docs/decisions/*' | sort)

for root_file in README.md ARCHITECTURE.md CONTRIBUTING.md SECURITY.md; do
  if [[ -f "${root_file}" ]]; then
    FILES+=("${root_file}")
  fi
done

fail=0
checked=0

for f in "${FILES[@]}"; do
  if [[ "${f}" == "${EXEMPT_FILE}" ]]; then
    continue
  fi
  checked=$((checked + 1))

  # (a) banned claims
  while IFS=: read -r lineno content; do
    [[ -z "${lineno}" ]] && continue
    echo "check-doc-claims: FAIL — ${f}:${lineno}: rule (a) banned claim:${content}" >&2
    fail=1
  done < <(grep -niE "${BANNED_CLAIMS}" "${f}" 2>/dev/null || true)

  # (b) future-licence wording
  while IFS=: read -r lineno content; do
    [[ -z "${lineno}" ]] && continue
    if is_allowlisted "${content}" ALLOWLIST_B; then
      continue
    fi
    echo "check-doc-claims: FAIL — ${f}:${lineno}: rule (b) future-licence wording:${content}" >&2
    fail=1
  done < <(grep -niE "${FUTURE_LICENCE}" "${f}" 2>/dev/null || true)

  # (c) another engine named together with a comparative word, same line
  while IFS=: read -r lineno content; do
    [[ -z "${lineno}" ]] && continue
    if is_allowlisted "${content}" ALLOWLIST_C; then
      continue
    fi
    echo "check-doc-claims: FAIL — ${f}:${lineno}: rule (c) marketing comparison:${content}" >&2
    fail=1
  done < <(grep -niE "${ENGINES}" "${f}" 2>/dev/null | grep -iE "${COMPARATIVE}" || true)

done

if [[ ${fail} -ne 0 ]]; then
  echo "check-doc-claims: FAIL" >&2
  exit 1
fi
echo "check-doc-claims: ok — ${checked} file(s) checked, 0 banned claims, 0 future-licence hits, 0 marketing comparisons"
