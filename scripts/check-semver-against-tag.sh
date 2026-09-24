#!/usr/bin/env bash
# ADR-0161 decision 5, ADR-0162, plan rows 8b and 8b'
# (docs/plans/2026-09-23-p3-packaging-and-first-release.md, *Sửa 2* and
# *Sửa 3*): once there is no crates.io upload, the API baseline a stranger's
# `Cargo.lock` actually sees is the last release tag, not `origin/main` — so
# the blocking semver gate compares `HEAD` against the newest release tag
# `HEAD` descends from. The tag is DERIVED, never passed in (ADR-0162): a
# hard-coded `v0.1.0` in `ci.yml` goes stale after the next tag, and a stale
# baseline makes every later version look like a major-level bump, which
# cargo-semver-checks skips — a gate that silently stops comparing anything
# while staying green.
#
# It is a wrapper, not a replacement, around
# `cargo semver-checks --workspace --baseline-rev <tag>`, because that tool
# has one hole ADR-0161 *Context* fact 6 measured directly: comparing
# identical `0.y.z` versions runs the real lint set (this repository's six
# crates: 196 checks, 58 skipped by design), but a `0.y` -> `0.(y+1)` bump
# is read as a MAJOR change and skips every single lint while still exiting
# 0 (`0 checks: 0 pass, 254 skip` per crate). So this script reads, per
# published crate, the actual `Checked […] N checks: …` line
# cargo-semver-checks itself prints, and applies ADR-0162 *Decision*:
#
#   1. The baseline B is the tag, among `git tag --merged HEAD`, named
#      exactly `vMAJOR.MINOR.PATCH` (no pre-release, no build metadata) with
#      the highest (major, minor, patch), compared as integers — never git's
#      own `--sort`, which local config can change. No such tag -> exit 2.
#      B's own `Cargo.toml` must declare B's version, or FAIL.
#   2. The workspace version V must be a plain `X.Y.Z` (else exit 2), not
#      behind B (else FAIL), and no tag `v<V>` may exist anywhere in the
#      repository other than B itself (else FAIL: a release of this exact
#      version exists that this history does not descend from — usually a
#      branch that needs rebasing onto `main`).
#   3. V == B -> every published crate must run N > 0 checks. No exception.
#   4. V > B  -> a 0-check crate is excused, with a NOTE, only if the bump is
#      major-level by Cargo's rule, computed from the numbers the way
#      cargo-semver-checks' `check_release.rs` does: V.major != B.major; or
#      V.major == 0 and V.minor != B.minor; or V.major == V.minor == 0 and
#      V.patch != B.patch. Any other V > B (`0.1.0` -> `0.1.1`) is rule 3.
#   5. cargo-semver-checks' own nonzero exit is returned unchanged.
#
# Rules 1 and 2 are settled BEFORE cargo-semver-checks runs: if the baseline
# is wrong, nothing it would print can be trusted.
#
# Both versions are read from the manifest text itself, with `tomllib`, the
# same way `scripts/check-release-versions.sh` already does — never from
# cargo-semver-checks' own `(no change; assume minor)` / `(major change)`
# wording in its output, which is that tool's phrasing for what IT assumed,
# not a fact this script can trust as an input to its own assumption.
#
# WHAT IT CANNOT SEE: a tag this checkout never fetched. A clone that lacks
# the newest release tag (`git clone --no-tags`, a partial `git fetch`)
# silently derives an OLDER baseline and compares against that — the one
# compensation is that the script prints B and every candidate it chose from
# (`baseline v0.1.0 = highest of: v0.1.0`) before anything else, so the log
# names what was compared; `ci.yml`'s `fetch-depth: 0` is what makes the CI
# checkout carry every tag. A checkout with NO release tag at all exits 2
# instead. Also unseen: a tag whose name is not a plain `vX.Y.Z` (such as a
# future `v1.0.0-rc.1`) is never a baseline candidate, by design; whether a
# tag has moved since this checkout last fetched it (`scripts/stranger-check.sh
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
# was excused by a major-level bump, printed as a NOTE) and
# cargo-semver-checks itself found nothing; the same nonzero status
# cargo-semver-checks itself returned when it found a real semver violation
# (a `function_missing` failure and friends — this script does not swallow
# that into its own exit 1); 1 when THIS script's own assertion fails (a
# baseline tag whose manifest disagrees with its name, a workspace version
# behind the baseline or released elsewhere, a crate missing from the output,
# or an unexcused 0-check crate); 2 when the script cannot run at all (an
# argument given, no release tag reachable from HEAD, a pre-release or
# unparsable workspace version, cargo-semver-checks not installed, a manifest
# that does not parse).
#
# Usage (no argument — the baseline is derived, ADR-0162):
#   scripts/check-semver-against-tag.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

if [[ $# -ne 0 ]]; then
  echo "check-semver-against-tag: FAIL — usage: check-semver-against-tag.sh (no argument: the baseline is the newest release tag HEAD descends from, ADR-0162)" >&2
  exit 2
fi

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

# --- ADR-0162 rules 1 and 2: derive B, read V, refuse a baseline that cannot
#     be trusted — all before cargo-semver-checks runs. Prints one line on
#     success: B <TAB> candidates <TAB> B's version <TAB> V <TAB> relation,
#     where relation is `same`, `major` or `not-major`. -----------------------
decision="$(python3 - "${ROOT}/Cargo.toml" <<'PY'
import re
import subprocess
import sys
import tomllib

ME = "check-semver-against-tag"
TAG_RE = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
VER_RE = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?P<pre>-[0-9A-Za-z.-]+)?(?P<build>\+[0-9A-Za-z.-]+)?"
)


def cannot_run(msg):
    print(f"{ME}: FAIL — {msg}", file=sys.stderr)
    sys.exit(2)


def fail(msg):
    print(f"FAIL semver: {msg}", file=sys.stderr)
    sys.exit(1)


def git(*args):
    r = subprocess.run(["git", *args], capture_output=True, text=True)
    return r.returncode, r.stdout


def workspace_version(text, where):
    try:
        ws = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        cannot_run(f"{where} does not parse as TOML: {e}")
    v = ws.get("workspace", {}).get("package", {}).get("version")
    if not isinstance(v, str):
        cannot_run(f"could not read [workspace.package] version from {where}")
    return v


# Rule 1: the candidates, and the highest of them as integers.
rc, out = git("tag", "--merged", "HEAD")
if rc != 0:
    cannot_run("`git tag --merged HEAD` failed — is this a git checkout with a HEAD?")
candidates = []
for name in out.splitlines():
    m = TAG_RE.fullmatch(name)
    if m:
        candidates.append((tuple(int(g) for g in m.groups()), name))
if not candidates:
    cannot_run("no release tag reachable from HEAD — git fetch --tags? (ADR-0162 rule 1)")
candidates.sort()
b_num, baseline = max(candidates)

rc, b_manifest = git("show", f"refs/tags/{baseline}:Cargo.toml")
if rc != 0:
    cannot_run(f"could not read Cargo.toml at {baseline} (git show refs/tags/{baseline}:Cargo.toml)")
b_version = workspace_version(b_manifest, f"{baseline}:Cargo.toml")
if b_version != baseline[1:]:
    fail(f"tag {baseline} points at a commit whose workspace version is {b_version}")

# Rule 2: V is plain, not behind B, and not released anywhere else.
with open(sys.argv[1], encoding="utf-8") as f:
    v_text = f.read()
v_version = workspace_version(v_text, "Cargo.toml")
m = VER_RE.fullmatch(v_version)
if not m:
    cannot_run(f"workspace version {v_version} is not a semver version")
if m.group("pre") or m.group("build"):
    cannot_run(
        f"workspace version {v_version} carries pre-release or build metadata — "
        "this gate compares plain X.Y.Z versions only (ADR-0162 rule 2)"
    )
v_num = tuple(int(m.group(i)) for i in (1, 2, 3))
if v_num < b_num:
    fail(f"workspace version {v_version} is behind the baseline {baseline}")
own_tag = f"v{v_version}"
rc, _ = git("rev-parse", "-q", "--verify", f"refs/tags/{own_tag}")
if rc == 0 and own_tag != baseline:
    print(
        f"FAIL semver: tag {own_tag} exists for workspace version {v_version} "
        f"but the baseline is {baseline}",
        file=sys.stderr,
    )
    print(
        f"  (a release of {v_version} exists that HEAD does not descend from — "
        "rebase onto main? ADR-0162 rule 2)",
        file=sys.stderr,
    )
    sys.exit(1)

# Rules 3 and 4: which kind of comparison cargo-semver-checks is about to make.
if v_num == b_num:
    relation = "same"
else:
    (vM, vm, vp), (bM, bm, bp) = v_num, b_num
    major = (
        vM != bM
        or (vM == 0 and vm != bm)
        or (vM == 0 and vm == 0 and vp != bp)
    )
    relation = "major" if major else "not-major"

print("\t".join([baseline, ", ".join(n for _, n in candidates), b_version, v_version, relation]))
PY
)"
decision_status=$?
if [[ "${decision_status}" -ne 0 ]]; then
  if [[ "${decision_status}" -eq 1 ]]; then
    echo "check-semver-against-tag: 1 failure(s) — cargo-semver-checks not run" >&2
  fi
  exit "${decision_status}"
fi
IFS=$'\t' read -r TAG CANDIDATES TAG_VERSION CURRENT_VERSION RELATION <<<"${decision}"
if [[ -z "${TAG:-}" || -z "${RELATION:-}" ]]; then
  echo "check-semver-against-tag: FAIL — could not derive a baseline (internal: '${decision}')" >&2
  exit 2
fi

echo "check-semver-against-tag: baseline ${TAG} = highest of: ${CANDIDATES}"
echo "check-semver-against-tag: workspace version ${CURRENT_VERSION}, baseline's ${TAG_VERSION} (${RELATION})"

# The six crates ADR-0160 publishes — named here, not read off the manifest,
# for the same reason scripts/check-release-versions.sh names them: a crate
# switched to publish = false by mistake must fail this check, not silently
# vanish from what it looks for.
PUBLISHED=(fixbolt-codec fixbolt-dict fixbolt-session fixbolt-engine fixbolt-sbe fixbolt)

log="$(mktemp)"
trap 'rm -f "${log}"' EXIT

echo "== cargo semver-checks --workspace --baseline-rev ${TAG} =="
# `--color never`: CARGO_TERM_COLOR=always (ci.yml's own workflow-level
# setting) wraps `Checking`/`Checked` in ANSI escapes that the plain-text
# regex below never matches — every crate then reads as "did not appear in
# cargo-semver-checks output at all", which is indistinguishable from a real
# missing crate without reading the raw log. `--color` on the command line
# overrides the environment variable; see docs/reference/cargo-output-
# colour-defeats-plain-text-parsing.md, the trap scripts/stranger-check.sh's
# `--from git` mode and scripts/check-indexing-debt.sh both paid for too.
cargo semver-checks --workspace --baseline-rev "${TAG}" --color never >"${log}" 2>&1
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
  if [[ "${n}" -gt 0 ]]; then
    echo "check-semver-against-tag: Checked ${name}: ${n} checks against ${TAG}"
    continue
  fi
  case "${RELATION}" in
    major)
      notes+=("NOTE: ${name} ran 0 checks against ${TAG} — ${TAG_VERSION} → ${CURRENT_VERSION} is a major-level bump (Cargo's rule, ADR-0162 rule 4); cargo-semver-checks skips every lint for it by design") ;;
    same)
      fails+=("FAIL semver: ${name} ran 0 checks against ${TAG} — workspace version ${CURRENT_VERSION} is the baseline's own, so 0 checks means nothing was compared (ADR-0162 rule 3)") ;;
    *)
      fails+=("FAIL semver: ${name} ran 0 checks against ${TAG} — ${TAG_VERSION} → ${CURRENT_VERSION} is not a major-level bump") ;;
  esac
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
