#!/usr/bin/env bash
# `promtool check metrics` is the format oracle for the Prometheus text
# exposition format this crate writes — ADR-0171 decision 4. Our own encoder
# checked by our own encoder proves nothing about Prometheus (ADR-0171
# *Context* item 2); this pipes a real scrape through the tool the format's
# owner ships, on a pinned release, the same pattern `scripts/interop-qfj.sh`
# already uses for a pinned JVM oracle (ADR-0130).
#
# A Go binary enters CI as a TEST ORACLE here — never a build dependency,
# never shipped (ADR-0171 *Consequences*, "bad, and accepted"). It is pinned
# by version AND by the SHA-256 Prometheus's own release publishes in
# `sha256sums.txt`, checked before anything downloaded is ever run. A mismatch
# stops the script before `promtool` executes a single byte.
#
# It is fetched into `target/promtool-pin/`, never into the repository tree —
# `target/` is gitignored by cargo itself, so this needs no entry of its own,
# and unlike `vendor/` it is not a place this repository stores fetched
# assets it wants to keep across a `cargo clean`.
#
# `crates/metrics/examples/scrape_fixture.rs` is the input: one scrape of a
# fixture holding all three deployment shapes a series can come from
# (RingDispatch, the real `serve` front door, `with_events`), so this checks
# the format of every series `crates/metrics/src/series.rs` defines, not just
# whichever ones a minimal engine happens to publish.
#
#   scripts/check-metrics-format.sh              # fail if promtool is absent
#   scripts/check-metrics-format.sh --allow-skip # a laptop without network
#
# Absence is not a pass (ADR-0171 decision 4): missing `promtool`, the wrong
# platform, no network, or a checksum mismatch all print
# `SKIPPED, NOT PASSED` and exit non-zero, so a laptop cannot mistake "I did
# not check" for "it is fine" — unless `--allow-skip` says that is accepted
# for this run.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE="${REPO_ROOT}/target/promtool-pin"

# Pinned 2026-09-24: version and the linux-amd64 SHA-256 from that release's
# own `sha256sums.txt` (checked against the fetched bytes below, not trusted
# from this comment). Moving this is deliberate: bump the version, read what
# `promtool`'s lints say about the current dashboard and fixture, and change
# both numbers in the same commit as whatever that surfaces.
PROM_VERSION="3.14.0"
PROM_OS="linux"
PROM_ARCH="amd64"
PROM_TARBALL="prometheus-${PROM_VERSION}.${PROM_OS}-${PROM_ARCH}.tar.gz"
PROM_SHA256="f665c6da19eb7ba399c915d30c7d9793c9b417bf8a749b504bc470678631478d"
PROM_URL="https://github.com/prometheus/prometheus/releases/download/v${PROM_VERSION}/${PROM_TARBALL}"

ALLOW_SKIP=0
if [[ "${1:-}" == "--allow-skip" ]]; then
  ALLOW_SKIP=1
fi

skip() {
  echo "check-metrics-format: SKIPPED, NOT PASSED — $1" >&2
  if [[ "${ALLOW_SKIP}" -eq 1 ]]; then
    echo "check-metrics-format: --allow-skip given — exiting 0 having checked nothing against promtool" >&2
    exit 0
  fi
  echo "check-metrics-format: absence is not a pass; pass --allow-skip only if that is truly accepted for this run" >&2
  exit 1
}

uname_s="$(uname -s)"
uname_m="$(uname -m)"
if [[ "${uname_s}" != "Linux" || "${uname_m}" != "x86_64" ]]; then
  skip "pinned to ${PROM_OS}-${PROM_ARCH} only; this machine reports ${uname_s}-${uname_m}"
fi

command -v cargo >/dev/null || skip "cargo is not installed"

PROMTOOL="${CACHE}/prometheus-${PROM_VERSION}.${PROM_OS}-${PROM_ARCH}/promtool"

fetch_promtool() {
  command -v curl >/dev/null || skip "curl is not installed, cannot fetch promtool"
  command -v sha256sum >/dev/null || skip "sha256sum is not installed, cannot verify promtool"
  mkdir -p "${CACHE}"
  local tarball="${CACHE}/${PROM_TARBALL}"
  echo "check-metrics-format: fetching promtool ${PROM_VERSION} (pinned)" >&2
  if ! curl -sL --max-time 120 -o "${tarball}.part" "${PROM_URL}"; then
    rm -f "${tarball}.part"
    skip "could not download ${PROM_URL} — no network?"
  fi
  mv "${tarball}.part" "${tarball}"
  local got
  got="$(sha256sum "${tarball}" | cut -d' ' -f1)"
  if [[ "${got}" != "${PROM_SHA256}" ]]; then
    echo "check-metrics-format: CHECKSUM MISMATCH: ${PROM_TARBALL}" >&2
    echo "  pinned:  ${PROM_SHA256}" >&2
    echo "  fetched: ${got}" >&2
    rm -f "${tarball}"
    echo "check-metrics-format: refusing to run an unverified promtool; move the pin and the reason for moving it into the same commit." >&2
    exit 1
  fi
  tar xzf "${tarball}" -C "${CACHE}"
}

if [[ ! -x "${PROMTOOL}" ]]; then
  fetch_promtool
fi
[[ -x "${PROMTOOL}" ]] || skip "promtool did not appear at ${PROMTOOL} after fetching"

echo "check-metrics-format: $("${PROMTOOL}" --version 2>&1 | head -1)" >&2

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT
BODY="${TMP}/scrape.prom"

if ! cargo run -q -p fixbolt-metrics --example scrape_fixture >"${BODY}" 2>"${TMP}/stderr.log"; then
  echo "check-metrics-format: FAIL — scripts/scrape_fixture did not run cleanly" >&2
  cat "${TMP}/stderr.log" >&2
  exit 1
fi
if [[ ! -s "${BODY}" ]]; then
  echo "check-metrics-format: FAIL — the fixture printed no output" >&2
  exit 1
fi

if ! "${PROMTOOL}" check metrics --extended <"${BODY}" >"${TMP}/promtool.log" 2>&1; then
  echo "check-metrics-format: FAIL — promtool found a problem in the exposition format" >&2
  cat "${TMP}/promtool.log" >&2
  exit 1
fi

series="$(grep -c '^fixbolt_' "${BODY}" || true)"
echo "check-metrics-format: SUCCESS — promtool ${PROM_VERSION} found no problem across ${series} sample line(s) from scrape_fixture"
exit 0
