#!/usr/bin/env bash
#
# The machine check behind `indexing_slicing = "deny"`, and the reason that line
# is a gate rather than a wish.
#
# CLAUDE.md §2 non-negotiable 7 forbids a panic in a library crate. `a[i..j]`
# panics and names none of `unwrap`, `expect` or `panic!`, so the three lints
# that enforce rule 7 cannot see it. `[measured 2026-09-06]` that is not
# hypothetical: a ring dispatch record gained a field between two existing ones,
# `fixed[OUT_SEQ..OUT_LAST_PROCESSED]` went from four bytes to five in silence,
# and `copy_from_slice` panicked inside `fixbolt-engine`.
#
# `crates/*/src` had 207 such sites on 2026-09-08, across 21 files, and cleaning
# them all at once would be a diff nobody could review. So each of those files
# carries `#![allow(clippy::indexing_slicing)]` and THIS script counts them
# anyway, with `--force-warn`, which overrides an `allow`. New code in a clean
# file is stopped by the lint; new code in a file that still owes is stopped by
# the ceiling below. The ceiling only ever goes down.
#
# Runs standalone: scripts/check-indexing-debt.sh
#
#   --show   print every site instead of only the total

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# The debt, as it stood when it was last lowered. LOWER THIS when sites go; do
# not raise it. `docs/plans/2026-09-07-three-gates-a-sibling-engine-has.md`.
#
# 2026-09-08: 207 -> 188, journal.rs (18 sites) and dispatch.rs (3) cleaned.
# 2026-09-08: 188 -> 184, clock.rs (4) cleaned by wave B plan 3 half A. The
#   file's `#![allow]` went with them: `parse_utc` grew a width and the new
#   code asks with `get`, so the four `s[8]`/`s[11]`/`s[14]`/`s[17]` that were
#   the whole of this file's debt were rewritten on the way past.
#
# **The floor is not 0.** Two of the remaining sites are `crc_table` and
# `crc32` in crates/engine/src/journal.rs, where the bound is a `& 0xFF` mask
# and a `const fn` respectively; both carry a scoped `#[allow]` naming the
# proof, and `--force-warn` counts them anyway — which is right, because the
# count is of subscripts, not of unexcused ones. When only sites like those two
# are left, the ceiling stops moving and STATUS.md item 55 closes by saying so.
CEILING=181

# `--force-warn` is the whole mechanism: unlike `-W`, it overrides `#![allow]`
# in the source. Proven by counting either side of adding one — see the
# `--force-warn overrides an allow` check in this repository's plan.
#
# Lib and bin targets only. `--all-targets` would also compile `tests/`,
# `benches/` and `examples/`, whose ~400 sites are out of scope: non-negotiable
# 7 is about library crates, and an index that panics in a test is a failing
# test, which is what a test is for.
#
# Default features. `affinity` is off by default and Linux-only (ADR-0015), so
# a run with it on would count a different set of files and the ceiling would
# mean two things at once.
LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

# `--color never`, and it is not cosmetic. `[measured 2026-09-08]` the CI
# workflow sets `CARGO_TERM_COLOR: always` at the top of the file, so cargo
# wrapped every `--message-format short` line in ANSI escapes, the grep below
# matched none of them, and this script counted **0** on a workspace with 188.
# The zero-guard further down is what turned that into a red job instead of a
# green one; without it the ceiling would have read as met, for ever, by a
# check that had stopped looking. Two of this repository's brand-new gates were
# broken by the same one-line environment variable on the same run.
CLIPPY=(cargo clippy --color never --message-format short)

count_with() {
  # $@: extra cargo arguments, e.g. --features affinity
  if ! "${CLIPPY[@]}" --workspace "$@" -- \
      --force-warn clippy::indexing_slicing >>"$LOG" 2>&1; then
    echo "check-indexing-debt: clippy itself failed ($*); the count below would be meaningless" >&2
    tail -20 "$LOG" >&2
    exit 2
  fi
}

count_with

# **A feature-gated file is invisible to a run that does not turn the feature
# on.** `[measured 2026-09-08]` `crates/engine/src/affinity.rs` holds three
# sites and sat behind `--features affinity`, off by default and Linux-only
# (ADR-0015); the first version of this script never compiled it, the ceiling
# never counted it, and CI's own `--features affinity` step is what found them.
# Counted here on Linux, where the feature exists at all — the union of the two
# runs, deduplicated, so a file compiled twice is not two debts.
if [[ "$(uname -s)" == "Linux" ]]; then
  count_with --features affinity
fi

# Dedup on the WHOLE line — path, line, column AND message — not on the span.
# `[measured 2026-09-08]` `self.queue[i].buf[..len]` at
# `crates/session/src/lib.rs:2176:42` earns two warnings at one column,
# `slicing may panic` and `indexing may panic`, because it is two hazards in
# one expression. Deduping on `file:line:col` alone reads that as one site and
# the total comes out 205 instead of 207 — a two-site discount for writing the
# denser expression, which is exactly backwards.
SITES="$(grep -E '^crates/[a-z0-9_]+/src/[a-zA-Z0-9_]+\.rs:[0-9]+:[0-9]+: warning: (indexing|slicing) may panic' "$LOG" | sort -u || true)"
COUNT="$(printf '%s' "$SITES" | grep -c '^' || true)"

if [[ "${1:-}" == "--show" ]]; then
  printf '%s\n' "$SITES"
fi

echo "check-indexing-debt: ${COUNT} indexing/slicing sites in crates/*/src, ceiling ${CEILING}"

# Zero is not a pass. This repository has already shipped a gate that was green
# because it read nothing — `cargo test --all --no-default-features` compiled
# `libc` anyway and the job was green about a build that never happened
# (docs/reference/feature-flags-unify-across-a-workspace.md). A count of 0 here
# means clippy emitted nothing, which is a broken invocation long before it is
# a clean workspace — see the note beside CEILING for what the real floor is.
if [[ "$COUNT" -eq 0 ]]; then
  echo "check-indexing-debt: FAIL — 0 sites counted. clippy emitted nothing, which is" >&2
  echo "  a broken invocation, not a clean workspace. Read $LOG's shape before believing it." >&2
  exit 1
fi

if [[ "$COUNT" -gt "$CEILING" ]]; then
  echo "check-indexing-debt: FAIL — the debt went UP, ${CEILING} -> ${COUNT}." >&2
  echo "  A new a[i] or a[i..j] in crates/*/src. Use .get() / .get_mut() and handle" >&2
  echo "  the None with a fieldless error — CLAUDE.md §2 non-negotiables 1 and 7." >&2
  echo "  Run scripts/check-indexing-debt.sh --show to see every site." >&2
  exit 1
fi

if [[ "$COUNT" -lt "$CEILING" ]]; then
  echo "check-indexing-debt: FAIL — the debt went DOWN, ${CEILING} -> ${COUNT}, and the" >&2
  echo "  ceiling in this script still says ${CEILING}. Lower it to ${COUNT} in the same" >&2
  echo "  commit. A ceiling left above the real number is permission to put the" >&2
  echo "  sites back, which is the one thing a ratchet exists to prevent." >&2
  exit 1
fi

echo "check-indexing-debt: ok"
