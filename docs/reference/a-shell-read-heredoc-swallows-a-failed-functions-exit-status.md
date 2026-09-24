# A `read ... <<<"$(fn)"` line reports the function's exit status as `read`'s, not its own

> `[found 2026-09-24]` — found reading
> [scripts/check-no-kernel-sleep-by-ctxt.sh](../../scripts/check-no-kernel-sleep-by-ctxt.sh)
> while writing [plans/2026-09-24-p4-sqlite-store.md](../plans/2026-09-24-p4-sqlite-store.md)
> row 7's documentation. **Pre-existing on the script; not fixed here** — recorded in the
> plan's step-6 commit and in `STATUS.md` open items as something for the architect or a later
> step to fix; this page is the write-up CLAUDE.md §4 asks for ("if it cost you, write it down"),
> not a patch.

## What the script does

`scripts/check-no-kernel-sleep-by-ctxt.sh` proves non-negotiable 4's second half without a
tracer (ADR-0072): it runs `tools/w2w --assert-no-voluntary-switches` in `hft` mode (must print
`voluntary 0`, GREEN) and again in `standard` mode (must print a nonzero count and exit nonzero,
RED — the reversal built into the gate itself). Both runs go through one helper:

```bash
run_and_read() {
  local mode="$1" out="${TMP}/out.$1" ran voluntary
  "${BIN}" --messages 300 --warmup 50 --hold-ms 400 --mode "${mode}" \
    --assert-no-voluntary-switches ${W2W_EXTRA:-} \
    > "${out}" 2>&1
  local status=$?
  ran="$(grep -oE '^mode: [a-z]+' "${out}" | head -1 | cut -d' ' -f2)"
  if [[ "${ran}" != "${mode}" ]]; then
    echo "w2w ran mode '${ran:-<none>}' when '${mode}' was asked for" >&2
    return 1
  fi
  voluntary="$(grep -oE '^engine-ctxt voluntary [0-9]+' "${out}" | head -1 | grep -oE '[0-9]+$')"
  if [[ -z "${voluntary}" ]]; then
    echo "no 'engine-ctxt voluntary' line in w2w's output for --mode ${mode}" >&2
    return 1
  fi
  echo "${voluntary} ${status}"
}
```

and each call site reads it the same way:

```bash
read -r hft_voluntary hft_status <<<"$(run_and_read hft)" || exit 1
```

## What happens when `w2w` fails to run

If `run_and_read` hits either of its own `return 1` cases — `w2w` ran the wrong mode, or printed
no `engine-ctxt voluntary` line, which is what "w2w fails to run [as expected]" looks like from
this script's side — it writes its diagnostic to **stderr** and returns 1 with **no stdout**.
The call site is `read -r hft_voluntary hft_status <<<"$(run_and_read hft)"`: bash runs the
command substitution, captures its (empty) stdout, and feeds that to `read` as a here-string.
**`read`'s own exit status is what `|| exit 1` sees — not `run_and_read`'s.** `read` on an empty
here-string succeeds (there is a line to read, it is just empty), so `hft_voluntary` and
`hft_status` are both set to the empty string, and the `|| exit 1` never fires. The function's
failure is swallowed.

Downstream, `[[ "${hft_status}" -ne 0 ]]` and the following `elif` compare an **empty string**
with `-ne`/`-eq`. Bash's `[[ ... ]]` arithmetic comparison on a non-integer operand prints
`integer expression expected` to stderr and the test evaluates false — so **both** the `if` and
the `elif` in that chain read false, execution falls through to the `else`, and the script prints:

```
GREEN ok — engine thread made 0 voluntary context switches
```

for a run where `w2w` never produced a usable result at all.

## Why this is not silently green overall

The `standard` half runs after the `hft` half regardless, through the same swallowed-failure
path if it also fails, or normally otherwise. In the case observed while reading this script (an
argument change that made `w2w` print nothing matching `engine-ctxt voluntary`), the `standard`
branch's own `elif -z "${red_line}"` catches it: with no `engine-ctxt voluntary` line, `red_line`
is also empty, and that branch's `FAIL: --mode standard exited ... but printed no assertion
message` sets `rc=1`, which the script's final `exit "${rc}"` returns. **The script still exits
red — through its `standard` half's own check, not through the `hft` half where the failure
actually happened** — and the misleading `GREEN ok` line stays in the transcript above a `FAIL`
that gives no reason to suspect the `hft` half was the one that was actually broken.

## The fix this page does not make

Two independent fixes would close this, and neither is applied here (out of scope for a
documentation-only step): change every call site to `voluntary=$(run_and_read hft)status=$?; ...`-style
capture that preserves `run_and_read`'s real exit status, and change the empty-vs-comparison
paths to test `[[ -z "${hft_voluntary}" ]]` before doing arithmetic on it.

## Guard

**None yet.** No test or script reverses this behaviour — there is no case in the repository
that deliberately makes `run_and_read` fail and asserts the script reports that failure honestly
rather than printing `GREEN ok`. Written down per the trap's own rule (*"every recorded trap
gets a regression test"*, `CLAUDE.md` §4) as owed, not as done.
