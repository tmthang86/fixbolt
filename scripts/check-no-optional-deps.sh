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
  # `[2026-09-02]` crates/library. It re-exports `serve`, which is `standard`
  # only, so it declares a `standard` feature of its own that forwards to
  # `fixbolt-engine/standard` — and a forwarding feature is exactly the shape
  # that puts an optional dependency back into a build that asked for nothing.
  # Asked per crate for the same reason `fixbolt-engine` is: at workspace scope
  # a sibling switches the flag back on and the question stops meaning what it
  # reads as.
  "fixbolt:libc"
  # `[2026-09-04]` tools/interop. It grew a `--role acceptor` that calls
  # `fixbolt::serve`, so it forwards `fixbolt/standard` — the same forwarding
  # shape as the line above, in a crate whose whole reason for existing is that
  # `cargo test --all --no-default-features` must build it on a machine with no
  # CMake. Asked per crate, because at workspace scope a sibling answers for it.
  "fixbolt-interop:libc"
)

rc=0
for case in "${CASES[@]}"; do
  crate="${case%%:*}"
  dep="${case##*:}"

  echo "== ${crate} --no-default-features must not pull ${dep} =="

  # The only unambiguous evidence is the tree itself: `-i <dep>` prints a root
  # line `<dep> v<version>` when the dependency really is in the normal graph.
  # Absence has TWO different messages and which one appears depends on
  # something unrelated to what is being asked:
  #
  #   "did not match any packages"  the crate is nowhere in the graph at all
  #   "nothing to print"            it is in the graph, but not through the
  #                                 edges `-e normal` selected — a dev- or
  #                                 build-dependency
  #
  # `[measured 2026-08-30]` that second message appeared the moment `libc`
  # became a dev-dependency of this crate as well, and the first version of
  # this script called it "could not tell" and failed. Failing was the right
  # thing to do — a check that cannot tell must never report ok — but both
  # messages mean the dependency is absent from what ships, so both pass.
  # Anything else is still a refusal to guess.
  out="$(cargo tree -p "${crate}" --no-default-features -e normal -i "${dep}" 2>&1)"
  if grep -qE "^${dep} v" <<<"${out}"; then
    echo "FAIL: ${dep} is in ${crate}'s normal dependency graph with no features on:" >&2
    echo "${out}" >&2
    rc=1
  elif grep -qE "did not match any packages|nothing to print" <<<"${out}"; then
    echo "ok — ${dep} is absent from what ships"
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
