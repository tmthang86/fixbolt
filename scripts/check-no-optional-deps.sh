#!/usr/bin/env bash
# CLAUDE.md §2 non-negotiable 6: THIS MUST BUILD ON A MACHINE WITH NOTHING
# OPTIONAL INSTALLED — and a feature flag must gate the `mod` declaration, not
# only the manifest.
#
# `[measured 2026-08-30]` THE JOB THAT WAS SUPPOSED TO PROVE THIS DID NOT.
# CI ran `cargo test --all --no-default-features` and that command **still
# builds `libc`**, because `tools/w2w` is a workspace member that depends on
# `fixbolt-engine` with its default features on, and cargo unifies features
# across everything it builds in one invocation. So the flag under test was
# switched back on by a sibling crate, and the job was green about a build that
# never happened. Observed, not reasoned:
#
#     $ cargo tree --workspace --no-default-features -e normal -i libc
#     libc v0.2.189
#     └── fixbolt-engine v0.0.0
#         └── fixbolt-w2w v0.0.0
#
# The fix is not to change `w2w` — it needs `standard` for `--mode standard`.
# It is to ask the question **per crate**, which is the only scope where
# `--no-default-features` means what it reads as.
#
# Run it with no argument for every crate this repository declares optional
# dependencies for.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

# crate:dependency that must be absent when the crate is built with no features.
CASES=(
  "fixbolt-engine:libc"
)

rc=0
for case in "${CASES[@]}"; do
  crate="${case%%:*}"
  dep="${case##*:}"

  echo "== ${crate} --no-default-features must not pull ${dep} =="

  # `-i <dep>` errors when the package is not in the graph at all, which is the
  # passing case. Distinguish "not there" from "the command broke" by looking at
  # the message rather than at the exit status alone.
  out="$(cargo tree -p "${crate}" --no-default-features -e normal -i "${dep}" 2>&1)"
  if grep -q "did not match any packages" <<<"${out}"; then
    echo "ok — ${dep} is absent"
  elif grep -q "^${dep} " <<<"${out}"; then
    echo "FAIL: ${dep} is in ${crate}'s dependency graph with no features on:" >&2
    echo "${out}" >&2
    rc=1
  else
    echo "FAIL: could not tell. cargo said:" >&2
    echo "${out}" >&2
    rc=1
  fi

  # And the crate must actually build and test that way. A dependency that is
  # absent from a crate that does not compile proves nothing.
  echo "== ${crate} --no-default-features must build and test =="
  if ! cargo test -p "${crate}" --no-default-features >/dev/null 2>&1; then
    echo "FAIL: ${crate} does not build or test with --no-default-features" >&2
    cargo test -p "${crate}" --no-default-features 2>&1 | tail -20 >&2
    rc=1
  else
    echo "ok — builds and tests"
  fi
done

exit "${rc}"
