# A readonly name as a command prefix is dropped, and the command still runs

> `[measured 2026-09-27]` — the phase-4 §9 boot. `scripts/boot-p4.sh` printed
> `line 604: ENGINE_CORE: readonly variable` once per arm, twenty times, and every arm ran.
> **`[to testing-skills]`**

## The shape

A script pins a setting so nobody can change it by accident:

```bash
readonly ENGINE_CORE=6
```

and later hands its settings to a child the usual way, as assignments in front of the command:

```bash
RUNS=$RUNS ENGINE_CORE=$ENGINE_CORE OUT_DIR=$dir \
  timeout 1800 scripts/w2w-baseline.sh
```

Bash refuses to assign a readonly name, even as a prefix whose only purpose is the child's
environment. It prints one line to stderr, **and then runs the command anyway**, with every
other prefix assignment in place and that one missing. A readonly variable is not exported,
so the child sees the name unset:

```text
$ bash -c 'readonly E=6; E=6 A=1 bash -c "echo ran E=\${E:-unset} A=\$A"; echo rc=$?'
bash: line 1: E: readonly variable
ran E=unset A=1
rc=0
```

(GNU bash 5.3.9, on the desk.) The exit status is the child's, so nothing that reads exit codes
sees anything wrong. The only trace is a stderr line, and in the boot that line went to the
driver's log between two arm banners, where nobody was reading.

## What it cost, and why it did not cost more

`scripts/boot-p4.sh` at `3f7a0a3` declared `readonly ENGINE_CORE=6 …` (line 176) so the engine core could not be
a knob, and passed `ENGINE_CORE=$ENGINE_CORE` as a prefix to `w2w-baseline.sh` (line 604). The boot
of 2026-09-27 (`target/boot-p4-evidence/20260927T042832Z/`) ran 20 `w2w` arms that way.
`driver.log` holds 20 lines `line 604: ENGINE_CORE: readonly variable`.

Every arm still ran on core 6, because `w2w-baseline.sh` defaults `ENGINE_CORE=${ENGINE_CORE:-6}`
(line 103), and 6 is the core the plan wanted. All 20 driver `summary.txt` files read
`pinned  engine cpu6`. **The figures are correct by coincidence**: the value the driver fixed never
reached the script, and a driver whose core differed from the child's default would have measured
the wrong core while printing its own value in `settings.txt`.

Row 2's scrape pair (`row2.sh`, metrics plan *Sửa 2*) passes `ENGINE_CORE=6` the same way but
declares nothing readonly, so it is not affected: `grep -c readonly` on its script and log reads 0,
and its eight summaries read `pinned  engine cpu6`.

## The fix

`scripts/boot-p4.sh` now hands every setting over through `env` inside one function,
`run_baseline`:

```bash
timeout --kill-after=30 "$ARM_TIMEOUT_S" env \
  RUNS="$RUNS" MESSAGES="$MESSAGES" ENGINE_CORE="$ENGINE_CORE" … "$1"
```

`env NAME=value` is an argument list given to a program, not a shell assignment, so no readonly
name can collide with it. The script keeps its `readonly` lines: the protection they give inside
the script is still wanted.

## The guard

`scripts/check-boot-p4-driver.sh` (CI job *script-logic*):

- sources the driver (`BOOT_P4_SOURCE_ONLY=1`) and runs `run_baseline` against a fake
  `w2w-baseline.sh` that prints what it received; `ENGINE_CORE=6` must be there, and bash must not
  have printed `readonly variable`;
- scans the driver's text for any name assigned after the line that made it readonly — the class,
  not the one instance — and proves the scanner sees the 2026-09-27 shape on a three-line probe.

Reversal, 2026-09-27: `run_baseline` put back to the prefix form went red on three assertions:
`want 'ENGINE_CORE=6' … got: …boot-p4.sh: line 197: ENGINE_CORE: readonly variable
ENGINE_CORE=<unset> …`, `bash printed 'readonly variable'`, and `boot-p4.sh assigns a readonly name
as a prefix or variable: 197: ENGINE_CORE`. Restored: `pass 28 fail 0`.

## The general rule

A stderr line that says a value was refused is not a failure unless something makes it one. When
a script passes settings to a child, check what the child **received**, by making the child say
it, not what the parent meant to send. A parent's `settings.txt` records the parent's intent.
