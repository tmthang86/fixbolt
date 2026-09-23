#!/usr/bin/env bash
# ADR-0160 decision 6, plan row 8a
# (docs/plans/2026-09-23-p3-packaging-and-first-release.md): before the first
# publish, the PACKAGED SOURCES are the stranger; after it, crates.io is. This
# script is the one command that plays that stranger for real — not a cargo
# build, a socket conversation with a fresh binary, driven by a client that
# shares no code with this repository (scripts/stranger-logon.py, Python
# standard library only).
#
# It does five things, in order:
#   1. Builds a throwaway crate at target/stranger/<scratch>/, OUTSIDE this
#      workspace (its own `[workspace]`), depending on `fixbolt` either
#      through `[patch.crates-io]` onto `target/package/fixbolt-<version>/`
#      (`--from packaged` — the exact bytes a `.crate` upload would contain,
#      the same technique `scripts/check-packaged-build.sh` uses) or through
#      `cargo add fixbolt@<version>` with no patch (`--from registry`, ADR-0097
#      exit criterion 7's post-publish half).
#   2. Takes that crate's `src/main.rs` and `acceptor.cfg` NOT from a fixture
#      this script carries, but from docs/GETTING-STARTED.md itself — the two
#      fenced code blocks immediately following the HTML comments
#      `<!-- stranger-check: main.rs -->` and `<!-- stranger-check: acceptor.cfg -->`.
#      That is what makes the getting-started page a test rather than a
#      picture of one (plan reference page trap "the page already diverges
#      from the file it was supposedly transcribed from" — measured fact 13
#      before this script existed).
#   3. Builds it and runs the binary as `<binary> <acceptor.cfg> <addr>`.
#   4. Runs `scripts/stranger-logon.py <host> <port>` against it: a Logon as
#      the `TW44` counterparty the pasted `acceptor.cfg` names, a Logout, and
#      nothing else.
#   5. Writes one line to the acceptor's stdin — the same "type a line to
#      stop it" contract `crates/library/examples/acceptor.rs` and the
#      pasted `main.rs` both carry — and waits up to 10 s for the process to
#      exit 0, having printed `stopped:` on the way out.
#
# WHAT IT CANNOT SEE: whether `docs/GETTING-STARTED.md`'s prose still
# describes the pasted code accurately — it only proves the code compiles,
# links against the exact bytes named, and answers on the wire; crates.io's
# own server-side checks (`--from registry` only asks the index, which a
# `[patch]` never touches); a `main.rs` that hardcodes a working example but
# would mislead a reader who changed one line (`docs/GETTING-STARTED.md`'s
# prose is a human check, walked by `CLAUDE.md` §4's sync table, not this
# script's job).
#
# Exit 0 when the whole round trip and the stdin-triggered exit both
# succeed; 1 on any FAIL (each printed, prefixed `FAIL` where the failing
# tool did not already print one itself); 2 when the script itself cannot
# run (bad arguments, cargo missing).
#
# Usage:
#   scripts/stranger-check.sh --from packaged
#   scripts/stranger-check.sh --from registry --version 0.1.0
#
# `--from registry` is EXPECTED RED until the owner has run `cargo publish`
# (ADR-0097 exit criterion 5, `RELEASING.md`): `cargo add` cannot resolve a
# name the index has never heard of, and this script says so rather than
# treating that as its own bug.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

MODE=""
REGISTRY_VERSION=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --from)
      MODE="${2:-}"
      shift 2
      ;;
    --version)
      REGISTRY_VERSION="${2:-}"
      shift 2
      ;;
    *)
      echo "stranger-check: FAIL — unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ "${MODE}" != "packaged" && "${MODE}" != "registry" ]]; then
  echo "stranger-check: FAIL — usage: stranger-check.sh --from packaged | --from registry --version X" >&2
  exit 2
fi
if [[ "${MODE}" == "registry" && -z "${REGISTRY_VERSION}" ]]; then
  echo "stranger-check: FAIL — --from registry needs --version X" >&2
  exit 2
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "stranger-check: FAIL — cargo not found on PATH" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "stranger-check: FAIL — python3 not found on PATH (needed by scripts/stranger-logon.py)" >&2
  exit 2
fi

PUBLISHED=(fixbolt-codec fixbolt-dict fixbolt-session fixbolt-engine fixbolt-sbe fixbolt)

# The one workspace version, read the same way check-packaged-build.sh and
# check-release-versions.sh read it: from the manifest's own
# [workspace.package] table, never from `cargo metadata`.
WORKSPACE_VERSION="$(python3 - "${ROOT}" <<'PY'
import pathlib, sys, tomllib
root = pathlib.Path(sys.argv[1])
with open(root / "Cargo.toml", "rb") as f:
    ws = tomllib.load(f)
v = ws.get("workspace", {}).get("package", {}).get("version")
if not isinstance(v, str):
    sys.exit(1)
print(v)
PY
)"
if [[ -z "${WORKSPACE_VERSION}" ]]; then
  echo "stranger-check: FAIL — could not read [workspace.package] version from Cargo.toml" >&2
  exit 2
fi

if [[ "${MODE}" == "packaged" ]]; then
  missing=()
  for name in "${PUBLISHED[@]}"; do
    dir="${ROOT}/target/package/${name}-${WORKSPACE_VERSION}"
    [[ -d "${dir}" ]] || missing+=("${dir}")
  done
  if [[ "${#missing[@]}" -gt 0 ]]; then
    echo "stranger-check: FAIL — the packaged sources are not there yet:" >&2
    printf '    %s\n' "${missing[@]}" >&2
    echo "    Run 'cargo publish --workspace --dry-run' first (no --allow-dirty" >&2
    echo "    on a clean checkout)." >&2
    exit 2
  fi
fi

# --- scratch crate, under target/ so rust-toolchain.toml is copied rather
#     than relied on (scripts/check-scratch-fixtures.sh: a fixture that walks
#     outside the tree must carry its own pin) --------------------------------
WORKDIR="${ROOT}/target/stranger"
mkdir -p "${WORKDIR}"
scratch="$(mktemp -d -p "${WORKDIR}")"
mkdir -p "${scratch}/src"
cp "${ROOT}/rust-toolchain.toml" "${scratch}/rust-toolchain.toml"

# Set to 1 only right before the final OK line — every exit before that,
# including one this script does not expect, leaves the scratch crate for
# reading, the same convention scripts/check-packaged-build.sh uses.
cleanup_ok=0
finish() {
  if [[ "${cleanup_ok}" -eq 1 ]]; then
    rm -rf "${scratch}"
  else
    echo "stranger-check: left the failing scratch crate at ${scratch}" >&2
  fi
}
trap finish EXIT

# --- pull main.rs and acceptor.cfg OUT OF THE DOC, verbatim ------------------
if ! python3 - "${ROOT}/docs/GETTING-STARTED.md" "${scratch}/src/main.rs" "${scratch}/acceptor.cfg" <<'PY'
import re
import sys
import pathlib

doc_path, main_out, cfg_out = sys.argv[1], sys.argv[2], sys.argv[3]
text = pathlib.Path(doc_path).read_text(encoding="utf-8")


def extract(name: str) -> str:
    marker = f"<!-- stranger-check: {name} -->"
    idx = text.find(marker)
    if idx == -1:
        print(
            f"FAIL: no block marked stranger-check: {name} in docs/GETTING-STARTED.md",
            file=sys.stderr,
        )
        sys.exit(1)
    rest = text[idx + len(marker) :]
    m = re.search(r"```[^\n]*\n(.*?)```", rest, re.S)
    if not m:
        print(
            f"FAIL: stranger-check: {name} marker in docs/GETTING-STARTED.md "
            "is not followed by a fenced code block",
            file=sys.stderr,
        )
        sys.exit(1)
    return m.group(1)


pathlib.Path(main_out).write_text(extract("main.rs"), encoding="utf-8")
pathlib.Path(cfg_out).write_text(extract("acceptor.cfg"), encoding="utf-8")
PY
then
  exit 1
fi

# --- the crate's own manifest, either patched onto the packaged sources or
#     resolving fixbolt through the real registry --------------------------
if [[ "${MODE}" == "packaged" ]]; then
  {
    echo "[workspace]"
    echo
    echo "[package]"
    echo "name = \"stranger\""
    echo "version = \"0.0.0\""
    echo "edition = \"2024\""
    echo "publish = false"
    echo
    echo "[dependencies]"
    echo "fixbolt = \"=${WORKSPACE_VERSION}\""
    echo
    echo "[patch.crates-io]"
    for name in "${PUBLISHED[@]}"; do
      echo "${name} = { path = \"${ROOT}/target/package/${name}-${WORKSPACE_VERSION}\" }"
    done
  } >"${scratch}/Cargo.toml"
else
  {
    echo "[workspace]"
    echo
    echo "[package]"
    echo "name = \"stranger\""
    echo "version = \"0.0.0\""
    echo "edition = \"2024\""
    echo "publish = false"
    echo
    echo "[dependencies]"
  } >"${scratch}/Cargo.toml"

  echo "== cargo add fixbolt@${REGISTRY_VERSION} (no [patch] — the real crates.io index) =="
  if ! cargo add "fixbolt@${REGISTRY_VERSION}" --manifest-path "${scratch}/Cargo.toml"; then
    echo "stranger-check: FAIL — could not select fixbolt = \"${REGISTRY_VERSION}\" from the" >&2
    echo "    registry. Expected RED until the owner has run 'cargo publish'" >&2
    echo "    (RELEASING.md, ADR-0097 exit criterion 5) — this is not a bug in" >&2
    echo "    this script." >&2
    exit 1
  fi
fi

echo "== cargo build --manifest-path ${scratch}/Cargo.toml =="
build_log="${scratch}/build.log"
if ! cargo build --manifest-path "${scratch}/Cargo.toml" >"${build_log}" 2>&1; then
  echo "stranger-check: FAIL — the pasted docs/GETTING-STARTED.md code did not build:" >&2
  cat "${build_log}" >&2
  exit 1
fi
tail -1 "${build_log}"

BIN="${scratch}/target/debug/stranger"
if [[ ! -x "${BIN}" ]]; then
  echo "stranger-check: FAIL — build finished but ${BIN} is not there" >&2
  exit 1
fi

PORT="$(python3 -c 'import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()')"
ADDR="127.0.0.1:${PORT}"

acceptor_log="${scratch}/acceptor.log"
# coproc gives a stdin fd to write the stop line to (ACCEPTOR[1]) and a
# stdout fd to read `stopped:` from (ACCEPTOR[0]), and $ACCEPTOR_PID. Both
# array-element fds are duplicated into plain numeric fds right away:
# referencing "${ACCEPTOR[0]}" a second time — once here to start the log
# reader, again later to write the stop line — closes it out from under the
# second use on this bash (5.3.9), "Bad file descriptor"; a `exec {VAR}<&`
# duplicate survives being read from repeatedly.
coproc ACCEPTOR { "${BIN}" "${scratch}/acceptor.cfg" "${ADDR}"; }
# $ACCEPTOR_PID itself is captured into a plain variable at once: once the
# coprocess exits (this one can, within a second of the stop line — there is
# no active session left to wait out), bash reaps it on its own SIGCHLD
# handling and unsets $ACCEPTOR_PID and the $ACCEPTOR array under it, so a
# later "${acceptor_pid}" can read as unbound under `set -u` even though the
# coproc undeniably ran.
# shellcheck disable=SC2153
# SC2153 suspects a misspelling of acceptor_pid; ACCEPTOR_PID is bash's own
# coproc-supplied variable for `coproc ACCEPTOR { ...; }`, not a typo.
acceptor_pid="${ACCEPTOR_PID}"
exec {acceptor_out}<&"${ACCEPTOR[0]}"
exec {acceptor_in}>&"${ACCEPTOR[1]}"
cat <&"${acceptor_out}" >"${acceptor_log}" &
log_reader_pid=$!

ready=0
for _ in $(seq 1 100); do
  # `exec 9<>...` runs inside this ( ) subshell, so fd 9 is subshell-local and
  # closes on its own when the subshell exits — there is nothing to close
  # back in THIS shell. (A bare `exec 9>&-` here, with no command, would
  # apply its OWN trailing redirection to this shell permanently: `exec
  # 9>&- 2>/dev/null` would silently rebind this script's own fd 2 to
  # /dev/null for everything after it, which is exactly the kind of failure
  # this script exists to catch in `fixbolt`, not commit itself.)
  if (exec 9<>"/dev/tcp/127.0.0.1/${PORT}") 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.1
done
if [[ "${ready}" -ne 1 ]]; then
  echo "stranger-check: FAIL — the acceptor never opened ${ADDR}" >&2
  kill "${acceptor_pid}" 2>/dev/null || true
  cat "${acceptor_log}" >&2
  exit 1
fi

echo "== scripts/stranger-logon.py 127.0.0.1 ${PORT} =="
if ! python3 "${ROOT}/scripts/stranger-logon.py" 127.0.0.1 "${PORT}"; then
  echo "stranger-check: FAIL — the Logon/Logout round trip did not complete" >&2
  kill "${acceptor_pid}" 2>/dev/null || true
  cat "${acceptor_log}" >&2
  exit 1
fi

# The stop line: exactly what a human running the pasted example would type.
echo "" >&"${acceptor_in}"

deadline=$((SECONDS + 10))
while kill -0 "${acceptor_pid}" 2>/dev/null; do
  if [[ "${SECONDS}" -ge "${deadline}" ]]; then
    echo "stranger-check: FAIL — the acceptor did not exit within 10 s of the stdin line" >&2
    kill "${acceptor_pid}" 2>/dev/null || true
    cat "${acceptor_log}" >&2
    exit 1
  fi
  sleep 0.1
done
wait "${acceptor_pid}"
status=$?
wait "${log_reader_pid}" 2>/dev/null || true

if [[ "${status}" -ne 0 ]]; then
  echo "stranger-check: FAIL — the acceptor exited ${status} after the stdin line" >&2
  cat "${acceptor_log}" >&2
  exit 1
fi

stopped_line="$(grep '^stopped:' "${acceptor_log}" || true)"
if [[ -z "${stopped_line}" ]]; then
  echo "stranger-check: FAIL — the acceptor exited 0 but never printed 'stopped:'" >&2
  cat "${acceptor_log}" >&2
  exit 1
fi
echo "${stopped_line}"

cleanup_ok=1
echo "stranger-check: OK — --from ${MODE}, fixbolt ${WORKSPACE_VERSION:-${REGISTRY_VERSION}}, Logon/Logout answered, acceptor stopped cleanly"
