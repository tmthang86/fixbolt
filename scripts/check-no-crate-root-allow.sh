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
# WHAT THIS CHECK SEES. `[changed 2026-09-12]` A1 and A2 no longer read the
# text of a crate root. They read `tools/attr-scan`, which lexes the file with
# `proc-macro2` — the lexer `rustc` itself uses — and prints one line per inner
# attribute at the crate root: `PATH:LINE<TAB>HEAD<TAB>IDENTS`. So the check now
# sees **every inner attribute the Rust lexer sees at a crate root**, in every
# spelling, because to a lexer a comment, a run of whitespace and a newline
# carry no meaning at all:
#
#   #![/*x*/allow(...)]        #![allow(...)]        # ! [ allow ( ... ) ]
#   /* c */ #![allow(...)]     #!\n[allow(...)]      #![cfg_attr(t, allow(..))]
#
# are one and the same three tokens — `#`, `!`, `[…]` — and each is a red.
# Equally, **a string is a string**: `#![doc = "#![allow(clippy::unwrap_used)]"]`
# stays green, and so does the pair `#![doc = "/*"]` … `#![doc = "*/"]` that
# would make a strip-the-comments-then-match pass lose everything between them.
# That pair is why the fix was a lexer rather than a fifth regex.
#
# This check does NOT see, and does not pretend to:
#   - an OUTER `#[allow]` on `mod foo;` — that silences one module, which is
#     item 55's ceiling to catch, not this script's;
#   - an INNER `#![allow]` written inside `mod x { … }` in the crate root file
#     — same reason, same item 55: `attr-scan` walks only the top-level token
#     stream and never steps into a Group;
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
# Needs: cargo metadata (so a reachable rust-toolchain.toml), jq, and a cargo
# that can build `tools/attr-scan`. `cargo run -q -p fixbolt-attr-scan` is invoked from inside the tree,
# never from a scratch directory, so `rustup` finds `rust-toolchain.toml` the
# ordinary way — see scripts/check-scratch-fixtures.sh for why that matters.

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
# Lex every crate root, once, with the lexer rustc uses. Relative paths go in,
# so a FAIL line reads the way a person would write the path. `cargo run` is
# invoked from $ROOT, so rustup resolves rust-toolchain.toml normally.
#
# attr-scan exits 2 for a file it cannot read or cannot lex. That is a REFUSAL,
# and this script passes it straight through: a gate that cannot see the source
# must go red, never green.
# ---------------------------------------------------------------------------
CRATE_SRC_REL=()
for f in "${CRATE_SRC_FILES[@]}"; do
  CRATE_SRC_REL+=("${f#"$ROOT"/}")
done

SCAN_STATUS=0
SCAN="$(cargo run -q -p fixbolt-attr-scan -- "${CRATE_SRC_REL[@]}")" || SCAN_STATUS=$?
if [[ "$SCAN_STATUS" -ne 0 ]]; then
  echo "check-no-crate-root-allow: FAIL — attr-scan refused, exit ${SCAN_STATUS} (its error is above)." >&2
  exit 2
fi

# The deny list A2 compares against is derived, never hard-coded, from the root
# Cargo.toml's own `= "deny"` lines — so a lint added tomorrow is covered
# without touching this file.
mapfile -t DENY_LINTS < <(
  grep -E '=[[:space:]]*"deny"' "$ROOT/Cargo.toml" \
    | sed -E 's/^[[:space:]]*([A-Za-z0-9_]+)[[:space:]]*=.*/\1/' \
    | sort -u
)

# ---------------------------------------------------------------------------
# A1 — no inner `allow`/`expect` at a crate root, bare or `cfg_attr`-wrapped.
#      `expect(...)` is treated exactly as `allow(...)`: it silences the lint
#      at the crate the same way and only fires if nothing turns out to need
#      excusing.
# A2 — no crate-root `warn(...)` naming a lint the workspace currently denies.
#      A `warn` where the workspace says `deny` LOWERS the level, which is the
#      same defect wearing a different word.
#
# Both read attr-scan's `PATH:LINE<TAB>HEAD<TAB>IDENTS`. HEAD is the first
# identifier inside the brackets; IDENTS is every identifier inside them,
# recursed through nested groups, which is what makes
# `cfg_attr(a, cfg_attr(b, allow(x)))` reveal its `allow`.
# ---------------------------------------------------------------------------
ATTRS=0

while IFS=$'\t' read -r loc head idents; do
  [[ -z "$loc" ]] && continue
  ATTRS=$((ATTRS + 1))

  read -ra IDENT_LIST <<< "$idents"

  # --- A1 -------------------------------------------------------------------
  offender=""
  case "$head" in
    allow | expect) offender="$head" ;;
    cfg_attr)
      for id in "${IDENT_LIST[@]}"; do
        case "$id" in
          allow | expect)
            offender="$id"
            break
            ;;
        esac
      done
      ;;
  esac

  if [[ -n "$offender" ]]; then
    echo "check-no-crate-root-allow: FAIL — crate-root allow at ${loc}: ${offender}(…)" >&2
    FAIL=1
    continue
  fi

  # --- A2 -------------------------------------------------------------------
  widens=0
  case "$head" in
    warn) widens=1 ;;
    cfg_attr)
      for id in "${IDENT_LIST[@]}"; do
        if [[ "$id" == "warn" ]]; then
          widens=1
          break
        fi
      done
      ;;
  esac

  if [[ "$widens" -eq 1 ]]; then
    for id in "${IDENT_LIST[@]}"; do
      for lint in "${DENY_LINTS[@]}"; do
        if [[ "$id" == "$lint" ]]; then
          echo "check-no-crate-root-allow: FAIL — crate-root warn lowers a workspace deny at ${loc}" >&2
          FAIL=1
          break 2
        fi
      done
    done
  fi
done <<< "$SCAN"

# A0b — zero inner attributes is not a pass either, for the same reason A0 is
# not. Every lib.rs in this workspace opens with `//!`, which the lexer turns
# into `#![doc = "…"]`, so the honest floor is well above zero and a total of 0
# means attr-scan is broken, not that the tree is clean. Compared against 0 and
# not against a floor on purpose: a floor would have to be maintained, and the
# `doc` attributes are counted precisely so that no maintenance is needed.
if [[ "$ATTRS" -eq 0 ]]; then
  echo "check-no-crate-root-allow: FAIL — attr-scan read nothing: 0 inner attributes over ${N} crate roots, which is a broken tool, not a clean workspace." >&2
  FAIL=1
fi

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

echo "check-no-crate-root-allow: ok — ${N} crate roots, ${M} manifests, ${ATTRS} inner attributes"
