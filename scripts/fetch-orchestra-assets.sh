#!/usr/bin/env bash
# Fetch the FIX Orchestra FIX 4.4 file the dictionary-source spike measures.
#
# It lands in vendor/orchestra/, which is gitignored, and is NEVER committed by
# this script: ADR-0101 decision 2 — the file at spike time is fetched, never
# committed. Whether a copy ships inside crates/dict/spec/ is ADR-0101's
# outcome A or B to decide, and that copy is checked against the same pin.
#
# What it fetches:
#   FIX Standard/OrchestraFIX44.xml  — pinned by commit AND by sha256
#   LICENSE                          — the repository's Apache-2.0 text, same commit
# from https://github.com/FIXTradingCommunity/orchestrations.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="${REPO_ROOT}/vendor/orchestra"

# Pinned to a commit, the way fetch-quickfix-assets.sh pins its SHA: a moving
# upstream is not an oracle. The upstream README calls the 4.4 file "frozen",
# yet it changed ten times since 2023 (ADR-0101 Research) — frozen is not
# unchanging.
#
# Upgrading is deliberate: set ORCHESTRA_REF and ORCHESTRA_SHA256 together, re-run
# scripts/dict-diff.py, and move both defaults in the same commit that records
# what the new file changes. A ref override without a matching sha256 fails
# below, on purpose.
PINNED_SHA="cd24169a2abd8daba7c360987c7a46ca11873a12"   # 2026-09-08
PINNED_SHA256="a36262895e90bbcad2948e0c98072173a571a253ba63107a89617441c67a9f86"
REF="${ORCHESTRA_REF:-${PINNED_SHA}}"
WANT_SHA256="${ORCHESTRA_SHA256:-${PINNED_SHA256}}"

BASE="https://raw.githubusercontent.com/FIXTradingCommunity/orchestrations/${REF}"
XML_URL="${BASE}/FIX%20Standard/OrchestraFIX44.xml"
LICENSE_URL="${BASE}/LICENSE"
XML="${VENDOR}/OrchestraFIX44.xml"
LICENSE="${VENDOR}/LICENSE"

command -v curl >/dev/null || { echo "curl is required" >&2; exit 1; }
command -v sha256sum >/dev/null || { echo "sha256sum is required" >&2; exit 1; }

mkdir -p "${VENDOR}"

# Download to a temporary name and move into place only once the hash matches:
# a failed or mismatched fetch must never leave a file at the path the spike
# reads, or the next run would measure it.
tmp="$(mktemp "${VENDOR}/.OrchestraFIX44.xml.XXXXXX")"
trap 'rm -f "${tmp}"' EXIT

echo "fetching OrchestraFIX44.xml at ${REF}"
curl -fsSL --retry 3 -o "${tmp}" "${XML_URL}"

got_sha256="$(sha256sum "${tmp}" | cut -d' ' -f1)"
if [[ "${got_sha256}" != "${WANT_SHA256}" ]]; then
  cat >&2 <<EOMSG
sha256 mismatch for FIX Standard/OrchestraFIX44.xml at ${REF}
  expected ${WANT_SHA256}
  got      ${got_sha256}

This is not the file ADR-0101 pinned, so nothing measured against it is the
measurement the decision rule reads. Nothing was written to ${XML}.
EOMSG
  exit 1
fi
mv -f "${tmp}" "${XML}"
trap - EXIT

echo "fetching LICENSE at ${REF}"
curl -fsSL --retry 3 -o "${LICENSE}" "${LICENSE_URL}"

echo
echo "Fetched into ${VENDOR}:"
echo "  OrchestraFIX44.xml — $(wc -c < "${XML}" | tr -d ' ') bytes, sha256 ${got_sha256}"
echo "  LICENSE            — $(head -n 3 "${LICENSE}" | tr -s ' \n' ' ' | cut -c1-60)"
echo
echo "vendor/ is gitignored. The spike (scripts/dict-diff.py) reads this file;"
echo "nothing here is committed (ADR-0101 decision 2)."
