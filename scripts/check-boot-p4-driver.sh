#!/usr/bin/env bash
# Two things the phase-4 §9 boot (2026-09-27) found scripts/boot-p4.sh doing
# wrong, checked here with no §9 machine, no Mac, no cargo and no root:
#
# 1. ENGINE_CORE never reached w2w-baseline.sh. The driver passed it as a
#    command-prefix assignment while it was readonly; bash prints
#    "ENGINE_CORE: readonly variable" and runs the command WITHOUT it. Every arm
#    of the boot fell back to w2w-baseline.sh's default (6, the right core, by
#    luck). docs/reference/a-readonly-name-as-a-command-prefix-is-dropped-and-the-command-still-runs.md
#    Checked two ways: run_baseline, the one call the driver makes, is run
#    against a fake w2w-baseline.sh that prints what it received; and the
#    driver's text is searched for any readonly name assigned after the line
#    that made it readonly — the class, not the one instance.
#
# 2. A timer whose `next` was empty was left running and fired mid-boot
#    (anacron.timer, attempt 2, exit 3 before p1 B). The driver now refuses
#    while ANY timer unit is active, system or user.
#    docs/reference/a-timer-with-no-next-elapse-can-still-fire.md
#    Checked through active_timers_verdict (pure) and active_timers_check with a
#    fake `systemctl` first on PATH, so both managers' wiring is exercised.
#
# WHAT THIS CANNOT SEE: the refusal's top-level loop in boot-p4.sh (it calls
# active_timers_check for `system` and `user` and refuses on any FAIL); that
# run_arm's other arguments are the arm's (it checks the settings the driver
# fixes, not the per-arm ones); and whether systemd's `active` state is the
# right question on a systemd other than the desk's (257 era) — the JSON field
# names are the ones `systemctl list-units --output=json` printed on the desk.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
pass=0
fail=0
ok() { pass=$((pass + 1)); printf 'ok    %s\n' "$1"; }
bad() { fail=$((fail + 1)); printf 'FAIL  %s\n' "$1"; }

scratch=$(mktemp -d) || exit 2
trap 'rm -rf "$scratch"' EXIT

# ------------------------------------------------ 1a. what run_baseline hands over
fake_baseline=$scratch/w2w-baseline.sh
cat >"$fake_baseline" <<'EOF'
#!/usr/bin/env bash
for v in ENGINE_CORE RUNS MESSAGES PIN WARMUP GAP CLIENT_CORE ARMS WIRE_NIC OBSERVER_CORE W2W_EXTRA OUT_DIR; do
  printf '%s=%s\n' "$v" "${!v-<unset>}"
done
EOF
chmod +x "$fake_baseline"

echo "=== run_baseline passes every fixed setting to w2w-baseline.sh"
# A caller's stray `export GAP=0` must not be what the fake sees. ENGINE_CORE is
# deliberately NOT exported here: exported before the driver's `readonly`, it would
# keep the export attribute and reach the child even through the broken prefix,
# which is exactly how the desk's shell did not have it.
got=$(
  export GAP=0
  # shellcheck source=boot-p4.sh disable=SC1091
  BOOT_P4_SOURCE_ONLY=1 . "$here/boot-p4.sh" || exit 3
  run_baseline "$fake_baseline" hft:admin enp9s0 7 "--transport uring" /out 2>&1
)
rc=$?
if [ "$rc" = 0 ]; then ok "run_baseline exit 0"; else bad "run_baseline exit $rc — output: $got"; fi
want() { # want <line> <what it means>
  if grep -qxF -- "$1" <<<"$got"; then ok "$1 — $2"; else bad "want '$1' — $2; got: $(tr '\n' ' ' <<<"$got")"; fi
}
want "ENGINE_CORE=6" "the driver's readonly ENGINE_CORE reaches the script (the 2026-09-27 driver passed nothing: <unset>)"
want "RUNS=10" "RUNS"
want "MESSAGES=20000" "MESSAGES"
want "PIN=1" "PIN from W2W_PIN"
want "WARMUP=2000" "WARMUP from W2W_WARMUP"
want "GAP=8" "GAP from W2W_GAP, not the caller's stray GAP=0"
want "CLIENT_CORE=7" "CLIENT_CORE from W2W_CLIENT_CORE"
want "ARMS=hft:admin" "the arm's ARMS"
want "WIRE_NIC=enp9s0" "the arm's WIRE_NIC"
want "OBSERVER_CORE=7" "the arm's OBSERVER_CORE"
want "W2W_EXTRA=--transport uring" "the arm's W2W_EXTRA, one word with its space"
want "OUT_DIR=/out" "the arm's OUT_DIR"
if grep -q 'readonly variable' <<<"$got"; then bad "bash printed 'readonly variable'"; else ok "no 'readonly variable' from bash"; fi

# ------------------------------------------------ 1b. the class, over the whole driver
echo "=== no readonly name in boot-p4.sh is assigned after it is made readonly"
# Every name a `readonly` line declares, then any later `NAME=` in command
# position or as a prefix (start of line, or after whitespace/`;`/`(`), outside
# comments. `env NAME=value` is an argument and is written `NAME="…"` after
# `env`; it is allowed only on a line that also says `env` — and the check below
# proves that allowance cannot hide the bug by testing a prefix without env.
readonly_scan() { # readonly_scan <file> — prints "line: NAME" for each violation
  awk '
    { code = $0; sub(/^[[:space:]]*#.*/, "", code) }
    code ~ /^[[:space:]]*readonly[[:space:]]/ {
      n = split(code, w, /[[:space:]]+/)
      for (i = 1; i <= n; i++) { nm = w[i]; sub(/=.*/, "", nm); if (nm ~ /^[A-Z_][A-Z0-9_]*$/) ro[nm] = NR }
      next
    }
    {
      for (nm in ro) {
        if (NR <= ro[nm]) continue
        if (code ~ ("(^|[[:space:];(])" nm "=") && code !~ /(^|[[:space:]])env([[:space:]]|$)/ && prev !~ /(^|[[:space:]])env[[:space:]]*\\$/) print NR ": " nm
      }
      prev = code
    }' "$1"
}
v=$(readonly_scan "$here/boot-p4.sh")
if [ -z "$v" ]; then
  ok "boot-p4.sh: none"
else
  bad "boot-p4.sh assigns a readonly name as a prefix or variable: $(tr '\n' ' ' <<<"$v")"
fi
# The scanner itself must see the shape that cost the boot.
# shellcheck disable=SC2016 # the probe is text: $ENGINE_CORE must stay unexpanded
printf 'readonly ENGINE_CORE=6\n  RUNS=1 ENGINE_CORE=$ENGINE_CORE \\\n    timeout 5 w2w-baseline.sh\n' >"$scratch/probe.sh"
v=$(readonly_scan "$scratch/probe.sh")
if [ "$v" = "2: ENGINE_CORE" ]; then
  ok "the scanner sees a prefix ENGINE_CORE= after readonly (probe line 2)"
else
  bad "the scanner missed the 2026-09-27 shape: got '$v'"
fi

# ------------------------------------------------ 2a. active_timers_verdict, pure
echo "=== active_timers_verdict"
# shellcheck source=boot-p4.sh disable=SC1091
BOOT_P4_SOURCE_ONLY=1 . "$here/boot-p4.sh" || { echo "cannot source boot-p4.sh"; exit 2; }
verdict() { # verdict <want> <manager> <json> <what> [<value substring>]
  local line v
  line=$(active_timers_verdict "$2" "$3")
  v=${line%%$'\t'*}
  if [ "$v" = "$1" ] && { [ -z "${5:-}" ] || [[ "$line" == *"$5"* ]]; }; then
    ok "$1 — $4"
  else
    bad "want $1${5:+ with \"$5\"} — $4; got: $line"
  fi
}
# The unit that stopped attempt 2: active, its triggered service running, no `next`.
anacron='[{"unit":"anacron.timer","load":"loaded","active":"active","sub":"running","description":"Trigger anacron every hour"}]'
verdict FAIL system "$anacron" "anacron.timer active with no next (sub running) — the 2026-09-27 case" "anacron.timer"
verdict FAIL system "$anacron" "the fix names the command" "sudo -n systemctl stop anacron.timer"
waiting='[{"unit":"man-db.timer","load":"loaded","active":"active","sub":"waiting"},{"unit":"fstrim.timer","load":"loaded","active":"inactive","sub":"dead"}]'
verdict FAIL system "$waiting" "an active waiting timer, beside an inactive one" "man-db.timer active"
user='[{"unit":"aura-glass-update-check.timer","load":"loaded","active":"active","sub":"waiting"},{"unit":"snap.firmware-updater.firmware-notifier.timer","load":"loaded","active":"active","sub":"waiting"}]'
verdict FAIL user "$user" "two user timers (attempt 1's), both named" "aura-glass-update-check.timer, snap.firmware-updater.firmware-notifier.timer"
verdict FAIL user "$user" "a user timer is stopped with systemctl --user" "systemctl --user stop aura-glass-update-check.timer snap.firmware-updater.firmware-notifier.timer"
stopped='[{"unit":"anacron.timer","load":"loaded","active":"inactive","sub":"dead"},{"unit":"apt-daily.timer","load":"loaded","active":"inactive","sub":"dead"}]'
verdict PASS system "$stopped" "every timer stopped — attempt 3, which ran to its end"
verdict PASS system '[{"unit":"x.timer","load":"loaded","active":"failed","sub":"failed"}]' "a failed timer cannot fire"
verdict PASS user '[]' "no user timers"
verdict UNKNOWN system 'null' "systemctl unreadable is not a pass"
verdict UNKNOWN system 'not json' "garbage is not a pass"

# ------------------------------------------------ 2b. both managers, through systemctl
echo "=== active_timers_check reads each manager"
mkdir -p "$scratch/bin"
cat >"$scratch/bin/systemctl" <<EOF
#!/usr/bin/env bash
case " \$* " in
  *" --user "*) printf '%s' '$user' ;;
  *) printf '%s' '$anacron' ;;
esac
EOF
chmod +x "$scratch/bin/systemctl"
line=$(PATH="$scratch/bin:$PATH" active_timers_check system)
if [[ "$line" == FAIL*"system: anacron.timer active"* ]]; then ok "system manager read: anacron.timer"; else bad "system manager: $line"; fi
line=$(PATH="$scratch/bin:$PATH" active_timers_check user)
if [[ "$line" == FAIL*"user: aura-glass-update-check.timer"* ]]; then ok "user manager read: --user"; else bad "user manager: $line"; fi

echo
echo "pass $pass   fail $fail"
[ "$fail" -eq 0 ]
