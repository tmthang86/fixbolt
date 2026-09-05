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
#   TIMING     nanoseconds per operation, compared against THIS machine's own
#              recorded baseline in benches/baselines.tsv (ADR-0016). A CPU with
#              no line in that file gets NO BASELINE, which is counted on its own
#              row of the summary and is not a pass.
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

# ADR-0049: bench builds pin function alignment, and the flag is READ BACK off
# the binaries below rather than trusted.
#
# `[measured 2026-09-05]` without it, `encode ExecutionReport (template)` is a
# 16% measurement of where the compiler put the code. Its baseline was recorded
# at 239.1 ns and the same encoder read 280.4 four days later; the whole jump is
# one commit that touches no `crates/*/src/` file, and adding INERT functions to
# the bench binary walks the case across 236.5-292.4 ns. Under the flag the same
# perturbation holds inside 4.0%.
#
# The string lives in ONE place. Two copies of a codegen flag are two different
# builds, and the figures would then come from artifacts the check never saw.
RUSTFLAGS="$(scripts/check-bench-alignment.sh --flags)"
export RUSTFLAGS

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
#
# `while read` rather than `mapfile`: `[measured 2026-09-01]` mapfile is bash 4+
# and macOS ships bash 3.2, so this script had never run on a development
# laptop at all -- it died at this line with `mapfile: command not found`
# before measuring anything. Not a §9 machine, so its NUMBERS are still
# worthless here (check-machine.sh says so on its own line); its BEHAVIOUR --
# does every target run, does a case fall outside its band -- is machine
# independent and was unverifiable for no good reason.
TARGETS=()
while IFS= read -r line; do
  [ -n "$line" ] && TARGETS+=("$line")
done < <(
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
# Cases the harness could find no baseline for on THIS machine. Counted because
# "not red" must never read as "green": ADR-0016 made the comparison per-machine,
# so an unknown CPU has nothing to compare against and every one of its cases
# would otherwise print a figure and pass. See harness.rs.
no_baseline=0
no_baseline_targets=()
under_baseline=0
under_baseline_targets=()

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
  # The harness prints one `cases without a baseline: N ...` line per target.
  nb=$(echo "$out" | sed -n 's/^cases without a baseline: \([0-9]*\).*/\1/p' | head -1)
  if [ -n "${nb:-}" ] && [ "$nb" -gt 0 ]; then
    no_baseline=$((no_baseline + nb))
    no_baseline_targets+=("$pkg/$name")
  fi
  # And one `cases under their baseline: N ...` line when any case came in below
  # `baseline / margin`. STATUS.md open item 25: a ceiling alone passes a
  # benchmark that stopped measuring, forever, and more comfortably every day.
  ub=$(echo "$out" | sed -n 's/^cases under their baseline: \([0-9]*\).*/\1/p' | head -1)
  if [ -n "${ub:-}" ] && [ "$ub" -gt 0 ]; then
    under_baseline=$((under_baseline + ub))
    under_baseline_targets+=("$pkg/$name")
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

# The flag above is a promise until something reads it. A `-C llvm-args=` string
# is handed to LLVM verbatim: a typo dies at the build (rustc refuses an unknown
# llvm-arg), but a flag that is ACCEPTED AND IGNORED by a later toolchain is
# silent, and every figure printed above would quietly be layout-bound again.
#
# Run as a statement, not inside `$( ... | tail -1)`: a pipeline into `tail`
# returns tail's status, so a FAILING check would have printed its complaint and
# left this script green. That is the exact shape of the thing being guarded
# against, one line away from the guard.
echo "=== alignment read-back (ADR-0049)"
if ! scripts/check-bench-alignment.sh; then
  echo "FAIL: the bench binaries were not built with alignment pinned, so the" >&2
  echo "      figures above carry up to 16% of binary layout — ADR-0049." >&2
  exit 1
fi
echo

echo "=== summary"
echo "targets measuring    $ran of $EXPECTED"
echo "targets silent       ${#silent[@]}  ${silent[*]:-}"
echo "invariant failures   ${#invariant_failed[@]}  ${invariant_failed[*]:-}"
echo "timing over baseline ${#timing_over[@]}  ${timing_over[*]:-}"
echo "cases w/o a baseline $no_baseline  ${no_baseline_targets[*]:-}"
echo "cases under the band $under_baseline  ${under_baseline_targets[*]:-}"

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

# A machine with no recorded baseline is not a machine that passed. Without
# --strict this is reported rather than fatal, because CI runs on a shared pool
# whose CPUs deliberately have no baseline and a red CI on that is a red CI
# nobody reads. With --strict -- what a DESIGN.md §9 machine runs -- one is fatal:
# on the box whose numbers get published, an uncompared case is a hole.
if [ "$no_baseline" -ne 0 ]; then
  if [ "$STRICT" -eq 1 ]; then
    echo "FAIL: --strict, and $no_baseline case(s) had no baseline for this CPU" >&2
    echo "      Record them in benches/baselines.tsv; the harness printed the" >&2
    echo "      line to paste for each. CLAUDE.md non-negotiable 10." >&2
    exit 1
  fi
  echo "$no_baseline case(s) have no baseline on this CPU: they were measured and"
  echo "printed but compared against nothing. Not fatal without --strict."
fi

# A figure BELOW its band. Two causes, and both need the same thing from a
# person: a real optimisation, in which case the baseline must be re-recorded or
# the ceiling above it guards nothing; or a benchmark that stopped measuring.
# Never fatal without --strict, because a genuine speed-up must not break the
# build before it can be merged -- what people would learn from that is to widen
# the margin. With --strict, on a DESIGN.md §9 box during a deliberate
# measurement session, it must be resolved.
if [ "$under_baseline" -ne 0 ]; then
  if [ "$STRICT" -eq 1 ]; then
    echo "FAIL: --strict, and $under_baseline case(s) came in UNDER their baseline" >&2
    echo "      Either re-record the baseline (a real speed-up leaves the ceiling" >&2
    echo "      wider than the truth) or fix the benchmark: STATUS.md item 25," >&2
    echo "      where a case published 1.3 ns while doing 8.5 ns of work." >&2
    exit 1
  fi
  echo "$under_baseline case(s) came in under their baseline: either re-record it,"
  echo "or the benchmark stopped measuring. Not fatal without --strict."
fi

if [ "${#timing_over[@]}" -ne 0 ]; then
  if [ "$STRICT" -eq 1 ]; then
    echo "FAIL: --strict, and a timing baseline was exceeded" >&2
    exit 1
  fi
  echo "Timing baselines were exceeded. Not fatal without --strict: see the header."
fi

echo "OK"
