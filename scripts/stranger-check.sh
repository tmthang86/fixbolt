#!/usr/bin/env bash
# ADR-0160 decision 6, ADR-0161 decisions 1 and 4, plan row 8a and row 8b
# (docs/plans/2026-09-23-p3-packaging-and-first-release.md, *Sửa 2*): before
# the first publish, the PACKAGED SOURCES are the stranger; ADR-0161 decided
# there is no publish, so the tag `v0.1.0` on GitHub is the release a
# stranger actually depends on. This script is the one command that plays
# that stranger for real — not a cargo build, a socket conversation with a
# fresh binary, driven by a client that shares no code with this repository
# (scripts/stranger-logon.py, Python standard library only).
#
# It does five things, in order:
#   1. Builds a throwaway crate at target/stranger/<scratch>/, OUTSIDE this
#      workspace (its own `[workspace]`), depending on `fixbolt` one of three
#      ways:
#        --from packaged  — `[patch.crates-io]` onto
#                            `target/package/fixbolt-<version>/`, the exact
#                            bytes a `.crate` upload would contain (the same
#                            technique `scripts/check-packaged-build.sh`
#                            uses).
#        --from registry  — `cargo add fixbolt@<version>` with no patch
#                            (ADR-0097 exit criterion 7's post-publish half;
#                            unused while ADR-0161 decision 1 holds, kept for
#                            the day a publish does happen).
#        --from git       — `cargo add --git <url> --tag <tag> fixbolt`, no
#                            patch: the channel ADR-0161 decision 3 actually
#                            tells a stranger to use.
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
# `--from git` additionally proves the source, the way ADR-0161 decision 4
# asks: the build log must name `<url>?tag=<tag>#<sha8>`, not a local path,
# and the scratch crate's own `Cargo.lock` must carry
# `source = "git+<url>?tag=<tag>#<sha40>"` with `<sha40>` equal to
# `git rev-parse <tag>^{commit}` in THIS checkout — and it refuses to run at
# all if `docs/GETTING-STARTED.md`'s own install line names a different tag
# than the one this run was asked to check (the page and the gate must speak
# of one release).
#
# WHAT IT CANNOT SEE: whether `docs/GETTING-STARTED.md`'s prose still
# describes the pasted code accurately — it only proves the code compiles,
# links against the exact bytes named, and answers on the wire; crates.io's
# own server-side checks (`--from registry` only asks the index, which a
# `[patch]` never touches); a `main.rs` that hardcodes a working example but
# would mislead a reader who changed one line (`docs/GETTING-STARTED.md`'s
# prose is a human check, walked by `CLAUDE.md` §4's sync table, not this
# script's job); a tag that moves or is deleted on GitHub AFTER this
# checkout's own clone already moved with it — `--from git`'s sha comparison
# reads `<tag>^{commit}` from THIS checkout, so a checkout that followed the
# same force-push sees no mismatch either (ADR-0161 decision 2's ruleset is
# what is supposed to make that unreachable, not this script).
#
# Exit 0 when the whole round trip and the stdin-triggered exit both
# succeed; 1 on any FAIL (each printed, prefixed `FAIL` where the failing
# tool did not already print one itself); 2 when the script itself cannot
# run (bad arguments, cargo missing, or — `--from git` only — the named tag
# is not resolvable in THIS checkout, so the expected sha cannot even be
# computed).
#
# Usage:
#   scripts/stranger-check.sh --from packaged [--allow-dirty]
#   scripts/stranger-check.sh --from registry --version 0.1.0
#   scripts/stranger-check.sh --from git --tag v0.1.0 [--url https://github.com/tmthang86/fixbolt]
#
# `--from packaged` refuses a stale target/package/ (built from a commit
# other than HEAD) or a dirty working tree (unless --allow-dirty, local
# iteration only) — see the freshness check below, before anything is built.
#
# `--from registry` is EXPECTED RED until the owner has run `cargo publish`
# (ADR-0097 exit criterion 5, `RELEASING.md`): `cargo add` cannot resolve a
# name the index has never heard of, and this script says so rather than
# treating that as its own bug. Unused while ADR-0161 decision 1 holds
# ("Không publish") — kept, not removed, because publishing later stays one
# command away (ADR-0160 decision 6, ADR-0161 decision 6).
#
# `--from git` is the mode ADR-0161 decision 4 made the real gate: `--url`
# defaults to `https://github.com/tmthang86/fixbolt`, the same repository
# `docs/GETTING-STARTED.md` and `README.md` name.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

MODE=""
REGISTRY_VERSION=""
ALLOW_DIRTY=0
TAG=""
GIT_URL="https://github.com/tmthang86/fixbolt"
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
    --tag)
      TAG="${2:-}"
      shift 2
      ;;
    --url)
      GIT_URL="${2:-}"
      shift 2
      ;;
    --allow-dirty)
      ALLOW_DIRTY=1
      shift
      ;;
    *)
      echo "stranger-check: FAIL — unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ "${MODE}" != "packaged" && "${MODE}" != "registry" && "${MODE}" != "git" ]]; then
  echo "stranger-check: FAIL — usage: stranger-check.sh --from packaged | --from registry --version X | --from git --tag X [--url Y]" >&2
  exit 2
fi
if [[ "${MODE}" == "registry" && -z "${REGISTRY_VERSION}" ]]; then
  echo "stranger-check: FAIL — --from registry needs --version X" >&2
  exit 2
fi
if [[ "${MODE}" == "git" && -z "${TAG}" ]]; then
  echo "stranger-check: FAIL — --from git needs --tag X" >&2
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

  # --- target/package/ must be built from THIS commit, not a stale one ------
  # A dry run leaves these directories behind; nothing removes them when
  # `crates/` changes underneath, so a green run against yesterday's bytes
  # proves nothing about today's source (senior review of PR #104: editing
  # crates/library/src/lib.rs without re-packaging must go red, not silently
  # patch in stale sources). `.cargo_vcs_info.json` inside each packaged
  # crate records the commit `cargo package` ran at; a dirty tree is refused
  # outright (unless --allow-dirty, local iteration only) because it can
  # differ from HEAD in ways no commit sha records.
  if [[ "${ALLOW_DIRTY}" -ne 1 ]] && [[ -n "$(git -C "${ROOT}" status --porcelain 2>/dev/null)" ]]; then
    echo "stranger-check: FAIL — the working tree is dirty. A dirty tree can" >&2
    echo "    differ from HEAD in ways .cargo_vcs_info.json's commit sha cannot" >&2
    echo "    record, so a match against HEAD would prove nothing. Commit or" >&2
    echo "    stash first, or pass --allow-dirty for local iteration only." >&2
    exit 1
  fi
  head_sha="$(git -C "${ROOT}" rev-parse HEAD 2>/dev/null || true)"
  if [[ -z "${head_sha}" ]]; then
    echo "stranger-check: FAIL — could not read 'git rev-parse HEAD' (not a git checkout?)" >&2
    exit 2
  fi
  stale=()
  for name in "${PUBLISHED[@]}"; do
    info="${ROOT}/target/package/${name}-${WORKSPACE_VERSION}/.cargo_vcs_info.json"
    if [[ ! -f "${info}" ]]; then
      stale+=("${name}: no .cargo_vcs_info.json — re-run the dry run")
      continue
    fi
    sha="$(python3 -c 'import json, sys
print(json.load(open(sys.argv[1]))["git"]["sha1"])' "${info}" 2>/dev/null || true)"
    if [[ "${sha}" != "${head_sha}" ]]; then
      stale+=("${name}: packaged at ${sha:-<unreadable>}, HEAD is ${head_sha}")
    fi
  done
  if [[ "${#stale[@]}" -gt 0 ]]; then
    echo "stranger-check: FAIL — target/package/ is stale (built from a commit" >&2
    echo "    other than HEAD, per .cargo_vcs_info.json):" >&2
    printf '    %s\n' "${stale[@]}" >&2
    echo "    Run 'cargo publish --workspace --dry-run' again on this commit." >&2
    exit 1
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
elif [[ "${MODE}" == "git" ]]; then
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

  echo "== cargo add --git ${GIT_URL} --tag ${TAG} fixbolt (no [patch] — cloned straight from GitHub) =="
  add_log="${scratch}/add.log"
  if ! cargo add --git "${GIT_URL}" --tag "${TAG}" fixbolt --manifest-path "${scratch}/Cargo.toml" >"${add_log}" 2>&1; then
    cat "${add_log}" >&2
    echo "stranger-check: FAIL — could not add fixbolt from ${GIT_URL} at tag ${TAG}" >&2
    exit 1
  fi
  cat "${add_log}"

  # ADR-0161 decision 4 / plan row 8b reversal (b): the page and this run
  # must name the same release. Checked only AFTER `cargo add` has already
  # succeeded against the tag THIS RUN was asked for — a run given a tag
  # that does not exist at all (the wrong-tag reversal above) must fail with
  # that cargo/git error, not with a doc mismatch that would otherwise fire
  # first regardless of which tag is wrong.
  doc_tag="$(grep -oE 'tag = "[^"]+"' "${ROOT}/docs/GETTING-STARTED.md" | head -1 | sed -E 's/tag = "([^"]+)"/\1/')"
  if [[ -z "${doc_tag}" ]]; then
    echo "stranger-check: FAIL — docs/GETTING-STARTED.md names no tag = \"...\" install line" >&2
    exit 1
  fi
  if [[ "${doc_tag}" != "${TAG}" ]]; then
    echo "stranger-check: FAIL — docs/GETTING-STARTED.md names tag ${doc_tag}, this run checks ${TAG}" >&2
    exit 1
  fi
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

# --- --from git only: prove the build actually came from GitHub at this tag,
#     not from a local path (ADR-0161 decision 4, plan row 8b reversal (a)) --
if [[ "${MODE}" == "git" ]]; then
  compiling_line="$(grep -E '^ *Compiling fixbolt v[0-9][0-9A-Za-z.+-]* \(' "${build_log}" | head -1)"
  expected_needle="(${GIT_URL}?tag=${TAG}#"
  if [[ -z "${compiling_line}" || "${compiling_line}" != *"${expected_needle}"* ]]; then
    found_src="$(printf '%s' "${compiling_line}" | sed -E 's/^.*\(([^)]*)\)[[:space:]]*$/\1/')"
    echo "stranger-check: FAIL — fixbolt was compiled from ${found_src:-<unknown source>}, not from ${GIT_URL}?tag=${TAG}" >&2
    exit 1
  fi

  # The expected sha comes from THIS checkout, not from GitHub again — a
  # tag this checkout has never fetched cannot be verified, so that is
  # exit 2 (the script cannot run the check), not a FAIL of fixbolt itself.
  tag_sha="$(git -C "${ROOT}" rev-parse "${TAG}^{commit}" 2>/dev/null || true)"
  if [[ -z "${tag_sha}" ]]; then
    echo "stranger-check: FAIL — tag ${TAG} is not resolvable in this checkout (git fetch --tags?) — cannot compute the expected commit" >&2
    exit 2
  fi
  expected_source="git+${GIT_URL}?tag=${TAG}#${tag_sha}"
  if ! grep -qF "source = \"${expected_source}\"" "${scratch}/Cargo.lock"; then
    echo "stranger-check: FAIL — ${scratch}/Cargo.lock does not pin fixbolt to source ${expected_source}" >&2
    exit 1
  fi
  echo "stranger-check: fixbolt resolved to ${expected_source}"
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

case "${MODE}" in
  packaged) version_desc="${WORKSPACE_VERSION}" ;;
  registry) version_desc="${REGISTRY_VERSION}" ;;
  git) version_desc="tag ${TAG}" ;;
esac

cleanup_ok=1
echo "stranger-check: OK — --from ${MODE}, fixbolt ${version_desc}, Logon/Logout answered, acceptor stopped cleanly"
