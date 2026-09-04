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
cleanup() {
  for pid in "${ACCEPTOR_PID}" "${FIXBOLT_PID}"; do
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
"${REPO_ROOT}/target/debug/interop" --role acceptor \
  --listen "127.0.0.1:${PORT2}" --cfg "${WORK}/fixbolt.cfg" \
  > "${WORK}/fixbolt-acceptor.log" 2>&1 &
FIXBOLT_PID=$!

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
kill "${FIXBOLT_PID}" 2>/dev/null || true
wait "${FIXBOLT_PID}" 2>/dev/null || true
FIXBOLT_PID=""

# ---- 4c. Read that output too. Every step, not only the PASS line. ---------
fail=0
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
echo "interop: 7 / 7 + 7 / 7 against libquickfix @ ${PINNED_SHA}"
echo "both roles, each checked by somebody else's engine"
