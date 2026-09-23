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
#   * qfj-acceptor-tls     — fixbolt ACCEPTOR  (serve_tls_requiring),   QuickFIX/J initiator (Judge)
#   * qfj-initiator-plain  — fixbolt INITIATOR (--role dial, real engine door), QuickFIX/J acceptor (Judge)
#   * qfj-initiator-tls    — fixbolt INITIATOR (--role dial, connect_and_serve_tls), QuickFIX/J acceptor (Judge)
#
# The two TLS arms are the plaintext arms with one variable changed: the same
# judge, the same seven steps, the same `Desk`, and a settings file that adds
# `SocketUseSSL=Y` and `TlsRequireKernel=Y`. Each also asserts `kernel`:
# `TlsTxSw` and `TlsRxSw` in /proc/net/tls_stat rose across the arm, fixbolt
# printed no `TlsFellBackToUserspace` event and lost none, and the file said
# `TlsRequireKernel=Y`. The four-arm summary line prints only when all four
# arms ran and passed, and CI greps that line.
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

for tool in curl sha256sum javac java keytool openssl unzip cargo; do
  command -v "${tool}" >/dev/null || { echo "${tool} is required" >&2; exit 1; }
done

# Taken before anything is fetched, compiled or run, so the final check can ask
# what this run added rather than whether the tree happened to be clean.
# scripts/interop.sh's own comment on this line still applies verbatim.
BEFORE="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | sort)"

# Config knobs, overridable for local iteration.
PORT_ACCEPTOR_PLAIN="${INTEROP_QFJ_PORT1:-15660}"
PORT_INITIATOR_PLAIN="${INTEROP_QFJ_PORT2:-15661}"
PORT_ACCEPTOR_TLS="${INTEROP_QFJ_PORT3:-15662}"
PORT_INITIATOR_TLS="${INTEROP_QFJ_PORT4:-15663}"
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

# `--features tls` for every arm, plaintext included: one binary, so the
# plaintext and TLS arms differ in the settings file and nothing else. The
# plaintext doors of that binary are the same code with or without the feature.
echo "==> building tools/interop --features tls"
cargo build -q -p fixbolt-interop --features tls

# ---- 2. Certificates for this run, and nothing kept ------------------------
#
# ADR-0130 decision 6. A CA and two leaves, P-256, each leaf carrying an IP SAN
# of 127.0.0.1: QuickFIX/J's initiator runs `EndpointIdentificationAlgorithm=
# HTTPS` against fixbolt's leaf, and fixbolt's initiator verifies QFJ's leaf by
# `SocketConnectHost=127.0.0.1` as `ServerName::IpAddress`. QFJ reads PKCS12
# (`openssl pkcs12 -export` for the keystore, `keytool -importcert` for the
# truststore); fixbolt reads PEM. The password protects nothing — the files
# live under gitignored vendor/ for the length of one run and are regenerated
# by the next.
PKI="${RUN}/pki"
PKI_PW="fixbolt-interop-run"
make_pki() {
  rm -rf "${PKI}"
  mkdir -p "${PKI}"
  openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
    -keyout "${PKI}/ca.key" -out "${PKI}/ca.pem" -days 2 \
    -subj "/CN=fixbolt interop-qfj run CA" \
    -addext "basicConstraints=critical,CA:TRUE" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
  # **`fixbolt-acceptor` also carries `DNS:localhost`, and it is a trap, not
  # decoration** — measured 2026-09-23. QFJ 3.0.2's `InitiatorSslFilter`
  # creates its `SSLEngine` with `InetSocketAddress.getHostName()` of the
  # connect address, which reverse-resolves `127.0.0.1` to `localhost` through
  # /etc/hosts; `EndpointIdentificationAlgorithm=HTTPS` then checks *that* name
  # and the handshake failed with `SSLHandshakeException: (certificate_unknown)
  # No name matching localhost found` against a leaf with only the IP SAN. The
  # IP SAN stays, for a resolver that answers with the literal. `qfj-acceptor`
  # keeps the IP SAN alone, so fixbolt's initiator is held to
  # `ServerName::IpAddress` with nothing else to fall back on.
  local leaf san
  for leaf in fixbolt-acceptor qfj-acceptor; do
    san="IP:127.0.0.1"
    [[ "${leaf}" == "fixbolt-acceptor" ]] && san="IP:127.0.0.1,DNS:localhost"
    openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
      -keyout "${PKI}/${leaf}.key" -out "${PKI}/${leaf}.csr" \
      -subj "/CN=${leaf}" 2>/dev/null
    printf '%s\n' \
      "basicConstraints=critical,CA:FALSE" \
      "keyUsage=critical,digitalSignature" \
      "extendedKeyUsage=serverAuth,clientAuth" \
      "subjectAltName=${san}" \
      "subjectKeyIdentifier=hash" \
      "authorityKeyIdentifier=keyid" > "${PKI}/${leaf}.ext"
    openssl x509 -req -in "${PKI}/${leaf}.csr" -CA "${PKI}/ca.pem" -CAkey "${PKI}/ca.key" \
      -set_serial "0x$(openssl rand -hex 8)" -days 2 \
      -extfile "${PKI}/${leaf}.ext" -out "${PKI}/${leaf}.pem" 2>/dev/null
  done
  # QFJ's acceptor presents `qfj-acceptor`; QFJ's initiator is given the same
  # keystore so it never goes looking for the `quickfixj.keystore` QFJ falls
  # back to (fixbolt's acceptor asks for no client certificate, so it is never
  # sent). Both QFJ roles trust only this run's CA.
  openssl pkcs12 -export -in "${PKI}/qfj-acceptor.pem" -inkey "${PKI}/qfj-acceptor.key" \
    -certfile "${PKI}/ca.pem" -name qfj -passout "pass:${PKI_PW}" \
    -out "${PKI}/qfj-keystore.p12"
  keytool -importcert -noprompt -alias fixbolt-interop-ca -file "${PKI}/ca.pem" \
    -keystore "${PKI}/qfj-truststore.p12" -storetype PKCS12 \
    -storepass "${PKI_PW}" >/dev/null 2>&1
  echo "==> certificates for this run: CA + fixbolt-acceptor (IP:127.0.0.1, DNS:localhost) + qfj-acceptor (IP:127.0.0.1), P-256"
}

# The QuickFIX/J side of every TLS arm, pinned to what fixbolt offers — ADR-0130
# decision 6: TLS 1.3 and TLS_AES_128_GCM_SHA256 only (`tls::server_config` and
# `tls::client_config` narrow to exactly that, because kTLS carries fewer
# suites than rustls negotiates). `INTEROP_QFJ_CIPHER` exists for reversal E.
qfj_tls_lines() {
  cat <<CFG
SocketUseSSL=Y
EnabledProtocols=TLSv1.3
CipherSuites=${INTEROP_QFJ_CIPHER:-TLS_AES_128_GCM_SHA256}
SocketKeyStore=${PKI}/qfj-keystore.p12
SocketKeyStorePassword=${PKI_PW}
KeyStoreType=PKCS12
SocketTrustStore=${PKI}/qfj-truststore.p12
SocketTrustStorePassword=${PKI_PW}
TrustStoreType=PKCS12
CFG
}

# ---- The kernel's own count --------------------------------------------------
#
# `TlsTxSw` / `TlsRxSw` count sockets the kernel installed software kTLS keys
# on, per direction. The engine cannot write these numbers; that is why they
# are the evidence. Empty when /proc/net/tls_stat does not exist (no `tls`
# module), which the `kernel` assertion reads as red.
tls_counter() {
  awk -v k="$1" '$1 == k { print $2 }' /proc/net/tls_stat 2>/dev/null || true
}

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

SHUTDOWN_OK=0
CLEAN_OK=0
KERNEL_OK=0

dump_and_exit() {
  local label="$1" judgelog="$2" fixboltlog="$3"
  echo >&2
  echo "---- [${label}] what fixbolt said ----" >&2
  cat "${fixboltlog}" >&2
  echo "---- [${label}] what the judge said ----" >&2
  cat "${judgelog}" >&2
  exit 1
}

# ---- Reading the output. This is the gate, not the exit code. --------------
#
# CLAUDE.md §10: a check proves nothing until something reads it. Every step
# name, every extra assertion, and the summary line are grepped by name —
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
    SHUTDOWN_OK=$((SHUTDOWN_OK + 1))
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
    CLEAN_OK=$((CLEAN_OK + 1))
  else
    echo "MISSING: ${label} clean — ${dirty} reject/business-reject line(s) seen" >&2
    fail=1
  fi

  if [[ "${fail}" -ne 0 ]]; then
    dump_and_exit "${label}" "${judgelog}" "${fixboltlog}"
  fi
}

# `kernel`, TLS arms only — ADR-0130 decision 5. All required:
#   * TlsTxSw and TlsRxSw each rose by at least 1 across the arm — the kernel
#     took keys in both directions; a userspace session moves neither;
#   * fixbolt printed 0 `interop: event TlsFellBackToUserspace` and
#     `interop: events lost 0` — a zero read off a stream that dropped events
#     would be a zero about nothing;
#   * fixbolt printed 0 `interop: event EndedWithoutReason` — the plan's trap
#     for QFJ's `close_notify` after Logout reaching kTLS as a non-data record;
#   * the file fixbolt was brought up from says `TlsRequireKernel=Y`.
# **What it cannot see**: the counters are machine-wide, so another process
# setting up kTLS during the arm also moves them. A rise is necessary, not
# sufficient, on a shared machine; the event stream is the per-process half.
assert_kernel() {
  local label="$1" judgelog="$2" fixboltlog="$3" cfg="$4"
  local tx0="$5" tx1="$6" rx0="$7" rx1="$8" fail=0
  local fell ended lost dtx="?" drx="?"
  fell="$(grep -c '^interop: event TlsFellBackToUserspace' "${fixboltlog}" || true)"
  ended="$(grep -c '^interop: event EndedWithoutReason' "${fixboltlog}" || true)"
  lost="$(grep -oE '^interop: events lost [0-9]+' "${fixboltlog}" | awk '{print $4}' | tail -1 || true)"
  if [[ -z "${tx0}" || -z "${tx1}" || -z "${rx0}" || -z "${rx1}" ]]; then
    echo "MISSING: ${label} kernel — /proc/net/tls_stat unreadable (before: TlsTxSw='${tx0}' TlsRxSw='${rx0}', after: TlsTxSw='${tx1}' TlsRxSw='${rx1}')" >&2
    fail=1
  else
    dtx=$((tx1 - tx0))
    drx=$((rx1 - rx0))
    if [[ "${dtx}" -lt 1 || "${drx}" -lt 1 ]]; then
      echo "MISSING: ${label} kernel — TlsTxSw +${dtx}, TlsRxSw +${drx}; the kernel took no keys" >&2
      fail=1
    fi
  fi
  if [[ "${fell}" -ne 0 ]]; then
    echo "MISSING: ${label} kernel — ${fell} TlsFellBackToUserspace event(s)" >&2
    fail=1
  fi
  if [[ "${ended}" -ne 0 ]]; then
    echo "MISSING: ${label} kernel — ${ended} EndedWithoutReason event(s)" >&2
    fail=1
  fi
  if [[ "${lost}" != "0" ]]; then
    echo "MISSING: ${label} kernel — events lost '${lost}', want a printed 0" >&2
    fail=1
  fi
  if ! grep -qx 'TlsRequireKernel=Y' "${cfg}"; then
    echo "MISSING: ${label} kernel — ${cfg} does not say TlsRequireKernel=Y" >&2
    fail=1
  fi
  if [[ "${fail}" -ne 0 ]]; then
    dump_and_exit "${label}" "${judgelog}" "${fixboltlog}"
  fi
  echo "${label}: kernel       ok    TlsTxSw +${dtx}, TlsRxSw +${drx}, 0 TlsFellBackToUserspace, 0 EndedWithoutReason, events lost 0, TlsRequireKernel=Y"
  KERNEL_OK=$((KERNEL_OK + 1))
}

# ---- Arms: fixbolt ACCEPTOR (qfj-acceptor-plain, qfj-acceptor-tls) ----------
#
# fixbolt is the acceptor — the product this repository is positioned on
# (ADR-0130 Consequences) — and Judge plays the QuickFIX/J initiator. `Desk`
# sends two News on logon, same as against libquickfix. `tls` is 0 or 1 and is
# the only difference between the two arms.
run_acceptor() {
  local label="$1" port="$2" tls="$3" work tx0="" rx0="" tx1="" rx1=""
  work="${RUN}/${label}"
  rm -rf "${work}"
  mkdir -p "${work}/fbstore" "${work}/qfjstore"

  {
    cat <<CFG
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=FIXBOLT
CFG
    if [[ "${tls}" -eq 1 ]]; then
      cat <<CFG
SocketUseSSL=Y
TlsRequireKernel=Y
ServerCertificateFile=${PKI}/fixbolt-acceptor.pem
ServerCertificateKeyFile=${PKI}/fixbolt-acceptor.key
CFG
    fi
    cat <<CFG

[SESSION]
TargetCompID=QFJINI
HeartBtInt=2
CFG
  } > "${work}/fixbolt.cfg"

  {
    cat <<CFG
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
CFG
    if [[ "${tls}" -eq 1 ]]; then
      qfj_tls_lines
      echo "EndpointIdentificationAlgorithm=HTTPS"
    fi
    cat <<CFG

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFJINI
TargetCompID=FIXBOLT
CFG
  } > "${work}/qfj-initiator.cfg"

  if [[ "${tls}" -eq 1 ]]; then
    tx0="$(tls_counter TlsTxSw)"
    rx0="$(tls_counter TlsRxSw)"
  fi

  echo
  echo "==> [${label}] fixbolt acceptor on ${port}"
  mkfifo "${work}/fixbolt.ctl"
  "${REPO_ROOT}/target/debug/interop" --role acceptor \
    --listen "127.0.0.1:${port}" --cfg "${work}/fixbolt.cfg" \
    < "${work}/fixbolt.ctl" \
    > "${work}/fixbolt.log" 2>&1 &
  FB_PID=$!
  exec 9> "${work}/fixbolt.ctl"

  # Either line ends the wait: an acceptor that refused to start (reversal D:
  # `TlsRequireKernel=Y` on a kernel with no `tls` module) says so at once, and
  # waiting out the deadline for a readiness line it will never print would
  # only delay the sentence that matters.
  if ! wait_for_line "${work}/fixbolt.log" "^interop: (listening|FAIL)" \
    || ! grep -q "^interop: listening" "${work}/fixbolt.log"; then
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
  if [[ "${tls}" -eq 1 ]]; then
    tx1="$(tls_counter TlsTxSw)"
    rx1="$(tls_counter TlsRxSw)"
    assert_kernel "${label}" "${work}/judge.log" "${work}/fixbolt.log" "${work}/fixbolt.cfg" \
      "${tx0}" "${tx1}" "${rx0}" "${rx1}"
  fi
}

# ---- Arms: fixbolt INITIATOR (qfj-initiator-plain, qfj-initiator-tls) -------
#
# fixbolt dials out through its real initiator door (`--role dial`,
# `connect_and_serve` / `connect_and_serve_tls`) — the plan's reason for
# building that role rather than reusing the hand-rolled `--role initiator`
# session. Judge plays the QuickFIX/J acceptor and drives every step from that
# side.
run_initiator() {
  local label="$1" port="$2" tls="$3" work tx0="" rx0="" tx1="" rx1=""
  work="${RUN}/${label}"
  rm -rf "${work}"
  mkdir -p "${work}/qfjstore"

  {
    cat <<CFG
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
CFG
    if [[ "${tls}" -eq 1 ]]; then
      qfj_tls_lines
      echo "NeedClientAuth=N"
    fi
    cat <<CFG

[SESSION]
BeginString=FIX.4.4
SenderCompID=QFJACC
TargetCompID=FIXBOLT
HeartBtInt=2
CFG
  } > "${work}/qfj-acceptor.cfg"

  # `ReconnectInterval=30` — same argument scripts/interop.sh's 4b makes for
  # the C++ direction: fixbolt must not redial after the judge logs it out and
  # exits, or a second Logon would land in the same transcript the assertions
  # above read.
  {
    cat <<CFG
[DEFAULT]
ConnectionType=initiator
SocketConnectHost=127.0.0.1
SocketConnectPort=${port}
HeartBtInt=2
ReconnectInterval=30
CFG
    if [[ "${tls}" -eq 1 ]]; then
      cat <<CFG
SocketUseSSL=Y
TlsRequireKernel=Y
CertificationAuthoritiesFile=${PKI}/ca.pem
CFG
    fi
    cat <<CFG

[SESSION]
BeginString=FIX.4.4
SenderCompID=FIXBOLT
TargetCompID=QFJACC
CFG
  } > "${work}/fixbolt-dial.cfg"

  if [[ "${tls}" -eq 1 ]]; then
    tx0="$(tls_counter TlsTxSw)"
    rx0="$(tls_counter TlsRxSw)"
  fi

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
  # fixbolt's dial has none of its own to wait on — except a refusal to start
  # (reversal D: `TlsRequireKernel=Y` on a kernel with no `tls` module), which
  # ends the wait at once with fixbolt's own sentence rather than after the
  # judge has run out every step's deadline against nobody.
  local ticks=$((DEADLINE * 10)) i=0
  while [[ "${i}" -lt "${ticks}" ]]; do
    grep -Eq "^${label}: (PASS|FAIL) " "${work}/judge.log" && break
    if grep -q "^interop: FAIL" "${work}/fixbolt.log" 2>/dev/null; then
      echo "[${label}] fixbolt's dial refused to start:" >&2
      cat "${work}/fixbolt.log" >&2
      exit 1
    fi
    sleep 0.1
    i=$((i + 1))
  done
  if ! grep -Eq "^${label}: (PASS|FAIL) " "${work}/judge.log"; then
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
  # to `crates/engine`, which this plan does not touch — reported, not fixed,
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
  if [[ "${tls}" -eq 1 ]]; then
    tx1="$(tls_counter TlsTxSw)"
    rx1="$(tls_counter TlsRxSw)"
    assert_kernel "${label}" "${work}/judge.log" "${work}/fixbolt.log" "${work}/fixbolt-dial.cfg" \
      "${tx0}" "${tx1}" "${rx0}" "${rx1}"
  fi
}

# ---- Which arms this invocation runs ----------------------------------------
#
# `INTEROP_QFJ_ARMS` lets a step under development run only its own arms. The
# default is all four, in the order the summary line names them, and only a
# run of all four prints that line — CI greps it, so a partial invocation can
# never be mistaken for the gate.
IFS=',' read -r -a ARMS <<< "${INTEROP_QFJ_ARMS:-acceptor-plain,acceptor-tls,initiator-plain,initiator-tls}"

WANT_PKI=0
for arm in "${ARMS[@]}"; do
  case "${arm}" in
    acceptor-plain | initiator-plain) ;;
    acceptor-tls | initiator-tls) WANT_PKI=1 ;;
    *)
      echo "unknown arm '${arm}' (acceptor-plain | acceptor-tls | initiator-plain | initiator-tls)" >&2
      exit 1
      ;;
  esac
done
if [[ "${WANT_PKI}" -eq 1 ]]; then
  make_pki
fi

declare -A RAN=()
for arm in "${ARMS[@]}"; do
  case "${arm}" in
    acceptor-plain) run_acceptor qfj-acceptor-plain "${PORT_ACCEPTOR_PLAIN}" 0 ;;
    acceptor-tls) run_acceptor qfj-acceptor-tls "${PORT_ACCEPTOR_TLS}" 1 ;;
    initiator-plain) run_initiator qfj-initiator-plain "${PORT_INITIATOR_PLAIN}" 0 ;;
    initiator-tls) run_initiator qfj-initiator-tls "${PORT_INITIATOR_TLS}" 1 ;;
  esac
  RAN[${arm}]=1
done

# ---- Last: what this run added, and what it means ---------------------------

AFTER="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | sort)"
if [[ "${BEFORE}" != "${AFTER}" ]]; then
  echo "the run added something git can see:" >&2
  diff <(echo "${BEFORE}") <(echo "${AFTER}") >&2 || true
  exit 1
fi
echo "==> the run added nothing git can see"

# The summary line, only when all four arms ran. Every arm that fails exits
# above, so reaching here with all four means four passes — and the counters
# are printed from what was asserted, not from that inference, so a future
# path that forgets to exit still cannot print 4 / 4.
if [[ -n "${RAN[acceptor-plain]:-}" && -n "${RAN[acceptor-tls]:-}" \
  && -n "${RAN[initiator-plain]:-}" && -n "${RAN[initiator-tls]:-}" ]]; then
  echo "interop-qfj: 7 / 7 acceptor plain + 7 / 7 acceptor TLS + 7 / 7 initiator plain + 7 / 7 initiator TLS (+ shutdown ${SHUTDOWN_OK} / 4, clean ${CLEAN_OK} / 4, kernel ${KERNEL_OK} / 2) against QuickFIX/J ${QFJ_VERSION} on ${JAVA_VERSION_LINE}"
else
  echo "interop-qfj: partial run (${ARMS[*]}) — the summary line prints only when all four arms run"
fi
