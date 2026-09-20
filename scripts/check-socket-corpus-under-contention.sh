#!/usr/bin/env bash
# The socket corpus, run many times at once — ADR-0087 decision 5.
#
# `crates/engine/tests/wire.rs` and `crates/engine/tests/wire_fixt.rs` replay the
# QuickFIX acceptance definitions through a real loopback socket. Run once on an
# idle machine they are green and say nothing about load; run ten at a time they
# were not. `[measured 2026-09-20]` 0 red in 40 sequential runs, 11 red in 50 as
# ten concurrent copies, and 8 red in 50 the same way on `main` at 64ea6c2 —
# docs/reference/the-same-commit-went-red-and-green-in-the-same-minute.md.
#
# So contention is a gate here rather than an anecdote: this script runs the two
# prebuilt test binaries `rounds × copies` times, `copies` at a time, and prints
# `N red in M` per binary. Any red is a non-zero exit.
#
# Usage: scripts/check-socket-corpus-under-contention.sh <rounds> <copies> [wire|wire_fixt|both]
#
# **What this cannot see:**
#   * It is a COUNT OF PASSES AND FAILURES, not a measurement. It does not
#     depend on the DESIGN.md §9 settings and must never be quoted as a latency
#     number (CLAUDE.md §2 non-negotiable 10 is about the second kind).
#   * `copies` is contention on THIS machine. Ten copies on a 16-thread desk and
#     ten on a two-vCPU runner are different pressures, and neither is a proof
#     about the other. A green run bounds nothing; a red run is a real defect.
#   * A test binary that runs no test exits 0. The per-run log is kept when a
#     run is red, and the harness's own `lifeline hit:` line is the other signal
#     — a run that settles on the lifeline rather than on a counted record is a
#     slow green, and slow greens are what this script exists to turn red later.
#   * Nothing here pins a CPU or isolates a core: another process on the machine
#     changes the pressure both ways.
set -uo pipefail

rounds=${1:-5}
copies=${2:-10}
which=${3:-both}

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root" || exit 1

case "$which" in
  wire | wire_fixt | both) ;;
  *)
    echo "usage: $0 <rounds> <copies> [wire|wire_fixt|both]" >&2
    exit 2
    ;;
esac

out=$(mktemp -d "${TMPDIR:-/tmp}/socket-contention.XXXXXX")
trap 'rm -rf "$out"' EXIT

echo "building the socket test binaries (--features fix50sp2)"
if ! cargo test -p fixbolt-engine --features fix50sp2 --test wire --test wire_fixt \
  --no-run --message-format=json >"$out/build.json" 2>"$out/build.err"; then
  cat "$out/build.err" >&2
  echo "::error::the socket test binaries did not build"
  exit 1
fi

# One executable per test target, named by the target cargo built it from.
bin_for() {
  jq -r --arg t "$1" \
    'select(.reason == "compiler-artifact")
     | select(.target.kind[]? == "test")
     | select(.target.name == $t)
     | .executable // empty' "$out/build.json" | tail -n 1
}

status=0
for target in wire wire_fixt; do
  if [ "$which" != both ] && [ "$which" != "$target" ]; then
    continue
  fi
  bin=$(bin_for "$target")
  if [ -z "$bin" ] || [ ! -x "$bin" ]; then
    echo "::error::no test binary for $target"
    exit 1
  fi

  total=0
  red=0
  for r in $(seq 1 "$rounds"); do
    pids=()
    for c in $(seq 1 "$copies"); do
      "$bin" --nocapture >"$out/$target-$r-$c.log" 2>&1 &
      pids+=($!)
    done
    for i in "${!pids[@]}"; do
      total=$((total + 1))
      if ! wait "${pids[$i]}"; then
        red=$((red + 1))
        echo "--- red: $target round $r copy $((i + 1)) ---"
        cat "$out/$target-$r-$((i + 1)).log"
      fi
    done
  done

  # The lifeline is the harness's own admission that it settled on a clock
  # rather than on a counted record (ADR-0087 decision 3). Reported beside the
  # score whether or not anything went red.
  hits=$(grep -h '^lifeline hit:' "$out/$target"-*.log 2>/dev/null | awk -F': ' '{s += $2} END {print s + 0}')
  echo "$target: $red red in $total, lifeline hit: $hits"
  if [ "$red" -ne 0 ]; then
    status=1
  fi
done

exit "$status"
