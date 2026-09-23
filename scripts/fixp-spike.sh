#!/usr/bin/env bash
# The FIXP spike (phase 3 row 9, docs/plans/2026-09-23-p3-fixp-spike.md, ADR-0140): pin and fetch
# Artio's Binary EntryPoint jars, extract the schema they actually decode with, build the Rust
# probe (spikes/fixp-probe) against that schema, and run it against this repository's own referee
# (spikes/fixp-probe/referee/Referee.java) — Artio's acceptor behind our authentication strategy.
#
# Arms (FIXP_SPIKE_ARMS, default: the three below; an unknown name fails loudly):
#   referee-only        the referee starts headless, is OBSERVED listening (FixEngine.launch binds
#                       the acceptor socket synchronously — see the trap at the top of
#                       Referee.java), runs for its own deadline, and stops on its own (row 1).
#   accept              Negotiate -> NegotiateResponse -> Establish -> EstablishAck -> Terminate ->
#                       Terminate -> EOF: five probe lines `accept: <step> ok`, and the referee's
#                       seven `referee: field <name> ok <value>` lines, each value held against
#                       what the probe was told to send (row 3).
#   reject-timestamp    a Negotiate an hour old: `NegotiateReject INVALID_TIMESTAMP(7)`, Artio's
#                       own check, after the referee accepted all seven fields (ADR-0140 dec. 6).
#   reject-credentials  credentials the referee does not expect: `referee: field credentials
#                       MISMATCH`, then `NegotiateReject CREDENTIALS(1)` (ADR-0140 decision 6).
#
# Only the printed lines decide (CLAUDE.md §7: read the output, not the exit status). The summary
#   fixp-spike: accept PASS 5/5, reject-timestamp PASS, reject-credentials PASS
# is printed only when all three of those arms ran and passed; nothing else prints it.
#
# ADR-0140 decision 3: no B3 schema byte and no jar is ever committed. Both land under
# vendor/fixp/, which /vendor/ already gitignores wholesale (ADR-0001's rule for third-party
# assets). This script's own last act is checking that nothing it did shows up in `git status`.
#
# ADR-0140 decision 2: the referee is our own code on Artio's public API only — the pattern of
# scripts/interop.sh (pin by SHA-256, fail before anything runs) and spikes/ktls (a crate/program
# detached from anything CI builds by default).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="${REPO_ROOT}/vendor/fixp"
JAR_DIR="${VENDOR_DIR}/jars"
SCHEMA_FILE="${VENDOR_DIR}/binary_entrypoint.xml"
CLASSES_DIR="${VENDOR_DIR}/classes"
REFEREE_SRC="${REPO_ROOT}/spikes/fixp-probe/referee/Referee.java"

PROBE_DIR="${REPO_ROOT}/spikes/fixp-probe"
PROBE_BIN="${PROBE_DIR}/target/debug/fixp-probe"

ARMS="${FIXP_SPIKE_ARMS:-accept reject-timestamp reject-credentials}"
# Empty is the only default: each arm takes a port the kernel hands out fresh — see pick_port
# below. FIXP_REFEREE_PORT overrides that with one fixed port for every arm, at the caller's risk;
# it must not be 15660-15663 (scripts/interop-qfj.sh's fixed PORT_ACCEPTOR_PLAIN /
# PORT_INITIATOR_PLAIN / PORT_ACCEPTOR_TLS / PORT_INITIATOR_TLS) — a parallel run of both scripts
# on that range binds the same port from two directions and one side fails with a bind error that
# reads as an interop regression rather than a port clash
# (docs/reference/b3-binary-entrypoint-facts.md names this trap).
PORT_OVERRIDE="${FIXP_REFEREE_PORT:-}"
ARCHIVE_CONTROL_PORT="${FIXP_REFEREE_ARCHIVE_CONTROL_PORT:-10010}"
ARCHIVE_RESPONSE_PORT="${FIXP_REFEREE_ARCHIVE_RESPONSE_PORT:-10020}"
DEADLINE_SECONDS="${FIXP_REFEREE_DEADLINE_SECONDS:-5}"
# A probe arm ends the referee through its stop file; this is only the ceiling if that never comes.
ARM_REFEREE_CEILING_SECONDS="${FIXP_ARM_REFEREE_CEILING_SECONDS:-60}"
# Every read the probe makes has this deadline (the plan's "Probe đọc không hạn chót" trap).
PROBE_DEADLINE_MS="${FIXP_PROBE_DEADLINE_MS:-5000}"
# How long to wait for the referee's "library connected" line before giving up on the arm.
REFEREE_READY_SECONDS="${FIXP_REFEREE_READY_SECONDS:-60}"

# ---- The session identity, said once --------------------------------------------------------------
# The referee is told to EXPECT these; the probe is told to SEND them (reject-credentials alone
# sends a different credentials value). The script then holds each value the referee prints as
# RECEIVED against what the probe was told to send.
#
# Every fixed-width field carries a value with NO ZERO BYTE at its declared width (the review of
# PR #102: with enteringFirm 77 and onbehalfFirm null, an encoder writing Firm as uint16 stayed
# green — the high bytes it dropped were zero anyway). A value that fills its width cannot survive
# a narrowed encoder (the writer refuses it, or the bytes on the wire move), and a widened one
# shifts every later field. keepAliveInterval alone cannot: Artio caps it at KEEPALIVE_MAX_MS.
SESSION_ID=287454020                     # uint32 0x11223344
SESSION_VER_ID=72623859790382856         # uint64 0x0102030405060708
ENTERING_FIRM=555885348                  # uint32 0x21222324
ONBEHALF_FIRM=825373492                  # uint32 0x31323334 (FirmOptional; 0 would be null)
NEXT_SEQ_NO=1094861636                   # uint32 0x41424344 (Artio accepts any >= 1, and echoes it)
COD_TYPE=1                               # uint8 CANCEL_ON_DISCONNECT_ONLY (the Terminate is a logout,
                                         # so Artio never arms it here)
COD_TIMEOUT_MS=5859837686836516696       # uint64 0x5152535455565758 (Artio clamps it; never armed)
TERMINATION_CODE=1                       # uint8 TerminationCode.FINISHED
CREDENTIALS="fixbolt-spike-credentials"
CLIENT_IP="127.0.0.1"
CLIENT_APP_NAME="fixbolt-fixp-probe"
CLIENT_APP_VERSION="0.0.0-spike"
WRONG_CREDENTIALS="${CREDENTIALS}-wrong"
# Establish.keepAliveInterval the probe sends: inside Artio's [min, max] below.
PROBE_KEEPALIVE_MS=10000
# The referee's limits, set explicitly and printed by it (plan trap: keep-alive out of bounds ->
# EstablishReject). ACCEPTOR_KEEPALIVE_MS is what EstablishAck.keepAliveInterval must carry.
SENDING_TIME_WINDOW_MS=120000
KEEPALIVE_MIN_MS=1
KEEPALIVE_MAX_MS=60000
ACCEPTOR_KEEPALIVE_MS=30000

for tool in curl sha256sum javac java git comm mktemp python3; do
  command -v "${tool}" >/dev/null || { echo "${tool} is required" >&2; exit 1; }
done

# Taken before anything is fetched, built or run, so the last step can ask what THIS run added
# rather than whether the tree happened to already be clean (scripts/interop.sh's pattern).
BEFORE="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | LC_ALL=C sort)"

echo "==> fixp-spike: arms=[${ARMS}] port=${PORT_OVERRIDE:-fresh per arm} archive-control=${ARCHIVE_CONTROL_PORT} archive-response=${ARCHIVE_RESPONSE_PORT} deadline=${DEADLINE_SECONDS}s"

# ---- 1. The 11 jars, pinned by SHA-256 (docs/plans/2026-09-23-p3-fixp-spike.md, "Jar cần ghim") -
# Maven Central path -> pinned SHA-256. Flip one hex digit here and the jar checked against a
# wrong pin must fail with CHECKSUM MISMATCH before a single byte is compiled or run — that is
# the row 1 reversal.
declare -A PINNED_JARS=(
  ["uk/co/real-logic/artio-binary-entrypoint-impl/0.184/artio-binary-entrypoint-impl-0.184.jar"]="38f26226fc09e0b99f7a5a0108e04e9ca8fb1396b1817f94f38d42a408cc1c02"
  ["uk/co/real-logic/artio-binary-entrypoint-codecs/0.184/artio-binary-entrypoint-codecs-0.184.jar"]="570c093e41a94ec2a1ff12631634c300a242f42cdb7d43b5d693ac736aba1931"
  ["uk/co/real-logic/artio-core/0.184/artio-core-0.184.jar"]="8deebebb80a19887e74e2272a53b6a19717d183c5ebbe26aa69010a041382d7c"
  ["uk/co/real-logic/artio-codecs/0.184/artio-codecs-0.184.jar"]="ceb176667681ccc78ad9ee5a6655627d4ea32c6faa069269248ee87780bcd85f"
  ["io/aeron/aeron-client/1.53.2/aeron-client-1.53.2.jar"]="1386ce46b2bb89b4694a63c76b19e452808b6ee446380b54f2374b72a65e4ff8"
  ["io/aeron/aeron-driver/1.53.2/aeron-driver-1.53.2.jar"]="ab102b05f064fc394c661e0fba23c09817a47775d26a3b87fb331597b8a4ac28"
  ["io/aeron/aeron-archive/1.53.2/aeron-archive-1.53.2.jar"]="ca8f988ba875b7936fd01df1b77ff01fa168f49a21e24560c2acdf40b07193c1"
  ["io/aeron/aeron-annotations/1.53.2/aeron-annotations-1.53.2.jar"]="e2e336ab3865e9fd6b08d6ca8202bda2364a8af30b48ec06cc2b1643bc3c0bc9"
  ["org/agrona/agrona/2.6.1/agrona-2.6.1.jar"]="84c07eb02695c06bfda3d6c3a20c1984e3ff1bbd617b2f8e678140baf973fa28"
  ["uk/co/real-logic/sbe-tool/1.40.2/sbe-tool-1.40.2.jar"]="df2556a61a199383952030a6cd7fd2e3a82c3eb835bf4d5bc6fe3ca11295d058"
  ["org/hdrhistogram/HdrHistogram/2.2.2/HdrHistogram-2.2.2.jar"]="22d1d4316c4ec13a68b559e98c8256d69071593731da96136640f864fa14fad8"
)
# The one jar row 1 needs a name for, to extract the schema out of it below.
CODECS_JAR="artio-binary-entrypoint-codecs-0.184.jar"
SCHEMA_PATH_IN_JAR="uk/co/real_logic/artio/entrypoint/binary_entrypoint.xml"
SCHEMA_SHA256="c31fcd6228e613fa6ee3af441832393a4a35c30263bb5cd80929f16529af4a71"

mkdir -p "${JAR_DIR}"

echo "==> pinning ${#PINNED_JARS[@]} jars from Maven Central"
for path in "${!PINNED_JARS[@]}"; do
  fname="$(basename "${path}")"
  dest="${JAR_DIR}/${fname}"
  want="${PINNED_JARS[${path}]}"

  if [[ ! -f "${dest}" ]]; then
    curl -fsSL --max-time 60 -o "${dest}.part" "https://repo1.maven.org/maven2/${path}"
    mv "${dest}.part" "${dest}"
  fi

  got="$(sha256sum "${dest}" | cut -d' ' -f1)"
  if [[ "${got}" != "${want}" ]]; then
    echo "CHECKSUM MISMATCH: ${fname}" >&2
    echo "  pinned:      ${want}" >&2
    echo "  downloaded:  ${got}" >&2
    echo "Nothing was compiled or run. Either move the pin above deliberately, having read what" >&2
    echo "changed, or the file under ${JAR_DIR} is stale — delete it and re-run." >&2
    rm -f "${dest}"
    exit 1
  fi
done
echo "==> all ${#PINNED_JARS[@]} jars match their pinned SHA-256"

# ---- 2. The schema the referee actually decodes with, extracted and pinned the same way -------
echo "==> extracting ${SCHEMA_PATH_IN_JAR} from ${CODECS_JAR}"
# Read straight out of the jar (a zip) into its place — no scratch directory to extract into and
# `cd` through, so nothing here is a directory scripts/check-scratch-fixtures.sh has to ask about.
python3 - "${JAR_DIR}/${CODECS_JAR}" "${SCHEMA_PATH_IN_JAR}" "${SCHEMA_FILE}" <<'PY'
import sys, zipfile
jar, member, out = sys.argv[1:]
with zipfile.ZipFile(jar) as z, open(out, "wb") as f:
    f.write(z.read(member))
PY

SCHEMA_GOT="$(sha256sum "${SCHEMA_FILE}" | cut -d' ' -f1)"
if [[ "${SCHEMA_GOT}" != "${SCHEMA_SHA256}" ]]; then
  echo "CHECKSUM MISMATCH: binary_entrypoint.xml" >&2
  echo "  pinned:      ${SCHEMA_SHA256}" >&2
  echo "  extracted:   ${SCHEMA_GOT}" >&2
  echo "Nothing was compiled or run. Either ${CODECS_JAR} changed contents at the same version" >&2
  echo "(it should not have) or the pin above was moved without checking what changed." >&2
  rm -f "${SCHEMA_FILE}"
  exit 1
fi
echo "==> binary_entrypoint.xml matches its pinned SHA-256 ($(wc -c < "${SCHEMA_FILE}") bytes)"

# ---- 3. Compile the referee --------------------------------------------------------------------
CLASSPATH="$(find "${JAR_DIR}" -name '*.jar' -printf '%p:')"
mkdir -p "${CLASSES_DIR}"
echo "==> compiling ${REFEREE_SRC#"${REPO_ROOT}"/}"
javac -proc:none -Xlint:all -cp "${CLASSPATH}" -d "${CLASSES_DIR}" "${REFEREE_SRC}"

JAVA_ADD_OPENS=(
  --add-opens java.base/sun.nio.ch=ALL-UNNAMED
  --add-opens java.base/jdk.internal.misc=ALL-UNNAMED
  --add-opens java.base/java.util.zip=ALL-UNNAMED
)

# A port for one arm's acceptor. Artio 0.184 binds it without SO_REUSEADDR
# (DefaultTcpChannelSupplier.bind -> ServerSocketChannel.bind; JDK NIO does not set it), and Artio
# closes first — after echoing Terminate, and at its authentication timeout after a library-level
# NegotiateReject — so the port it listened on is left in TIME_WAIT for about a minute, and the next
# referee on that port fails in FixEngine.launch with `BindException: Address already in use`
# [measured 2026-09-23 on this desk: the third arm of the first run on one fixed port]. So each arm
# asks the kernel for a port that is bindable now, without SO_REUSEADDR, exactly as Artio will bind
# it, and prints it. FIXP_REFEREE_PORT pins one port for every arm instead, at the caller's risk.
pick_port() {
  if [[ -n "${PORT_OVERRIDE}" ]]; then
    echo "${PORT_OVERRIDE}"
    return
  fi
  # IPv6 first, bound to the v4-mapped loopback, because that is the socket a JVM opens for
  # 127.0.0.1 on a dual-stack host; plain IPv4 where there is no IPv6.
  python3 - <<'PY'
import socket
try:
    s = socket.socket(socket.AF_INET6)
    s.bind(("::ffff:127.0.0.1", 0))
except OSError:
    s = socket.socket(socket.AF_INET)
    s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

# ---- 4. The probe, built against the schema just extracted ---------------------------------------
# Outside the workspace (root Cargo.toml `exclude`), so it builds from its own manifest and its own
# committed Cargo.lock (`--locked`: a re-run must not resolve different versions silently).
NEEDS_PROBE=0
for arm in ${ARMS}; do
  [[ "${arm}" == "referee-only" ]] || NEEDS_PROBE=1
done
if [[ "${NEEDS_PROBE}" -eq 1 ]]; then
  command -v cargo >/dev/null || { echo "cargo is required" >&2; exit 1; }
  echo "==> building ${PROBE_DIR#"${REPO_ROOT}"/}"
  cargo build --locked --quiet --manifest-path "${PROBE_DIR}/Cargo.toml"
fi

# ---- 5. Arms -------------------------------------------------------------------------------
#
# One arm, one run directory of its own, and the JVM's fatal-error log is pointed into it
# (-XX:ErrorFile) — so a crashing JVM's hs_err_pid*.log lands under gitignored vendor/fixp/, never
# at the repository root, without the JVM's working directory having to be that scratch directory
# (nothing enters one: scripts/check-scratch-fixtures.sh; anything else a JVM wrote to its working
# directory would still be caught by the git-status check at the end). The directory is removed whether the arm passes or fails, so two
# invocations of this script back to back never see Aeron or archive state left over from the one
# before (the plan's "Thư mục Aeron / archive còn sót" trap) — this row's reversal at the process
# level, proven separately from the jar/schema checksum reversal above.
run_referee_only() {
  local run_dir log rc start_s elapsed PORT
  PORT="$(pick_port)"
  run_dir="$(mktemp -d "${VENDOR_DIR}/run.XXXXXX")"
  log="${run_dir}/referee.log"
  start_s="${SECONDS}"

  echo
  echo "==> [referee-only] port=${PORT} aeron-dir=${run_dir}"

  set +e
  ( java "${JAVA_ADD_OPENS[@]}" "-XX:ErrorFile=${run_dir}/hs_err_pid%p.log" -cp "${CLASSES_DIR}:${CLASSPATH}" Referee \
      --port "${PORT}" \
      --archive-control-port "${ARCHIVE_CONTROL_PORT}" \
      --archive-response-port "${ARCHIVE_RESPONSE_PORT}" \
      --deadline-seconds "${DEADLINE_SECONDS}" \
      --aeron-dir "${run_dir}/aeron" ) 2>&1 | tee "${log}"
  rc="${PIPESTATUS[0]}"
  set -e

  elapsed=$(( SECONDS - start_s ))

  local fail=0
  if ! grep -qF "referee: listening on 127.0.0.1:${PORT}" "${log}"; then
    echo "MISSING: 'referee: listening on 127.0.0.1:${PORT}'" >&2
    fail=1
  fi
  if ! grep -qF "referee: shutdown ok" "${log}"; then
    echo "MISSING: 'referee: shutdown ok'" >&2
    fail=1
  fi
  if [[ "${rc}" -ne 0 ]]; then
    echo "the referee process exited ${rc}" >&2
    fail=1
  fi

  if [[ "${fail}" -ne 0 ]]; then
    echo "---- referee output (${log}, about to be removed) ----" >&2
    cat "${log}" >&2
    rm -rf "${run_dir}"
    return 1
  fi

  echo "==> [referee-only] ok in ${elapsed}s"
  rm -rf "${run_dir}"
  return 0
}

# `require LOG LINE` — LINE must appear in LOG as a whole line; says MISSING otherwise.
require() {
  if grep -qxF -- "$2" "$1"; then
    return 0
  fi
  echo "MISSING: '$2'" >&2
  return 1
}

# The tap's Negotiate lines. Values are the ones the probe was told to send; the two that the
# reject-credentials arm and reversal B vary come in as arguments.
require_wire_negotiate() {
  local log="$1" credentials="$2" app_version="$3" bad=0
  require "${log}" "referee: wire Negotiate.blockLength ok 28" || bad=1
  require "${log}" "referee: wire Negotiate.schemaId ok 1" || bad=1
  require "${log}" "referee: wire Negotiate.version ok 5" || bad=1
  require "${log}" "referee: wire Negotiate.sessionID ok ${SESSION_ID}" || bad=1
  require "${log}" "referee: wire Negotiate.sessionVerID ok ${SESSION_VER_ID}" || bad=1
  require "${log}" "referee: wire Negotiate.enteringFirm ok ${ENTERING_FIRM}" || bad=1
  require "${log}" "referee: wire Negotiate.onbehalfFirm ok ${ONBEHALF_FIRM}" || bad=1
  if [[ "${credentials}" == "${CREDENTIALS}" ]]; then
    require "${log}" "referee: wire Negotiate.credentials ok ${credentials}" || bad=1
  else
    require "${log}" "referee: wire Negotiate.credentials MISMATCH got ${credentials} want ${CREDENTIALS}" || bad=1
  fi
  require "${log}" "referee: wire Negotiate.clientIP ok ${CLIENT_IP}" || bad=1
  require "${log}" "referee: wire Negotiate.clientAppName ok ${CLIENT_APP_NAME}" || bad=1
  require "${log}" "referee: wire Negotiate.clientAppVersion ok ${app_version}" || bad=1
  if ! grep -qE '^referee: wire Negotiate\.length ok [0-9]+$' "${log}"; then
    echo "MISSING: 'referee: wire Negotiate.length ok <n>'" >&2
    bad=1
  fi
  return "${bad}"
}

require_wire_establish_terminate() {
  local log="$1" bad=0
  require "${log}" "referee: wire Establish.blockLength ok 41" || bad=1
  require "${log}" "referee: wire Establish.schemaId ok 1" || bad=1
  require "${log}" "referee: wire Establish.version ok 5" || bad=1
  require "${log}" "referee: wire Establish.sessionID ok ${SESSION_ID}" || bad=1
  require "${log}" "referee: wire Establish.sessionVerID ok ${SESSION_VER_ID}" || bad=1
  require "${log}" "referee: wire Establish.keepAliveInterval ok ${PROBE_KEEPALIVE_MS}" || bad=1
  require "${log}" "referee: wire Establish.nextSeqNo ok ${NEXT_SEQ_NO}" || bad=1
  require "${log}" "referee: wire Establish.cancelOnDisconnectType ok ${COD_TYPE}" || bad=1
  require "${log}" "referee: wire Establish.codTimeoutWindow ok ${COD_TIMEOUT_MS}" || bad=1
  require "${log}" "referee: wire Establish.credentials ok ${CREDENTIALS}" || bad=1
  # framing 4 + SBE header 8 + block 41 + credentials' uint8 length prefix and bytes
  require "${log}" "referee: wire Establish.length ok $(( 4 + 8 + 41 + 1 + ${#CREDENTIALS} ))" || bad=1
  require "${log}" "referee: wire Terminate.blockLength ok 13" || bad=1
  require "${log}" "referee: wire Terminate.schemaId ok 1" || bad=1
  require "${log}" "referee: wire Terminate.version ok 5" || bad=1
  require "${log}" "referee: wire Terminate.sessionID ok ${SESSION_ID}" || bad=1
  require "${log}" "referee: wire Terminate.sessionVerID ok ${SESSION_VER_ID}" || bad=1
  require "${log}" "referee: wire Terminate.terminationCode ok ${TERMINATION_CODE}" || bad=1
  require "${log}" "referee: wire Terminate.length ok $(( 4 + 8 + 13 ))" || bad=1
  return "${bad}"
}

# Each timestamp the probe says it sent must be the one the tap decoded.
require_sent_timestamps() {
  local ref_log="$1" probe_log="$2" arm="$3" bad=0 msg ts n=0
  while read -r msg ts; do
    n=$(( n + 1 ))
    require "${ref_log}" "referee: wire ${msg}.timestamp ${ts}" || bad=1
  done < <(sed -n "s/^${arm}: sent \([A-Za-z]*\)\.timestamp \([0-9]*\)\$/\1 \2/p" "${probe_log}")
  if [[ "${n}" -eq 0 ]]; then
    echo "MISSING: '${arm}: sent <Message>.timestamp <n>' in the probe's output" >&2
    bad=1
  fi
  return "${bad}"
}

# One probe arm: the referee in the background with its own run directory, stop file and
# expectations; the probe in the foreground; then the stop file, and both transcripts judged by
# their lines.
run_probe_arm() {
  local arm="$1"
  local run_dir ref_log probe_log stop_file ref_pid ref_rc probe_rc start_s elapsed waited PORT ARTIO_PORT
  # Two ports: the referee's wire tap listens on PORT, where the probe connects, and relays to
  # Artio's acceptor on ARTIO_PORT (Referee.java's class comment, "The wire tap").
  PORT="$(pick_port)"
  ARTIO_PORT="$(pick_port)"
  while [[ "${ARTIO_PORT}" == "${PORT}" ]]; do ARTIO_PORT="$(pick_port)"; done
  run_dir="$(mktemp -d "${VENDOR_DIR}/run.XXXXXX")"
  ref_log="${run_dir}/referee.log"
  probe_log="${run_dir}/probe.log"
  stop_file="${run_dir}/stop"
  start_s="${SECONDS}"

  local sent_credentials="${CREDENTIALS}"
  [[ "${arm}" == "reject-credentials" ]] && sent_credentials="${WRONG_CREDENTIALS}"
  local sent_client_app_version="${CLIENT_APP_VERSION}"

  echo
  echo "==> [${arm}] port=${PORT} artio-port=${ARTIO_PORT} aeron-dir=${run_dir}"
  # Created before the referee starts, so the wait below never greps a file not yet there.
  : >"${ref_log}"

  ( exec java "${JAVA_ADD_OPENS[@]}" "-XX:ErrorFile=${run_dir}/hs_err_pid%p.log" -cp "${CLASSES_DIR}:${CLASSPATH}" Referee \
      --port "${PORT}" \
      --archive-control-port "${ARCHIVE_CONTROL_PORT}" \
      --archive-response-port "${ARCHIVE_RESPONSE_PORT}" \
      --deadline-seconds "${ARM_REFEREE_CEILING_SECONDS}" \
      --aeron-dir "${run_dir}/aeron" \
      --stop-file "${stop_file}" \
      --sending-time-window-ms "${SENDING_TIME_WINDOW_MS}" \
      --keepalive-min-ms "${KEEPALIVE_MIN_MS}" \
      --keepalive-max-ms "${KEEPALIVE_MAX_MS}" \
      --acceptor-keepalive-ms "${ACCEPTOR_KEEPALIVE_MS}" \
      --expect-session-id "${SESSION_ID}" \
      --expect-session-ver-id "${SESSION_VER_ID}" \
      --expect-entering-firm "${ENTERING_FIRM}" \
      --expect-credentials "${CREDENTIALS}" \
      --expect-client-ip "${CLIENT_IP}" \
      --expect-client-app-name "${CLIENT_APP_NAME}" \
      --expect-client-app-version "${CLIENT_APP_VERSION}" \
      --artio-port "${ARTIO_PORT}" \
      --expect-onbehalf-firm "${ONBEHALF_FIRM}" \
      --expect-keepalive-ms "${PROBE_KEEPALIVE_MS}" \
      --expect-next-seq-no "${NEXT_SEQ_NO}" \
      --expect-cod-type "${COD_TYPE}" \
      --expect-cod-timeout-ms "${COD_TIMEOUT_MS}" \
      --expect-termination-code "${TERMINATION_CODE}" ) >>"${ref_log}" 2>&1 &
  ref_pid=$!

  # The probe starts only once a library is connected to acquire its connection.
  waited=0
  until grep -qxF "referee: library connected" "${ref_log}"; do
    if ! kill -0 "${ref_pid}" 2>/dev/null; then
      break
    fi
    if (( waited >= REFEREE_READY_SECONDS * 10 )); then
      break
    fi
    sleep 0.1
    waited=$(( waited + 1 ))
  done

  probe_rc=-1
  if grep -qxF "referee: library connected" "${ref_log}"; then
    set +e
    "${PROBE_BIN}" \
      --arm "${arm}" \
      --addr "127.0.0.1:${PORT}" \
      --deadline-ms "${PROBE_DEADLINE_MS}" \
      --session-id "${SESSION_ID}" \
      --session-ver-id "${SESSION_VER_ID}" \
      --entering-firm "${ENTERING_FIRM}" \
      --credentials "${sent_credentials}" \
      --client-ip "${CLIENT_IP}" \
      --client-app-name "${CLIENT_APP_NAME}" \
      --client-app-version "${sent_client_app_version}" \
      --keepalive-ms "${PROBE_KEEPALIVE_MS}" \
      --server-keepalive-ms "${ACCEPTOR_KEEPALIVE_MS}" \
      --onbehalf-firm "${ONBEHALF_FIRM}" \
      --next-seq-no "${NEXT_SEQ_NO}" \
      --cod-type "${COD_TYPE}" \
      --cod-timeout-ms "${COD_TIMEOUT_MS}" >"${probe_log}" 2>&1
    probe_rc=$?
    set -e
  else
    echo "the referee never printed 'referee: library connected' (waited $(( waited / 10 ))s)" >"${probe_log}"
  fi

  touch "${stop_file}"
  set +e
  wait "${ref_pid}"
  ref_rc=$?
  set -e
  elapsed=$(( SECONDS - start_s ))

  echo "---- referee ----"
  cat "${ref_log}"
  echo "---- probe ----"
  cat "${probe_log}"
  echo "----"

  local bad=0
  require "${ref_log}" "referee: limits sending-time-window=${SENDING_TIME_WINDOW_MS}ms keepalive-min=${KEEPALIVE_MIN_MS}ms keepalive-max=${KEEPALIVE_MAX_MS}ms acceptor-keepalive=${ACCEPTOR_KEEPALIVE_MS}ms" || bad=1
  require "${ref_log}" "referee: shutdown ok" || bad=1
  [[ "${ref_rc}" -eq 0 ]] || { echo "the referee process exited ${ref_rc}" >&2; bad=1; }

  # The referee's seven lines: each value it RECEIVED, against what the probe was told to SEND.
  require "${ref_log}" "referee: field sessionID ok ${SESSION_ID}" || bad=1
  require "${ref_log}" "referee: field sessionVerID ok ${SESSION_VER_ID}" || bad=1
  require "${ref_log}" "referee: field enteringFirm ok ${ENTERING_FIRM}" || bad=1
  if [[ "${arm}" == "reject-credentials" ]]; then
    require "${ref_log}" "referee: field credentials MISMATCH got ${sent_credentials} want ${CREDENTIALS}" || bad=1
  else
    require "${ref_log}" "referee: field credentials ok ${sent_credentials}" || bad=1
  fi
  require "${ref_log}" "referee: field clientIP ok ${CLIENT_IP}" || bad=1
  require "${ref_log}" "referee: field clientAppName ok ${CLIENT_APP_NAME}" || bad=1
  require "${ref_log}" "referee: field clientAppVersion ok ${sent_client_app_version}" || bad=1

  # The wire tap's lines: every Negotiate field as Real Logic's NegotiateDecoder read it off the
  # wire, the header, and the frame length against the decoder's own end of message.
  require_wire_negotiate "${ref_log}" "${sent_credentials}" "${sent_client_app_version}" || bad=1
  require_sent_timestamps "${ref_log}" "${probe_log}" "${arm}" || bad=1

  case "${arm}" in
    accept)
      require "${ref_log}" "referee: authentication accepted" || bad=1
      require_wire_establish_terminate "${ref_log}" || bad=1
      local step passed=0
      for step in negotiate establish terminate echo eof; do
        if grep -qxF "accept: ${step} ok" "${probe_log}"; then
          passed=$(( passed + 1 ))
        else
          echo "MISSING: 'accept: ${step} ok'" >&2
        fi
      done
      ACCEPT_STEPS_PASSED="${passed}"
      [[ "${passed}" -eq 5 ]] || bad=1
      ;;
    reject-timestamp)
      require "${ref_log}" "referee: authentication accepted" || bad=1
      require "${probe_log}" "reject-timestamp: refused ok: NegotiateReject INVALID_TIMESTAMP(7)" || bad=1
      ;;
    reject-credentials)
      require "${ref_log}" "referee: authentication rejected" || bad=1
      require "${probe_log}" "reject-credentials: refused ok: NegotiateReject CREDENTIALS(1)" || bad=1
      ;;
  esac
  if grep -q " FAIL: " "${probe_log}"; then
    echo "the probe printed a FAIL line" >&2
    bad=1
  fi
  [[ "${probe_rc}" -eq 0 ]] || { echo "the probe exited ${probe_rc}" >&2; bad=1; }

  rm -rf "${run_dir}"
  if [[ "${bad}" -ne 0 ]]; then
    echo "==> [${arm}] FAIL in ${elapsed}s"
    return 1
  fi
  echo "==> [${arm}] PASS in ${elapsed}s"
  return 0
}

fail=0
ACCEPT_STEPS_PASSED=0
declare -A ARM_RESULT=()
for arm in ${ARMS}; do
  case "${arm}" in
    referee-only)
      run_referee_only || fail=1
      ;;
    accept | reject-timestamp | reject-credentials)
      if run_probe_arm "${arm}"; then
        ARM_RESULT[${arm}]=PASS
      else
        ARM_RESULT[${arm}]=FAIL
        fail=1
      fi
      ;;
    *)
      echo "UNKNOWN ARM: ${arm} (see docs/plans/2026-09-23-p3-fixp-spike.md, Chia việc)" >&2
      fail=1
      ;;
  esac
done

if [[ "${fail}" -ne 0 ]]; then
  echo
  echo "==> fixp-spike: FAIL (accept steps ${ACCEPT_STEPS_PASSED}/5; arms: $(for k in "${!ARM_RESULT[@]}"; do printf '%s=%s ' "${k}" "${ARM_RESULT[${k}]}"; done))" >&2
  exit 1
fi

# ---- 6. Nothing this run did is visible to git --------------------------------------------------
AFTER="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | LC_ALL=C sort)"
# Byte order on both sides: under en_US.UTF-8, `sort` and `comm` disagreed about porcelain lines
# that start with a space (" M file" beside "?? file"), and comm said "file 1 is not in sorted
# order" and compared unreliably [measured 2026-09-23, row 3, once the tree had untracked files].
ADDED="$(LC_ALL=C comm -13 <(echo "${BEFORE}") <(echo "${AFTER}") || true)"
if [[ -n "${ADDED}" ]]; then
  echo "THIS RUN CHANGED WHAT git STATUS SEES:" >&2
  echo "${ADDED}" >&2
  exit 1
fi
echo
echo "==> the run added nothing git can see"

# ---- 7. The summary, only when all three probe arms ran and passed ------------------------------
if [[ "${ARM_RESULT[accept]:-}" == PASS && "${ARM_RESULT[reject-timestamp]:-}" == PASS \
      && "${ARM_RESULT[reject-credentials]:-}" == PASS && "${ACCEPT_STEPS_PASSED}" -eq 5 ]]; then
  echo "fixp-spike: accept PASS 5/5, reject-timestamp PASS, reject-credentials PASS"
fi
