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
  # `[2026-09-09]` wave B plan 4. `rustls` and `ktls-core` are the first
  # dependencies here that pull trees of their own — `ring`, `zeroize`,
  # `subtle` and the rest — so a leak is not one crate appearing where it
  # should not be, it is a dozen. Both are asked for separately rather than
  # trusting that gating one gates the other: `tls = [...]` names three
  # `dep:` entries and a typo in any one of them is invisible until asked.
  "fixbolt-engine:rustls"
  "fixbolt-engine:ktls-core"
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
  # `[2026-09-23]` ADR-0130 / row 3 of the QuickFIX/J interop plan. `tools/interop` grew a `tls`
  # feature of its own (`tools/interop/Cargo.toml`) forwarding to `fixbolt-engine/tls`, for the
  # two TLS arms `scripts/interop-qfj.sh` drives; it is not in `default`. Same forwarding shape,
  # same reason, as the two `fixbolt-engine` lines above: asked per crate, because a sibling in
  # the workspace answers for it at the workspace scope `cargo test --all --no-default-features`
  # runs at.
  "fixbolt-interop:rustls"
  "fixbolt-interop:ktls-core"
  # `[2026-09-13]` tools/w2w grew `--tls` (step 6a of the `tls` plan), and its
  # `tls` feature names `rustls` and `rcgen` as optional **normal** dependencies
  # of its own — not only through `fixbolt-engine/tls`. `rcgen` in particular is
  # a dev-dependency everywhere else in this workspace, so this is the first
  # place it could reach a featureless build. Asked separately for the reason
  # the two engine lines above are: a typo in one `dep:` entry is invisible
  # until asked.
  "fixbolt-w2w:rustls"
  "fixbolt-w2w:rcgen"
  # `[2026-09-19]` crates/sbe, phase 2 step C4. `impl codec::Encoding for
  # Sbe<S>` brings `fixbolt-codec` in behind the `encoding` feature. The
  # zero-dependency loop below allows any `fixbolt-*` crate, so it cannot see
  # this one leak into a featureless build; asking by name can.
  "fixbolt-sbe:fixbolt-codec"
  # `[2026-09-19]` crates/library, phase 2 step C7. `fixbolt` re-exports
  # `fixbolt-sbe` as `sbe` behind its own `sbe` feature — the same forwarding
  # shape as the `fixbolt:libc` line above, and the same reason: asked per
  # crate, because a sibling in the workspace would answer for it at scope.
  "fixbolt:fixbolt-sbe"
  # `[2026-09-24]` crates/metrics, phase 4 row 1 (ADR-0170 decision 9). It
  # needs only `fixbolt_engine::observe`, which no feature gates, so it takes
  # the engine with `default-features = false` and has no features of its own.
  # A user building `hft` without `standard` must get no `libc` through it —
  # and its dev-dependency on the engine WITH `standard` is exactly the sibling
  # that would switch the flag back on at workspace scope. Asked per crate.
  "fixbolt-metrics:libc"
  # `[2026-09-24]` crates/store-sqlite, phase 4 row 3. ADR-0182 decision 1:
  # the default `sqlite` feature gates the `mod`s and `rusqlite`, and
  # `rusqlite`'s `bundled` compiles SQLite's C through `libsqlite3-sys` — the
  # one thing a machine with nothing optional installed must never be asked
  # to do. Both asked by name: the second is what compiles the C.
  "fixbolt-store-sqlite:rusqlite"
  "fixbolt-store-sqlite:libsqlite3-sys"
  # `[2026-09-24]` the same plan, row 6: `tools/w2w` takes the store behind
  # its own off-by-default `sqlite` feature, for `--journal sqlite-async`. A
  # no-feature w2w must not compile SQLite's C.
  "fixbolt-w2w:rusqlite"
  # `[2026-09-26]` crates/dict, plan 2026-09-26-docs-for-embedders row 19
  # (ADR-0207 decision 1). The generator is a library behind the off-by-default
  # `codegen` feature, and `roxmltree` is its optional **normal** dependency —
  # while staying the unconditional **build**-dependency `build.rs` has always
  # had. So the answer with no features is "nothing to print" (in the graph,
  # through build edges only), never "did not match": the XML parser must not
  # reach the target build of a crate that asked for nothing.
  "fixbolt-dict:roxmltree"
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

# Crates whose normal dependency graph must hold NOTHING from outside this
# workspace, under any feature set. A `crate:dep` line above cannot say that:
# it names one dependency that must be absent, and a zero-dependency crate has
# no one dependency to name — the leak would be whichever crate someone adds.
#
# `[2026-09-19]` crates/sbe, phase 2 step C1. ADR-0081 decision 1: zero runtime
# dependencies, `no_std` from the first commit. Workspace crates (`fixbolt-*`)
# are allowed because step C4 makes `sbe` implement `fixbolt_codec::Encoding`;
# anything else is a dependency the ADR did not decide.
ZERO_DEP_CRATES=(
  "fixbolt-sbe"
)

for crate in "${ZERO_DEP_CRATES[@]}"; do
  for features in --no-default-features --all-features; do
    echo "== ${crate} ${features} must pull nothing from outside the workspace =="
    # `--prefix none` prints one package per line, `<name> v<version> [...]`;
    # `-e normal` leaves dev- and build-dependencies out, as the loop above does.
    if ! out="$(cargo tree -p "${crate}" "${features}" -e normal --prefix none 2>&1)"; then
      echo "FAIL: cargo tree failed:" >&2
      echo "${out}" >&2
      rc=1
      continue
    fi
    # The crate itself must be the first line — otherwise the tree is not the
    # one asked about, and an empty foreign list would be a false green.
    if ! head -1 <<<"${out}" | grep -qE "^${crate} v"; then
      echo "FAIL: could not tell. cargo said:" >&2
      echo "${out}" >&2
      rc=1
      continue
    fi
    foreign="$(grep -vE '^fixbolt(-[a-z0-9-]+)? v' <<<"${out}" || true)"
    if [[ -n "${foreign}" ]]; then
      echo "FAIL: ${crate} ${features} depends on crates outside this workspace:" >&2
      echo "${foreign}" >&2
      rc=1
    else
      echo "ok — $(wc -l <<<"${out}" | tr -d ' ') package(s), all in this workspace"
    fi
  done

  echo "== ${crate} --no-default-features must build and test =="
  if ! cargo test -p "${crate}" --no-default-features >/dev/null 2>&1; then
    echo "FAIL: ${crate} does not build or test with --no-default-features" >&2
    cargo test -p "${crate}" --no-default-features 2>&1 | tail -20 >&2
    rc=1
  else
    echo "ok — builds and tests"
  fi
done


# `[2026-09-24]` phase 4 row 5, ADR-0190 decision 6: the `io-uring` crate and
# the two it brings, `bitflags` and `cfg-if`. They must be absent from
# `fixbolt-engine`'s shipped graph **by default and with no features** — and
# **present with `--features io-uring`**, because a probe that only ever
# answers "absent" is not known to be able to answer anything else (the same
# rule as every reversal here). Asked by name, per crate, for the reason the
# loop above gives; `tools/w2w` forwards the feature, so it is asked too.
# No test run here: the crate's featureless build is already the loop's.
uring_tree() { # uring_tree <crate> <dep> <feature words...>
  local crate="$1" dep="$2"
  shift 2
  cargo tree -p "${crate}" "$@" -e normal -i "${dep}" 2>&1
}
for crate in fixbolt-engine fixbolt-w2w; do
  for dep in io-uring bitflags cfg-if; do
    for features in "" "--no-default-features"; do
      echo "== ${crate} ${features:-(default features)} must not pull ${dep} =="
      # shellcheck disable=SC2086 # $features is zero or one word.
      out="$(uring_tree "${crate}" "${dep}" ${features})"
      if grep -qE "^${dep} v" <<<"${out}"; then
        echo "FAIL: ${dep} is in ${crate}'s normal dependency graph without --features io-uring:" >&2
        echo "${out}" >&2
        rc=1
      elif grep -qE "did not match any packages|nothing to print" <<<"${out}"; then
        echo "ok — ${dep} is absent from what ships"
      else
        echo "FAIL: could not tell. cargo said:" >&2
        echo "${out}" >&2
        rc=1
      fi
    done
    echo "== ${crate} --features io-uring must pull ${dep} (the probe can answer yes) =="
    out="$(uring_tree "${crate}" "${dep}" --features io-uring)"
    if grep -qE "^${dep} v" <<<"${out}"; then
      echo "ok — ${dep} is there when asked for"
    else
      echo "FAIL: --features io-uring does not pull ${dep}, so the absence above proves nothing:" >&2
      echo "${out}" >&2
      rc=1
    fi
  done
done

exit "${rc}"
