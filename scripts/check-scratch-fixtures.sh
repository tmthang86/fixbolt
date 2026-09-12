#!/usr/bin/env bash
#
# Guards the CLASS behind docs/reference/a-scratch-fixture-inherits-the-machine.md,
# not the one instance already fixed in check-lint-config.sh.
#
# `[measured 2026-08-31]` check-lint-config.sh built a throwaway crate in
# `mktemp -d`, where rust-toolchain.toml does not reach. On a machine with no
# `rustup default` set, that was a FALSE RED about the workspace's clippy lints
# while `cargo clippy` had never run at all; on a machine that DID have a
# default, it was a quieter false green — testing the lint config against a
# different clippy from the one the workspace pins. The fix there was one line:
# copy rust-toolchain.toml into the scratch crate before using it.
#
# Nothing stops the NEXT fixture from being written the same way, and an
# allow-list of "the three scripts known to be fine today" would be exempt
# forever — the day one of them grows a `cargo build` inside its `$TMP`, no
# check would notice. So the rule here is not about script names, it is the
# rule rustup itself uses: rust-toolchain.toml (and friends) are discovered by
# walking UP from the current directory. The trigger is CD-ING INTO A
# DIRECTORY OUTSIDE THE TREE, not calling `mktemp` — a script that builds a
# scratch file with `mktemp -d` but never `cd`s into it (or never points
# `cargo`/`--manifest-path` at it) never leaves the tree that rustup already
# walks up from, and passes without being named.
#
# Four assertions:
#   B0 — the pinning-artefact set is derived from what EXISTS at the repo root
#        today (never hard-coded). Empty set is itself a failure: a deleted
#        rust-toolchain.toml must be heard, not silently pass.
#   B1 — for each script in scripts/ (SCOPE below says which files those are),
#        collect "scratch variables": names assigned from `mktemp`, `$TMPDIR`,
#        or a `/tmp/` literal, then iterate to a
#        fixed point, adding names assigned from an existing scratch variable
#        (check-lint-config.sh: TMP, then CRATE = "$TMP/lintcheck").
#   B2 — a line that ENTERS a scratch dir (`cd`/`pushd` with an argument
#        containing a scratch variable, or `--manifest-path`/`-C` pointing
#        into one) must be accompanied, in the SAME script, by a `cp` naming
#        EACH B0 artefact and that scratch variable (or one derived from it).
#        B2b — entering a scratch dir that names no scratch VARIABLE at all
#        (e.g. `cd "$(mktemp -d)"`) fails outright: there is no variable to
#        check a copy against.
#   B3 — `grep -rn 'Command::new("cargo")\|Command::new("rustc")' crates tools`
#        must be empty. A Rust test that spawns the toolchain in `temp_dir()`
#        is the same class, and this script cannot read Rust — so it REFUSES
#        and says go teach the check first, rather than silently passing.
#   B4 — 0 scripts scanned is not a pass, same reasoning as
#        check-indexing-debt.sh's zero-guard: a count of nothing is a broken
#        invocation, not a clean workspace.
#
# SCOPE, and why it is not a glob. `[measured 2026-09-12]` this script globbed
# `scripts/*.sh`, so `scripts/fixture-probe.bash` — a shell script that `cd`s
# into `$(mktemp -d)` and copies nothing — was not scanned at all and the gate
# said "20 scripts, ok". Widening the glob to `*.sh *.bash` would only move the
# hole to `*.ksh`, or to a script with no extension: an extension is a naming
# convention, and this repository has already paid twice for a gate that is a
# list somebody must remember to extend (STATUS.md items 62 and 68). So the
# scope is now the property the KERNEL reads to decide the file is a shell
# script — its shebang. Every regular file directly in `scripts/` is in scope
# if its first line is `#!` naming an interpreter whose name ends in `sh`
# (`sh`, `bash`, `dash`, `ksh`, `zsh`, with or without `env` and with or
# without trailing flags), or if it is named `*.sh` at all, which keeps a
# `.sh` file that forgot its shebang in scope. `check-links.py` is out because
# its shebang says `python3`, which is also the honest answer: this script
# cannot read Python, and says so two bullets down.
#
# What this check does NOT see, said before it is found the expensive way:
#   - a `cd` reached through a function call or `eval`
#   - `cd -- "$(dirname "$X")"` or other indirection that hides the target
#   - a fixture written inside a CI `run:` step under $RUNNER_TEMP (ci.yml,
#     not scripts/*.sh, is outside this script's grep)
#   - a fixture script written in Python (check-links.py) or another language
#     — the shebang scope above puts it out deliberately, not by accident
#   - a `cp` whose pin name is only inside a quoted string that also begins
#     with a ` #` — the trailing-comment strip below would cut it. That is a
#     spurious red, the same direction as the `cd`-after-`#`-in-a-string case
#     below, not a false green
#   - an artefact pinned at the MACHINE level ($CARGO_HOME/config.toml) — no
#     repository file can pin that, so no repository check can see it either
#   - the reference doc's second failure direction: the right file was copied
#     but that exact toolchain is not installed locally — rustup installs it
#     on demand, and on the §9 machine that means reading the output, not the
#     exit status, same as everywhere else in this repository
#   - order: a `cp` appearing after the `cd`/`pushd` in the script still
#     counts as covering it (see the per-file "live line" section below)
#   - a `cd` that sits after a `#` inside a string on the SAME line (very
#     rare) — the entering line is still scanned whole, so the wrong
#     direction there is a spurious red, not a false green
#   - `[measured 2026-09-12]` a `cp` inside a heredoc body — the body is data,
#     not a command, and a line inside it that reads like `cp` is not one
#   - `[measured 2026-09-12]` a `cp` that copies OUT of the scratch dir
#     rather than into it — B2 checks that a `cp` naming the artefact and the
#     scratch variable is present, not which way the copy runs
#   - `[measured 2026-09-12]` `read -r TMP < <(mktemp -d)` — TMP is seeded by
#     a `read`, not by an assignment, so B1's scratch-variable scan never
#     picks it up
#   - `[measured 2026-09-12]` a bare `TMP=/tmp`, or `TMP=/var/tmp` — B1a
#     recognises a literal scratch path by the substring `/tmp/`, WITH the
#     trailing slash, so a path that ends at the directory itself seeds
#     nothing. This is narrower than it looks and the narrow reading is the
#     true one: `TMP=/tmp/zzz` and `SCRATCH=/var/tmp/x` are literal paths with
#     no `mktemp` on the line and both ARE caught. Only the two bare spellings
#     slip
# These four are a known, closed set, not grown by one more regex each —
# see ADR-0061 for why the gate stays as it is.
# Each, when it is hit for real, gets the same answer: extend this script in
# the same commit, per CLAUDE.md §4's "discover a protocol trap" row.
#
# Runs standalone: scripts/check-scratch-fixtures.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# --- B0: the pinning-artefact set is whatever exists today -------------------
CANDIDATES=(
  rust-toolchain.toml
  rust-toolchain
  clippy.toml
  .clippy.toml
  .cargo/config.toml
  rustfmt.toml
  .rustfmt.toml
)
PINS=()
for c in "${CANDIDATES[@]}"; do
  if [[ -f "$ROOT/$c" ]]; then
    PINS+=("$c")
  fi
done

if [[ "${#PINS[@]}" -eq 0 ]]; then
  echo "check-scratch-fixtures: FAIL — no pinning artefact at the repo root; nothing to copy means nothing to check" >&2
  exit 1
fi

shopt -s nullglob
CANDIDATES_IN_SCRIPTS=(scripts/*)
shopt -u nullglob

SCRIPTS=()
for cand in "${CANDIDATES_IN_SCRIPTS[@]}"; do
  [[ -f "$cand" ]] || continue
  if [[ "$cand" == *.sh ]]; then
    SCRIPTS+=("$cand")
    continue
  fi
  shebang=""
  IFS= read -r shebang < "$cand" || true
  case "$shebang" in
    '#!'*sh | '#!'*sh[[:space:]]*) SCRIPTS+=("$cand") ;;
  esac
done

# --- B4: zero scripts scanned is not a pass ----------------------------------
if [[ "${#SCRIPTS[@]}" -eq 0 ]]; then
  echo "check-scratch-fixtures: FAIL — 0 scripts scanned" >&2
  exit 1
fi

status=0
enter_count=0

# A scratch-variable reference: `$VAR` or `${VAR}` with a non-identifier
# character (or a string boundary) on each side, so TMP does not match TMPDIR.
refers_to() {
  local haystack="$1" var="$2"
  [[ "$haystack" =~ (^|[^A-Za-z0-9_])\$\{?${var}\}?([^A-Za-z0-9_]|$) ]]
}

# A "live" line is one whose first non-whitespace character is not `#`. In
# `bash`, outside a heredoc or a multi-line string, a line starting with `#`
# is always a comment, so it is never read by B1a, B1b, B2 or the `cp` search
# below — a commented-out `cd` never seeds a scratch var, and a commented-out
# `cp` never counts as a copy. (A `cp` line that happens to live inside a
# heredoc's DATA and itself starts with `#` is filtered the same way — it was
# never a real `cp` invocation to begin with, so excluding it costs nothing.)
is_live() {
  [[ ! "$1" =~ ^[[:space:]]*# ]]
}

# `cp` counts only in *command position*: at the start of a line, or right
# after a command separator (`;` `&&` `||` `|` `(` `{`) or a keyword that
# starts a new command (`then` `do` `else`). `echo x # cp …` does not match
# (`cp` sits after `#`, not a separator); `echo cp …` does not match (`cp`
# sits after a plain word). `sudo cp`, `command cp` and `\cp` do not match
# either — the wrong direction here is red, and the fix is to write a plain
# `cp` at the call site.
CP_POSITION_RE='(^|[;&|({]|[[:space:]](then|do|else))[[:space:]]*cp[[:space:]]'

# Seed regex: one optional leading word, with optional -flags, in front of
# the assignment — so `readonly TMP=`, `declare -r TMP=`, `typeset -g TMP=`,
# `export TMP=` and `local TMP=` all seed, as does a bare `TMP=`. It also
# catches `echo TMP=…`; over-seeding only makes the check look at MORE lines,
# which is the safe direction.
ASSIGN_RE='^[[:space:]]*([A-Za-z]+([[:space:]]+-[A-Za-z]+)*[[:space:]]+)?([A-Za-z_][A-Za-z0-9_]*)=(.*)$'

declare -A is_scratch
declare -A origin_line

for f in "${SCRIPTS[@]}"; do
  is_scratch=()
  origin_line=()

  # --- B1a: seed pass — vars assigned straight from mktemp/$TMPDIR//tmp/ ----
  line_no=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    line_no=$((line_no + 1))
    is_live "$line" || continue
    if [[ "$line" =~ $ASSIGN_RE ]]; then
      var="${BASH_REMATCH[3]}"
      rhs="${BASH_REMATCH[4]}"
      if [[ -z "${is_scratch[$var]:-}" ]]; then
        if [[ "$rhs" == *mktemp* || "$rhs" == *'TMPDIR'* || "$rhs" == *'/tmp/'* ]]; then
          is_scratch[$var]=1
          origin_line[$var]=$line_no
        fi
      fi
    fi
  done < "$f"

  # --- B1b: propagate to a fixed point — vars derived from a scratch var ----
  changed=1
  while [[ "$changed" -eq 1 ]]; do
    changed=0
    line_no=0
    while IFS= read -r line || [[ -n "$line" ]]; do
      line_no=$((line_no + 1))
      is_live "$line" || continue
      if [[ "$line" =~ $ASSIGN_RE ]]; then
        var="${BASH_REMATCH[3]}"
        rhs="${BASH_REMATCH[4]}"
        if [[ -n "${is_scratch[$var]:-}" ]]; then
          continue
        fi
        for sv in "${!is_scratch[@]}"; do
          if refers_to "$rhs" "$sv"; then
            is_scratch[$var]=1
            origin_line[$var]=${origin_line[$sv]}
            changed=1
            break
          fi
        done
      fi
    done < "$f"
  done

  # --- B2 / B2b: lines that enter a scratch dir ------------------------------
  line_no=0
  # shellcheck disable=SC2094
  # SC2094 is read-and-write of one file in one pipeline. Both uses of
  # "$f" here are READS — this script writes no file at all — so the
  # warning is a false positive, and it is disabled at the two loops it
  # points at rather than for the file, so a real one elsewhere still
  # goes red. `shellcheck -S info` in CI is what makes this visible:
  # `-S warning` never reported it, and never reports SC2086 either.
  while IFS= read -r line || [[ -n "$line" ]]; do
    line_no=$((line_no + 1))
    is_live "$line" || continue

    entered_var=""
    entering=0

    if [[ "$line" =~ (^|[^A-Za-z0-9_])(cd|pushd)[[:space:]]+(.*)$ ]] \
       || [[ "$line" =~ (--manifest-path[=[:space:]]|-C[[:space:]]) ]]; then
      entering=1
      for sv in "${!is_scratch[@]}"; do
        if refers_to "$line" "$sv"; then
          entered_var="$sv"
          break
        fi
      done
    fi

    # B2b: a line that enters a scratch dir but names no scratch VARIABLE at
    # all — `cd "$(mktemp -d)"` and siblings — cannot be checked for a copy,
    # so it fails outright rather than passing silently because B1 never had
    # anything to seed.
    if [[ "$entering" -eq 1 && -z "$entered_var" ]]; then
      if [[ "$line" =~ mktemp|TMPDIR|/tmp/ ]]; then
        echo "check-scratch-fixtures: FAIL — enters a scratch dir it never named at ${f}:${line_no} — assign it to a variable so the copy can be checked" >&2
        status=1
        continue
      fi
    fi

    if [[ -n "$entered_var" ]]; then
      enter_count=$((enter_count + 1))
      origin="${origin_line[$entered_var]}"

      # Every scratch var sharing this origin is a valid copy destination:
      # check-lint-config.sh copies into $CRATE (derived from $TMP), not $TMP.
      family=()
      for sv in "${!is_scratch[@]}"; do
        if [[ "${origin_line[$sv]}" == "$origin" ]]; then
          family+=("$sv")
        fi
      done

      for pin in "${PINS[@]}"; do
        found=0
        # shellcheck disable=SC2094
        # SC2094 is read-and-write of one file in one pipeline. Both uses of
        # "$f" here are READS — this script writes no file at all — so the
        # warning is a false positive, and it is disabled at the two loops it
        # points at rather than for the file, so a real one elsewhere still
        # goes red. `shellcheck -S info` in CI is what makes this visible:
        # `-S warning` never reported it, and never reports SC2086 either.
        while IFS= read -r cpline; do
          for fv in "${family[@]}"; do
            if refers_to "$cpline" "$fv"; then
              found=1
              break
            fi
          done
          [[ "$found" -eq 1 ]] && break
        # The trailing-comment strip is B-5's fix. `[measured 2026-09-12]`
        # `cp "$ROOT/Cargo.toml" "$CRATE/Cargo.toml.bak"   # not
        # rust-toolchain.toml` satisfied this search: `grep -F -- "$pin"`
        # matches the pin name ANYWHERE on the line, and a comment is
        # anywhere. The line is cut at its first ` #` before the pin is
        # looked for, so a pin named only in a comment no longer counts as a
        # copy. `refers_to` reads the cut line too, so a scratch variable
        # that appears only in the comment does not count either.
        done < <(grep -vE '^[[:space:]]*#' "$f" | sed -E 's/[[:space:]]+#.*$//' | grep -E "$CP_POSITION_RE" | grep -F -- "$pin")

        if [[ "$found" -eq 0 ]]; then
          echo "check-scratch-fixtures: FAIL — ${f}:${line_no} enters a scratch dir (\$${entered_var}, from mktemp at :${origin}) and never copies ${pin} into it" >&2
          status=1
        fi
      done
    fi
  done < "$f"
done

# --- B3: the toolchain spawned from Rust is the same class, and this check --
# --- cannot read Rust, so it refuses rather than silently passing ----------
b3_hits="$(grep -rn 'Command::new("cargo")\|Command::new("rustc")' crates tools 2>/dev/null || true)"
if [[ -n "$b3_hits" ]]; then
  while IFS= read -r hit; do
    hfile="${hit%%:*}"
    hrest="${hit#*:}"
    hline="${hrest%%:*}"
    echo "check-scratch-fixtures: FAIL — ${hfile}:${hline} spawns the toolchain from Rust; teach this check where it runs before adding one" >&2
  done <<<"$b3_hits"
  status=1
fi

if [[ "$status" -ne 0 ]]; then
  exit 1
fi

echo "check-scratch-fixtures: ok — ${#SCRIPTS[@]} scripts, ${enter_count} enter a scratch dir, ${#PINS[@]} pins"
