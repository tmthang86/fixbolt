# ADR-0182 — The SQLite store is born release-shaped behind a default feature, and joins the tagged release family only when its kill line passes

- **Status**: **Accepted — 2026-09-24, by the manager under the owner's delegation of
  2026-09-18.** Proposed 2026-09-24, revised in place the same day (see *Revision 1*), the
  revision folded into the accepted text. Decision 3 (joining the tagged release family) is
  not yet applied — that is row 4 of the plan below, in a later pull request.
  Written by the architect (Opus) for row 3 of
  [docs/plans/2026-09-23-phase-4-scope.md](../plans/2026-09-23-phase-4-scope.md), planned in
  [docs/plans/2026-09-24-p4-sqlite-store.md](../plans/2026-09-24-p4-sqlite-store.md). On the day
  decision 3 is applied (the kill line passed), it **supersedes the count "six" in
  [ADR-0160](ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md)
  decision 1**: the family of crates that share the workspace version and are released together
  becomes seven. Every other part of ADR-0160 that ADR-0161 keeps applies to the new member
  unchanged.
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: ADR-0160 decisions 1–8; ADR-0161 (being written 2026-09-24: phase 3 closes with
  the git tag `v0.1.0`, and **no crates.io publish is planned**);
  [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  item 4 (*"Otherwise it stays unpublished"*) and owner answer Q1;
  [ADR-0180](ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md);
  `CLAUDE.md` §2 item 6, §6 *Dependencies*;
  [feature-flags-unify-across-a-workspace](../reference/feature-flags-unify-across-a-workspace.md).

## Revision 1 — 2026-09-24

The owner decided on 2026-09-24 that **nothing is published to crates.io**: phase 3 closes with
the git tag `v0.1.0` (ADR-0161). The first draft of this ADR had the store *"join the lockstep
release"* and flip `publish = false` so that `cargo publish` would upload it. That target no
longer exists. What changed:

- *Joins the lockstep release* → **joins the tagged release family**: the crates that inherit
  the workspace version, pin each other exactly, are checked by ADR-0160's scripts and CI jobs,
  and are released together under one git tag.
- `publish = false` stays the marker that keeps a crate **out of that family** (it is what
  `cargo publish --workspace --dry-run` in the `package` job and rule 4 of
  `check-release-versions.sh` read), not a guard against an upload that is planned.
- Every sentence about crates.io (the version the store first appears at on the registry,
  *"publish now and yank if killed"*, *"harmless because nothing is published"*) is rewritten
  for tags.
- MSRV stated as the workspace's `rust-version = "1.89"` throughout.

Decisions 1, 2 (feature gate, shape), 4 and 5 are unchanged in substance.

## Context

1. **Non-negotiable 6**: a feature flag gates the `mod` declaration itself, `build.rs` invokes
   no external toolchain unless that feature is on, and CI builds `--no-default-features` *"on
   a machine with nothing optional installed"*. `rusqlite`'s `bundled` feature makes
   `libsqlite3-sys`'s build script compile SQLite's C source with `cc` (rusqlite README). A
   workspace member that depends on it unconditionally would put a C compile into `cargo test
   --all --no-default-features` — the CI job whose whole purpose is to see nothing optional.
2. **ADR-0160** keeps six crates in lockstep: one workspace version, `=` pins on internal
   normal dependencies, an `include` allowlist, licence copies per crate, a declared
   `rust-version` the `package` CI job builds the packaged sources on, docs.rs feature lists.
   Scripts hard-code the six (`scripts/check-release-versions.sh` `PUBLISHED`,
   `scripts/check-packaged-build.sh` and `scripts/check-package-contents.sh` `PUBLISHED=(…)`),
   and rule 4 of `check-release-versions.sh` requires every other member to be
   `publish = false`. ADR-0161 releases that family by git tag instead of by `cargo publish`.
3. **ADR-0098 item 4's kill line** says a store that misses it *stays unpublished* — read under
   ADR-0161 as *stays out of the released family*; the owner's Q1 makes a killed item a
   completed one.
4. `[measured 2026-09-24]` rusqlite 0.40.2 declares no `rust-version`; its README's MSRV policy
   is *"Latest stable Rust version at the time of release"*. A scratch crate with
   `rusqlite = { version = "=0.40.2", default-features = false, features = ["bundled"] }`
   builds on Rust 1.88.0 and 1.98.0 (desk, `cargo +<toolchain> build`). **The workspace MSRV is
   `rust-version = "1.89"`** (`Cargo.toml` `[workspace.package]`, raised by ADR-0154); 1.89.0
   itself was not installed on the desk and was not tried.
5. `[measured 2026-09-24]` its normal/build dependency tree with those features:
   `bitflags`, `fallible-iterator`, `fallible-streaming-iterator`, `smallvec`,
   `libsqlite3-sys` (build: `cc`, `find-msvc-tools`, `shlex`, `pkg-config`, `vcpkg`) — no async
   runtime, nothing that needs an ADR under `CLAUDE.md` §6.

## Decision

1. **One default feature, `sqlite`, gates the crate's `mod` declarations and the dependency.**
   `sqlite = ["dep:rusqlite"]`, `default = ["sqlite"]`, `rusqlite = { version = "0.40",
   default-features = false, features = ["bundled"], optional = true }`. With
   `--no-default-features` the crate compiles to an empty library and **no C is compiled**; its
   tests and benches carry `#![cfg(feature = "sqlite")]`. `scripts/check-no-optional-deps.sh`
   gains `fixbolt-store-sqlite:rusqlite` and `fixbolt-store-sqlite:libsqlite3-sys`. No second
   feature (a system `libsqlite3`, SQLCipher): each would need a library on the CI runner that
   `cargo hack --feature-powerset` builds, and nobody has asked.
2. **Born release-shaped, outside the family.** From its first commit the crate has everything
   ADR-0160 demands of a family member — `version.workspace = true` (so `rust-version = "1.89"`
   is inherited), `=` pins on `fixbolt-engine` and `fixbolt-session`, `description`,
   `keywords`, `categories`, `readme`, the `include` allowlist (`src/**`, `README.md`,
   `LICENSE-MIT`, `LICENSE-APACHE`), the two licence copies byte-identical to the root,
   `[package.metadata.docs.rs] features = ["sqlite"]`, `#![cfg_attr(docsrs, feature(doc_cfg))]`
   — **and `publish = false`**, so `cargo publish --workspace --dry-run`, `cargo semver-checks`
   and the three scripts of Context 2 leave it out while it is on trial.
3. **It joins the tagged release family on the commit that records its kill line passed** (row 4
   of the scope plan): `publish = false` removed; the name added to `PUBLISHED` in
   `check-release-versions.sh`, `check-packaged-build.sh` (with a `sqlite` build case on 1.98.0
   and on the declared `rust-version`, 1.89) and `check-package-contents.sh`; ADR-0160's *six*
   becomes *seven* by this ADR. It is then released with the other six under the next git tag
   (ADR-0161) at the **same** workspace version — `v0.1.0` is cut before this crate lands, so
   its first tag is the next one — never at a version of its own. **No crates.io publish is
   planned** (ADR-0161); if one is ever decided, the crate is already in the family that
   decision would upload.
4. **If the kill line fails**, the crate stays in the workspace with `publish = false`, outside
   the family, the failing pair goes to `docs/reference/measured-costs.md`, and ADR-0180 is marked
   with the result — ADR-0098 item 4's *"stays unpublished"*, not a removal.
5. **Not in the facade.** `fixbolt` gains no `sqlite` feature; a user adds
   `fixbolt-store-sqlite` beside it. Reversible later without a break.

## Options not taken

- **Leave `rusqlite` unconditional and exclude the crate from the `no-default-features` job.**
  The job's claim would then be *"builds with nothing optional installed, except where it
  doesn't"*, and every other `--workspace` build would still compile C. Rejected.
- **Keep the crate out of the workspace** (like `fuzz/`). It would escape `cargo test --all`,
  clippy, the lint ratchets and the semver gate — the gates a released crate must pass.
  Rejected.
- **A version of its own** (e.g. `0.0.x` while on trial). It pins `fixbolt-engine` exactly, so
  every engine release would need a store release anyway; separate numbering only adds a matrix
  nobody builds. Rejected.
- **Put it in the family now and take it out if killed.** A tag is as permanent as the history
  it names: a crate released under a tag and then dropped leaves a release that shipped
  something its own kill line refused. The kill line exists to be applied before that.
  Rejected.

## Consequences

**Good**

- `--no-default-features` still means *nothing optional*, for the new crate as for the engine.
- Joining the family is a few lines and a script list, not a packaging project: the shape is
  proven by every CI run from the first commit.

**Bad — and accepted**

- **Every default workspace build now compiles SQLite's C** (`cargo test --all`, clippy, the
  `cargo hack` powerset, the bench job): tens of seconds per CI job, and a C compiler on every
  runner and developer machine that builds the workspace with defaults. The
  `no-default-features` job is the one that must not need it.
- **A crate whose `--no-default-features` build is empty** is a surprise to a user who turns
  defaults off by habit. Its rustdoc and `README.md` say so in the first line.
- **MSRV is inherited from a dependency that promises none.** The workspace declares 1.89; a
  `cargo update` within rusqlite 0.40 can raise what the crate really needs, silently. Only the
  `package` job's build on the declared `rust-version` catches it, and only once the crate is in
  that job (decision 3). Until then an MSRV break is invisible — and harmless, because the crate
  is in no tagged release.
- **One more hard-coded list entry in three scripts** when it joins — the lists are hard-coded
  on purpose (`check-release-versions.sh` lines 32–35) and this is their price.
