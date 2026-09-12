#!/usr/bin/env bash
#
# The machine check that `scripts/check-indexing-debt.sh` could not be, because
# it is a check on ONE lint and this defect is not about a lint.
#
# `[measured 2026-09-08]` `#![allow(clippy::indexing_slicing)]` written as line 1
# of a `lib.rs` is an INNER attribute — it does not silence "this file", it
# silences the whole crate, submodules included. Three of the 21 files that
# carried the indexing-debt annotation were `lib.rs`, so the biggest three
# crates in this workspace had `indexing_slicing = "deny"` switched off for as
# long as that stood, while `check-indexing-debt.sh`'s ratchet kept reading
# exactly the right number the whole time — `--force-warn` overrides an
# `#![allow]`, so the counting half never noticed the denying half was inert.
# See docs/reference/an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md.
#
# That bug was one lint. The class is bigger: `#![allow(clippy::unwrap_used)]`
# at a crate root turns off non-negotiable 7 just as completely, and
# `check-lint-config.sh` — which proves the workspace DENIES unwrap/expect/panic
# — cannot see it, because it inspects Cargo.toml, not source. A check keyed to
# one lint name has to be rewritten every time a lint is added; this one is not
# keyed to a lint at all. Any inner attribute at a crate root that WIDENS a
# lint is wrong, full stop — the only things allowed there are attributes that
# TIGHTEN (`forbid`, `deny`), `no_std`, `doc`, or `warn` naming a lint the
# workspace does not itself deny. `expect(...)` is treated the same as
# `allow(...)`: it silences the lint at the crate exactly the same way, and
# only fires if there turns out to be no site to excuse.
#
# Scope: every `lib` and `bin` target's `src_path`, for packages under
# `crates/`, read from `cargo metadata` — never from a file glob, so a future
# `crates/x/src/bin/foo.rs` cannot go uncounted by having the wrong name.
# `tools/` is deliberately excluded: non-negotiable 7 is about LIBRARY crates,
# `check-indexing-debt.sh` already excludes `tools/` on the same reasoning, and
# `tools/w2w` and `tools/interop` carry their own scoped, commented allows on
# purpose — a check that ships with an allow-list on day one is a check with
# two holes already named. A3 (below) is not scoped this way: it reads every
# workspace member `cargo metadata` reports, `tools/` included, because a
# per-crate `[lints.*]` override bypasses the workspace lints regardless of
# whether the crate is a library.
#
# This check does NOT see, and does not pretend to:
#   - an OUTER `#[allow]` on `mod foo;` — that silences one module, which is
#     item 55's ceiling to catch, not this script's;
#   - `RUSTFLAGS=-A ...` set from the environment (CI deliberately never sets
#     RUSTFLAGS — see ci.yml:25-29);
#   - `--cap-lints` passed from a command outside this repository;
#   - an `allow` emitted by an `include!`-generated file onto the generated
#     ITEMS rather than the crate root (item 58 hit this already: the fix is
#     the generator emitting the attribute on the function, which is exactly
#     why it does not land here).
# When any of these shows up for real, extend this script in the same commit
# that finds it — do not add it to a list "for later".
#
# Runs standalone: scripts/check-no-crate-root-allow.sh
# Needs: cargo metadata (so a reachable rust-toolchain.toml), jq.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FAIL=0

# ---------------------------------------------------------------------------
# Gather scope from cargo metadata. Never from find/glob — see the header.
# ---------------------------------------------------------------------------
METADATA="$(cargo metadata --format-version 1 --no-deps 2>&1)" || {
  echo "check-no-crate-root-allow: cargo metadata failed:" >&2
  echo "$METADATA" >&2
  exit 2
}

# A1/A2 scope: lib+bin src_path of packages whose manifest lives under crates/.
mapfile -t CRATE_SRC_FILES < <(
  printf '%s' "$METADATA" | jq -r --arg root "$ROOT/crates/" '
    .packages[]
    | select(.manifest_path | startswith($root))
    | .targets[]
    | select((.kind | contains(["lib"])) or (.kind | contains(["bin"])))
    | .src_path
  ' | sort -u
)

# A3 scope: every workspace member's manifest, crates/ and tools/ alike.
mapfile -t ALL_MANIFESTS < <(
  printf '%s' "$METADATA" | jq -r '.packages[].manifest_path' | sort -u
)

N=${#CRATE_SRC_FILES[@]}
M=${#ALL_MANIFESTS[@]}

# A0 — zero is not a pass. Mirrors check-indexing-debt.sh:116: a count of 0
# from a tool that enumerates files is a broken invocation, not proof of a
# clean workspace, until something makes 0 the known-correct answer.
if [[ "$N" -eq 0 ]]; then
  echo "check-no-crate-root-allow: FAIL — 0 crate roots found — broken invocation, not a clean workspace." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# A1 — no inner allow/expect (bare or cfg_attr-wrapped) at a crate root.
# ---------------------------------------------------------------------------
A1_RE='^[[:space:]]*#!\[[[:space:]]*(cfg_attr\([^]]*,[[:space:]]*)?(allow|expect)\b'

for f in "${CRATE_SRC_FILES[@]}"; do
  rel="${f#"$ROOT"/}"
  while IFS=: read -r lineno line; do
    [[ -z "$lineno" ]] && continue
    echo "check-no-crate-root-allow: FAIL — crate-root allow at ${rel}:${lineno}: ${line#"${line%%[![:space:]]*}"}" >&2
    FAIL=1
  done < <(grep -nE "$A1_RE" "$f" || true)
done

# ---------------------------------------------------------------------------
# A2 — no crate-root `warn(...)` that names a lint the workspace currently
# denies. The deny list is derived, never hard-coded, from the root
# Cargo.toml's own `= "deny"` lines.
# ---------------------------------------------------------------------------
mapfile -t DENY_LINTS < <(
  grep -E '=[[:space:]]*"deny"' "$ROOT/Cargo.toml" \
    | sed -E 's/^[[:space:]]*([A-Za-z0-9_]+)[[:space:]]*=.*/\1/' \
    | sort -u
)

A2_RE='^[[:space:]]*#!\[[[:space:]]*warn\('

for f in "${CRATE_SRC_FILES[@]}"; do
  rel="${f#"$ROOT"/}"
  while IFS=: read -r lineno line; do
    [[ -z "$lineno" ]] && continue
    for lint in "${DENY_LINTS[@]}"; do
      if echo "$line" | grep -qE "\\b${lint}\\b"; then
        echo "check-no-crate-root-allow: FAIL — crate-root warn lowers a workspace deny at ${rel}:${lineno}" >&2
        FAIL=1
        break
      fi
    done
  done < <(grep -nE "$A2_RE" "$f" || true)
done

# ---------------------------------------------------------------------------
# A3 — every workspace member's Cargo.toml inherits the workspace lints
# wholesale: a `[lints]` table that is exactly `workspace = true`, and no
# `[lints.*]` table of its own overriding any part of it.
# ---------------------------------------------------------------------------
for manifest in "${ALL_MANIFESTS[@]}"; do
  rel="${manifest#"$ROOT"/}"

  if grep -qE '^\[lints\.' "$manifest"; then
    echo "check-no-crate-root-allow: FAIL — ${rel} does not inherit the workspace lints" >&2
    FAIL=1
    continue
  fi

  if ! grep -qE '^\[lints\]' "$manifest"; then
    echo "check-no-crate-root-allow: FAIL — ${rel} does not inherit the workspace lints" >&2
    FAIL=1
    continue
  fi

  block="$(awk '/^\[lints\]/{f=1; next} /^\[/{f=0} f' "$manifest" \
    | sed -E 's/#.*$//' \
    | sed -E '/^[[:space:]]*$/d' \
    | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//')"

  if [[ "$block" != "workspace = true" ]]; then
    echo "check-no-crate-root-allow: FAIL — ${rel} does not inherit the workspace lints" >&2
    FAIL=1
  fi
done

if [[ "$FAIL" -ne 0 ]]; then
  exit 1
fi

echo "check-no-crate-root-allow: ok — ${N} crate roots, ${M} manifests"
