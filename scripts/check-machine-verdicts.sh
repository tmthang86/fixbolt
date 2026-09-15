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

# --- the NIC rows' pure parts --------------------------------------------------
#
# `[2026-09-14]` senior review of PR #72. The NIC IRQ affinity row's overlap check
# had a recorded trap (docs/reference/comm-compares-by-locale-collation-not-number.md)
# and no test, and its coalescing row read `rx-usecs` while ignoring
# `Adaptive RX: on`. Both need a NIC to reach from the real script; neither needs
# one here.

same() { # same <want> <got> <what it means>
  if [[ "$2" == "$1" ]]; then
    pass=$((pass + 1)); printf 'ok    %s\n' "$3"
  else
    fail=$((fail + 1)); printf 'FAIL  want [%s] got [%s]  %s\n' "${1//$'\n'/ }" "${2//$'\n'/ }" "$3"
  fi
}

echo
echo "=== expand_cpulist, irq_overlap"

same $'6\n7\n14\n15' "$(expand_cpulist 6-7,14-15)" "the isolated list of the §9 desk expands in order"
same $'6\n7\n14\n15' "$(irq_overlap 6-7,14-15 0-15)" "affinity 0-15 overlaps every isolated CPU, two-digit ones included"
same "" "$(irq_overlap 6-7,14-15 0-5,8-13)" "affinity steered around the isolated CPUs: no overlap"
same "10" "$(irq_overlap 6,10 9-10)" "a two-digit CPU after a one-digit one — where comm said 'not in sorted order'"
same "" "$(irq_overlap 1 10-11)" "isolated cpu1, affinity cpu10-11: no overlap — a match without -x reads '1' inside '10'"
same "" "$(irq_overlap 10-11 1)" "isolated cpu10-11, affinity cpu1: no overlap, the other direction"
same "" "$(irq_overlap 6-7 "")" "an empty affinity list overlaps nothing"

echo
echo "=== nic_irqs"

fake=$(mktemp -d)
mkdir -p "$fake/msi"
printf '%s\n' \
  '           CPU0       CPU1' \
  '  85:          0          0  IR-PCI-MSIX-0000:09:00.0    0-edge      enp9s0' \
  '  86:        100          0  IR-PCI-MSIX-0000:09:00.0    1-edge      enp9s0-rx-0' \
  '  87:        100          0  IR-PCI-MSIX-0000:09:00.0    2-edge      enp9s0-rx-1' \
  '  90:        100          0  IR-PCI-MSIX-0000:0a:00.0    0-edge      enp9s01' \
  ' LOC:     123456     123456   Local timer interrupts' > "$fake/interrupts"
same $'names\n85\n86\n87' "$(nic_irqs enp9s0 "$fake/msi" "$fake/interrupts")" \
  "no msi_irqs entries: by name, bare and queue names, not a longer interface that shares the prefix"
touch "$fake/msi/85" "$fake/msi/86" "$fake/msi/87" "$fake/msi/88" "$fake/msi/89" "$fake/msi/100"
same $'msi_irqs\n85\n86\n87\n88\n89\n100' "$(nic_irqs enp9s0 "$fake/msi" "$fake/interrupts")" \
  "msi_irqs lists vectors: preferred, numerically sorted, including ones no name matched"
same "names" "$(nic_irqs enp9s0 "$fake/absent" "$fake/absent-interrupts")" \
  "neither source readable: the source line and no IRQ, which the row reads as UNKNOWN"
rm -rf "$fake"

echo
echo "=== coalesce_verdict"

ethtool_c() { # ethtool_c <Adaptive RX value> <rx-usecs value>
  printf 'Coalesce parameters for enp9s0:\nAdaptive RX: %s  TX: %s\nstats-block-usecs:\tn/a\n\nrx-usecs:\t%s\nrx-frames:\tn/a\n' "$1" "$1" "$2"
}
same $'PASS\trx-usecs 0' "$(coalesce_verdict "$(ethtool_c n/a 0)")" "igb shape, rx-usecs 0"
same $'FAIL\trx-usecs 3' "$(coalesce_verdict "$(ethtool_c n/a 3)")" "igb shape as the §9 desk reads today, rx-usecs 3"
same $'PASS\trx-usecs 0' "$(coalesce_verdict "$(ethtool_c off 0)")" "adaptive off, rx-usecs 0"
same $'FAIL\tAdaptive RX on, rx-usecs 0' "$(coalesce_verdict "$(ethtool_c on 0)")" \
  "adaptive on FAILS even at rx-usecs 0 — the driver rewrites the rate from traffic"
same $'UNKNOWN\trx-usecs not reported by this driver' "$(coalesce_verdict "$(ethtool_c n/a n/a)")" "rx-usecs n/a"
same $'UNKNOWN\trx-usecs not reported by this driver' "$(coalesce_verdict "")" "no output at all"

echo
echo "=== eee_verdict"

# `[2026-09-15]` Q15, docs/plans/2026-09-04-the-second-linux-desk.md Điều 3: desk
# boot B6 measured EEE on costing `tools/w2w` wire p50 at interval 1 s +14.6 µs
# (54 310 ns vs 39 714 ns with EEE off, same hour, enp9s0/igb) — far over the 5%
# threshold, so `check-machine.sh` must FAIL a NIC with EEE on.

ethtool_eee() { # ethtool_eee <EEE status value> <Tx LPI value>
  printf 'EEE settings for enp9s0:\n\tEEE status: %s\n\tTx LPI: %s\n' "$1" "$2"
}

# Real: `ethtool --show-eee enp9s0` on this desk today, EEE off (the B6 baseline).
same $'PASS\tEEE status: disabled' "$(eee_verdict "$(ethtool_eee disabled disabled)")" \
  "real desk output, EEE off — the B6 baseline every procedure header requires"
# Real: `ethtool --show-eee enp9s0` on this desk today, EEE forced on for the A/B arm.
same $'FAIL\tEEE status: enabled - active' "$(eee_verdict "$(ethtool_eee "enabled - active" "0 (us)")")" \
  "real desk output, EEE forced on — the +14.6 µs state"
# Invented: `inactive` only means the link partner is not advertising EEE today,
# not that this NIC will stay quiet once it does — must still FAIL.
same $'FAIL\tEEE status: enabled - inactive' "$(eee_verdict "$(ethtool_eee "enabled - inactive" disabled)")" \
  "invented: enabled but the far end isn't advertising today — still FAIL, not a pass by omission"
# Invented: a NIC/driver ethtool cannot ask about EEE at all.
same $'UNKNOWN\tno EEE status reported' "$(eee_verdict "netlink error: Operation not supported")" \
  "invented: unsupported NIC — UNKNOWN, never a pass"
same $'UNKNOWN\tno EEE status reported' "$(eee_verdict "")" \
  "no output at all (e.g. ethtool missing) — UNKNOWN"

echo
echo "=== pick_nic"

# fake_nic <net-root> <name> <type> <carrier> [marker ...]
#
# One fake interface directory under <net-root>. `carrier` empty means no
# carrier file at all (some virtual interfaces have none). `marker` is
# `device`, `wireless` or `phy80211` — pick_nic only checks that these
# exist, never their content.
fake_nic() {
  local root="$1" name="$2" type="$3" carrier="$4" marker
  mkdir -p "$root/$name"
  echo "$type" > "$root/$name/type"
  if [ -n "$carrier" ]; then
    echo "$carrier" > "$root/$name/carrier"
  fi
  shift 4
  for marker in "$@"; do
    mkdir -p "$root/$name/$marker"
  done
}

net=$(mktemp -d)
fake_nic "$net" enp9s0 1 0 device
same "enp9s0" "$(pick_nic "$net" "")" \
  "a wired NIC without carrier is still picked — the §9 rows do not need a cable"
rm -rf "$net"

net=$(mktemp -d)
fake_nic "$net" wlp7s0 1 1 device wireless
same "" "$(pick_nic "$net" "")" \
  "wireless is never the measurement NIC"
rm -rf "$net"

net=$(mktemp -d)
fake_nic "$net" tailscale0 65534 ""
fake_nic "$net" docker0 1 ""
fake_nic "$net" lo 772 ""
same "" "$(pick_nic "$net" "")" \
  "virtual interfaces have no bus device"
rm -rf "$net"

net=$(mktemp -d)
fake_nic "$net" enp8s0 1 0 device
fake_nic "$net" enp9s0 1 1 device
same "enp9s0" "$(pick_nic "$net" "")" \
  "carrier breaks a tie, and only a tie"
rm -rf "$net"

net=$(mktemp -d)
same "enp8s0" "$(pick_nic "$net" enp8s0)" \
  "FIXBOLT_NIC wins without a carrier, as before"
rm -rf "$net"

echo
echo "=== summary"
echo "pass $pass   fail $fail"
[[ "$fail" -eq 0 ]]
