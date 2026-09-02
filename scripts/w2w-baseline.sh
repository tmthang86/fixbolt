#!/usr/bin/env bash
# The wire-to-wire baseline: `tools/w2w`, N whole runs, on a DESIGN.md §9 box.
#
# Phase 1 exit criterion 6 asks for p50 / p99 / p99.9 "published from tools/w2w
# on Linux with the §9 settings stated", and CLAUDE.md §2 non-negotiable 10 asks
# for the benchmark that produced a number, not just the number. `tools/w2w` is
# the benchmark; this is the procedure, so that the figure in DESIGN.md §8 is
# reproducible by one command rather than by a paragraph describing what
# somebody typed.
#
# Why N runs and not one. `[measured 2026-08-31]` the same box gives
# 267.2-335.7 ns for one bench case run to run, which is why
# `benches/baselines.tsv` records a median over >= 20 runs and a margin off a
# fixed ladder. The same applies here and more so: this figure contains the
# kernel's TCP stack.
#
# Why the quiet row is re-read PER RUN. `[measured 2026-08-31]` LM Studio
# started a `llama-server` mid-sample and every run after it read 59-63% busy
# against check-machine.sh's 3% ceiling, ending a 20-run sample at run 8.
# Competing load moves this project's ring median 71%, against 0.8% for every
# §9 tuning row combined — it is the largest error term there is, and a model
# that loads mid-sample passes a check taken only at the start.
set -euo pipefail
cd "$(dirname "$0")/.."

RUNS=${RUNS:-20}
MESSAGES=${MESSAGES:-20000}
WARMUP=${WARMUP:-2000}
ENGINE_CORE=${ENGINE_CORE:-6}
CLIENT_CORE=${CLIENT_CORE:-7}
# Seconds between runs. `[measured 2026-08-31]` back-to-back runs leave the
# previous suite inside the next quiet check's own one-second window and it
# reads 25-36% busy, disqualifying itself. Eight was enough.
GAP=${GAP:-8}
# Arms, as `mode:path` pairs. Both modes because ADR-0013 says a change proven
# in one mode is proven in neither; both paths because an app figure is not an
# admin figure.
ARMS=${ARMS:-"hft:admin hft:app standard:admin standard:app"}
# PIN=0 runs with no pinning at all, and ALLOW_UNISOLATED=1 pins to a core
# isolcpus does not name. Both are how the A/B in
# `docs/reference/measured-costs.md` was taken; neither produces a §9 figure,
# and the per-run lines say `unpinned` so that a pasted output cannot be
# mistaken for one.
PIN=${PIN:-1}
ALLOW_UNISOLATED=${ALLOW_UNISOLATED:-0}
BIN=target/release/w2w
PINARGS=()
if [ "$PIN" = 1 ]; then
  PINARGS=(--engine-core "$ENGINE_CORE" --client-core "$CLIENT_CORE")
  [ "$ALLOW_UNISOLATED" = 1 ] && PINARGS+=(--allow-unisolated)
fi

# The machine block travels with the figures, read off the box rather than
# asserted — CLAUDE.md §2 non-negotiable 10. Its verdict is captured because
# `benches/baselines.tsv` records one per line for the same reason.
echo "=============================================================="
scripts/check-machine.sh || true
VERDICT=$(scripts/check-machine.sh 2>/dev/null | grep -E '^pass [0-9]+' || echo "unknown")
echo "=============================================================="
echo

if [ ! -x "$BIN" ]; then
  echo "no $BIN — build it first:"
  echo "  cargo build --release -p fixbolt-w2w --features affinity"
  exit 1
fi
# A build with no `affinity` feature cannot pin, and refuses the flag rather
# than ignoring it, so this asks the binary rather than asking cargo.
if [ "$PIN" = 1 ] && ! "$BIN" "${PINARGS[@]}" \
     --messages 10 --warmup 2 >/dev/null 2>&1; then
  echo "$BIN cannot pin to cpu$ENGINE_CORE / cpu$CLIENT_CORE. Rebuild with:"
  echo "  cargo build --release -p fixbolt-w2w --features affinity"
  echo "and check both are in isolcpus (/proc/cmdline)."
  exit 1
fi

# CPU busy over one second, as a whole-number percent, from /proc/stat. The same
# quantity check-machine.sh's quiet row reads, inline so that N runs do not pay
# for N full machine reports.
busy_pct() {
  read -r _ a b c idle rest < /proc/stat
  local t0=$((a+b+c+idle)) i0=$idle
  sleep 1
  read -r _ a b c idle rest < /proc/stat
  local t1=$((a+b+c+idle)) i1=$idle
  local dt=$((t1-t0)) di=$((i1-i0))
  [ "$dt" -le 0 ] && { echo 100; return; }
  echo $(( (100*(dt-di)) / dt ))
}

median() { sort -n | awk '{v[NR]=$1} END {print (NR%2) ? v[(NR+1)/2] : int((v[NR/2]+v[NR/2+1])/2)}'; }

echo "runs $RUNS   messages $MESSAGES   warmup $WARMUP   gap ${GAP}s"
if [ "$PIN" = 1 ]; then
  echo "engine cpu$ENGINE_CORE   client cpu$CLIENT_CORE   (allow-unisolated $ALLOW_UNISOLATED)"
else
  echo "UNPINNED — not a DESIGN.md §9 figure whatever check-machine.sh says above"
fi
echo

for arm in $ARMS; do
  mode=${arm%%:*}
  path=${arm##*:}
  p50s=(); p99s=(); p999s=(); mins=(); skipped=0
  for i in $(seq 1 "$RUNS"); do
    b=$(busy_pct)
    if [ "$b" -gt 3 ]; then
      printf '  %-8s %-5s run %2d  DISQUALIFIED, %s%% busy\n' "$mode" "$path" "$i" "$b"
      skipped=$((skipped+1))
      sleep "$GAP"
      continue
    fi
    out=$("$BIN" --mode "$mode" --path "$path" "${PINARGS[@]}" \
            --messages "$MESSAGES" --warmup "$WARMUP")
    # A run whose allocation count is not zero is not a figure about this
    # engine, and the binary already asserts it; this is the second reader,
    # because a `set -e` that never looked would be a green nobody read.
    echo "$out" | grep -qE '^ *allocs +0 ' || { echo "$out"; echo "allocs != 0"; exit 1; }
    g() { echo "$out" | awk -v k="$1" '$1==k {print $2}'; }
    mins+=("$(g min)"); p50s+=("$(g p50)"); p99s+=("$(g p99)"); p999s+=("$(g p99.9)")
    printf '  %-8s %-5s run %2d  %s%% busy   min %8s  p50 %8s  p99 %8s  p99.9 %8s\n' \
      "$mode" "$path" "$i" "$b" "$(g min)" "$(g p50)" "$(g p99)" "$(g p99.9)"
    sleep "$GAP"
  done

  q=${#p50s[@]}
  echo
  if [ "$q" -eq 0 ]; then
    echo "  == $mode / $path: NO QUALIFYING RUNS ($skipped disqualified) =="
    echo
    continue
  fi
  m50=$(printf '%s\n' "${p50s[@]}" | median)
  m99=$(printf '%s\n' "${p99s[@]}" | median)
  m999=$(printf '%s\n' "${p999s[@]}" | median)
  mmin=$(printf '%s\n' "${mins[@]}" | median)
  x50=$(printf '%s\n' "${p50s[@]}" | sort -n | tail -1)
  n50=$(printf '%s\n' "${p50s[@]}" | sort -n | head -1)
  echo "  == $mode / $path: median of $q qualifying runs ($skipped disqualified) =="
  echo "     min    $mmin ns"
  echo "     p50    $m50 ns      (across runs: $n50 .. $x50)"
  echo "     p99    $m99 ns"
  echo "     p99.9  $m999 ns"
  # The spread of the per-run p50, as baselines.tsv's margin column defines it:
  # a bound tighter than the dispersion of the measurement is a randomly red
  # gate (crates/engine/benches/dispatch.rs paid for that lesson).
  echo "     spread max/median $(awk -v a="$x50" -v b="$m50" 'BEGIN{printf "%.3f", a/b}')"
  echo "     machine $VERDICT"
  if [ "$PIN" = 1 ]; then
    echo "     pinned  engine cpu$ENGINE_CORE, client cpu$CLIENT_CORE"
  else
    echo "     pinned  NO — NOT a §9 figure"
  fi
  echo
done
