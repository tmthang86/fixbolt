#!/usr/bin/env bash
# ADR-0214 decision 1, plan docs/plans/2026-09-28-ci-tiers.md *Cách làm* 1:
# `ci.yml` skips a docs-only diff via a `paths` filter, and this repository's
# own definition of "documentation" has an exception — a `docs/…` or root
# `*.md` file a code gate reads at run time must NOT be skipped. Two checks,
# and neither stands in for the other:
#
#   1. every place under crates/, tools/, examples/, benches/, fuzz/ or
#      spikes/ that reads a `docs/…` or root `*.md` file at build or test
#      time — `concat!(env!("CARGO_MANIFEST_DIR"), "…")` or `include_str!`
#      reaching outside its own crate — has a matching re-include line
#      (`- 'docs/…'`) in BOTH of `ci.yml`'s `paths` lists;
#   2. the `push` and `pull_request` `paths` lists in `ci.yml` are
#      byte-identical, line for line.
#
# What this script CANNOT see: a docs read reached through anything other
# than the two literal patterns above (a path built by string concatenation
# at runtime with `+`, or handed in via an environment variable, would not
# match). It also does not run cargo — a new crate under `crates/` with no
# `Cargo.toml` yet committed will not resolve `CARGO_MANIFEST_DIR` reads
# correctly; that is a real gap, not a false negative to explain away, and a
# plan adding such a crate should re-run this check by hand once the
# manifest exists.
#
# Runs standalone: scripts/check-ci-paths-filter.sh
# Reversal (plan R8): drop a re-include line from `paths` → this script FAILs
# naming the file that reads it; restore it, then make the two `paths` lists
# differ → this script FAILs "lists differ"; restore, back to ok.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

WORKFLOW=".github/workflows/ci.yml"

if [ ! -f "${WORKFLOW}" ]; then
  echo "check-ci-paths-filter: FAIL — ${WORKFLOW} does not exist" >&2
  exit 1
fi

status=0

# ---------------------------------------------------------------------------
# 1. Extract the two `paths:` lists from the `on:` block.
# ---------------------------------------------------------------------------

extract_paths() {
  # $1 = the exact top-level key line to anchor on, e.g. "  push:"
  awk -v anchor="$1" '
    $0 == anchor { found = 1; next }
    found && /^    paths:$/ { in_paths = 1; next }
    in_paths && /^      - / { print; next }
    in_paths { exit }
  ' "${WORKFLOW}"
}

push_paths="$(extract_paths '  push:')"
pr_paths="$(extract_paths '  pull_request:')"

if [ -z "${push_paths}" ] || [ -z "${pr_paths}" ]; then
  echo "check-ci-paths-filter: FAIL — could not find both 'paths:' lists under 'on: push:' and 'on: pull_request:' in ${WORKFLOW}. This script's awk anchors on the literal lines '  push:' and '  pull_request:' immediately under 'on:', each followed by a '    paths:' key; a reindent breaks this extraction, not the filter itself." >&2
  exit 1
fi

if [ "${push_paths}" != "${pr_paths}" ]; then
  echo "check-ci-paths-filter: FAIL — push and pull_request paths lists differ:" >&2
  diff <(printf '%s\n' "${push_paths}") <(printf '%s\n' "${pr_paths}") >&2 || true
  status=1
else
  n="$(printf '%s\n' "${push_paths}" | grep -c '^      - ')"
  echo "check-ci-paths-filter: ok — push and pull_request paths lists are identical (${n} entries)"
fi

# ---------------------------------------------------------------------------
# 2. Find every runtime docs/root-*.md read under the named directories, and
#    check each one has a re-include line in both lists above.
# ---------------------------------------------------------------------------

SCAN_DIRS=""
for d in crates tools examples benches fuzz spikes; do
  if [ -d "${d}" ]; then
    SCAN_DIRS="${SCAN_DIRS} ${d}"
  fi
done

# A resolved, repo-root-relative path counts as "documentation" the same way
# ADR-0214 decision 1 defines it for the filter itself: under docs/, or a
# bare filename ending in .md sitting at the repository root (no further
# slash — a nested */*.md is code, e.g. crates/*/README.md).
is_watched_doc_path() {
  case "$1" in
    docs/*) return 0 ;;
    */*) return 1 ;;
    *.md) return 0 ;;
    *) return 1 ;;
  esac
}

find_crate_root() {
  # $1 = a source file; walks up from its directory to the nearest
  # Cargo.toml, which is what CARGO_MANIFEST_DIR resolves to for that file.
  d="$(cd "$(dirname "$1")" && pwd)"
  while :; do
    if [ -f "${d}/Cargo.toml" ]; then
      printf '%s\n' "${d}"
      return 0
    fi
    if [ "${d}" = "${ROOT}" ] || [ "${d}" = "/" ]; then
      return 1
    fi
    d="$(dirname "${d}")"
  done
}

resolve_path() {
  # $1 = absolute base directory, $2 = a relative path from that base.
  base="$1"
  rel="$2"
  reldir="$(dirname "${rel}")"
  relname="$(basename "${rel}")"
  fulldir="$(cd "${base}/${reldir}" 2>/dev/null && pwd)" || return 1
  printf '%s/%s\n' "${fulldir}" "${relname}"
}

first_quoted() {
  grep -o '"[^"]*"' | head -n1 | sed -e 's/^"//' -e 's/"$//'
}

hits_file="$(mktemp)"
trap 'rm -f "${hits_file}"' EXIT

if [ -n "${SCAN_DIRS}" ]; then
  # shellcheck disable=SC2086
  grep -rln 'env!("CARGO_MANIFEST_DIR")' ${SCAN_DIRS} --include='*.rs' 2>/dev/null | while IFS= read -r file; do
    grep -n 'env!("CARGO_MANIFEST_DIR")' "${file}" | while IFS=: read -r line _; do
      # `concat!(env!("CARGO_MANIFEST_DIR"), "…")` can wrap the path onto a
      # later line (crates/session/tests/drop_reason.rs does), so a handful
      # of lines are joined into one before looking for the FIRST quoted
      # literal AFTER the CARGO_MANIFEST_DIR marker — not the last quoted
      # literal in the window, which would as happily match an unrelated
      # string constant two statements down.
      window="$(sed -n "${line},$((line + 4))p" "${file}" | tr '\n' ' ')"
      after_marker="${window#*CARGO_MANIFEST_DIR\")}"
      relpath="$(printf '%s' "${after_marker}" | first_quoted)"
      case "${relpath}" in
        ""|*CARGO_MANIFEST_DIR*|*OUT_DIR*) continue ;;
      esac
      crate_dir="$(find_crate_root "${file}")" || continue
      resolved="$(resolve_path "${crate_dir}" "${relpath}")" || continue
      rel_to_root="${resolved#"${ROOT}"/}"
      if is_watched_doc_path "${rel_to_root}"; then
        printf '%s:%s:%s\n' "${file}" "${line}" "${rel_to_root}" >>"${hits_file}"
      fi
    done
  done

  # shellcheck disable=SC2086
  grep -rln 'include_str!(' ${SCAN_DIRS} --include='*.rs' 2>/dev/null | while IFS= read -r file; do
    grep -n 'include_str!(' "${file}" | while IFS=: read -r line content; do
      case "${content}" in *OUT_DIR*) continue ;; esac
      relpath="$(printf '%s\n' "${content}" | first_quoted)"
      [ -z "${relpath}" ] && continue
      filedir="$(cd "$(dirname "${file}")" && pwd)"
      resolved="$(resolve_path "${filedir}" "${relpath}")" || continue
      rel_to_root="${resolved#"${ROOT}"/}"
      if is_watched_doc_path "${rel_to_root}"; then
        printf '%s:%s:%s\n' "${file}" "${line}" "${rel_to_root}" >>"${hits_file}"
      fi
    done
  done
fi

missing=0
watched=0
if [ -s "${hits_file}" ]; then
  while IFS=: read -r file line relpath; do
    watched=$((watched + 1))
    ok=1
    if ! printf '%s\n' "${push_paths}" | grep -qxF "      - '${relpath}'"; then
      echo "check-ci-paths-filter: FAIL — ${file}:${line} reads ${relpath} at run time, but ci.yml's push paths list has no '- '${relpath}'' re-include line" >&2
      ok=0
    fi
    if ! printf '%s\n' "${pr_paths}" | grep -qxF "      - '${relpath}'"; then
      echo "check-ci-paths-filter: FAIL — ${file}:${line} reads ${relpath} at run time, but ci.yml's pull_request paths list has no '- '${relpath}'' re-include line" >&2
      ok=0
    fi
    if [ "${ok}" -eq 0 ]; then
      missing=$((missing + 1))
      status=1
    fi
  done <"${hits_file}"
fi

if [ "${watched}" -gt 0 ] && [ "${missing}" -eq 0 ]; then
  echo "check-ci-paths-filter: ok — ${watched} runtime docs/root-*.md read(s) found, all re-included"
elif [ "${watched}" -eq 0 ]; then
  echo "check-ci-paths-filter: ok — 0 runtime docs/root-*.md reads found under$( [ -n "${SCAN_DIRS}" ] && printf '%s' "${SCAN_DIRS}" || echo ' (no scan dirs present)')"
fi

exit "${status}"
