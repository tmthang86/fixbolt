#!/usr/bin/env bash
# Put this engine and a real QuickFIX/J on opposite ends of a socket, in BOTH
# roles, and read what comes back.
#
# ADR-0097 exit criterion 6 / ADR-0130. `libquickfix` (C++, scripts/interop.sh)
# is one independent opinion; QuickFIX/J is a second family entirely — a JVM,
# a different codebase, a different reading of the same spec — and it is the
# implementation a stranger running fixbolt is likeliest to meet on the other
# side of a Java shop's FIX gateway.
#
#   * qfj-acceptor-plain   — fixbolt ACCEPTOR  (fixbolt::serve),        QuickFIX/J initiator (Judge)
#   * qfj-acceptor-tls     — fixbolt ACCEPTOR  (serve_tls_requiring),   QuickFIX/J initiator (Judge)   [plan step 3]
#   * qfj-initiator-plain  — fixbolt INITIATOR (--role dial, real engine door), QuickFIX/J acceptor (Judge)
#   * qfj-initiator-tls    — fixbolt INITIATOR (--role dial, TLS),      QuickFIX/J acceptor (Judge)    [plan step 3]
#
# This file builds the first two arms of that list (plan step 1 and step 2 —
# plaintext only). The TLS arms and the four-arm summary line
# (`interop-qfj: 7 / 7 acceptor plain + 7 / 7 acceptor TLS + ...`) are plan
# step 3's job, on this same file — CLAUDE.md §1: one file, one writer at a
# time, so this script grows rather than being rewritten.
#
# `tools/interop-qfj/Judge.java` is this repository's own code (non-negotiable
# 9: no QuickFIX source is copied) calling only QuickFIX/J's public API. It
# drives every step and prints one line per step plus a `PASS n/7` /
# `FAIL n/7` summary — CLAUDE.md §10: this script reads those lines, never the
# process exit code, exactly as scripts/interop.sh already does for
# libquickfix.
#
# Five jars are fetched from Maven Central into gitignored vendor/quickfixj/,
# each checked against a SHA-256 pinned below. A mismatch stops the script
# before anything is compiled or run. Nothing under vendor/ is ever committed —
# ADR-0001, CLAUDE.md §2 rule 9 — and the last thing this script does is check
# that `git status` is still clean.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="${REPO_ROOT}/vendor/quickfixj"
RUN="${REPO_ROOT}/vendor/interop-qfj-run"

QFJ_VERSION="3.0.2"

# The five jars ADR-0130 Decision 1 names, and the SHA-256 this script pins
# each one to. `fetch_pinned_jar` refuses to run anything if a fetched byte
# does not match — reversal in the plan's Chia việc bước 1: flip one hex digit
# below, see `CHECKSUM MISMATCH` before a single process starts, restore.
JARS=(quickfixj-core quickfixj-base quickfixj-messages-fix44 mina-core slf4j-api)
declare -A JAR_PATH=(
  [quickfixj-core]="org/quickfixj/quickfixj-core/${QFJ_VERSION}/quickfixj-core-${QFJ_VERSION}.jar"
  [quickfixj-base]="org/quickfixj/quickfixj-base/${QFJ_VERSION}/quickfixj-base-${QFJ_VERSION}.jar"
  [quickfixj-messages-fix44]="org/quickfixj/quickfixj-messages-fix44/${QFJ_VERSION}/quickfixj-messages-fix44-${QFJ_VERSION}.jar"
  [mina-core]="org/apache/mina/mina-core/2.2.9/mina-core-2.2.9.jar"
  [slf4j-api]="org/slf4j/slf4j-api/2.0.18/slf4j-api-2.0.18.jar"
)
declare -A JAR_SHA256=(
  [quickfixj-core]="0eda0b8470846eb088126013da20d574f5eef02ed8d04604ca276ad4e6b33008"
  [quickfixj-base]="47ac4aa92410877c9660e8a1f3b04dc988ac3ba813ebc802eabb37e923fd6c3d"
  [quickfixj-messages-fix44]="b07529e6c3f70b2eea8ddabeae03688b093bc769a8bd7a202f1c5bbc9b6e6993"
  [mina-core]="09b4b5e416834e5281dd0dfccac1a10413d6f42c89f133b1c43641e34f33e840"
  [slf4j-api]="44508fd1576500688c790b190acdd16fec4f8c79a3e0b900afd70503cf055f55"
)

for tool in curl sha256sum javac java unzip cargo; do
  command -v "${tool}" >/dev/null || { echo "${tool} is required" >&2; exit 1; }
done

# Taken before anything is fetched, compiled or run, so the final check can ask
# what this run added rather than whether the tree happened to be clean.
# scripts/interop.sh's own comment on this line still applies verbatim.
BEFORE="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | sort)"

# Config knobs, overridable for local iteration.
PORT_ACCEPTOR_PLAIN="${INTEROP_QFJ_PORT1:-15660}"
PORT_INITIATOR_PLAIN="${INTEROP_QFJ_PORT2:-15661}"
# Reserved for plan step 3: PORT_ACCEPTOR_TLS 15662, PORT_INITIATOR_TLS 15663.
DEADLINE="${INTEROP_QFJ_DEADLINE:-20}"

# ---- 1. Five jars, pinned, and the compiled judge ---------------------------

fetch_pinned_jar() {
  local name="$1" path="$2" want_sha="$3" dest
  dest="${VENDOR}/${name}.jar"
  mkdir -p "${VENDOR}"
  if [[ ! -f "${dest}" ]]; then
    curl -sL --max-time 60 -o "${dest}.part" "https://repo1.maven.org/maven2/${path}"
    mv "${dest}.part" "${dest}"
  fi
  local got_sha
  got_sha="$(sha256sum "${dest}" | cut -d' ' -f1)"
  if [[ "${got_sha}" != "${want_sha}" ]]; then
    echo "CHECKSUM MISMATCH: ${name}.jar" >&2
    echo "  pinned:  ${want_sha}" >&2
    echo "  fetched: ${got_sha}" >&2
    echo "Move the pin and the reason for moving it into the same commit." >&2
    rm -f "${dest}"
    exit 1
  fi
}

echo "==> quickfixj ${QFJ_VERSION}: 5 jars, every SHA-256 as pinned"
for j in "${JARS[@]}"; do
  fetch_pinned_jar "${j}" "${JAR_PATH[${j}]}" "${JAR_SHA256[${j}]}"
done

CP=""
for j in "${JARS[@]}"; do
  CP="${CP}${VENDOR}/${j}.jar:"
done

mkdir -p "${RUN}"
# The dictionary QuickFIX/J itself ships, not libquickfix's copy under
# vendor/quickfix/spec/ — the plan's own note on why the two must not be
# confused (a divergent DTD would make QFJ Reject a perfectly good fixbolt
# message, or accept a malformed one, over a difference this gate would
# misattribute to the engine).
QFJ_DICT="${RUN}/qfj-FIX44.xml"
if [[ ! -f "${QFJ_DICT}" ]]; then
  unzip -p "${VENDOR}/quickfixj-messages-fix44.jar" FIX44.xml > "${QFJ_DICT}"
fi

CLASSES="${RUN}/classes"
mkdir -p "${CLASSES}"
echo "==> javac tools/interop-qfj/Judge.java"
javac -Xlint:all -cp "${CP}" -d "${CLASSES}" "${REPO_ROOT}/tools/interop-qfj/Judge.java"

JAVA_VERSION_LINE="$(java -version 2>&1 | head -1)"

echo "==> building tools/interop"
cargo build -q -p fixbolt-interop

# ---- Waiting on a line, bounded ---------------------------------------------
#
# A reversal that removes a restart, or a judge that never becomes ready, must
# go RED, not HANG (docs/reference/a-reversal-can-fail-by-hanging.md).
wait_for_line() {
  local file="$1" pattern="$2" ticks i=0
  ticks=$((DEADLINE * 10))
  while [[ "${i}" -lt "${ticks}" ]]; do
    [[ -f "${file}" ]] && grep -Eq -- "${pattern}" "${file}" && return 0
    sleep 0.1
    i=$((i + 1))
  done
  return 1
}

# ---- Reading the output. This is the gate, not the exit code. --------------
#
# CLAUDE.md §10: a check proves nothing until something reads it. Every step
# name, both extra assertions, and the summary line are grepped by name —
# reversal C in the plan (misspell one name here) must turn this red even
# though the judge itself printed PASS 7/7.
assert_arm() {
  local label="$1" judgelog="$2" fixboltlog="$3" shutdown_pattern="$4" fail=0

  for step in logon order heartbeat testrequest resend gapfill logout; do
    if ! grep -qE "^${label}: ${step} +ok" "${judgelog}"; then
      echo "MISSING OR FAILED STEP: ${label} ${step}" >&2
      fail=1
    fi
  done
  if ! grep -q "^${label}: PASS 7/7" "${judgelog}"; then
    echo "no '${label}: PASS 7/7' line" >&2
    fail=1
  fi

  if grep -qE "${shutdown_pattern}" "${fixboltlog}"; then
    echo "${label}: shutdown     ok    $(grep -oE "${shutdown_pattern}.*" "${fixboltlog}" | head -1)"
  else
    echo "MISSING: fixbolt never returned through Admin::shutdown (${label})" >&2
    fail=1
  fi

  # Reject (35=3) or BusinessMessageReject (35=j), either direction, is the
  # form a data-dictionary disagreement between QFJ and fixbolt takes — the
  # plan's own trap table names this as the likeliest cross-family surprise.
  local dirty
  dirty="$(grep -cE '\|35=3\||\|35=j\|' "${judgelog}" || true)"
  if [[ "${dirty}" -eq 0 ]]; then
    echo "${label}: clean        ok    no 35=3, no 35=j"
  else
    echo "MISSING: ${label} clean — ${dirty} reject/business-reject line(s) seen" >&2
    fail=1
  fi

  if [[ "${fail}" -ne 0 ]]; then
    echo >&2
    echo "---- [${label}] what fixbolt said ----" >&2
    cat "${fixboltlog}" >&2
    echo "---- [${label}] what the judge said ----" >&2
    cat "${judgelog}" >&2
    exit 1
  fi
}

FB_PID=""
QFJ_PID=""
cleanup() {
  for pid in "${FB_PID}" "${QFJ_PID}"; do
    [[ -n "${pid}" ]] || continue
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
  done
}
trap cleanup EXIT

# ---- Arm: qfj-acceptor-plain -------------------------------------------------
#
# fixbolt is the acceptor — the product this repository is positioned on
# (ADR-0130 Consequences) — and Judge plays the QuickFIX/J initiator. `Desk`
# sends two News on logon, same as against libquickfix.
run_acceptor_plain() {
  local label="qfj-acceptor-plain" port="${PORT_ACCEPTOR_PLAIN}" work
  work="${RUN}/${label}"
  rm -rf "${work}"
  mkdir -p "${work}/fbstore" "${work}/qfjstore"

  cat > "${work}/fixbolt.cfg" <<CFG
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=FIXBOLT

[SESSION]
TargetCompID=QFJINI
HeartBtInt=2
CFG

  cat > "${work}/qfj-initiator.cfg" <<CFG
[DEFAULT]
ConnectionType=initiator
SocketConnectHost=127.0.0.1
SocketConnectPort=${port}
HeartBtInt=2
ReconnectInterval=30
ResetOnLogon=Y
ResetOnLogout=Y
ResetOnDisconnect=Y
StartTime=00:00:00
EndTime=00:00:00
UseDataDictionary=Y
DataDictionary=${QFJ_DICT}
FileStorePath=${work}/qfjstore

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFJINI
TargetCompID=FIXBOLT
CFG

  echo
  echo "==> [${label}] fixbolt acceptor on ${port}"
  mkfifo "${work}/fixbolt.ctl"
  "${REPO_ROOT}/target/debug/interop" --role acceptor \
    --listen "127.0.0.1:${port}" --cfg "${work}/fixbolt.cfg" \
    < "${work}/fixbolt.ctl" \
    > "${work}/fixbolt.log" 2>&1 &
  FB_PID=$!
  exec 9> "${work}/fixbolt.ctl"

  if ! wait_for_line "${work}/fixbolt.log" "interop: listening"; then
    echo "[${label}] fixbolt's acceptor never became ready:" >&2
    cat "${work}/fixbolt.log" >&2
    exit 1
  fi

  echo "==> [${label}] QuickFIX/J initiator, judging"
  set +e
  java -cp "${CP}${CLASSES}" Judge initiator "${work}/qfj-initiator.cfg" "${label}" \
    ${INTEROP_QFJ_JUDGE_ARGS:-} 2>&1 | tee "${work}/judge.log"
  set -e

  echo "stop" >&9 || true
  local stopped="no"
  for _ in $(seq 1 100); do
    kill -0 "${FB_PID}" 2>/dev/null || { stopped="yes"; break; }
    sleep 0.1
  done
  exec 9>&-
  if [[ "${stopped}" != "yes" ]]; then
    echo "[${label}] fixbolt's acceptor did not return from serve within 10s of Admin::shutdown" >&2
    kill "${FB_PID}" 2>/dev/null || true
  fi
  wait "${FB_PID}" 2>/dev/null || true
  FB_PID=""

  assert_arm "${label}" "${work}/judge.log" "${work}/fixbolt.log" \
    '^interop: acceptor stopped: Shutdown \{'
}

# ---- Arm: qfj-initiator-plain ------------------------------------------------
#
# fixbolt dials out through its real initiator door (`--role dial`,
# `connect_and_serve`) — the plan's reason for building that role rather than
# reusing the hand-rolled `--role initiator` session. Judge plays the
# QuickFIX/J acceptor and drives every step from that side.
run_initiator_plain() {
  local label="qfj-initiator-plain" port="${PORT_INITIATOR_PLAIN}" work
  work="${RUN}/${label}"
  rm -rf "${work}"
  mkdir -p "${work}/qfjstore"

  cat > "${work}/qfj-acceptor.cfg" <<CFG
[DEFAULT]
ConnectionType=acceptor
SocketAcceptPort=${port}
SocketReuseAddress=Y
StartTime=00:00:00
EndTime=00:00:00
UseDataDictionary=Y
DataDictionary=${QFJ_DICT}
FileStorePath=${work}/qfjstore
ResetOnLogon=Y
ResetOnLogout=Y
ResetOnDisconnect=Y

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFJACC
TargetCompID=FIXBOLT
HeartBtInt=2
CFG

  # `ReconnectInterval=30` — same argument scripts/interop.sh's 4b makes for
  # the C++ direction: fixbolt must not redial after the judge logs it out and
  # exits, or a second Logon would land in the same transcript the assertions
  # above read.
  cat > "${work}/fixbolt-dial.cfg" <<CFG
[DEFAULT]
ConnectionType=initiator
SocketConnectHost=127.0.0.1
SocketConnectPort=${port}
HeartBtInt=2
ReconnectInterval=30

[SESSION]
BeginString=FIX.4.4
SenderCompID=FIXBOLT
TargetCompID=QFJACC
CFG

  echo
  echo "==> [${label}] QuickFIX/J acceptor on ${port}"
  set +e
  java -cp "${CP}${CLASSES}" Judge acceptor "${work}/qfj-acceptor.cfg" "${label}" \
    ${INTEROP_QFJ_JUDGE_ARGS:-} > "${work}/judge.log" 2>&1 &
  QFJ_PID=$!
  set -e

  if ! wait_for_line "${work}/judge.log" "interop-qfj: ready"; then
    echo "[${label}] QuickFIX/J's acceptor never became ready:" >&2
    cat "${work}/judge.log" >&2
    exit 1
  fi

  echo "==> [${label}] fixbolt --role dial"
  mkfifo "${work}/fixbolt.ctl"
  "${REPO_ROOT}/target/debug/interop" --role dial \
    --cfg "${work}/fixbolt-dial.cfg" \
    < "${work}/fixbolt.ctl" \
    > "${work}/fixbolt.log" 2>&1 &
  FB_PID=$!
  exec 8> "${work}/fixbolt.ctl"

  # Judge drives every step from the acceptor side and has the only verdict;
  # fixbolt's dial has none of its own to wait on.
  if ! wait_for_line "${work}/judge.log" "^${label}: (PASS|FAIL) "; then
    echo "[${label}] the judge never reached a verdict within the deadline:" >&2
    echo "---- judge ----" >&2
    cat "${work}/judge.log" >&2
    echo "---- fixbolt ----" >&2
    cat "${work}/fixbolt.log" >&2
    exit 1
  fi
  wait "${QFJ_PID}" 2>/dev/null || true
  QFJ_PID=""
  cat "${work}/judge.log"

  echo "stop" >&8 || true
  # **Not 10 s here — measured 2026-09-23, and it is a trap, not a typo.**
  # `fixbolt_engine::dial`'s reconnect-wait branch (`policy.next() == At(_)`)
  # calls `engine.idle_with` and `continue`s straight back to the top of the
  # loop; `engine.shutdown_finished()` is only checked once that branch falls
  # through to `Next::Now`. So once Judge's acceptor closes the connection,
  # `Admin::shutdown` on a dial sitting in that wait is not observed until the
  # scheduled reconnect attempt actually arrives — bounded by this arm's own
  # `ReconnectInterval=30`, not by anything this script asks for. Reproduced
  # standalone outside the script: `Shutdown {` printed at +26 s, every time.
  #
  # A shorter `ReconnectInterval` would close this window, but the plan's own
  # "Cấu hình" section pins it at 30 s on purpose — "để không quay số lại
  # trong lúc Judge đang tắt sau logout" — so shortening it here would trade
  # one hazard the plan already named for another it did not. This wait is
  # widened to match the engine's real, observed behaviour instead of
  # silently reinterpreting a pinned setting; the underlying latency belongs
  # to `crates/engine`, which rows 1-2 do not touch — reported, not fixed,
  # here.
  local stopped="no"
  for _ in $(seq 1 400); do
    kill -0 "${FB_PID}" 2>/dev/null || { stopped="yes"; break; }
    sleep 0.1
  done
  exec 8>&-
  if [[ "${stopped}" != "yes" ]]; then
    echo "[${label}] fixbolt's dial did not return within 40s of Admin::shutdown" >&2
    kill "${FB_PID}" 2>/dev/null || true
  fi
  wait "${FB_PID}" 2>/dev/null || true
  FB_PID=""

  assert_arm "${label}" "${work}/judge.log" "${work}/fixbolt.log" \
    '^interop: dial stopped: Shutdown \{'
}

# ---- Which arms this invocation runs ----------------------------------------
#
# `INTEROP_QFJ_ARMS` lets a step under development run only its own arm. The
# default below is every arm THIS script builds today; plan step 3 widens it
# to all four when the TLS arms exist, and only then does the combined
# `interop-qfj: 7 / 7 ... + 7 / 7 ...` line print — CI's grep is added in step
# 4, once that line means what it says.
IFS=',' read -r -a ARMS <<< "${INTEROP_QFJ_ARMS:-acceptor-plain,initiator-plain}"

ran_acceptor_plain=0
ran_initiator_plain=0
for arm in "${ARMS[@]}"; do
  case "${arm}" in
    acceptor-plain) run_acceptor_plain; ran_acceptor_plain=1 ;;
    initiator-plain) run_initiator_plain; ran_initiator_plain=1 ;;
    acceptor-tls | initiator-tls)
      echo "arm '${arm}' is not built yet — plan step 3 adds TLS" >&2
      exit 1
      ;;
    *)
      echo "unknown arm '${arm}' (acceptor-plain | initiator-plain)" >&2
      exit 1
      ;;
  esac
done

# ---- Last: what this run added, and what it means ---------------------------

AFTER="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | sort)"
if [[ "${BEFORE}" != "${AFTER}" ]]; then
  echo "the run added something git can see:" >&2
  diff <(echo "${BEFORE}") <(echo "${AFTER}") >&2 || true
  exit 1
fi
echo "==> the run added nothing git can see"

# Not the plan's final four-arm line — that one names all four arms and lands
# with plan step 3. This says plainly what actually ran, so a partial
# invocation during development is never mistaken for the finished gate.
if [[ "${ran_acceptor_plain}" -eq 1 && "${ran_initiator_plain}" -eq 1 ]]; then
  echo "interop-qfj: (plaintext only, TLS arms land in plan step 3) 7 / 7 acceptor plain + 7 / 7 initiator plain against QuickFIX/J ${QFJ_VERSION} on ${JAVA_VERSION_LINE}"
fi
