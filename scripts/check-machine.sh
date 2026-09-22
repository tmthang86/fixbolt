#!/usr/bin/env bash
# Read the DESIGN.md §9 checklist off the running machine and say, row by row,
# whether it is actually in force.
#
# CLAUDE.md §2 non-negotiable 10: no performance number without the benchmark,
# the machine, AND the §9 settings in force. Until this script existed, §9 was a
# table of things somebody was supposed to have done — there was no way to tell a
# tuned box from an untuned one except by asking the person who set it up.
#
# It READS ONLY. Applying these is root, machine-specific, and belongs to the
# person sitting at the box; each FAIL prints the command that fixes it.
#
# Exit 1 if any row FAILS, so `scripts/bench.sh --strict` can refuse to publish a
# number from a machine that is not set up. `unknown` is NOT a pass: a container
# that cannot read /sys must not look like a tuned host.
#
# [2026-09-14] step A5 of docs/plans/2026-09-04-the-second-linux-desk.md: the NIC
# IRQ affinity row, plus two new rows (coalescing, irqbalance) and an extra field
# on the busy_poll row, are judged for real once a NIC is selected — FIXBOLT_NIC,
# or auto-selected by `pick_nic` below (a physical, non-wireless bus device
# under /sys/class/net; carrier only breaks a tie among several — step S3,
# same day, replaced the earlier carrier-only rule, whose scope shrank
# silently when the link bounced:
# docs/reference/a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md).
# No NIC selected: every row below is byte-identical to the script before
# this step, including the NIC IRQ affinity row's `? ? ?`.
#
# The overlap check (NIC IRQ affinity vs /sys/devices/system/cpu/isolated) first
# used `comm -12` on two `sort -n`-ed lists and got "not in sorted order" on both,
# despite the numeric order being correct: `comm` compares by the current
# locale's collation, not numerically, and under a normal locale "10" sorts
# before "6". Replaced with `grep -Fxf`, which needs no particular sort order.
# See docs/reference/comm-compares-by-locale-collation-not-number.md.
set -uo pipefail

pass=0
fail=0
unknown=0

row() { # row PASS|FAIL|UNKNOWN name value fixcmd
  case "$1" in
    PASS) pass=$((pass + 1)); mark="PASS " ;;
    FAIL) fail=$((fail + 1)); mark="FAIL " ;;
    *) unknown=$((unknown + 1)); mark="? ? ?" ;;
  esac
  printf "%s  %-22s %s\n" "$mark" "$2" "$3"
  if [ "$1" != PASS ] && [ -n "${4:-}" ]; then
    printf "       %-22s fix: %s\n" "" "$4"
  fi
}

# Read a file, or print nothing if it is not there / not readable.
r() { cat "$1" 2>/dev/null; }

# Expand a cpulist like "6-7,14-15" (the format of smp_affinity_list and of
# /sys/devices/system/cpu/isolated) into one CPU number per line.
expand_cpulist() {
  echo "$1" | tr ',' '\n' | while IFS= read -r part; do
    case "$part" in
      *-*) seq "${part%%-*}" "${part##*-}" ;;
      "") ;;
      *) echo "$part" ;;
    esac
  done
}

# irq_overlap <isolated cpulist> <smp_affinity_list cpulist>
#
# Every CPU in both lists, one per line; nothing when they do not overlap.
# `grep -Fxf`, not `comm`: comm demands its inputs sorted by the current locale's
# collation, which disagrees with `sort -n` on two-digit CPU numbers ("10" sorts
# before "6" lexically), and comm says so loudly even though the numeric order is
# exactly right — docs/reference/comm-compares-by-locale-collation-not-number.md.
# A function so that scripts/check-machine-verdicts.sh can test it with two-digit
# CPUs on any machine, NIC or not.
irq_overlap() {
  grep -Fxf <(expand_cpulist "$1") <(expand_cpulist "$2")
}

# nic_irqs <nic> <msi_irqs dir> <interrupts file>
#
# The NIC's IRQ numbers, one per line. `/sys/class/net/<nic>/device/msi_irqs/` is
# preferred when it lists anything: it is the kernel's own list of the vectors
# allocated to that PCI function, so a queue whose /proc/interrupts name does not
# start with the interface name (a driver that names them after the PCI address,
# say) is still counted. Otherwise, every /proc/interrupts line whose last field is
# the bare name or `<nic>-<queue>`. The first line printed is `msi_irqs` or `names`,
# saying which — the row names its source, because a PASS over a name match that
# missed a vector is a PASS about fewer IRQs than the NIC has.
nic_irqs() {
  local nic="$1" msi="$2" interrupts="$3" from_msi
  from_msi=$(find "$msi" -mindepth 1 -maxdepth 1 -name '[0-9]*' -printf '%f\n' 2>/dev/null | sort -n)
  if [ -n "$from_msi" ]; then
    echo msi_irqs
    echo "$from_msi"
    return 0
  fi
  echo names
  awk -v nic="$nic" '{
    n = split($0, f, " ")
    name = f[n]
    irq = f[1]
    sub(/:$/, "", irq)
    if (irq ~ /^[0-9]+$/ && (name == nic || substr(name, 1, length(nic) + 1) == nic "-")) print irq
  }' "$interrupts" 2>/dev/null
}

# coalesce_verdict <ethtool -c output>
#
# One line, `PASS|FAIL|UNKNOWN<TAB><value>`. `Adaptive RX: on` FAILS whatever
# rx-usecs reads: with adaptive moderation on, the driver rewrites the interrupt
# rate from traffic, so `rx-usecs 0` is not what is in force — ethtool(8), `-C`
# `adaptive-rx`. `n/a` (igb has no adaptive mode) and `off` fall through to
# rx-usecs, exactly as the row read before the adaptive check existed.
coalesce_verdict() {
  local out="$1" adaptive rx_usecs
  adaptive=$(echo "$out" | awk '/^Adaptive RX:/{print $3; exit}')
  rx_usecs=$(echo "$out" | awk -F: '/^rx-usecs:/{v=$2; gsub(/[ \t]/,"",v); print v; exit}')
  if [ "$adaptive" = on ]; then
    printf 'FAIL\tAdaptive RX on, rx-usecs %s\n' "${rx_usecs:-unreported}"
  elif [ -z "$rx_usecs" ] || [ "$rx_usecs" = "n/a" ]; then
    printf 'UNKNOWN\trx-usecs not reported by this driver\n'
  elif [ "$rx_usecs" = 0 ]; then
    printf 'PASS\trx-usecs 0\n'
  else
    printf 'FAIL\trx-usecs %s\n' "$rx_usecs"
  fi
}

# eee_verdict <ethtool --show-eee output>
#
# One line, `PASS|FAIL|UNKNOWN<TAB><value>`. Reads the `EEE status:` line only —
# `Tx LPI` and the advertised-modes lines are not judged. [measured 2026-09-15]
# desk boot B6, docs/plans/2026-09-04-the-second-linux-desk.md Điều 3 (Đề nghị,
# Q15): with EEE on, `enp9s0` (Intel I211, igb) cabled to the Mac read
# `tools/w2w` wire p50 at interval 1 s of 54 310 ns against 39 714 ns with EEE
# off in the same hour — +14.6 µs, close to the 16.5 µs 1000BASE-T LPI wake
# time, far over the 5% publishing threshold. `enabled - inactive` still
# FAILs: it means only that the link partner is not advertising EEE *today*,
# not that this NIC will stay quiet once the far end starts — the desk's own
# setting is the only half §9 can hold constant. No `EEE status:` line at all
# (ethtool -1, or a driver/NIC that does not support EEE) is UNKNOWN, same as
# coalesce_verdict above: never a pass on text this function cannot read.
eee_verdict() {
  local out="$1" status
  status=$(echo "$out" | awk -F: '/EEE status:/{v=$2; gsub(/^[ \t]+|[ \t]+$/,"",v); print v; exit}')
  case "$status" in
    disabled) printf 'PASS\tEEE status: disabled\n' ;;
    "enabled - active" | "enabled - inactive") printf 'FAIL\tEEE status: %s\n' "$status" ;;
    *) printf 'UNKNOWN\tno EEE status reported\n' ;;
  esac
}

# virt_verdict <systemd-detect-virt output> <steal % over the window>
#
# Why this row exists: a guest CANNOT satisfy §9, and it does not fail loudly — it
# fails by the files simply not being there. `governor`, `turbo`, `C-states`, `SMT`
# and NIC IRQ affinity are all HOST properties; a guest that sets its own THP and
# busy_poll can collect `unknown` on the rest and look merely under-configured
# rather than structurally unable. `unknown` is already not a pass, so the script
# does not lie — but it does not say the one thing the reader needs, which is that
# no amount of configuration inside this machine will fix those rows.
#
# Split out so scripts/check-machine-verdicts.sh can exercise it: this repository
# has no VM to run against, and a row that cannot be tested where it matters is how
# scripts/check-ktls-available.sh shipped a wrong answer for a day.
virt_verdict() {
  local virt="$1" steal="$2"
  case "$virt" in
    none)
      if [ "${steal:-0}" -gt 0 ] 2>/dev/null; then
        echo "STEAL_ON_METAL"
        echo "  Bare metal reporting ${steal}% steal. Nothing here explains that;"
        echo "  find out why before publishing a number from this machine."
        return 1
      fi
      echo "BARE_METAL"
      echo "  Bare metal, no steal. §9's host-level rows are settable here."
      return 0
      ;;
    ""|unknown)
      echo "UNKNOWN"
      echo "  Cannot tell whether this is a guest. Treat every §9 row below as"
      echo "  unconfirmed — CLAUDE.md §2 non-negotiable 10."
      return 1
      ;;
    *)
      echo "GUEST"
      echo "  Running under '${virt}'. governor, turbo, C-states, SMT and NIC IRQ"
      echo "  affinity are HOST properties: no configuration inside this machine"
      echo "  can set them, so §9 cannot be satisfied here and no latency figure"
      echo "  from it is publishable. Steal over the window: ${steal:-unknown}%."
      echo "  Use bare metal, or publish nothing but counts and same-machine A/B."
      return 1
      ;;
  esac
}

# pick_nic <net-root> <explicit>
#
# The measurement NIC, one name or empty. `explicit` (FIXBOLT_NIC) wins
# outright — no carrier required, because a cable-less FAIL/UNKNOWN run is
# still useful (A5). Otherwise walk <net-root>/*/ in name order and keep
# every interface that is a physical bus device: `type` reads 1 (a loopback
# reads 772, a Tailscale-style tunnel reads 65534), it has a `device`
# symlink (a virtual interface — `docker0`, `veth*`, `br-*`, `lo`,
# `tailscale*` — has none), and it is not wireless (`wireless/`, the old
# sysfs marker, or `phy80211`, the cfg80211/mac80211 one — wireless has no
# stable IRQ or coalescing story and is never what §9's NIC rows mean).
# Among the survivors, the first with carrier = 1 wins; with none carrying,
# the first survivor; with no survivor, empty.
#
# [2026-09-14] step S3 of docs/plans/2026-09-04-the-second-linux-desk.md:
# selecting by carrier ALONE (the rule this replaces) meant a ~4 s link
# bounce (`ethtool --set-eee`/`-A` reinitialising `igb`) left no candidate,
# so the NIC-dependent rows below silently disappeared while the run still
# printed "§9 satisfied". Carrier now only breaks a tie among physical
# devices that do not depend on the cable being up this instant. See
# docs/reference/a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md.
pick_nic() {
  local net_root="$1" explicit="$2" dev cand first=""
  if [ -n "$explicit" ]; then
    echo "$explicit"
    return 0
  fi
  for dev in "$net_root"/*/; do
    if [ ! -d "$dev" ]; then
      continue
    fi
    cand=$(basename "$dev")
    if [ "$(r "${dev}type")" != 1 ]; then
      continue
    fi
    if [ ! -e "${dev}device" ]; then
      continue
    fi
    if [ -e "${dev}wireless" ] || [ -e "${dev}phy80211" ]; then
      continue
    fi
    if [ "$(r "${dev}carrier")" = 1 ]; then
      echo "$cand"
      return 0
    fi
    if [ -z "$first" ]; then
      first="$cand"
    fi
  done
  echo "$first"
}

# fmt_dur <non-negative whole seconds> -> "<H>h<MM>m" or "<M>m<SS>s"
#
# A plain, machine-independent duration formatter for timers_verdict below —
# no `date`, so nothing here depends on the reader's locale or timezone.
fmt_dur() {
  local s="$1"
  if [ "$s" -ge 3600 ]; then
    printf '%dh%02dm' $((s / 3600)) $(((s % 3600) / 60))
  else
    printf '%dm%02ds' $((s / 60)) $((s % 60))
  fi
}

# utc_stamp <epoch seconds> -> "YYYY-MM-DDTHH:MMZ", UTC, PURE.
#
# `date` is NOT used, and that is the whole point of this function. This was
# `date -u -d "@$N" … || echo "epoch $N"`, and `-d` is a GNU extension: BSD
# and macOS `date` spell it `-r N` and reject `-d`, so on the Mac mini (the
# machine DESIGN.md §2's table already owes an ADR-0091 reversal on, and the
# one CI can never speak for — CI is Ubuntu) the fallback fired, the value
# read `epoch 1700003600`, and two of check-machine-verdicts.sh's four
# timers_verdict assertions went red. A second output shape no fixture
# pinned, on the machine no gate watches.
#
# The conversion is Howard Hinnant's `civil_from_days` (howardhinnant.
# github.io/date_algorithms.html, the algorithm behind C++20's
# <chrono>/`std::chrono::year_month_day`): shift the epoch to 0000-03-01 so
# leap day is the last day of the year, then read the 400-year era, the year
# of the era, and the day of the year off it — no lookup table, no leap-year
# special case, correct for every proleptic-Gregorian date including the
# non-leap centuries. Every division is integer (`int()`), which is what the
# algorithm's own derivation assumes; awk's doubles hold these magnitudes
# exactly (z stays under ~10^6).
#
# `[measured 2026-09-22]` checked against GNU `date -u -d @N` on 3015
# values — 0, ±1 s, the epoch's own day boundaries, 2000-02-29,
# 2020-03-01, 2024-02-29, 2038-01-19T03:14Z, 2100-01-01 (a non-leap
# century), two negative epochs, and 3000 random points — 0 mismatches.
# Pinned by check-machine-verdicts.sh's `=== utc_stamp` section.
utc_stamp() {
  awk -v n="$1" 'BEGIN {
    days = int(n / 86400); sod = n - days * 86400
    if (sod < 0) { days -= 1; sod += 86400 }
    z = days + 719468
    era = int((z >= 0 ? z : z - 146096) / 146097)
    doe = z - era * 146097
    yoe = int((doe - int(doe / 1460) + int(doe / 36524) - int(doe / 146096)) / 365)
    y = yoe + era * 400
    doy = doe - (365 * yoe + int(yoe / 4) - int(yoe / 100))
    mp = int((5 * doy + 2) / 153)
    d = doy - int((153 * mp + 2) / 5) + 1
    m = (mp < 10) ? mp + 3 : mp - 9
    if (m <= 2) y += 1
    printf "%04d-%02d-%02dT%02d:%02dZ", y, m, d, int(sod / 3600), int((sod % 3600) / 60)
  }'
}

# timer_window_sec <value> -> whole seconds, or nothing and exit 1.
#
# The `FIXBOLT_TIMER_WINDOW` knob is documented "in hours" and nowhere says
# WHOLE hours — and timers_verdict's own fixture exercises a 0.5 h window —
# so a fraction is accepted here and converted with awk, not with `$(( ))`.
# `[measured 2026-09-22]` under this script's `set -u`,
# `window_sec=$((TIMER_WINDOW_H * 3600))` killed the whole run mid-report,
# before the kTLS rows and before the pass/fail/unknown summary, naming
# neither the knob nor the row:
#   FIXBOLT_TIMER_WINDOW=0.5  -> 0.5: syntax error: invalid arithmetic
#                                operator (error token is ".5")
#   FIXBOLT_TIMER_WINDOW=12h  -> 12h: value too great for base
#   FIXBOLT_TIMER_WINDOW=abc  -> abc: unbound variable
#   FIXBOLT_TIMER_WINDOW=" "  -> no error at all: a silent 0 s window
#   FIXBOLT_TIMER_WINDOW=-1   -> no error at all: a silent -3600 s window
# (An EMPTY value never did: `${FIXBOLT_TIMER_WINDOW:-12}` treats it as
# unset and reads 12 — measured, against the expectation that it died too.)
# The last two are the ones a validator has to catch as well: a window that
# is silently wrong is worse than one that is refused, because the row still
# prints PASS.
#
# Accepted: a non-negative decimal — `12`, `0.5`, `.5`, `12.`, `0`.
# Refused: everything else, including whitespace, a sign, a suffix and
# scientific notation. The caller turns a refusal into an UNKNOWN row.
timer_window_sec() {
  case "$1" in
    '' | '.' | *[!0-9.]* | *.*.*) return 1 ;;
  esac
  awk -v h="$1" 'BEGIN { printf "%d", h * 3600 }'
}

# timers_verdict <now_usec> <window_sec> <json>
#
# `json` is `systemctl list-timers --all --output=json` verbatim: an array of
# objects with `unit` and `next` (µs since the epoch, or `null` for a timer
# `--all` lists but that is not scheduled — systemd's own
# src/shared/format-table.c, ADR-0093 decision 3 "What the search found").
# `now_usec` and `window_sec` are ARGUMENTS, never read from `date` in here —
# the caller (the real row below) reads the clock exactly once; this function
# stays pure so scripts/check-machine-verdicts.sh can test it with a FIXED
# `now` and never see a flake from the second it happened to run in. It calls
# no `date` at all, in any branch: the UTC stamp in its FAIL value comes from
# utc_stamp() above, which computes the civil date itself — see that
# function's comment for what `date -u -d @N` cost on a BSD `date`.
#
# Prints one line, three tab-separated fields: `VERDICT<TAB>VALUE<TAB>FIXCMD`
# (FIXCMD empty unless VERDICT is FAIL) — the same shape row() already takes
# as its own name/value/fixcmd arguments, so the row below is a straight
# `read` of this function's output.
#
#   FAIL    any timer's `next` is <= now + window, INCLUDING a `next` already
#           in the past (its service may still be running — over-reading is
#           the safe direction, same as decision 2's R1). VALUE names every
#           such unit, its next firing in UTC (not the reader's local
#           timezone: a formatter that depended on it could not be pinned by
#           a fixed-`now` test run on two machines in two timezones) and how
#           far off it is; FIXCMD is `sudo -n systemctl stop <unit>` per
#           unit, `; `-joined — stop, not disable, so the next boot restores
#           it (docs/reference/a-quiet-machine-check-cannot-see-a-timer-that-
#           has-not-fired.md).
#   PASS    no timer's `next` falls inside the window. A `next` of `null`
#           (an inactive timer `--all` still lists) is always ignored.
#   UNKNOWN malformed JSON — not this function's problem to diagnose further;
#           the caller already ruled out "no systemctl / no jq / can't reach
#           PID 1" before calling this at all.
#
# The window is printed in VALUE on every verdict, PASS included: a reader
# who sees `no timer due` still needs to know it was only asked about the
# next `window_sec`, per the reference page's rule.
timers_verdict() {
  local now_usec="$1" window_sec="$2" json="$3"
  local window_h due unit next_us left_sec when next_utc value fixcmd

  window_h=$(awk -v s="$window_sec" 'BEGIN { printf "%.6g", s / 3600 }')

  if ! printf '%s' "$json" | jq -e 'type == "array"' >/dev/null 2>&1; then
    printf 'UNKNOWN\tmalformed systemctl list-timers JSON [window %sh]\t\n' "$window_h"
    return 0
  fi

  due=$(printf '%s' "$json" | jq -r --argjson now "$now_usec" --argjson win "$((window_sec * 1000000))" '
    .[] | select(.next != null) | select(.next <= ($now + $win)) |
    [.unit, (.next | tostring)] | @tsv
  ' 2>/dev/null)

  if [ -z "$due" ]; then
    printf 'PASS\tno timer due inside the window [window %sh]\t\n' "$window_h"
    return 0
  fi

  value=""
  fixcmd=""
  while IFS=$'\t' read -r unit next_us; do
    [ -z "$unit" ] && continue
    left_sec=$(((next_us - now_usec) / 1000000))
    if [ "$left_sec" -lt 0 ]; then
      when="overdue $(fmt_dur $((-left_sec)))"
    else
      when="in $(fmt_dur "$left_sec")"
    fi
    next_utc=$(utc_stamp "$((next_us / 1000000))")
    value="${value}${value:+, }${unit} next ${next_utc} (${when})"
    fixcmd="${fixcmd}${fixcmd:+; }sudo -n systemctl stop ${unit}"
  done <<<"$due"

  printf 'FAIL\t%s [window %sh]\t%s\n' "$value" "$window_h" "$fixcmd"
}

# Sourced by the verdict test, which wants the functions and none of the probing.
if [ "${MACHINE_SOURCE_ONLY:-0}" = 1 ]; then
  return 0 2>/dev/null || exit 0
fi

echo "=== machine"
uname -srm
if [ -r /proc/cpuinfo ]; then
  echo "cpu       $(grep -m1 '^model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')"
  echo "cores     $(nproc 2>/dev/null || echo unknown)"
else
  echo "cpu       $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown)"
  echo "cores     $(getconf _NPROCESSORS_ONLN 2>/dev/null || echo unknown)"
fi

# --- NIC selection --------------------------------------------------------
#
# `FIXBOLT_NIC=<name>` names the NIC explicitly (carrier not required — a
# cable-less FAIL/UNKNOWN run is still useful). Otherwise `pick_nic` (above)
# walks /sys/class/net in name order and keeps the first physical NIC — a
# bus device, not virtual, not wireless — breaking a tie between several by
# carrier. No candidate: NIC stays empty and every row below behaves exactly
# as it did before this NIC awareness existed.
#
# Printed here, in the machine header, not just used silently by the rows
# further down: [2026-09-14] a run whose auto-pick lost every candidate
# during a link bounce dropped three rows and still printed "§9 satisfied"
# without saying its own scope had shrunk — see
# docs/reference/a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md.
NIC=$(pick_nic /sys/class/net "${FIXBOLT_NIC:-}")
if [ -n "$NIC" ]; then
  nic_carrier=$(r "/sys/class/net/$NIC/carrier")
  echo "nic       $NIC (carrier ${nic_carrier:-unknown})"
else
  echo "nic       none — no wired NIC with a bus device under /sys/class/net; FIXBOLT_NIC=<name> names one"
fi

echo "rustc     $(rustc --version 2>/dev/null || echo 'not on PATH')"
echo

if [ "$(uname -s)" != Linux ]; then
  echo "=== DESIGN.md §9"
  row UNKNOWN "everything" "not Linux — §9 is a Linux checklist" \
    "run this on the Linux box; a number from here is not a §9 number"
  echo
  echo "=== summary"
  echo "pass $pass   fail $fail   unknown $unknown"
  exit 1
fi

CMDLINE=$(r /proc/cmdline)

echo "=== DESIGN.md §9"

# --- isolcpus + rcu_nocbs -----------------------------------------------------
#
# This row used to demand `nohz_full` as well, and ADR-0021 reversed that.
# [measured 2026-08-31] `nohz_full` is the whole of the 36% the isolated core was
# costing: 670.7 ns per `Engine::turn` against 494.8 on an `isolcpus`-only core,
# because full dynticks runs context tracking on every kernel entry and this
# engine is nothing but kernel entries. `isolcpus` and `rcu_nocbs` are free.
#
# So this gate now FAILS a machine for HAVING nohz_full, where it used to fail
# one for lacking it. That reversal is the point and not an accident: the
# baselines in benches/baselines.tsv were recorded without it, and a machine
# carrying it reads 35% over on all four `turn` cases.
iso=$(echo "$CMDLINE" | tr ' ' '\n' | grep '^isolcpus=' || true)
nocb=$(echo "$CMDLINE" | tr ' ' '\n' | grep '^rcu_nocbs=' || true)
nohz=$(echo "$CMDLINE" | tr ' ' '\n' | grep '^nohz_full=' || true)
if [ -z "$CMDLINE" ]; then
  row UNKNOWN "isolcpus + rcu_nocbs" "/proc/cmdline not readable" \
    "run outside a restricted container"
elif [ -n "$iso" ] && [ -n "$nocb" ]; then
  row PASS "isolcpus + rcu_nocbs" "$iso $nocb"
else
  row FAIL "isolcpus + rcu_nocbs" "${iso:-no isolcpus}${nocb:+ $nocb}" \
    "add 'isolcpus=N rcu_nocbs=N' to the kernel command line, then reboot"
fi

# --- nohz_full, which §9 no longer asks for -----------------------------------
if [ -z "$CMDLINE" ]; then
  : # already reported unknown above; one unreadable file is one row
elif [ -z "$nohz" ]; then
  row PASS "no nohz_full" "absent — ADR-0021"
else
  row FAIL "no nohz_full" "$nohz" \
    "REMOVE nohz_full from the kernel command line: it adds 160 ns to every kernel entry (+36% on Engine::turn) and is behind at p50, p99 AND p99.9 — see ADR-0021"
fi

# --- CPU speculation mitigations ----------------------------------------------
#
# ADR-0023. [measured 2026-09-01] disabling these makes every syscall this
# engine performs 59-63% cheaper: `engine turn, 1 idle sessions` goes from
# 448.9 ns to 175.2, while thirteen pure user-space benchmarks move -4.1% to
# +4.1% with no direction. All of it is `retbleed`s untrained return thunk plus
# `spec_rstack_overflow`s Safe RET; `vmscape` — the mechanism STATUS.md had
# named for two days — costs nothing.
#
# This row PASSES when the machine IS mitigated, which is the default, the safe
# state, and the state `benches/baselines.tsv` was recorded in. It is NOT advice
# to turn them off. A machine with them off reads ~60% UNDER every syscall-bound
# baseline, which passes — a baseline is a ceiling — so the bench gate cannot
# catch it and something else must.
#
# Read from /sys rather than /proc/cmdline: the command line says what was asked
# for and sysfs says what the kernel is doing, and [measured 2026-09-01] they
# differ — `retbleed=off` also removed `STIBP: always-on` from spectre_v2s line,
# which no reading of the command line would show.
vuln_dir=/sys/devices/system/cpu/vulnerabilities
if [ ! -d "$vuln_dir" ]; then
  row UNKNOWN "CPU mitigations" "$vuln_dir not readable" \
    "run outside a restricted container"
else
  off=$(grep -l '^Vulnerable' "$vuln_dir"/* 2>/dev/null | xargs -r -n1 basename | tr '\n' ' ')
  if [ -z "$off" ]; then
    row PASS "CPU mitigations" "all in force"
  else
    row FAIL "CPU mitigations" "disabled: ${off% }" \
      "these are worth 61% of every syscall here (ADR-0023), so numbers from this machine are NOT comparable to benches/baselines.tsv — remove the mitigation overrides from the kernel command line and reboot"
  fi
fi

# --- CPU frequency governor ---------------------------------------------------
gov=$(r /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
if [ -z "$gov" ]; then
  row UNKNOWN "governor" "cpufreq not exposed" "run on bare metal, not in this container"
elif [ "$gov" = performance ]; then
  row PASS "governor" "$gov"
else
  row FAIL "governor" "$gov" \
    "sudo cpupower frequency-set -g performance   (or write 'performance' to scaling_governor)"
fi

# --- turbo --------------------------------------------------------------------
nt=$(r /sys/devices/system/cpu/intel_pstate/no_turbo)
boost=$(r /sys/devices/system/cpu/cpufreq/boost)
if [ "$nt" = 1 ] || [ "$boost" = 0 ]; then
  row PASS "turbo" "off"
elif [ -z "$nt" ] && [ -z "$boost" ]; then
  row UNKNOWN "turbo" "neither intel_pstate/no_turbo nor cpufreq/boost is readable" \
    "on AMD: echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost"
else
  row FAIL "turbo" "on" \
    "echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo   (Intel) or boost=0 (AMD)"
fi

# --- C-states -----------------------------------------------------------------
# The kernel command line is what actually holds this; per-state disable files
# are the fallback for a box that cannot be rebooted.
if echo "$CMDLINE" | grep -qE 'intel_idle\.max_cstate=0|processor\.max_cstate=[01]|idle=poll'; then
  row PASS "C-states" "capped on the kernel command line"
elif [ -z "$CMDLINE" ]; then
  row UNKNOWN "C-states" "/proc/cmdline not readable" "run outside a restricted container"
else
  row FAIL "C-states" "not capped" \
    "add 'intel_idle.max_cstate=0 processor.max_cstate=1 idle=poll' to the kernel command line"
fi

# --- SMT ----------------------------------------------------------------------
smt=$(r /sys/devices/system/cpu/smt/control)
case "$smt" in
  off | forceoff | notsupported) row PASS "SMT / hyperthreading" "$smt" ;;
  "") row UNKNOWN "SMT / hyperthreading" "not exposed" "check in BIOS" ;;
  *) row FAIL "SMT / hyperthreading" "$smt" "echo off | sudo tee /sys/devices/system/cpu/smt/control" ;;
esac

# --- transparent huge pages ---------------------------------------------------
thp=$(r /sys/kernel/mm/transparent_hugepage/enabled)
if [ -z "$thp" ]; then
  row UNKNOWN "transparent hugepages" "not exposed" "run on bare metal"
elif echo "$thp" | grep -q '\[never\]'; then
  row PASS "transparent hugepages" "never"
else
  row FAIL "transparent hugepages" "$thp" \
    "echo never | sudo tee /sys/kernel/mm/transparent_hugepage/enabled"
fi

# --- busy poll ----------------------------------------------------------------
# Without a NIC selected this row is exactly as it always was: only
# `net.core.busy_poll`. With a NIC selected it also reads `net.core.busy_read`
# — the receive-side half of the same A/B (see DESIGN.md §9, SO_BUSY_POLL row)
# — and applies the same ">0" threshold to both, naming both values.
bp=$(sysctl -n net.core.busy_poll 2>/dev/null)
if [ -z "$NIC" ]; then
  if [ -z "$bp" ]; then
    row UNKNOWN "net.core.busy_poll" "sysctl unavailable" "run on the host"
  elif [ "$bp" -gt 0 ] 2>/dev/null; then
    row PASS "net.core.busy_poll" "$bp"
  else
    row FAIL "net.core.busy_poll" "$bp" "sudo sysctl -w net.core.busy_poll=50 net.core.busy_read=50"
  fi
else
  br=$(sysctl -n net.core.busy_read 2>/dev/null)
  if [ -z "$bp" ] || [ -z "$br" ]; then
    row UNKNOWN "net.core.busy_poll" "sysctl unavailable" "run on the host"
  elif [ "$bp" -gt 0 ] 2>/dev/null && [ "$br" -gt 0 ] 2>/dev/null; then
    row PASS "net.core.busy_poll" "busy_poll=$bp busy_read=$br"
  else
    row FAIL "net.core.busy_poll" "busy_poll=$bp busy_read=$br" \
      "sudo sysctl -w net.core.busy_poll=50 net.core.busy_read=50"
  fi
fi

# --- the machine is quiet -----------------------------------------------------
# `[measured 2026-08-30]` This row exists because the rows above were measuring the
# wrong thing. On this project's own desktop the five tuning rows move the ring
# benchmark's median by 0.8%; competing CPU load moves it by 71% — 262 ns to 449 ns
# — and NOTHING in §9 looked at load. The box scored `pass 6` while running an LLM,
# an editor and two Electron apps. A tuned machine that is busy is not a machine you
# can take a latency number from, and until this row existed the script could not say
# so. Guarded by reversal: with eight spinners running it must FAIL.
#
# CPU time over a real window, not `loadavg` — loadavg is a one-minute average and
# says nothing about the second the benchmark ran in.
#
# Both the total AND the per-process attribution are deltas over that window.
# `[measured 2026-08-30]` `ps -eo pcpu` was used here first and named the wrong
# processes: **`%CPU` from `ps` is an average over the process's whole lifetime**,
# not what it is doing now. It reported an LLM at 19% on a machine /proc/stat
# measured as 1% busy, and that number reached the owner as "the machine is loaded"
# before the two were put side by side. An instrument that answers a question
# adjacent to the one asked is the failure this repository keeps finding.
QUIET_WINDOW=${QUIET_WINDOW:-1}
busy_pct=""
steal_pct=""
# One awk pass over every /proc/<pid>/stat. Done in shell it took seconds, which
# made the sampling window longer than QUIET_WINDOW and reported a single-threaded
# process at 310% of a core — a reading that is impossible on its face, and the
# only reason it was caught.
snap() {
  head -1 /proc/stat
  awk 'FNR==1 {
         a = index($0, "(")
         b = 0; for (i = length($0); i > 0; i--) if (substr($0, i, 1) == ")") { b = i; break }
         if (a == 0 || b == 0) next
         pid  = substr($0, 1, a - 2)
         comm = substr($0, a + 1, b - a - 1)
         n = split(substr($0, b + 2), f, " ")      # f[1] is state, so utime=f[12]
         if (n >= 13) print "p", pid, f[12] + f[13], comm
       }' /proc/[0-9]*/stat 2>/dev/null
}
if [ -r /proc/stat ]; then
  A=$(snap); sleep "$QUIET_WINDOW"; B=$(snap)
  # shellcheck disable=SC2046
  set -- $(echo "$A" | head -1); shift
  a_idle=$(($4 + $5)); a_steal=${8:-0}; a_tot=0; for v in "$@"; do a_tot=$((a_tot + v)); done
  # shellcheck disable=SC2046
  set -- $(echo "$B" | head -1); shift
  b_idle=$(($4 + $5)); b_steal=${8:-0}; b_tot=0; for v in "$@"; do b_tot=$((b_tot + v)); done
  d_tot=$((b_tot - a_tot)); d_idle=$((b_idle - a_idle)); d_steal=$((b_steal - a_steal))
  [ "$d_tot" -gt 0 ] && busy_pct=$(((d_tot - d_idle) * 100 / d_tot))
  [ "$d_tot" -gt 0 ] && steal_pct=$((d_steal * 100 / d_tot))
fi

# --- virtualisation -----------------------------------------------------------
# Reported before the quiet row because it governs it: on a guest, "quiet" can only
# ever mean "quiet inside this VM", and the neighbours are invisible except as steal.
# `systemd-detect-virt` EXITS 1 WHEN THE ANSWER IS "none" — the good case is
# reported as a failure. `[measured 2026-08-30]` written first as
# `$(systemd-detect-virt || echo unknown)`, which on bare metal ran both halves and
# set VIRT to the two lines "none\nunknown", so this machine reported itself a
# guest. The verdict test could not catch it: it feeds virt_verdict directly and
# never sees how the argument is obtained. Running the real script did.
VIRT=$(systemd-detect-virt 2>/dev/null)
[ -z "$VIRT" ] && VIRT=unknown
vv=$(virt_verdict "$VIRT" "${steal_pct:-0}")
case "$(echo "$vv" | head -1)" in
  BARE_METAL) row PASS "not virtualised" "bare metal, ${steal_pct:-0}% steal" ;;
  GUEST) row FAIL "not virtualised" "guest under '${VIRT}', ${steal_pct:-0}% steal" \
    "measure on bare metal; governor, turbo, C-states, SMT and IRQ affinity are host properties" ;;
  STEAL_ON_METAL) row FAIL "not virtualised" "bare metal but ${steal_pct}% steal — unexplained" \
    "find out what is stealing time before publishing a number from this machine" ;;
  *) row UNKNOWN "not virtualised" "cannot tell (systemd-detect-virt unavailable)" \
    "confirm by hand; every row below is unconfirmed until you do" ;;
esac
# The reasoning only earns its space when the answer is not PASS — every other row
# here prints a fix line and nothing else when it passes.
[ "$(echo "$vv" | head -1)" = BARE_METAL ] || echo "$vv" | tail -n +2 | sed 's/^/     /'

if [ -z "$busy_pct" ]; then
  row UNKNOWN "machine is quiet" "cannot read /proc/stat" \
    "a latency number needs a quiet machine; find another way to confirm it"
elif [ "$busy_pct" -le 3 ]; then
  row PASS "machine is quiet" "${busy_pct}% CPU busy over ${QUIET_WINDOW}s"
else
  top=$(
    { echo "$A" | awk '$1=="p"{print "a", $2, $3, $4}'
      echo "$B" | awk '$1=="p"{print "b", $2, $3, $4}'; } |
    awk '$1=="a"{was[$2]=$3; nm[$2]=$4}
         $1=="b" && ($2 in was){d=$3-was[$2]; if(d>0) print d, nm[$2]}' |
    sort -rn | head -3 |
    awk -v tot="$d_tot" -v ncpu="$(nproc 2>/dev/null || echo 1)" \
      'tot > 0 {printf "%s %d%% of a core  ", $2, $1 * 100 * ncpu / tot}'
  )
  row FAIL "machine is quiet" "${busy_pct}% CPU busy over ${QUIET_WINDOW}s — ${top:-unattributed}" \
    "close what is running; competing load moved this project's ring median 71%, against 0.8% for every tuning row combined"
fi

# --- no timer due ---------------------------------------------------------
# ADR-0093 decision 3. The row above reads CPU busy for ONE second, now, and
# says nothing about what systemd has already scheduled for four hours' time
# — boot D lost rounds 13-20 to apt-daily-upgrade.timer at 06:51 while the
# quiet row read green before AND after
# (docs/reference/a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired.md).
# `systemctl list-timers --all --output=json` gives every timer's `next` in
# µs since the epoch (or `null` for one `--all` lists that is not
# scheduled); `timers_verdict` above does the pure comparison against `now`
# and a window. UNKNOWN, not FAIL, when the question genuinely cannot be
# asked here (no `systemctl`, no `jq`, or PID 1 unreachable — this
# container's case: `systemctl` is systemd 255 but PID 1 is not systemd).
#
# THE WINDOW IS VALIDATED BEFORE ANYTHING ELSE, and a bad value costs one
# UNKNOWN row rather than the rest of the report: `window_sec=$((
# TIMER_WINDOW_H * 3600 ))` under this script's own `set -u` killed the run
# HERE — before the kTLS rows, before the IRQ rows, before the
# pass/fail/unknown summary — on `FIXBOLT_TIMER_WINDOW=0.5`, naming neither
# the knob nor this row (`0.5: syntax error: invalid arithmetic operator`).
# timer_window_sec() above accepts the fraction the knob's own documentation
# always allowed (it says "in hours", never WHOLE hours, and
# timers_verdict's fixture exercises 0.5 h) and refuses what is not a
# number of hours at all — including ` ` and `-1`, which the arithmetic
# accepted SILENTLY as a 0 s and a -3600 s window with the row still
# printing PASS.
TIMER_WINDOW_H=${FIXBOLT_TIMER_WINDOW:-12}
if ! window_sec=$(timer_window_sec "$TIMER_WINDOW_H"); then
  row UNKNOWN "no timer due" \
    "FIXBOLT_TIMER_WINDOW='${TIMER_WINDOW_H}' is not a number of hours — no window was checked [window 12h is the default]" \
    "unset FIXBOLT_TIMER_WINDOW, or set it to a non-negative number of hours (12, or 0.5 for thirty minutes)"
elif ! command -v systemctl >/dev/null 2>&1; then
  row UNKNOWN "no timer due" "systemctl not on PATH [window ${TIMER_WINDOW_H}h]" \
    "install systemd, or set FIXBOLT_TIMER_WINDOW and check by hand"
elif ! command -v jq >/dev/null 2>&1; then
  row UNKNOWN "no timer due" "jq not on PATH [window ${TIMER_WINDOW_H}h]" \
    "install jq, or check 'systemctl list-timers --all' by hand"
else
  lt_out=$(systemctl list-timers --all --output=json 2>&1)
  lt_status=$?
  if [ "$lt_status" -ne 0 ] || ! printf '%s' "$lt_out" | jq -e . >/dev/null 2>&1; then
    lt_msg=$(printf '%s\n' "$lt_out" | head -1)
    row UNKNOWN "no timer due" "cannot reach PID 1 (${lt_msg:-systemctl exited $lt_status}) [window ${TIMER_WINDOW_H}h]" \
      "check 'systemctl list-timers --all' by hand on the real host"
  else
    now_usec=$(($(date +%s) * 1000000))
    tv=$(timers_verdict "$now_usec" "$window_sec" "$lt_out")
    IFS=$'\t' read -r tv_verdict tv_value tv_fix <<<"$tv"
    case "$tv_verdict" in
      PASS) row PASS "no timer due" "$tv_value" ;;
      FAIL) row FAIL "no timer due" "$tv_value" "$tv_fix" ;;
      *) row UNKNOWN "no timer due" "$tv_value" "check 'systemctl list-timers --all' by hand" ;;
    esac
  fi
fi

# --- kTLS ---------------------------------------------------------------------
# STATUS.md open item 10. This is a kernel feature, not a latency property, so it
# is reported separately from the tuning rows above.
# `[measured 2026-08-30]` this row used `lsmod | grep -q '^tls'`, and under this
# script's own `set -o pipefail` that branch can never be taken: grep -q exits on
# the first match, lsmod dies of SIGPIPE with status 141, and the pipeline reports
# failure exactly when the module IS loaded. The `|| modinfo` fallback hid it here
# — but on a kernel with CONFIG_TLS=y the module is built in, `modinfo` has
# nothing to find, and this row would have reported "no tls module" on the machine
# best equipped to run kTLS. /sys/module/tls covers loaded and built-in alike.
# Guarded by scripts/check-ktls-classify.sh, which holds the same rule.
if [ -d /sys/module/tls ]; then
  row PASS "kTLS (CONFIG_TLS)" "tls module loaded — open item 10 is unblocked here"
elif modinfo tls >/dev/null 2>&1; then
  row PASS "kTLS (CONFIG_TLS)" "tls module on disk, not loaded — 'sudo modprobe tls' away"
else
  row FAIL "kTLS (CONFIG_TLS)" "no tls module, loaded or on disk" \
    "sudo modprobe tls; if that fails the kernel lacks CONFIG_TLS — see scripts/check-ktls-available.sh"
fi

# --- IRQ affinity -------------------------------------------------------------
# Without a NIC selected this is reported, not judged: which core the NIC may
# interrupt is a decision about which core the engine runs on, and without a
# NIC this script does not know that. With a NIC selected (FIXBOLT_NIC, or
# auto-selected above), it can actually judge: every IRQ of that NIC —
# `nic_irqs` above, from /sys/class/net/<nic>/device/msi_irqs when it lists any,
# else the /proc/interrupts lines naming it (the bare name, e.g. "enp9s0", or
# one of its queues, e.g. "enp9s0-TxRx-0") — gets its
# /proc/irq/<n>/smp_affinity_list checked against
# /sys/devices/system/cpu/isolated.
#
# `[2026-09-14]` senior review of PR #72: an IRQ whose smp_affinity_list could
# not be read was skipped, so a NIC none of whose IRQs could be read PASSED
# having checked nothing. It now makes the row UNKNOWN, naming the IRQs; an
# overlap found on the IRQs that could be read still FAILS first.
if [ -z "$NIC" ]; then
  nic_irqs=$(grep -ciE 'eth|enp|ens|eno|mlx|sfc' /proc/interrupts 2>/dev/null)
  : "${nic_irqs:=0}"
  row UNKNOWN "NIC IRQ affinity" "$nic_irqs NIC interrupt line(s) — steer them AWAY from the engine core" \
    "see /proc/interrupts, then write a mask to /proc/irq/<n>/smp_affinity"
else
  nic_irq_out=$(nic_irqs "$NIC" "/sys/class/net/$NIC/device/msi_irqs" /proc/interrupts)
  irq_source=$(echo "$nic_irq_out" | head -1)
  nic_irq_nums=$(echo "$nic_irq_out" | tail -n +2)
  isolated=$(r /sys/devices/system/cpu/isolated)
  if [ -z "$nic_irq_nums" ]; then
    row UNKNOWN "NIC IRQ affinity" "$NIC: no IRQ in /sys/class/net/$NIC/device/msi_irqs or /proc/interrupts" \
      "check /proc/interrupts for this NIC's queue names"
  elif [ -z "$isolated" ]; then
    # An empty isolated list means "nothing to check this against" — not a
    # pass, since an un-isolated engine core makes the whole question moot,
    # and not a fail, since the NIC may still be steered correctly; UNKNOWN
    # says exactly that rather than guessing either way.
    row UNKNOWN "NIC IRQ affinity" "$NIC: /sys/devices/system/cpu/isolated is empty — nothing to check IRQs against" \
      "set isolcpus for the engine core first, then re-run"
  else
    bad=""
    unread=""
    for irq in $nic_irq_nums; do
      al=$(r "/proc/irq/$irq/smp_affinity_list")
      if [ -z "$al" ]; then
        unread="${unread}${unread:+,}$irq"
        continue
      fi
      if [ -n "$(irq_overlap "$isolated" "$al")" ]; then
        bad="${bad}irq $irq ($al) "
      fi
    done
    irq_csv=$(echo "$nic_irq_nums" | tr '\n' ',' | sed 's/,$//')
    if [ -n "$bad" ]; then
      row FAIL "NIC IRQ affinity" "$NIC: ${bad}overlaps isolated $isolated" \
        "echo <non-isolated-cpu> | sudo tee /proc/irq/<n>/smp_affinity_list"
    elif [ -n "$unread" ]; then
      row UNKNOWN "NIC IRQ affinity" "$NIC: smp_affinity_list unreadable for IRQ $unread (of $irq_csv, from $irq_source) — not checked" \
        "read /proc/irq/<n>/smp_affinity_list as a user that can, or run on the host"
    else
      row PASS "NIC IRQ affinity" "$NIC: IRQ $irq_csv (from $irq_source) — none on isolated $isolated"
    fi
  fi
fi

# --- NIC coalescing -------------------------------------------------------
# New row (A5): only meaningful once a NIC is selected, and §9's "IRQ
# affinity" row is pointless if the NIC is still batching interrupts.
if [ -n "$NIC" ]; then
  if ! command -v ethtool >/dev/null 2>&1; then
    row UNKNOWN "coalescing" "ethtool not on PATH" "install ethtool"
  else
    ec_out=$(ethtool -c "$NIC" 2>&1)
    ec_status=$?
    cv=$(coalesce_verdict "$ec_out")
    cv_value=${cv#*$'\t'}
    if [ "$ec_status" -ne 0 ]; then
      row UNKNOWN "coalescing" "$NIC: ethtool -c failed (exit $ec_status)" \
        "check the NIC/driver supports coalescing"
    else
      case "${cv%%$'\t'*}" in
        PASS) row PASS "coalescing" "$NIC: $cv_value" ;;
        UNKNOWN) row UNKNOWN "coalescing" "$NIC: $cv_value" "unsupported here — check by hand" ;;
        # Only the adaptive FAIL names `adaptive-rx off`: a driver with no
        # adaptive mode (igb reads `n/a`) refuses the whole `-C` over it.
        *) case "$cv_value" in
             Adaptive*) row FAIL "coalescing" "$NIC: $cv_value" "sudo ethtool -C $NIC adaptive-rx off rx-usecs 0" ;;
             *) row FAIL "coalescing" "$NIC: $cv_value" "sudo ethtool -C $NIC rx-usecs 0" ;;
           esac ;;
      esac
    fi
  fi
fi

# --- irqbalance -------------------------------------------------------------
# New row (A5): irqbalance actively moving IRQs around defeats a pinned NIC
# IRQ affinity the moment it runs.
if [ -n "$NIC" ]; then
  if ! command -v systemctl >/dev/null 2>&1; then
    row UNKNOWN "irqbalance inactive" "systemctl not available" "check by hand: ps -C irqbalance"
  else
    ib_status=$(systemctl is-active irqbalance 2>/dev/null)
    if [ "$ib_status" = active ]; then
      row FAIL "irqbalance inactive" "active" \
        "sudo systemctl stop irqbalance && sudo systemctl disable irqbalance"
    else
      row PASS "irqbalance inactive" "${ib_status:-inactive}"
    fi
  fi
fi

# --- EEE (802.3az) on the measurement NIC ---------------------------------
# New row (Q15, desk boot B6): only meaningful once a NIC is selected, same as
# coalescing and irqbalance above. See eee_verdict above for the measurement
# and the reasoning; DESIGN.md §9 names this row "EEE off on the measurement
# NIC".
if [ -n "$NIC" ]; then
  if ! command -v ethtool >/dev/null 2>&1; then
    row UNKNOWN "eee" "ethtool not on PATH" "install ethtool"
  else
    ee_out=$(ethtool --show-eee "$NIC" 2>&1)
    ee_status=$?
    ev=$(eee_verdict "$ee_out")
    ev_value=${ev#*$'\t'}
    if [ "$ee_status" -ne 0 ]; then
      row UNKNOWN "eee" "$NIC: ethtool --show-eee failed (exit $ee_status)" \
        "check the NIC/driver supports EEE"
    else
      case "${ev%%$'\t'*}" in
        PASS) row PASS "eee" "$NIC: $ev_value" ;;
        UNKNOWN) row UNKNOWN "eee" "$NIC: $ev_value" "unsupported here — check by hand" ;;
        *) row FAIL "eee" "$NIC: $ev_value" "sudo ethtool --set-eee $NIC eee off" ;;
      esac
    fi
  fi
fi

echo
echo "=== summary"
echo "pass $pass   fail $fail   unknown $unknown"
if [ "$fail" -gt 0 ] || [ "$unknown" -gt 1 ]; then
  echo
  echo "This machine is NOT set up to DESIGN.md §9. Numbers measured here are"
  echo "usable for counts and for A/B comparisons against themselves, and are NOT"
  echo "publishable as latency figures — CLAUDE.md §2 non-negotiable 10."
  exit 1
fi
echo "§9 satisfied. Latency numbers from this machine carry their settings."
