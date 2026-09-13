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
#
# `[2026-09-13]` step 6b of docs/plans/2026-09-04-tls.md, Sửa 6: an `ARMS`
# entry may now name a TLS arm too — `mode:path:tls`, the third field optional
# and defaulting to `off`, so `ARMS="hft:admin"` (the only shape this script
# knew before today) still means exactly what it meant before. `off` and
# `ktls` keep the `allocs 0` assertion; `userspace` leaves ADR-0005 decision
# 3's hot-path guarantee, so that one grep is skipped for it — the allocation
# count is still printed in the per-run line, just not asserted zero.
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
  rest=${arm#*:}
  path=${rest%%:*}
  # Third field optional. `rest` still has a ':' in it only when a tls field
  # was actually given — `arm##*:` alone (the old `path` line) would instead
  # have picked off the THIRD field as `path` the moment one arm grew a tls
  # name, which is why this is two strips rather than one.
  if [ "$rest" != "$path" ]; then
    tls=${rest#*:}
  else
    tls=off
  fi
  tls_args=()
  [ "$tls" != "off" ] && tls_args=(--tls "$tls")
  # What the engine must REPORT for this arm, not what the flag is spelled —
  # `--tls ktls` reads back as `tls: kernel` (tools/w2w/src/main.rs
  # `seen_name`), and a handover that quietly fell back to userspace must
  # disqualify the run rather than publish its p50 under the `ktls` label it
  # never earned.
  case "$tls" in
    ktls) want_tls=kernel ;;
    *) want_tls=$tls ;;
  esac
  p50s=(); p99s=(); p999s=(); mins=(); skipped=0
  for i in $(seq 1 "$RUNS"); do
    b=$(busy_pct)
    if [ "$b" -gt 3 ]; then
      printf '  %-8s %-5s %-9s run %2d  DISQUALIFIED, %s%% busy\n' "$mode" "$path" "$tls" "$i" "$b"
      skipped=$((skipped+1))
      sleep "$GAP"
      continue
    fi
    # **The binary is allowed to fail, and its output is still read.**
    # `[measured 2026-09-13]` under `set -e` a non-zero exit inside this command
    # substitution ended the whole script right here, with nothing printed and
    # no FAIL line — so every identity check below was unreachable for the one
    # case each was written for. `|| rc=$?` keeps the output and the status;
    # the checks then run in the order that names the cause best, and the exit
    # status itself is judged last, after they have had their say. `2>&1` so a
    # panic or a refusal from the binary travels with them.
    rc=0
    out=$("$BIN" --mode "$mode" --path "$path" "${tls_args[@]}" "${PINARGS[@]}" \
            --messages "$MESSAGES" --warmup "$WARMUP" 2>&1) || rc=$?
    # WHAT RAN is read back and checked before anything the run measured is
    # trusted — the same order `check-no-kernel-sleep.sh` learned the hard way
    # after `--mode standard` once printed its banner and ran nothing. A typo
    # in ARMS, or a kTLS handover that quietly fell back, must not quietly
    # produce a column of figures for the wrong arm.
    echo "$out" | grep -qx "mode: $mode" || { echo "$out"; echo "ran a mode other than '$mode'"; exit 1; }
    echo "$out" | grep -qx "path: $path" || { echo "$out"; echo "ran a path other than '$path'"; exit 1; }
    # The transport, checked before the allocation count below and before the
    # exit status: `ktls` expects zero allocations, so a quiet fallback to
    # userspace would otherwise be caught by the allocs assertion instead of by
    # this one, naming the wrong defect — `allocs != 0` reads as a hot-path
    # regression, not as a transport that never took the keys.
    #
    # **Being written above the allocs check was not enough to make it run.**
    # `[measured 2026-09-13]` a senior review forced the engine's handover off
    # and this script exited 101 without printing the line below, because the
    # binary's own `assert_eq!(allocs, 0)` fired first and `set -e` ended the
    # script inside the command substitution above. Two things changed: `w2w`
    # now refuses a read-back that does not match its `--tls` flag before it
    # takes a sample, and the substitution above keeps the status instead of
    # dying on it. This line is the second reader, and it now reads.
    echo "$out" | grep -qx "tls: $want_tls" || {
      ran_tls=$(echo "$out" | awk '/^tls: /{print $2; exit}')
      echo "$out"
      echo "FAIL: $mode:$path:$tls ran tls '${ran_tls:-<none>}' when '$want_tls' was required"
      exit 1
    }
    # The status, after the three identity checks and before the figures: a run
    # that ended any other way than by printing them is not a measurement, and
    # the checks above get to name the cause first when they can.
    if [ "$rc" -ne 0 ]; then
      echo "$out"
      echo "FAIL: $mode:$path:$tls — $BIN exited $rc"
      exit 1
    fi
    # A run whose allocation count is not zero is not a figure about this
    # engine, and the binary already asserts it; this is the second reader,
    # because a `set -e` that never looked would be a green nobody read.
    # Skipped for `userspace`: ADR-0005 decision 3 leaves the zero-allocation
    # guarantee there on purpose, and the binary itself only prints the count
    # for that arm rather than asserting it — see tools/w2w/src/main.rs.
    if [ "$tls" != userspace ]; then
      echo "$out" | grep -qE '^ *allocs +0 ' || { echo "$out"; echo "allocs != 0"; exit 1; }
    fi
    g() { echo "$out" | awk -v k="$1" '$1==k {print $2}'; }
    mins+=("$(g min)"); p50s+=("$(g p50)"); p99s+=("$(g p99)"); p999s+=("$(g p99.9)")
    printf '  %-8s %-5s %-9s run %2d  %s%% busy   min %8s  p50 %8s  p99 %8s  p99.9 %8s\n' \
      "$mode" "$path" "$tls" "$i" "$b" "$(g min)" "$(g p50)" "$(g p99)" "$(g p99.9)"
    sleep "$GAP"
  done

  q=${#p50s[@]}
  echo
  if [ "$q" -eq 0 ]; then
    echo "  == $mode / $path / $tls: NO QUALIFYING RUNS ($skipped disqualified) =="
    echo
    continue
  fi
  m50=$(printf '%s\n' "${p50s[@]}" | median)
  m99=$(printf '%s\n' "${p99s[@]}" | median)
  m999=$(printf '%s\n' "${p999s[@]}" | median)
  mmin=$(printf '%s\n' "${mins[@]}" | median)
  x50=$(printf '%s\n' "${p50s[@]}" | sort -n | tail -1)
  n50=$(printf '%s\n' "${p50s[@]}" | sort -n | head -1)
  echo "  == $mode / $path / $tls: median of $q qualifying runs ($skipped disqualified) =="
  echo "     min    $mmin ns"
  echo "     p50    $m50 ns      (across runs: $n50 .. $x50)"
  echo "     p99    $m99 ns"
  echo "     p99.9  $m999 ns"
  # The spread of the per-run p50, as baselines.tsv's margin column defines it:
  # a bound tighter than the dispersion of the measurement is a randomly red
  # gate (crates/engine/benches/dispatch.rs paid for that lesson).
  echo "     spread max/median $(awk -v a="$x50" -v b="$m50" 'BEGIN{printf "%.3f", a/b}')"
  echo "     machine $VERDICT"
  if [ "$tls" = userspace ]; then
    echo "     allocs  NOT asserted zero — userspace leaves ADR-0005 decision 3's guarantee"
  fi
  if [ "$PIN" = 1 ]; then
    echo "     pinned  engine cpu$ENGINE_CORE, client cpu$CLIENT_CORE"
  else
    echo "     pinned  NO — NOT a §9 figure"
  fi
  echo
done
