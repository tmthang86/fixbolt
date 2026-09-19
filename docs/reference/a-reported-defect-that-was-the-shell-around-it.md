# A reported defect that was the shell around the script, not the script

`[measured 2026-09-19]` — found during phase 2 step A3, auditing a report that
`scripts/interop.sh` printed `cmake is required` and exited `0`.

## The claim

`scripts/interop.sh` was run on a desk without `cmake` installed. It printed only
`cmake is required` and exited `0` — a missing prerequisite that would have let a caller
record a pass for a run that never built anything. CLAUDE.md §12 treats a report like this
as a claim, not a defect, until reproduced.

## What the reversal actually showed

`scripts/interop.sh:76-78` is:

```sh
for tool in cmake g++ git cargo; do
  command -v "${tool}" >/dev/null || { echo "${tool} is required" >&2; exit 1; }
done
```

Reproducing "no cmake on `PATH`" without uninstalling anything (a `PATH` built from every
`/usr/bin` entry except `cmake`) and running the script directly:

```text
cmake is required
direct exit=1
```

Non-zero, as written, on the first commit that added this file (`571844f0`, 2026-09-02) —
this line has never returned 0. A grep of every other `scripts/*.sh` prerequisite check
(`command -v ... || { echo ...; exit N; }`, `fetch-quickfix-assets.sh:30`,
`check-ktls-on-a-plain-socket.sh:31`, `measure-isolation-cost.sh:49`) found the same:
every one exits 1 or 2. **No script in this repository has a prerequisite check that prints
and returns 0.**

## What does reproduce it

The same run, through a pipe with no `pipefail` in the calling shell:

```sh
bash -c 'PATH=... scripts/interop.sh 2>&1 | tee interop.log'
```

```text
cmake is required
piped exit=0
```

This is `docs/reference/reading-the-output-you-grepped-for.md`'s fourth rule: *a pipeline's
exit status is its last command's*. `scripts/interop.sh` cannot fix a pipe built around it
by whoever calls it — CI already guards its own call with `set -o pipefail`
(`.github/workflows/ci.yml`, the `interop` job) before piping to `tee`. An ad-hoc invocation
that skips that line reproduces exactly the reported symptom.

## The fix

None needed in `scripts/interop.sh` or in any other `scripts/*.sh`: verified, not fixed.
CLAUDE.md §7 — *read the output, not the exit status* — extends here to *read which
command's* exit status a pipe reports.

## The rule

A reported defect is reproduced before it is routed to a fix. This one reproduced only one
way, and that way was outside the file named in the report.
