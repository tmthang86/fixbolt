#!/usr/bin/env bash
# Fetch the QuickFIX assets nanofixengine uses as data and as a test oracle.
#
# These land in vendor/, which is gitignored. They are NEVER committed: doing so would
# pull the QuickFIX Software License's attribution clause into this repository.
# See docs/decisions/ADR-0001-relationship-to-quickfix.md.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="${REPO_ROOT}/vendor/quickfix"

# STATUS.md open item 7. This used to default to `master`, which meant every
# number this repository has measured off the corpus — 59 files, 539 message
# lines, 244 expected lines carrying `10=` — could change upstream with no
# commit here and no warning. A test oracle that moves is not an oracle.
#
# Pinned to a commit. Upgrading is deliberate: set QUICKFIX_REF, read what the
# counts below print, and change this default in the same commit that updates
# whatever the new corpus breaks.
PINNED_SHA="386ce46e917ae494ab6e90b1be90fd421cdbe3f9"   # 2026-05-20
REF="${QUICKFIX_REF:-${PINNED_SHA}}"

# What the pinned corpus contains. Checked below, not trusted: if a future ref
# disagrees with these, the fetch fails here rather than three layers away in a
# test whose message will be about a field count.
WANT_DEFS=59
WANT_MSG_LINES=539
WANT_CHECKSUM_LINES=244

command -v git >/dev/null || { echo "git is required" >&2; exit 1; }

if [[ ! -d "${VENDOR}/.git" ]]; then
  mkdir -p "${VENDOR}"
  git -C "${VENDOR}" init -q
  git -C "${VENDOR}" remote add origin https://github.com/quickfix/quickfix.git
  git -C "${VENDOR}" config core.sparseCheckout true
fi

# `fetch <sha>` rather than `clone --branch`: a branch name is a moving target
# and a commit is not.
echo "fetching quickfix at ${REF}"
git -C "${VENDOR}" fetch -q --depth 1 --filter=blob:none origin "${REF}"
git -C "${VENDOR}" checkout -q --detach FETCH_HEAD

# Set unconditionally, not only on a fresh clone: an existing checkout was made
# by an older version of this script with a narrower list, and `fetch` alone
# would leave it that way. That failure is silent — the tests that need the new
# paths just do not find them.
git -C "${VENDOR}" sparse-checkout set --no-cone \
  '/spec/' '/test/definitions/' '/src/C++/fix44/' '/LICENSE' \
  '/src/C++/FixFieldNumbers.h' '/src/C++/FixFields.h' \
  '/src/C++/FixCommonFields.h' '/src/C++/FixValues.h'

echo
echo "Fetched into ${VENDOR}:"
echo "  spec/FIX44.xml                    — data dictionary, input to the code generator"
echo "  test/definitions/server/fix44/    — session acceptance definitions (59 files)"
echo "  src/C++/fix44/                    — generated headers, read ONLY as an oracle"
echo "  src/C++/FixFieldNumbers.h         — tag numbers, read ONLY as an oracle"
echo "  src/C++/FixFields.h + FixCommonFields.h — field types, read ONLY as an oracle"
echo "  src/C++/FixValues.h               — enum values, a PARTIAL oracle (see the plan)"
echo "  LICENSE                           — read it before using anything else here"
echo
# --- the corpus is what this project measured, or the fetch fails -------------
#
# Every one of these appears in docs/ as a measured fact. Checking them here is
# what makes "the corpus is pinned" mean something: a pin nobody verifies is a
# comment.
got_defs=$(find "${VENDOR}/test/definitions/server/fix44" -name '*.def' | wc -l | tr -d ' ')
got_msg=$(cat "${VENDOR}"/test/definitions/server/fix44/*.def | grep -cE '^[IE]' || true)
# `$'\001'` and not '\x01': grep -E takes the pattern literally, so the escape
# form silently matches nothing and this check would pass at zero for ever.
got_ck=$(cat "${VENDOR}"/test/definitions/server/fix44/*.def | grep -c $'^E.*\00110=' || true)

fail=0
check() {
  if [[ "$2" != "$3" ]]; then
    echo "CORPUS MISMATCH: $1 is $2, this project measured $3" >&2
    fail=1
  fi
}
check "acceptance definitions" "${got_defs}" "${WANT_DEFS}"
check "message lines (I/E)"    "${got_msg}"  "${WANT_MSG_LINES}"
check "E lines carrying 10="   "${got_ck}"   "${WANT_CHECKSUM_LINES}"
if [[ "${fail}" -ne 0 ]]; then
  cat >&2 <<'EOMSG'

The corpus at this ref is not the one this project's numbers were measured on.
That is not automatically wrong — it is unreviewed. Read what changed, update
the documents that quote these counts, and move PINNED_SHA in the same commit.
EOMSG
  exit 1
fi
echo "corpus verified: ${got_defs} definitions, ${got_msg} message lines, ${got_ck} with a checksum field"

ls "${VENDOR}/src/C++/fix44" | wc -l | xargs echo "generated headers:"
for f in FixFieldNumbers.h FixFields.h FixCommonFields.h FixValues.h; do
  [[ -f "${VENDOR}/src/C++/${f}" ]] || { echo "MISSING ${f}" >&2; exit 1; }
  wc -l < "${VENDOR}/src/C++/${f}" | xargs echo "  src/C++/${f}:"
done
echo
echo "Everything under src/C++/ is QuickFIX's own generated C++. It is fetched to be"
echo "READ as a second opinion — on repeating-group field order (crates/dict/tests/"
echo "interop_quickfix_order.rs), and on tag numbers, field types and per-message tag"
echo "sets (interop_quickfix_fields.rs, interop_quickfix_messages.rs). Nothing is"
echo "copied, translated or committed; vendor/ is gitignored and stays that way"
echo "(ADR-0001, CLAUDE.md §2 rule 9)."
