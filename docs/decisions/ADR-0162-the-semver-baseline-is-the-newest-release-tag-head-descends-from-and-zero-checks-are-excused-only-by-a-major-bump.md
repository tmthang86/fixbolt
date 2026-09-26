# ADR-0162 — The semver baseline is the newest release tag HEAD descends from, and zero checks are excused only by a major-level bump

- **Status**: **Accepted — 2026-09-24, by the manager under the owner's delegation of 2026-09-18.** Proposed the same day. Written by the architect (Opus) from finding F3 of the
  senior review of PR #112 (plan row 8b). The manager accepts it; it is a
  correction of how a gate is enforced, inside a decision the owner already made (ADR-0161), and
  changes nothing the owner chose. On acceptance it **supersedes
  [ADR-0161](ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)
  decision 5's second sentence** (the exception: "when the workspace version differs from the
  tag's version … it passes") **and its last sentence as far as the semver gate goes** ("The
  baseline literal moves to the new tag in the pull request that follows each new tag"). The rest
  of decision 5 stands: blocking, a wrapper, N > 0 per crate, versions read from the manifest with
  `tomllib`, never from cargo-semver-checks' wording. ADR-0161 decision 4 (the `stranger-git` job's
  `--tag` literal and the page's install line) is **not** changed.
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: ADR-0161 decisions 2, 4, 5 and *Context* fact 6; ADR-0160 decision 7;
  `scripts/check-semver-against-tag.sh`; `.github/workflows/ci.yml` job `semver`; `RELEASING.md`
  *Cutting a release*, the list of literals that move after each tag;
  [plan p3-packaging, *Sửa 3*](../plans/2026-09-23-p3-packaging-and-first-release.md).

## Why a new ADR, not an addendum

ADR-0161 is Accepted and merged. Decision 5 says, in so many words, that the wrapper passes
whenever the two versions differ and that the baseline is a literal moved by hand after each tag.
This ADR replaces both clauses with different rules, so it changes the decision's substance, and
`CLAUDE.md` §5 forbids editing an accepted ADR's substance. It is not a clarification.

## Context

The senior review of PR #112 (row 8b, `scripts/check-semver-against-tag.sh` as built to ADR-0161
decision 5) found two ways the wrapper goes green without having compared anything. ADR-0161
*Context* fact 6 is exactly this failure: cargo-semver-checks classifies a major-level bump,
skips all 254 lints and still exits 0.

1. **A baseline literal that goes stale.** `ci.yml` runs `scripts/check-semver-against-tag.sh
   v0.1.0`. After `v0.2.0` is tagged on a `main` whose workspace version is `0.2.0`, the job still
   names `v0.1.0` until someone moves it by hand. `0.1.0` → `0.2.0` is major under Cargo's `0.y`
   rule. Each crate then runs 0 checks, the wrapper excuses all six with a `NOTE` because the
   versions differ, and the gate stays green **on every pull request** until someone notices.
   Only a hand step stands between the gate and that state.
2. **The exception is too wide.** It excuses 0 checks on *any* version difference. A patch-level
   difference (`0.1.0` → `0.1.1`) is a *minor* change in cargo-semver-checks' zerover
   classification and runs the lint set (ADR-0161 *Research*, `check_release.rs`). So 0 checks
   there means something else broke, and the wrapper would hide it.

The `stranger-git` literal (ADR-0161 decision 4) does not fail this way. `stranger-check.sh
--from git` fails when the page's install line names a different tag than the job, so a stale
literal there turns red instead of green.

## Research

| Source | What it says | Bearing |
|---|---|---|
| <https://github.com/obi1kenobi/cargo-semver-checks-action> | "comparing it to the latest normal (not pre-release or yanked) version published on crates.io" is the default baseline; `baseline-rev` is the explicit override | The tool's own default is a **derived** baseline, "the latest normal release", not a literal. This ADR applies the same rule to git tags |
| <https://raw.githubusercontent.com/obi1kenobi/cargo-semver-checks/v0.50.0/src/check_release.rs> | `classify_minimum_semver_version_change`: a major change → Major; a minor change with major 0 → Major; a patch change with `0.0` → Major, with `0.y` → Minor, otherwise Patch; a pre-release difference → Major; lints are kept only if the level does not already support their required update | The exact classification the wrapper must reproduce to know when 0 checks is expected |
| <https://git-scm.com/docs/git-tag> | `--merged [<commit>]`: "Only list tags whose commits are reachable from `<commit>` (HEAD if not specified)"; `--sort=v:refname` treats names as versions, and `versionsort.suffix` configuration can change that order | "Newest release this history descends from" is `git tag --merged HEAD`. The maximum is computed by the script from the parsed numbers, not from `--sort`, which a user's git configuration can reorder |
| <https://git-scm.com/docs/git-worktree> (*Refs*) | All refs under `refs/` are shared between worktrees; only pseudo-refs are per-worktree | A throwaway tag made in a `git worktree` for a reversal is a real tag in this repository's refs, one `git push --tags` from GitHub. Reversals that make tags run in a **`git clone`**, never a worktree |
| <https://github.com/actions/checkout> | `fetch-depth: 0` fetches all history and tags | The `semver` job already sets it, so `git tag --merged HEAD` sees every release tag on `main` |

**Searched, found nothing:** a cargo-semver-checks option that derives a baseline from git tags by
itself. `--baseline-rev` takes one revision, and the action's derivation works only against
crates.io.

## Decision

`scripts/check-semver-against-tag.sh` takes **no argument**. `ci.yml`'s `semver` job calls it
without one, and `RELEASING.md` drops the semver job from the literals that move after a tag. The
script implements exactly these rules, in this order:

1. **The baseline is derived.** Candidates are the tags `git tag --merged HEAD` lists whose names
   match `^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$` (no pre-release, no build
   metadata). The baseline `B` is the candidate with the highest `(major, minor, patch)`, compared
   as integers. No candidate → exit 2 (`no release tag reachable from HEAD — git fetch --tags?`).
   `B`'s `Cargo.toml` (`git show B:Cargo.toml`, `[workspace.package] version`, `tomllib`) must equal
   `B`'s name without the `v`; otherwise it is a FAIL (`tag vX.Y.Z points at a commit whose
   workspace version is W`).
2. **The current version `V` must be plain and not behind.** `V` (`Cargo.toml`, same reading) with
   pre-release or build metadata → exit 2. `V < B` → FAIL. If a tag named `v<V>` exists **anywhere**
   in the repository (`git rev-parse -q --verify refs/tags/v<V>`) and is not `B`, that is a FAIL:
   a release of this exact version exists that this history does not descend from, and the check
   refuses to pick a baseline for it.
3. **`V == B` → every one of the six published crates must print `Checked … N checks` with
   N > 0.** No exception.
4. **`V > B` → 0 checks is excused only if the bump is major-level by Cargo's rule**, computed from
   the numbers exactly as `check_release.rs` does: `V.major != B.major`; or `V.major == 0` and
   `V.minor != B.minor`; or `V.major == 0 && V.minor == 0` and `V.patch != B.patch`. Then each
   0-check crate prints a `NOTE` naming `B`, `V` and "major-level bump". Any other `V > B`, such as
   `0.1.0` → `0.1.1`, is treated as rule 3: N > 0 is required.
5. **cargo-semver-checks' own non-zero exit is still returned unchanged**, after rules 1–4 have
   printed their result, as ADR-0161 decision 5's wrapper already does.

The script prints `B` and how it was chosen (`baseline v0.1.0 = highest of: v0.1.0`) before running
cargo-semver-checks, so every log names what was compared.

## Consequences

**Good**

- The stale-literal state cannot happen: the moment `v0.2.0` is on `main`'s history, every pull
  request compares against it with no edit to `ci.yml`. The list of hand steps after a tag gets
  shorter by one.
- Zero checks is excused only in the one case where cargo-semver-checks skips by design. A
  patch-level bump that ran nothing is red.
- A tag on the wrong commit (manifest ≠ name) and a release this history does not descend from
  (`v<V>` elsewhere) are both named FAILs rather than silent baselines.
- The rule matches the tool's own default in spirit: the newest normal release.

**Bad — and accepted**

- **The baseline depends on which tags the checkout has.** A shallow or tag-less clone gets exit 2
  (rule 1), not a wrong answer. But a clone missing only the *newest* tag silently falls back to an
  older one. CI's `fetch-depth: 0` prevents that; a local run on a stale clone does not. The script
  prints `B`, so a reader can see which tag was used.
- **The script now carries a copy of cargo-semver-checks' classification.** If a future
  cargo-semver-checks changes how it classifies versions, rules 3–4 and the tool can disagree.
  Rule 3 fails loud in that case (0 checks where the copy expected checks). Rule 4 could excuse a
  case the tool no longer skips, but only when the tool also ran 0 checks, and then there was
  nothing to compare anyway. The pin to 0.50.0 in `ci.yml` bounds this until someone moves the
  pin.
- **A deliberate major bump is still unchecked on its own pull request** (rule 4), as in ADR-0161.
  That is semver-correct: a major bump is allowed to break anything.
- **Reversals that need tags cost a clone.** They must run in a `git clone` under `target/`,
  because a tag made in a worktree is shared with the real repository (*Research*, git-worktree).
- **A tag named `v<V>` on a branch that never merges back** (for example a maintenance line) makes
  rule 2 fail on `main` whenever `main`'s version equals it. The project has no maintenance
  branches. If it ever gets one, this rule is revisited then.

## Sources

The table under *Research*, read 2026-09-24. In the repository:
`scripts/check-semver-against-tag.sh` and `.github/workflows/ci.yml` job `semver` as of
`8a38fd7` (branch `plan/p3-close-8b`); `RELEASING.md` *Cutting a release*, the list of literals
that move; ADR-0161 decision 5.
