# ADR-0160 — Six crates release in lockstep, and the packaged sources stand in for crates.io until the first publish

- **Status**: **Accepted — 2026-09-23** (manager, owner's standing mandate, with the packaging plan). Proposed 2026-09-23. Written by the architect (Opus) for phase 3 rows 6–8
  ([plan](../plans/2026-09-23-p3-packaging-and-first-release.md)). Accepted by the manager under
  the owner's standing mandate, or by the owner. It decides nothing ADR-0097 or ADR-0104 already
  decided; it settles what building to them surfaced.
- **Date**: 2026-09-23
- **Deciders**: proposed by the architect; accepted by the manager or the owner.
- **Related**: [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  decisions 3, 7 (exit criteria 3, 7, 8) and the owner's answers Q3, Q5, Q8;
  [ADR-0104](ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)
  decision 6; `CLAUDE.md` §2 item 6;
  [a-scratch-fixture-inherits-the-machine](../reference/a-scratch-fixture-inherits-the-machine.md).

## Context

ADR-0097 fixed *what* is published (Q8: `fixbolt-codec`, `fixbolt-dict`, `fixbolt-session`,
`fixbolt-engine`, `fixbolt-sbe`, `fixbolt`), *which* version (Q3: `0.1.0`) and *who* presses the
button (Q5: the owner). Preparing the manifests for that raised six questions ADR-0097 did not
answer, and one of them contradicts a number the workspace already publishes in its manifests.

What was measured, on 2026-09-23, on the desk (`tmt-B450-I-AORUS-PRO-WIFI`, cargo 1.98.0), in a
scratch copy of `main` `a6c6026` with **no `vendor/`**, after only these edits: `version =
"0.1.0"` in `[workspace.package]`, `version.workspace = true` and no `publish = false` in the six
crates, and `version = "=0.1.0"` added beside `path` on every internal **normal** dependency
(dev-dependencies left path-only):

1. `cargo package -p fixbolt-dict --no-verify` on the untouched tree refuses:
   `all dependencies must have a version requirement specified when packaging. dependency
   'fixbolt-codec' does not specify a version`.
2. After the edits, `cargo publish --workspace --dry-run --allow-dirty` packages and verifies all
   six in dependency order — codec 87.1 KiB, dict 253.5 KiB, session 188.6 KiB, engine
   504.7 KiB, sbe 42.2 KiB, fixbolt 32.7 KiB compressed — skips `fixbolt-conformance`,
   `fixbolt-sbe-gen` and `tools/*` (all `publish = false`), and prints no warning other than
   `aborting upload due to dry run`. The packaged `fixbolt-codec` manifest has an **empty**
   `[dev-dependencies]` and its `fix50sp2` feature rewritten to `fix50sp2 = []`: path-only
   dev-dependencies were stripped, as the Cargo book says.
3. A crate outside the tree, depending on `fixbolt = { version = "0.1.0", features = ["sbe"] }`
   and `fixbolt-engine = { version = "0.1.0", features = ["tls", "affinity", "fix50sp2"] }`, with
   `[patch.crates-io]` pointing each of the six names at `target/package/<name>-0.1.0/` (the
   unpacked `.crate` sources the dry run leaves behind), builds and runs on 1.98.0.
4. The same crate on **Rust 1.85.0** — the `rust-version` every manifest declares today — fails:
   five `error[E0658]: 'let' expressions in this position are unstable` in `fixbolt-codec`, and
   `fixbolt-dict`'s build script fails the same way. On **1.88.0** it builds, every feature above
   on. The `1.85` claim was never checked by anything (the engine's `Cargo.toml` comment already
   says so for the tests).
5. `docs/GUIDE.md` §SBE tells a user to add `fixbolt-sbe-gen` as a build-dependency — a crate Q8
   keeps unpublished.
6. `crates.io/api/v1/crates/<name>` answers 404 for all six names (2026-09-23): the names are
   free today, and nothing reserves them until the first publish.

## Research

| Source | What it says |
|---|---|
| <https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html> (*Multiple locations*) | "The `git` or `path` dependency will be used locally … and when published to a registry like crates.io, it will use the registry version"; "only dev-dependencies that specify a `version` will be included in the published crate." |
| <https://www.tweag.io/blog/2025-07-10-cargo-package-workspace/> | Workspace publish verifies each crate against a local registry overlay of its not-yet-published siblings, then uploads; "not truly atomic … you can just retry publishing the ones that failed to upload." |
| <https://github.com/rust-lang/cargo/pull/15525> | `cargo publish --workspace` skips a `publish = false` package instead of failing. |
| <https://www.infoworld.com/article/4060262/rust-1-90-brings-workspace-publishing-support-to-cargo.html> | Multi-package publish is stable from Rust 1.90; the workspace is pinned to 1.98.0. |
| <https://raw.githubusercontent.com/rust-lang/crates.io/main/src/licenses.rs> | crates.io parses `license` with `spdx`, `allow_unknown: false`, `allow_imprecise_license_names: false`. |
| <https://raw.githubusercontent.com/EmbarkStudios/spdx/main/src/lexer.rs> | The lexer turns `LicenseRef-…` into a `LicenseRef` token **before** it consults `allow_unknown`. |
| `https://crates.io/api/v1/crates/slint`, read 2026-09-23 | `slint` 1.18.1, published 2026-09-21, `license = "GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0"` — crates.io accepted a `LicenseRef-` expression two days ago. Found by a GitHub code search for `LicenseRef` in `Cargo.toml`. |
| <https://doc.rust-lang.org/cargo/reference/manifest.html> | `description` and `license`/`license-file` required by crates.io; at most 5 keywords (≤ 20 chars) and 5 categories from `category_slugs`; `README.md` in the package root is picked up by default. |
| `https://crates.io/api/v1/category_slugs`, read 2026-09-23 | `finance`, `network-programming`, `parser-implementations`, `encoding`, `no-std` all exist. |
| <https://doc.rust-lang.org/cargo/reference/publishing.html> | A publish is permanent; `cargo yank --version X` (and `--undo`) stops new lockfiles resolving to it and deletes nothing; 10 MB `.crate` limit. |
| <https://rust-lang.github.io/rfcs/3660-crates-io-crate-deletions.html> | An owner may delete a crate within 72 h of publish, or later with one owner and few downloads — **only if no other crate depends on it**, so six interdependent crates delete in reverse dependency order. |
| <https://docs.rs/about/builds>, <https://docs.rs/about/metadata> | docs.rs builds on nightly, `x86_64-unknown-linux-gnu`, **network blocked**, sets `DOCS_RS` and `--cfg docsrs`; `[package.metadata.docs.rs] features / all-features / rustdoc-args`. |
| <https://github.com/rust-lang/rust/pull/138907>, <https://users.rust-lang.org/t/doc-auto-cfg-is-gone-what-am-i-supposed-to-do/135070> | `doc_auto_cfg` was merged into `doc_cfg` (Oct 2025); `#![cfg_attr(docsrs, feature(doc_auto_cfg))]` now fails the docs.rs build — use `feature(doc_cfg)`. |
| <https://github.com/obi1kenobi/cargo-semver-checks> | Baselines: `--baseline-version` (registry), `--baseline-rev` (git), `--baseline-root`; each release supports the then-current stable and beta rustdoc JSON. crates.io: latest 0.50.0 (2026-08-01), its own `rust-version` 1.93. |
| <https://doc.rust-lang.org/cargo/reference/semver.html> | "Initial development releases starting with `0.y.z` can treat changes in `y` as a major release, and `z` as a minor release." |
| <https://raw.githubusercontent.com/rustls/rustls/main/RELEASING.md> | A sibling project's order: version bump commit → `cargo publish --dry-run` **without** `--allow-dirty` → PR green → tag → `cargo publish` → GitHub release. |

**Found nothing:** a crate on crates.io whose `license` combines `LicenseRef-` with `AND` and
parentheses (slint uses `OR` only; crates.io's own tests parse `MIT OR (Apache-2.0 AND MIT)`);
a statement of whether `cargo publish --dry-run` asks crates.io to validate the licence (the
server's checks run at upload); whether `cargo semver-checks --workspace` skips `publish = false`
members (read it off the first run).

## Decision

1. **Lockstep.** One version for the six crates, in `[workspace.package] version`, inherited by
   each. Every internal **normal** dependency carries `version = "=<that version>"` beside its
   `path`: the tables `fixbolt-dict` generates implement `fixbolt-codec`'s traits, `session` is
   generic over both, and an exact pin means a stranger can never resolve a mix no CI run built.
   Internal **dev**-dependencies stay path-only, so they are stripped (measured, fact 2) — which
   is what lets the dev-dependency cycles (`codec` ↔ `dict`, `session` ↔ `engine`) and the
   unpublished `fixbolt-conformance` stay as they are. A new script,
   `scripts/check-release-versions.sh`, fails if any internal normal requirement is not `=` the
   workspace version or any published crate stops inheriting it.
2. **Q8 stands: `fixbolt-conformance`, `fixbolt-sbe-gen` and `tools/*` keep `publish = false`.**
   `docs/GUIDE.md` tells an SBE user to take `fixbolt-sbe-gen` as a **git** build-dependency
   pinned to the release tag (`{ git = "https://github.com/tmthang86/fixbolt", tag = "v0.1.0" }`),
   which a binary crate may do. Publishing it later is additive and costs no semver break; this
   is the reversal path if a stranger asks.
3. **`rust-version` becomes `"1.88"`**, the lowest toolchain the packaged sources were seen to
   build on (fact 4), and CI keeps it true by building the packaged sources on exactly that
   toolchain. A declared MSRV no job builds is prose.
4. **What a `.crate` holds is an allowlist** (`include`): `src/**`, `build.rs` where there is
   one, `spec/**` and `NOTICE` for `fixbolt-dict`, `examples/**` for `fixbolt`, `README.md`,
   `LICENSE-MIT`, `LICENSE-APACHE`. Tests and benches are not shipped: their dev-dependencies are
   stripped and half of them read `vendor/`, so they cannot compile from the `.crate`. The two
   licence texts are **copies** in each crate (only files under a package root reach its
   `.crate`, as ADR-0104 decision 4 found for `NOTICE`), held byte-identical to the root copies by
   the same new check.
5. **The licence expression stays as ADR-0104 decision 6 wrote it.** crates.io accepted
   `LicenseRef-` ids in a publish on 2026-09-21 (slint), through a lexer that admits them before
   `allow_unknown` is consulted. `license-file = "NOTICE"` remains the documented fallback; it
   is now expected not to be needed. The first real proof is still the owner's upload, because a
   dry run does not reach the server's validation.
6. **Before the first publish, the packaged sources are the stranger.** CI runs `cargo publish
   --workspace --dry-run` on a runner with no `vendor/`, then builds a crate outside the tree
   against `target/package/<name>-<version>/` through `[patch.crates-io]` — the exact bytes that
   would upload — over the feature sets a user can pick, on 1.98.0 and on the declared
   `rust-version`, and drives one Logon/Logout through `docs/GETTING-STARTED.md`'s code pasted
   verbatim. After the publish, the same script runs with the patch removed and `cargo add
   fixbolt@<version>` instead (ADR-0097 exit criterion 7).
7. **`cargo-semver-checks` is pinned (0.50.0) and advisory until the first version exists**,
   with `--baseline-rev origin/main` so it reads something real on every PR; after the publish it
   compares against the registry and blocks (exit criterion 8). The `0.y` rule is Cargo's: a
   breaking change needs `y` bumped.
8. **docs.rs builds a named feature list, not `--all-features`**: `fixbolt-engine` `standard`,
   `affinity`, `tls`, `fix50sp2`; `fixbolt` `standard`, `sbe`; `fixbolt-dict` and
   `fixbolt-session` `fix50sp2`; `fixbolt-sbe` defaults; `fixbolt-codec` defaults (its
   `fix50sp2` is dev-only and is published as an empty feature — fact 2). Each crate root gains
   `#![cfg_attr(docsrs, feature(doc_cfg))]`, never `doc_auto_cfg`.

## Consequences

**Good**

- Exit criterion 3 and most of criterion 7 are proven **before** anything irreversible happens:
  the owner's `cargo publish` uploads bytes a CI run already built, with every public feature,
  on two toolchains, and drove through a Logon/Logout.
- A stranger cannot build a mixed set of fixbolt crates; the one combination CI built is the
  only one that resolves.
- The MSRV a stranger reads on crates.io is a measured fact, and a regression of it is red.
- The crates stay small (largest 505 KiB compressed of a 10 MB limit) and carry their licences.

**Bad — and accepted**

- **Any fix to any crate republishes all six.** Lockstep plus exact pins means a one-line
  `codec` patch is six uploads. For a single-owner project on `0.x` that is the cheaper failure.
- **Tests do not travel with the `.crate`.** A distribution packager who runs tests from the
  crate (Debian's practice) cannot; they build from the git tag instead.
- **An SBE user takes one crate from git.** `sbe-gen` from a tag is a worse experience than
  `cargo add`; decision 2 says how it reverses.
- **`rust-version = "1.88"` is a stronger floor than the manifests claim today** — but today's
  claim was false.
- **The six names can still be taken by someone else before the owner publishes.** Nothing but
  publishing reserves a crates.io name.
- **semver-checks before the publish measures drift from `main`, not from a release**; it will
  also flag breaks that are fine on `0.x` until the first version exists. Advisory is the only
  honest status for it.
- **The pre-publish stranger uses `[patch]`, not the registry.** It cannot see a registry-only
  failure (the server refusing the licence, an index delay); only the post-publish run can.

## Sources

The table under *Research*. Measurements in *Context* were taken by the architect in a scratch
copy outside the repository; the plan's rows 6a and 6b reproduce them as committed gates.
