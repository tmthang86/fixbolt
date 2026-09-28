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

`scripts/check-boot-p4-driver.sh` (CI job *lint-config*, fast tier; *script-logic* until ADR-0214):

- sources the driver (`BOOT_P4_SOURCE_ONLY=1`) and runs `run_baseline` against a fake
  `w2w-baseline.sh` that prints what it received; `ENGINE_CORE=6` must be there, and bash must not
  have printed `readonly variable`;
- scans the driver's text for any name assigned after the line that made it readonly — the class,
  not the one instance. Continuation lines are joined first. The only form it lets by is an
  argument of `env`: the text before `NAME=` on that command must be `env` followed only by other
  `NAME=value` words. A seven-line probe proves three things. The 2026-09-27 shape is seen (line
  2). `ENGINE_CORE=$ENGINE_CORE timeout 5 env true` is seen too, although `env` appears later on
  the command (line 4). The `env` form is let by (line 5).

Reversal, 2026-09-27: `run_baseline` put back to the prefix form went red on three assertions:
`want 'ENGINE_CORE=6' … got: …boot-p4.sh: line 197: ENGINE_CORE: readonly variable
ENGINE_CORE=<unset> …`, `bash printed 'readonly variable'`, and `boot-p4.sh assigns a readonly name
as a prefix or variable: 197: ENGINE_CORE`. Restored: `pass 28 fail 0`.

The first scanner exempted any line that merely **contained** `env`, so the line-4 probe passed
(senior review of PR #124). Reversal of the tightened rule, putting back the old exemption:
`FAIL  the scanner read the probe as '2: ENGINE_CORE ', not '2: ENGINE_CORE 4: ENGINE_CORE '`.
Restored: `pass 30 fail 0`.

## A trap met while guarding it: the desk's shellcheck is not CI's

`[measured 2026-09-27]` CI's lint job runs `shellcheck -S info` on `boot-p4.sh` and this guard,
with the shellcheck the runner image ships. The job's *Set up job* reads `Image: ubuntu-24.04`,
`Version: 20260920.314.1`, and that image's readme lists `shellcheck 0.9.0-1`. The desk has
0.11.0. On `[ -x "$turn" ] && [ -x "$density" ] || die "…"`, 0.9.0 reports SC2015 (info) and
0.11.0 reports nothing, so the desk was green and all three commits of PR #124 were red. The
0.9.0 binary from the upstream release (`koalaman/shellcheck` v0.9.0) reproduces CI's finding
exactly. **Before pushing a shell script CI lints, run it through the runner's shellcheck
version, not the desk's.** Guard: the lint job itself. `ci.yml` names no version, so the
runner image decides.

## The general rule

A stderr line that says a value was refused is not a failure unless something makes it one. When
a script passes settings to a child, check what the child **received**, by making the child say
it, not what the parent meant to send. A parent's `settings.txt` records the parent's intent.
