#!/usr/bin/env bash
# Run EVERY benchmark, print every figure, and never let one target's failure
# hide another's numbers.
#
# STATUS.md open item 20. `cargo test --all` does not run a `harness = false`
# bench target and no CI job ran `cargo bench`, so all nine assertions had never
# executed anywhere — including `benches/alloc.rs`, which CLAUDE.md §2 names as
# the machine check for non-negotiable 1.
#
# Two kinds of benchmark live here and they must not share an exit code:
#
#   INVARIANT  counts allocations, or counts messages. The answer is the same on
#              every machine, so a failure is a real defect and this script
#              exits non-zero.
#   TIMING     nanoseconds per operation. The ceilings are tuned to an Apple M5.
#              `[measured 2026-08-30]` on a shared 4 vCPU Linux container the
#              same case swings 5-232% run to run and three of twelve flip
#              colour between runs, so a timing failure HERE proves nothing and
#              is reported rather than fatal. --strict makes it fatal, which is
#              what a DESIGN.md §9 machine should use.
#
# CLAUDE.md §2 non-negotiable 10: no number without the benchmark that produced
# it, the machine, and the §9 settings. The machine block below travels with the
# figures for that reason — copy the whole output, not the table.
set -euo pipefail

cd "$(dirname "$0")/.."

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

# Which targets are invariant rather than timing. An unlisted bench is a hard
# error rather than a default, because either default is wrong: silently
# advisory hides a real guard, silently blocking makes CI flap.
is_invariant() {
  case "$1" in
    alloc | ring_full) return 0 ;;
    *) return 1 ;;
  esac
}

# The machine block is scripts/check-machine.sh, so every figure below travels
# with the DESIGN.md §9 settings that were actually READ off the box rather than
# asserted — CLAUDE.md §2 non-negotiable 10. Its exit code is deliberately
# ignored here: an untuned machine still produces usable allocation counts and
# usable A/B comparisons. --strict is where it becomes fatal.
scripts/check-machine.sh || MACHINE_NOT_TUNED=1
: "${MACHINE_NOT_TUNED:=0}"
echo "profile   bench (release)"
echo

# The set of targets comes from cargo, not from a list in this file: a bench
# renamed or added must not quietly stop being run. Compared against what
# actually ran, at the end.
mapfile -t TARGETS < <(
  cargo metadata --no-deps --format-version 1 |
    jq -r '.packages[] | .name as $p | .targets[] | select(.kind[]=="bench") | "\($p) \(.name)"' |
    sort
)
EXPECTED=${#TARGETS[@]}
if [ "$EXPECTED" -eq 0 ]; then
  echo "no bench targets found — cargo metadata returned none, which cannot be right" >&2
  exit 1
fi

ran=0
silent=()
invariant_failed=()
timing_over=()

for entry in "${TARGETS[@]}"; do
  pkg=${entry% *}
  name=${entry#* }
  if is_invariant "$name"; then kind=INVARIANT; else kind=TIMING; fi
  echo "=== $kind  $pkg --bench $name"
  set +e
  out=$(cargo bench -q -p "$pkg" --bench "$name" 2>&1)
  code=$?
  set -e
  echo "$out"
  # Liveness, per target. A target that printed no measurement measured
  # nothing, whatever its exit code — `cargo bench --bench harness` did exactly
  # that until autobenches was turned off: "0 measured", exit 0, green.
  if [ "$kind" = TIMING ]; then
    echo "$out" | grep -q "ns/op" && ran=$((ran + 1)) || silent+=("$pkg/$name")
  else
    [ -n "${out//[[:space:]]/}" ] && ran=$((ran + 1)) || silent+=("$pkg/$name")
  fi
  if [ "$code" -ne 0 ]; then
    if [ "$kind" = INVARIANT ]; then
      invariant_failed+=("$pkg/$name")
    else
      timing_over+=("$pkg/$name")
    fi
  fi
  echo
done

echo "=== summary"
echo "targets measuring    $ran of $EXPECTED"
echo "targets silent       ${#silent[@]}  ${silent[*]:-}"
echo "invariant failures   ${#invariant_failed[@]}  ${invariant_failed[*]:-}"
echo "timing over ceiling  ${#timing_over[@]}  ${timing_over[*]:-}"

# Liveness. A green result from a run that executed nothing is the failure this
# whole script exists to end, so it is checked rather than assumed.
if [ "$ran" -ne "$EXPECTED" ]; then
  echo "FAIL: $((EXPECTED - ran)) target(s) produced no measurement: ${silent[*]:-}" >&2
  echo "      A bench that measures nothing must not be reported as green." >&2
  exit 1
fi

if [ "${#invariant_failed[@]}" -ne 0 ]; then
  echo "FAIL: a machine-independent benchmark failed; that is a real defect" >&2
  exit 1
fi

# --strict is for a DESIGN.md §9 machine, and on such a machine the settings are
# part of the result. Publishing a latency figure from an untuned box is exactly
# what non-negotiable 10 forbids, so --strict refuses before it even looks at the
# ceilings.
if [ "$STRICT" -eq 1 ] && [ "$MACHINE_NOT_TUNED" -ne 0 ]; then
  echo "FAIL: --strict, and this machine is not set up to DESIGN.md §9" >&2
  echo "      See the failing rows above. Without --strict the counts and the" >&2
  echo "      A/B comparisons are still usable; the latency figures are not." >&2
  exit 1
fi

if [ "${#timing_over[@]}" -ne 0 ]; then
  if [ "$STRICT" -eq 1 ]; then
    echo "FAIL: --strict, and a timing ceiling was exceeded" >&2
    exit 1
  fi
  echo "Timing ceilings were exceeded. Not fatal without --strict: see the header."
fi

echo "OK"
