#!/usr/bin/env bash
# CLAUDE.md §2 non-negotiable 6, and STATUS.md item 68 — the same shape as
# item 62 one layer in.
#
# The `tls` job in ci.yml used to run `cargo test --test tls --test tls_wire
# --test tls_mode --test tls_settings_wire --test settings`: a hand-written
# list of file names. `tests/tls_settings_wire.rs` and, separately, three
# `#[cfg(feature = "tls")]` tests living inside `tests/settings.rs` (a file
# the list had no reason to name, since most of it is not about TLS) both
# ran on one desk and nowhere else, because the list did not name them. A
# list that must be remembered to be extended is not a gate; it is a wish.
#
# This script deletes the list. It asks the build itself which tests exist
# under FEATURE, then reads a log of an actual run under the same feature and
# requires every one of them to be accounted for in it.
#
# WHY THE LISTING HALF NO LONGER PARSES `Running` LINES. `[measured
# 2026-09-12]` run 34696713544, job "TLS, with the kernel it needs", commit
# 7323299, went red with:
#
#     FAIL — a test line appeared before any 'Running' line:
#     observe::event_loss_tests::a_full_ring_still_counts_what_it_drops: test
#
# `cargo test -- --list` prints `Running <target> (<path>)` from cargo, on
# STDERR, and the `<name>: test` lines from each test binary, on STDOUT. The
# first version of this script merged them with `2>&1` and then used their
# relative ORDER to decide which binary a test belonged to. Order between two
# streams through one pipe is not guaranteed by anything: it held on the
# owner's desk, every time, and did not hold on the GitHub runner. The desk's
# green was green by luck.
#
# So the listing half never merges the two streams again. It asks cargo for
# the compiled test binaries as machine-readable JSON
# (`--no-run --message-format=json`, taking `.executable` of every artifact
# with `profile.test == true`), then runs each binary itself with `--list`.
# Attribution is exact BY CONSTRUCTION — the names came out of that binary —
# and there is no second stream anywhere in it. As a self-check, the number
# of `<name>: test` lines parsed out of each binary must equal the
# `N tests, M benchmarks` count that same binary prints for itself.
#
# WHAT THE RECONCILE HALF CAN AND CANNOT PROMISE. LOG is the log of a real
# `cargo test` run, which merges those same two streams — so the log CANNOT
# attribute a test line to a binary, and this script does not pretend it can.
# It asserts three things that do not depend on order at all:
#
#   R1  every test name the build listed appears in LOG carrying a run
#       status (`ok`, `FAILED` or `ignored`) at least as many times as the
#       build listed it. Multiplicity matters: one test name in this
#       workspace is listed by two binaries (`[measured 2026-09-12]`
#       `relabelling_to_the_same_sender_reproduces_the_corpus_bytes`), and
#       counting rather than set-membership is what keeps one log line from
#       accounting for both.
#   R2  the set of test binaries named in LOG's `Running` lines — compared by
#       the executable's file name, which carries cargo's hash and is
#       therefore unique — is EXACTLY the set the build produced. Neither a
#       binary that never ran nor a binary from some other build passes. This
#       is per-binary presence, which the log does give exactly; it is not
#       per-test attribution, which it does not.
#   R3  the number of listed tests recorded in LOG as `ignored` is at most
#       IGNORED_CEILING, which is 0 (below). `ok` and `FAILED` both mean "it
#       ran". `ignored` means the test exists and did NOT execute, and a test
#       that did not execute is exactly what this gate exists to catch. The
#       senior review of this branch took a real log, turned every `ok` into
#       `ignored`, and got `326 listed, 326 accounted for (326 ignored)` and
#       exit 0 out of the version before this one — the whole TLS suite
#       switched off, green. `[measured 2026-09-12]` the same log through R3
#       now ends `FAIL R3 — 326 ignored, ceiling 0`, naming every one of them
#       above it. The ceiling is a constant in this file, not an environment
#       variable, on the pattern check-indexing-debt.sh already uses: a
#       ratchet that only a commit can move.
#
# --tests builds the lib's unit tests plus every integration test target
# (anything cargo would run for `cargo test --tests`); it does not build
# benches (`harness = false` targets) or examples, so a test gated behind
# either is out of scope for this script by construction, same as it is out
# of scope for the `--tests` flag itself. DOCTESTS are out of scope too,
# and that one is a real hole rather than a definitional one:
# `[measured 2026-09-12]` `cargo test -p fixbolt-engine --doc --features
# tls -- --list` lists 4 doctests, the same 4 that `--doc` lists without the
# feature — so none of them is `cfg`-gated on `tls` today — and ci.yml's
# `tls` job runs none of them, because `--tests` and `--doc` are separate
# cargo invocations. Closing that hole is a change to ci.yml as much as to
# this file, and `--doc` has no `--no-run`, so the listing half above would
# need a second shape for it.
#
# Usage: check-feature-gated-tests-ran.sh PACKAGE FEATURE LOG
#   PACKAGE  the -p argument to cargo, e.g. fixbolt-engine. Exactly one
#            crate: a feature flag means what it reads as only at this
#            scope, not at the workspace's — see
#            docs/reference/feature-flags-unify-across-a-workspace.md. This
#            script deliberately has no --all / --workspace mode.
#   FEATURE  the --features argument, e.g. tls.
#   LOG      path to a file holding the stdout+stderr of
#            `cargo test -p PACKAGE --tests --features FEATURE --no-fail-fast`
#            (or a log that reads the same way — see the reversals below).
#
# Needs: cargo, and jq (for the artifact JSON).
#
# A `cargo` failure while building the list (a misspelt feature name, a build
# error), or a test binary that will not even list itself, is reported as the
# error and exits 2; it is never read as a pass.
set -uo pipefail

# R3's ratchet. 0 is the honest count today; raising it needs a commit that
# says which test is ignored and why.
IGNORED_CEILING=0

if [ "$#" -ne 3 ]; then
  echo "usage: $0 PACKAGE FEATURE LOG" >&2
  exit 2
fi

package="$1"
feature="$2"
log_file="$3"

if [ ! -f "${log_file}" ]; then
  echo "FAIL — log file not found: ${log_file}" >&2
  exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${root}" || exit 2

# --- 1. Build: ask cargo for the test binaries themselves, as JSON. Nothing
# here depends on the order of two streams, because nothing here reads
# cargo's human output at all: every line that is not a JSON object is
# dropped by `fromjson?`, so cargo's stderr cannot be mistaken for data.
build_out="$(cargo test -p "${package}" --tests --features "${feature}" --no-run --message-format=json 2>&1)"
build_status=$?
if [ "${build_status}" -ne 0 ]; then
  echo "FAIL — cargo could not build the tests for -p ${package} --features ${feature}:" >&2
  echo "${build_out}" >&2
  exit 2
fi

mapfile -t artifacts < <(
  printf '%s\n' "${build_out}" | jq -R -r '
    fromjson? // empty
    | select(.reason == "compiler-artifact")
    | select(.profile.test == true)
    | select(.executable != null)
    | [.executable, .target.src_path] | @tsv
  ' | sort -u
)

binaries="${#artifacts[@]}"
if [ "${binaries}" -eq 0 ]; then
  echo "FAIL C0 — cargo reported no test binaries for -p ${package} --tests --features ${feature}. A count of zero from a tool that enumerates is a broken invocation, not a clean build." >&2
  exit 1
fi

# --- 2. List: run each binary directly. `--list` (the default format, not
# terse) ends with its own `N tests, M benchmarks` line, and that number is
# checked against the lines parsed — a parse that drifts goes red here rather
# than quietly listing fewer tests than exist.
declare -A listed_n=()    # test name -> how many binaries list it
declare -A listed_by=()   # test name -> which ones, for the FAIL message
declare -A build_exe=()   # executable file name -> its source path
listed_count=0

for entry in "${artifacts[@]}"; do
  exe="${entry%%$'\t'*}"
  src="${entry#*$'\t'}"
  disp="${src#"${root}"/}"
  build_exe["$(basename "${exe}")"]="${disp}"

  list_out="$("${exe}" --list 2>&1)"
  list_status=$?
  if [ "${list_status}" -ne 0 ]; then
    echo "FAIL — ${disp} (${exe}) could not list its own tests, exit ${list_status}:" >&2
    echo "${list_out}" >&2
    exit 2
  fi

  parsed=0
  claimed=-1
  while IFS= read -r line; do
    if [[ "${line}" =~ ^(.+):[[:space:]]test$ ]]; then
      name="${BASH_REMATCH[1]}"
      listed_n["${name}"]=$(( ${listed_n["${name}"]:-0} + 1 ))
      if [ -z "${listed_by[${name}]+x}" ]; then
        listed_by["${name}"]="${disp}"
      else
        listed_by["${name}"]="${listed_by[${name}]}, ${disp}"
      fi
      parsed=$((parsed + 1))
      continue
    fi
    if [[ "${line}" =~ ^([0-9]+)[[:space:]]tests?,[[:space:]][0-9]+[[:space:]]benchmarks?$ ]]; then
      claimed="${BASH_REMATCH[1]}"
    fi
  done <<<"${list_out}"

  if [ "${claimed}" -lt 0 ]; then
    echo "FAIL — ${disp} printed no 'N tests, M benchmarks' line; this script cannot confirm it read the whole listing." >&2
    exit 2
  fi
  if [ "${parsed}" -ne "${claimed}" ]; then
    echo "FAIL — ${disp} lists ${claimed} tests and this script parsed ${parsed}." >&2
    exit 2
  fi
  listed_count=$((listed_count + parsed))
done

if [ "${listed_count}" -eq 0 ]; then
  echo "FAIL C0 — listed nothing: ${binaries} test binaries for -p ${package} --features ${feature} and not one test in them. This is STATUS.md item 62 one layer up." >&2
  exit 1
fi

# --- 3. Reconcile against LOG. Order-free: names are counted, binaries are
# matched by executable file name. See R1/R2/R3 in the header for exactly
# what this half does and does not promise.
declare -A log_ran=()       # test name -> times seen with ok/FAILED/ignored
declare -A log_ignored=()   # test name -> times seen ignored
declare -A log_exe=()       # executable file name -> times a Running line named it
running_lines=0

# LOG is read through a filter that removes ANSI colour escapes. `[measured
# 2026-09-12]` this half read **0 Running lines** out of a CI log in which
# every one of the 38 binaries had run, because cargo colours its output on
# GitHub's runner and not on a piped desk terminal: the runner writes
# `ESC[1mESC[92m     RunningESC[0m tests/foo.rs (...)`, and `^[[:space:]]*Running`
# cannot match that. The desk was green for the third time on a gate CI was
# red on — see
# docs/reference/two-streams-through-one-pipe-have-no-guaranteed-order.md.
# The filter is here rather than in ci.yml on purpose: this script is handed a
# log it did not produce, so it may not assume anything about how that log was
# generated, `CARGO_TERM_COLOR` included.
while IFS= read -r line; do
  if [[ "${line}" =~ ^[[:space:]]*Running[[:space:]].*\(([^()]*)\)[[:space:]]*$ ]]; then
    running_lines=$((running_lines + 1))
    base="$(basename "${BASH_REMATCH[1]}")"
    log_exe["${base}"]=$(( ${log_exe["${base}"]:-0} + 1 ))
    continue
  fi
  if [[ "${line}" =~ ^test[[:space:]](.+)[[:space:]]\.\.\.[[:space:]](ok|FAILED|ignored)(,.*)?$ ]]; then
    name="${BASH_REMATCH[1]}"
    status="${BASH_REMATCH[2]}"
    # libtest decorates the NAME of a `#[should_panic]` test in the run log
    # (`test foo - should panic ... ok`) and does not decorate it in `--list`
    # (`foo: test`). `[measured 2026-09-12]` this made the gate a FALSE RED on
    # crates/engine/tests/settings_wire.rs's
    # `a_socket_that_is_held_open_and_silent_is_not_read_as_a_refusal` — 325 of
    # 326 accounted for on a log in which all 326 had run. The trailing
    # `, <reason>` after `ignored` is stripped by the regex above for the same
    # reason: `#[ignore = "why"]` prints the reason on the line.
    name="${name% - should panic}"
    log_ran["${name}"]=$(( ${log_ran["${name}"]:-0} + 1 ))
    if [ "${status}" = "ignored" ]; then
      log_ignored["${name}"]=$(( ${log_ignored["${name}"]:-0} + 1 ))
    fi
  fi
done < <(sed -E $'s/\x1b\\[[0-9;]*[a-zA-Z]//g' "${log_file}")

fail=0
accounted=0
ignored_count=0

# R1 — every listed name, as many times as it was listed.
for name in "${!listed_n[@]}"; do
  want="${listed_n[${name}]}"
  got="${log_ran[${name}]:-0}"
  if [ "${got}" -ge "${want}" ]; then
    accounted=$((accounted + want))
  else
    accounted=$((accounted + got))
    echo "FAIL R1 — ${name} is in the ${feature} build (listed by ${listed_by[${name}]}, ${want}×) and the log records it running ${got}×" >&2
    fail=1
  fi
done

# R2 — exactly the binaries the build produced, no more and no fewer.
for base in "${!build_exe[@]}"; do
  if [ -z "${log_exe[${base}]+x}" ]; then
    echo "FAIL R2 — ${build_exe[${base}]} (${base}) was built for --features ${feature} and no 'Running' line in the log names it" >&2
    fail=1
  fi
done
for base in "${!log_exe[@]}"; do
  if [ -z "${build_exe[${base}]+x}" ]; then
    echo "FAIL R2 — the log has a 'Running' line for ${base}, which is not one of the ${binaries} test binaries this build produced" >&2
    fail=1
  fi
done

# R3 — an ignored test did not run, and this gate is about tests running.
for name in "${!listed_n[@]}"; do
  n="${log_ignored[${name}]:-0}"
  if [ "${n}" -gt 0 ]; then
    ignored_count=$((ignored_count + n))
    echo "FAIL R3 — ${name} is in the ${feature} build (listed by ${listed_by[${name}]}) and the log records it as ignored ${n}×" >&2
  fi
done
if [ "${ignored_count}" -gt "${IGNORED_CEILING}" ]; then
  echo "FAIL R3 — ${ignored_count} ignored, ceiling ${IGNORED_CEILING}. An ignored test did not execute; this gate exists to notice that." >&2
  fail=1
fi

summary="${package} --features ${feature}: ${binaries} binaries, ${running_lines} Running lines, ${listed_count} listed, ${accounted} accounted for, ${ignored_count} ignored (ceiling ${IGNORED_CEILING})"

if [ "${fail}" -ne 0 ]; then
  echo "check-feature-gated-tests-ran: FAIL — ${summary}" >&2
  exit 1
fi

echo "check-feature-gated-tests-ran: ok — ${summary}"
