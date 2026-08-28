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
fi

# Set unconditionally, not only on a fresh clone: an existing checkout was made
# by an older version of this script with a narrower list, and `fetch` alone
# would leave it that way. That failure is silent — the tests that need the new
# paths just do not find them.
git -C "${VENDOR}" sparse-checkout set --no-cone \
  '/spec/' '/test/definitions/' '/src/C++/fix44/' '/LICENSE'

echo
echo "Fetched into ${VENDOR}:"
echo "  spec/FIX44.xml                    — data dictionary, input to the code generator"
echo "  test/definitions/server/fix44/    — session acceptance definitions (59 files)"
echo "  src/C++/fix44/                    — generated headers, read ONLY as an ordering oracle"
echo "  LICENSE                           — read it before using anything else here"
echo
ls "${VENDOR}/test/definitions/server/fix44" | wc -l | xargs echo "acceptance definitions:"
ls "${VENDOR}/src/C++/fix44" | wc -l | xargs echo "generated headers:"
echo
echo "src/C++/fix44/ is QuickFIX's own generated C++. It is fetched to be READ as a"
echo "second opinion on repeating-group field order — see crates/dict/tests/"
echo "interop_quickfix_order.rs. Nothing is copied, translated or committed; vendor/"
echo "is gitignored and stays that way (ADR-0001, CLAUDE.md §2 rule 9)."
