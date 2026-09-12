#!/usr/bin/env bash
# CLAUDE.md §2 non-negotiable 6, and STATUS.md item 68 — the same shape as
# item 62 one layer in.
#
# The `tls` job in ci.yml used to run `cargo test --test tls --test tls_wire
# --test tls_mode --test tls_settings_wire --test settings`: a hand-written
# list of file names. `tests/tls_settings_wire.rs` and, separately, three
# `#[cfg(feature = "tls")]` tests living inside `tests/settings.rs` (a file
# the list had no reason to name, since most of it is not about TLS) both
# ran on one desk and nowhere else, because the list did not name them. A
# list that must be remembered to be extended is not a gate; it is a wish.
#
# This script deletes the list. It asks the build itself which tests exist
# under FEATURE (`cargo test -- --list`), then reads a log of an actual run
# under the same feature and requires every listed test to have run, inside
# its own binary's section. `ok` and `FAILED` both mean "it ran"; `ignored`
# means the test exists and did not execute, and is reported rather than
# silently accepted as present.
#
# --tests builds the lib's unit tests plus every integration test target
# (anything cargo would run for `cargo test --tests`); it does not build
# benches (`harness = false` targets) or examples, so a test gated behind
# either is out of scope for this script by construction, same as it is out
# of scope for the `--tests` flag itself.
#
# Usage: check-feature-gated-tests-ran.sh PACKAGE FEATURE LOG
#   PACKAGE  the -p argument to cargo, e.g. fixbolt-engine. Exactly one
#            crate: a feature flag means what it reads as only at this
#            scope, not at the workspace's — see
#            docs/reference/feature-flags-unify-across-a-workspace.md. This
#            script deliberately has no --all / --workspace mode.
#   FEATURE  the --features argument, e.g. tls.
#   LOG      path to a file holding the stdout+stderr of
#            `cargo test -p PACKAGE --tests --features FEATURE --no-fail-fast`
#            (or a log that reads the same way — see the reversals below).
#
# A `cargo` failure while listing (a misspelt feature name, a build error) is
# reported as the error and exits 2; it is never read as a pass.
set -uo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 PACKAGE FEATURE LOG" >&2
  exit 2
fi

package="$1"
feature="$2"
log_file="$3"

if [ ! -f "${log_file}" ]; then
  echo "FAIL — log file not found: ${log_file}" >&2
  exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${root}" || exit 2

# --- 1. List: ask the build what exists under FEATURE, rather than reading
# a spelling anywhere in the source. `--format terse` prints one line per
# test, "<name>: test", grouped under a "Running <binary> (<path>)" line per
# compiled test binary.
list_out="$(cargo test -p "${package}" --tests --features "${feature}" -- --list --format terse 2>&1)"
list_status=$?
if [ "${list_status}" -ne 0 ]; then
  echo "FAIL — cargo could not list tests for -p ${package} --features ${feature}:" >&2
  echo "${list_out}" >&2
  exit 2
fi

declare -a listed_paths=()
declare -a listed_names=()
current_path=""
binaries=0

while IFS= read -r line; do
  if [[ "${line}" =~ ^[[:space:]]*Running[[:space:]]+(.+)[[:space:]]\(.*\)[[:space:]]*$ ]]; then
    running="${BASH_REMATCH[1]}"
    # "unittests src/lib.rs" -> "src/lib.rs"; "tests/foo.rs" is already the
    # form we want, as is the (excluded by --tests, but handled the same
    # way if it ever appears) "benches/foo.rs".
    current_path="${running#unittests }"
    binaries=$((binaries + 1))
    continue
  fi
  if [[ "${line}" =~ ^(.+):[[:space:]]test$ ]]; then
    name="${BASH_REMATCH[1]}"
    if [ -z "${current_path}" ]; then
      echo "FAIL — a test line appeared before any 'Running' line: ${line}" >&2
      exit 2
    fi
    listed_paths+=("${current_path}")
    listed_names+=("${name}")
  fi
done <<<"${list_out}"

listed_count="${#listed_names[@]}"
if [ "${listed_count}" -eq 0 ]; then
  echo "FAIL C0 — listed nothing: cargo test -p ${package} --tests --features ${feature} -- --list produced no tests. This is STATUS.md item 62 one layer up." >&2
  echo "${list_out}" >&2
  exit 1
fi

# --- 2. Reconcile: LOG is sectioned by the same "Running" lines; every
# listed (path, name) pair must appear in its own binary's section, either
# as "ok"/"FAILED" (it ran) or "ignored" (it exists and did not run — kept
# separate, and counted, rather than treated as accounted for).
declare -A ran_ok=()
declare -A ran_ignored=()
log_path=""

while IFS= read -r line; do
  if [[ "${line}" =~ ^[[:space:]]*Running[[:space:]]+(.+)[[:space:]]\(.*\)[[:space:]]*$ ]]; then
    running="${BASH_REMATCH[1]}"
    log_path="${running#unittests }"
    continue
  fi
  if [[ "${line}" =~ ^test[[:space:]](.+)[[:space:]]\.\.\.[[:space:]](ok|FAILED|ignored)$ ]]; then
    name="${BASH_REMATCH[1]}"
    status="${BASH_REMATCH[2]}"
    key="${log_path}"$'\t'"${name}"
    if [ "${status}" = "ignored" ]; then
      ran_ignored["${key}"]=1
    else
      ran_ok["${key}"]=1
    fi
  fi
done <"${log_file}"

fail=0
accounted=0
ignored_count=0
for i in "${!listed_names[@]}"; do
  path="${listed_paths[$i]}"
  name="${listed_names[$i]}"
  key="${path}"$'\t'"${name}"
  if [ -n "${ran_ok[${key}]+x}" ]; then
    accounted=$((accounted + 1))
  elif [ -n "${ran_ignored[${key}]+x}" ]; then
    accounted=$((accounted + 1))
    ignored_count=$((ignored_count + 1))
  else
    echo "FAIL — ${path}::${name} is in the ${feature} build and did not run" >&2
    fail=1
  fi
done

if [ "${fail}" -ne 0 ]; then
  echo "check-feature-gated-tests-ran: FAIL — ${package} --features ${feature}: ${binaries} binaries, ${listed_count} listed, ${accounted} accounted for (${ignored_count} ignored)" >&2
  exit 1
fi

echo "check-feature-gated-tests-ran: ok — ${package} --features ${feature}: ${binaries} binaries, ${listed_count} listed, ${accounted} accounted for (${ignored_count} ignored)"
