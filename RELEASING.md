# Releasing fixbolt

The exact command sequence for the owner to run `cargo publish`. Nobody else runs these steps:
`CLAUDE.md` §2 non-negotiable 10 and ADR-0097 Q5 both say the owner presses the button, and the
manager never does (`CLAUDE.md` §12). This file is what "presses the button" means, in order.

Six crates release in lockstep, at one version, always all six together
([ADR-0160](docs/decisions/ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md)):
`fixbolt-codec`, `fixbolt-dict`, `fixbolt-session`, `fixbolt-engine`, `fixbolt-sbe`, `fixbolt`.
`fixbolt-conformance`, `fixbolt-sbe-gen` and `tools/*` stay `publish = false` (ADR-0160
decision 2) — never run these steps against them.

## 0. Before starting: everything else is already green

Do not start unless all of these hold, on the commit about to be released:

- `main` is clean (`git status --short` prints nothing) and this is the commit CI is green for
  — name the CI run id before continuing.
- The `package` job is green on that commit: `cargo publish --workspace --dry-run`, six
  `Packaging` / six `Verifying`, `scripts/check-package-contents.sh`,
  `scripts/check-packaged-build.sh` on both the pinned toolchain and `+1.88.0`.
  `scripts/stranger-check.sh --from packaged` is green — it built the exact bytes this release
  uploads and drove a Logon/Logout through `docs/GETTING-STARTED.md`'s own pasted code.
  ADR-0097 exit criteria 1–6 are all met.
- `CHANGELOG.md`'s `## [0.1.0]` section reads true, and `## [Unreleased]` above it is empty.

## 1. A clean checkout, at the right commit

```sh
git status --short   # must print nothing
git log -1 --oneline # this commit must match the CI run named in step 0
```

Optional but recommended: do the actual publish from a fresh clone rather than a long-lived
working tree, so nothing local (an untracked file, a stray `target/`) can leak in even under
`--allow-dirty` — which step 3 never passes anyway.

## 2. A scoped token

Create a crates.io API token with:

- Scope: **`publish-new` and `publish-update` only** — no `yank`, no account-wide token reused
  from another project.
- Name pattern: **`fixbolt*`** — the token cannot touch a crate this project does not own.
- Expiry: a few days out, not "never".

```sh
cargo login
```

Paste the token into `cargo login`'s own prompt, in your own terminal — **never into a chat, an
issue, a commit message, or anything another session or agent could read.** The token is not
pasted here or anywhere in this repository.

## 3. Dry run, on the clean checkout, no `--allow-dirty`

```sh
cargo publish --workspace --dry-run
```

Read all six `Packaging` and six `Verifying` lines — codec, dict, sbe, session, engine, fixbolt,
in that dependency order — and confirm no warning is new since the `package` CI job's own dry
run on this commit. **No `--allow-dirty`**: a real publish never gets it either, and a checkout
that is not already clean is not ready.

## 4. The real publish

```sh
cargo publish --workspace
```

Cargo publishes in dependency order on its own — codec → dict, sbe → session → engine →
fixbolt — waiting for the index to pick up each crate before uploading the one that depends on
it. This can take a few minutes; let it run.

## 5. After each crate: read it back

For each of the six, once its `Uploading` line has printed:

- `https://crates.io/crates/<name>/0.1.0` shows the right licence. For `fixbolt-dict`:
  `(MIT OR Apache-2.0) AND LicenseRef-QuickFIX-1.0`. For the other five: `MIT OR Apache-2.0`.
- A few minutes later, `https://docs.rs/<name>/0.1.0` builds green.

## 6. If it breaks partway through

Publishing six crates is **not atomic**. If cargo stops before all six are up:

```sh
cargo info <name>@0.1.0   # which of the six actually landed?
```

Fix the cause, then publish only what is still missing:

```sh
cargo publish -p <name-still-missing>   # repeat up the dependency order
```

**If crates.io refuses `fixbolt-dict`'s licence expression** (the `AND LicenseRef-QuickFIX-1.0`
combination): switch to the documented fallback (ADR-0104 decision 6) —

```toml
# crates/dict/Cargo.toml
license-file = "NOTICE"   # instead of license = "(MIT OR Apache-2.0) AND LicenseRef-QuickFIX-1.0"
```

commit, then publish `fixbolt-dict` and everything above it again. All of this is still `0.1.0`
— nothing that reached this point has been uploaded as anything else yet.

## 7. Tag and release

```sh
git tag -a v0.1.0 -m 'fixbolt 0.1.0'
git push origin v0.1.0
```

Create a GitHub release from the `## [0.1.0]` section of `CHANGELOG.md`.

## 8. If something already published is wrong

**Yank**, starting from the crate nothing else depends on downward — `fixbolt` first, then
`fixbolt-engine`, `fixbolt-sbe`, `fixbolt-session`, `fixbolt-dict`, `fixbolt-codec` last (the
reverse of the dependency order step 4 uploaded in, because a dependency cannot be pulled out
from under something that still resolves to it):

```sh
cargo yank --version 0.1.0 fixbolt
cargo yank --version 0.1.0 fixbolt-engine
cargo yank --version 0.1.0 fixbolt-sbe
cargo yank --version 0.1.0 fixbolt-session
cargo yank --version 0.1.0 fixbolt-dict
cargo yank --version 0.1.0 fixbolt-codec
```

`--undo` reverses a yank (`cargo yank --undo --version 0.1.0 <name>`).

**Deleting a crate outright** is possible only within the first 72 hours of that publish (or
later, with one owner and few downloads), through each crate's own *Settings* page on crates.io
— never through `cargo` — and only when no other crate on the registry still depends on it. That
means the **same reverse order** as yanking: `fixbolt` first, `fixbolt-codec` last, because
crates.io refuses to delete a crate something else still resolves to.

**If a secret leaked** (the token itself, or anything else): yanking or deleting fixes nothing
about that — rotate the secret immediately, separately from anything in this file.

## 9. Tell the manager

Report back with `cargo publish`'s own output: which crates uploaded, in what order, and
whether step 6's partial-failure path was needed. The manager runs everything after this point
(the post-publish PR: `scripts/stranger-check.sh --from registry --version 0.1.0`, the
`cargo-semver-checks` job dropping `continue-on-error`, `docs/CONFORMANCE.md`) — nobody runs
`cargo publish` again for this version once it succeeds.
