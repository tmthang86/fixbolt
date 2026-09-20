#!/usr/bin/env bash
# CLAUDE.md §10 — a guard is proven by reversal. `crates/dict/build.rs` dies
# when a `<message>` in the FIX XML carries no `msgcat` attribute
# (`None => die(...)` at build.rs:801-807), and that path has never once
# been observed to fire: nothing in this repository ever hands the
# generator a dictionary missing the attribute, so the guard existed as
# code, not as a reproduced red. This script is that reversal, run every
# time in CI rather than once by hand.
# docs/plans/2026-09-20-the-desk-free-residue.md, row 214 of *Chia việc*,
# `Not proven (b)`.
#
# It also proves the two die arms sitting in the same `match` (the
# unknown-category arm at build.rs:792-800, and the admin_types-empty arm
# at build.rs:1206-1211) fire with the message they claim, and that the
# generator does NOT die on an untouched copy of the same file — a script
# that only ever produces red would pass just as well with `die()` wired to
# fire unconditionally.
#
# Four arms, all built from vendor/quickfix/spec/FIX44.xml, none of it
# edited in place (vendor/ is gitignored and fetched, never modified —
# ADR-0001):
#   0  the untouched copy — must BUILD.
#   1  Heartbeat's `msgcat='admin'` removed entirely — must refuse, the
#      message naming "has no msgcat attribute" (build.rs:801-807).
#   2  Heartbeat's `msgcat='admin'` changed to `msgcat='other'` — must
#      refuse, the message naming `has msgcat="other"` (build.rs:797-800).
#   3  every `msgcat='admin'` in the file changed to `msgcat='app'`,
#      leaving no administrative message at all — must refuse, the message
#      naming "not one <message> carries msgcat='admin'" (build.rs:1206).
#
# The fixture directory is target/check-msgcat/, never /tmp: /tmp is tmpfs
# on the desk (project memory) and scripts/check-scratch-fixtures.sh watches
# scratch directories reached by `cd`/`pushd`/`--manifest-path`/`-C`. This
# script never enters target/check-msgcat/ that way — every damaged copy is
# only ever named through NANOFIX_FIX44_XML — so it never trips that gate's
# "entered a scratch dir" rule either.
#
# What this script does NOT see:
#   - the FIXT/FIX50SP2 build path (NANOFIX_FIXT11_XML / NANOFIX_FIX50SP2_XML)
#     runs through the same `match` on the same three arms for FIXT11.xml's
#     own <message> loop; this script does not run that branch separately —
#     one generator, one match, judged once here (plan row 214)
#   - a `msgcat` value that is present but empty (`msgcat=''`) — the XML
#     parser reports that as `Some("")`, landing in the "has msgcat=..."
#     arm, not the "no msgcat attribute" arm; not exercised here
#   - any die arm not reachable from FIX44.xml's own <message> list
#   - a sed pattern that silently stops matching a future FIX44.xml — guarded
#     below by requiring each arm to actually differ from arm 0
#
# Runs standalone: scripts/check-dict-refuses-a-message-without-msgcat.sh

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SRC="$ROOT/vendor/quickfix/spec/FIX44.xml"
if [[ ! -f "$SRC" ]]; then
  echo "check-dict-refuses-a-message-without-msgcat: FAIL — $SRC not found; run scripts/fetch-quickfix-assets.sh" >&2
  exit 1
fi

WORK="$ROOT/target/check-msgcat"
rm -rf "$WORK"
mkdir -p "$WORK"

HEARTBEAT_OPEN="<message name='Heartbeat' msgtype='0' msgcat='admin'>"

# Arm 0: untouched copy.
cp "$SRC" "$WORK/arm0.xml"

# Arm 1: Heartbeat's msgcat attribute removed entirely.
sed "s/${HEARTBEAT_OPEN}/<message name='Heartbeat' msgtype='0'>/" "$SRC" > "$WORK/arm1.xml"

# Arm 2: Heartbeat's msgcat changed to a category the generator does not know.
sed "s/${HEARTBEAT_OPEN}/<message name='Heartbeat' msgtype='0' msgcat='other'>/" "$SRC" > "$WORK/arm2.xml"

# Arm 3: every admin message recategorised as app — no admin message left.
sed "s/msgcat='admin'/msgcat='app'/g" "$SRC" > "$WORK/arm3.xml"

# Each arm must actually differ from arm 0 before it is ever handed to
# cargo — a sed pattern that stopped matching would otherwise report a pass
# by testing arm 0 four times over.
for n in 1 2 3; do
  if diff -q "$WORK/arm0.xml" "$WORK/arm${n}.xml" > /dev/null; then
    echo "check-dict-refuses-a-message-without-msgcat: FAIL — arm${n}'s sed changed nothing; its target text was not found verbatim in ${SRC}" >&2
    exit 1
  fi
done
if grep -q "msgcat='admin'" "$WORK/arm3.xml"; then
  echo "check-dict-refuses-a-message-without-msgcat: FAIL — arm3 still carries msgcat='admin' somewhere; it must carry none" >&2
  exit 1
fi

# NANOFIX_FIX44_XML, when it names a relative path, is read relative to
# crates/dict (build.rs:37-38, DEFAULT is "../../vendor/..."). Absolute
# paths sidestep that entirely, so $WORK (already absolute) is used as-is.
run_arm() {
  local file="$1" log="$2"
  NANOFIX_FIX44_XML="$file" cargo build -p fixbolt-dict > "$log" 2>&1
}

status=0

if run_arm "$WORK/arm0.xml" "$WORK/arm0.log"; then
  echo "check-dict-refuses-a-message-without-msgcat: ok — arm 0 (untouched) builds"
else
  echo "check-dict-refuses-a-message-without-msgcat: FAIL — arm 0 (untouched copy) did not build; the harness itself is broken, not the guard under test" >&2
  cat "$WORK/arm0.log" >&2
  status=1
fi

# $1 name, $2 file, $3 short reason (the sentence a reversal must be missing
# to fail on), $4 the die text this arm's stderr must carry.
check_red_arm() {
  local name="$1" file="$2" reason="$3" want="$4"
  if run_arm "$file" "$WORK/${name}.log"; then
    echo "check-dict-refuses-a-message-without-msgcat: FAIL — expected the build to refuse: ${reason}, but ${name} built clean" >&2
    status=1
    return
  fi
  if grep -qF "$want" "$WORK/${name}.log"; then
    echo "check-dict-refuses-a-message-without-msgcat: ok — ${name} refused, carrying: ${want}"
  else
    echo "check-dict-refuses-a-message-without-msgcat: FAIL — expected the build to refuse: ${reason}" >&2
    echo "--- ${name} stderr ---" >&2
    cat "$WORK/${name}.log" >&2
    status=1
  fi
}

check_red_arm arm1 "$WORK/arm1.xml" "no msgcat" "has no msgcat attribute"
check_red_arm arm2 "$WORK/arm2.xml" "unknown msgcat 'other'" 'has msgcat="other"'
check_red_arm arm3 "$WORK/arm3.xml" "no admin message left" "not one <message> carries msgcat='admin'"

if [[ "$status" -ne 0 ]]; then
  exit 1
fi

echo "check-dict-refuses-a-message-without-msgcat: ok — 4 arms, all as expected"
