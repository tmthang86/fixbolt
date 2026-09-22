#!/usr/bin/env bash
# Does scripts/check-sudo-names-what-root-can-find.sh's pure verdict —
# sudo_verdict, and the join_logical_lines / sudo_rests it is built on —
# reach ADR-0093 decision 2's rules the way boot D's own trap, and the six
# advisory lines already living in check-machine.sh, actually read?
#
# sudo_verdict and friends are pure: one string in, one string out, no file,
# no cargo, no /proc — sourced here with SUDO_GATE_SOURCE_ONLY=1, the same
# shape check-machine-verdicts.sh uses on check-machine.sh
# (MACHINE_SOURCE_ONLY) and check-ab-rotation.sh uses on ab-rotation.sh
# (AB_ROTATION_SOURCE_ONLY).
#
# The reversal target lives in the first `same` call below: boot D's own
# line. Before R2 existed this file read `FAIL want [FAIL R2 cargo] got
# [ok]` — `perf` passes R1 (it is on ALLOW), and the workload after `--` is
# exactly what root could not find (see docs/reference/
# perf-record-exits-zero-when-sudo-cannot-find-the-workload.md). That red
# run is quoted in the plan's *Cách kiểm chứng*, step 3, and in this
# developer step's own handoff — it is not reproduced by editing this file,
# only by reading the plan's quoted output and the gate's own commit
# history.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=check-sudo-names-what-root-can-find.sh
# shellcheck disable=SC1091 # found at runtime via $here; not followed without -x
SUDO_GATE_SOURCE_ONLY=1 . "$here/check-sudo-names-what-root-can-find.sh"

pass=0
fail=0

same() { # same <want> <got> <what it means>
  if [[ "$2" == "$1" ]]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$3"
  else
    fail=$((fail + 1)); printf 'FAIL  want [%s] got [%s]  %s\n' "${1//$'\n'/ }" "${2//$'\n'/ }" "$3"
  fi
}

echo "=== sudo_verdict"

# Boot D's exact line — docs/plans/2026-09-20-boot-d.md:309, the line the
# driver copied. REVERSAL TARGET (R2): perf passes R1, the workload after
# -- is what root cannot find.
same "FAIL R2 cargo" \
  "$(sudo_verdict 'sudo -n perf record -e cycles -F 4999 -o d.data -- cargo bench -q -p fixbolt-engine --bench density')" \
  "REVERSAL TARGET: boot D's own line — perf passes R1, the workload after -- is what root cannot find"

# The six advisory lines check-machine.sh hands to row() as a fix command —
# ADR-0093 fact 2 / decision 2: a string is read by the same rule as an
# invocation, and every one of these is already ALLOW-clean (tee, systemctl).
same "ok" "$(sudo_verdict '"on AMD: echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost"')" \
  "advisory 1 (check-machine.sh:391) — tee is on ALLOW"
same "ok" "$(sudo_verdict '"echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo   (Intel) or boost=0 (AMD)"')" \
  "advisory 2 (check-machine.sh:394)"
same "ok" "$(sudo_verdict '"echo off | sudo tee /sys/devices/system/cpu/smt/control"')" \
  "advisory 3 (check-machine.sh:414)"
same "ok" "$(sudo_verdict '"echo never | sudo tee /sys/kernel/mm/transparent_hugepage/enabled"')" \
  "advisory 4 (check-machine.sh:425)"
same "ok" "$(sudo_verdict '"echo <non-isolated-cpu> | sudo tee /proc/irq/<n>/smp_affinity_list"')" \
  "advisory 5 (check-machine.sh:620)"
same "ok" "$(sudo_verdict '"sudo systemctl stop irqbalance && sudo systemctl disable irqbalance"')" \
  "advisory 6 (check-machine.sh:669) — TWO sudo occurrences on one line, both systemctl"

# R1: a name this script cannot say root's secure_path resolves.
same "FAIL R1" "$(sudo_verdict 'sudo mytool --flag')" \
  "sudo mytool --flag — 'mytool' is on no ALLOW list and carries no /"

# R2 again, without perf in the picture at all: the command word itself is
# the toolchain name, and sudo -E does not rescue it (ADR-0093, sudo applies
# secure_path whether or not env_reset is on).
same "FAIL R2 cargo" "$(sudo_verdict 'sudo -E cargo bench')" \
  "sudo -E cargo bench — -E does not exempt PATH from secure_path"

# Gap G1, named rather than silently passed: a command word held in a
# variable is unread by R1. This IS the intended fix shape (a
# manifest-pinned absolute path in the variable) — decision 1's driver rule
# carries the weight this gate cannot.
# shellcheck disable=SC2016 # the single quotes are the point: this is the
# LITERAL text '$BIN', unexpanded, the way it would read inside another
# script's double-quoted line — gap G1 is that the gate cannot see past the
# variable name to whatever it holds.
same "ok" "$(sudo_verdict 'sudo -n "$BIN"')" \
  "gap G1 — sudo -n \"\$BIN\" passes R1 unread, by design (ADR-0093 decision 2, Known gaps)"

# The fix boot D applied by hand, and the shape decision 1 asks every
# campaign driver for: an absolute path root can resolve without PATH.
same "ok" "$(sudo_verdict 'sudo -n perf record -e cycles -F 4999 -o d.data -- /abs/path/density-abc')" \
  "perf's workload given as an absolute path — R3 passes, no bare toolchain word remains for R2"

echo
echo "=== sudo_rests — the boundary class excludes '-', not just alnum/underscore"

# REVERSAL TARGET: this file's own '[^A-Za-z0-9_]' boundary (before the '-'
# was added) read "check-sudo" as an occurrence of the word 'sudo', because
# a hyphen satisfied the left boundary. This gate's own source is full of
# "check-sudo: ..." — so this was a real trap, not a hypothetical one, found
# on this gate's own first self-scan (see the script's SUDO_WORD comment).
same "" "$(sudo_rests 'echo "check-sudo: FAIL — some/path:1: ..."')" \
  "REVERSAL TARGET: 'check-sudo' is one hyphenated token, not a boundary before 'sudo'"
same "1" "$(sudo_rests 'sudo tee /sys/x' | wc -l | tr -d ' ')" \
  "a real, space-preceded sudo is still read as one occurrence"

echo
echo "=== join_logical_lines — a backslash-continued workload is still read"

fixtures="$(mktemp -d)"
trap 'rm -rf "$fixtures"' EXIT

# Plan trap table: "Dòng nối \\ giấu workload sang dòng sau" — the workload
# is on the SECOND physical line; only reading joined logical lines catches
# it. Read as two separate physical lines, line 1 has no bare toolchain word
# at all and would read "ok".
printf 'sudo -n perf record -e cycles -F 4999 -o d.data \\\n  -- cargo bench -q -p fixbolt-engine\n' \
  > "$fixtures/continued.sh"

joined="$(join_logical_lines "$fixtures/continued.sh" | head -1 | cut -f3-)"
same "FAIL R2 cargo" "$(sudo_verdict "$joined")" \
  "REVERSAL TARGET: a workload hidden on the backslash-continued second physical line is still read once the two lines are joined"

same "1" "$(join_logical_lines "$fixtures/continued.sh" | wc -l | tr -d ' ')" \
  "two physical lines join into exactly one logical line"

# A commented-out line is never live, continuation or not — is_live() skips
# it the same way check-scratch-fixtures.sh's does.
printf '# sudo -n perf record -- cargo bench\nsudo tee /sys/x\n' > "$fixtures/commented.sh"
same "2	1	sudo tee /sys/x" "$(join_logical_lines "$fixtures/commented.sh" | sed -n '2p')" \
  "the comment line is still emitted (so line numbers stay right) but marked not-live"

echo
echo "=== summary"
echo "pass $pass   fail $fail"
[[ "$fail" -eq 0 ]]
