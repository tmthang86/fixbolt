#!/usr/bin/env bash
# Fetch the QuickFIX assets nanofixengine uses as data and as a test oracle.
#
# These land in vendor/, which is gitignored. They are NEVER committed: doing so would
# pull the QuickFIX Software License's attribution clause into this repository.
# See docs/decisions/ADR-0001-relationship-to-quickfix.md.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="${REPO_ROOT}/vendor/quickfix"
REF="${QUICKFIX_REF:-master}"

command -v git >/dev/null || { echo "git is required" >&2; exit 1; }

if [[ -d "${VENDOR}/.git" ]]; then
  echo "vendor/quickfix already present; fetching ${REF}"
  git -C "${VENDOR}" fetch --depth 1 origin "${REF}"
  git -C "${VENDOR}" checkout -q FETCH_HEAD
else
  mkdir -p "$(dirname "${VENDOR}")"
  git clone --depth 1 --branch "${REF}" --filter=blob:none --sparse \
    https://github.com/quickfix/quickfix.git "${VENDOR}"
  git -C "${VENDOR}" sparse-checkout set --no-cone '/spec/' '/test/definitions/' '/LICENSE'
fi

echo
echo "Fetched into ${VENDOR}:"
echo "  spec/FIX44.xml                    — data dictionary, input to the code generator"
echo "  test/definitions/server/fix44/    — session acceptance definitions (59 files)"
echo "  LICENSE                           — read it before using anything else here"
echo
ls "${VENDOR}/test/definitions/server/fix44" | wc -l | xargs echo "acceptance definitions:"
