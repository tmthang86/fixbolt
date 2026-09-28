#!/usr/bin/env bash
# scripts/ci-evidence.sh <commit-sha> — CLAUDE.md §9's last box: "a green CI
# run is named, by id, for the commit being closed." ADR-0214 makes that
# harder to read by eye: a `CI` run's own conclusion is green even when
# every full-tier job in it was SKIPPED (the fast tier alone, or a stray
# label's NOOP run — decision 2's `if:`), because a job GitHub never started
# reports no failure. This script reads GitHub's own record of the run and
# refuses in exactly that case, rather than accepting the run's top-level
# `conclusion` at face value.
#
# What it does: finds the `CI` and `Docs` workflow runs for <commit-sha>,
# then judges the CI run job by job — not its overall conclusion — against
# the fixed lists of job NAMES below (the fast tier, five jobs; the full
# tier, eleven). It refuses unless every one of those sixteen is `success`,
# neither run's headSha differs from <commit-sha>, and the Docs run is
# `success`. `drop-full-ci-label` (ADR-0214 decision 2) is reported on its
# own line, never folded into the verdict: a failed label removal is not a
# red gate.
#
# Why job NAMES, not the workflow's job ids (`gates`, `bench`, …): the GitHub
# Actions jobs API returns each job's display `name:` and nothing else that
# identifies which `jobs:` key produced it — there is no field for the YAML
# id. The lists below are therefore this script's own copy of every job's
# `name:` in ci.yml; if a job is renamed there, this file is not updated by
# any machine and silently stops finding that job (reported as `missing`,
# which this script treats as a red gate, not as "job removed on purpose") —
# update the matching entry below in the same commit that renames the job.
#
# What it cannot see: whether the pull request that carried a `full-ci` run
# is the one being closed — it trusts <commit-sha> alone, the same way
# `gh run list --commit` does. A run still IN PROGRESS reads as "not success
# yet", the same as a failed one; re-run this once the run has finished.
#
# Needs: `gh`, authenticated (e.g. `GH_TOKEN="$(gh auth token --user
# <you>)"`), and `jq`. Read-only: makes no GitHub API writes.

set -euo pipefail

# The fast tier (ADR-0214 decision 2): every one of these runs on every
# pull-request push. Names as ci.yml's `name:` field spells them today.
FAST_TIER_NAMES=(
  "Lint config enforces non-negotiable 7"
  "fmt · clippy · test"
  "Builds with nothing optional installed"
  "The indexing debt only goes down"
  "fixbolt-dict builds from nothing but the crate itself (no vendor/)"
)

# The full tier: added on push to main, workflow_dispatch, or a pull request
# carrying the label full-ci.
FULL_TIER_NAMES=(
  "Benchmarks run, and the machine-independent ones must pass"
  "cargo-semver-checks against the newest release tag HEAD descends from (blocking, ADR-0162)"
  "Every feature set to depth two builds, lints and documents"
  "Every dependency is permissively licensed and unadvised"
  "Both roles, against a real libquickfix"
  "Both roles, plaintext and TLS, against QuickFIX/J"
  "TLS, with the kernel it needs"
  "The engine thread never sleeps in the kernel"
  "A standard engine gives the core back"
  "The published crates package, dry-run publish, and build from nothing but the .crate contents (no vendor/)"
  "FIXP spike — fixbolt-sbe against Artio's Binary EntryPoint (blocking, ADR-0140)"
)

LABEL_JOB_NAME="Remove the one-shot full-ci label after the full tier it caused"

usage() {
  cat <<'EOF'
usage: scripts/ci-evidence.sh <commit-sha>

Names the `CI` and `Docs` GitHub Actions runs for <commit-sha> and refuses
unless the CI run's full tier ran and all sixteen gate jobs are green
(ADR-0214). `drop-full-ci-label`'s conclusion is reported separately; it is
never counted as a gate. Requires `gh` (authenticated) and `jq`.
EOF
}

if [ "$#" -eq 0 ]; then
  usage >&2
  exit 2
fi
if [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
  usage
  exit 0
fi
if [ "$#" -ne 1 ]; then
  usage >&2
  exit 2
fi

SHA="$1"

command -v gh >/dev/null 2>&1 || { echo "ci-evidence: FAIL — gh CLI not found on PATH" >&2; exit 2; }
command -v jq >/dev/null 2>&1 || { echo "ci-evidence: FAIL — jq not found on PATH" >&2; exit 2; }

status=0

find_run() {
  # $1 = workflow name ("CI" or "Docs"). Picks the highest run id among any
  # runs GitHub lists for this commit and this workflow — a commit force-
  # pushed or re-run keeps only its newest attempt as evidence.
  gh run list --commit "${SHA}" --workflow "$1" \
    --json databaseId,headSha,conclusion,status,event,url \
    --jq 'sort_by(.databaseId) | last // empty' 2>/dev/null || true
}

ci_run_json="$(find_run CI)"
docs_run_json="$(find_run Docs)"

if [ -z "${ci_run_json}" ]; then
  echo "ci-evidence: FAIL — no CI run found for ${SHA}. A docs-only commit runs no CI run by design (ADR-0214 decision 1); that is not evidence for a commit closing a plan, which needs the full tier." >&2
  status=1
fi
if [ -z "${docs_run_json}" ]; then
  echo "ci-evidence: FAIL — no Docs run found for ${SHA}" >&2
  status=1
fi

if [ "${status}" -ne 0 ]; then
  exit 1
fi

ci_id="$(printf '%s' "${ci_run_json}" | jq -r '.databaseId')"
ci_head="$(printf '%s' "${ci_run_json}" | jq -r '.headSha')"
ci_url="$(printf '%s' "${ci_run_json}" | jq -r '.url')"
docs_id="$(printf '%s' "${docs_run_json}" | jq -r '.databaseId')"
docs_head="$(printf '%s' "${docs_run_json}" | jq -r '.headSha')"
docs_url="$(printf '%s' "${docs_run_json}" | jq -r '.url')"
docs_conclusion="$(printf '%s' "${docs_run_json}" | jq -r '.conclusion')"

if [ "${ci_head}" != "${SHA}" ]; then
  echo "ci-evidence: FAIL — CI run ${ci_id}'s headSha is ${ci_head}, not ${SHA}" >&2
  status=1
fi
if [ "${docs_head}" != "${SHA}" ]; then
  echo "ci-evidence: FAIL — Docs run ${docs_id}'s headSha is ${docs_head}, not ${SHA}" >&2
  status=1
fi
if [ "${docs_conclusion}" != "success" ]; then
  echo "ci-evidence: FAIL — Docs run ${docs_id} concluded '${docs_conclusion}', not success" >&2
  status=1
fi

jobs_json="$(gh run view "${ci_id}" --json jobs --jq '.jobs')"

# GitHub truncates job names longer than 100 characters to first 97 chars + "..." (observed: "The published crates package, dry-run publish, and build from nothing but the .crate contents (no...")
truncate_job_name() {
  if [ ${#1} -gt 100 ]; then
    printf '%s...' "${1:0:97}"
  else
    printf '%s' "$1"
  fi
}

job_conclusion() {
  local name
  name="$(truncate_job_name "$1")"
  printf '%s' "${jobs_json}" | jq -r --arg n "${name}" '([.[] | select(.name == $n) | .conclusion] | first) // "missing"'
}

skipped_full=""
red_gates=""

for n in "${FULL_TIER_NAMES[@]}"; do
  c="$(job_conclusion "${n}")"
  if [ "${c}" = "skipped" ]; then
    skipped_full="${skipped_full}
  - ${n}"
  elif [ "${c}" != "success" ]; then
    red_gates="${red_gates}
  - ${n}: ${c}"
  fi
done

for n in "${FAST_TIER_NAMES[@]}"; do
  c="$(job_conclusion "${n}")"
  if [ "${c}" != "success" ]; then
    red_gates="${red_gates}
  - ${n}: ${c}"
  fi
done

label_conclusion="$(job_conclusion "${LABEL_JOB_NAME}")"

if [ -n "${skipped_full}" ]; then
  echo "ci-evidence: FAIL — CI run ${ci_id} (${ci_url}) skipped full-tier job(s), so this is a fast-tier or no-op run, not evidence for a closing commit:${skipped_full}" >&2
  status=1
fi
if [ -n "${red_gates}" ]; then
  echo "ci-evidence: FAIL — CI run ${ci_id} (${ci_url}) has non-success gate job(s):${red_gates}" >&2
  status=1
fi

if [ "${status}" -ne 0 ]; then
  exit 1
fi

echo "ci-evidence: ok — full tier, commit ${SHA}"
echo "  CI   run ${ci_id} — ${ci_url}"
echo "  Docs run ${docs_id} — ${docs_url}"
echo "  drop-full-ci-label: ${label_conclusion} (not a gate; 'missing' means this run had no such job — a push or workflow_dispatch run, which never carries it)"
