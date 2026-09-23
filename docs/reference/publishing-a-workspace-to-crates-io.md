# Publishing a workspace to crates.io

> **What this page is:** the traps between a workspace that builds and six crates
> `cargo publish` accepts, each measured on this repository. Decisions are
> [ADR-0160](../decisions/ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md);
> this page is what it cost to find them. Machine for every measurement: the desk
> (`tmt-B450-I-AORUS-PRO-WIFI`), cargo 1.98.0 unless named, `2026-09-23`, on a copy
> of the tree **without `vendor/`** — the state a crates.io user and docs.rs are in.

## 1. A path dependency with no version is refused

`cargo package -p fixbolt-dict --no-verify` on `main` `a6c6026`:

```
all dependencies must have a version requirement specified when packaging.
dependency `fixbolt-codec` does not specify a version
```

A `path` is dropped from the uploaded manifest, so cargo insists on a `version`
beside it. Here every internal **normal** dependency carries `version = "=0.1.0"`
— an exact pin, so a stranger never resolves a mix of two releases no CI run built
together.

**Guard:** `scripts/check-release-versions.sh` — red on that tree with
`FAIL fixbolt-dict: dependency fixbolt-codec has no version requirement` (and 36
more lines); red again with a caret `"0.1.0"` instead of `"=0.1.0"`.

## 2. A path-only dev-dependency is stripped — and a feature that named it becomes empty

The Cargo book (*Specifying dependencies*, "Multiple locations"): a dev-dependency
with only a `path` is removed from the published manifest. That is what lets the
dev-dependency cycles (`codec` ↔ `dict`, `session` ↔ `engine`) and the
unpublished `fixbolt-conformance` stay as they are.

The side effect: `fixbolt-codec`'s `fix50sp2 = ["fixbolt-dict/fix50sp2"]` names a
crate that is now gone, and the packaged manifest carries `fix50sp2 = []`. The
feature exists on crates.io and does nothing — which is correct, since it was only
ever for codec's own benches. docs.rs is told to build codec with defaults only.

**Guard:** none that asserts the empty feature; it is harmless by construction.
The docs.rs feature lists are in each crate's `[package.metadata.docs.rs]`.

## 3. A declared `rust-version` nobody builds is prose

Every manifest said `rust-version = "1.85"`. A crate outside the tree, patched onto
the packaged sources, on Rust 1.85.0: five
`error[E0658]: 'let' expressions in this position are unstable` in
`fixbolt-codec`, and `fixbolt-dict`'s build script failed the same way. On 1.88.0
it built with every feature on. `[measured 2026-09-23]` `cargo +1.88.0 check -p
fixbolt --all-features` and `-p fixbolt-engine --all-features` both finish.

**Guard:** the `package` CI job (plan row 6b) builds the packaged sources on
`+1.88.0`; its reversal is `rust-version = "1.85"` on `+1.85.0` → `E0658`.

## 4. Raising `rust-version` switches clippy lints on

Clippy reads `rust-version` as the MSRV and suppresses lints whose suggestion needs
a newer compiler. Moving 1.85 → 1.88 turned on three at once, across the whole
workspace — `[measured 2026-09-23]`, clippy 0.1.98, one variable changed:

```
rust-version = "1.85": cargo clippy --all-targets  → 0 warnings
rust-version = "1.88": cargo clippy --all-targets  → 15 warnings in 9 files
  collapsible_if            (let chains, 1.88)
  manual_is_multiple_of     (u*::is_multiple_of, 1.87)
  chunks_exact_to_as_chunks (slice::as_chunks, 1.88)
```

With `-D warnings` that is red, and it lands in `src/` of `session`, `engine` and
`conformance` — files a "metadata only" change does not expect to touch. An MSRV
bump is therefore a code change, not a manifest edit. Here it was plan step 6a':
`cargo clippy --fix` applied clippy's own 15 suggestions (let chains,
`is_multiple_of`, `as_chunks`), no behaviour change, with the 59 / 59 gates, the
FIXT corpus and the session and engine allocation benches re-run afterwards.

**Guard:** `cargo clippy --all-targets -- -D warnings` itself; it goes red.

## 5. An explicit `readme` that names a missing file is refused

```
error: readme `README.md` does not appear to exist (relative to `…/crates/codec`)
```

With no `readme` key, cargo picks up `README.md` at the package root on its own
and writes `readme = "README.md"` into the packaged manifest (checked on a copy with
a temporary README: the `.crate`'s `Cargo.toml` read `readme = "README.md"`;
without one it reads `readme = false`). Here every published crate has its own
`README.md` and names it explicitly, so a README deleted by mistake fails the dry run
instead of silently publishing a crate page with none. Whatever `include` lists must
still name `README.md`, or the file is not shipped.

**Guard:** `cargo publish --workspace --dry-run` fails on it.

## 6. `homepage` equal to `repository` is flagged

nightly cargo 1.100 (`2026-09-16`) prints, once per crate:

```
warning: `package.homepage` is redundant with `package.repository`
```

Stable 1.98.0 is silent. The field is omitted here; crates.io shows the repository
link anyway.

**Guard:** none. It is a warning, and only nightly (docs.rs) prints it today.

## 7. What `cargo publish --workspace --dry-run` does and does not prove

Stable since Rust 1.90; skips `publish = false` members (cargo PR #15525); packages, then builds each
`.crate` against the others' packaged sources through a temporary local registry
(`target/package/tmp-registry/`). It builds each crate's **default** features only,
never tests, and never reaches crates.io's server-side checks (the licence
expression, the name). An `include` allowlist makes cargo print one
`warning: ignoring test/benchmark … not included in the published package` per
test and bench target — 119 here, expected, not a defect. The real upload is
**not atomic**: a failure midway leaves the crates already uploaded.

Sizes `[measured 2026-09-23]`, compressed, with the `include` allowlist: codec
34.2 KiB, dict 226.1 KiB, session 96.1 KiB, engine 231.8 KiB, sbe 36.8 KiB,
fixbolt 25.6 KiB (limit 10 MB).

## 8. A crate that has never been published can still be built as itself, through `[patch.crates-io]`

Trap 7 above is the closest thing to "build the packaged sources" that `cargo
publish --workspace --dry-run` offers, and it only ever builds default
features. To build the exact same bytes under a feature a real user might
pick — the `package` CI job's actual job, plan row 6b — a second crate outside
the workspace can depend on, say, `fixbolt-engine = { version = "=0.1.0",
features = ["tls"] }` as an ordinary registry dependency, and
`[patch.crates-io]` redirects that name to `target/package/fixbolt-engine-0.1.0/`
— the directory the dry run above already left behind. `[measured 2026-09-23]`
this works even though `fixbolt-engine` has never been uploaded and `cargo
info fixbolt-engine` would 404: cargo never needs the registry to actually
hold the crate once a patch satisfies the same name and a compatible version,
it only needs the registry *index* reachable (`Updating crates.io index`
still runs). The whole six-crate dependency graph resolves and compiles this
way, patched all at once, whether or not the case under test reaches every
one of them.

**Guard:** `scripts/check-packaged-build.sh`, the technique behind every one
of its 12 cases.

## 9. cargo's MSRV-aware resolver refuses before it compiles, once the manifest tells the truth

Trap 3 found `error[E0658]` on 1.85.0 while every manifest still said
`rust-version = "1.85"` — a lie the resolver had no reason to doubt, so it let
rustc find the lie itself. Once `rust-version = "1.88"` is the honest
declaration (ADR-0160 decision 3), asking cargo 1.98.0 to build the same
packaged sources on `+1.85.0` **never reaches rustc at all**:

```
error: rustc 1.85.0 is not supported by the following packages:
  fixbolt@0.1.0 requires rustc 1.88
  fixbolt-codec@0.1.0 requires rustc 1.88
  …
```

`[measured 2026-09-23]` reverting the *declaration* back to `1.85` while the
`src/` files still use `1.88`-only syntax (the let chains 6a' introduced)
reopens the resolver's door — the version now matches what was asked for —
and the original `error[E0658]: 'let' expressions in this position are
unstable` reappears in `fixbolt-codec`'s `src/template.rs`. **Two different
red messages guard the same fact from two different lies**: an honest,
too-high declaration is caught by the resolver before a byte is compiled; a
dishonest, too-low one is caught by the compiler once it tries.

**Guard:** `scripts/check-packaged-build.sh` builds the packaged sources on
`+1.88.0` itself, so a manifest correctly declaring `1.88` cannot silently
regress to needing more; there is no committed reversal for the dishonest
direction, since ADR-0160 decision 3 depends on the declaration staying true
rather than on a test that keeps re-lying about it.

## 10. `cargo-semver-checks --workspace` already knows about `publish = false`, and a `0.0.0` baseline skips everything

No `--exclude` is needed to keep `fixbolt-conformance`, `fixbolt-sbe-gen` or
`tools/*` out of `cargo semver-checks --workspace --baseline-rev origin/main`
(plan row 6c): `[measured 2026-09-23]` it reads `publish = false`
(ADR-0160 decision 2) the same way `cargo publish --workspace` does and never
mentions them, on either side of the diff.

The sharper trap is the other direction. Before row 6a merges to `main`, every
one of the six crates is `0.0.0` there — comparing against `0.1.0` is, in
semver terms, a **major** version change, and cargo-semver-checks treats a
major bump as "anything goes": every one of its 254 lints is *skipped*, not
*passed* (`Checked [0.000s] 0 checks: 0 pass, 254 skip` per crate,
`[measured 2026-09-23]`), and the job exits 0 regardless of what the API
actually did. A green run of the `semver` job on a PR opened **before** row 6a
lands proves nothing; the first meaningful comparison is the first PR opened
**after** it does, once both sides of `--baseline-rev origin/main` read
`0.1.0` and a real structural diff runs.

**Guard:** none inside the job itself — it is advisory
(`continue-on-error: true`) until ADR-0097 exit criterion 8, by design
(ADR-0160 decision 7). The `semver` job's own comment in `ci.yml` names this
so the first few runs are read correctly; the reversal that proves the
*mechanism* (renaming `fixbolt_codec::checksum::checksum`) was run against an
unmodified worktree at the same `0.1.0`, not against `origin/main`, for
exactly this reason — see `DESIGN.md` §6 *Packaging*.

## Sources

- The Cargo book: *Publishing on crates.io*; *The manifest format* (`readme`,
  `include`, `rust-version`, `homepage`); *Specifying dependencies*, "Multiple
  locations" and "Overriding dependencies" (`[patch]`).
- cargo PR #15525 (`cargo publish --workspace`).
- Clippy's lint list for 1.98.0: `collapsible_if`, `manual_is_multiple_of`,
  `chunks_exact_to_as_chunks` (each names its MSRV).
- cargo's own `rust-version`-aware dependency resolution, observed here on
  1.98.0: the source of trap 9's pre-compile refusal. Not independently dated
  against upstream cargo history — the observation is the measurement below,
  not a claim about when it landed.
- `cargo-semver-checks` 0.50.0 docs and lint list (`function_missing.ron`);
  its own README on how a `0.y → 0.y'` comparison is scored.
- The measurements above; plan
  [2026-09-23-p3-packaging-and-first-release](../plans/2026-09-23-p3-packaging-and-first-release.md)
  *Những gì đã biết chắc* facts 1–5 and 8, *Chia việc* rows 6b and 6c.
