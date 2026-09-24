# ADR-0161 — 0.1.0 is a git tag, not a crates.io upload, and the stranger and the semver gate read the tag

- **Status**: **Accepted — 2026-09-24, by the manager under the owner's decision in *Context* ("Không publish").** Proposed the same day. Written by the architect (Opus) from the owner's decision
  recorded verbatim in *Context*. The manager accepts it under that decision; the owner may refuse
  it. On acceptance it **supersedes
  [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  Q5** (*the owner runs `cargo publish`*) **and ADR-0097 decision 7's exit criteria 7 and 8** (both
  read from crates.io), and **ADR-0160 decision 6's second sentence and decision 7's "after the
  publish it compares against the registry"** — the two places ADR-0160 said what happens after an
  upload. Everything else in ADR-0097 and ADR-0160 stands, in particular ADR-0160 decisions 1–5
  and 8: the six crates stay publish-shaped.
- **Date**: 2026-09-24
- **Deciders**: Tran Manh Thang (chose "Không publish"). Written by the architect (Opus); accepted
  by the manager under that decision.
- **Related**: [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  decisions 3 and 7, Q3, Q5, Q8;
  [ADR-0160](ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md)
  (lockstep, packaged-sources stranger, semver job; decision 2 already sends SBE users to a git
  tag); [ADR-0104](ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)
  (the dictionary a git consumer builds is the one in `crates/dict/spec/`, no `vendor/`);
  [plan p3-packaging, *Sửa 2*](../plans/2026-09-23-p3-packaging-and-first-release.md);
  `RELEASING.md`; `scripts/stranger-check.sh`; `.github/workflows/ci.yml` jobs `package`, `semver`.

## Context

**The owner's decision, in conversation, 2026-09-24**, as the manager recorded it (it exists
nowhere else):

> The owner uses fixbolt only for their own project and chose **"Không publish"**: do not publish
> 0.1.0 to crates.io. Tag `v0.1.0` on a CI-green commit; a new ADR replaces ADR-0097 Q5 (owner
> presses `cargo publish`) and ADR-0097's registry-based exit criteria 7 and 8; the stranger check
> and the semver gate compare against the git tag. fixbolt is consumed as a git dependency
> (`fixbolt = { git = "https://github.com/tmthang86/fixbolt", tag = "v0.1.0" }`). The repo is
> PUBLIC on GitHub. The six crates stay publish-shaped (ADR-0160 unchanged) so publishing later
> stays a one-command option.

What this changes: phase 3 was built "to the owner's `cargo publish`" (`STATUS.md` *Start here —
2026-09-24*). Two of ADR-0097's eight exit criteria read from crates.io and so could never close
without an upload: 7 (`cargo add fixbolt@0.1.0` from the registry) and 8
(`cargo semver-checks --baseline-version 0.1.0`, a registry lookup). Every other criterion, and
ADR-0160's `package` job (`cargo publish --workspace --dry-run`, the packaged-sources build, the
`--from packaged` stranger), is independent of an upload and stays.

What was measured for this ADR, 2026-09-24, on the desk (`tmt-B450-I-AORUS-PRO-WIFI`, cargo
1.98.0, cargo-semver-checks 0.50.0), from a scratch crate outside the repository and from the
worktree of `main` `094bfc3` with no `vendor/`:

1. **A named package is found anywhere in the repository.** `cargo add --git
   https://github.com/tmthang86/fixbolt --rev 094bfc3` with **no name** fails: `error: multiple
   packages found at …: fixbolt, fixbolt-attr-scan, fixbolt-codec, … fixbolt-w2w` (15 names — every
   workspace member, `publish = false` or not) `To disambiguate, run cargo add --git … <package>`.
   With the name `fixbolt`: `Updating git repository` `https://github.com/tmthang86/fixbolt`,
   `Adding fixbolt (git) to dependencies`, features `+ standard - sbe`, and the manifest line
   `fixbolt = { git = "https://github.com/tmthang86/fixbolt", rev = "094bfc3", version = "0.1.0" }`
   — cargo adds the `version` itself.
2. **The lockfile pins the full commit.** `Cargo.lock`: `source =
   "git+https://github.com/tmthang86/fixbolt?rev=094bfc3#094bfc3e159a294341f6af65e547f47ab3841e0b"`;
   five packages carry a `git+` source (the four fixbolt crates `fixbolt` needs by default, and
   `fixbolt`).
3. **It builds from GitHub with no `vendor/`.** `cargo build` in that scratch crate:
   `Compiling fixbolt-codec v0.1.0 (https://github.com/tmthang86/fixbolt?rev=094bfc3#094bfc3e)` …
   `Compiling fixbolt v0.1.0 (https://github.com/tmthang86/fixbolt?rev=094bfc3#094bfc3e)`,
   `Finished`, exit 0. The parenthesis is the source: a path dependency prints a filesystem path,
   a registry dependency prints none. The clone costs 12 MB (`~/.cargo/git/db`) plus 17 MB
   (checkout).
4. **A missing tag is a named cargo error.** `cargo add --git https://github.com/tmthang86/fixbolt
   --tag v9.9.9 fixbolt` exits 101: `Updating git repository` …, `error: failed to load source for
   dependency 'fixbolt'`, `unable to update https://github.com/tmthang86/fixbolt?tag=v9.9.9`,
   `failed to find tag 'v9.9.9'` (cargo prints backticks where this line shows single quotes).
5. **cargo-semver-checks against a git revision works with no `vendor/`, and an unchanged
   `0.1.0` runs every lint.** `cargo semver-checks --workspace --baseline-rev HEAD` prints, for each
   of the six published crates and no other, `Checking <crate> v0.1.0 -> v0.1.0 (no change; assume
   minor)`, `Checked [...] 196 checks: 196 pass, 58 skip`, `Summary no semver update required`;
   exit 0. `--baseline-rev v9.9.9` exits 101: `error: couldn't parse revision: "v9.9.9^{tree}"`,
   `The ref partially named "v9.9.9" could not be found`.
6. **A major-level bump runs no lint and exits 0.** Read in cargo-semver-checks 0.50.0
   `src/check_release.rs` (`classify_minimum_semver_version_change`): identical `0.y.z` versions →
   minimum change *minor*; `0.y` → `0.(y+1)` → *major*. The `semver` job's own comment records the
   measured consequence on 0.0.0 → 0.1.0: `0 checks: 0 pass, 254 skip` per crate. A green exit
   from this tool is therefore not evidence that anything was compared.

## Research

| Source | What it says | Bearing |
|---|---|---|
| <https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html> (*Specifying dependencies from git repositories*) | "Cargo fetches the git repository at that location and traverses the file tree to find Cargo.toml file for the requested crate anywhere inside the git repository" (example: `regex-lite` and `regex-syntax` from `rust-lang/regex`); `branch`, `tag`, `rev` select the commit, none means the default branch's latest; "Cargo locks the commits of git dependencies in Cargo.lock file at the time of their addition and checks for updates only when you run cargo update"; "crates.io does not allow packages to be published with dependencies on code published outside of crates.io itself" (dev-dependencies excepted) | How `fixbolt` resolves inside a 15-member repository (measured, fact 1); a `tag` is a name resolved at lock time, the lockfile holds the sha; **a stranger's own crate that depends on fixbolt by git cannot be published to crates.io** |
| <https://doc.rust-lang.org/cargo/commands/cargo-add.html> | `--git <url>`, `--tag <tag>`, `--rev <sha>`, `--branch <branch>`; synopsis `cargo add [options] --git url [crate…]` | The page does not say how a multi-package repository is disambiguated; fact 1 measured it |
| <https://github.com/obi1kenobi/cargo-semver-checks> (README) | Default baseline is crates.io; `--baseline-rev <REV>` "Git revision to lookup for a baseline" — "will walk up the current directory until it finds a .git/ directory to resolve the revision and extract the corresponding worktree"; `--baseline-root`, `--baseline-rustdoc`; "unpublished crates should use alternative baseline approaches like git revisions or local directories" | `--baseline-rev v0.1.0` is the documented route for a crate never uploaded; it needs the tag in the local clone |
| <https://raw.githubusercontent.com/obi1kenobi/cargo-semver-checks/v0.50.0/src/check_release.rs> | `classify_minimum_semver_version_change`: equal versions → `get_minimum_version_change` (`(0, _)` → Minor); a `0.y` minor change → Major; test `classify_zerover_same_version` asserts 0.1.0 vs 0.1.0 → Minor | Fact 6: an unchanged version is checked in full, a `0.(y+1)` bump skips everything |
| <https://github.com/LTplus-AG/ifc-lite/issues/4786> | "Rust crate semver gate passes on zero executed checks whenever the release carries a major" | A sibling project paid for fact 6; the gate must assert that checks ran |
| <https://github.com/actions/checkout> | `fetch-depth: 0` — "all history for all branches and tags" | The `semver` job already sets it, so `v0.1.0` is in the runner's clone |
| <https://rust-lang.github.io/rfcs/3463-crates-io-policy-update.html> (crates.io usage policy, adopted 2023-11-07) | "crates.io has a first-come, first-serve policy on crate names"; content that "exists only to reserve a name for a prolonged period of time (often called 'name squatting') without having any genuine functionality" is not allowed; transfers go through the current owner, the team only mediates when the owner is unreachable | **Nothing reserves `fixbolt` for this project**, and a placeholder upload to hold it would itself break the policy |
| <https://docs.github.com/en/enterprise-server@3.18/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets> | Tag rulesets can restrict deletions and block force pushes / updates on matching tags | A `v*` ruleset is what makes a tag behave like a release that cannot move |
| <https://doc.rust-lang.org/cargo/reference/publishing.html> (via ADR-0160 *Research*) | A publish is permanent; yank only | What "not publishing" avoids, and what it gives up (yank as a signal) |

**Searched, found nothing:** a cargo mechanism that refuses a git `tag` whose commit moved since
the lockfile was written (cargo re-resolves only on `cargo update`; nothing warns); a docs.rs
equivalent for git-only crates (docs.rs builds only crates.io uploads).

## Decision

1. **0.1.0 is not uploaded to crates.io.** The release is the annotated tag `v0.1.0`, pushed by
   the **manager** (`git tag -a v0.1.0 -m 'fixbolt 0.1.0'`, `git push origin v0.1.0`) on a `main`
   commit whose CI run is green and named by id. No token, no `cargo login`, no owner step.
   ADR-0097 Q5 is superseded; `STATUS.md`'s *Do not* "run `cargo publish` from an agent" stays
   true and becomes "run `cargo publish` at all, without a new ADR".
2. **A tag, once pushed, never moves and is never deleted.** A later fix is `v0.1.1`, never a
   re-pointed `v0.1.0`. A repository tag ruleset on `v*` (restrict deletions, block updates and
   force pushes) makes that a setting rather than a promise — set by the manager through the
   GitHub API if the session's token has admin rights, otherwise by the owner (*Consequences*).
3. **How a stranger depends on it**, in `README.md` and `docs/GETTING-STARTED.md`:
   `cargo add fixbolt --git https://github.com/tmthang86/fixbolt --tag v0.1.0`, i.e.
   `fixbolt = { git = "https://github.com/tmthang86/fixbolt", tag = "v0.1.0" }`. The package name is
   required (fact 1). A user who wants the sha in the manifest, not only the lockfile, writes
   `rev = "<sha of v0.1.0>"`; both resolve to the same commit while decision 2 holds.
4. **ADR-0097 exit criterion 7 becomes**: `scripts/stranger-check.sh --from git --tag v0.1.0` exits
   0 — a scratch crate outside the tree, `cargo add fixbolt --git <url> --tag v0.1.0`,
   `docs/GETTING-STARTED.md`'s marked code pasted verbatim, one Logon/Logout through
   `scripts/stranger-logon.py`, and **proof it came from GitHub**: the build log's
   `Compiling fixbolt v0.1.0 (https://github.com/tmthang86/fixbolt?tag=v0.1.0#<sha8>)` line and the
   scratch `Cargo.lock`'s `source = "git+https://github.com/tmthang86/fixbolt?tag=v0.1.0#<sha40>"`,
   with `<sha40>` equal to `git rev-parse v0.1.0^{commit}` in the checkout. The script also fails
   if `docs/GETTING-STARTED.md`'s install line does not name the same tag — the page and the gate
   must speak of one release. It runs as a **blocking** CI job on `pull_request` and on `push` to
   `main`, from the first pull request after the tag exists.
5. **ADR-0097 exit criterion 8 becomes**: `cargo semver-checks --workspace --baseline-rev v0.1.0`,
   **blocking**, through a wrapper that fails unless each of the six published crates printed a
   `Checked … N checks` line with N > 0 (fact 6). The one exception the wrapper allows: when the
   workspace version differs from the tag's version (a deliberate `0.2.0` bump), it passes and
   prints that the skip is by design. The baseline literal moves to the new tag in the pull request
   that follows each new tag, together with the page's install line (decision 4).
6. **ADR-0160 stays.** Six crates, lockstep, `=` pins, `include`, licence copies, `rust-version`,
   docs.rs metadata; the `package` job, `cargo publish --workspace --dry-run` and
   `stranger-check.sh --from packaged` keep running on every pull request. `--from registry` stays
   in the script, unused. **Publishing later is still one command** —
   `cargo publish --workspace` from a tagged commit, per `RELEASING.md` — and needs only a new ADR
   superseding decision 1 of this one.
7. **`RELEASING.md` is reordered, not deleted**: "Cutting a release" (tag steps, CI-green commit,
   ruleset, the follow-up pull request that moves the tag literal) comes first; the crates.io
   sequence follows as "Publishing to crates.io, if ever", unchanged in substance.

## Consequences

**Good**

- Phase 3 closes without an irreversible act and without the owner at the keyboard; no crates.io
  token ever exists.
- The stranger check exercises the channel a consumer really uses — GitHub, the tag, the full
  tree — and proves the source by the line cargo prints, not by the exit status.
- The semver gate is blocking from the first pull request after the tag, and asserts that it
  compared something; the fact-6 hole would otherwise have made it silently green at the first
  `0.2.0`.
- Nothing in ADR-0160 is wasted: the `.crate` bytes stay CI-proven, so a later publish uploads what
  a CI run already built.

**Bad — and accepted**

- **No docs.rs.** API documentation is `cargo doc --open` on the consumer's machine. The
  `[package.metadata.docs.rs]` tables and the `--cfg docsrs` simulation keep costing CI minutes for
  a site that is not built.
- **A stranger must know the repository.** No crates.io search, no `cargo add fixbolt`, no
  download counts, no reverse-dependency listing. Discoverability is the GitHub URL.
- **A crate that depends on fixbolt by git cannot be published to crates.io** (Cargo book). Any
  downstream library is stuck in git too until fixbolt is uploaded — a cost borne by users, not by
  this repository.
- **The name is not reserved.** crates.io is first-come, first-served; `fixbolt` and the five
  `fixbolt-*` names were free on 2026-09-23 (ADR-0160 fact 6) and anyone may take them. A
  placeholder upload to hold them would be name squatting under crates.io's own policy, so it is
  not an option. If the names are taken before a later publish, that publish needs a rename — a
  break for every git consumer's dependency key (the `package = "…"` key softens it, it does not
  remove it).
- **A tag is only as immutable as the setting behind it.** Without the ruleset of decision 2, a
  force-pushed tag changes what a fresh `cargo update` resolves; cargo does not warn (*Research*,
  found nothing). A consumer's existing `Cargo.lock` keeps the old sha.
- **No yank.** A broken release cannot be withdrawn from resolution; the remedy is `v0.1.1` and a
  note in `CHANGELOG.md` and the GitHub release.
- **The consumer builds the whole tree, not the `include` allowlist.** A git dependency clones
  history (29 MB measured) and builds from the repository layout; `--from packaged` proves the
  `.crate` bytes, `--from git` proves the tree — two different sets of bytes, both gated.
- **GitHub is now a build dependency** of every consumer and of the `stranger-git` CI job; a GitHub
  outage turns that job red with nothing wrong in the code. The repository must stay public:
  making it private breaks every consumer at their next clean build.
- **The tree at `v0.1.0` carries the pre-ADR-0161 install note.** The tag goes on a commit before
  the documentation pull request (plan *Sửa 2*: the gates need the tag to exist), so the tagged
  `GETTING-STARTED.md` still says "not on crates.io yet, depend on a `rev`", and its `CHANGELOG.md`
  has no date. Strangers read `main`; the GitHub release notes are written from `main`'s
  `CHANGELOG.md`.
- **The page is pinned to the release.** Because decision 4 checks `main`'s page against the tag
  the page names, a pull request that makes the page use API not in `v0.1.0` goes red until the
  next tag. That is the intended strictness; it also means new API cannot be shown on the
  getting-started page between releases.
- **Still unproven, and now unprovable without a publish:** crates.io accepting
  `LicenseRef-QuickFIX-1.0` (ADR-0160 decision 5). The `STATUS.md` bullet stays, reworded to "only
  if published".
- ADR-0097 decision 3's `1.0` condition (one outside deployment reported publicly, one minor
  release with no semver-check exemption) still reads correctly against tags; nothing changes
  there.

## Sources

The table under *Research*, all read 2026-09-24. Measurements in *Context*: a scratch crate in the
architect's scratchpad (cargo 1.98.0) against `https://github.com/tmthang86/fixbolt` at `094bfc3`,
and `cargo semver-checks` 0.50.0 run in the worktree `/home/tmt/Projects/fb-p3tag` (branch
`plan/p3-close-by-tag`, `094bfc3`, no `vendor/`). Repository visibility: `gh repo view
tmthang86/fixbolt` → `"visibility":"PUBLIC"`. In-repository: ADR-0097 *Decision* 7 and Q5;
ADR-0160 *Decision* 6–7; `.github/workflows/ci.yml` jobs `package` and `semver`;
`scripts/stranger-check.sh` lines 1–100 and 247–287.
