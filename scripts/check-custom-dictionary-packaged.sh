#!/usr/bin/env bash
# ADR-0207 decisions 1, 3 and 4; plan docs/plans/2026-09-26-docs-for-embedders.md
# row 28: a user of `examples/custom-dictionary` builds it from the SIX
# `.crate` files, not from this tree. `scripts/check-packaged-build.sh`'s
# `fixbolt-dict-codegen*` cases (its own comment there) only prove `fixbolt-dict`
# with `codegen` compiles as a dependency of a scratch crate — they never run a
# caller's `build.rs` against it. Three things that check cannot see, and this
# script exists to prove instead:
#
#   1. `fixbolt_dict::codegen`'s `include_str!("../../spec/FIX44.xml")`
#      (`crates/dict/src/codegen/mod.rs`) is resolved against the FILES INSIDE
#      THE PACKAGED `fixbolt-dict` crate directory, at ITS OWN compile time —
#      so a `spec/**` dropped from that crate's `include` allowlist
#      (ADR-0160 decision 4) breaks a caller's `build.rs` the moment it turns
#      the `codegen` feature on, not only a check that reads `include` itself.
#   2. The Rust `codegen::generate` emits, compiled inside a caller's
#      `$OUT_DIR`, resolves `::fixbolt::dict::…` (`Paths::facade`, ADR-0207
#      decision 4) against the PACKAGED `fixbolt`, never a path dependency on
#      this tree.
#   3. `roxmltree` — pulled in only by `fixbolt-dict`'s `codegen` feature,
#      taken here as a BUILD-dependency (resolver 2+/3: a build-dependency's
#      features never reach the target build's feature graph) — never reaches
#      the NORMAL dependency graph a user's own binary, or `cargo install`,
#      resolves.
#
# This script builds a scratch crate — a straight copy of
# `examples/custom-dictionary`'s `build.rs`, `venue.xml` and `src/` — OUTSIDE
# this workspace and this repository entirely, in a system temp directory, so
# no path dependency could quietly still be reaching this tree. It depends on
# the packaged `fixbolt` (normal) and the packaged `fixbolt-dict` with
# `codegen` (build-dependency) through `[patch.crates-io]` onto
# `target/package/<name>-<version>/` — the same technique
# `scripts/check-packaged-build.sh` and `scripts/stranger-check.sh --from
# packaged` use, and, like both, it must run AFTER `cargo publish --workspace
# --dry-run` has populated that directory.
#
# WHAT IT CANNOT SEE: whether `docs/GETTING-STARTED.md` or any other prose
# describes this example accurately (CLAUDE.md §4's sync table, walked by
# hand); a `roxmltree` reachable only through a dev-dependency or a feature
# unification this scratch crate itself never turns on — `cargo tree -e
# normal` reads the graph THIS build actually resolved, nothing hypothetical;
# crates.io's own server-side checks; a `target/package/` built from a commit
# other than HEAD, unless the freshness check below catches it.
#
# Exit 0 when the scratch crate builds AND `cargo tree -e normal` contains no
# `roxmltree` line; 1 on either failure (each printed, prefixed FAIL, and the
# scratch crate is left on disk, named, for reading); 2 when the script itself
# cannot run (packaged sources missing/stale, a required tool absent, or
# examples/custom-dictionary itself missing one of the files it copies).
#
# Runs standalone, but only meaningfully AFTER a dry run has populated
# target/package/: scripts/check-custom-dictionary-packaged.sh [--allow-dirty]
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

ALLOW_DIRTY=0
for arg in "$@"; do
  case "${arg}" in
    --allow-dirty) ALLOW_DIRTY=1 ;;
    *)
      echo "check-custom-dictionary-packaged: FAIL — unknown argument: ${arg}" >&2
      exit 2
      ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "check-custom-dictionary-packaged: FAIL — cargo not found on PATH" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "check-custom-dictionary-packaged: FAIL — python3 not found on PATH" >&2
  exit 2
fi

EXAMPLE_DIR="${ROOT}/examples/custom-dictionary"
for f in build.rs venue.xml src/lib.rs src/main.rs; do
  if [[ ! -f "${EXAMPLE_DIR}/${f}" ]]; then
    echo "check-custom-dictionary-packaged: FAIL — examples/custom-dictionary/${f} not found" >&2
    exit 2
  fi
done

PUBLISHED=(fixbolt-codec fixbolt-dict fixbolt-session fixbolt-engine fixbolt-sbe fixbolt)

# The one workspace version, read the same way check-packaged-build.sh and
# stranger-check.sh read it: from [workspace.package] in Cargo.toml directly,
# never from `cargo metadata`.
VERSION="$(python3 - "${ROOT}" <<'PY'
import pathlib, sys, tomllib
root = pathlib.Path(sys.argv[1])
with open(root / "Cargo.toml", "rb") as f:
    ws = tomllib.load(f)
v = ws.get("workspace", {}).get("package", {}).get("version")
if not isinstance(v, str):
    sys.exit(1)
print(v)
PY
)"
if [[ -z "${VERSION}" ]]; then
  echo "check-custom-dictionary-packaged: FAIL — could not read [workspace.package] version from Cargo.toml" >&2
  exit 2
fi

# --- the packaged sources must already exist --------------------------------
missing=()
for name in "${PUBLISHED[@]}"; do
  dir="${ROOT}/target/package/${name}-${VERSION}"
  [[ -d "${dir}" ]] || missing+=("${dir}")
done
if [[ "${#missing[@]}" -gt 0 ]]; then
  echo "check-custom-dictionary-packaged: FAIL — the packaged sources are not there yet:" >&2
  printf '    %s\n' "${missing[@]}" >&2
  echo "    Run 'cargo publish --workspace --dry-run' first (no --allow-dirty" >&2
  echo "    on a clean checkout) — that is what leaves these directories," >&2
  echo "    byte-identical to what a .crate upload would contain." >&2
  exit 2
fi

# --- and built from THIS commit, not a stale one ----------------------------
# A dry run leaves target/package/ behind; nothing deletes it when the source
# changes underneath, so a green run against yesterday's bytes would prove
# nothing about today's crates/dict/Cargo.toml or examples/custom-dictionary.
if [[ "${ALLOW_DIRTY}" -ne 1 ]] && [[ -n "$(git -C "${ROOT}" status --porcelain 2>/dev/null)" ]]; then
  echo "check-custom-dictionary-packaged: FAIL — the working tree is dirty. A dirty" >&2
  echo "    tree can differ from HEAD in ways .cargo_vcs_info.json's commit sha" >&2
  echo "    cannot record, so a match against HEAD would prove nothing. Commit or" >&2
  echo "    stash first, or pass --allow-dirty for local iteration only." >&2
  exit 1
fi
HEAD_SHA="$(git -C "${ROOT}" rev-parse HEAD 2>/dev/null || true)"
if [[ -z "${HEAD_SHA}" ]]; then
  echo "check-custom-dictionary-packaged: FAIL — could not read 'git rev-parse HEAD' (not a git checkout?)" >&2
  exit 2
fi
stale=()
for name in "${PUBLISHED[@]}"; do
  info="${ROOT}/target/package/${name}-${VERSION}/.cargo_vcs_info.json"
  if [[ ! -f "${info}" ]]; then
    stale+=("${name}: no .cargo_vcs_info.json — re-run the dry run")
    continue
  fi
  sha="$(python3 -c 'import json, sys
print(json.load(open(sys.argv[1]))["git"]["sha1"])' "${info}" 2>/dev/null || true)"
  if [[ "${sha}" != "${HEAD_SHA}" ]]; then
    stale+=("${name}: packaged at ${sha:-<unreadable>}, HEAD is ${HEAD_SHA}")
  fi
done
if [[ "${#stale[@]}" -gt 0 ]]; then
  echo "check-custom-dictionary-packaged: FAIL — target/package/ is stale (built from a" >&2
  echo "    commit other than HEAD, per .cargo_vcs_info.json):" >&2
  printf '    %s\n' "${stale[@]}" >&2
  echo "    Run 'cargo publish --workspace --dry-run' again on this commit." >&2
  exit 1
fi

# One [patch.crates-io] block, reused from check-packaged-build.sh's own
# reasoning: it redirects all six names, whether or not this crate's own
# dependency graph reaches all of them.
PATCH_BLOCK="[patch.crates-io]
$(for name in "${PUBLISHED[@]}"; do
  echo "${name} = { path = \"${ROOT}/target/package/${name}-${VERSION}\" }"
done)"

# --- scratch crate, OUTSIDE the tree, in a real system temp dir -------------
# check-scratch-fixtures.sh's rule: entering a directory outside the tree
# needs the pinned rust-toolchain.toml copied in, or this build silently tests
# whatever toolchain the machine happens to default to, not the one CI pins.
scratch="$(mktemp -d)"
cleanup_ok=0
finish() {
  if [[ "${cleanup_ok}" -eq 1 ]]; then
    rm -rf "${scratch}"
  else
    echo "check-custom-dictionary-packaged: left the failing scratch crate at ${scratch}" >&2
  fi
}
trap finish EXIT

cp "${ROOT}/rust-toolchain.toml" "${scratch}/rust-toolchain.toml"
mkdir -p "${scratch}/src"
cp "${EXAMPLE_DIR}/build.rs" "${scratch}/build.rs"
cp "${EXAMPLE_DIR}/venue.xml" "${scratch}/venue.xml"
cp "${EXAMPLE_DIR}/src/lib.rs" "${scratch}/src/lib.rs"
cp "${EXAMPLE_DIR}/src/main.rs" "${scratch}/src/main.rs"

# The dependency/feature shape is examples/custom-dictionary/Cargo.toml's own
# [features]/[dependencies]/[build-dependencies], verbatim, with the two path
# dependencies swapped for version requirements the [patch.crates-io] block
# below redirects onto target/package/.
{
  echo "[workspace]"
  echo
  echo "[package]"
  echo "name = \"custom-dictionary-packaged-check\""
  echo "version = \"0.0.0\""
  echo "edition = \"2024\""
  echo "publish = false"
  echo
  echo "[features]"
  echo "default = [\"standard\"]"
  echo "standard = [\"fixbolt/standard\"]"
  echo
  echo "[dependencies]"
  echo "fixbolt = { version = \"=${VERSION}\", default-features = false }"
  echo
  echo "[build-dependencies]"
  echo "fixbolt-dict = { version = \"=${VERSION}\", features = [\"codegen\"] }"
  echo
  echo "[lib]"
  echo "name = \"custom_dictionary\""
  echo "path = \"src/lib.rs\""
  echo
  echo "[[bin]]"
  echo "name = \"custom-dictionary\""
  echo "path = \"src/main.rs\""
  echo
  echo "${PATCH_BLOCK}"
} >"${scratch}/Cargo.toml"

status=0

echo "== building examples/custom-dictionary's build.rs + src/ against the packaged .crate files (fixbolt=${VERSION}, fixbolt-dict+codegen=${VERSION}) =="
build_log="${scratch}/build.log"
if cargo build --manifest-path "${scratch}/Cargo.toml" >"${build_log}" 2>&1; then
  tail -1 "${build_log}"
else
  echo "FAIL: the scratch crate did not build against the packaged sources. Left at ${scratch}:" >&2
  tail -60 "${build_log}" >&2
  status=1
fi

if [[ "${status}" -eq 0 ]]; then
  echo "== cargo tree -e normal: roxmltree must not be in it =="
  tree_log="${scratch}/tree.log"
  if ! cargo tree --manifest-path "${scratch}/Cargo.toml" -e normal >"${tree_log}" 2>&1; then
    echo "FAIL: cargo tree -e normal did not run. Left at ${scratch}:" >&2
    cat "${tree_log}" >&2
    status=1
  elif grep -qi 'roxmltree' "${tree_log}"; then
    echo "FAIL: roxmltree is in the normal dependency graph — a user's own binary would carry the XML parser it should only need at build time:" >&2
    grep -i 'roxmltree' "${tree_log}" >&2
    status=1
  else
    echo "ok — roxmltree absent from cargo tree -e normal"
  fi
fi

if [[ "${status}" -ne 0 ]]; then
  exit 1
fi

cleanup_ok=1
echo "check-custom-dictionary-packaged: OK — examples/custom-dictionary built against the packaged .crate files at ${VERSION} (fixbolt normal, fixbolt-dict+codegen build-only), roxmltree absent from cargo tree -e normal"
