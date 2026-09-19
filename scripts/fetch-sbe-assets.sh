#!/usr/bin/env bash
# Fetch the SBE 1.0 specification and the reference implementation that fixbolt's
# `sbe` crate is tested against (ADR-0081 decision 3).
#
# Both land in vendor/, which is gitignored. They are NEVER committed: the spec
# text, its example schema and the reference implementation's example schema are
# read as data and as a test oracle, the way ADR-0001 treats QuickFIX's assets.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SPEC_DIR="${REPO_ROOT}/vendor/sbe-spec"
REF_DIR="${REPO_ROOT}/vendor/sbe-ref"

# Pinned to commits, not branches: an oracle that moves is not an oracle.
# Upgrading is deliberate — set the env override, read what changed, and move
# the default here in the same commit that updates whatever the new ref breaks.
#
# FIXTradingCommunity/fix-simple-binary-encoding: default-branch HEAD on
# 2026-09-19. The repository has no release tag for 1.0; the 1.0 text lives under
# v1-0-STANDARD/ (and the RC4 text the plan cites under v1-0-RC4/).
SPEC_PINNED_SHA="418a8f6a8b93c65b308638dab2bc6a35dddcd864"   # 2026-09-19 HEAD
# real-logic/simple-binary-encoding: the commit of release tag 1.40.2, the
# newest release tag on 2026-09-19 (a lightweight tag, so this is the commit).
REF_PINNED_SHA="05b076c0c64d7164b56db2f9b5e6ca476ac46072"    # tag 1.40.2
SPEC_REF="${SBE_SPEC_REF:-${SPEC_PINNED_SHA}}"
REF_REF="${SBE_REF_REF:-${REF_PINNED_SHA}}"

command -v git >/dev/null || { echo "git is required" >&2; exit 1; }

# fetch_at <dir> <url> <sha> <sparse path>...
fetch_at() {
  local dir="$1" url="$2" sha="$3"
  shift 3
  if [[ ! -d "${dir}/.git" ]]; then
    mkdir -p "${dir}"
    git -C "${dir}" init -q
    git -C "${dir}" remote add origin "${url}"
    git -C "${dir}" config core.sparseCheckout true
  fi
  echo "fetching ${url} at ${sha}"
  # `fetch <sha>` rather than `clone --branch`: a branch name is a moving target.
  git -C "${dir}" fetch -q --depth 1 --filter=blob:none origin "${sha}"
  # Set unconditionally, before checkout, so an older checkout made with a
  # narrower list is widened rather than silently left as it was.
  git -C "${dir}" sparse-checkout set --no-cone "$@"
  git -C "${dir}" checkout -q --detach FETCH_HEAD
}

fetch_at "${SPEC_DIR}" https://github.com/FIXTradingCommunity/fix-simple-binary-encoding.git \
  "${SPEC_REF}" '/v1-0-STANDARD/' '/v1-0-RC4/' '/LICENSE*' '/README*'

fetch_at "${REF_DIR}" https://github.com/real-logic/simple-binary-encoding.git \
  "${REF_REF}" '/sbe-samples/src/main/resources/' '/LICENSE' '/README*'

# --- what the tests read must be there, or the fetch fails here ----------------
fail=0
need() {
  if [[ ! -e "$1" ]]; then
    echo "MISSING: $1" >&2
    fail=1
  fi
}
need "${SPEC_DIR}/v1-0-RC4/doc/03MessageStructure.md"
need "${SPEC_DIR}/v1-0-RC4/doc/07Examples.md"
need "${REF_DIR}/sbe-samples/src/main/resources/example-schema.xml"
if [[ "${fail}" -ne 0 ]]; then
  echo "The pinned refs no longer contain what fixbolt's tests read. Read what moved" >&2
  echo "upstream and update this script and the tests in the same commit." >&2
  exit 1
fi

echo
echo "Fetched into vendor/ (gitignored, never committed — ADR-0081 decision 3):"
echo "  sbe-spec/v1-0-RC4/doc/03MessageStructure.md — header, root block, groups, varData, §3.6"
echo "  sbe-spec/v1-0-RC4/doc/07Examples.md         — the hex dumps crates/sbe tests decode"
echo "  sbe-ref/sbe-samples/src/main/resources/example-schema.xml — Real Logic's Car schema"
