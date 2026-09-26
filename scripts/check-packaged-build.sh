#!/usr/bin/env bash
# ADR-0160 decision 6: before the first publish, the PACKAGED SOURCES are the
# stranger. `cargo publish --workspace --dry-run` proves the six crates build
# against EACH OTHER'S packaged sources, but only with default features
# (docs/reference/publishing-a-workspace-to-crates-io.md trap 7) — a feature a
# real user picks, on the exact bytes that would upload, is untested by it.
#
# This script builds a temporary crate per feature combination, outside the
# workspace (`target/packaged-build/<case>/`, its own `[workspace]`, never
# added to the root manifest's `members`), depending on the published crate
# through an ordinary registry-style version requirement
# (`fixbolt-engine = "=<version>"`) that `[patch.crates-io]` redirects to
# `target/package/<name>-<version>/` — the directory `cargo publish
# --workspace --dry-run` (or `cargo package`) leaves behind, byte-identical to
# what a `.crate` upload would contain. **This must run after that dry run**;
# see the check below.
#
# Two toolchains, because ADR-0160 decision 3's `rust-version` — raised from
# "1.88" to "1.89" by ADR-0154 decision 1, for `File::try_lock` — is prose
# until something builds AT it (plan trap 7, reference page trap 3):
#
#   - on the pinned default toolchain (`rust-toolchain.toml`, currently
#     1.98.0): eleven cases, each a single feature switched on beside the
#     default, one crate at a time — "one crate turns every feature on"
#     would hide a feature that is broken standing alone (plan trap "một
#     crate tạm bật mọi feature che mất một feature hỏng khi đứng riêng").
#   - on `+1.89.0`, the declared MSRV: the combined-everything build that
#     reference page trap 4 already measured by hand on the then-declared
#     `+1.88.0` (`cargo +1.88.0 check -p fixbolt --all-features` and `-p
#     fixbolt-engine --all-features`, both finish on 1.88.0 and fail with
#     E0658 on 1.85.0), plus `fixbolt --no-default-features` — the featureless
#     build non-negotiable 6 requires, now proven on the floor toolchain too,
#     and `fixbolt-dict` with `codegen,fix50sp2` (ADR-0207 decision 1).
#
# WHAT IT CANNOT SEE: whether `+1.89.0` is installed (`rustup toolchain
# install 1.89.0` is a step of the `package` CI job, run before this script;
# locally, rustup normally fetches it on first use — read the output, not the
# exit status, same as everywhere else in this repository); crates.io's own
# server-side checks; a ninth optional dependency added to a manifest and
# never named in a case below, the same limit `check-no-optional-deps.sh`'s
# CASES array has.
#
# Exit 0 when every case's `cargo build` finishes; 1 on any FAIL (the
# scratch directory of a failing case is left behind and named, for reading
# the error — every other scratch directory is removed as its case passes);
# 2 when the script itself cannot run (packaged sources missing or stale).
#
# **A stale `target/package/` is a false green.** `cargo publish --workspace
# --dry-run` leaves those directories behind; nothing deletes them when the
# source changes underneath, so a case run against yesterday's packaged
# bytes reports on code that is no longer in `crates/`. `.cargo_vcs_info.json`
# inside each packaged crate records the commit `cargo package` ran at
# (`{"git":{"sha1":"..."}}`); this script refuses unless every one of the six
# matches `git rev-parse HEAD` here — and, since a dirty working tree can
# differ from HEAD in ways no commit records, refuses a dirty tree outright
# unless `--allow-dirty` is passed (local iteration only: a dirty tree still
# proves nothing about what HEAD's packaged bytes are).
#
# Runs standalone, but only meaningfully AFTER a dry run has populated
# target/package/: scripts/check-packaged-build.sh [--allow-dirty]
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

ALLOW_DIRTY=0
for arg in "$@"; do
  case "${arg}" in
    --allow-dirty) ALLOW_DIRTY=1 ;;
    *)
      echo "check-packaged-build: FAIL — unknown argument: ${arg}" >&2
      exit 2
      ;;
  esac
done

PUBLISHED=(fixbolt-codec fixbolt-dict fixbolt-session fixbolt-engine fixbolt-sbe fixbolt)

# The one workspace version, read the same way check-release-versions.sh
# reads it — from the manifest, not from `cargo metadata` (which would
# resolve `version.workspace = true` to the same string either way and
# cannot tell the difference the caller cares about here).
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
  echo "check-packaged-build: FAIL — could not read [workspace.package] version from Cargo.toml" >&2
  exit 2
fi

# --- the packaged sources must already exist -------------------------------
missing=()
for name in "${PUBLISHED[@]}"; do
  dir="${ROOT}/target/package/${name}-${VERSION}"
  [[ -d "${dir}" ]] || missing+=("${dir}")
done
if [[ "${#missing[@]}" -gt 0 ]]; then
  echo "check-packaged-build: FAIL — the packaged sources are not there yet:" >&2
  printf '    %s\n' "${missing[@]}" >&2
  echo "    Run 'cargo publish --workspace --dry-run' first (no --allow-dirty" >&2
  echo "    on a clean checkout) — that is what leaves these directories," >&2
  echo "    byte-identical to what a .crate upload would contain." >&2
  exit 2
fi

# --- the packaged sources must be built from THIS commit, not a stale one --
if [[ "${ALLOW_DIRTY}" -ne 1 ]] && [[ -n "$(git -C "${ROOT}" status --porcelain 2>/dev/null)" ]]; then
  echo "check-packaged-build: FAIL — the working tree is dirty. A dirty tree" >&2
  echo "    can differ from HEAD in ways .cargo_vcs_info.json's commit sha" >&2
  echo "    cannot record, so a match against HEAD would prove nothing. Commit" >&2
  echo "    or stash first, or pass --allow-dirty for local iteration only." >&2
  exit 1
fi
HEAD_SHA="$(git -C "${ROOT}" rev-parse HEAD 2>/dev/null || true)"
if [[ -z "${HEAD_SHA}" ]]; then
  echo "check-packaged-build: FAIL — could not read 'git rev-parse HEAD' (not a git checkout?)" >&2
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
  echo "check-packaged-build: FAIL — target/package/ is stale (built from a" >&2
  echo "    commit other than HEAD, per .cargo_vcs_info.json):" >&2
  printf '    %s\n' "${stale[@]}" >&2
  echo "    Run 'cargo publish --workspace --dry-run' again on this commit." >&2
  exit 1
fi

# One [patch.crates-io] block, reused by every case: it redirects ALL six
# names to their packaged sources, whether or not the case under test
# reaches all of them, exactly as the manual reference-page measurement did.
PATCH_BLOCK="[patch.crates-io]
$(for name in "${PUBLISHED[@]}"; do
  echo "${name} = { path = \"${ROOT}/target/package/${name}-${VERSION}\" }"
done)"

WORKDIR="${ROOT}/target/packaged-build"
mkdir -p "${WORKDIR}"

# Each case: label, the dependency crate under test, its default-features
# setting, and a comma-separated feature list (empty = none beyond default),
# and the toolchain to build it with (empty = the pinned default).
#
# The eleven on the default toolchain: one feature at a time, beside whatever
# is on by default, one crate at a time (fixbolt-dict's `codegen` twice: alone
# and beside `fix50sp2`).
DEFAULT_CASES=(
  "fixbolt-default|fixbolt|true|"
  "fixbolt-no-default|fixbolt|false|"
  "fixbolt-sbe|fixbolt|true|sbe"
  "fixbolt-engine-no-default|fixbolt-engine|false|"
  "fixbolt-engine-tls|fixbolt-engine|true|tls"
  "fixbolt-engine-affinity|fixbolt-engine|true|affinity"
  "fixbolt-engine-fix50sp2|fixbolt-engine|true|fix50sp2"
  "fixbolt-sbe-no-default|fixbolt-sbe|false|"
  "fixbolt-codec-alone|fixbolt-codec|true|"
  # ADR-0207 decision 1: the generator as a library, from the packaged
  # sources — `src/codegen/` and the optional `roxmltree` must both reach
  # the `.crate`. Compiling it is what this proves; running it against the
  # packaged `spec/` is plan row 28's check-custom-dictionary-packaged.sh.
  "fixbolt-dict-codegen|fixbolt-dict|false|codegen"
  "fixbolt-dict-codegen-fix50sp2|fixbolt-dict|true|codegen,fix50sp2"
)

# The four on +1.89.0 (the declared rust-version, ADR-0154 decision 1): the
# combined-everything build for the three crates that have more than one
# feature (the exact combination reference page trap 4 measured by hand on
# the then-declared 1.88.0), and the featureless build non-negotiable 6
# requires, now proven on the floor toolchain.
MSRV="1.89.0"
MSRV_CASES=(
  "fixbolt-all-features-msrv|fixbolt|true|standard,sbe"
  "fixbolt-engine-all-features-msrv|fixbolt-engine|true|standard,affinity,tls,fix50sp2"
  "fixbolt-no-default-msrv|fixbolt|false|"
  "fixbolt-dict-all-features-msrv|fixbolt-dict|true|codegen,fix50sp2"
)

status=0
ran=0

run_case() {
  local label="$1" crate="$2" default_features="$3" features="$4" toolchain="$5"
  ran=$((ran + 1))

  local scratch
  scratch="$(mktemp -d -p "${WORKDIR}")"
  mkdir -p "${scratch}/src"
  # The scratch crate lives under target/, inside this repository, so
  # rust-toolchain.toml is already found by rustup's own upward walk — but
  # check-scratch-fixtures.sh (and the class of bug it guards, "a scratch
  # fixture inherits the machine") does not read that far; it asks that
  # every script entering a directory built from `mktemp` copy the pin in,
  # regardless of whether the walk would have found it anyway. Belt and
  # braces, and correct if this ever moves outside the tree (a different
  # CARGO_TARGET_DIR, for instance).
  cp "${ROOT}/rust-toolchain.toml" "${scratch}/rust-toolchain.toml"

  local dep_line
  if [[ -n "${features}" ]]; then
    local feat_toml
    feat_toml="$(python3 -c "import sys; print(', '.join('\"%s\"' % f for f in sys.argv[1].split(',')))" "${features}")"
    dep_line="${crate} = { version = \"=${VERSION}\", default-features = ${default_features}, features = [${feat_toml}] }"
  else
    dep_line="${crate} = { version = \"=${VERSION}\", default-features = ${default_features} }"
  fi

  {
    echo "[workspace]"
    echo
    echo "[package]"
    echo "name = \"packaged-build-check\""
    echo "version = \"0.0.0\""
    echo "edition = \"2024\""
    echo "publish = false"
    echo
    echo "[dependencies]"
    echo "${dep_line}"
    echo
    echo "${PATCH_BLOCK}"
  } >"${scratch}/Cargo.toml"

  local ident="${crate//-/_}"
  echo "pub use ${ident} as _packaged_build_check;" >"${scratch}/src/lib.rs"

  local cargo_bin=(cargo)
  [[ -n "${toolchain}" ]] && cargo_bin=(cargo "+${toolchain}")

  echo "== ${label}: ${dep_line} (${toolchain:-default toolchain}) =="
  local log="${scratch}/build.log"
  if "${cargo_bin[@]}" build --manifest-path "${scratch}/Cargo.toml" >"${log}" 2>&1; then
    tail -1 "${log}"
    rm -rf "${scratch}"
  else
    echo "FAIL: ${label} did not build. Left at ${scratch}:" >&2
    tail -40 "${log}" >&2
    status=1
  fi
}

for case in "${DEFAULT_CASES[@]}"; do
  IFS='|' read -r label crate default_features features <<<"${case}"
  run_case "${label}" "${crate}" "${default_features}" "${features}" ""
done

for case in "${MSRV_CASES[@]}"; do
  IFS='|' read -r label crate default_features features <<<"${case}"
  run_case "${label}" "${crate}" "${default_features}" "${features}" "${MSRV}"
done

if [[ "${ran}" -eq 0 ]]; then
  echo "check-packaged-build: FAIL — 0 cases ran" >&2
  exit 1
fi

if [[ "${status}" -ne 0 ]]; then
  echo "check-packaged-build: at least one case failed" >&2
  exit 1
fi

echo "check-packaged-build: OK — ${ran} cases, packaged sources at ${VERSION}, both the pinned toolchain and +${MSRV}"
