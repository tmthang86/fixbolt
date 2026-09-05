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

if [ "${1:-}" = "--flags" ]; then
  printf '%s' "$BENCH_RUSTFLAGS"
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
bench_binaries() {
  local flags=$1
  RUSTFLAGS="$flags" cargo bench --workspace --no-run --message-format=json -q 2>/dev/null |
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
