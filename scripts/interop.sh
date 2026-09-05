#!/usr/bin/env bash
# Put this engine and a real `libquickfix` on opposite ends of a socket, in
# BOTH DIRECTIONS, and read what comes back.
#
#   * `interop:` lines           — this engine's INITIATOR into a C++ acceptor.
#                                  Phase 1 exit criterion 4.
#   * `interop-acceptor:` lines  — a C++ INITIATOR into this engine's acceptor.
#                                  `[2026-09-04]` The acceptor is the product
#                                  this repository is positioned on and had
#                                  never been driven by another implementation:
#                                  its whole evidence was 59 `.def` files read
#                                  by this repository's own runner.
#
# ONE SCRIPT, BOTH DIRECTIONS, ON PURPOSE. Two scripts are two `PINNED_SHA`s,
# and two pins that can drift apart make a disagreement between the directions
# unattributable — the same argument the pin check below already makes about
# the corpus and the counterparty.
#
# Builds QuickFIX from source at the SAME pinned commit
# `scripts/fetch-quickfix-assets.sh` uses, compiles `tools/interop/acceptor.cpp`
# and `tools/interop/initiator.cpp` against it, and READS THE OUTPUT of both.
#
# CLAUDE.md §10: "a check proves nothing until something reads it". This script
# greps for the `interop: PASS n/n` line and for every step name; a run that
# exits 0 having printed nothing is a failure here, which is the shape a bare
# `set -e` cannot see.
#
# Nothing this script fetches or builds enters the repository. vendor/ is
# gitignored — ADR-0001, CLAUDE.md §2 rule 9 — and the last thing this script
# does is check that `git status` is still clean.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="${REPO_ROOT}/vendor/quickfix-src"
WORK="${REPO_ROOT}/vendor/interop-run"

# The same commit scripts/fetch-quickfix-assets.sh pins. Two pins that can drift
# apart is one pin: the acceptance corpus and the C++ counterparty must be the
# same QuickFIX or a disagreement between them is unattributable.
PINNED_SHA="386ce46e917ae494ab6e90b1be90fd421cdbe3f9"   # 2026-05-20
declare -r EXPECT_SHA="$(grep -oE '^PINNED_SHA="[0-9a-f]{40}"' "${REPO_ROOT}/scripts/fetch-quickfix-assets.sh" | cut -d'"' -f2)"
if [[ "${PINNED_SHA}" != "${EXPECT_SHA}" ]]; then
  echo "PIN MISMATCH: this script pins ${PINNED_SHA}, fetch-quickfix-assets.sh pins ${EXPECT_SHA}" >&2
  echo "Two pins that disagree make a disagreement between the corpus and the counterparty" >&2
  echo "unattributable. Move both in one commit." >&2
  exit 1
fi

# Taken before anything is fetched or built, so step 5 can ask what this run
# added rather than whether the tree happened to be clean.
BEFORE="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | sort)"

PORT="${INTEROP_PORT:-15644}"
# A second port, because the two directions each stand up a listener and a
# collision would look exactly like a protocol failure.
PORT2="${INTEROP_PORT2:-15645}"
# A third, for the reconnect scenarios: they stand the C++ acceptor up TWICE on
# the same port and a collision with either direction above would look exactly
# like a counterparty that refused to come back.
PORT3="${INTEROP_PORT3:-15646}"
# How long any single wait below gets before the run is called a failure.
# A reversal that removes the restart must go RED, not HANG — a hang is how a
# reversal fails to prove anything (docs/reference/a-reversal-can-fail-by-hanging.md).
DEADLINE="${INTEROP_DEADLINE:-20}"
JOBS="${INTEROP_JOBS:-$(nproc 2>/dev/null || echo 2)}"

for tool in cmake g++ git cargo; do
  command -v "${tool}" >/dev/null || { echo "${tool} is required" >&2; exit 1; }
done

# ---- 1. QuickFIX, from source, at the pin -----------------------------------
if [[ ! -f "${SRC}/lib/libquickfix.a" ]]; then
  echo "==> fetching quickfix at ${PINNED_SHA}"
  if [[ ! -d "${SRC}/.git" ]]; then
    mkdir -p "${SRC}"
    git -C "${SRC}" init -q
    git -C "${SRC}" remote add origin https://github.com/quickfix/quickfix.git
  fi
  git -C "${SRC}" fetch -q --depth 1 origin "${PINNED_SHA}"
  git -C "${SRC}" checkout -q --detach FETCH_HEAD

  echo "==> building libquickfix (static, no SSL)"
  cmake -S "${SRC}" -B "${SRC}/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DHAVE_SSL=OFF -DQUICKFIX_TESTS=OFF -DQUICKFIX_EXAMPLES=OFF \
    -DQUICKFIX_SHARED_LIBS=OFF >/dev/null
  cmake --build "${SRC}/build" -j "${JOBS}" >/dev/null
fi
# Built, or the run stops here rather than three steps further on with a
# connection refused. `[measured 2026-09-02]` the first build of this in a fresh
# tree exited 0 and left the archive somewhere else entirely.
[[ -f "${SRC}/lib/libquickfix.a" ]] || { echo "libquickfix.a was not produced" >&2; exit 1; }
echo "==> libquickfix.a $(stat -c %s "${SRC}/lib/libquickfix.a" 2>/dev/null || stat -f %z "${SRC}/lib/libquickfix.a") bytes"

# ---- 2. The counterparty ----------------------------------------------------
rm -rf "${WORK}"
mkdir -p "${WORK}/store"
echo "==> building the acceptor"
g++ -std=c++17 -O1 -I "${SRC}/include" \
  "${REPO_ROOT}/tools/interop/acceptor.cpp" \
  -o "${WORK}/acceptor" "${SRC}/lib/libquickfix.a" -lpthread

cat > "${WORK}/acceptor.cfg" <<CFG
[DEFAULT]
ConnectionType=acceptor
SocketAcceptPort=${PORT}
SocketReuseAddress=Y
StartTime=00:00:00
EndTime=00:00:00
UseDataDictionary=Y
DataDictionary=${SRC}/spec/FIX44.xml
FileStorePath=${WORK}/store
ResetOnLogon=Y
ResetOnLogout=Y
ResetOnDisconnect=Y

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFACC
TargetCompID=FIXBOLT
HeartBtInt=30
CFG

# ---- 3. Run both, and read what they say ------------------------------------
echo "==> starting the acceptor on ${PORT}"
"${WORK}/acceptor" "${WORK}/acceptor.cfg" > "${WORK}/acceptor.log" 2>&1 &
ACCEPTOR_PID=$!
FIXBOLT_PID=""
# The reconnect scenarios' three processes: the C++ acceptor before the kill,
# the one after it, and this engine's initiator, which outlives both.
QF1_PID=""
QF2_PID=""
RECON_PID=""
cleanup() {
  for pid in "${ACCEPTOR_PID}" "${FIXBOLT_PID}" "${QF1_PID}" "${QF2_PID}" "${RECON_PID}"; do
    [[ -n "${pid}" ]] || continue
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
  done
}
trap cleanup EXIT

# Wait for the line it prints once the port is listening, not for a sleep.
for _ in $(seq 1 200); do
  grep -q "acceptor: ready" "${WORK}/acceptor.log" && break
  sleep 0.1
done
if ! grep -q "acceptor: ready" "${WORK}/acceptor.log"; then
  echo "the acceptor never became ready:" >&2
  cat "${WORK}/acceptor.log" >&2
  exit 1
fi

echo "==> driving the initiator"
cargo build -q -p fixbolt-interop
set +e
"${REPO_ROOT}/target/debug/interop" --connect "127.0.0.1:${PORT}" \
  --sender FIXBOLT --target QFACC | tee "${WORK}/interop.log"
set -e

# ---- 4. Read the output. This is the gate. ----------------------------------
#
# Not `$?` from the binary: a binary that dies before printing anything and a
# binary that prints six failures both exit non-zero, and CLAUDE.md §10 is about
# telling those apart.
fail=0
for step in logon news heartbeat testrequest resend gapfill logout; do
  if grep -qE "^interop: ${step} +ok" "${WORK}/interop.log"; then
    :
  else
    echo "MISSING OR FAILED STEP: ${step}" >&2
    fail=1
  fi
done
if ! grep -q "^interop: PASS 7/7" "${WORK}/interop.log"; then
  echo "no 'interop: PASS 7/7' line" >&2
  fail=1
fi

if [[ "${fail}" -ne 0 ]]; then
  echo >&2
  echo "---- what the acceptor saw ----" >&2
  cat "${WORK}/acceptor.log" >&2
  exit 1
fi

# ---- 4b. The other direction: a C++ initiator into THIS engine's acceptor ---
#
# The C++ acceptor from the first direction is done; stopping it now keeps the
# two runs from sharing anything but the library they were both built against.
kill "${ACCEPTOR_PID}" 2>/dev/null || true
wait "${ACCEPTOR_PID}" 2>/dev/null || true
ACCEPTOR_PID=""

echo
echo "==> building the C++ initiator"
g++ -std=c++17 -O1 -I "${SRC}/include" \
  "${REPO_ROOT}/tools/interop/initiator.cpp" \
  -o "${WORK}/initiator" "${SRC}/lib/libquickfix.a" -lpthread

mkdir -p "${WORK}/store2"
cat > "${WORK}/fixbolt.cfg" <<CFG
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=FIXBOLT

[SESSION]
TargetCompID=QFINI
HeartBtInt=2
CFG

cat > "${WORK}/initiator.cfg" <<CFG
[DEFAULT]
ConnectionType=initiator
SocketConnectHost=127.0.0.1
SocketConnectPort=${PORT2}
HeartBtInt=2
ReconnectInterval=1
ResetOnLogon=Y
ResetOnLogout=Y
ResetOnDisconnect=Y
StartTime=00:00:00
EndTime=00:00:00
UseDataDictionary=Y
DataDictionary=${SRC}/spec/FIX44.xml
FileStorePath=${WORK}/store2

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFINI
TargetCompID=FIXBOLT
CFG

echo "==> starting this engine's acceptor on ${PORT2}"
# **Stdin is the operator's channel**, and the fifo is what keeps it open: a
# process reading a pipe nobody holds sees EOF immediately, so `exec 9>` is not
# decoration. `tools/interop/src/main.rs::stop_on_stdin` says why the trigger is
# a line rather than a signal — ADR-0054, plan Sửa 4.
mkfifo "${WORK}/fixbolt-acceptor.ctl"
"${REPO_ROOT}/target/debug/interop" --role acceptor \
  --listen "127.0.0.1:${PORT2}" --cfg "${WORK}/fixbolt.cfg" \
  < "${WORK}/fixbolt-acceptor.ctl" \
  > "${WORK}/fixbolt-acceptor.log" 2>&1 &
FIXBOLT_PID=$!
exec 9> "${WORK}/fixbolt-acceptor.ctl"

# The line it waits for is printed by a thread that CONNECTED to the port, not
# by the thread that is about to call `serve`. A readiness line printed before
# the bind is a claim; this one is an observation. CLAUDE.md §10.
for _ in $(seq 1 200); do
  grep -q "interop: listening" "${WORK}/fixbolt-acceptor.log" && break
  sleep 0.1
done
if ! grep -q "interop: listening" "${WORK}/fixbolt-acceptor.log"; then
  echo "this engine's acceptor never became ready:" >&2
  cat "${WORK}/fixbolt-acceptor.log" >&2
  exit 1
fi

echo "==> driving the C++ initiator"
set +e
"${WORK}/initiator" "${WORK}/initiator.cfg" ${INTEROP_INITIATOR_ARGS:-} \
  2>&1 | tee "${WORK}/interop-acceptor.log"
set -e

# **The first time in this repository that one of these processes is asked to
# stop rather than killed.** `STATUS.md` item 47: `Admin::shutdown` reaches the
# engine through the front door, so `serve` returns on its own and prints what
# it managed. A `kill` here would prove nothing about that and would hide it.
#
# The wait has a bound: an engine that ignores the stop must fail this gate,
# not hang it. If it is still up after ten seconds it is killed and the
# assertion below reads a missing line rather than a timeout with no message.
echo "stop" >&9 || true
stopped="no"
for _ in $(seq 1 100); do
  kill -0 "${FIXBOLT_PID}" 2>/dev/null || { stopped="yes"; break; }
  sleep 0.1
done
exec 9>&-
if [[ "${stopped}" != "yes" ]]; then
  echo "this engine's acceptor did not return from serve within 10s of Admin::shutdown" >&2
  kill "${FIXBOLT_PID}" 2>/dev/null || true
fi
wait "${FIXBOLT_PID}" 2>/dev/null || true
FIXBOLT_PID=""

# ---- 4c. Read that output too. Every step, not only the PASS line. ---------
fail=0

# The eighth assertion of this direction, and it is about this engine rather
# than about the C++ initiator: `serve` came back because an operator asked,
# and it counted the session it said goodbye to.
if grep -qE "^interop: acceptor stopped: Shutdown \{" "${WORK}/fixbolt-acceptor.log"; then
  echo "interop-acceptor: shutdown ok $(grep -oE 'Shutdown \{.*' "${WORK}/fixbolt-acceptor.log" | head -1)"
else
  echo "MISSING: serve never returned through Admin::shutdown" >&2
  fail=1
fi
for step in logon order heartbeat testrequest resend gapfill logout; do
  if grep -qE "^interop-acceptor: ${step} +ok" "${WORK}/interop-acceptor.log"; then
    :
  else
    echo "MISSING OR FAILED STEP: ${step}" >&2
    fail=1
  fi
done
if ! grep -q "^interop-acceptor: PASS 7/7" "${WORK}/interop-acceptor.log"; then
  echo "no 'interop-acceptor: PASS 7/7' line" >&2
  fail=1
fi

if [[ "${fail}" -ne 0 ]]; then
  echo >&2
  echo "---- what this engine's acceptor said ----" >&2
  cat "${WORK}/fixbolt-acceptor.log" >&2
  exit 1
fi

# ---- 4d / 4e. The reconnect loop, judged by an acceptor that dies -----------
#
# `STATUS.md` item 38. Item 35 shipped `connect_and_serve` and EVERY test of it
# is this repository's own reading: the 59 acceptance definitions never
# reconnect an initiator, the mirrored corpus does not reach it, and the two
# directions above connect once. ADR-0043 said so in its own Consequences —
# "only an interop scenario driving a real counterparty through a disconnect
# would close that, which scripts/interop.sh could grow and today does not".
# This is that scenario.
#
# WHAT PLAYS THE COUNTERPARTY, AND WHY IT IS KILLED RATHER THAN STOPPED.
# `tools/interop/acceptor.cpp` is reused unchanged, and the venue "restarting"
# is its process dying and coming back on the same FileStorePath. That is the
# deployment case, and it is what forces QuickFIX's own store — not this
# repository's — to be the thing that remembers where the numbering was.
#
# WHO JUDGES. Not fixbolt. The assertions below read the C++ acceptor's
# transcripts, `A1.log` before the kill and `A2.log` after it. The one thing
# read from this engine's own output is `delivered`, and that line is written
# from inside the application: it says a message arrived AND was accepted in
# sequence, which a line printed on the wire would not.

rc_fail=0
rc_total=0

# One assertion. Always prints, never returns early — a step that cannot run
# must not be able to hide the ones behind it.
rc_step() {
  local name="$1" ok="$2" saw="$3" line
  if [[ "${ok}" == "yes" ]]; then
    line="$(printf '%s: %-11s ok    %s' "${TAG}" "${name}" "${saw}")"
  else
    line="$(printf '%s: %-11s FAIL  %s' "${TAG}" "${name}" "${saw}")"
    rc_fail=$((rc_fail + 1))
  fi
  rc_total=$((rc_total + 1))
  echo "${line}"
  # Also to a file, so the grep gate at 4f reads something this function
  # actually wrote rather than checking its own arithmetic. A scenario that
  # returned early prints fewer lines, and that is the failure 4f exists for.
  echo "${line}" >> "${WORK}/${TAG}.steps"
}

# Wait for `pattern` to appear in `file` at least `n` times, bounded.
# Returns non-zero on timeout rather than hanging: a reversal that removes the
# restart has to go red, and a reversal that hangs proves nothing.
rc_wait() {
  local file="$1" pattern="$2" n="$3" i=0
  local ticks=$((DEADLINE * 10))
  while [[ "${i}" -lt "${ticks}" ]]; do
    if [[ -f "${file}" ]] && [[ "$(grep -c -- "${pattern}" "${file}" || true)" -ge "${n}" ]]; then
      return 0
    fi
    sleep 0.1
    i=$((i + 1))
  done
  return 1
}

# The `34=` values on lines matching a filter, one per line.
rc_seqs() { grep -F -- "$2" "$1" 2>/dev/null | sed -nE 's/.*\|34=([0-9]+)\|.*/\1/p' || true; }

# The C++ acceptor's configuration for these two scenarios.
#
# **The three `ResetOn*` are `N`, and that is the whole scenario.** The two
# directions above run them all `Y`, which makes QuickFIX forget its numbering
# at every logon — under that configuration "the session continued" is not a
# question that can be asked, because both ends restart at 1 and a broken
# engine passes.
mkdir -p "${WORK}/store3"
cat > "${WORK}/acceptor-reconnect.cfg" <<CFG
[DEFAULT]
ConnectionType=acceptor
SocketAcceptPort=${PORT3}
SocketReuseAddress=Y
StartTime=00:00:00
EndTime=00:00:00
UseDataDictionary=Y
DataDictionary=${SRC}/spec/FIX44.xml
FileStorePath=${WORK}/store3
ResetOnLogon=N
ResetOnLogout=N
ResetOnDisconnect=N

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFACC
TargetCompID=FIXBOLT
HeartBtInt=30
CFG

# One scenario. $1 is the signal that ends the first acceptor, $2 the tag its
# lines carry.
run_reconnect() {
  local signal="$1"
  TAG="$2"
  # Seconds to wait after both News are delivered and before the venue is
  # killed. `0` for the two original scenarios; the third uses it to guarantee
  # a Heartbeat lands between the last application message and the death.
  local settle="${3:-0}"
  local A1="${WORK}/${TAG}-A1.log"
  local A2="${WORK}/${TAG}-A2.log"
  local R="${WORK}/${TAG}-R.log"

  rm -rf "${WORK}/store3" "${WORK}/jrnl3"
  mkdir -p "${WORK}/store3" "${WORK}/jrnl3"
  rm -f "${A1}" "${A2}" "${R}" "${WORK}/${TAG}.steps"

  echo
  echo "==> [${TAG}] the acceptor, on ${PORT3}"
  "${WORK}/acceptor" "${WORK}/acceptor-reconnect.cfg" > "${A1}" 2>&1 &
  QF1_PID=$!
  if ! rc_wait "${A1}" "acceptor: ready" 1; then
    echo "the acceptor never became ready:" >&2; cat "${A1}" >&2; exit 1
  fi

  echo "==> [${TAG}] this engine's initiator, through connect_and_serve"
  "${REPO_ROOT}/target/debug/interop" --role reconnect \
    --connect "127.0.0.1:${PORT3}" --journal "${WORK}/jrnl3/FIXBOLT.journal" \
    ${INTEROP_RECONNECT_ARGS:-} > "${R}" 2>&1 &
  RECON_PID=$!

  # Both News delivered means the session is fully up AND this end has already
  # sent its own application message — which is what puts a number in the
  # journal for `Recovery` to continue from. Killing before this point would
  # be killing a session that has nothing to continue.
  local up="yes"
  rc_wait "${R}" "delivered" 2 || up="no"
  if [[ "${up}" != "yes" ]]; then
    echo "---- this engine said ----" >&2; cat "${R}" >&2
    echo "---- the acceptor said ----" >&2; cat "${A1}" >&2
  fi

  # **A deliberate wait, and it is the point of the third scenario.** With
  # `--heart-bt-int 1` this puts at least one Heartbeat after the `35=B`, so the
  # last number this engine spent is one the journal holds no bytes for. Before
  # ADR-0053 that made `next_out` come back short; the two original scenarios
  # avoided it by never letting a Heartbeat happen, which is a fixture choosing
  # the result. STATUS.md item 48.
  if [[ "${settle}" != "0" ]]; then
    echo "==> [${TAG}] waiting ${settle}s so a Heartbeat falls after the last 35=B"
    sleep "${settle}"
  fi

  echo "==> [${TAG}] the venue goes away (${signal})"
  kill "${signal}" "${QF1_PID}" 2>/dev/null || true
  wait "${QF1_PID}" 2>/dev/null || true
  QF1_PID=""

  # A goodbye needs a moment to be answered before the socket dies with it.
  if [[ "${signal}" == "-TERM" ]]; then
    rc_wait "${A1}" "35=5" 2 || true
  fi

  echo "==> [${TAG}] the venue comes back, same store"
  "${WORK}/acceptor" "${WORK}/acceptor-reconnect.cfg" > "${A2}" 2>&1 &
  QF2_PID=$!
  if ! rc_wait "${A2}" "acceptor: ready" 1; then
    echo "the acceptor never came back:" >&2; cat "${A2}" >&2; exit 1
  fi

  # Nothing here tells this engine to redial. `reconnect::Policy` does, on its
  # own ladder, and this wait is the assertion that it did.
  local came_back="yes"
  rc_wait "${A2}" "acceptor: in " 1 || came_back="no"
  # Let the resumed session finish saying what it has to say.
  rc_wait "${R}" "delivered" 4 || true
  sleep 0.5

  kill "${RECON_PID}" 2>/dev/null || true; wait "${RECON_PID}" 2>/dev/null || true; RECON_PID=""
  kill "${QF2_PID}" 2>/dev/null || true;  wait "${QF2_PID}" 2>/dev/null || true;  QF2_PID=""

  if [[ "${signal}" == "-KILL" ]]; then
    rc_assert_kill "${A1}" "${A2}" "${R}" "${came_back}"
  else
    rc_assert_logout "${A1}" "${A2}" "${R}" "${came_back}"
  fi
}

# What each scenario claims, read off the transcripts.
#
# **The two scenarios claim the same five things, and that is new.**
# `[đo 2026-09-05]` until item 48 was fixed they could not: after a clean logout
# this engine answers the goodbye, spending an outbound number `Journal::put`
# never recorded, so the numbering could not continue and the third assertion
# pinned the size of the gap instead of asserting it was closed. The journal now
# records the number (ADR-0053), so `known_gap` is gone and the logout scenario
# asserts continuation exactly as the kill scenario does. The one thing that
# differs is the first step: one transcript must contain no `35=5`, the other
# must contain one in each direction.
rc_assert_kill() {
  local A1="$1" A2="$2" R="$3" came_back="$4"

  # 1. Nobody said goodbye. That is what makes this the abrupt scenario, and it
  #    is asserted rather than assumed from the signal.
  local goodbyes
  goodbyes="$(grep -c -F '|35=5|' "${A1}" || true)"
  rc_step "dropped" "$([[ "${goodbyes}" -eq 0 ]] && echo yes || echo no)" \
    "no 35=5 in the first transcript (saw ${goodbyes})"

  rc_assert_continued "${A1}" "${A2}" "${R}" "${came_back}"
}

# Assertions 2-5, which both scenarios now make.
rc_assert_continued() {
  local A1="$1" A2="$2" R="$3" came_back="$4"

  # 2. It came back. Nothing told it to — `reconnect::Policy` did.
  local logon_in
  logon_in="$(grep -E '^acceptor: in ' "${A2}" 2>/dev/null | grep -F '|35=A|' | head -1 || true)"
  rc_step "back" "$([[ -n "${logon_in}" && "${came_back}" == "yes" ]] && echo yes || echo no)" \
    "${logon_in:-nothing reached the second acceptor}"

  # 3. The numbering continued. **Relational**: the number this engine's second
  #    Logon carries must be one past the last it sent before the kill. A
  #    literal `34=3` would be a gate a slow runner could break for a reason
  #    that is not the protocol.
  local last_out first_logon want got_seq
  last_out="$(rc_seqs "${A1}" 'acceptor: in ' | tail -1)"
  first_logon="$(rc_seqs "${A2}" 'acceptor: in ' | head -1)"
  got_seq="${first_logon:-none}"
  if [[ -n "${last_out}" ]]; then want=$((last_out + 1)); else want="?"; fi
  rc_step "next_out" "$([[ "${got_seq}" == "${want}" ]] && echo yes || echo no)" \
    "sent up to 34=${last_out:-none} before the kill, came back at 34=${got_seq}, wanted 34=${want}"

  # 4. The inbound direction continued too, and this is where `delivered` earns
  #    its place: a session whose `next_in` had restarted would have opened a
  #    gap on these two and asked for them back instead of handing them up.
  local news_ok="yes" n wanted=""
  for n in $(grep -E '^acceptor: out ' "${A2}" 2>/dev/null | grep -F '|35=B|' | sed -nE 's/.*\|34=([0-9]+)\|.*/\1/p' || true); do
    wanted="${wanted} ${n}"
    grep -q "delivered 34=${n} " "${R}" || news_ok="no"
  done
  [[ -n "${wanted}" ]] || news_ok="no"
  rc_step "next_in" "${news_ok}" \
    "35=B at 34=${wanted# } sent after the restart, each delivered to the application"

  # 5. And without anybody papering over it. A ResendRequest, a reset flag or a
  #    "MsgSeqNum too low" would each mean the numbering did NOT carry — and the
  #    third is the exact refusal this scenario produced before the journal held
  #    an application message at all (the plan's Sửa 2).
  local resend reset toolow
  resend="$(grep -c -F '|35=2|' "${A2}" || true)"
  reset="$(grep -c -F '|141=Y|' "${A2}" || true)"
  toolow="$(grep -c -F 'MsgSeqNum too low' "${A2}" || true)"
  rc_step "no_resend" \
    "$([[ "${resend}" -eq 0 && "${reset}" -eq 0 && "${toolow}" -eq 0 ]] && echo yes || echo no)" \
    "35=2: ${resend}, 141=Y: ${reset}, 'MsgSeqNum too low': ${toolow}"

  # 6. **Two sources for the same number, and the journal is never behind.**
  #    The durable one is the journal, read by `Resumed::from_journal` when the
  #    session is resumed; the live one is the engine's own `next_out`, read
  #    through an `Observer` from another thread. `STATUS.md` item 48's write-up
  #    has a table saying the second was unreachable — `connect_and_serve`
  #    handed out no handle. Item 47 is what makes this assertion exist at all.
  #
  #    **It is an inequality, and that is not a weaker version of an equality
  #    that was tried and failed.** ADR-0053 argued that an observer knows the
  #    number *when somebody asks*, so a message sent between the last poll and
  #    the connection ending is spent, durable, and invisible to this side. On a
  #    clean logout it always is: answering the counterparty's `35=5` and
  #    dropping the link happen inside one turn, so no snapshot falls between
  #    them. `[đo 2026-09-05]` the first version of this assertion demanded
  #    `live == resumed + 1` and read `resumed 4, live 6` — the application also
  #    speaks first on logon, so the constant was wrong too, and a gate built on
  #    a constant like that breaks when the *application* changes.
  #
  #    Sampling can only make the live number low, never high. So: every number
  #    an operator saw spent is one the journal knows about. Item 48's defect
  #    makes the resumed number LOWER than one already printed here — in the
  #    `beat` scenario, resumed 3 against a live 5 — and that is red.
  local two
  two="$(awk '
    /interop-reconnect: observer next_out=/ {
      match($0, /next_out=[0-9]+/); l = substr($0, RSTART + 9, RLENGTH - 9) + 0
      if (l > high) { high = l }
      next
    }
    /interop-reconnect: resuming next_out=/ {
      match($0, /next_out=[0-9]+/); r = substr($0, RSTART + 9, RLENGTH - 9) + 0
      n++
      if (r < high) { bad = bad " (resumed " r ", already seen live " high ")" }
      high = 0
      next
    }
    END {
      if (n == 0) { print "no resume was observed" }
      else if (bad != "") { print "BEHIND" bad }
      else { print "ok " n " resume(s), journal never behind the live count" }
    }
  ' "${R}")"
  rc_step "two_sources" "$([[ "${two}" == ok* ]] && echo yes || echo no)" \
    "journal vs Observer: ${two}"
}

rc_assert_logout() {
  local A1="$1" A2="$2" R="$3" came_back="$4"

  # 1. A goodbye, and this engine answered it. `crates/session/tests/goodbye.rs`
  #    holds that behaviour; here another implementation confirms it.
  local said got
  said="$(grep -c -E '^acceptor: out .*\|35=5\|' "${A1}" || true)"
  got="$(grep -c -E '^acceptor: in .*\|35=5\|' "${A1}" || true)"
  rc_step "goodbye" "$([[ "${said}" -ge 1 && "${got}" -ge 1 ]] && echo yes || echo no)" \
    "35=5 out: ${said}, answered by this engine: ${got}"

  # 2-5. **The same four the kill scenario makes**, and the third of them is the
  #      one that used to be impossible: the answer to the goodbye spends a
  #      number, and the journal now knows it. A run against the pre-ADR-0053
  #      engine fails `next_out` with `expecting N but received N-1`, which is
  #      what `known_gap` used to pin. ADR-0053, STATUS.md item 48.
  rc_assert_continued "${A1}" "${A2}" "${R}" "${came_back}"
}

# ---- 4d. The venue is killed. Nobody says goodbye. --------------------------
rc_fail=0; rc_total=0
run_reconnect -KILL "interop-reconnect"
if [[ "${rc_fail}" -eq 0 && "${rc_total}" -gt 0 ]]; then
  echo "interop-reconnect: PASS $((rc_total - rc_fail))/${rc_total}"
else
  echo "interop-reconnect: FAIL $((rc_total - rc_fail))/${rc_total}"
fi

# ---- 4e. The venue says goodbye first, then goes. ---------------------------
#
# ADR-0043 decision 5: EVERY ending climbs the ladder, including a clean
# logout. A policy that counted only failures would reconnect instantly after a
# goodbye, which is a reconnect storm with a polite name. This is that decision
# seen from an engine that never heard of it.
rc_fail=0; rc_total=0
run_reconnect -TERM "interop-reconnect-logout"
if [[ "${rc_fail}" -eq 0 && "${rc_total}" -gt 0 ]]; then
  echo "interop-reconnect-logout: PASS $((rc_total - rc_fail))/${rc_total}"
else
  echo "interop-reconnect-logout: FAIL $((rc_total - rc_fail))/${rc_total}"
fi

# ---- 4e-bis. Killed, with a Heartbeat guaranteed inside the window. --------
#
# The same abrupt scenario as 4d, run at `HeartBtInt=1` with a pause before the
# kill. `[đo 2026-09-05]` 4d passed at `HeartBtInt=30` **because no
# administrative message was sent after the last application one** — the exact
# condition STATUS.md item 48 is about — so it was green for a reason that was
# the fixture rather than the engine. This round removes that condition: the
# last number spent is a Heartbeat's, which no journal holds bytes for, and
# `next_out` coming back right is ADR-0053 working for every administrative
# message rather than for `35=5` alone.
rc_fail=0; rc_total=0
INTEROP_RECONNECT_ARGS="--heart-bt-int 1" run_reconnect -KILL "interop-reconnect-beat" 2.5
if [[ "${rc_fail}" -eq 0 && "${rc_total}" -gt 0 ]]; then
  echo "interop-reconnect-beat: PASS $((rc_total - rc_fail))/${rc_total}"
else
  echo "interop-reconnect-beat: FAIL $((rc_total - rc_fail))/${rc_total}"
fi

# ---- 4f. Read that output too. Every assertion, not only the PASS line. -----
#
# The same shape the two directions above use, and for the same reason: a
# scenario that exits early prints fewer step lines and still leaves a PASS
# line's arithmetic looking tidy. Naming every step is what catches that —
# `[measured 2026-09-04]` renaming one step in the acceptor direction left the
# binary printing `PASS 7/7` while the script correctly failed.
fail=0
for step in dropped back next_out next_in no_resend two_sources; do
  if ! grep -qE "^interop-reconnect: ${step} +ok" "${WORK}/interop-reconnect.steps" 2>/dev/null; then
    echo "MISSING OR FAILED ASSERTION: interop-reconnect ${step}" >&2
    fail=1
  fi
done
for step in goodbye back next_out next_in no_resend two_sources; do
  if ! grep -qE "^interop-reconnect-logout: ${step} +ok" "${WORK}/interop-reconnect-logout.steps" 2>/dev/null; then
    echo "MISSING OR FAILED ASSERTION: interop-reconnect-logout ${step}" >&2
    fail=1
  fi
done
# The third scenario: a kill with a Heartbeat guaranteed inside the window.
for step in dropped back next_out next_in no_resend two_sources; do
  if ! grep -qE "^interop-reconnect-beat: ${step} +ok" "${WORK}/interop-reconnect-beat.steps" 2>/dev/null; then
    echo "MISSING OR FAILED ASSERTION: interop-reconnect-beat ${step}" >&2
    fail=1
  fi
done

if [[ "${fail}" -ne 0 ]]; then
  echo >&2
  echo "---- the reconnect scenarios failed; every transcript follows ----" >&2
  for f in "${WORK}"/interop-reconnect*-A1.log "${WORK}"/interop-reconnect*-A2.log "${WORK}"/interop-reconnect*-R.log; do
    [[ -f "${f}" ]] || continue
    echo "---- ${f} ----" >&2
    cat "${f}" >&2
  done
  exit 1
fi

# ---- 5. Nothing of QuickFIX's entered the repository ------------------------
#
# The question is what THIS SCRIPT added, not whether the tree was clean when it
# started — a developer runs this with work in progress, and a check that fails
# on their own edits gets ignored, which is worse than not having it.
#
# So: the snapshot taken before the run, compared with the one taken now.
after="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | sort)"
added="$(comm -13 <(echo "${BEFORE}") <(echo "${after}") || true)"
if [[ -n "${added}" ]]; then
  echo "THIS RUN ADDED FILES GIT CAN SEE:" >&2
  echo "${added}" >&2
  echo >&2
  echo "Everything libquickfix belongs under vendor/, which is gitignored —" >&2
  echo "committing any of it pulls QuickFIX's attribution clause into this" >&2
  echo "repository. ADR-0001, CLAUDE.md §2 rule 9." >&2
  exit 1
fi
echo "==> the run added nothing git can see"

echo
echo "interop: 7 / 7 + 8 / 8 + 6 / 6 + 6 / 6 + 6 / 6 against libquickfix @ ${PINNED_SHA}"
echo "both roles and all three reconnect scenarios, each checked by somebody else's engine"
