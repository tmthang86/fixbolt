#!/usr/bin/env bash
# CLAUDE.md §2 non-negotiable 9 (as amended by ADR-0104): exactly three
# QuickFIX files ship, byte-identical to the pin scripts/fetch-quickfix-assets.sh
# uses, under NOTICE. This script is the machine check that row names.
#
# Five things, none of which the others can stand in for:
#   1. each of crates/dict/spec/{FIX44,FIXT11,FIX50SP2}.xml hashes to the
#      sha256 recorded below, which is the value docs/decisions/ADR-0104-*.md
#      *Research* measured at the pin;
#   2. crates/dict/spec/ holds EXACTLY those three files and nothing else —
#      "exactly three QuickFIX files ship" (CLAUDE.md §2 item 9) is a claim
#      about the whole directory, not just about the three names check 1
#      already looked for; a fourth file sitting beside them would pass
#      check 1 in full and still be a QuickFIX file nobody is accounting for;
#   3. this script's own pin equals fetch-quickfix-assets.sh's PINNED_SHA —
#      catches the case where one was bumped and the other was not;
#   4. NOTICE at the repository root and crates/dict/NOTICE are byte-identical
#      (only a file under the crate's own root reaches its .crate, so both
#      must exist and agree);
#   5. when vendor/ is present (a developer machine or a CI job that fetched
#      it), the shipped file and the vendored file are compared directly with
#      cmp — two files that separately hash right could still be two
#      different files if the expected-sha256 table above were ever wrong.
#
# When vendor/ is absent (a machine that only checked out this repository,
# or a CI job that deliberately never fetches it — the whole point of
# ADR-0104), point 5 is skipped and said so out loud: "vendor absent:
# sha256 only" is not silence, it is this script naming what it did not
# check. FIXBOLT_VENDOR overrides where "vendor/" is looked for, mainly so a
# CI job or a test can point it at a path that certainly does not exist and
# see this script still pass.
#
# Runs standalone: scripts/check-dict-spec-pin.sh
# Reversal: scripts/check-dict-spec-pin-reversal.sh — four arms (missing
# file, one flipped byte, NOTICE copies differing, an unexpected file in
# spec/), each stated as an expected FAIL before it runs, each restored by a
# trap that fires even when an assertion fails.

set -uo pipefail

# Needs bash 4 or newer (declare -A). macOS's /bin/bash is 3.2, and a script
# like this one can print `ok` there having checked nothing:
# docs/reference/a-check-script-under-bash-3-2-can-print-ok-having-checked-nothing.md.
# Refused before anything runs; scripts/check-old-bash-is-refused.sh holds
# this guard in every scripts/check-*.sh that needs it.
if [ "${BASH_VERSINFO[0]:-0}" -lt 4 ]; then
  echo "check-dict-spec-pin: FAIL — needs bash 4 or newer (declare -A); this is bash ${BASH_VERSION:-unknown}. Put a newer bash first on PATH; under macOS's /bin/bash 3.2 this script can report ok having checked nothing." >&2
  exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

# The pin this repository ships. Bumping the QuickFIX corpus moves this line
# and fetch-quickfix-assets.sh's PINNED_SHA in the same commit — check 2
# below is what makes that "in the same commit" rather than "eventually".
PINNED_SHA="386ce46e917ae494ab6e90b1be90fd421cdbe3f9"

SPEC_DIR="${ROOT}/crates/dict/spec"
FETCH_SCRIPT="${ROOT}/scripts/fetch-quickfix-assets.sh"

fail=0

# --- Check 3: this script's pin against fetch-quickfix-assets.sh's. --------
if [[ ! -f "${FETCH_SCRIPT}" ]]; then
  echo "check-dict-spec-pin: FAIL — ${FETCH_SCRIPT} not found" >&2
  exit 2
fi
fetch_pin="$(grep -oE 'PINNED_SHA="[0-9a-f]{40}"' "${FETCH_SCRIPT}" | head -n1 | grep -oE '[0-9a-f]{40}')"
if [[ -z "${fetch_pin}" ]]; then
  echo "check-dict-spec-pin: FAIL — could not read a 40-hex-digit PINNED_SHA out of ${FETCH_SCRIPT}" >&2
  exit 2
fi
if [[ "${fetch_pin}" != "${PINNED_SHA}" ]]; then
  echo "check-dict-spec-pin: FAIL — pin mismatch: this script pins ${PINNED_SHA}, fetch-quickfix-assets.sh pins ${fetch_pin}. Upgrading the QuickFIX corpus moves both in one commit (ADR-0104 decision 2)." >&2
  fail=1
fi

# --- Check 1: sha256 of the three shipped files. ---------------------------
# Recorded from vendor/quickfix at the pin above (ADR-0104 *Research*).
declare -A EXPECTED_SHA256=(
  [FIX44.xml]="a82655b54363aa9c6d1b2f21f294f1198c0d7125d7b44c26d93d3179f3358425"
  [FIXT11.xml]="baf0ef6ddebbbbe32c6d66c00bd4a9bab7ded324c4bbca6a07f42e808090bf20"
  [FIX50SP2.xml]="7d34e565586dd4096a08691d10e415b5a2fd531a8dadfcfc831daea419d3c3f3"
)

for name in FIX44.xml FIXT11.xml FIX50SP2.xml; do
  path="${SPEC_DIR}/${name}"
  if [[ ! -f "${path}" ]]; then
    echo "check-dict-spec-pin: FAIL — missing crates/dict/spec/${name}" >&2
    fail=1
    continue
  fi
  got="$(sha256sum "${path}" | cut -d' ' -f1)"
  want="${EXPECTED_SHA256[${name}]}"
  if [[ "${got}" != "${want}" ]]; then
    echo "check-dict-spec-pin: FAIL — sha256 mismatch on crates/dict/spec/${name}: got ${got}, want ${want} (pin ${PINNED_SHA})" >&2
    fail=1
  fi
done

# --- Check 2: crates/dict/spec/ holds EXACTLY those three files. -----------
# Check 1 above only ever asks "is FIX44.xml there and right" for each of the
# three names — it would pass in full with a fourth file sitting right next
# to them. "Exactly three QuickFIX files ship" (CLAUDE.md §2 item 9) is a
# claim about the directory's whole contents, so it needs its own listing.
if [[ -d "${SPEC_DIR}" ]]; then
  unexpected=()
  while IFS= read -r -d '' entry; do
    base="$(basename "${entry}")"
    case "${base}" in
      FIX44.xml | FIXT11.xml | FIX50SP2.xml) ;;
      *) unexpected+=("${base}") ;;
    esac
  done < <(find "${SPEC_DIR}" -mindepth 1 -maxdepth 1 -print0)
  if [[ "${#unexpected[@]}" -gt 0 ]]; then
    echo "check-dict-spec-pin: FAIL — crates/dict/spec/ must hold exactly FIX44.xml, FIXT11.xml and FIX50SP2.xml; found unexpected entr$([ "${#unexpected[@]}" -eq 1 ] && echo y || echo ies): ${unexpected[*]}" >&2
    fail=1
  fi
else
  echo "check-dict-spec-pin: FAIL — crates/dict/spec/ does not exist" >&2
  fail=1
fi

# --- Check 4: the two NOTICE copies. ----------------------------------------
if [[ ! -f "${ROOT}/NOTICE" ]]; then
  echo "check-dict-spec-pin: FAIL — missing NOTICE at the repository root" >&2
  fail=1
elif [[ ! -f "${ROOT}/crates/dict/NOTICE" ]]; then
  echo "check-dict-spec-pin: FAIL — missing crates/dict/NOTICE" >&2
  fail=1
elif ! cmp -s "${ROOT}/NOTICE" "${ROOT}/crates/dict/NOTICE"; then
  echo "check-dict-spec-pin: FAIL — NOTICE copies differ: NOTICE and crates/dict/NOTICE must be byte-identical" >&2
  fail=1
fi

# --- Check 5: against vendor/, when there is one. ---------------------------
vendor_spec="${FIXBOLT_VENDOR:-${ROOT}/vendor/quickfix}/spec"
if [[ -d "${vendor_spec}" ]]; then
  for name in FIX44.xml FIXT11.xml FIX50SP2.xml; do
    shipped="${SPEC_DIR}/${name}"
    vendored="${vendor_spec}/${name}"
    if [[ ! -f "${vendored}" ]]; then
      echo "check-dict-spec-pin: FAIL — vendor/ is present but ${vendored} is missing; run scripts/fetch-quickfix-assets.sh again" >&2
      fail=1
      continue
    fi
    if [[ -f "${shipped}" ]] && ! cmp -s "${shipped}" "${vendored}"; then
      echo "check-dict-spec-pin: FAIL — crates/dict/spec/${name} differs from ${vendored}" >&2
      fail=1
    fi
  done
else
  echo "vendor absent: sha256 only"
fi

if [[ "${fail}" -ne 0 ]]; then
  echo "check-dict-spec-pin: FAIL" >&2
  exit 1
fi
echo "check-dict-spec-pin: ok — pin ${PINNED_SHA}, three files, two NOTICE copies"
