# Releasing fixbolt

The release channel is a **git tag**, not a crates.io upload
([ADR-0161](docs/decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)
decision 1, "Không publish" — the owner uses fixbolt only for their own project). *Cutting a
release* below is what "0.1.0 exists" means today, and it is a **manager** step —
`CLAUDE.md` §12 says the manager runs it, no token, no `cargo login`. *Publishing to crates.io,
if ever* is kept, unchanged in substance, for the day a new ADR supersedes decision 1; nobody
runs it under the current decision.

Six crates release in lockstep, at one version, always all six together
([ADR-0160](docs/decisions/ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md)):
`fixbolt-codec`, `fixbolt-dict`, `fixbolt-session`, `fixbolt-engine`, `fixbolt-sbe`, `fixbolt`.
`fixbolt-conformance`, `fixbolt-sbe-gen` and `tools/*` stay `publish = false` (ADR-0160
decision 2) — never run any of the steps below against them.

## Cutting a release (the tag)

### 0. Before starting: everything else is already green

Do not start unless all of these hold, on the commit about to be tagged:

- `main` is clean (`git status --short` prints nothing) and this is the commit CI is green for
  **for this exact commit** — name the CI run id before continuing (`CLAUDE.md` §9: a laptop
  says the gates pass, only CI says they pass for the commit).
- The `package` job is green on that commit: `scripts/check-release-versions.sh` (lockstep
  versions, exact-pinned internal dependencies, identical licence files — ADR-0160 decisions 1
  and 4), `cargo publish --workspace --dry-run`, six `Packaging` / six `Verifying`,
  `scripts/check-package-contents.sh`, `scripts/check-packaged-build.sh` on both the pinned
  toolchain and the declared MSRV. `scripts/stranger-check.sh --from packaged` is green.
- `CHANGELOG.md`'s `## [0.1.0]` section reads true, and `## [Unreleased]` above it is empty.

### 1. Tag the commit

```sh
git status --short   # must print nothing
git log -1 --oneline # this commit must match the CI run named in step 0
git tag -a v0.1.0 -m 'fixbolt 0.1.0'
git push origin v0.1.0
```

### 2. Read it back

```sh
git ls-remote --tags origin 'v0.1.0^{}'
```

Confirm the sha it prints is the commit named in step 0 — trust the read-back, not the push's
own success message.

### 3. Lock the tag down, if the session's token allows it

A tag, once pushed, is only as immovable as the setting behind it
(ADR-0161 decision 2). If the session's GitHub token has admin rights on the repository, create
a ruleset restricting `v*` tags (no deletion, no force-push, no update) through `gh api`, then
read it back:

```sh
gh api repos/tmthang86/fixbolt/rulesets --method POST -f name='immutable-release-tags' \
  -f target=tag -f enforcement=active \
  -f 'conditions[ref_name][include][]=refs/tags/v*' \
  -f 'rules[][type]=deletion' -f 'rules[][type]=non_fast_forward' -f 'rules[][type]=update'
gh api repos/tmthang86/fixbolt/rulesets/<id>   # confirm enforcement and the rule list
```

If the token cannot do this, tell the owner rather than skipping it silently — until it exists,
"a tag never moves" is a promise, not a setting.

**Never test the ruleset by trying to move or delete the real tag.** If it turns out to have no
effect, that attempt IS the damage to the release; the only allowed evidence is reading the
ruleset back.

### 4. GitHub release

Create a GitHub release from `CHANGELOG.md`'s `### Summary` under `## [0.1.0]` — not the full
section below it, which is one entry per change made while building toward this release and
runs long by design (`CLAUDE.md` §7). Link to the full section for anyone who wants it.

### 5. The follow-up pull request

**Every** tag, including this first one, is followed by a pull request that moves the tag
literal everywhere it is hard-coded, so the page, the CI gates and the release all name the same
one:

- `docs/GETTING-STARTED.md` and `README.md`'s install lines (`--tag v0.1.0` / `tag = "v0.1.0"`).
- `.github/workflows/ci.yml`'s `semver` job (`scripts/check-semver-against-tag.sh v0.1.0`) and
  `stranger-git` job (`scripts/stranger-check.sh --from git --tag v0.1.0`).
- `docs/CONFORMANCE.md` — the run id of this PR's own CI (`stranger-git` and `semver` both
  green against the NEW tag) and of the `push` to `main` right after it merges.

Until this PR merges, `stranger-git` and `semver` on `main` still check the OLD tag — that is
expected, not a defect, because the new tag is what this PR itself is introducing.

### 6. Tell the manager

The manager confirms `scripts/stranger-check.sh --from git --tag v0.1.0` and
`scripts/check-semver-against-tag.sh v0.1.0` are both green on the desk and in CI, and writes the
run ids into `docs/CONFORMANCE.md` and `STATUS.md`.

## Publishing to crates.io, if ever

**Needs a new ADR superseding [ADR-0161](docs/decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)
decision 1 before any step below runs.** The owner chose "Không publish": `0.1.0` is the git tag
above, not a crates.io upload, and nobody presses `cargo publish` under that decision. This
section is kept, unchanged in substance from when phase 3 was planned around a crates.io
release, because ADR-0160 kept the six crates publish-shaped on purpose (decision 6) — the day a
new ADR changes the decision, publishing is one command away, not a redesign. `CLAUDE.md` §2
non-negotiable 10 and the original ADR-0097 Q5 both said the owner presses the button, and the
manager never does (`CLAUDE.md` §12); that division of labour still holds if this section is ever
run for real.

### P1. A scoped token

Create a crates.io API token with:

- Scope: **`publish-new` and `publish-update` only** — no `yank`, no account-wide token reused
  from another project. This token cannot yank anything (step P6 needs a separate one, made only
  if that day comes).
- Name pattern: **`fixbolt*`** — the token cannot touch a crate this project does not own.
- Expiry: a few days out, not "never".

```sh
cargo login
```

Paste the token into `cargo login`'s own prompt, in your own terminal — **never into a chat, an
issue, a commit message, or anything another session or agent could read.** The token is not
pasted here or anywhere in this repository.

### P2. Dry run, on the clean checkout, no `--allow-dirty`

```sh
cargo publish --workspace --dry-run
```

Read all six `Packaging` and six `Verifying` lines and confirm no warning is new since the
`package` CI job's own dry run on this commit. **The order these print in is not the upload
order** (step P3) — the dry run packages and verifies in the order this workspace measured on
2026-09-23: codec, dict, session, engine, sbe, fixbolt
(`docs/reference/publishing-a-workspace-to-crates-io.md`) — a Packaging/Verifying line missing or
reordered from that is itself worth reading twice before continuing. **No `--allow-dirty`**: a
real publish never gets it either, and a checkout that is not already clean is not ready.

### P3. The real publish

```sh
cargo publish --workspace
```

**This upload order is topological, not the dry run's order.** Cargo waits for each crate's own
dependencies to be visible on the index before uploading it, so the real order is: `codec` first
(nothing else must wait on it); then `dict` and `sbe`, in either order (both need only `codec`);
then `session` (needs `codec` and `dict`); then `engine` (needs `session`); then `fixbolt` last
(needs all five). This can take a few minutes; let it run.

### P4. After each crate: read it back

For each of the six, once its `Uploading` line has printed:

- `https://crates.io/crates/<name>/0.1.0` shows the right licence. For `fixbolt-dict`:
  `(MIT OR Apache-2.0) AND LicenseRef-QuickFIX-1.0`. For the other five: `MIT OR Apache-2.0`.
- A few minutes later, `https://docs.rs/<name>/0.1.0` builds green.

### P5. If it breaks partway through

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

**This is still a manifest edit, and `CLAUDE.md` §8 still applies: never on `main`.** Branch,
commit the fix, open the pull request, and wait for CI to go green on it — the same gate every
other change to this repository meets — before publishing anything else:

```sh
git checkout -b fix/dict-licence-fallback
git commit -am "fix(dict): fall back to license-file, crates.io refused the LicenseRef expression"
git push -u origin fix/dict-licence-fallback
gh pr create --fill   # wait for CI green, then merge to main
git checkout main && git pull
```

Once that commit is on `main` and green, publish `fixbolt-dict` and continue **up from the
first crate that is still missing** (`cargo info <name>@0.1.0` from step P5's check above says
which that is) — never re-publish a crate that already landed. All of this is still `0.1.0`:
nothing that reached this point has been uploaded as anything else yet.

### P6. If something already published is wrong

The P1 token cannot do this — **make a second, `yank`-scoped token first, only now that it is
actually needed** (same `fixbolt*` name pattern, same short expiry), and `cargo login` with it.

**Yank all six.** Unlike deletion below, crates.io does **not** refuse to yank a crate that
something else still depends on — yanking only stops a *new* `Cargo.lock` from selecting that
version; it does not touch a lockfile that has already resolved to it, and it deletes nothing.
Order therefore does not protect a partially-yanked family the way it does for deletion: because
every internal dependency is pinned exactly to `=0.1.0` (ADR-0160 decision 1), yanking even one
of the six already makes a fresh `cargo add fixbolt` fail to resolve. Yank `fixbolt` first anyway
— it is the common entry point, so a stranger typing `cargo add fixbolt` sees the failure at
once — then the five underneath, in whatever order is convenient:

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

### P7. Tell the manager

Report back with `cargo publish`'s own output: which crates uploaded, in what order, and
whether step P5's partial-failure path was needed. The manager then runs
`scripts/stranger-check.sh --from registry --version 0.1.0` as an ADDITIONAL check beside
`--from git --tag v0.1.0` — the git tag stays the release channel (decision 1's replacement ADR
decides otherwise, not this publish by itself) — and updates `docs/CONFORMANCE.md` with both run
ids. Nobody runs `cargo publish` again for this version once it succeeds.
