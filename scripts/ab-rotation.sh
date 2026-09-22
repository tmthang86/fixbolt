#!/usr/bin/env bash
# The boot D rotation driver — one interleaved round-robin over pre-built
# arms, one shared CONTROL, never a compiler in the loop. ADR-0090, the
# plan's P1 row and *Một vòng xoay, một đối chứng chung, ba A/B — bước D4*.
#
# WHY INTERLEAVED, NOT SEQUENTIAL
#
# The desk drifts: open item 85 measured the SAME procedure moving 15.9%
# between two identical runs an hour apart. Three A/Bs run one after another
# would put the third's numbers on a different hour of the machine's day
# than its control, which is not a comparison. Round-robin against one
# shared CONTROL, order reversed on alternate rounds, means every arm gets
# the same slice of the day at any cut — ADR-0090 decision 3.
#
# WHY IT REFUSES TO BUILD
#
# A `cargo build` mid-boot is ~10 minutes of load and heat between two runs
# — the very thing being measured. Every arm here is a git worktree built
# BEFORE the reboot, with `bench.sh`'s own RUSTFLAGS (ADR-0049) and that
# tree's own feature map. So this driver treats compiling as a bug, not a
# fallback:
#
#   * A suite seen for the first time in this EVIDENCE directory is resolved
#     with `cargo bench --no-run --message-format=json`, and cargo's own
#     `fresh` field on every compiler-artifact message is READ BACK.
#     `[measured 2026-09-20]` a binary already built with the exact
#     RUSTFLAGS/features prints `"fresh":true` and does no compile work at
#     all; changing so much as RUSTFLAGS makes cargo recompile and report
#     `"fresh":false` on the artifact it just built. Any `false` here is
#     refused — the arm was not pre-built as ADR-0090 decision 2 requires,
#     and the run stops rather than accepting the binary it was just forced
#     to make.
#   * Once resolved, the path and its sha256 are written to
#     `$EVIDENCE/manifest.txt` and never asked of cargo again in this
#     EVIDENCE directory. A second invocation (resuming after a kill) reads
#     the manifest, stats the file and re-hashes it — no cargo call, so no
#     way to recompile even by accident — and refuses if the file is gone or
#     its sha256 has moved, rather than trusting a target that changed under
#     it.
#   * Every ROUND then runs the resolved binary DIRECTLY (no `cargo bench`
#     wrapper). `[measured 2026-09-20]`
#     target/release/deps/serialize-<hash> run standalone prints the exact
#     `ns/op` lines `cargo bench` prints for it — harness = false everywhere
#     in this workspace (crates/codec/Cargo.toml and friends), so the
#     compiled binary IS the benchmark; cargo's own role ends at resolution.
#
# WHY A WHOLE ROUND IS DROPPED, NOT ONE ARM
#
# Plan trap table: "Vòng có một arm bị loại → n lệch giữa arm — driver đánh
# dấu cả vòng incomplete; summary bỏ vòng đó cho mọi arm" and ADR-0090
# decision 3, "a round with any disqualified arm dropped for all arms so n
# stays equal". A round where `w0` was disqualified but `wa` ran fine would,
# if `wa`'s data were kept, hand `wa` one more sample than its own control —
# the comparison ADR-0090 promises ("every comparison has the same n") no
# longer holds. So every arm still ATTEMPTS its own suites each round (its
# own quiet row decides whether it runs), every run is still kept in
# `runs.txt` — CLAUDE.md §2 non-negotiable 10, nothing is averaged away — but
# `timeline.txt` gets one trailing `round N complete|incomplete` line, and
# `--summary` counts only rows from `complete` rounds.
#
# EVIDENCE, per run (never averaged away):
#   manifest.txt   arm  suite  path  worktree-sha  features  binary  sha256
#   timeline.txt   round N arm X busy B% ok|DISQUALIFIED   (one per arm/round)
#                  round N complete|incomplete              (one per round)
#   runs.txt       arm  round  case  ns                     (one per case)
#   raw/N-arm-suite.txt   the suite's raw stdout for that round, kept whole
#
# INTERRUPTIBLE: every line above is appended as it is produced, not
# buffered — killing the driver between (or mid-) rounds leaves every round
# that finished fully readable. Resuming points EVIDENCE at the same
# directory; the manifest recognises what it already resolved and re-numbers
# nothing on its own — a fresh campaign wants a fresh EVIDENCE directory.
#
# `/tmp` IS REFUSED. Plan trap table: "`/tmp` tmpfs mất bằng chứng — mọi thứ
# vào target/boot-d-evidence/, driver từ chối EVIDENCE dưới /tmp" — this
# machine's /tmp is tmpfs (MEMORY.md "Scratchpad is tmpfs"); evidence
# written there does not survive a shutdown, and a boot this long ends in
# one.
#
# USAGE
#   ROUNDS=20 ARMS="w0=../fb-boot-d/w0:fixbolt-codec/parse,fixbolt-codec/serialize \
#                   wa=../fb-boot-d/wa:fixbolt-codec/parse,fixbolt-codec/serialize" \
#     CONTROL=w0 EVIDENCE=target/boot-d-evidence scripts/ab-rotation.sh
#
#   Each ARMS entry: name=path:suite[,suite...], suite = <pkg>/<bench> where
#   <pkg> is the real cargo package name (`fixbolt-codec`, not `codec` —
#   what `cargo bench -p` itself takes, and what check-bench-alignment.sh's
#   --features-map keys its rows on),
#   optionally suffixed @nofeat to force that ONE suite to run with no
#   features even when the tree's own --features-map would add some (the
#   plan's `w2`, run without features on a tree that also has fix50sp2).
#
#   --dry-run           print the resolved rotation and every command that
#                        would run, in order; touches no cargo, no file, no
#                        binary. Never needs pre-built binaries.
#   --summary <runs.txt> print the pure summary table (median, min/median,
#                        max/median, n, diff% vs CONTROL) for an existing
#                        runs.txt. Reads `$(dirname runs.txt)/timeline.txt`
#                        unless TIMELINE is set. Needs CONTROL.
#
# GAP (default 8s, plan trap table: "Hai cargo bench sát nhau → run sau tự
# loại 25-36% busy — driver GAP 8s"): a pause after an arm's suites finish,
# before the NEXT arm's quiet row is read, so one arm's CPU load does not
# bleed into the next arm's "is the machine quiet" reading.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

# ---------------------------------------------------------------------------
# Pure functions — sourced directly by scripts/check-ab-rotation.sh with
# AB_ROTATION_SOURCE_ONLY=1, the same shape as compare-w2w-procedures.sh /
# check-w2w-compare.sh. No file, no cargo, no global state but the two
# arguments they are given.
# ---------------------------------------------------------------------------

# Median of stdin, one number per line. Float, unlike w2w-baseline.sh's
# median() (which intentionally truncates whole-nanosecond wire figures) —
# a bench case's ns/op is already fractional and truncating it here would
# be a second rounding nobody asked for.
ab_median() {
  sort -n | awk '{v[NR]=$1} END {print (NR % 2) ? v[(NR+1)/2] : (v[NR/2]+v[NR/2+1])/2}'
}

# Round numbers marked complete in a timeline.txt — the trailing
# `round N complete` lines this driver writes once every arm in that round
# has been attempted. A round the driver never finished writing (killed
# mid-round) has neither a `complete` nor an `incomplete` trailer and is
# silently excluded here exactly as an `incomplete` one would be — visible
# in runs.txt as raw data, absent from any n.
ab_complete_rounds() { # ab_complete_rounds <timeline.txt>
  awk '$1 == "round" && $3 == "complete" { print $2 }' "$1" 2>/dev/null
}

# The summary table: for every (arm, case) pair that has at least one row in
# a COMPLETE round, its median / min-over-median / max-over-median / n, the %
# difference of its median from CONTROL's median on the same case (blank when
# CONTROL never ran that case), and, ADR-0092 decision 3, an `over` column —
# `k/n`, rows carrying verdict "over" among that pair's `n` — or `?` when any
# row of the pair has no fifth (verdict) column at all, i.e. `runs.txt` was
# written before this ADR. A trailing footer line names every (arm, case)
# pair with at least one `over` row, or says `none`, so the finding cannot be
# missed and can be grepped. Pure — two file paths and a name in, a table out.
ab_summary() { # ab_summary <runs.txt> <timeline.txt> <control-arm>
  local runs=$1 timeline=$2 control=$3
  local goodfile
  goodfile=$(mktemp)
  ab_complete_rounds "$timeline" >"$goodfile"

  local filtered
  filtered=$(awk -F'\t' -v gf="$goodfile" '
    BEGIN { while ((getline l < gf) > 0) ok[l] = 1 }
    NF >= 4 && ($2 in ok) { print }
  ' "$runs" 2>/dev/null)
  rm -f "$goodfile"

  printf '%-10s %-46s %10s %11s %11s %5s %10s %8s\n' \
    "arm" "case" "median" "min/med" "max/med" "n" "diff%" "over"

  if [ -z "$filtered" ]; then
    echo "(no complete rounds yet)"
    return 0
  fi

  local pairs
  pairs=$(printf '%s\n' "$filtered" | awk -F'\t' '!seen[$1 SUBSEP $3]++ { print $1 "\t" $3 }')

  local over_pairs=0
  while IFS=$'\t' read -r arm kase; do
    [ -n "$arm" ] || continue
    local vals n med min max minmed maxmed diff cvals cmed verdicts missing overcol k
    vals=$(printf '%s\n' "$filtered" | awk -F'\t' -v a="$arm" -v c="$kase" '$1 == a && $3 == c { print $4 }')
    n=$(printf '%s\n' "$vals" | grep -c .)
    med=$(printf '%s\n' "$vals" | ab_median)
    min=$(printf '%s\n' "$vals" | sort -n | head -1)
    max=$(printf '%s\n' "$vals" | sort -n | tail -1)
    minmed=$(awk -v a="$min" -v b="$med" 'BEGIN { printf "%.3f", a / b }')
    maxmed=$(awk -v a="$max" -v b="$med" 'BEGIN { printf "%.3f", a / b }')
    diff="—"
    if [ "$arm" != "$control" ]; then
      cvals=$(printf '%s\n' "$filtered" | awk -F'\t' -v a="$control" -v c="$kase" '$1 == a && $3 == c { print $4 }')
      if [ -n "$cvals" ]; then
        cmed=$(printf '%s\n' "$cvals" | ab_median)
        diff="$(awk -v c="$cmed" -v m="$med" 'BEGIN { printf "%+.1f", ((m - c) / c) * 100 }')%"
      fi
    fi

    verdicts=$(printf '%s\n' "$filtered" | awk -F'\t' -v a="$arm" -v c="$kase" '$1 == a && $3 == c { print (NF >= 5 ? $5 : "NA") }')
    missing=$(printf '%s\n' "$verdicts" | grep -c '^NA$')
    if [ "$missing" -gt 0 ]; then
      overcol="?"
    else
      k=$(printf '%s\n' "$verdicts" | grep -c '^over$')
      overcol="$k/$n"
      [ "$k" -gt 0 ] && over_pairs=$((over_pairs + 1))
    fi

    printf '%-10s %-46s %10.1f %11s %11s %5s %10s %8s\n' "$arm" "$kase" "$med" "$minmed" "$maxmed" "$n" "$diff" "$overcol"
  done <<<"$pairs"

  if [ "$over_pairs" -gt 0 ]; then
    printf 'over baseline: %s (arm, case) pairs\n' "$over_pairs"
  else
    echo "over baseline: none"
  fi
}

# Extract measurement rows from one suite's raw stdout — ADR-0092 decision 1.
# A row is exactly (harness.rs:305-360, :361):
#   <name><spaces><figure> ns/op   baseline <b> x<m> = [<floor>, <ceiling>]<mark>
#   <name><spaces><figure> ns/op   NO BASELINE for '<cpu>'
# where <mark> is empty, "  OVER BASELINE" or "  UNDER BASELINE". The anchor
# is the THREE spaces right after ` ns/op` and the word that follows them
# (`baseline` or `NO`) — every other line containing ` ns/op` (the pushed
# over/under report strings at harness.rs:344-358, the joined "cases under
# their baseline:" line, and the panic body at harness.rs:406-411) has a
# single space or other text there and fails the anchor, on purpose:
# ADR-0092 Context item 2 measured the old `/ ns\/op/` match turning those
# lines into phantom rows. `harness.rs` is not touched (ADR-0092 decision 1:
# an arm is a binary built before this fix exists) — the row shape is pinned
# by a fixture captured verbatim from a real bench binary, not typed here.
# Pure: stdin in, TSV out (`arm round case ns verdict`), no file, no cargo,
# no clock.
ab_extract() { # ab_extract <arm> <round>  (stdin: one suite's raw stdout)
  local arm=$1 round=$2
  awk -v arm="$arm" -v rnd="$round" '
    {
      idx = index($0, " ns/op")
      if (idx == 0) next
      after = substr($0, idx + 6)
      if (substr(after, 1, 3) != "   ") next
      tail = substr(after, 4)
      if (tail !~ /^(baseline |NO BASELINE for )/) next

      line = substr($0, 1, idx - 1)
      n = split(line, a, " ")
      cname = ""
      for (i = 1; i < n; i++) cname = (cname == "" ? a[i] : cname " " a[i])
      ns = a[n]

      verdict = "in"
      if (tail ~ /  OVER BASELINE$/) verdict = "over"
      else if (tail ~ /  UNDER BASELINE$/) verdict = "under"
      else if (tail ~ /^NO BASELINE for /) verdict = "none"

      printf "%s\t%s\t%s\t%s\t%s\n", arm, rnd, cname, ns, verdict
    }
  '
}

# The per-suite state one run_suite call ends in — ADR-0092 decision 2. Pure:
# four numbers/strings in, one word out. `panic_m` is the harness's own <m>
# from "<k> of <m> case(s) over the machine baseline:" (empty string when
# that line was not printed at all).
#   ok      exit 0 and at least one row was extracted.
#   OVER    exit != 0, the harness's verdict line was present, and its <m>
#           equals the rows actually extracted (every case was printed
#           before the assert fired, harness.rs:56-63) — the round STAYS
#           complete; these figures are real measurements (ADR-0092 fact 5).
#   FAILED  anything else: a non-zero exit with no verdict line (a crash, not
#           a panic on an over-band case), a verdict line whose <m> does not
#           match rows (the assert fired before every case was printed), or
#           zero rows whatever the exit (bench.sh's own liveness rule: a
#           binary that printed no row measured nothing — also catches a
#           harness row-shape drift, since ab_extract would then read 0).
#           A FAILED suite drops the whole round, same as a busy arm.
ab_suite_verdict() { # ab_suite_verdict <exit> <rows> <over> <panic_m>
  local exit_code=$1 rows=$2 over=$3 panic_m=$4
  if [ "$exit_code" = 0 ] && [ "$rows" -ge 1 ]; then
    echo ok
  elif [ "$exit_code" != 0 ] && [ -n "$panic_m" ] && [ "$panic_m" = "$rows" ] && [ "$over" -ge 1 ]; then
    echo OVER
  else
    echo FAILED
  fi
}

if [ "${AB_ROTATION_SOURCE_ONLY:-0}" = 1 ]; then
  # shellcheck disable=SC2317 # reachable when sourced
  return 0 2>/dev/null || exit 0
fi

# ---------------------------------------------------------------------------
# Everything past here touches files, cargo or a clock. Not sourced by the
# pure-function test.
# ---------------------------------------------------------------------------

usage() {
  echo "usage: ROUNDS=n ARMS=\"name=path:suite[,suite@nofeat]… …\" CONTROL=name EVIDENCE=dir $(basename "$0") [--dry-run]" >&2
  echo "       CONTROL=name $(basename "$0") --summary <runs.txt>" >&2
  echo "       $(basename "$0") --reextract <evidence-dir>" >&2
  exit 2
}

if [ "${1:-}" = "--summary" ]; then
  [ $# -ge 2 ] || usage
  RUNS=$2
  [ -f "$RUNS" ] || {
    echo "no such runs file: $RUNS" >&2
    exit 1
  }
  : "${CONTROL:?CONTROL must name the control arm}"
  TIMELINE=${TIMELINE:-"$(dirname "$RUNS")/timeline.txt"}
  ab_summary "$RUNS" "$TIMELINE" "$CONTROL"
  exit 0
fi

# ADR-0092 decision 3: rebuild <dir>/runs.reextracted.txt from the whole-run
# raw stdout every run keeps (<dir>/raw/<round>-<arm>-<suite>.txt, module
# header *EVIDENCE*), through the same ab_extract used live — so evidence
# captured under an old parser (or before this ADR at all) can be re-read
# honestly without a reboot. File-in, file-out: no cargo, no binary, no
# clock, and `runs.txt` itself is never opened for writing.
if [ "${1:-}" = "--reextract" ]; then
  [ $# -ge 2 ] || usage
  EVDIR=$2
  RAWDIR="$EVDIR/raw"
  [ -d "$RAWDIR" ] || {
    echo "no such raw directory: $RAWDIR" >&2
    exit 1
  }
  OUT="$EVDIR/runs.reextracted.txt"
  : >"$OUT"
  shopt -s nullglob
  for rawfile in "$RAWDIR"/*.txt; do
    base=$(basename "$rawfile" .txt)
    round=${base%%-*}
    rest=${base#*-}
    arm=${rest%%-*}
    ab_extract "$arm" "$round" <"$rawfile" >>"$OUT"
  done
  shopt -u nullglob
  echo "reextracted -> $OUT"
  exit 0
fi

DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

: "${ROUNDS:?ROUNDS must be set — the number of rotation rounds}"
: "${ARMS:?ARMS must be set — \"name=path:suite[,suite]… …\"}"
: "${CONTROL:?CONTROL must name one of the ARMS entries}"
: "${EVIDENCE:?EVIDENCE must be set — a directory outside /tmp}"
GAP=${GAP:-8}

case "$ROUNDS" in
'' | *[!0-9]* | 0)
  echo "ROUNDS must be a positive whole number, got '$ROUNDS'" >&2
  exit 2
  ;;
esac

# Plan trap table: /tmp is tmpfs here and evidence written there does not
# survive the shutdown a boot this long ends in — MEMORY.md "Scratchpad is
# tmpfs". Refused before a single directory is made.
case "$EVIDENCE" in
/tmp | /tmp/*)
  echo "REFUSING: EVIDENCE=$EVIDENCE is under /tmp, which is tmpfs on this" >&2
  echo "          machine — evidence written there is gone at shutdown," >&2
  echo "          which is exactly when this boot ends. Point EVIDENCE at" >&2
  echo "          target/ or another path that survives a reboot." >&2
  exit 1
  ;;
esac

# --- parse ARMS --------------------------------------------------------------

declare -a ARM_NAMES=()
declare -A ARM_PATH=()
declare -A ARM_SUITES=()

for entry in $ARMS; do
  name=${entry%%=*}
  rest=${entry#*=}
  if [ "$name" = "$entry" ] || [ -z "$name" ]; then
    echo "ARMS entry '$entry' is not 'name=path:suite[,suite]…'" >&2
    exit 2
  fi
  path=${rest%%:*}
  suites=${rest#*:}
  if [ "$path" = "$rest" ] || [ -z "$path" ] || [ -z "$suites" ]; then
    echo "ARMS entry '$entry' is not 'name=path:suite[,suite]…'" >&2
    exit 2
  fi
  if [ -n "${ARM_PATH[$name]:-}" ]; then
    echo "ARMS entry '$name' appears twice" >&2
    exit 2
  fi
  if [ ! -d "$path" ]; then
    echo "ARMS entry '$name': no such directory '$path'" >&2
    exit 2
  fi
  ARM_NAMES+=("$name")
  ARM_PATH[$name]=$path
  ARM_SUITES[$name]=$suites
done

if [ "${#ARM_NAMES[@]}" -eq 0 ]; then
  echo "ARMS named no arms" >&2
  exit 2
fi
if [ -z "${ARM_PATH[$CONTROL]:-}" ]; then
  echo "CONTROL='$CONTROL' is not one of ARMS' names: ${ARM_NAMES[*]}" >&2
  exit 2
fi

# RUSTFLAGS come from THIS tree — the one running the driver — never from an
# arm's own tree: ADR-0090 decision 2, "every arm is built with the same
# RUSTFLAGS bench.sh uses". Two different flag strings would be two
# different builds being compared as though they were one.
RUSTFLAGS_VAL="$("$HERE/check-bench-alignment.sh" --flags)"

# An arm's own feature map — read only if that tree's check-bench-alignment.sh
# exists (plan P1 row: "nếu cây đó có cờ, không thì rỗng"). Calls only cargo
# metadata (via --features-map), never a build.
features_for() { # features_for <arm-path> <pkg>
  local path=$1 pkg=$2
  [ -x "$path/scripts/check-bench-alignment.sh" ] || {
    echo ""
    return 0
  }
  (cd "$path" && ./scripts/check-bench-alignment.sh --features-map) |
    awk -F'\t' -v p="$pkg" '$1 == p { print $2; found = 1; exit } END { if (!found) print "" }'
}

# --- dry run: print the plan, touch nothing ----------------------------------

if [ "$DRY_RUN" = 1 ]; then
  echo "=== ab-rotation.sh --dry-run"
  echo "RUSTFLAGS  $RUSTFLAGS_VAL"
  echo "ROUNDS     $ROUNDS   CONTROL   $CONTROL   GAP   ${GAP}s"
  echo "EVIDENCE   $EVIDENCE (not created)"
  echo
  for ((round = 1; round <= ROUNDS; round++)); do
    if ((round % 2 == 1)); then
      order=("${ARM_NAMES[@]}")
    else
      order=()
      for ((i = ${#ARM_NAMES[@]} - 1; i >= 0; i--)); do order+=("${ARM_NAMES[i]}"); done
    fi
    echo "round $round: ${order[*]}"
    for name in "${order[@]}"; do
      echo "  check-machine.sh quiet row for $name"
      IFS=',' read -ra toks <<<"${ARM_SUITES[$name]}"
      for tok in "${toks[@]}"; do
        nofeat=0
        spec=$tok
        case "$tok" in *@nofeat)
          nofeat=1
          spec=${tok%@nofeat}
          ;;
        esac
        pkg=${spec%%/*}
        bnch=${spec#*/}
        feats=""
        [ "$nofeat" = 0 ] && feats=$(features_for "${ARM_PATH[$name]}" "$pkg")
        if [ -n "$feats" ]; then
          echo "  run (resolved binary of) cargo bench -p $pkg --bench $bnch --features $feats   [$name @ ${ARM_PATH[$name]}]"
        else
          echo "  run (resolved binary of) cargo bench -p $pkg --bench $bnch   [$name @ ${ARM_PATH[$name]}]"
        fi
      done
      echo "  sleep ${GAP}s (GAP)"
    done
  done
  echo
  echo "OK: dry run only — nothing was resolved, built or run"
  exit 0
fi

# --- real run: preflight resolve, then rounds --------------------------------

mkdir -p "$EVIDENCE/raw"
MANIFEST="$EVIDENCE/manifest.txt"
TIMELINE="$EVIDENCE/timeline.txt"
RUNS="$EVIDENCE/runs.txt"
touch "$MANIFEST" "$TIMELINE" "$RUNS"

declare -A RESOLVED_BIN=()

# Resolve every (arm, suite) exactly once, and REFUSE outright rather than
# let a single one compile. See the module header for the two paths this
# takes: manifest replay (no cargo at all) or a first-time cargo resolve
# whose `fresh` field is read back and enforced.
preflight() {
  local name path suites tok nofeat spec pkg bnch feats key existing binpath sha_old sha_now worktree_sha
  for name in "${ARM_NAMES[@]}"; do
    path=${ARM_PATH[$name]}
    IFS=',' read -ra suite_list <<<"${ARM_SUITES[$name]}"
    for tok in "${suite_list[@]}"; do
      nofeat=0
      spec=$tok
      case "$tok" in *@nofeat)
        nofeat=1
        spec=${tok%@nofeat}
        ;;
      esac
      pkg=${spec%%/*}
      bnch=${spec#*/}
      key="$name	$tok"

      existing=$(awk -F'\t' -v k="$name" -v s="$tok" '$1 == k && $2 == s { print; exit }' "$MANIFEST")
      if [ -n "$existing" ]; then
        binpath=$(printf '%s' "$existing" | cut -f6)
        sha_old=$(printf '%s' "$existing" | cut -f7)
        if [ ! -f "$binpath" ]; then
          echo "REFUSING: binary missing for arm '$name' suite '$tok': $binpath" >&2
          echo "          (recorded in $MANIFEST) — refusing rather than rebuilding it." >&2
          exit 1
        fi
        sha_now=$(sha256sum "$binpath" | awk '{print $1}')
        if [ "$sha_now" != "$sha_old" ]; then
          echo "REFUSING: sha256 mismatch for arm '$name' suite '$tok'." >&2
          echo "          manifest has $sha_old, binary is now $sha_now" >&2
          echo "          — the binary changed since the manifest was written." >&2
          echo "          Refusing rather than trusting a moved target." >&2
          exit 1
        fi
        RESOLVED_BIN[$key]=$binpath
        continue
      fi

      feats=""
      [ "$nofeat" = 0 ] && feats=$(features_for "$path" "$pkg")
      extra=()
      [ -n "$feats" ] && extra=(--features "$feats")

      json=$(cd "$path" && RUSTFLAGS="$RUSTFLAGS_VAL" cargo bench -q -p "$pkg" --bench "$bnch" \
        --no-run --message-format=json "${extra[@]}" 2>&1)

      if printf '%s\n' "$json" | grep -q '"reason":"compiler-artifact".*"fresh":false'; then
        echo "REFUSING: arm '$name' suite '$tok' is not pre-built with the exact" >&2
        echo "          RUSTFLAGS/features — cargo reports it would compile" >&2
        echo "          (fresh=false). Build it first, in $path:" >&2
        echo "            RUSTFLAGS=\"$RUSTFLAGS_VAL\" cargo bench -p $pkg --bench $bnch --no-run${feats:+ --features $feats}" >&2
        echo "          Refusing rather than compiling it here." >&2
        exit 1
      fi

      binpath=$(printf '%s\n' "$json" |
        jq -r 'select(.executable != null and (.target.kind[]? == "bench")) | .executable' | tail -1)
      if [ -z "$binpath" ] || [ ! -f "$binpath" ]; then
        echo "REFUSING: binary missing/unresolvable for arm '$name' suite '$tok'." >&2
        echo "$json" >&2
        exit 1
      fi

      sha_now=$(sha256sum "$binpath" | awk '{print $1}')
      worktree_sha=$(cd "$path" && git rev-parse HEAD 2>/dev/null || echo unknown)
      printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$name" "$tok" "$path" "$worktree_sha" "$feats" "$binpath" "$sha_now" >>"$MANIFEST"
      RESOLVED_BIN[$key]=$binpath
    done
  done
}

preflight

# Read check-machine.sh's own "machine is quiet" row (its threshold, its
# reading — one rule, one place) rather than a second copy of the busy-%
# calculation. `|| true`: check-machine.sh exits non-zero on unrelated rows
# (NIC affinity etc.) and this driver only reads the one row it names.
quiet_status() { # quiet_status -> prints "<busy-or-unknown> <ok|DISQUALIFIED>"
  local out line busy
  out=$("$HERE/check-machine.sh" 2>/dev/null || true)
  line=$(printf '%s\n' "$out" | grep -E 'machine is quiet' || true)
  busy=$(printf '%s\n' "$line" | grep -oE '[0-9]+% CPU busy' | grep -oE '^[0-9]+')
  if printf '%s\n' "$line" | grep -q '^PASS'; then
    printf '%s ok\n' "${busy:-unknown}"
  else
    printf '%s DISQUALIFIED\n' "${busy:-unknown}"
  fi
}

run_suite() { # run_suite <round> <arm> <suite-token> — prints its verdict word
  local round=$1 arm=$2 tok=$3 spec bin safe rawfile out code
  spec=${tok%@nofeat}
  bin=${RESOLVED_BIN["$arm	$tok"]}
  safe=${spec//\//_}
  rawfile="$EVIDENCE/raw/${round}-${arm}-${safe}.txt"
  out=$("$bin" 2>&1)
  code=$?
  printf '%s\n' "$out" >"$rawfile"

  local extracted rows over under nobase panic_line panic_m status
  extracted=$(printf '%s\n' "$out" | ab_extract "$arm" "$round")
  if [ -n "$extracted" ]; then
    printf '%s\n' "$extracted" >>"$RUNS"
  fi

  # `printf '%s' | grep -c .`, not `wc -l`: an empty $extracted must count as
  # zero rows, not one — docs/reference/wc-l-counts-an-empty-capture-as-one-line.md.
  rows=$(printf '%s' "$extracted" | grep -c .)
  over=$(printf '%s\n' "$extracted" | awk -F'\t' '$5 == "over"' | grep -c .)
  under=$(printf '%s\n' "$extracted" | awk -F'\t' '$5 == "under"' | grep -c .)
  nobase=$(printf '%s\n' "$extracted" | awk -F'\t' '$5 == "none"' | grep -c .)

  # The harness's own "<k> of <m> case(s) over the machine baseline:" line
  # from a panicking finish() (harness.rs:405-411) — ADR-0092 decision 2 reads
  # <m> back to check every case was printed before the assert fired.
  panic_line=$(printf '%s\n' "$out" | grep -E '^[0-9]+ of [0-9]+ case\(s\) over the machine baseline:$' | head -1)
  panic_m=""
  [ -n "$panic_line" ] && panic_m=$(printf '%s' "$panic_line" | awk '{print $3}')

  status=$(ab_suite_verdict "$code" "$rows" "$over" "$panic_m")

  printf 'round %s arm %s suite %s exit %s rows %s over %s under %s nobase %s  %s\n' \
    "$round" "$arm" "$spec" "$code" "$rows" "$over" "$under" "$nobase" "$status" >>"$TIMELINE"

  printf '%s\n' "$status"
}

for ((round = 1; round <= ROUNDS; round++)); do
  if ((round % 2 == 1)); then
    order=("${ARM_NAMES[@]}")
  else
    order=()
    for ((i = ${#ARM_NAMES[@]} - 1; i >= 0; i--)); do order+=("${ARM_NAMES[i]}"); done
  fi

  round_ok=1
  for name in "${order[@]}"; do
    read -r busy status < <(quiet_status)
    printf 'round %s arm %s busy %s%% %s\n' "$round" "$name" "$busy" "$status" >>"$TIMELINE"
    if [ "$status" = ok ]; then
      IFS=',' read -ra toks <<<"${ARM_SUITES[$name]}"
      for tok in "${toks[@]}"; do
        suite_status=$(run_suite "$round" "$name" "$tok")
        # OVER keeps the round complete (ADR-0092 decision 2) — only FAILED
        # drops it, the same as a busy/DISQUALIFIED arm.
        [ "$suite_status" = "FAILED" ] && round_ok=0
      done
      sleep "$GAP"
    else
      round_ok=0
    fi
  done

  if [ "$round_ok" = 1 ]; then
    echo "round $round complete" >>"$TIMELINE"
  else
    echo "round $round incomplete" >>"$TIMELINE"
  fi
done

echo "=== timeline"
cat "$TIMELINE"
echo
echo "=== summary"
ab_summary "$RUNS" "$TIMELINE" "$CONTROL"
