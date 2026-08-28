#!/usr/bin/env bash
#
# Proves the workspace clippy configuration denies what CLAUDE.md §2
# non-negotiable 7 says it denies: no panic!, unwrap() or expect() in a library
# crate. It proves it by reversal, per CLAUDE.md §7 — a throwaway crate carrying
# the workspace's own [lints.*] blocks is shown RED with the three constructs
# present, then GREEN with them gone.
#
# Why this file exists: on 2026-08-28 the workspace carried `all = "warn"` and
# nothing else. `clippy::all` does not contain unwrap_used, expect_used or panic
# — those are `clippy::restriction` — so `cargo clippy -- -D warnings` returned
# exit 0 on a crate containing all three. A rule nothing reads is prose.
#
# Runs standalone: scripts/check-lint-config.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

CRATE="$TMP/lintcheck"
mkdir -p "$CRATE/src"

# Copy the workspace's [workspace.lints.*] blocks in verbatim, as [lints.*].
# Textual on purpose: no TOML parser, so what is tested is exactly what is
# written in Cargo.toml, including anything added later.
{
  printf '[package]\nname = "lintcheck"\nversion = "0.0.0"\nedition = "2024"\n\n'
  awk '
    /^\[workspace\.lints\./ { inb = 1; sub(/^\[workspace\./, "["); print; next }
    /^\[/                   { inb = 0 }
    inb                     { print }
  ' "$ROOT/Cargo.toml"
} > "$CRATE/Cargo.toml"

if ! grep -q '^\[lints\.clippy\]' "$CRATE/Cargo.toml"; then
  echo "FAIL: Cargo.toml has no [workspace.lints.clippy] block at all." >&2
  exit 1
fi

echo "--- lint configuration under test -------------------------------------"
sed -n '/^\[lints/,$p' "$CRATE/Cargo.toml"
echo "-----------------------------------------------------------------------"

cd "$CRATE"
status=0

# --- RED: exactly the three constructs CLAUDE.md §2.7 forbids ---------------
cat > src/lib.rs <<'RUST'
pub fn u(x: Option<u32>) -> u32 { x.unwrap() }
pub fn e(x: Option<u32>) -> u32 { x.expect("boom") }
pub fn p() -> ! { panic!("boom") }
RUST

out="$(cargo clippy --all-targets --quiet -- -D warnings 2>&1 || true)"

# Exit status alone is not enough: a malformed [lints] block also fails, and
# would make this test pass for the wrong reason. Each lint must be named.
missing=""
for lint in unwrap_used expect_used panic; do
  printf '%s\n' "$out" | grep -qE "index\.html#${lint}( |\$)" || missing="$missing $lint"
done

if [ -n "$missing" ]; then
  echo
  echo "FAIL: the workspace lints do not deny:$missing" >&2
  echo "      CLAUDE.md §2 non-negotiable 7 states that workspace clippy lints" >&2
  echo "      enforce this. Add to [workspace.lints.clippy] in Cargo.toml:" >&2
  echo >&2
  echo '        all = { level = "warn", priority = -1 }' >&2
  echo '        unwrap_used = "deny"' >&2
  echo '        expect_used = "deny"' >&2
  echo '        panic = "deny"' >&2
  echo >&2
  echo "      priority = -1 is not optional; cargo rejects the config without it." >&2
  echo "--- clippy output ---" >&2
  printf '%s\n' "$out" >&2
  status=1
else
  echo "RED  ok — clippy rejected unwrap(), expect() and panic!()"
fi

# --- GREEN: the same crate, written the way the rule asks -------------------
# Without this half, a config that denies everything would also "pass".
cat > src/lib.rs <<'RUST'
#[derive(Debug)]
pub enum Bad { Missing }
pub fn u(x: Option<u32>) -> u32 { x.unwrap_or(0) }
pub fn e(x: Option<u32>) -> u32 { x.unwrap_or_default() }
pub fn p(x: Option<u32>) -> Result<u32, Bad> { x.ok_or(Bad::Missing) }
RUST

if cargo clippy --all-targets --quiet -- -D warnings > "$TMP/green.log" 2>&1; then
  echo "GREEN ok — the same crate passes once the three are gone"
else
  echo
  echo "FAIL: the workspace lints reject code that follows the rule." >&2
  echo "      A gate that cannot be satisfied is a gate that gets switched off." >&2
  echo "--- clippy output ---" >&2
  cat "$TMP/green.log" >&2
  status=1
fi

exit "$status"
