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
# A fourth, for the microsecond `52=` scenario. Same argument as the others: a
# port collision and a protocol failure look identical from the log.
PORT4="${INTEROP_PORT4:-15647}"
# A fifth, for the odd-precision `52=` scenario (4i). Same argument again.
PORT5="${INTEROP_PORT5:-15648}"
# A sixth, for the `ResetOnLogon` scenario (4j). It stands this engine's
# acceptor up TWICE on the same port, so a collision with any listener above
# would look exactly like an acceptor that refused to come back.
PORT6="${INTEROP_PORT6:-15649}"
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
# The 4g scenario's acceptor, so a failure there does not leak a listener.
NE_PID=""
# The 4h scenario's acceptor.
MIC_PID=""
# The 4i scenario's acceptor.
ODD_PID=""
# The 4j scenario stands this engine's acceptor up twice on the same port: once
# before the restart and once after. Two pids, because a failure between them
# would otherwise leak the first listener into the second half of the run.
RST1_PID=""
RST2_PID=""
cleanup() {
  for pid in "${ACCEPTOR_PID}" "${FIXBOLT_PID}" "${QF1_PID}" "${QF2_PID}" "${RECON_PID}" "${NE_PID}" "${MIC_PID}" "${ODD_PID}" "${RST1_PID}" "${RST2_PID}"; do
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

  # **The goodbye, and then the listener, at once.** `[đo 2026-09-05]` waiting
  # only for the process to be reaped left a window this engine can dial into:
  # `SIGTERM` makes QuickFIX log its sessions out and *then* shut down, so its
  # listening socket outlives the `35=5` by however long that takes. The
  # reconnect ladder's first rung is 200 ms, so on a loaded runner this engine
  # connects to a venue that is already stopping, spends a `34=` on a `Logon`
  # nobody will ever answer, and comes back on the NEXT attempt one number
  # higher than the restarted venue expects. The restarted venue then asks for a
  # resend — correct on both sides, and nothing at all to do with what this
  # scenario is about, but it reads as a `no_resend` failure.
  #
  # So: wait for the goodbye to be answered (two `35=5`, out and in — which is
  # what assertion 1 reads), then take the process away immediately. **How the
  # venue finally dies after saying goodbye is not the subject**; that it said
  # goodbye is.
  if [[ "${signal}" == "-TERM" ]]; then
    rc_wait "${A1}" "35=5" 2 || true
    kill -KILL "${QF1_PID}" 2>/dev/null || true
  fi
  wait "${QF1_PID}" 2>/dev/null || true
  QF1_PID=""

  # And do not stand the venue back up until the port really refuses. A restart
  # that raced the old listener would be the same window from the other end.
  for _ in $(seq 1 100); do
    if ! (exec 3<>"/dev/tcp/127.0.0.1/${PORT3}") 2>/dev/null; then break; fi
    exec 3>&- 2>/dev/null || true
    sleep 0.1
  done

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
  # `resumes` is not asserted on; it is printed because **this assertion has a
  # premise** — that the engine reconnected once. Two resumes means a number was
  # spent on an attempt the restarted venue never saw, so a `35=2` is the
  # protocol working rather than the numbering failing, and a reader should not
  # have to re-derive that from three transcripts. The premise is held by the
  # kill above, not by this line.
  local resumes
  resumes="$(grep -c -F 'interop-reconnect: resuming next_out=' "${R}" || true)"
  rc_step "no_resend" \
    "$([[ "${resend}" -eq 0 && "${reset}" -eq 0 && "${toolow}" -eq 0 ]] && echo yes || echo no)" \
    "35=2: ${resend}, 141=Y: ${reset}, 'MsgSeqNum too low': ${toolow}, resumes: ${resumes}"

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

# ---- 4g. `789=NextExpectedMsgSeqNum`, both directions on one connection -----
#
# **The only outside opinion this engine can get about `789`.** The 59
# acceptance definitions carry the field 0 times, so nothing in this repository
# can confirm that a real counterparty accepts what this end writes or that
# this end reads what a real counterparty sends. This scenario is that
# confirmation, and it is the whole of it.
#
# **It asserts that the field arrived before it asserts anything about the
# response, and that ordering is the point.** `[verified 2026-09-06]` QuickFIX
# C++ reads settings by name on demand (`SessionFactory.cpp:228`) with no
# validation pass over the file, so a key it does not recognise is ignored in
# **silence**. The two QuickFIX families spell this one differently —
# `SendNextExpectedMsgSeqNum` here, `EnableNextExpectedMsgSeqNum` in Java, and
# this repository had recorded the Java one. With the wrong name the
# counterparty sends no `789`, the session comes up perfectly, every ordinary
# step passes, and the scenario is green over a field that never existed.
#
# So there are two preconditions, one per direction, and each is a real
# assertion rather than a comment:
#
#   * `sent`     — the C++ acceptor's own transcript shows the inbound Logon
#                  carrying `789=`. This end wrote it and libquickfix took it.
#   * `received` — the `next_expected` step inside the initiator, which reads
#                  the counterparty's Logon reply and requires `789=` on it.
#                  This is what fails if the key name is wrong.
#
# docs/reference/who-owns-the-outbound-header.md
echo
echo "==> [interop-next-expected] a counterparty that speaks 789"

# **Generated with stderr captured, and the capture is asserted empty.**
# `[measured 2026-09-06]` an unquoted heredoc expands its body, so two
# backticks in a *comment* inside one made the shell try to run the word
# between them. It printed "…: command not found" and the script carried on
# to `PASS 9/9` and `exit 0`: `set -e` does not see a failed substitution
# inside a heredoc, and nothing else was looking. The error sat in a green
# job's log for a whole CI run. This is the check that makes it fail instead.
cat > "${WORK}/acceptor-789.cfg" 2> "${WORK}/acceptor-789.err" <<CFG
[DEFAULT]
ConnectionType=acceptor
SocketAcceptPort=${PORT}
SocketReuseAddress=Y
StartTime=00:00:00
EndTime=00:00:00
UseDataDictionary=Y
DataDictionary=${SRC}/spec/FIX44.xml
FileStorePath=${WORK}/store-789
ResetOnLogon=Y
ResetOnLogout=Y
ResetOnDisconnect=Y
# The C++ spelling. Java's EnableNextExpectedMsgSeqNum would be ignored here
# without a word, and the two preconditions below are what would catch it.
#
# NO BACKTICKS AND NO COMMAND SUBSTITUTION ANYWHERE IN THIS HEREDOC, comments
# included: the delimiter is unquoted because the body needs ${PORT} and
# ${SRC}, so the shell expands the body and runs substitutions in it. [measured 2026-09-06] the two backticks that
# used to be around the name above made CI print
# "EnableNextExpectedMsgSeqNum: command not found" in the middle of a job that
# went on to pass 9/9. The assertion below is what makes this comment more than
# a wish.
SendNextExpectedMsgSeqNum=Y

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFACC
TargetCompID=FIXBOLT
HeartBtInt=30
CFG

# **The generated file is read back before it is used.** An unquoted heredoc
# expands its body, so a stray backtick or `$(` mangles the config *and* prints
# a shell error nobody reads — the failure mode this scenario exists to catch,
# arriving through the fixture instead of the engine.
if [[ -s "${WORK}/acceptor-789.err" ]]; then
  echo "[interop-next-expected] writing the config produced errors:" >&2
  cat "${WORK}/acceptor-789.err" >&2
  exit 1
fi
grep -qx 'SendNextExpectedMsgSeqNum=Y' "${WORK}/acceptor-789.cfg" || {
  echo "[interop-next-expected] the generated config lost its key:" >&2
  cat "${WORK}/acceptor-789.cfg" >&2
  exit 1
}

mkdir -p "${WORK}/store-789"
"${WORK}/acceptor" "${WORK}/acceptor-789.cfg" > "${WORK}/acceptor-789.log" 2>&1 &
NE_PID=$!
for _ in $(seq 1 200); do
  grep -q "acceptor: ready" "${WORK}/acceptor-789.log" && break
  sleep 0.1
done
if ! grep -q "acceptor: ready" "${WORK}/acceptor-789.log"; then
  echo "[interop-next-expected] the acceptor never became ready:" >&2
  cat "${WORK}/acceptor-789.log" >&2
  exit 1
fi

set +e
"${REPO_ROOT}/target/debug/interop" --role initiator --connect "127.0.0.1:${PORT}" \
  --sender FIXBOLT --target QFACC --next-expected \
  > "${WORK}/interop-789.log" 2>&1
set -e
kill "${NE_PID}" 2>/dev/null || true
wait "${NE_PID}" 2>/dev/null || true
NE_PID=""

ne_fail=0

# Precondition 1: this end's `789` reached libquickfix, on the Logon it took.
if grep -E "^acceptor: in  .*35=A.*789=" "${WORK}/acceptor-789.log" >/dev/null; then
  echo "interop-next-expected: sent        ok    $(grep -E "^acceptor: in  .*35=A.*789=" "${WORK}/acceptor-789.log" | head -1)"
else
  echo "interop-next-expected: sent        FAIL  no inbound 35=A carrying 789= in the acceptor transcript" >&2
  ne_fail=1
fi

# Precondition 2: libquickfix's `789` reached this end. **This is the one that
# fails on a misspelled key**, and without it every line below is vacuous.
if grep -qE "^interop: next_expected +ok" "${WORK}/interop-789.log"; then
  echo "interop-next-expected: received    ok    $(grep -E "^interop: next_expected +ok" "${WORK}/interop-789.log" | head -1)"
else
  echo "interop-next-expected: received    FAIL  the counterparty's Logon carried no 789= — is SendNextExpectedMsgSeqNum the right key?" >&2
  ne_fail=1
fi

# And the ordinary scenario still runs end to end with the field on both
# Logons. A `789` that is accepted but breaks the session is not a pass.
for step in logon news heartbeat testrequest resend gapfill logout; do
  if ! grep -qE "^interop: ${step} +ok" "${WORK}/interop-789.log"; then
    echo "interop-next-expected: ${step} FAIL  step did not pass with 789 on" >&2
    ne_fail=1
  fi
done

if [[ "${ne_fail}" -eq 0 ]]; then
  echo "interop-next-expected: PASS 9/9"
else
  echo "interop-next-expected: FAIL" >&2
  echo "---- what the acceptor saw ----" >&2
  cat "${WORK}/acceptor-789.log" >&2
  echo "---- what this engine printed ----" >&2
  cat "${WORK}/interop-789.log" >&2
  exit 1
fi

# ---- 4h. `52=` at microsecond precision, judged on the raw bytes ------------
#
# ADR-0057's half of `timestamp-micros`. Every other assertion in this script is
# about a step completing; this one is about **what the field actually looks
# like on the wire**, because a session that logs on proves nothing about the
# width of its timestamps — QuickFIX C++ accepts any `UTCTimestamp` from 17 to
# 27 bytes (`FieldConvertors.h:492-596`, read at the pin), so a 21-byte stamp
# from an engine configured for 24 would pass every step gate above in silence.
#
# **The oracle is real here, which is unusual.** ADR-0056 had to record that
# `369` can never be judged this way because QuickFIX C++ does not implement it.
# `TimestampPrecision` it does: an integer 0-9, `Session.h:167-174`, default 3.
# So both ends are configured for six digits and both directions are read.
#
# The raw bytes come from QuickFIX's own `FileLogPath`, not from this
# repository's message log — a log written by the code under test is not
# evidence about the code under test.
echo
echo "==> [interop-micros] both ends at TimestampPrecision=6 on ${PORT4}"
mkdir -p "${WORK}/store4"

cat > "${WORK}/fixbolt-micros.cfg" <<CFG
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=FIXBOLT
TimestampPrecision=6

[SESSION]
TargetCompID=QFMIC
HeartBtInt=2
CFG

cat > "${WORK}/initiator-micros.cfg" <<CFG
[DEFAULT]
ConnectionType=initiator
SocketConnectHost=127.0.0.1
SocketConnectPort=${PORT4}
HeartBtInt=2
ReconnectInterval=1
ResetOnLogon=Y
ResetOnLogout=Y
ResetOnDisconnect=Y
StartTime=00:00:00
EndTime=00:00:00
UseDataDictionary=Y
DataDictionary=${SRC}/spec/FIX44.xml
FileStorePath=${WORK}/store4
TimestampPrecision=6

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFMIC
TargetCompID=FIXBOLT
CFG

mkfifo "${WORK}/micros.ctl"
"${REPO_ROOT}/target/debug/interop" --role acceptor \
  --listen "127.0.0.1:${PORT4}" --cfg "${WORK}/fixbolt-micros.cfg" \
  < "${WORK}/micros.ctl" \
  > "${WORK}/fixbolt-micros.log" 2>&1 &
MIC_PID=$!
exec 8> "${WORK}/micros.ctl"

for _ in $(seq 1 200); do
  grep -q "interop: listening" "${WORK}/fixbolt-micros.log" && break
  sleep 0.1
done
if ! grep -q "interop: listening" "${WORK}/fixbolt-micros.log"; then
  echo "the microsecond acceptor never became ready:" >&2
  cat "${WORK}/fixbolt-micros.log" >&2
  exit 1
fi

set +e
# `--dump-tape` so the transcript is printed on a run that PASSES. Every other
# scenario here is judged on step lines; this one is judged on the bytes.
"${WORK}/initiator" "${WORK}/initiator-micros.cfg" --dump-tape \
  2>&1 | tee "${WORK}/interop-micros.log"
set -e

echo "stop" >&8 || true
for _ in $(seq 1 100); do
  kill -0 "${MIC_PID}" 2>/dev/null || break
  sleep 0.1
done
exec 8>&-
kill "${MIC_PID}" 2>/dev/null || true
wait "${MIC_PID}" 2>/dev/null || true
MIC_PID=""

# **Where the raw bytes actually are.** `FileLogPath` is in the config above and
# QuickFIX ignores it here: `tools/interop/initiator.cpp` installs its own
# `RawLogFactory`, so the file log is never created. The first version of this
# block globbed for that file, found nothing, and — under `set -o pipefail` —
# died with no message at all. The bytes were on stdout the whole time: that
# `FIX::Log` is a callback on the real wire, and the tool prints every frame it
# sees with `SOH` shown as `|`. So this reads QuickFIX's own view of the socket,
# which is still not a log written by the code under test.
MICLOG="${WORK}/interop-micros.log"
mic_fail=0
echo "==> [interop-micros] reading ${MICLOG}"
# A 24-byte `52=`: eight date digits, the time, a dot and SIX fractional digits,
# then the field separator. **Anchoring on the separator is what makes this
# count widths rather than prefixes** — `.123` matches the first four characters
# of `.123456`, so an unanchored pattern would count every millisecond stamp as
# a pass and this gate would be green about nothing.
micros_re='52=[0-9]{8}-[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{6}\|'
millis_re='52=[0-9]{8}-[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}\|'
# `in ` is a frame that arrived at the C++ initiator, so it came from this
# engine; `out` is one QuickFIX sent. Direction matters: ADR-0057 built the
# first, and half A the second.
n_ours="$(grep -cE "^ *in .*${micros_re}" "${MICLOG}" || true)"
n_theirs="$(grep -cE "^ *out .*${micros_re}" "${MICLOG}" || true)"
n_millis="$(grep -cE "${millis_re}" "${MICLOG}" || true)"
n_reject="$(grep -cE '35=3\|.*371=52' "${MICLOG}" || true)"
echo "interop-micros: 24-byte 52= — ${n_ours} from fixbolt, ${n_theirs} from libquickfix"
echo "interop-micros: 21-byte 52= — ${n_millis} (must be 0)"
echo "interop-micros: 35=3 naming tag 52 — ${n_reject} (must be 0)"
if [[ "${n_ours}" -lt 1 ]]; then
  echo "MISSING: not one 24-byte 52= from this engine — TimestampPrecision=6 did not reach the wire" >&2
  mic_fail=1
fi
if [[ "${n_theirs}" -lt 1 ]]; then
  echo "MISSING: not one 24-byte 52= from libquickfix — the oracle was not configured" >&2
  mic_fail=1
fi
# **The assertion that says the two ends agree rather than merely both work.** A
# run where this engine stayed at 21 bytes still logs on, because QuickFIX
# accepts any width from 17 to 27 — so what separates the two outcomes is a
# count of what is NOT there.
if [[ "${n_millis}" -ne 0 ]]; then
  echo "UNEXPECTED: ${n_millis} millisecond 52= in a run where both ends asked for six digits" >&2
  mic_fail=1
fi
# `[measured 2026-09-09]` this is the assertion that earned this scenario its
# keep. Half A widened `session::clock::parse_utc` and left `dict`'s
# `UTCTIMESTAMP` reader at 8 or 12 bytes of time, so the first run of this block
# logged on and then answered every message after the Logon with
# `35=3 ... 371=52 373=6` — a Reject per Heartbeat, per SequenceReset, per
# Logout, and nothing in this repository could see it.
if [[ "${n_reject}" -ne 0 ]]; then
  echo "UNEXPECTED: ${n_reject} Reject naming tag 52 — a valid timestamp was refused" >&2
  mic_fail=1
fi
if ! grep -qE "^interop-acceptor: logon +ok" "${MICLOG}"; then
  echo "MISSING: the microsecond session never logged on" >&2
  mic_fail=1
fi

if [[ "${mic_fail}" -eq 0 ]]; then
  echo "interop-micros: PASS 5/5"
else
  echo "interop-micros: FAIL" >&2
  echo "---- what this engine's acceptor said ----" >&2
  cat "${WORK}/fixbolt-micros.log" >&2
  echo "---- what the C++ initiator said ----" >&2
  cat "${WORK}/interop-micros.log" >&2
  exit 1
fi

# ---- 4i. `52=` at a precision this engine cannot send -----------------------
#
# ADR-0058, and `STATUS.md` open item 59. Scenario 4h proved the two ends agree
# when both are configured for six digits. This one proves something the other
# scenarios structurally cannot: that a width **fixbolt never produces** is
# still understood when it arrives.
#
# **That is why the C++ end runs at `TimestampPrecision=2`.** This engine
# refuses that value on the way out — `settings.rs` takes 3, 6 or 9 and nothing
# else, ADR-0057 decision 1 — so the assertion below cannot be satisfied by this
# repository talking to itself, and no fixture here can produce the bytes under
# test. Only the oracle can.
#
# **What this looked like before ADR-0058, and why it is the item's real gate.**
# A 20-byte `52=` read as `None`, which is `time_ok = false`, which in
# `AwaitingLogon` is `Refusal::BadSendingTime` — a hang-up with **no byte sent**
# (`crates/session/src/lib.rs`). Not a Reject, not a Logout: silence. So the
# reversal of this gate is not a wrong count, it is a session that never comes
# up, and the assertion that catches it is `logon ok`.
echo
echo "==> [interop-odd] libquickfix at TimestampPrecision=2 on ${PORT5}"
mkdir -p "${WORK}/store5"

cat > "${WORK}/fixbolt-odd.cfg" <<CFG
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=FIXBOLT

[SESSION]
TargetCompID=QFODD
HeartBtInt=2
CFG

cat > "${WORK}/initiator-odd.cfg" <<CFG
[DEFAULT]
ConnectionType=initiator
SocketConnectHost=127.0.0.1
SocketConnectPort=${PORT5}
HeartBtInt=2
ReconnectInterval=1
ResetOnLogon=Y
ResetOnLogout=Y
ResetOnDisconnect=Y
StartTime=00:00:00
EndTime=00:00:00
UseDataDictionary=Y
DataDictionary=${SRC}/spec/FIX44.xml
FileStorePath=${WORK}/store5
TimestampPrecision=2

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFODD
TargetCompID=FIXBOLT
CFG

mkfifo "${WORK}/odd.ctl"
"${REPO_ROOT}/target/debug/interop" --role acceptor \
  --listen "127.0.0.1:${PORT5}" --cfg "${WORK}/fixbolt-odd.cfg" \
  < "${WORK}/odd.ctl" \
  > "${WORK}/fixbolt-odd.log" 2>&1 &
ODD_PID=$!
exec 9> "${WORK}/odd.ctl"

for _ in $(seq 1 200); do
  grep -q "interop: listening" "${WORK}/fixbolt-odd.log" && break
  sleep 0.1
done
if ! grep -q "interop: listening" "${WORK}/fixbolt-odd.log"; then
  echo "the odd-precision acceptor never became ready:" >&2
  cat "${WORK}/fixbolt-odd.log" >&2
  exit 1
fi

set +e
"${WORK}/initiator" "${WORK}/initiator-odd.cfg" --dump-tape \
  2>&1 | tee "${WORK}/interop-odd.log"
set -e

echo "stop" >&9 || true
for _ in $(seq 1 100); do
  kill -0 "${ODD_PID}" 2>/dev/null || break
  sleep 0.1
done
exec 9>&-
kill "${ODD_PID}" 2>/dev/null || true
wait "${ODD_PID}" 2>/dev/null || true
ODD_PID=""

ODDLOG="${WORK}/interop-odd.log"
odd_fail=0
echo "==> [interop-odd] reading ${ODDLOG}"
# A 20-byte `52=`: the seconds, a dot, TWO digits, then the separator. Anchored
# on the separator for the same reason 4h is — `.11` is a prefix of `.111`, and
# an unanchored pattern would count a millisecond stamp as a pass.
odd_re='52=[0-9]{8}-[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{2}\|'
n_odd="$(grep -cE "^ *out .*${odd_re}" "${ODDLOG}" || true)"
n_odd_reject="$(grep -cE '35=3\|.*371=52' "${ODDLOG}" || true)"
echo "interop-odd: 20-byte 52= from libquickfix — ${n_odd}"
echo "interop-odd: 35=3 naming tag 52 — ${n_odd_reject} (must be 0)"
# **The assertion that says the oracle really was configured.** Without it a run
# in which `TimestampPrecision=2` was ignored would look identical to a pass:
# the session would come up on 21-byte stamps and prove nothing at all.
if [[ "${n_odd}" -lt 1 ]]; then
  echo "MISSING: not one 20-byte 52= from libquickfix — the oracle was not configured, so nothing was tested" >&2
  odd_fail=1
fi
# **The assertion the item is about.** Before ADR-0058 this run never got here:
# the Logon carried a 20-byte `52=` and was answered with silence.
if ! grep -qE "^interop-acceptor: logon +ok" "${ODDLOG}"; then
  echo "MISSING: the odd-precision session never logged on — a valid 52= was refused before the Logon, in silence" >&2
  odd_fail=1
fi
# After the Logon the same width must not be Rejected either, which is the
# `dict` reader rather than `parse_utc` — the two-reader split of 2026-09-08.
if [[ "${n_odd_reject}" -ne 0 ]]; then
  echo "UNEXPECTED: ${n_odd_reject} Reject naming tag 52 — the second reader still refuses this width" >&2
  odd_fail=1
fi

if [[ "${odd_fail}" -eq 0 ]]; then
  echo "interop-odd: PASS 3/3"
else
  echo "interop-odd: FAIL" >&2
  echo "---- what this engine's acceptor said ----" >&2
  cat "${WORK}/fixbolt-odd.log" >&2
  echo "---- what the C++ initiator said ----" >&2
  cat "${WORK}/interop-odd.log" >&2
  exit 1
fi

# ---- 4j. `ResetOnLogon` judged over a socket, at BOTH values ----------------
#
# `STATUS.md` open item 53. The knob was proven at every layer and never on the
# wire, and the reason it was deferred is worth repeating because it is what
# this scenario had to work around: **an acceptor takes the `ResetOnLogon`
# branch only when its session was resumed**, and `fixbolt::serve` has no
# `Recovery` seam. So under every scenario above, `Y` and `N` produce byte-for-
# byte identical wire traffic, and a gate over them would be green about
# nothing.
#
# `--role acceptor --journal` is the seam, and it reuses the plumbing
# `--role reconnect` already had (`tools/interop/src/reconnect.rs`, d31db5e).
#
# **The C++ end is one-shot**, so this does not kill an acceptor mid-session the
# way 4d-4f do. It runs the initiator to completion, stops this engine, starts
# it again on the same journal, and runs the initiator a second time against the
# same `FileStorePath`. What is under test is the SECOND session.
#
# **All three `ResetOn*` are `N` on the C++ side, in both arms.** Same argument
# as the reconnect scenarios (see `acceptor-reconnect.cfg` above): under `Y` the
# oracle forgets its numbering at every logon, "did the session continue" stops
# being a question that can be asked, and a broken engine passes. Holding the
# C++ side identical across the two arms is also what makes the knob the only
# variable — `docs/reference/the-strongest-knob-is-not-the-settle-point.md` is
# about a gate that moved with a timeout and was really failing on something
# else entirely.
#
# `$1` is the value of `ResetOnLogon` on THIS engine. Everything else is held.
run_reset_on_logon() {
  local knob="$1"
  local work="${WORK}/reset-${knob}"
  mkdir -p "${work}/store"

  cat > "${work}/fixbolt.cfg" <<CFG
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=FIXBOLT

[SESSION]
TargetCompID=QFRST
HeartBtInt=30
ResetOnLogon=${knob}
CFG

  cat > "${work}/initiator.cfg" <<CFG
[DEFAULT]
ConnectionType=initiator
SocketConnectHost=127.0.0.1
SocketConnectPort=${PORT6}
HeartBtInt=30
ReconnectInterval=1
ResetOnLogon=N
ResetOnLogout=N
ResetOnDisconnect=N
StartTime=00:00:00
EndTime=00:00:00
UseDataDictionary=Y
DataDictionary=${SRC}/spec/FIX44.xml
FileStorePath=${work}/store

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFRST
TargetCompID=FIXBOLT
CFG

  # One half of the scenario: stand this engine up on the journal, run the
  # oracle against it, stop this engine cleanly. `$1` is the log suffix.
  #
  # **`stop` down a fifo, not a signal.** A `SIGKILL` here would leave the
  # journal's writer thread unjoined and the next `FileJournal::open` reading a
  # torn tail — which is a real failure mode, and it is `--role reconnect`'s
  # scenario, not this one. Confusing the two would make a `ResetOnLogon`
  # failure indistinguishable from a durability one.
  local half
  for half in 1 2; do
    mkfifo "${work}/ctl${half}"
    "${REPO_ROOT}/target/debug/interop" --role acceptor \
      --listen "127.0.0.1:${PORT6}" --cfg "${work}/fixbolt.cfg" \
      --journal "${work}/journal" \
      < "${work}/ctl${half}" \
      > "${work}/fixbolt${half}.log" 2>&1 &
    local pid=$!
    if [[ "${half}" == "1" ]]; then RST1_PID="${pid}"; else RST2_PID="${pid}"; fi
    exec 8> "${work}/ctl${half}"

    local ready=0
    for _ in $(seq 1 200); do
      # `2>/dev/null`: the file is created by the process that was just
      # spawned, so the first iteration can lose the race. `[measured
      # 2026-09-10]` without it a **passing** run printed
      # `grep: .../fixbolt2.log: No such file or directory` — a shell error
      # line inside a green job, the class PR #49 was about.
      grep -q "interop: listening" "${work}/fixbolt${half}.log" 2>/dev/null && { ready=1; break; }
      sleep 0.1
    done
    if [[ "${ready}" -eq 0 ]]; then
      echo "reset-${knob}: acceptor half ${half} never became ready:" >&2
      cat "${work}/fixbolt${half}.log" >&2
      exec 8>&-
      return 1
    fi

    # **Half 2 logs on and stops there.** The seven-step scenario is written
    # for a NEW session carrying `141=Y`; against a RESUMED one six of its
    # steps have no meaning, and `[measured 2026-09-10]` they read
    # `logon FAIL 141=MISSING`, `order FAIL` and `heartbeat FAIL` in both arms
    # while this scenario printed PASS beside them. See `run_logon_only` in
    # `tools/interop/initiator.cpp`.
    local only=""
    [[ "${half}" == "2" ]] && only="--logon-only"
    set +e
    "${WORK}/initiator" "${work}/initiator.cfg" --dump-tape ${only} \
      > "${work}/tape${half}.log" 2>&1
    set -e

    echo "stop" >&8 || true
    for _ in $(seq 1 100); do
      kill -0 "${pid}" 2>/dev/null || break
      sleep 0.1
    done
    exec 8>&-
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
    if [[ "${half}" == "1" ]]; then RST1_PID=""; else RST2_PID=""; fi
  done
  return 0
}

# The `34=` this engine put on its Logon, read off the frames that ARRIVED at
# the C++ initiator. `in ` is a frame the oracle received, so it came from here
# — reading our own `out` lines would be this engine grading itself, which
# ADR-0042 decision 1 is about.
fixbolt_logon_seq() {
  grep -E '^ *in .*\|35=A\|' "$1" 2>/dev/null | sed -nE 's/.*\|34=([0-9]+)\|.*/\1/p' | head -1
}

echo
echo "==> [interop-reset] ResetOnLogon over a socket, both values, on ${PORT6}"
reset_fail=0
# **Two plain variables, not an associative array.** `[measured 2026-09-10]`
# `declare -A` is a bash 4 feature and macOS ships bash 3.2: the first run of
# this scenario died on `declare: -A: invalid option` after the eight scenarios
# above had already passed. A gate that runs only on the CI machine is a gate
# the person writing the code cannot use.
reset_seq_N=""
reset_seq_Y=""

for knob in N Y; do
  if ! run_reset_on_logon "${knob}"; then
    echo "reset-${knob}: the arm did not run to completion" >&2
    reset_fail=1
    continue
  fi
  s1="$(fixbolt_logon_seq "${WORK}/reset-${knob}/tape1.log")"
  s2="$(fixbolt_logon_seq "${WORK}/reset-${knob}/tape2.log")"
  if [[ "${knob}" == "N" ]]; then reset_seq_N="${s2}"; else reset_seq_Y="${s2}"; fi
  echo "interop-reset: ResetOnLogon=${knob} — this engine's Logon 34= was ${s1:-none} then ${s2:-none}"

  # **The first session must be ordinary in both arms.** If it is not, the
  # second one is being compared against nothing, and a scenario that never
  # reached its own subject would still print two numbers.
  if [[ "${s1}" != "1" ]]; then
    echo "MISSING: reset-${knob} first session's Logon was 34=${s1:-none}, expected 1 — the arm did not start clean" >&2
    reset_fail=1
  fi
  if [[ -z "${s2}" ]]; then
    echo "MISSING: reset-${knob} second session never produced a Logon from this engine" >&2
    reset_fail=1
  fi
done

# **The assertion item 53 exists for.** Two runs that differ only in one line of
# one config file must not produce the same number. If they do, the knob was
# never under test — which is the exact state this scenario was written to end,
# and the reason `docs/reference/a-test-that-cannot-fail-reads-as-coverage.md`
# is in this repository.
if [[ "${reset_fail}" -eq 0 ]]; then
  if [[ "${reset_seq_N}" == "${reset_seq_Y}" ]]; then
    echo "UNCHANGED: ResetOnLogon=N and =Y both gave 34=${reset_seq_N} on the resumed session." >&2
    echo "A direction whose result does not change has not tested the knob." >&2
    reset_fail=1
  fi
  # `N` continues: the resumed session's Logon carries a number the first
  # session already passed.
  if [[ "${reset_seq_N}" -le 1 ]]; then
    echo "WRONG: ResetOnLogon=N gave 34=${reset_seq_N} on the resumed session — the numbering did not continue" >&2
    reset_fail=1
  fi
  # `Y` restarts, which is the whole meaning of the key.
  if [[ "${reset_seq_Y}" -ne 1 ]]; then
    echo "WRONG: ResetOnLogon=Y gave 34=${reset_seq_Y} on the resumed session — the count did not restart" >&2
    reset_fail=1
  fi
fi

if [[ "${reset_fail}" -eq 0 ]]; then
  echo "interop-reset: PASS 4/4"
else
  echo "interop-reset: FAIL" >&2
  for knob in N Y; do
    for half in 1 2; do
      echo "---- reset-${knob} acceptor half ${half} ----" >&2
      cat "${WORK}/reset-${knob}/fixbolt${half}.log" 2>/dev/null >&2 || true
      echo "---- reset-${knob} oracle tape ${half} ----" >&2
      cat "${WORK}/reset-${knob}/tape${half}.log" 2>/dev/null >&2 || true
    done
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
echo "interop: 7 / 7 + 8 / 8 + 6 / 6 + 6 / 6 + 6 / 6 + 9 / 9 + 5 / 5 + 3 / 3 + 4 / 4 against libquickfix @ ${PINNED_SHA}"
echo "both roles, three reconnect scenarios and 789 in both directions,"
echo "each checked by somebody else's engine"
