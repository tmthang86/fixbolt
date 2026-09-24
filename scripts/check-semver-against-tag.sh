#!/usr/bin/env bash
# ADR-0161 decision 5, plan row 8b
# (docs/plans/2026-09-23-p3-packaging-and-first-release.md, *Sửa 2*): once
# there is no crates.io upload, the API baseline a stranger's `Cargo.lock`
# actually sees is the last tag, not `origin/main` — so the blocking semver
# gate compares `HEAD` against `v0.1.0` (or whatever tag is named), not the
# branch it grew from.
#
# It is a wrapper, not a replacement, around
# `cargo semver-checks --workspace --baseline-rev <tag>`, because that tool
# has one hole ADR-0161 *Context* fact 6 measured directly: comparing
# identical `0.y.z` versions runs the real lint set (this repository's six
# crates: 196 checks, 58 skipped by design), but a `0.y` -> `0.(y+1)` bump
# is read as a MAJOR change and skips every single lint while still exiting
# 0 (`0 checks: 0 pass, 254 skip` per crate, measured at `0.0.0` -> `0.1.0`
# before this repository ever had a real tag). A gate that only reads the
# exit status would go green at exactly the moment nothing was compared —
# the sibling-project link in ADR-0161 *Research* names the same defect
# class. So this script reads, per published crate, the actual
# `Checked […] N checks: …` line cargo-semver-checks itself prints, and
# fails unless N > 0 — UNLESS the workspace's own declared version already
# differs from the tag's (a deliberate `0.1.0` -> `0.2.0` bump), in which
# case a 0-check crate is expected and the script says so with a `NOTE`
# rather than failing on it.
#
# Both versions are read from the manifest text itself, with `tomllib`, the
# same way `scripts/check-release-versions.sh` already does — never from
# cargo-semver-checks' own `(no change; assume minor)` / `(major change)`
# wording in its output, which is that tool's phrasing for what IT assumed,
# not a fact this script can trust as an input to its own assumption.
#
# WHAT IT CANNOT SEE: whether the tag itself has moved since somebody last
# fetched it (`git rev-parse <tag>^{commit}` below only reads what THIS
# checkout currently believes the tag points to — `scripts/stranger-check.sh
# --from git`'s own header carries the same caveat, and ADR-0161 decision 2's
# ruleset is the thing meant to make it unreachable, not this script); a
# semver break in a crate cargo-semver-checks has no lint for (its own lint
# list is a closed set, not this repository's); whether `cargo-semver-checks`
# itself is the version pinned in CI (`rust-toolchain.toml` does not pin it —
# `cargo install --locked cargo-semver-checks --version 0.50.0` in
# `.github/workflows/ci.yml` does; a local run with a different version is
# read, not trusted blindly, same as everywhere else in this repository).
#
# Exit 0 when every one of the six published crates ran a real comparison (or
# was excused by a genuine version bump, printed as a NOTE) and
# cargo-semver-checks itself found nothing; the same nonzero status
# cargo-semver-checks itself returned when it found a real semver violation
# (a `function_missing` failure and friends — this script does not swallow
# that into its own exit 1); 1 when THIS script's own assertion fails (a
# crate missing from the output, or an unexcused 0-check crate); 2 when the
# script cannot run at all (bad arguments, the tag not resolvable in this
# checkout, cargo-semver-checks not installed, a manifest that does not
# parse).
#
# Usage:
#   scripts/check-semver-against-tag.sh v0.1.0
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

if [[ $# -ne 1 || -z "${1:-}" ]]; then
  echo "check-semver-against-tag: FAIL — usage: check-semver-against-tag.sh <tag>" >&2
  exit 2
fi
TAG="$1"

if ! command -v cargo >/dev/null 2>&1; then
  echo "check-semver-against-tag: FAIL — cargo not found on PATH" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "check-semver-against-tag: FAIL — python3 not found on PATH" >&2
  exit 2
fi
if ! cargo semver-checks --version >/dev/null 2>&1; then
  echo "check-semver-against-tag: FAIL — cargo-semver-checks is not installed (cargo install --locked cargo-semver-checks --version 0.50.0)" >&2
  exit 2
fi

# --- the tag must be resolvable in THIS checkout, or nothing below can be
#     computed (a shallow clone, or a tag never fetched) ---------------------
if ! git rev-parse "${TAG}^{commit}" >/dev/null 2>&1; then
  echo "check-semver-against-tag: FAIL — tag ${TAG} is not resolvable in this checkout (git fetch --tags?) — cannot compute a baseline" >&2
  exit 2
fi

# --- the two versions, read from the manifest text, never from cargo's own
#     wording of what changed -------------------------------------------------
CURRENT_VERSION="$(python3 - "${ROOT}/Cargo.toml" <<'PY'
import pathlib, sys, tomllib
with open(sys.argv[1], "rb") as f:
    ws = tomllib.load(f)
v = ws.get("workspace", {}).get("package", {}).get("version")
if not isinstance(v, str):
    sys.exit(1)
print(v)
PY
)"
if [[ -z "${CURRENT_VERSION}" ]]; then
  echo "check-semver-against-tag: FAIL — could not read [workspace.package] version from Cargo.toml" >&2
  exit 2
fi

TAG_MANIFEST="$(git show "${TAG}:Cargo.toml" 2>/dev/null || true)"
if [[ -z "${TAG_MANIFEST}" ]]; then
  echo "check-semver-against-tag: FAIL — could not read Cargo.toml at ${TAG} (git show ${TAG}:Cargo.toml)" >&2
  exit 2
fi
TAG_VERSION="$(python3 -c '
import sys, tomllib
ws = tomllib.loads(sys.stdin.read())
v = ws.get("workspace", {}).get("package", {}).get("version")
if not isinstance(v, str):
    sys.exit(1)
print(v)
' <<<"${TAG_MANIFEST}")"
if [[ -z "${TAG_VERSION}" ]]; then
  echo "check-semver-against-tag: FAIL — could not read [workspace.package] version from ${TAG}:Cargo.toml" >&2
  exit 2
fi

VERSION_BUMPED=0
if [[ "${CURRENT_VERSION}" != "${TAG_VERSION}" ]]; then
  VERSION_BUMPED=1
fi

# The six crates ADR-0160 publishes — named here, not read off the manifest,
# for the same reason scripts/check-release-versions.sh names them: a crate
# switched to publish = false by mistake must fail this check, not silently
# vanish from what it looks for.
PUBLISHED=(fixbolt-codec fixbolt-dict fixbolt-session fixbolt-engine fixbolt-sbe fixbolt)

log="$(mktemp)"
trap 'rm -f "${log}"' EXIT

echo "== cargo semver-checks --workspace --baseline-rev ${TAG} =="
cargo semver-checks --workspace --baseline-rev "${TAG}" >"${log}" 2>&1
cargo_status=$?
cat "${log}"

# --- attribute each "Checked […] N checks: …" line to the "Checking <crate>"
#     line immediately above it — that is the order cargo-semver-checks
#     prints in, one crate fully finished before the next one starts --------
mapfile -t crate_lines < <(python3 - "${log}" <<'PY'
import re
import sys

path = sys.argv[1]
text = open(path, encoding="utf-8", errors="replace").read()
lines = text.splitlines()

checking_re = re.compile(r"^\s*Checking (\S+) v")
checked_re = re.compile(r"^\s*Checked \[[^]]*\]\s+(\d+) checks:")

current = None
for line in lines:
    m = checking_re.match(line)
    if m:
        current = m.group(1)
        continue
    m = checked_re.match(line)
    if m and current is not None:
        print(f"{current}\t{m.group(1)}")
        current = None
PY
)

declare -A checks_for
for entry in "${crate_lines[@]}"; do
  name="${entry%%$'\t'*}"
  count="${entry#*$'\t'}"
  checks_for["${name}"]="${count}"
done

fails=()
notes=()
for name in "${PUBLISHED[@]}"; do
  if [[ -z "${checks_for[${name}]+x}" ]]; then
    fails+=("FAIL semver: ${name} did not appear in cargo-semver-checks output at all")
    continue
  fi
  n="${checks_for[${name}]}"
  if [[ "${n}" -eq 0 ]]; then
    if [[ "${VERSION_BUMPED}" -eq 1 ]]; then
      notes+=("NOTE: ${name} ran 0 checks against ${TAG} — workspace version ${CURRENT_VERSION} differs from ${TAG}'s ${TAG_VERSION} (a deliberate bump; cargo-semver-checks skips a major-level comparison by design, ADR-0161 Context fact 6)")
    else
      fails+=("FAIL semver: ${name} ran 0 checks against ${TAG} (workspace version ${CURRENT_VERSION} is unchanged from ${TAG}'s ${TAG_VERSION} — 0 checks here means nothing was compared)")
    fi
  else
    echo "check-semver-against-tag: Checked ${name}: ${n} checks against ${TAG}"
  fi
done

for note in "${notes[@]}"; do
  echo "${note}"
done

if [[ "${#fails[@]}" -gt 0 ]]; then
  printf '%s\n' "${fails[@]}" >&2
  echo "check-semver-against-tag: ${#fails[@]} failure(s)" >&2
  exit 1
fi

if [[ "${cargo_status}" -ne 0 ]]; then
  echo "check-semver-against-tag: FAIL — cargo-semver-checks itself found a real semver break against ${TAG} (exit ${cargo_status}); read the output above" >&2
  exit "${cargo_status}"
fi

echo "check-semver-against-tag: OK — ${#PUBLISHED[@]} crates checked against ${TAG}, cargo-semver-checks exit 0"
