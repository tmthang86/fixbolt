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
#
# `[2026-09-14]` step A6 of docs/plans/2026-09-04-the-second-linux-desk.md:
# three independent additions, each off by default so `ARMS="hft:admin"` with
# no new variable set still runs the same command line it ran yesterday.
#   * `ARMS` grows a FOURTH, optional field: `mode:path:tls:interval`, the
#     interval in microseconds passed on as `--interval <us>` (tools/w2w's
#     own pacing, module doc "Two halves, and pacing" — it spins, it never
#     sleeps). `mode:path` and `mode:path:tls` still mean exactly what they
#     meant before; `0`, the default, adds no flag and names itself in no
#     line, same as today.
#   * `LISTEN=<addr>` splits each run into two processes: `--listen` here,
#     locally, and `--connect` either here too (`GENERATOR_SSH` empty — a
#     loopback split, useful for rehearsing this script before a cable is
#     run) or on another host over `ssh`. The combined, single-process run
#     `ARMS` has always meant is what runs when `LISTEN` is unset — nothing
#     about that path changes below. A split run's figures are the
#     generator's table, headed "as the counterparty sees it" in tools/w2w
#     itself; this script repeats that label rather than publish them as an
#     acceptor wire figure. `--tls` other than `off` is refused for a split
#     arm before anything runs — tools/w2w refuses it too (the self-signed
#     certificate is made per process, and a split run is two processes),
#     this is just the earlier, clearer refusal.
#   * `FIXBOLT_NIC` reaches the two `scripts/check-machine.sh` calls below the
#     same way `RUNS` or `GAP` always have: bash hands a script's whole
#     environment to everything it execs, and `check-machine.sh` already
#     reads `${FIXBOLT_NIC:-}` (same default-to-empty as an unset variable),
#     so `FIXBOLT_NIC=enp9s0 scripts/w2w-baseline.sh` needed nothing new here.
#     The `export` below is that claim made checkable rather than assumed.
#
# `[2026-09-14]` senior review of PR #72, three fixes to the split run:
#   * `WIRE_NIC=<ifname>` with `OBSERVER_CORE=<cpu>` makes a split run take the
#     acceptor's own wire figure (plan B6; Sửa 2, Điều 4(d) and Q10): the listen
#     half gets `--wire-timestamps --nic $WIRE_NIC --observer-core
#     $OBSERVER_CORE --warmup $WARMUP` — the same `--warmup` the generator gets,
#     so its cold requests are left out of the wire window rather than being the
#     p99.9. Before this, A6 could run a split but never ask for a wire figure,
#     so B6 had no procedure. Refused before anything runs: without `LISTEN`
#     (the combined run is over loopback, which a hardware NIC never carries),
#     without `OBSERVER_CORE`, with the observer on a measured core, and for
#     any `standard` arm (Q10 — `standard` publishes no wire figure; run it in
#     its own invocation without `WIRE_NIC`, for the counterparty table only).
#     **A run whose `hw-rx-missing` or `hw-tx-missing` is not 0 FAILS the
#     script; it is not DISQUALIFIED.** DISQUALIFIED here means a busy machine,
#     which the next run can escape by waiting. A missing hardware stamp is the
#     instrument: the plan's `igb` trap (a link change silently drops RX stamps
#     to software, and the driver's read-back lies about it) persists into every
#     later run until a person runs `ethtool -T` / link down-up and records the
#     kernel — so carrying on would spend RUNS x GAP producing nothing, and a
#     sample built from the runs that happened to keep their stamps would be a
#     selected sample. The FAIL prints both counts and the listen half's output,
#     which is the finding B6 records.
#   * With `GENERATOR_SSH` the generator is NOT pinned by this script (it runs
#     with no `--client-core`), and the header and summary now say so instead
#     of naming `client cpu$CLIENT_CORE`.
#   * The generator connects to the address the listen half PRINTED on its
#     `listening:` line, not to `$LISTEN`, so `LISTEN=127.0.0.1:0` works.
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
# Arms, as `mode:path` pairs, optionally `mode:path:tls` or
# `mode:path:tls:interval` — see the `[2026-09-13]` and `[2026-09-14]` notes
# above. Both modes because ADR-0013 says a change proven in one mode is
# proven in neither; both paths because an app figure is not an admin figure.
ARMS=${ARMS:-"hft:admin hft:app standard:admin standard:app"}
# PIN=0 runs with no pinning at all, and ALLOW_UNISOLATED=1 pins to a core
# isolcpus does not name. Both are how the A/B in
# `docs/reference/measured-costs.md` was taken; neither produces a §9 figure,
# and the per-run lines say `unpinned` so that a pasted output cannot be
# mistaken for one.
PIN=${PIN:-1}
ALLOW_UNISOLATED=${ALLOW_UNISOLATED:-0}
# Split mode (`[2026-09-14]` step A6). Empty `LISTEN` is the combined,
# single-process run this script has always done; everything in this block is
# inert until `LISTEN` is set. `GENERATOR_W2W` is the remote binary's name on
# `GENERATOR_SSH`'s `PATH` — `w2w` there is the usual case once it is
# installed rather than built fresh on every run.
LISTEN=${LISTEN:-}
GENERATOR_SSH=${GENERATOR_SSH:-}
GENERATOR_W2W=${GENERATOR_W2W:-w2w}
# The acceptor's wire figure (`[2026-09-14]` review note above). Both empty is
# a split run exactly as before.
WIRE_NIC=${WIRE_NIC:-}
OBSERVER_CORE=${OBSERVER_CORE:-}
# Read the same way `check-machine.sh` reads it (`NIC="${FIXBOLT_NIC:-}"`), and
# exported so that claim is this line rather than an assumption about bash —
# `${FIXBOLT_NIC:-}` reads identically whether it was unset or empty, so this
# changes nothing for a run that never sets it.
export FIXBOLT_NIC="${FIXBOLT_NIC:-}"
BIN=target/release/w2w
PINARGS=()
# Split-mode pin args, one core per process rather than two in one — tools/w2w
# refuses `--client-core` to `--listen` and `--engine-core` to `--connect`
# (module doc "Two halves, and pacing"), so each half gets only the flag that
# applies to it.
LISTEN_PINARGS=()
CONNECT_PINARGS=()
if [ "$PIN" = 1 ]; then
  PINARGS=(--engine-core "$ENGINE_CORE" --client-core "$CLIENT_CORE")
  LISTEN_PINARGS=(--engine-core "$ENGINE_CORE")
  CONNECT_PINARGS=(--client-core "$CLIENT_CORE")
  if [ "$ALLOW_UNISOLATED" = 1 ]; then
    PINARGS+=(--allow-unisolated)
    LISTEN_PINARGS+=(--allow-unisolated)
    CONNECT_PINARGS+=(--allow-unisolated)
  fi
fi

# Every refusal that does not need the binary, before the machine block and
# before any run — a B6 procedure that stops at arm three over a flag that was
# wrong from the start has wasted two arms of a shared desk's time.
refuse() { echo "refused before running: $*"; exit 1; }
if [ -n "$OBSERVER_CORE" ] && [ -z "$WIRE_NIC" ]; then
  refuse "OBSERVER_CORE is set and WIRE_NIC is not — the observer belongs to --wire-timestamps"
fi
if [ -n "$WIRE_NIC" ]; then
  [ -n "$LISTEN" ] || refuse "WIRE_NIC needs LISTEN: the combined run is over loopback, and a hardware NIC never carries it"
  case "$OBSERVER_CORE" in
    ''|*[!0-9]*) refuse "WIRE_NIC needs OBSERVER_CORE=<cpu>: the observer spins, and w2w will not run it unpinned" ;;
  esac
  [ "$OBSERVER_CORE" != "$ENGINE_CORE" ] || refuse "OBSERVER_CORE $OBSERVER_CORE is ENGINE_CORE — the observer spins on its core"
  if [ -z "$GENERATOR_SSH" ] && [ "$PIN" = 1 ] && [ "$OBSERVER_CORE" = "$CLIENT_CORE" ]; then
    refuse "OBSERVER_CORE $OBSERVER_CORE is CLIENT_CORE, and the generator runs on this host"
  fi
  for arm in $ARMS; do
    case "${arm%%:*}" in
      hft) ;;
      *) refuse "ARMS entry '$arm' with WIRE_NIC: only hft publishes a wire figure (plan Sửa 2, Q10). Run standard arms in their own invocation without WIRE_NIC — the counterparty table only" ;;
    esac
  done
fi
if [ -n "$GENERATOR_SSH" ]; then
  case "${LISTEN%:*}" in
    ''|0.0.0.0|'[::]') refuse "LISTEN=$LISTEN binds every address, and GENERATOR_SSH's host needs one it can reach — name this host's address on the link" ;;
  esac
fi

# The machine block travels with the figures, read off the box rather than
# asserted — CLAUDE.md §2 non-negotiable 10. Its verdict is captured because
# `benches/baselines.tsv` records one per line for the same reason.
echo "=============================================================="
scripts/check-machine.sh || true
# `[2026-09-14]` fix: `check-machine.sh` exits non-zero on any FAIL row, and
# under `set -o pipefail` that made the old one-liner's `|| echo "unknown"`
# fire IN ADDITION to the line grep had already matched — pipefail reports
# the pipeline's status as check-machine.sh's, not grep's, even once grep has
# found its line, so the machine record (CLAUDE.md §2 rule 10) doubled to
# `pass N fail N unknown N` followed by a bare second `unknown` line. Capturing
# check-machine.sh's output first, with its own exit status thrown away by
# `|| true`, means the `echo | grep` pipeline that follows carries only
# grep's status.
MACHINE_OUT=$(scripts/check-machine.sh 2>/dev/null || true)
VERDICT=$(echo "$MACHINE_OUT" | grep -E '^pass [0-9]+' || echo "unknown")
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

# Poll `file` for a line matching `pat`, up to `timeout_s` — the `--listen`
# half prints `listening:` right after `bind`, long before its peer connects,
# and Rust's `Stdout` is always line-buffered (never the C-stdio habit of full
# buffering off a terminal), so the line is in the file as soon as it is
# printed.
wait_for_line() {
  local file=$1 pat=$2 timeout_s=$3 i=0
  while ! grep -q "$pat" "$file" 2>/dev/null; do
    i=$((i+1))
    [ "$i" -ge $((timeout_s*10)) ] && return 1
    sleep 0.1
  done
  return 0
}

# Wait for a backgrounded pid up to `timeout_s`, killing it and returning 124
# on a timeout. The engine half serves until its peer's connection closes; a
# generator that never reached `--connect` at all (a dead ssh host, say) would
# otherwise leave it waiting forever and hang this script, not just that run.
wait_with_timeout() {
  local pid=$1 timeout_s=$2 i=0
  while kill -0 "$pid" 2>/dev/null; do
    i=$((i+1))
    if [ "$i" -ge $((timeout_s*10)) ]; then
      kill "$pid" 2>/dev/null
      wait "$pid" 2>/dev/null
      return 124
    fi
    sleep 0.1
  done
  wait "$pid"
}

echo "runs $RUNS   messages $MESSAGES   warmup $WARMUP   gap ${GAP}s"
if [ "$PIN" = 1 ] && [ -n "$LISTEN" ] && [ -n "$GENERATOR_SSH" ]; then
  echo "engine cpu$ENGINE_CORE   generator on $GENERATOR_SSH, NOT pinned by this script   (allow-unisolated $ALLOW_UNISOLATED)"
elif [ "$PIN" = 1 ]; then
  echo "engine cpu$ENGINE_CORE   client cpu$CLIENT_CORE   (allow-unisolated $ALLOW_UNISOLATED)"
else
  echo "UNPINNED — not a DESIGN.md §9 figure whatever check-machine.sh says above"
fi
if [ -n "$LISTEN" ]; then
  echo "split mode: listen $LISTEN, generator ${GENERATOR_SSH:-this host (loopback split)}"
fi
if [ -n "$WIRE_NIC" ]; then
  echo "wire timestamps: $WIRE_NIC, observer cpu$OBSERVER_CORE, listen --warmup $WARMUP"
fi
echo

for arm in $ARMS; do
  # `mode:path`, `mode:path:tls` or `mode:path:tls:interval` — a missing field
  # reads as empty from `read` and is defaulted below, so a two- or
  # three-field arm means exactly what it meant before the fourth field
  # existed.
  IFS=':' read -r mode path tls interval <<< "$arm"
  tls=${tls:-off}
  interval=${interval:-0}
  case "$interval" in
    ''|*[!0-9]*)
      echo "ARMS entry '$arm': fourth field '$interval' is not a whole number of microseconds"
      exit 1
      ;;
  esac
  tls_args=()
  [ "$tls" != "off" ] && tls_args=(--tls "$tls")
  interval_args=()
  [ "$interval" != 0 ] && interval_args=(--interval "$interval")
  # Named in the per-run line and the summary header only when it is not the
  # default, so an arm with no fourth field prints byte-identically to before.
  iv_note=""
  [ "$interval" != 0 ] && iv_note="  interval ${interval}us"
  # What the engine must REPORT for this arm, not what the flag is spelled —
  # `--tls ktls` reads back as `tls: kernel` (tools/w2w/src/main.rs
  # `seen_name`), and a handover that quietly fell back to userspace must
  # disqualify the run rather than publish its p50 under the `ktls` label it
  # never earned.
  case "$tls" in
    ktls) want_tls=kernel ;;
    *) want_tls=$tls ;;
  esac

  if [ -n "$LISTEN" ] && [ "$tls" != off ]; then
    echo "ARMS entry '$arm': LISTEN is set and tls is '$tls' — refused before running."
    echo "tools/w2w refuses --tls other than off to both --listen and --connect: the"
    echo "self-signed certificate is made per process, and a split run is two processes."
    exit 1
  fi

  gen_host="${GENERATOR_SSH:-this host (loopback split)}"

  p50s=(); p99s=(); p999s=(); mins=(); skipped=0
  wp50s=(); wp99s=(); wp999s=()
  wire_args=()
  if [ -n "$WIRE_NIC" ]; then
    wire_args=(--wire-timestamps --nic "$WIRE_NIC" --observer-core "$OBSERVER_CORE" --warmup "$WARMUP")
  fi
  for i in $(seq 1 "$RUNS"); do
    b=$(busy_pct)
    if [ "$b" -gt 3 ]; then
      printf '  %-8s %-5s %-9s run %2d  DISQUALIFIED, %s%% busy%s\n' "$mode" "$path" "$tls" "$i" "$b" "$iv_note"
      skipped=$((skipped+1))
      sleep "$GAP"
      continue
    fi

    if [ -n "$LISTEN" ]; then
      # The split run: the engine half in the background, the generator half
      # either here or over ssh, then both outputs read back before either
      # figure is trusted — same order as the combined run below: identity
      # first, allocations second, exit status last, because the checks above
      # name the cause better than a bare nonzero status ever could.
      listen_log=$(mktemp)
      "$BIN" --listen "$LISTEN" --mode "$mode" --path "$path" "${LISTEN_PINARGS[@]}" "${wire_args[@]}" \
        >"$listen_log" 2>&1 &
      listen_pid=$!

      if ! wait_for_line "$listen_log" '^listening: ' 5; then
        kill "$listen_pid" 2>/dev/null; wait "$listen_pid" 2>/dev/null
        cat "$listen_log"; rm -f "$listen_log"
        echo "FAIL: $mode:$path:$tls:$interval — engine half never printed 'listening:' within 5s"
        exit 1
      fi

      # The address the engine half actually bound, off its own line — `$LISTEN`
      # itself is only right when it names a port, and `:0` asks the kernel for
      # one (tools/w2w module doc: `listening:` prints the bound address).
      connect_addr=$(awk '/^listening: /{print $2; exit}' "$listen_log")

      rc=0
      if [ -z "$GENERATOR_SSH" ]; then
        cout=$("$BIN" --connect "$connect_addr" --path "$path" "${CONNECT_PINARGS[@]}" \
                 --messages "$MESSAGES" --warmup "$WARMUP" "${interval_args[@]}" 2>&1) || rc=$?
      else
        # Separate `ssh` arguments, not one interpolated string: `sshd` joins
        # them with spaces before handing the line to the remote shell, so
        # nothing here is evaluated a second time by this shell first, and
        # none of these tokens (an address, a word, a number) needs quoting
        # either side of the hop.
        cout=$(ssh "$GENERATOR_SSH" "$GENERATOR_W2W" --connect "$connect_addr" --path "$path" \
                 --messages "$MESSAGES" --warmup "$WARMUP" "${interval_args[@]}" 2>&1) || rc=$?
      fi

      lrc=0
      wait_with_timeout "$listen_pid" 5 || lrc=$?
      lout=$(cat "$listen_log"); rm -f "$listen_log"

      echo "$cout" | grep -qx "path: $path" || {
        echo "$cout"
        echo "FAIL: $mode:$path:$tls:$interval — generator ran a path other than '$path'"
        exit 1
      }
      echo "$lout" | grep -qx "mode: $mode" || {
        echo "$lout"
        echo "FAIL: $mode:$path:$tls:$interval — engine half ran a mode other than '$mode'"
        exit 1
      }
      echo "$lout" | grep -qx "path: $path" || {
        echo "$lout"
        echo "FAIL: $mode:$path:$tls:$interval — engine half ran a path other than '$path'"
        exit 1
      }
      echo "$lout" | grep -qx "tls: off" || {
        echo "$lout"
        echo "FAIL: $mode:$path:$tls:$interval — engine half ran a tls other than off"
        exit 1
      }
      echo "$lout" | grep -qE '^ *allocs +0 ' || { echo "$lout"; echo "allocs != 0 (engine half)"; exit 1; }
      echo "$cout" | grep -qE '^ *allocs +0 ' || { echo "$cout"; echo "allocs != 0 (generator half)"; exit 1; }
      if [ "$rc" -ne 0 ] || [ "$lrc" -ne 0 ]; then
        echo "$lout"
        echo "$cout"
        echo "FAIL: $mode:$path:$tls:$interval — split run: listen exit $lrc, connect exit $rc"
        exit 1
      fi

      # The acceptor's wire figure, read off the listen half's own lines
      # (tools/w2w/src/main.rs `Observed::report`: `hw-rx-missing <n> of <m>`,
      # `hw-tx-missing <n> of <m>`, `wire p50 <ns> ns`, ...). After the exit
      # status: a report that could not trust its tap returns an error before it
      # prints any of these, and the status check above has already shown it.
      wire_note=""
      if [ -n "$WIRE_NIC" ]; then
        rxm=$(echo "$lout" | awk '$1=="hw-rx-missing" {print $2; exit}')
        txm=$(echo "$lout" | awk '$1=="hw-tx-missing" {print $2; exit}')
        if [ "$rxm" != 0 ] || [ "$txm" != 0 ]; then
          echo "$lout"
          echo "FAIL: $mode:$path:$tls:$interval — run $i: hw-rx-missing ${rxm:-<not printed>}, hw-tx-missing ${txm:-<not printed>} on $WIRE_NIC."
          echo "A run with any missing hardware stamp is not a wire figure (plan B6 publishes only missing 0)."
          echo "Not DISQUALIFIED: the cause is the NIC's stamping, not load, and it will not clear by waiting —"
          echo "see the plan's igb trap (ethtool -T, link down/up), and record this count and the kernel version."
          exit 1
        fi
        wv() { echo "$lout" | awk -v k="$1" '$1=="wire" && $2==k {print $3; exit}'; }
        wp50=$(wv p50); wp99=$(wv p99); wp999=$(wv p99.9)
        if [ -z "$wp50" ] || [ -z "$wp99" ] || [ -z "$wp999" ]; then
          echo "$lout"
          echo "FAIL: $mode:$path:$tls:$interval — run $i: missing counts are 0 and no wire column was printed (tap drops or overflow — read the listen half above)"
          exit 1
        fi
        wp50s+=("$wp50"); wp99s+=("$wp99"); wp999s+=("$wp999")
        wire_note=$(printf '  wire p50 %8s  p99 %8s  p99.9 %8s  (acceptor, %s)' "$wp50" "$wp99" "$wp999" "$WIRE_NIC")
      fi

      out="$cout"
      g() { echo "$out" | awk -v k="$1" '$1==k {print $2}'; }
      mins+=("$(g min)"); p50s+=("$(g p50)"); p99s+=("$(g p99)"); p999s+=("$(g p99.9)")
      printf '  %-8s %-5s %-9s run %2d  %s%% busy   min %8s  p50 %8s  p99 %8s  p99.9 %8s  (counterparty: %s)%s%s\n' \
        "$mode" "$path" "$tls" "$i" "$b" "$(g min)" "$(g p50)" "$(g p99)" "$(g p99.9)" "$gen_host" "$iv_note" "$wire_note"
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
    out=$("$BIN" --mode "$mode" --path "$path" "${tls_args[@]}" "${interval_args[@]}" "${PINARGS[@]}" \
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
    printf '  %-8s %-5s %-9s run %2d  %s%% busy   min %8s  p50 %8s  p99 %8s  p99.9 %8s%s\n' \
      "$mode" "$path" "$tls" "$i" "$b" "$(g min)" "$(g p50)" "$(g p99)" "$(g p99.9)" "$iv_note"
    sleep "$GAP"
  done

  q=${#p50s[@]}
  echo
  if [ "$q" -eq 0 ]; then
    echo "  == $mode / $path / $tls: NO QUALIFYING RUNS ($skipped disqualified) ==$iv_note"
    echo
    continue
  fi
  m50=$(printf '%s\n' "${p50s[@]}" | median)
  m99=$(printf '%s\n' "${p99s[@]}" | median)
  m999=$(printf '%s\n' "${p999s[@]}" | median)
  mmin=$(printf '%s\n' "${mins[@]}" | median)
  x50=$(printf '%s\n' "${p50s[@]}" | sort -n | tail -1)
  n50=$(printf '%s\n' "${p50s[@]}" | sort -n | head -1)
  echo "  == $mode / $path / $tls: median of $q qualifying runs ($skipped disqualified) ==$iv_note"
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
  if [ "$PIN" = 1 ] && [ -n "$LISTEN" ] && [ -n "$GENERATOR_SSH" ]; then
    echo "     pinned  engine cpu$ENGINE_CORE; the generator on $GENERATOR_SSH is NOT pinned by this script"
  elif [ "$PIN" = 1 ]; then
    echo "     pinned  engine cpu$ENGINE_CORE, client cpu$CLIENT_CORE"
  else
    echo "     pinned  NO — NOT a §9 figure"
  fi
  if [ -n "$LISTEN" ]; then
    echo "     split   as the counterparty sees it — not an acceptor wire figure"
    echo "     generator  $gen_host"
  fi
  if [ -n "$WIRE_NIC" ]; then
    wm50=$(printf '%s\n' "${wp50s[@]}" | median)
    wx50=$(printf '%s\n' "${wp50s[@]}" | sort -n | tail -1)
    wn50=$(printf '%s\n' "${wp50s[@]}" | sort -n | head -1)
    echo
    echo "     -- acceptor wire figure: NIC in -> NIC out at the acceptor, $WIRE_NIC hardware stamps --"
    echo "     -- median of the same $q runs; NOT the counterparty table above, and nothing is subtracted --"
    echo "     wire p50    $wm50 ns      (across runs: $wn50 .. $wx50)"
    echo "     wire p99    $(printf '%s\n' "${wp99s[@]}" | median) ns"
    echo "     wire p99.9  $(printf '%s\n' "${wp999s[@]}" | median) ns"
    echo "     window      every request after the logon, leaving out the first $WARMUP"
    echo "     stamps      hw-rx-missing 0 and hw-tx-missing 0 in all $q runs"
    echo "     observer    cpu$OBSERVER_CORE"
  fi
  echo
done
