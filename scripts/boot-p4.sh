#!/usr/bin/env bash
# The phase-4 §9 boot, unattended: the `io_uring` A/B (phase-4 row 5) and the
# SQLite-store `w2w` pair (row 4, step 4b) in two procedures, from binaries
# built before the reboot. ADR-0202 decisions 1-3; plan
# docs/plans/2026-09-24-p4-bypass-and-s9-boot.md, "Hàng 7", row 7a.1.
#
# WHY A COMMITTED DRIVER
#
# A reboot ends the manager's session, and a manager tool call during a
# rotation costs an arm-round (boot E). So the boot is one command, launched
# once, read once when it exits: every block and every arm below runs in an
# order written here, nothing asks a question, and every output lands under
# target/boot-p4-evidence/ (never /tmp, which is tmpfs on the desk).
#
# SUBCOMMANDS
#
#   scripts/boot-p4.sh build   Pre-build (ADR-0090 decision 2, ADR-0202
#                              decision 1): one git worktree per feature set
#                              under $BOOT_ROOT at $COMMIT, `bench.sh`'s
#                              RUSTFLAGS, then MANIFEST.txt and BUILD-INFO.txt.
#                              Idempotent: an existing tree at $COMMIT is
#                              reused and cargo finds it fresh.
#   scripts/boot-p4.sh [run]   The boot itself.
#
# WHAT `build` MAKES (the interface `run` reads — one file owns both halves)
#
#   $BOOT_ROOT/uring    worktree, `cargo build --release -p fixbolt-w2w
#                       --features affinity,io-uring` (K, U, U′, S, std), and
#                       `cargo bench -p fixbolt-engine --features io-uring
#                       --bench turn --bench density --no-run` (idle, density)
#   $BOOT_ROOT/sqlite   worktree, `cargo build --release -p fixbolt-w2w
#                       --features affinity,sqlite` (the store pair: both
#                       `--journal file-async` and `--journal sqlite-async`
#                       from this ONE binary — ADR-0202 decision 1)
#   (both w2w)          `setcap cap_net_raw,cap_net_admin+ep` — the NIC tap
#                       opens AF_PACKET; `run` refuses a w2w without it
#   MANIFEST.txt        `sha256sum` format, absolute paths, four lines: both
#                       `w2w`, `turn-<hash>`, `density-<hash>`. `sha256sum -c
#                       MANIFEST.txt` is plan row 7a.2's gate.
#   BUILD-INFO.txt      `key value` lines: commit, rustflags, rustc, turn,
#                       density, and the Mac's `mac_head` / `mac_w2w_sha256`
#                       as read over ssh when `build` ran.
#
#   The Mac's `w2w` is NOT built here (plan row 7a.2 gives that to the
#   runner). Build it at $COMMIT first, then run `build` (again) so
#   BUILD-INFO.txt records it:
#     git push thangtran@192.168.77.2:Projects/fixbolt.git <commit>:refs/heads/boot-p4
#     ssh thangtran@192.168.77.2 'cd Projects/nanofixengine && git fetch -q origin \
#       && git checkout -q --detach <commit> && ~/.cargo/bin/cargo build --release -p fixbolt-w2w'
#
# THE ORDER (ADR-0202 decision 3; plan "Hàng 7" 7a step 1)
#
#   Procedure 1   A  io_uring w2w   K U U′ S (hft:admin, NIC stamps), stdK stdU
#                 B  bench          turn density (pinned to the engine core)
#                 C  store pair     hft-file hft-sqlite (hft:app, NIC stamps),
#                                   std-file std-sqlite (standard:app, Mac side)
#   >= MIN_GAP_S seconds, read off the clock, machine left alone (ADR-0068)
#   Procedure 2   C B A, and the arms inside each block reversed
#   Last          check-machine.sh once more
#
#   K     W2W_EXTRA=""                                              observer 7
#   U     W2W_EXTRA="--transport uring"                             observer 7
#   U′    W2W_EXTRA="--transport uring"                             observer 5
#   S     W2W_EXTRA="--transport uring --uring-arm sqpoll --sqpoll-core 7"
#                                                                   observer 5
#   NIC stamps are turned on ONLY by WIRE_NIC/OBSERVER_CORE of w2w-baseline.sh
#   (plan Sửa 3; ADR-0190 Revision 3, R6). `standard` arms run without them
#   (w2w-baseline.sh refuses WIRE_NIC for standard — Q10); their measure is the
#   Mac's own table, "as the counterparty sees it".
#
# WHAT STOPS THE BOOT (exit 3, the reason printed as `STOPPED: …`)
#
#   * a red row of `FIXBOLT_NIC=$NIC scripts/check-machine.sh` before any
#     block — FAIL and `? ? ?` alike, since the script itself says `unknown`
#     is not a pass. The row is named. One exception, and only for the row
#     `machine is quiet` when it is the ONLY red row: re-read up to
#     QUIET_RETRIES times, QUIET_RETRY_S apart, every attempt kept — a
#     one-second busy read must not cost the night, and w2w-baseline.sh
#     re-reads quiet per run anyway;
#   * a binary whose sha256 is not MANIFEST.txt's, before or after any arm,
#     or a `binary`/`generator binary` line in an arm's own output that
#     disagrees; a worktree not at BUILD-INFO.txt's commit;
#   * the generator unreachable (`ssh -o BatchMode=yes`) or the cable without
#     carrier before an arm.
#   An arm that FAILS on its own (w2w-baseline.sh exit non-zero, a transport
#   line that is not the arm's) is recorded FAILED and the boot goes on —
#   a failure early in the night must not cost the rest (ADR-0202
#   Consequences); the exit status is then 1 and the summary names it.
#
# REHEARSAL (plan 7a step 2) — REHEARSAL=1, and nothing below is a figure
#
#   Only a rehearsal may set: TOLERATE_ROWS (comma list of check-machine row
#   names allowed red — the desktop grub line's), ALLOW_UNISOLATED=1, RUNS
#   below 10, MIN_GAP_S below 1800, and a Mac whose HEAD or `w2w` is not
#   BUILD-INFO.txt's (it is then held to what the driver read at the start).
#   Each is printed loudly; the evidence directory, every summary line and
#   verdict-inputs.txt say REHEARSAL — NOT A MEASUREMENT. Without REHEARSAL=1
#   each of them is refused before anything runs.
#
# EVIDENCE — target/boot-p4-evidence/<UTC>[-REHEARSAL]/
#
#   driver.log            everything this script printed
#   settings.txt          every variable, the cmdline, the Mac identity
#   manifest-start.txt    `sha256sum -c MANIFEST.txt` before the first block
#   manifest-end.txt      … after the last
#   p<N>/<block>/gate-check-machine*.txt   the gate read before that block
#   p<N>/A-uring-w2w/<arm>/   w2w-baseline.sh's OUT_DIR (summary.txt, every
#                         run's generator and listen output, dumps) and
#                         baseline.log (its stdout, check-machine included)
#   p<N>/B-bench/<turn|density>.txt   the bench binary's whole output
#   p<N>/C-store/<arm>/   as A
#   gap.txt               the clock read at both ends of the wait
#   final-check-machine.txt
#   compare/*.txt         scripts/compare-w2w-procedures.sh, every pair
#   verdict-inputs.txt    every number the two kill lines read, the file it
#                         came from, and the ratios — computed here, never by
#                         hand. The verdict itself is 7b.3's (plan rows 4, 5).
#   summary.txt           the status lines (no latency figure in them)
#
# WHAT THIS CANNOT SEE
#
#   * It proves the binaries did not change; not that they were built from
#     the commit BUILD-INFO.txt names (that is `build` and the worktree HEAD
#     check). The Mac's binary is identified by sha256 and `git rev-parse
#     HEAD` of its checkout — a binary built from a dirty tree there reads
#     the same.
#   * `check-machine.sh` is read once per block, and w2w-baseline.sh reads it
#     once per arm and the quiet row once per run; a setting that flips
#     between two reads is seen at the next one, not when it flips.
#   * The bench is one run per procedure (best-of-7 inside the binary), as
#     ADR-0190 decision 10 writes it; FIXBOLT_BENCH_COUNT_ONLY=1 so that an
#     unrelated case over its baseline cannot abort the run before the idle
#     pairs print (ADR-0102 decision 2) — no baseline is judged here.
#   * It computes the ratios each kill line reads; it does not apply them.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
REPO=$(pwd -P)

SUB=${1:-run}

# ---------------------------------------------------------------- settings
BOOT_ROOT=${BOOT_ROOT:-"$REPO/../fb-p4-boot"}
COMMIT=${COMMIT:-HEAD}
NIC=${NIC:-enp9s0}
LISTEN=${LISTEN:-192.168.77.1:0}
GENERATOR_SSH=${GENERATOR_SSH:-thangtran@192.168.77.2}
GENERATOR_W2W=${GENERATOR_W2W:-Projects/nanofixengine/target/release/w2w}
RUNS=${RUNS:-10}
MESSAGES=${MESSAGES:-20000}
MIN_GAP_S=${MIN_GAP_S:-1800}
REHEARSAL=${REHEARSAL:-0}
TOLERATE_ROWS=${TOLERATE_ROWS:-}
ALLOW_UNISOLATED=${ALLOW_UNISOLATED:-0}
QUIET_RETRIES=${QUIET_RETRIES:-3}
QUIET_RETRY_S=${QUIET_RETRY_S:-60}
die() { echo "boot-p4: $*" >&2; exit 2; }
isnum() { case "$1" in '' | *[!0-9]*) return 1 ;; esac; }
# Validated here, before any `$(( ))` reads them: under `set -u`, bash
# arithmetic takes `RUNS=x` as the name of an unset variable and dies with
# "unbound variable" (exit 1), naming neither the knob nor this script.
for v in RUNS MESSAGES MIN_GAP_S QUIET_RETRIES QUIET_RETRY_S; do
  isnum "${!v}" || die "$v='${!v}' is not a whole number"
done
ARM_TIMEOUT_S=${ARM_TIMEOUT_S:-$((RUNS * 120 + 600))}
BENCH_TIMEOUT_S=${BENCH_TIMEOUT_S:-3600}
for v in ARM_TIMEOUT_S BENCH_TIMEOUT_S; do
  isnum "${!v}" || die "$v='${!v}' is not a whole number"
done
# Passed to w2w-baseline.sh explicitly, never inherited from whatever the
# caller's shell happens to export: its own defaults are what ADR-0190
# decision 10 measured with, and a stray `export GAP=0` must not reach it.
readonly W2W_PIN=1 W2W_WARMUP=2000 W2W_GAP=8 W2W_CLIENT_CORE=7
# Fixed by the plan (Sửa 3) and ADR-0190 R6, not knobs: 6 is the engine's, 7
# the observer's (or the SQ thread's in S), 5 is housekeeping — the observer
# of U′ and S. 14/15 are 6/7's SMT siblings, offline in the boot.
readonly ENGINE_CORE=6 OBSERVER_CORE_A=7 OBSERVER_CORE_S=5 SQPOLL_CORE=7
readonly MIN_REAL_GAP_S=1800 MIN_REAL_RUNS=10 REAL_MESSAGES=20000
# ServerAlive: a Mac that drops off the cable mid-command ends the call in
# ~15 s instead of never; every call is also under `timeout` (mac() below).
SSH_OPTS="-o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=5 -o ServerAliveCountMax=3"
readonly SSH_OPTS MAC_TIMEOUT_S=60

# `build` creates BOOT_ROOT; `run` only ever reads one that exists.
[ "$SUB" != build ] || mkdir -p "$BOOT_ROOT" || die "cannot create BOOT_ROOT $BOOT_ROOT"
root_abs=$(cd "$BOOT_ROOT" 2>/dev/null && pwd -P) || die "BOOT_ROOT $BOOT_ROOT is not a directory — run 'scripts/boot-p4.sh build' first"
BOOT_ROOT=$root_abs

has_caps() { # has_caps <w2w> — the NIC tap's file capability is on it, and in force
  getcap "$1" 2>/dev/null | grep -qE 'cap_net_admin,cap_net_raw[=+]ep' || return 1
  # `[measured 2026-09-24]` a w2w under /tmp (tmpfs, `nosuid` on the desk)
  # carried the capability by getcap and still failed AF_PACKET with EPERM:
  # the kernel ignores file capabilities on a nosuid mount. So getcap alone
  # is a green that runs nothing.
  case ",$(findmnt -no OPTIONS -T "$1" 2>/dev/null)," in
    *,nosuid,*) return 1 ;;
  esac
}
mac() { # mac <remote command>   — never prompts: BatchMode
  # shellcheck disable=SC2086,SC2029 # SSH_OPTS is options on purpose; the command is built here, on purpose
  timeout --kill-after=5 "$MAC_TIMEOUT_S" ssh $SSH_OPTS "$GENERATOR_SSH" "$1"
}
mac_identity() { # prints "<head> <sha256>" of the Mac's checkout and w2w
  mac "h=\$(git -C Projects/nanofixengine rev-parse HEAD 2>/dev/null || echo unknown); \
s=\$(shasum -a 256 $GENERATOR_W2W 2>/dev/null | cut -d' ' -f1); echo \"\$h \${s:-unknown}\""
}

# ------------------------------------------------------------------- build
if [ "$SUB" = build ]; then
  COMMIT=$(git rev-parse --verify "$COMMIT^{commit}") || die "COMMIT does not name a commit"
  flags=$(scripts/check-bench-alignment.sh --flags) || die "check-bench-alignment.sh --flags failed"
  vendor_src=$(readlink -f vendor) || die "no vendor/ here — scripts/fetch-quickfix-assets.sh"
  [ -d "$vendor_src/quickfix" ] || die "$vendor_src has no quickfix/ — scripts/fetch-quickfix-assets.sh"
  echo "build: commit $COMMIT into $BOOT_ROOT, RUSTFLAGS '$flags'"
  # Each tree gets a `vendor` SYMLINK below, and `.gitignore`'s `/vendor/`
  # matches only a directory — so every arm's header read `tree 1 paths:
  # vendor` (`[measured 2026-09-24]`, the rehearsal). `/vendor` in the shared
  # info/exclude matches the link in every worktree of this repository and
  # changes nothing for the real directory, which `.gitignore` already hides.
  exclude="$(git rev-parse --path-format=absolute --git-common-dir)/info/exclude"
  mkdir -p "$(dirname "$exclude")"
  grep -qxF /vendor "$exclude" 2>/dev/null || echo /vendor >>"$exclude" || die "cannot write $exclude"
  for set in uring sqlite; do
    tree=$BOOT_ROOT/$set
    if [ -e "$tree" ]; then
      have=$(git -C "$tree" rev-parse HEAD 2>/dev/null || echo none)
      [ "$have" = "$COMMIT" ] || die "$tree exists at $have, not $COMMIT — remove it (git worktree remove) or pick another BOOT_ROOT"
      echo "build: $tree already at $COMMIT, reused"
    else
      git worktree add -q --detach "$tree" "$COMMIT" || die "git worktree add $tree failed"
    fi
    [ -e "$tree/vendor" ] || ln -s "$vendor_src" "$tree/vendor"
    [ -z "$(git -C "$tree" status --porcelain)" ] ||
      die "$tree is not clean after the vendor link: $(git -C "$tree" status --porcelain | tr '\n' ' ')"
    case $set in
      uring) feats=affinity,io-uring ;;
      sqlite) feats=affinity,sqlite ;;
    esac
    echo "build: $set — cargo build --release -p fixbolt-w2w --features $feats"
    (cd "$tree" && RUSTFLAGS="$flags" cargo build -q --release -p fixbolt-w2w --features "$feats") ||
      die "$set: cargo build failed"
    # The NIC tap (`--wire-timestamps`) opens AF_PACKET, which needs the file
    # capability; a fresh build has none, and w2w refuses to run without it
    # (`[measured 2026-09-24]` the first rehearsal: every NIC-stamped arm
    # failed "socket(AF_PACKET): Operation not permitted"). An xattr, so the
    # sha256 in MANIFEST.txt does not move; a rebuild drops it, which `run`
    # checks with getcap. ADR-0093: the command word is a path, not a name.
    sudo -n /usr/sbin/setcap cap_net_raw,cap_net_admin+ep "$tree/target/release/w2w" ||
      die "$set: setcap on $tree/target/release/w2w failed — root without a password is needed for the capability"
    has_caps "$tree/target/release/w2w" || die "$set: getcap does not read the capability back"
  done
  echo "build: uring — cargo bench -p fixbolt-engine --features io-uring --bench turn --bench density --no-run"
  bench_json=$(cd "$BOOT_ROOT/uring" && RUSTFLAGS="$flags" cargo bench -q -p fixbolt-engine \
    --features io-uring --bench turn --bench density --no-run --message-format=json) ||
    die "uring: cargo bench --no-run failed"
  exe_of() { printf '%s\n' "$bench_json" | jq -r --arg n "$1" \
    'select(.executable != null and .target.name == $n and (.target.kind[]? == "bench")) | .executable' | tail -1; }
  turn=$(exe_of turn)
  density=$(exe_of density)
  [ -x "$turn" ] && [ -x "$density" ] || die "cargo named no turn/density executable"
  (cd / && sha256sum "$BOOT_ROOT/uring/target/release/w2w" "$BOOT_ROOT/sqlite/target/release/w2w" \
    "$turn" "$density") >"$BOOT_ROOT/MANIFEST.txt" || die "sha256sum failed"
  if mid=$(mac_identity 2>/dev/null) && [ -n "$mid" ]; then
    read -r mac_head mac_sha <<<"$mid"
  else
    mac_head=unreachable
    mac_sha=unreachable
  fi
  {
    echo "commit $COMMIT"
    echo "built_at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "rustflags $flags"
    echo "rustc $(rustc -V)"
    echo "features_uring affinity,io-uring (w2w); io-uring (fixbolt-engine benches)"
    echo "features_sqlite affinity,sqlite (w2w)"
    echo "turn $turn"
    echo "density $density"
    echo "mac_head $mac_head"
    echo "mac_w2w_sha256 $mac_sha"
  } >"$BOOT_ROOT/BUILD-INFO.txt"
  cat "$BOOT_ROOT/MANIFEST.txt" "$BOOT_ROOT/BUILD-INFO.txt"
  if [ "$mac_head" != "$COMMIT" ]; then
    echo "build: WARNING — the Mac's checkout is at $mac_head, not $COMMIT. Build it there (header),"
    echo "       then run 'scripts/boot-p4.sh build' again; 'run' refuses this outside a rehearsal."
  fi
  exit 0
fi
[ "$SUB" = run ] || die "unknown subcommand '$SUB' — build or run"

# --------------------------------------------------------------- refusals
refuse() { echo "boot-p4: refused before running: $*" >&2; exit 2; }
if [ "$REHEARSAL" != 1 ]; then
  [ -z "$TOLERATE_ROWS" ] || refuse "TOLERATE_ROWS is a rehearsal knob — the boot stops on every red row"
  [ "$ALLOW_UNISOLATED" = 0 ] || refuse "ALLOW_UNISOLATED=1 is a rehearsal knob — a §9 figure is taken on isolated cores"
  [ "$MIN_GAP_S" -ge "$MIN_REAL_GAP_S" ] || refuse "MIN_GAP_S=$MIN_GAP_S — ADR-0068 decision 1 needs >= $MIN_REAL_GAP_S"
  [ "$RUNS" -ge "$MIN_REAL_RUNS" ] || refuse "RUNS=$RUNS — ADR-0190 decision 10 needs >= $MIN_REAL_RUNS"
  [ "$MESSAGES" = "$REAL_MESSAGES" ] || refuse "MESSAGES=$MESSAGES — ADR-0190 decision 10 measures $REAL_MESSAGES requests per run"
fi
[ "$ALLOW_UNISOLATED" = 0 ] || [ "$ALLOW_UNISOLATED" = 1 ] || refuse "ALLOW_UNISOLATED must be 0 or 1"
[ -r "$BOOT_ROOT/MANIFEST.txt" ] || refuse "no $BOOT_ROOT/MANIFEST.txt — run 'scripts/boot-p4.sh build' first"
[ -r "$BOOT_ROOT/BUILD-INFO.txt" ] || refuse "no $BOOT_ROOT/BUILD-INFO.txt — run 'scripts/boot-p4.sh build' first"
info() { awk -v k="$1" '$1 == k { sub(/^[^ ]+ /, ""); print; exit }' "$BOOT_ROOT/BUILD-INFO.txt"; }
BUILD_COMMIT=$(info commit)
W2W_URING=$BOOT_ROOT/uring/target/release/w2w
W2W_SQLITE=$BOOT_ROOT/sqlite/target/release/w2w
BENCH_TURN=$(info turn)
BENCH_DENSITY=$(info density)
manifest_sha() { # manifest_sha <path>
  awk -v p="$1" '{ f = $0; sub(/^[0-9a-f]+ [ *]/, "", f); if (f == p) { print $1; exit } }' "$BOOT_ROOT/MANIFEST.txt"
}
for b in "$W2W_URING" "$W2W_SQLITE" "$BENCH_TURN" "$BENCH_DENSITY"; do
  [ -n "$(manifest_sha "$b")" ] || refuse "MANIFEST.txt has no line for $b"
done
for b in "$W2W_URING" "$W2W_SQLITE"; do
  has_caps "$b" || refuse "$b has no cap_net_raw,cap_net_admin+ep in force (getcap, and not on a nosuid mount such as /tmp) — the NIC tap needs it; 'scripts/boot-p4.sh build' on a normal mount"
done
tolerated() { # tolerated <row name>
  local IFS=,
  local t
  for t in $TOLERATE_ROWS; do [ "$t" = "$1" ] && return 0; done
  return 1
}

# ADR-0068 decision 1 wants every arm from a clean tree, and w2w-baseline.sh
# prints `tree N paths: …` into each arm's header: a boot whose trees are not
# clean publishes that line beside every figure.
for set in uring sqlite; do
  dirty=$(git -C "$BOOT_ROOT/$set" status --porcelain 2>&1)
  [ -z "$dirty" ] && continue
  if [ "$REHEARSAL" = 1 ]; then
    echo "!!! $BOOT_ROOT/$set is not clean ($(printf '%s' "$dirty" | tr '\n' ' ')) — REHEARSAL only"
  else
    refuse "$BOOT_ROOT/$set is not clean: $(printf '%s' "$dirty" | tr '\n' ' ')— 'scripts/boot-p4.sh build' excludes /vendor"
  fi
done
# The boot measures what `main` holds, nothing a branch still carries.
if [ "$REHEARSAL" != 1 ]; then
  git merge-base --is-ancestor "$BUILD_COMMIT" origin/main 2>/dev/null ||
    refuse "BUILD-INFO.txt's commit $BUILD_COMMIT is not an ancestor of origin/main (git fetch, then build at the 7a merge commit)"
fi

# Timers (ADR-0093 decision 3). `check-machine.sh`'s `no timer due` row looks
# 12 h ahead from each gate's OWN time, and the boot's last gate is hours after
# its first: a timer due at 00:05 is outside the first gate's window for a boot
# started at 11:00 and inside a later one's, and the boot then stops on healthy
# arms. So every timer due before the LAST gate's window closes is refused
# here, before anything runs — 12 h plus the boot's expected length. The length
# is estimated from the rehearsal's own clock (`[measured 2026-09-24]`, desktop
# line, RUNS=2: ~33 s an arm, i.e. ~12.5 s a run plus ~8 s; turn + density
# ~450 s a procedure), then taken 1.5 times for margin. System and user
# managers both, since a user timer loads the desk as much as a system one.
TIMER_ROW_WINDOW_S=$((12 * 3600))
ARMS_PER_PROCEDURE=10
BOOT_EXPECTED_S=$((2 * (ARMS_PER_PROCEDURE * (RUNS * 15 + 30) + 600) + MIN_GAP_S))
TIMER_WINDOW_S=$((TIMER_ROW_WINDOW_S + BOOT_EXPECTED_S * 3 / 2))
timer_check() { # timer_check <system|user> — prints the verdict line of timers_verdict
  local json
  if [ "$1" = user ]; then
    json=$(systemctl --user list-timers --all --output=json 2>/dev/null) || json='[]'
  else
    json=$(systemctl list-timers --all --output=json 2>/dev/null) || json='null'
  fi
  # The row's own pure function (check-machine.sh `timers_verdict`), so the
  # driver and the gate cannot disagree on what "due" means; a subshell, so
  # none of that script's globals leak into this one.
  (
    # shellcheck source=check-machine.sh disable=SC1091
    MACHINE_SOURCE_ONLY=1 . scripts/check-machine.sh
    timers_verdict "$(($(date +%s) * 1000000))" "$TIMER_WINDOW_S" "$json"
  )
}
timer_bad=""
timer_fix=""
for mgr in system user; do
  IFS=$'\t' read -r tv_verdict tv_value tv_fix <<<"$(timer_check "$mgr")"
  case "$tv_verdict" in
    PASS) ;;
    FAIL)
      timer_bad="$timer_bad${timer_bad:+; }$mgr: $tv_value"
      if [ "$mgr" = user ]; then tv_fix=${tv_fix//sudo -n systemctl stop/systemctl --user stop}; fi
      timer_fix="$timer_fix${timer_fix:+; }$tv_fix"
      ;;
    *) timer_bad="$timer_bad${timer_bad:+; }$mgr: cannot read the timers ($tv_value)" ;;
  esac
done
if [ -n "$timer_bad" ]; then
  if tolerated "no timer due"; then
    echo "!!! timers due within $((TIMER_WINDOW_S / 60)) min (12 h + 1.5 × the expected ${BOOT_EXPECTED_S} s), tolerated by TOLERATE_ROWS — REHEARSAL only"
  else
    refuse "a timer is due before the boot's last gate closes its 12 h window ($((TIMER_WINDOW_S / 60)) min from now = 12 h + 1.5 × the expected ${BOOT_EXPECTED_S} s): $timer_bad — stop it first: $timer_fix"
  fi
fi

# ---------------------------------------------------------------- evidence
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
LABEL=""
[ "$REHEARSAL" = 1 ] && LABEL="-REHEARSAL"
EVD=$REPO/target/boot-p4-evidence/$STAMP$LABEL
mkdir -p "$EVD" || die "cannot create $EVD"
exec </dev/null >  >(tee -a "$EVD/driver.log") 2>&1
NOT_A_FIGURE=""
[ "$REHEARSAL" = 1 ] && NOT_A_FIGURE="  [REHEARSAL — NOT A MEASUREMENT]"

SUMMARY=()
note() { SUMMARY+=("$*$NOT_A_FIGURE"); echo "== $*$NOT_A_FIGURE"; }
FAILED_ARMS=()
finish() { # finish <exit status>
  {
    echo "== boot-p4 summary$NOT_A_FIGURE =="
    echo "evidence  $EVD"
    printf '%s\n' "${SUMMARY[@]}"
    if [ "${#FAILED_ARMS[@]}" -gt 0 ]; then
      echo "failed arms: ${FAILED_ARMS[*]}"
    fi
    echo "exit $1"
  } | tee "$EVD/summary.txt"
  exit "$1"
}
stop() { # stop <reason> — the boot cannot go on
  note "STOPPED: $*"
  finish 3
}

banner() {
  if [ "$REHEARSAL" = 1 ]; then
    echo "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
    echo "!!! REHEARSAL — NOT A MEASUREMENT. No number below is a figure, anywhere.  !!!"
    [ -n "$TOLERATE_ROWS" ] && echo "!!! TOLERATE_ROWS: $TOLERATE_ROWS"
    [ "$ALLOW_UNISOLATED" = 1 ] && echo "!!! ALLOW_UNISOLATED=1 — cores are not isolated"
    [ "$MIN_GAP_S" -lt "$MIN_REAL_GAP_S" ] && echo "!!! MIN_GAP_S=$MIN_GAP_S — ADR-0068 decision 1 needs $MIN_REAL_GAP_S; REHEARSAL ONLY"
    [ "$RUNS" -lt "$MIN_REAL_RUNS" ] && echo "!!! RUNS=$RUNS — the boot runs >= $MIN_REAL_RUNS"
    echo "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
  fi
}
banner

# ----------------------------------------------------------- pre-flight
# ADR-0202 decision 4: no foreign module in the kernel every figure is taken on.
if lsmod | grep -qE '^(onload|sfc_resource) '; then
  stop "an onload/sfc_resource module is loaded (ADR-0202 decision 4)"
fi
[ ! -e /etc/modprobe.d/onload.conf ] || stop "/etc/modprobe.d/onload.conf exists (ADR-0202 decision 4)"
CMDLINE=$(cat /proc/cmdline)
if [ "$REHEARSAL" != 1 ]; then
  case " $CMDLINE " in
    *" isolcpus=6,7,14,15 "*) ;;
    *) stop "/proc/cmdline has no 'isolcpus=6,7,14,15' — not the §9 grub line (plan 7b step 2)" ;;
  esac
  case "$CMDLINE" in
    *nohz_full*) stop "/proc/cmdline carries nohz_full — the wrong §9 line (ADR-0202 decision 5)" ;;
  esac
fi
for set in uring sqlite; do
  have=$(git -C "$BOOT_ROOT/$set" rev-parse HEAD 2>/dev/null || echo none)
  [ "$have" = "$BUILD_COMMIT" ] || stop "$BOOT_ROOT/$set is at $have, BUILD-INFO.txt says $BUILD_COMMIT"
done

carrier_ok() { [ "$(cat "/sys/class/net/$NIC/carrier" 2>/dev/null)" = 1 ]; }
carrier_ok || stop "$NIC has no carrier — the cable to the generator"
MAC_ID=$(mac_identity 2>/dev/null) || stop "the generator $GENERATOR_SSH does not answer ssh -o BatchMode=yes (FileVault: the owner logs in on the Mac)"
read -r MAC_HEAD MAC_SHA <<<"$MAC_ID"
[ "$MAC_SHA" != unknown ] || stop "no $GENERATOR_W2W on $GENERATOR_SSH"
if [ "$MAC_HEAD" != "$BUILD_COMMIT" ] || [ "$MAC_SHA" != "$(info mac_w2w_sha256)" ]; then
  if [ "$REHEARSAL" = 1 ]; then
    echo "!!! the Mac's checkout is $MAC_HEAD (w2w ${MAC_SHA:0:12}); BUILD-INFO.txt says $BUILD_COMMIT"
    echo "!!! (w2w $(info mac_w2w_sha256 | cut -c1-12)) — REHEARSAL: held to what was read now instead"
  else
    stop "the Mac is at $MAC_HEAD with w2w ${MAC_SHA:0:12}; BUILD-INFO.txt says $BUILD_COMMIT / $(info mac_w2w_sha256 | cut -c1-12) — build it at the boot commit (header)"
  fi
fi

{
  echo "stamp $STAMP"
  echo "rehearsal $REHEARSAL"
  for v in BOOT_ROOT BUILD_COMMIT NIC LISTEN GENERATOR_SSH GENERATOR_W2W RUNS MESSAGES MIN_GAP_S \
    TOLERATE_ROWS ALLOW_UNISOLATED QUIET_RETRIES QUIET_RETRY_S ARM_TIMEOUT_S BENCH_TIMEOUT_S \
    ENGINE_CORE OBSERVER_CORE_A OBSERVER_CORE_S SQPOLL_CORE; do
    echo "$v ${!v}"
  done
  echo "driver_head $(git rev-parse HEAD)"
  echo "driver_sha256 $(sha256sum "$REPO/scripts/boot-p4.sh" | cut -d' ' -f1)"
  if [ -z "$(git status --porcelain -- scripts/boot-p4.sh)" ] &&
    git cat-file -e "HEAD:scripts/boot-p4.sh" 2>/dev/null; then
    echo "driver_committed yes (HEAD:scripts/boot-p4.sh is this file)"
  else
    echo "driver_committed NO — the running driver differs from HEAD's or is untracked"
  fi
  echo "timer_window_s $TIMER_WINDOW_S (12 h + 1.5 × expected $BOOT_EXPECTED_S s)"
  echo "w2w_baseline PIN=$W2W_PIN WARMUP=$W2W_WARMUP GAP=$W2W_GAP CLIENT_CORE=$W2W_CLIENT_CORE"
  echo "mac_head $MAC_HEAD"
  echo "mac_w2w_sha256 $MAC_SHA"
  echo "cmdline $CMDLINE"
  echo "uname $(uname -r)"
} >"$EVD/settings.txt"
cat "$EVD/settings.txt"

manifest_check() { # manifest_check <file>
  (cd / && sha256sum -c "$BOOT_ROOT/MANIFEST.txt") >"$1" 2>&1
  local rc=$? n ok
  n=$(wc -l <"$BOOT_ROOT/MANIFEST.txt")
  ok=$(grep -c ': OK$' "$1")
  [ "$rc" -eq 0 ] && [ "$ok" -eq "$n" ] && { echo "OK ($ok of $n)"; return 0; }
  echo "NOT OK ($ok of $n)"
  return 1
}
if m=$(manifest_check "$EVD/manifest-start.txt"); then
  note "manifest  start $m"
else
  stop "sha256sum -c MANIFEST.txt at the start: $m — see manifest-start.txt"
fi
note "generator ${MAC_SHA:0:12} at ${MAC_HEAD:0:12} on $GENERATOR_SSH — held through every arm"

# ---------------------------------------------------------- machine gate
# The rows check-machine.sh prints red: `FAIL ` and `? ? ?` marks, the name in
# the 22-wide field after them (check-machine.sh `row()`).
red_rows() { # red_rows <check-machine output file>
  awk '/^(FAIL |\? \? \?)  / { n = substr($0, 8, 22); sub(/ +$/, "", n); print n }' "$1"
}
gate() { # gate <label> <dir>
  local label=$1 dir=$2 attempt=0 f rows bad name tol
  mkdir -p "$dir"
  while :; do
    f=$dir/gate-check-machine.txt
    [ "$attempt" -gt 0 ] && f=$dir/gate-check-machine-retry$attempt.txt
    FIXBOLT_NIC=$NIC scripts/check-machine.sh >"$f" 2>&1
    grep -qE '^pass [0-9]+ +fail [0-9]+ +unknown [0-9]+' "$f" ||
      stop "before $label: check-machine.sh printed no summary line — see $f"
    rows=$(red_rows "$f")
    bad=()
    tol=()
    while IFS= read -r name; do
      [ -n "$name" ] || continue
      if tolerated "$name"; then tol+=("$name"); else bad+=("$name"); fi
    done <<<"$rows"
    if [ "${#bad[@]}" -eq 0 ]; then
      local t=""
      [ "${#tol[@]}" -gt 0 ] && t="   (red, tolerated by TOLERATE_ROWS: $(IFS=,; echo "${tol[*]}"))"
      note "gate $label: $(grep -E '^pass [0-9]+' "$f")$t"
      return 0
    fi
    if [ "${#bad[@]}" -eq 1 ] && [ "${bad[0]}" = "machine is quiet" ] && [ "$attempt" -lt "$QUIET_RETRIES" ]; then
      attempt=$((attempt + 1))
      echo "gate $label: only 'machine is quiet' is red — re-read $attempt of $QUIET_RETRIES in ${QUIET_RETRY_S}s"
      sleep "$QUIET_RETRY_S"
      continue
    fi
    grep -E '^(FAIL |\? \? \?)  ' "$f"
    stop "before $label: check-machine.sh red row(s): $(IFS=,; echo "${bad[*]}") — see $f"
  done
}

# ---------------------------------------------------------- identity
bin_ok() { # bin_ok <path> — sha256 is MANIFEST.txt's
  local want have
  want=$(manifest_sha "$1")
  have=$(sha256sum "$1" 2>/dev/null | cut -d' ' -f1)
  [ -n "$want" ] && [ "$have" = "$want" ]
}
generator_ok() {
  carrier_ok || stop "$NIC lost carrier — the cable to the generator"
  local id
  id=$(mac_identity 2>/dev/null) || stop "the generator stopped answering ssh -o BatchMode=yes (FileVault: the owner logs in on the Mac)"
  [ "$id" = "$MAC_HEAD $MAC_SHA" ] || stop "the generator changed during the boot: '$id', was '$MAC_HEAD $MAC_SHA'"
}

# ---------------------------------------------------------- arms
# arm <id> → "set|ARMS|wire(0/1)|observer|W2W_EXTRA|transport regex"
# Every uring arm also requires `unarmed=0 cq-overflow=0 enter-errors=0` on
# its `transport:` line: ADR-0190 Revision 3 makes an unarmed receive a
# failure, and an overflowed CQ or a failed enter is a ring that did not run
# the arm it is labelled with.
# No '|' in it: arm_spec's fields are split on '|'.
readonly URING_CLEAN=' unarmed=0 cq-overflow=0 enter-errors=0( .*)?$'
arm_spec() {
  case "$1" in
    K) echo "uring|hft:admin|1|$OBSERVER_CORE_A||^transport: kernel\$" ;;
    U) echo "uring|hft:admin|1|$OBSERVER_CORE_A|--transport uring|^transport: uring arm=enter cqes=[1-9][0-9]* .*$URING_CLEAN" ;;
    Uprime) echo "uring|hft:admin|1|$OBSERVER_CORE_S|--transport uring|^transport: uring arm=enter cqes=[1-9][0-9]* .*$URING_CLEAN" ;;
    S) echo "uring|hft:admin|1|$OBSERVER_CORE_S|--transport uring --uring-arm sqpoll --sqpoll-core $SQPOLL_CORE|^transport: uring arm=sqpoll cqes=[1-9][0-9]* .*$URING_CLEAN" ;;
    stdK) echo "uring|standard:admin|0|||^transport: kernel\$" ;;
    # `standard` reaps its ring by blocking: w2w prints `arm=block` there
    # (`[measured 2026-09-24]` the rehearsal's first stdU read it), never `enter`.
    stdU) echo "uring|standard:admin|0||--transport uring|^transport: uring arm=block cqes=[1-9][0-9]* .*$URING_CLEAN" ;;
    hft-file) echo "sqlite|hft:app|1|$OBSERVER_CORE_A|--journal file-async|^transport: kernel\$" ;;
    hft-sqlite) echo "sqlite|hft:app|1|$OBSERVER_CORE_A|--journal sqlite-async|^transport: kernel\$" ;;
    std-file) echo "sqlite|standard:app|0||--journal file-async|^transport: kernel\$" ;;
    std-sqlite) echo "sqlite|standard:app|0||--journal sqlite-async|^transport: kernel\$" ;;
    *) return 1 ;;
  esac
}

run_arm() { # run_arm <procedure> <block dir> <arm id>
  local p=$1 bdir=$2 id=$3 spec set arms wire obs extra tre bin dir rc sha12 gsha q skipped n bad f
  spec=$(arm_spec "$id") || die "no arm '$id'"
  IFS='|' read -r set arms wire obs extra tre <<<"$spec"
  bin=$BOOT_ROOT/$set/target/release/w2w
  dir=$bdir/$id
  mkdir -p "$dir"
  bin_ok "$bin" || stop "p$p $id: $bin is not MANIFEST.txt's before the arm"
  has_caps "$bin" || stop "p$p $id: $bin lost its cap_net_raw,cap_net_admin file capability"
  generator_ok
  echo "---- p$p $id: ARMS=$arms W2W_EXTRA='$extra' wire=$wire observer=${obs:--} ($set build) $(date -u +%H:%M:%SZ)"
  local wire_nic="" obs_core=""
  if [ "$wire" = 1 ]; then wire_nic=$NIC; obs_core=$obs; fi
  RUNS=$RUNS MESSAGES=$MESSAGES ENGINE_CORE=$ENGINE_CORE ALLOW_UNISOLATED=$ALLOW_UNISOLATED \
    PIN=$W2W_PIN WARMUP=$W2W_WARMUP GAP=$W2W_GAP CLIENT_CORE=$W2W_CLIENT_CORE \
    LISTEN=$LISTEN GENERATOR_SSH=$GENERATOR_SSH GENERATOR_W2W=$GENERATOR_W2W \
    FIXBOLT_NIC=$NIC WIRE_NIC=$wire_nic OBSERVER_CORE=$obs_core \
    ARMS=$arms W2W_EXTRA=$extra OUT_DIR=$dir \
    timeout --kill-after=30 "$ARM_TIMEOUT_S" "$BOOT_ROOT/$set/scripts/w2w-baseline.sh" >"$dir/baseline.log" 2>&1
  rc=$?
  bin_ok "$bin" || stop "p$p $id: $bin is not MANIFEST.txt's after the arm"
  has_caps "$bin" || stop "p$p $id: $bin lost its cap_net_raw,cap_net_admin file capability during the arm"
  generator_ok
  sha12=$(manifest_sha "$bin" | cut -c1-12)
  grep -q "^binary $sha12 " "$dir/baseline.log" ||
    stop "p$p $id: the arm's own 'binary' line is not $sha12 — see $dir/baseline.log"
  gsha=$(awk '$1 == "generator" && $2 == "binary" { print $3; exit }' "$dir/baseline.log")
  [ "$gsha" = "${MAC_SHA:0:12}" ] ||
    stop "p$p $id: the arm's 'generator binary' line reads '${gsha:-none}', not ${MAC_SHA:0:12}"
  bad=""
  if [ "$rc" -ne 0 ]; then
    bad="w2w-baseline.sh exit $rc$([ "$rc" = 124 ] && echo " (timeout ${ARM_TIMEOUT_S}s)")"
  else
    n=0
    for f in "$dir"/*-run-*-listen.txt; do
      [ -e "$f" ] || continue
      n=$((n + 1))
      grep -qE "$tre" "$f" || { bad="$f has no line matching '$tre'"; break; }
    done
    [ -n "$bad" ] || [ "$n" -gt 0 ] || bad="no listen output in $dir"
    if [ -z "$bad" ] && ! grep -qE '^ *p50 ' "$dir/summary.txt" 2>/dev/null; then
      bad="summary.txt has no p50 — no qualifying run"
    fi
  fi
  q=$(awk '/^  == .*median of/ { for (i = 1; i <= NF; i++) if ($i == "of") { print $(i+1); exit } }' "$dir/summary.txt" 2>/dev/null)
  skipped=$(sed -nE 's/.*\(([0-9]+) disqualified\).*/\1/p' "$dir/summary.txt" 2>/dev/null | head -1)
  if [ -n "$bad" ]; then
    FAILED_ARMS+=("p$p/$id")
    note "arm  p$p $(basename "$bdir") $id: FAILED — $bad (see $dir/baseline.log)"
    return 0
  fi
  note "arm  p$p $(basename "$bdir") $id: OK — ${q:-?} of $RUNS runs qualified (${skipped:-?} disqualified); w2w $sha12, generator $gsha, transport /$tre/"
}

run_bench() { # run_bench <procedure> <block dir> <turn|density>
  local p=$1 bdir=$2 id=$3 bin rc out missing n kind
  case $id in
    turn) bin=$BENCH_TURN; kind="idle loop, %s idle sessions, %s" ;;
    density) bin=$BENCH_DENSITY; kind="busy loop, %s busy sessions, %s" ;;
  esac
  mkdir -p "$bdir"
  out=$bdir/$id.txt
  bin_ok "$bin" || stop "p$p $id: $bin is not MANIFEST.txt's before the bench"
  echo "---- p$p bench $id: taskset -c $ENGINE_CORE FIXBOLT_BENCH_COUNT_ONLY=1 $bin $(date -u +%H:%M:%SZ)"
  {
    echo "# $(date -u +%Y-%m-%dT%H:%M:%SZ) taskset -c $ENGINE_CORE env FIXBOLT_BENCH_COUNT_ONLY=1 $bin"
    echo "# sha256 $(manifest_sha "$bin")$NOT_A_FIGURE"
  } >"$out"
  timeout --kill-after=30 "$BENCH_TIMEOUT_S" taskset -c "$ENGINE_CORE" \
    env FIXBOLT_BENCH_COUNT_ONLY=1 "$bin" >>"$out" 2>&1
  rc=$?
  bin_ok "$bin" || stop "p$p $id: $bin is not MANIFEST.txt's after the bench"
  missing=""
  for n in 1 16 64; do
    for t in kernel uring; do
      # shellcheck disable=SC2059 # the format is the case name's own shape
      name=$(printf "$kind" "$n" "$t")
      [ -n "$(bench_ns "$out" "$name")" ] || missing="$missing '$name'"
    done
  done
  if [ "$rc" -ne 0 ] || [ -n "$missing" ]; then
    FAILED_ARMS+=("p$p/$id")
    note "bench p$p $id: FAILED — exit $rc${missing:+, no line for$missing} (see $out)"
    return 0
  fi
  note "bench p$p $id: OK — six paired cases printed (N = 1, 16, 64 × kernel, uring); binary $(manifest_sha "$bin" | cut -c1-12), pinned cpu$ENGINE_CORE"
}
bench_ns() { # bench_ns <file> <case name> — the ns/op figure of that exact case
  awk -v n="$2" 'index($0, n " ") == 1 { for (i = 1; i <= NF; i++) if ($i == "ns/op") { print $(i - 1); exit } }' "$1"
}

BLOCK_A=(K U Uprime S stdK stdU)
BLOCK_B=(turn density)
BLOCK_C=(hft-file hft-sqlite std-file std-sqlite)
reversed() { local i; for ((i = $# ; i > 0; i--)); do printf '%s\n' "${!i}"; done; }

run_block() { # run_block <procedure> <A|B|C> <reverse 0|1>
  local p=$1 b=$2 rev=$3 bdir ids id
  case $b in
    A) bdir=$EVD/p$p/A-uring-w2w; ids=("${BLOCK_A[@]}") ;;
    B) bdir=$EVD/p$p/B-bench; ids=("${BLOCK_B[@]}") ;;
    C) bdir=$EVD/p$p/C-store; ids=("${BLOCK_C[@]}") ;;
  esac
  [ "$rev" = 1 ] && mapfile -t ids < <(reversed "${ids[@]}")
  gate "p$p $b ($(basename "$bdir"))" "$bdir"
  for id in "${ids[@]}"; do
    if [ "$b" = B ]; then run_bench "$p" "$bdir" "$id"; else run_arm "$p" "$bdir" "$id"; fi
  done
}

# ------------------------------------------------------------- the boot
note "procedure 1 start $(date -u +%Y-%m-%dT%H:%M:%SZ) — blocks A B C, arms in written order"
run_block 1 A 0
run_block 1 B 0
run_block 1 C 0
P1_END=$(date +%s)
P2_NOT_BEFORE=$((P1_END + MIN_GAP_S))
echo "procedure 1 ended $(date -u -d "@$P1_END" +%Y-%m-%dT%H:%M:%SZ); procedure 2 starts no earlier than $(date -u -d "@$P2_NOT_BEFORE" +%Y-%m-%dT%H:%M:%SZ) (MIN_GAP_S $MIN_GAP_S)"
while now=$(date +%s) && [ "$now" -lt "$P2_NOT_BEFORE" ]; do
  left=$((P2_NOT_BEFORE - now))
  sleep $((left < 60 ? left : 60))
done
P2_START=$(date +%s)
GAP_S=$((P2_START - P1_END))
{
  echo "p1_end $(date -u -d "@$P1_END" +%Y-%m-%dT%H:%M:%SZ) ($P1_END)"
  echo "p2_start $(date -u -d "@$P2_START" +%Y-%m-%dT%H:%M:%SZ) ($P2_START)"
  echo "elapsed_s $GAP_S"
  echo "min_gap_s $MIN_GAP_S"
} >"$EVD/gap.txt"
[ "$GAP_S" -ge "$MIN_GAP_S" ] || stop "the clock says $GAP_S s between procedures, fewer than $MIN_GAP_S"
note "wait      $GAP_S s by the clock between procedures (need >= $MIN_GAP_S; ADR-0068 decision 1 needs $MIN_REAL_GAP_S)"
note "procedure 2 start $(date -u +%Y-%m-%dT%H:%M:%SZ) — blocks C B A, arms reversed"
run_block 2 C 1
run_block 2 B 1
run_block 2 A 1

if m=$(manifest_check "$EVD/manifest-end.txt"); then
  note "manifest  end $m"
else
  stop "sha256sum -c MANIFEST.txt at the end: $m — see manifest-end.txt"
fi
FINAL_OK=1
FIXBOLT_NIC=$NIC scripts/check-machine.sh >"$EVD/final-check-machine.txt" 2>&1
final_bad=()
while IFS= read -r name; do
  [ -n "$name" ] || continue
  tolerated "$name" || final_bad+=("$name")
done < <(red_rows "$EVD/final-check-machine.txt")
if [ "${#final_bad[@]}" -gt 0 ]; then
  FINAL_OK=0
  note "final check-machine: RED — $(IFS=,; echo "${final_bad[*]}") (see final-check-machine.txt)"
else
  note "final check-machine: $(grep -E '^pass [0-9]+' "$EVD/final-check-machine.txt")"
fi

# ------------------------------------------------------- verdict inputs
mkdir -p "$EVD/compare"
cmp() { # cmp <name> <summary 1> <summary 2>
  scripts/compare-w2w-procedures.sh "$2" "$3" >"$EVD/compare/$1.txt" 2>&1
  echo "compare $1: exit $? — compare/$1.txt"
}
{
  for id in "${BLOCK_A[@]}" "${BLOCK_C[@]}"; do
    blk=A-uring-w2w
    case $id in hft-* | std-*) blk=C-store ;; esac
    cmp "$id-p1-vs-p2" "$EVD/p1/$blk/$id/summary.txt" "$EVD/p2/$blk/$id/summary.txt"
  done
  for p in 1 2; do
    a=$EVD/p$p/A-uring-w2w
    c=$EVD/p$p/C-store
    cmp "p$p-K-vs-U" "$a/K/summary.txt" "$a/U/summary.txt"
    cmp "p$p-Uprime-vs-S" "$a/Uprime/summary.txt" "$a/S/summary.txt"
    cmp "p$p-stdK-vs-stdU" "$a/stdK/summary.txt" "$a/stdU/summary.txt"
    cmp "p$p-hft-file-vs-sqlite" "$c/hft-file/summary.txt" "$c/hft-sqlite/summary.txt"
    cmp "p$p-std-file-vs-sqlite" "$c/std-file/summary.txt" "$c/std-sqlite/summary.txt"
  done
} >"$EVD/compare/index.txt"

declare -A R
sval() { # sval <summary> <wire|plain> <p50|p99|p99.9>
  if [ "$2" = wire ]; then
    awk -v k="$3" '$1 == "wire" && $2 == k { print $3; exit }' "$1" 2>/dev/null
  else
    awk -v k="$3" '$1 == k { print $2; exit }' "$1" 2>/dev/null
  fi
}
# A FAILED arm's summary may still carry numbers (a transport line that was
# not the arm's, a run that exited non-zero after printing): none of them is
# an input. `[measured 2026-09-24]` the 171428Z rehearsal fed two FAILED stdU
# arms into "standard half: no".
arm_failed() { # arm_failed <procedure> <id>
  local f
  for f in "${FAILED_ARMS[@]}"; do [ "$f" = "p$1/$2" ] && return 0; done
  return 1
}
aval() { # aval <procedure> <id> <summary> <wire|plain> <pct> — empty for a FAILED arm
  arm_failed "$1" "$2" && return 0
  sval "$3" "$4" "$5"
}
bval() { # bval <procedure> <turn|density> <file> <case> — empty for a FAILED bench
  arm_failed "$1" "$2" && return 0
  bench_ns "$3" "$4"
}
# Ratios are kept unrounded for every comparison; `show` rounds for the eye only.
show() { # show <ratio>
  [ "$1" = missing ] && { echo missing; return; }
  awk -v r="$1" 'BEGIN { printf "%.4f", r }'
}
ratio() { # ratio <a> <b> — a/b unrounded, or "missing"
  if isnum "$1" || [[ $1 =~ ^[0-9]+\.[0-9]+$ ]]; then
    if isnum "$2" || [[ $2 =~ ^[0-9]+\.[0-9]+$ ]]; then
      awk -v a="$1" -v b="$2" 'BEGIN { if (b == 0) print "missing"; else printf "%.12g", a / b }'
      return
    fi
  fi
  echo missing
}
le() { # le <ratio> <bound> — yes / no / missing
  [ "$1" = missing ] && { echo missing; return; }
  awk -v r="$1" -v b="$2" 'BEGIN { print (r + 0 <= b + 0) ? "yes" : "no" }'
}
both() { # both <v1> <v2> — yes only when both are yes
  if [ "$1" = yes ] && [ "$2" = yes ]; then echo yes
  elif [ "$1" = missing ] || [ "$2" = missing ]; then echo missing
  else echo no; fi
}
{
  echo "# verdict inputs — $STAMP$NOT_A_FIGURE"
  echo "# Every value is read by scripts/boot-p4.sh from the file named beside it; every ratio is"
  echo "# computed here. The verdicts are applied in 7b.3 by plan row 5 (ADR-0190 decision 10) and"
  echo "# plan row 4 (4b), not by this file. Summaries: w2w-baseline.sh medians over qualifying runs."
  if [ "${#FAILED_ARMS[@]}" -gt 0 ]; then
    echo "# FAILED arms — any value below from them is not an input: ${FAILED_ARMS[*]}"
  else
    echo "# FAILED arms: none"
  fi
  echo
  echo "## io_uring — wire figures are the acceptor's ($NIC hardware stamps); std is the Mac's table"
  for p in 1 2; do
    a=$EVD/p$p/A-uring-w2w
    for id in K U Uprime S; do
      for k in p50 p99; do
        v=$(aval "$p" "$id" "$a/$id/summary.txt" wire "$k")
        R[$p.$id.$k]=${v:-missing}
        echo "p$p $id wire $k ${v:-missing}   $a/$id/summary.txt"
      done
    done
    for id in stdK stdU; do
      v=$(aval "$p" "$id" "$a/$id/summary.txt" plain p50)
      R[$p.$id.p50]=${v:-missing}
      echo "p$p $id counterparty p50 ${v:-missing}   $a/$id/summary.txt"
    done
    for t in kernel uring; do
      for n in 1 16 64; do
        v=$(bval "$p" turn "$EVD/p$p/B-bench/turn.txt" "idle loop, $n idle sessions, $t")
        R[$p.idle$n.$t]=${v:-missing}
        echo "p$p idle loop N=$n $t ${v:-missing} ns/op   $EVD/p$p/B-bench/turn.txt"
      done
    done
    for t in kernel uring; do
      for n in 1 16 64; do
        v=$(bval "$p" density "$EVD/p$p/B-bench/density.txt" "busy loop, $n busy sessions, $t")
        echo "p$p busy loop N=$n $t ${v:-missing} ns/op (recorded, not judged)   $EVD/p$p/B-bench/density.txt"
      done
    done
  done
  echo
  for p in 1 2; do
    R[$p.u50]=$(ratio "${R[$p.U.p50]}" "${R[$p.K.p50]}")
    R[$p.u99]=$(ratio "${R[$p.U.p99]}" "${R[$p.K.p99]}")
    R[$p.idle]=$(ratio "${R[$p.idle16.uring]}" "${R[$p.idle16.kernel]}")
    R[$p.s50]=$(ratio "${R[$p.S.p50]}" "${R[$p.Uprime.p50]}")
    R[$p.s99]=$(ratio "${R[$p.S.p99]}" "${R[$p.Uprime.p99]}")
    R[$p.std]=$(ratio "${R[$p.stdU.p50]}" "${R[$p.stdK.p50]}")
    echo "p$p U/K wire p50 $(show "${R[$p.u50]}")   U/K wire p99 $(show "${R[$p.u99]}")   idle N=16 uring/kernel $(show "${R[$p.idle]}")"
    echo "p$p S/U′ wire p50 $(show "${R[$p.s50]}")   S/U′ wire p99 $(show "${R[$p.s99]}")   (S cannot keep the item alone)"
    echo "p$p stdU/stdK counterparty p50 $(show "${R[$p.std]}")"
  done
  echo
  ca=$(both "$(both "$(le "${R[1.u50]}" 0.97)" "$(le "${R[2.u50]}" 0.97)")" \
    "$(both "$(le "${R[1.u99]}" 1.05)" "$(le "${R[2.u99]}" 1.05)")")
  cb=$(both "$(both "$(le "${R[1.idle]}" 0.75)" "$(le "${R[2.idle]}" 0.75)")" \
    "$(both "$(le "${R[1.u50]}" 1.05)" "$(le "${R[2.u50]}" 1.05)")")
  sa=$(both "$(both "$(le "${R[1.s50]}" 0.97)" "$(le "${R[2.s50]}" 0.97)")" \
    "$(both "$(le "${R[1.s99]}" 1.05)" "$(le "${R[2.s99]}" 1.05)")")
  sw1=$(le "${R[1.std]}" 1.05)
  sw2=$(le "${R[2.std]}" 1.05)
  swb=no
  [ "$sw1" = no ] && [ "$sw2" = no ] && swb=yes
  { [ "$sw1" = missing ] || [ "$sw2" = missing ]; } && swb=missing
  echo "clause (a) U/K p50 <= 0.97 and p99 <= 1.05, both procedures: $ca"
  echo "clause (b) idle N=16 uring/kernel <= 0.75 and U/K p50 <= 1.05, both procedures: $cb"
  echo "S beside U′, clause (a)'s arithmetic on S/U′ (cannot keep the item alone): $sa"
  echo "standard half: stdU/stdK p50 > 1.05 in both procedures (then UringBlock/serve_uring go): $swb"
  echo
  echo "## store pair (plan row 4, 4b) — hft: acceptor wire p50; standard: the Mac's p50; 5 % band"
  for p in 1 2; do
    c=$EVD/p$p/C-store
    hf=$(aval "$p" hft-file "$c/hft-file/summary.txt" wire p50)
    hs=$(aval "$p" hft-sqlite "$c/hft-sqlite/summary.txt" wire p50)
    sf=$(aval "$p" std-file "$c/std-file/summary.txt" plain p50)
    ss=$(aval "$p" std-sqlite "$c/std-sqlite/summary.txt" plain p50)
    echo "p$p hft-file wire p50 ${hf:-missing}   hft-sqlite wire p50 ${hs:-missing}   sqlite/file $(show "$(ratio "${hs:-x}" "${hf:-x}")")"
    echo "p$p std-file counterparty p50 ${sf:-missing}   std-sqlite counterparty p50 ${ss:-missing}   sqlite/file $(show "$(ratio "${ss:-x}" "${sf:-x}")")"
    echo "p$p band verdicts: compare/p$p-hft-file-vs-sqlite.txt, compare/p$p-std-file-vs-sqlite.txt"
  done
  echo "allocs: w2w-baseline.sh asserts 'allocs 0' on both halves of every run; an arm with a"
  echo "non-zero count exits non-zero and is listed FAILED in summary.txt."
} >"$EVD/verdict-inputs.txt"
note "verdict inputs  $EVD/verdict-inputs.txt; comparisons $EVD/compare/"

status=0
[ "${#FAILED_ARMS[@]}" -eq 0 ] || status=1
[ "$FINAL_OK" = 1 ] || status=1
finish "$status"
