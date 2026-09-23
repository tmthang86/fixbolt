#!/usr/bin/env bash
# The FIXP spike (phase 3 row 9, docs/plans/2026-09-23-p3-fixp-spike.md, ADR-0140): pin and fetch
# Artio's Binary EntryPoint jars, extract the schema they actually decode with, and run this
# repository's own referee (spikes/fixp-probe/referee/Referee.java) alone.
#
# THIS ROW (1) ONLY BUILDS ONE ARM: `referee-only` — the referee starts headless behind an
# in-process Aeron media driver, is OBSERVED listening (FixEngine.launch binds the acceptor
# socket synchronously and only returns once that succeeds — see the trap recorded at the top of
# spikes/fixp-probe/referee/Referee.java for why this, and not a bare loopback probe, is what
# counts as the observation here), runs for its own deadline, and stops on its own.
# Later rows (plan Chia việc 3-5) add the `accept`, `reject-timestamp` and `reject-credentials`
# arms and the summary line; this script refuses any arm it does not yet know, by name, so a
# typo in FIXP_SPIKE_ARMS fails loudly instead of silently skipping something.
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

ARMS="${FIXP_SPIKE_ARMS:-referee-only}"
PORT="${FIXP_REFEREE_PORT:-15660}"
ARCHIVE_CONTROL_PORT="${FIXP_REFEREE_ARCHIVE_CONTROL_PORT:-10010}"
ARCHIVE_RESPONSE_PORT="${FIXP_REFEREE_ARCHIVE_RESPONSE_PORT:-10020}"
DEADLINE_SECONDS="${FIXP_REFEREE_DEADLINE_SECONDS:-5}"

for tool in curl sha256sum jar javac java git comm mktemp; do
  command -v "${tool}" >/dev/null || { echo "${tool} is required" >&2; exit 1; }
done

# Taken before anything is fetched, built or run, so the last step can ask what THIS run added
# rather than whether the tree happened to already be clean (scripts/interop.sh's pattern).
BEFORE="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | sort)"

echo "==> fixp-spike: arms=[${ARMS}] port=${PORT} archive-control=${ARCHIVE_CONTROL_PORT} archive-response=${ARCHIVE_RESPONSE_PORT} deadline=${DEADLINE_SECONDS}s"

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
EXTRACT_TMP="$(mktemp -d "${VENDOR_DIR}/extract.XXXXXX")"
( cd "${EXTRACT_TMP}" && jar xf "${JAR_DIR}/${CODECS_JAR}" "${SCHEMA_PATH_IN_JAR}" )
mv "${EXTRACT_TMP}/${SCHEMA_PATH_IN_JAR}" "${SCHEMA_FILE}"
rm -rf "${EXTRACT_TMP}"

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

# ---- 4. Arms -------------------------------------------------------------------------------
#
# One arm, one run directory of its own, and the JVM's OWN CWD is that directory too — so any
# incidental file a crashing JVM writes (hs_err_pid*.log and friends) lands under vendor/fixp/,
# never at the repository root. The directory is removed whether the arm passes or fails, so two
# invocations of this script back to back never see Aeron or archive state left over from the one
# before (the plan's "Thư mục Aeron / archive còn sót" trap) — this row's reversal at the process
# level, proven separately from the jar/schema checksum reversal above.
run_referee_only() {
  local run_dir log rc start_s elapsed
  run_dir="$(mktemp -d "${VENDOR_DIR}/run.XXXXXX")"
  log="${run_dir}/referee.log"
  start_s="${SECONDS}"

  echo
  echo "==> [referee-only] aeron-dir=${run_dir}"

  set +e
  ( cd "${run_dir}" && java "${JAVA_ADD_OPENS[@]}" -cp "${CLASSES_DIR}:${CLASSPATH}" Referee \
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

fail=0
for arm in ${ARMS}; do
  case "${arm}" in
    referee-only)
      run_referee_only || fail=1
      ;;
    *)
      echo "UNKNOWN ARM: ${arm} (not built yet — see docs/plans/2026-09-23-p3-fixp-spike.md, Chia việc)" >&2
      fail=1
      ;;
  esac
done

if [[ "${fail}" -ne 0 ]]; then
  exit 1
fi

# ---- 5. Nothing this run did is visible to git --------------------------------------------------
AFTER="$(cd "${REPO_ROOT}" && git status --porcelain --untracked-files=all | sort)"
ADDED="$(comm -13 <(echo "${BEFORE}") <(echo "${AFTER}") || true)"
if [[ -n "${ADDED}" ]]; then
  echo "THIS RUN CHANGED WHAT git STATUS SEES:" >&2
  echo "${ADDED}" >&2
  exit 1
fi
echo
echo "==> the run added nothing git can see"
