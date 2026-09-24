# A `read ... <<<"$(fn)"` line reports the function's exit status as `read`'s, not its own

> `[found 2026-09-24]` — found reading
> [scripts/check-no-kernel-sleep-by-ctxt.sh](../../scripts/check-no-kernel-sleep-by-ctxt.sh)
> while writing [plans/2026-09-24-p4-sqlite-store.md](../plans/2026-09-24-p4-sqlite-store.md)
> row 7's documentation. `[fixed 2026-09-24]` in the same PR (#109) after its senior review
> (finding L1) showed the gate could **PASS** on a `w2w` that never ran in `hft` mode; the fix
> and its guard are at the end of this page. The sections in between describe the script as it
> was, because the trap is in the shell idiom, not in this one script.

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

## It could be green overall

The `standard` half runs after the `hft` half through the same idiom. When **both** halves fail
the same way (an argument `w2w` refuses, say), the `standard` branch's own `elif -z
"${red_line}"` sets `rc=1` and the script exits red — through its `standard` half, with a
misleading `GREEN ok` above it. But when **only the `hft` half fails**, the `standard` half runs
normally, prints `RED ok`, and the script prints `PASS` and exits 0. The senior review of PR
#109 showed exactly that with a fake `w2w` that crashes in `hft` mode only:

```
hft voluntary 
GREEN ok — engine thread made 0 voluntary context switches
...
PASS
script exit 0
```

A gate for non-negotiable 4 passed on a run that measured nothing.

## The fix

Capture the function's output first, so `|| exit 1` sees the function's own status, then read
it, then refuse anything that is not two numbers. In the script this is one helper both halves
call, `read_half`:

```bash
read_half() {
  local mode="$1" got voluntary status
  if ! got="$(run_and_read "${mode}")"; then
    echo "FAIL: --mode ${mode} produced no result — w2w did not run as asked (…)" >&2
    exit 1
  fi
  read -r voluntary status <<<"${got}"
  if ! [[ "${voluntary}" =~ ^[0-9]+$ && "${status}" =~ ^[0-9]+$ ]]; then
    echo "FAIL: --mode ${mode} produced no result — read voluntary '…' and exit '…', not two numbers" >&2
    exit 1
  fi
  echo "${voluntary} ${status}"
}
hft_half="$(read_half hft)" || exit 1
read -r hft_voluntary hft_status <<<"${hft_half}"
```

**The idiom to avoid anywhere**: `read … <<<"$(f)" || …` never reports `f`'s failure. Capture
with `x="$(f)" || …`, then `read … <<<"${x}"`.

## Guard

[scripts/check-ctxt-gate-refuses-a-failed-run.sh](../../scripts/check-ctxt-gate-refuses-a-failed-run.sh),
run by CI's `script-logic` job. No cargo: it points the gate's `W2W_BIN` at three fake `w2w`
scripts — `healthy` (the gate must `PASS`, so the harness is not a check that is always red),
`crash-hft` (the gate must exit 1 and say `FAIL: --mode hft produced no result`, and never
`GREEN ok`), `crash-standard` (exit 1, `FAIL: --mode standard produced no result`). Against the
unfixed script `crash-hft` read `FAIL crash-hft: the gate exited 0, expected 1` — the defect
above, red, before the fix.
