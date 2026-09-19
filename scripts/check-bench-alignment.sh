#!/usr/bin/env bash
# Prove that the bench binaries were actually built with function alignment
# pinned — ADR-0049, STATUS.md open item 41.
#
# WHY THIS EXISTS
#
# `[measured 2026-09-05]` `encode ExecutionReport (template)` read 239.1 ns when
# its baseline was recorded and 280.4 ns four days later, and **nothing in the
# encoder changed**. The jump is one commit, `4396d6d`, which touches no
# `crates/*/src/` file at all: it added ~150 lines to the bench harness, and
# `include!`ing them into the same binary moved the figure by 11.4%. Adding
# INERT functions that the encoder never calls walks the same case across
# 236.5-292.4 ns. The case measures, to a sixth of its own value, where the
# compiler happened to put it.
#
# ADR-0049 pins function alignment for bench builds so that the figure moves
# when the code under test moves and not when the harness gains a line. That
# decision is worth exactly as much as the evidence that the flag is still doing
# something -- `-C llvm-args=` hands a string to LLVM, and a future toolchain
# that renames or ignores `align-all-functions` would leave every bench figure
# quietly layout-bound again, with a green gate on top of it. A typo cannot
# survive (rustc refuses an unknown llvm-arg and the build dies), but a flag
# that is ACCEPTED AND IGNORED is silent, and that is the shape this repository
# has already been caught by three times.
#
# So the flag is read back off the binary rather than trusted:
#
#   `[measured 2026-09-05]` crates/codec/benches/serialize.rs
#     built WITHOUT the flag:  5 of 23 own-crate text symbols 64-byte aligned
#     built WITH the flag:    23 of 23
#
# Only this workspace's own symbols are counted. `RUSTFLAGS` does not rebuild
# the precompiled standard library, so a whole-binary count reads 137/629 vs
# 158/629 -- a real difference buried under std, and far too weak to assert on.
#
# Proven by reversal: run `scripts/check-bench-alignment.sh --reversal` and this
# script builds one target with the flag REMOVED and requires the check to go
# red. A gate for a build flag is the easiest kind to write as a no-op.
set -euo pipefail

cd "$(dirname "$0")/.."

# The single definition of the flag. `scripts/bench.sh` asks for it with
# `--flags` rather than repeating the string: two copies of a codegen flag is
# two builds that differ, and the artifacts would silently not be the ones the
# figures came from.
#
# 6 is log2(64) -- one cache line on x86_64 and on aarch64's 64-byte lines.
BENCH_RUSTFLAGS="-C llvm-args=-align-all-functions=6"
ALIGN=64

# The single definition of the bench FEATURE set, here for the same reason the
# flag above is: it is a property of how the bench binaries are built, so two
# copies of it are two different builds and "the figures would come from
# artifacts the check never saw" stops being a warning and becomes a
# description. `scripts/bench.sh` asks for it with `--features`, exactly as it
# asks for the flag with `--flags`, and keeps no list of its own.
#
# These gate bench CASES, not dependencies. `[measured 2026-09-19]` every FIXT
# 1.1 / FIX 5.0 SP2 case sits behind `#[cfg(feature = "fix50sp2")]`, and a bench
# run without the feature compiles them to nothing and stays green having
# measured none of them -- the trap in
# docs/reference/a-feature-gated-test-is-a-test-ci-never-runs.md, one layer
# down: there a test binary CI never ran, here a case that never existed in the
# binary CI did run.
#
# A feature NAME, never a package list: `fixbolt-codec`, `fixbolt-session` and
# `fixbolt-engine` each grew this feature in a different pull request. Which
# packages declare it is a per-package question and belongs to bench.sh, which
# asks cargo; what the set IS belongs here.
#
# Additive only -- no `--no-default-features` -- so each package keeps its own
# default set and gains these on top.
BENCH_FEATURES=(fix50sp2)

# The set as cargo wants it on a command line: comma separated, empty when there
# is none. `${a[*]:-}` rather than `${a[*]}` so an empty list is not an unbound
# variable under `set -u`, and no `mapfile` anywhere -- macOS ships bash 3.2.
bench_features_csv() {
  (
    IFS=,
    echo "${BENCH_FEATURES[*]:-}"
  )
}

# One line per workspace package: "<name><TAB><the features of BENCH_FEATURES it
# declares, comma separated>". Empty second field when it declares none.
#
# WHICH packages get the feature is asked of cargo, never listed: `fixbolt-codec`,
# `fixbolt-session` and `fixbolt-engine` each grew `fix50sp2` in a different pull
# request and a list would have missed whichever came last. Passing a feature a
# package does not declare is a hard cargo error, so the question has to be asked
# of every target either way.
#
# It lives HERE, next to the set, because `bench_binaries` below needs the same
# answer `scripts/bench.sh` acts on -- see that function's comment. bench.sh asks
# for this map once with `--features-map` and looks packages up in it.
bench_features_map() {
  local want_json
  want_json=$(printf '%s\n' "${BENCH_FEATURES[@]:-}" | jq -R . | jq -s 'map(select(. != ""))')
  cargo metadata --no-deps --format-version 1 |
    jq -r --argjson want "$want_json" '
      .packages[]
      | . as $pkg
      | "\($pkg.name)\t\([$want[] | . as $f | select($pkg.features | has($f))] | join(","))"
    ' | sort
}

if [ "${1:-}" = "--flags" ]; then
  printf '%s' "$BENCH_RUSTFLAGS"
  exit 0
fi

if [ "${1:-}" = "--features" ]; then
  printf '%s' "$(bench_features_csv)"
  exit 0
fi

if [ "${1:-}" = "--features-map" ]; then
  bench_features_map
  exit 0
fi

REVERSAL=0
[ "${1:-}" = "--reversal" ] && REVERSAL=1

if ! command -v nm >/dev/null 2>&1; then
  echo "FAIL: nm is not installed, so the flag cannot be read back off the" >&2
  echo "      binary. This check does not get to pass by being unable to run." >&2
  exit 1
fi

# Count how many of a binary's OWN text symbols sit on an $ALIGN boundary.
# Prints "<aligned> <total>".
aligned_in() {
  local bin=$1 tot=0 al=0 addr type name
  while read -r addr type name; do
    case "$type" in
    t | T) ;;
    *) continue ;;
    esac
    # This workspace's own code only -- see the header. The bench harness is
    # `#[path]`-included rather than a crate, so its symbols carry the bench
    # target's own name.
    case "$name" in
    *fixbolt* | *harness* | *verdict*) ;;
    *) continue ;;
    esac
    tot=$((tot + 1))
    [ $(((16#$addr) % ALIGN)) -eq 0 ] && al=$((al + 1))
  done < <(nm "$bin" 2>/dev/null)
  echo "$al $tot"
}

# The bench executables, from cargo rather than from a glob: a stale binary left
# in `deps/` by an earlier build is exactly what this check must not read.
#
# **ONE INVOCATION PER PACKAGE, with that package's features -- exactly as
# `scripts/bench.sh` builds them.** Not `--workspace`, which was what this did
# and which reads back binaries nothing measured. A cargo unit's identity
# includes its feature set AND whatever feature unification the invocation
# performs across the packages it selects, so `--workspace` is a different build
# from `-p <pkg>` even when the flags match:
#
#   `[measured 2026-09-19]` fixbolt-session --bench alloc
#     -p, --features fix50sp2   alloc-66cc3ea49f7b020a   <- what bench.sh RUNS
#     --workspace --features    alloc-f3c4b7c7d945a11e
#     --workspace, no features  alloc-e674ff459564cb4a
#
#   and under `--workspace` the mismatch is not confined to the featured
#   packages: featureless, `fixbolt --bench alloc` was already
#   alloc-1845cc4723f12ea9 per package against alloc-282bf43cebf5f93e in the
#   workspace build this function used to read.
#
# Certifying any of those but the first is ADR-0049's guard pointed at the wrong
# artifact, which is the failure mode this whole script exists to end.
bench_binaries() {
  local flags=$1 pkg feats
  local map
  map=$(bench_features_map)
  while IFS=$'\t' read -r pkg feats; do
    [ -n "$pkg" ] || continue
    if [ -n "${feats:-}" ]; then
      RUSTFLAGS="$flags" cargo bench -q -p "$pkg" --no-run --features "$feats" \
        --message-format=json 2>/dev/null
    else
      RUSTFLAGS="$flags" cargo bench -q -p "$pkg" --no-run \
        --message-format=json 2>/dev/null
    fi
  done <<<"$map" |
    jq -r 'select(.executable != null and (.target.kind[]? == "bench")) | .executable'
}

if [ "$REVERSAL" -eq 1 ]; then
  echo "=== reversal: the same check against a build with the flag removed"
  # One target is enough and keeps the reversal cheap. It must go RED.
  bin=$(RUSTFLAGS="" cargo bench -p fixbolt-codec --bench serialize --no-run \
    --message-format=json -q 2>/dev/null |
    jq -r 'select(.executable != null) | .executable' | head -1)
  read -r al tot < <(aligned_in "$bin")
  echo "unpinned  $al of $tot own-crate text symbols on a ${ALIGN}-byte boundary"
  if [ "$al" -eq "$tot" ]; then
    echo "FAIL: the reversal did not go red — every symbol is aligned even" >&2
    echo "      without the flag, so this check cannot tell the two apart" >&2
    echo "      and proves nothing about the pinned build." >&2
    exit 1
  fi
  echo "OK: the reversal is red, so the check can see the difference"
  exit 0
fi

echo "=== bench alignment (ADR-0049)"
echo "flag      $BENCH_RUSTFLAGS"
echo "features  $(bench_features_csv)"

bad=0
seen=0
while IFS= read -r bin; do
  [ -n "$bin" ] || continue
  seen=$((seen + 1))
  read -r al tot < <(aligned_in "$bin")
  name=$(basename "$bin")
  if [ "$tot" -eq 0 ]; then
    echo "FAIL   $name: no own-crate text symbols found — nm read nothing to check"
    bad=$((bad + 1))
  elif [ "$al" -eq "$tot" ]; then
    echo "PASS   $name: $al of $tot own-crate text symbols ${ALIGN}-byte aligned"
  else
    echo "FAIL   $name: $al of $tot own-crate text symbols ${ALIGN}-byte aligned"
    bad=$((bad + 1))
  fi
done < <(bench_binaries "$BENCH_RUSTFLAGS")

if [ "$seen" -eq 0 ]; then
  echo "FAIL: cargo reported no bench executables, so nothing was checked." >&2
  echo "      A check that examined nothing is not a check." >&2
  exit 1
fi

if [ "$bad" -ne 0 ]; then
  echo >&2
  echo "FAIL: $bad of $seen bench binaries are not built with alignment pinned." >&2
  echo "      Either RUSTFLAGS did not reach the build, or the toolchain no" >&2
  echo "      longer honours $BENCH_RUSTFLAGS. Until it does, every timing" >&2
  echo "      figure carries up to 16% of binary layout — ADR-0049, and" >&2
  echo "      docs/reference/a-benchmark-that-measures-where-the-compiler-put-it.md" >&2
  exit 1
fi

echo "OK: $seen bench binaries, alignment pinned and read back"
