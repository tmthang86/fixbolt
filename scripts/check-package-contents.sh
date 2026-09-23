#!/usr/bin/env bash
# ADR-0160 decision 4: what a `.crate` holds is an allowlist (`include`), never
# the source tree and never `git ls-files`. `cargo package --list -p <crate>`
# is what actually asks cargo to resolve that allowlist — it packages nothing
# to disk, so this is cheap enough to run on its own — and this script reads
# that list for each of the six published crates (plan
# 2026-09-23-p3-packaging-and-first-release, row 6b) and asserts:
#
#   1. `fixbolt-dict` ships `NOTICE` and its three `spec/*.xml` files
#      (ADR-0104): without them the crate cannot build at all, on crates.io or
#      anywhere else, because `build.rs` reads nothing but `spec/`.
#   2. Every one of the six ships `README.md`, `LICENSE-MIT` and
#      `LICENSE-APACHE` — files crates.io and docs.rs render, and
#      `check-release-versions.sh` already holds byte-identical to the root
#      copies; this script only asks that they REACH the package.
#   3. None of the six ships a `tests/`, `benches/` or `vendor` path, or a
#      `.def` fixture. Their dev-dependencies are stripped by cargo on
#      publish (ADR-0160 decision 1 / decision 4), and half of the test
#      suites read `vendor/` besides, so a shipped test could not even build
#      from the `.crate` — carrying one anyway is dead weight a stranger pays
#      to download, and the `.def` corpus is QuickFIX's own test data, never
#      committed here in the first place (CLAUDE.md §2 non-negotiable 9).
#   4. Exactly six crates are published — the same count, and the same names.
#      `check-release-versions.sh` asserts this too, against the manifests
#      directly; this script asks the same question its own way, by reading
#      every OTHER workspace member's `publish` key, so a seventh crate
#      silently switched to `publish = true` fails HERE as well as there
#      (senior review of PR #104: the header used to claim this check without
#      the code making it — a member reading `publish = true` by mistake
#      would have passed every assertion this script actually ran).
#
# WHAT IT CANNOT SEE: whether the packaged sources actually BUILD — that is
# `scripts/check-packaged-build.sh` and the `package` CI job's dry run, which
# this script does not invoke; crates.io's own server-side checks (the name,
# the licence expression) — `cargo package --list` never reaches the server;
# a file added to `include` after this script was last taught to look for it.
# The list of six crate names, and the two per-crate checklists, are written
# out on purpose rather than discovered from the manifests — the same
# reasoning `check-release-versions.sh` gives: a crate quietly switched to
# publish = false (or fixbolt-dict quietly losing its NOTICE requirement)
# must fail loudly, not vanish from what is being asked.
#
# Exit 0 when every assertion holds; 1 on any FAIL (each printed, prefixed
# `FAIL`); 2 when the script itself cannot run (cargo not found, wrong cwd).
#
# Runs standalone: scripts/check-package-contents.sh — needs the crates.io
# index reachable the first time cargo resolves the workspace in a session
# (later runs re-use `~/.cargo/registry`); nothing here uploads or writes a
# `.crate` file.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

if ! command -v cargo >/dev/null 2>&1; then
  echo "check-package-contents: FAIL — cargo not found on PATH" >&2
  exit 2
fi

# The list of six is written here on purpose — see the header above and
# scripts/check-release-versions.sh, which gives the same reasoning for the
# same list.
PUBLISHED=(fixbolt-codec fixbolt-dict fixbolt-session fixbolt-engine fixbolt-sbe fixbolt)

# Required in every one of the six.
COMMON_REQUIRED=(README.md LICENSE-MIT LICENSE-APACHE)

# Required in fixbolt-dict alone (ADR-0104).
DICT_REQUIRED=(NOTICE spec/FIX44.xml spec/FIXT11.xml spec/FIX50SP2.xml)

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

fails=()
crates_checked=0

for name in "${PUBLISHED[@]}"; do
  out_file="${TMP}/${name}.out"
  err_file="${TMP}/${name}.err"

  if ! cargo package --list -p "${name}" >"${out_file}" 2>"${err_file}"; then
    fails+=("FAIL ${name}: cargo package --list failed:")
    while IFS= read -r line; do
      fails+=("    ${line}")
    done <"${err_file}"
    continue
  fi

  listing="$(sort "${out_file}")"
  crates_checked=$((crates_checked + 1))

  for req in "${COMMON_REQUIRED[@]}"; do
    if ! grep -qxF "${req}" <<<"${listing}"; then
      fails+=("FAIL ${name}: ${req} missing from package")
    fi
  done

  if [[ "${name}" == "fixbolt-dict" ]]; then
    for req in "${DICT_REQUIRED[@]}"; do
      if ! grep -qxF "${req}" <<<"${listing}"; then
        fails+=("FAIL ${name}: ${req} missing from package")
      fi
    done
  fi

  # tests/ and benches/ as path prefixes (a source file legitimately named
  # e.g. "src/tests.rs" must not trip this), a bare or nested "vendor" path
  # component, and any ".def" fixture wherever it sits.
  while IFS= read -r path; do
    [[ -z "${path}" ]] && continue
    case "${path}" in
      tests/* | */tests/*)
        fails+=("FAIL ${name}: tests/ shipped (${path})")
        ;;
      benches/* | */benches/*)
        fails+=("FAIL ${name}: benches/ shipped (${path})")
        ;;
      vendor | vendor/* | */vendor | */vendor/*)
        fails+=("FAIL ${name}: vendor shipped (${path})")
        ;;
      *.def)
        fails+=("FAIL ${name}: a .def fixture shipped (${path})")
        ;;
    esac
  done <<<"${listing}"
done

if [[ "${crates_checked}" -ne "${#PUBLISHED[@]}" ]]; then
  fails+=("FAIL: expected ${#PUBLISHED[@]} published crates checked, got ${crates_checked}")
fi

# --- no seventh crate: every OTHER workspace member must be publish = false -
# Independent of check-release-versions.sh, and reading the manifests
# directly rather than trusting `cargo package --list` above to have noticed
# — that loop only ever iterates the six names already in PUBLISHED, so it
# cannot see a member that was never on that list at all.
extra_published="$(python3 - "${ROOT}" <<'PY'
import pathlib
import sys
import tomllib

root = pathlib.Path(sys.argv[1])
published = {
    "fixbolt-codec",
    "fixbolt-dict",
    "fixbolt-session",
    "fixbolt-engine",
    "fixbolt-sbe",
    "fixbolt",
}

with open(root / "Cargo.toml", "rb") as f:
    ws = tomllib.load(f)

for member in ws.get("workspace", {}).get("members", []):
    manifest_path = root / member / "Cargo.toml"
    try:
        with open(manifest_path, "rb") as f:
            manifest = tomllib.load(f)
    except OSError:
        continue
    package = manifest.get("package", {})
    name = package.get("name", member)
    if name in published:
        continue
    if package.get("publish") is not False:
        print(f"{name} ({member}): publish = {package.get('publish')!r}, not false")
PY
)"
if [[ -n "${extra_published}" ]]; then
  while IFS= read -r line; do
    fails+=("FAIL: a seventh publishable crate — ${line}")
  done <<<"${extra_published}"
fi

if [[ "${#fails[@]}" -gt 0 ]]; then
  printf '%s\n' "${fails[@]}" >&2
  echo "check-package-contents: ${#fails[@]} failure(s)" >&2
  exit 1
fi

echo "check-package-contents: OK — ${crates_checked} crates, each with README.md/LICENSE-MIT/LICENSE-APACHE, fixbolt-dict with NOTICE + 3 spec/*.xml, none shipping tests/benches/vendor/.def"
