# ADR-0182 — The SQLite store is born publish-shaped behind a default feature, and joins the lockstep release only when its kill line passes

- **Status**: **Proposed — 2026-09-24.** Written by the architect (Opus) for row 3 of
  [docs/plans/2026-09-23-phase-4-scope.md](../plans/2026-09-23-phase-4-scope.md), planned in
  [docs/plans/2026-09-24-p4-sqlite-store.md](../plans/2026-09-24-p4-sqlite-store.md). On the day
  decision 3 is applied (the kill line passed), it **supersedes the count "six" in
  [ADR-0160](ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md)
  decision 1** — the lockstep family becomes seven; every other part of ADR-0160 stands and
  applies to the new member unchanged.
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner. Pressing `cargo publish` stays the owner's (ADR-0097 Q5).
- **Related**: ADR-0160 decisions 1–8; [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  item 4 (*"Otherwise it stays unpublished"*) and owner answer Q1;
  [ADR-0180](ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md);
  `CLAUDE.md` §2 item 6, §6 *Dependencies*;
  [feature-flags-unify-across-a-workspace](../reference/feature-flags-unify-across-a-workspace.md).

## Context

1. **Non-negotiable 6**: a feature flag gates the `mod` declaration itself, `build.rs` invokes
   no external toolchain unless that feature is on, and CI builds `--no-default-features` *"on
   a machine with nothing optional installed"*. `rusqlite`'s `bundled` feature makes
   `libsqlite3-sys`'s build script compile SQLite's C source with `cc` (rusqlite README). A
   workspace member that depends on it unconditionally would put a C compile into `cargo test
   --all --no-default-features` — the CI job whose whole purpose is to see nothing optional.
2. **ADR-0160** publishes six crates in lockstep: one workspace version, `=` pins on internal
   normal dependencies, an `include` allowlist, licence copies per crate, a declared
   `rust-version` the `package` CI job builds on, docs.rs feature lists. Scripts hard-code the
   six (`scripts/check-release-versions.sh` `PUBLISHED`, `scripts/check-packaged-build.sh` and
   `scripts/check-package-contents.sh` `PUBLISHED=(…)`), and rule 4 of
   `check-release-versions.sh` requires every other member to be `publish = false`.
3. **ADR-0098 item 4's kill line** says a store that misses it *stays unpublished*; the owner's
   Q1 makes a killed item a completed one.
4. `[measured 2026-09-24]` rusqlite 0.40.2 declares no `rust-version`; its README's MSRV policy
   is *"Latest stable Rust version at the time of release"*. A scratch crate with
   `rusqlite = { version = "=0.40.2", default-features = false, features = ["bundled"] }`
   builds on Rust 1.88.0 and 1.98.0 (desk, `cargo +<toolchain> build`). The workspace declares
   `rust-version = "1.89"`.
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
2. **Born publish-shaped, not published.** From its first commit the crate has everything
   ADR-0160 demands of a published member — `version.workspace = true`, `=` pins on
   `fixbolt-engine` and `fixbolt-session`, `description`, `keywords`, `categories`,
   `readme`, the `include` allowlist (`src/**`, `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`),
   the two licence copies byte-identical to the root, `[package.metadata.docs.rs] features =
   ["sqlite"]`, `#![cfg_attr(docsrs, feature(doc_cfg))]` — **and `publish = false`**, so
   `cargo publish --workspace`, `cargo semver-checks` and the three scripts of Context 2 leave it
   out while it is on trial.
3. **It joins the lockstep family on the commit that records its kill line passed** (row 4 of the
   scope plan): `publish = false` removed; the name added to `PUBLISHED` in
   `check-release-versions.sh`, `check-packaged-build.sh` (with a `sqlite` build case on
   1.98.0 and on the declared `rust-version`) and `check-package-contents.sh`; ADR-0160's
   *six* becomes *seven* by this ADR. It then releases at the **same** version as the other six
   — if 0.1.0 is already on crates.io by then, it first appears at the next lockstep version,
   never at a version of its own.
4. **If the kill line fails**, the crate stays in the workspace with `publish = false`, the
   failing pair goes to `docs/reference/measured-costs.md`, and ADR-0180 is marked with the
   result — ADR-0098 item 4's *"stays unpublished"*, not a removal.
5. **Not in the facade.** `fixbolt` gains no `sqlite` feature; a user adds
   `fixbolt-store-sqlite` beside it. Reversible later without a break.

## Options not taken

- **Leave `rusqlite` unconditional and exclude the crate from the `no-default-features` job.**
  The job's claim would then be *"builds with nothing optional installed, except where it
  doesn't"*, and every other `--workspace` build would still compile C. Rejected.
- **Keep the crate out of the workspace** (like `fuzz/`). It would escape `cargo test --all`,
  clippy, the lint ratchets and the semver gate — the gates a published crate must pass.
  Rejected.
- **A version of its own** (e.g. `0.0.x` while on trial). It pins `fixbolt-engine` exactly, so
  every engine release would need a store release anyway; separate numbering only adds a matrix
  nobody builds. Rejected.
- **Publish it now and yank if killed.** A publish is permanent (ADR-0160 research); the kill
  line exists to be applied before that. Rejected.

## Consequences

**Good**

- `--no-default-features` still means *nothing optional*, for the new crate as for the engine.
- Flipping to published is a few lines and a script list, not a packaging project: the shape
  is proven by every CI run from the first commit.

**Bad — and accepted**

- **Every default workspace build now compiles SQLite's C** (`cargo test --all`, clippy, the
  `cargo hack` powerset, the bench job): tens of seconds per CI job, and a C compiler on every
  runner and developer machine that builds the workspace with defaults. The
  `no-default-features` job is the one that must not need it.
- **A crate whose `--no-default-features` build is empty** is a surprise to a user who turns
  defaults off by habit. Its rustdoc and `README.md` say so in the first line.
- **MSRV is inherited from a dependency that promises none.** A `cargo update` within 0.40 can
  raise it silently; only the `package` job's build on the declared `rust-version` catches it,
  and only once the crate is in that job (decision 3). Until then an MSRV break is invisible —
  and harmless, because nothing is published.
- **One more hard-coded list entry in three scripts** when the flip happens — the lists are
  hard-coded on purpose (`check-release-versions.sh` lines 32–35) and this is their price.
