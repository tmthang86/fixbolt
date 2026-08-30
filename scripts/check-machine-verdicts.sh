#!/usr/bin/env bash
# Does check-machine.sh reach the right verdict about the machine it is running on?
#
# The rows in check-machine.sh mostly read a file and compare it to a constant. The
# virtualisation row does not: it decides whether the OTHER rows can mean anything,
# because `governor`, `turbo`, `C-states`, `SMT` and NIC IRQ affinity are host
# properties that a guest cannot set at all. A guest does not fail those rows
# loudly — the files are simply absent, so it collects `unknown` and reads as
# under-configured rather than as structurally unable.
#
# This repository has no VM to check that against, and `[measured 2026-08-30]` the
# last time a machine-probing script went untested it printed `config: CONFIG_TLS=m`
# and "it was built without CONFIG_TLS" in the same run and blocked an open item for
# a day. So the verdict is a pure function of (virt, steal) and is tested here, on
# any machine, with no VM and no root.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=check-machine.sh
MACHINE_SOURCE_ONLY=1 . "$here/check-machine.sh"

pass=0
fail=0

expect() { # expect <want> <virt> <steal> <what it means>
  local want="$1" virt="$2" steal="$3" what="$4" got
  got="$(virt_verdict "$virt" "$steal" | head -1)"
  if [[ "$got" == "$want" ]]; then
    pass=$((pass + 1)); printf 'ok    %-15s %s\n' "$want" "$what"
  else
    fail=$((fail + 1)); printf 'FAIL  want %-15s got %-15s %s\n' "$want" "$got" "$what"
  fi
}

echo "=== virt_verdict"

expect BARE_METAL     none   0  "bare metal, no steal — the only state §9 can hold in"

# Every hypervisor string systemd-detect-virt can return is a FAIL, and the row must
# not care which one: the reason is that the host owns the knobs, not that KVM is
# special. A case list that named hypervisors would silently pass the next one.
expect GUEST          kvm    0  "kvm, no steal — still a guest, still cannot set host knobs"
expect GUEST          vmware 0  "vmware"
expect GUEST          xen    3  "xen with steal"
expect GUEST          microsoft 0 "hyper-v"
expect GUEST          lxc    0  "container — shares the host kernel's CPU state"
expect GUEST          docker 0  "container"
expect GUEST          amazon 12 "a name this script has never seen, with heavy steal"

# Steal on bare metal is not a §9 failure with a known fix — it is a contradiction,
# and saying so beats picking whichever of the two readings looks tidier.
expect STEAL_ON_METAL none   7  "bare metal reporting steal — unexplained, must say so"

# `unknown` must never read as a pass: a machine that cannot answer the question is
# exactly the machine whose numbers should not be published.
expect UNKNOWN        unknown 0 "systemd-detect-virt unavailable"
expect UNKNOWN        ""      0 "empty output"

echo
echo "=== summary"
echo "pass $pass   fail $fail"
[[ "$fail" -eq 0 ]]
