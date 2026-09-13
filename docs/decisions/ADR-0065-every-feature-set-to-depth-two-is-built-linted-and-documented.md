# ADR-0065 — Every feature set to depth two is built, linted and documented

**Status:** Accepted (owner, 2026-09-13) · **Date:** 2026-09-13 · **Plan:** docs/plans/2026-09-13-what-the-residue-review-found.md, steps 4–5 · decision 1's `RUSTDOCFLAGS` superseded by [ADR-0066](ADR-0066-the-rustdoc-gate-denies-every-warning.md)

## Context

Two defects found by the senior review of PR #68 share one shape: **a feature combination
no gate builds.**

- STATUS.md item 78: `cargo check -p fixbolt-engine --no-default-features --features
  affinity` → `warning: unused import: crate::msglog::MaybeLog` at
  `crates/engine/src/shard.rs:43`. `serve_sharded_hft_with` is `#[cfg(feature =
  "standard")]`; the import it uses is not. CI's clippy runs under the default set and
  under `--features affinity` *with* the default; the one `cargo test` under
  `--no-default-features --features affinity` prints the warning and passes, because a
  warning is not a failure to `cargo test`. `[measured 2026-09-13]` reproduced from the
  desk with `--target x86_64-unknown-linux-gnu`; invisible to a plain `cargo check` on
  macOS, where `mod shard` does not compile at all.
- STATUS.md item 79: the `docs` job runs `cargo doc` twice, under the default set and
  under `--all-features`. An intra-doc link to `Self::Tls` was valid under the second and
  broken under `--features affinity` alone. A developer caught it by hand.

Both are instances of the class `docs/reference/a-doc-gate-never-opened-the-file-it-was-guarding.md`
already names — a gate green about code it was never handed — and item 61 closed the first
instance by adding the one missing set. That fix does not generalise: with features
`standard`, `affinity` and `tls` on `fixbolt-engine`, three more on `tools/w2w`, and one
each on `crates/library` and `tools/interop`, adding sets by hand as each defect arrives
is the loop item 61 already ran once.

What sibling projects do was read before this was written. tokio's CI runs `cargo hack
check --all --feature-powerset --depth 2 --keep-going` and `cargo hack test
--each-feature`, installing the tool with `taiki-e/install-action@cargo-hack`; hyper and
futures use the same `--feature-powerset --depth 2` shape. serde enumerates six sets by
hand across three jobs, which works for a crate whose feature list is fixed and small. The
Rust Project Primer's feature-checks page recommends `--feature-powerset --depth 2` for
compile checks and `--each-feature` for tests, on the grounds that a full powerset is
impractical at any real feature count. cargo-hack deduplicates fully equivalent
combinations and includes `--no-default-features` and the default set in both modes.
**It does not include `--all-features` under `--feature-powerset --depth 2`** when a crate has
three or more features — `[corrected 2026-09-13]` this sentence said it did; see the second
revision below.

## Decision

1. **One CI job builds every feature set of every workspace crate up to depth two** — no
   features, each feature alone, and each pair — with `cargo hack`, using
   `--feature-powerset --depth 2 --workspace`, and runs two commands over that set:
   `clippy --all-targets -- -D warnings` and `doc --no-deps` with
   `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::redundant_explicit_links"`.
   **The same job then runs both once more under `--all-features`**, as plain `cargo clippy
   --workspace --all-targets --all-features -- -D warnings` and `cargo doc --workspace
   --no-deps --all-features` under the same flags, because that set is not in the powerset
   at depth two. `[corrected 2026-09-13]` this decision said the powerset contained "all
   features".
   Clippy rather than `check`, because clippy runs rustc's own lints too and `-D warnings`
   is what turns item 78 from a printed warning into a red job.
2. **The boundary is depth two, and it stays two when the feature count grows.** The
   reason is not the number; it is what a defect of this class needs. Every instance seen
   in this repository so far needed exactly one feature named and one absent: `tls` off
   for item 61, `standard` off with `affinity` on for items 78 and 79. A pair is the
   smallest set in which "on" and "off" can both be named — the single-feature sets say
   what a feature needs, the pairs say what two features share (here, the one optional
   `libc`). A defect that appears only when three particular features are on together
   has not been observed here, and each further depth multiplies the job by the feature
   count. When a three-feature defect is found, the plan that fixes it decides whether to
   raise the depth or add that set by name, and records why in a new ADR.
3. **Tests are not run per feature set by this job.** The existing jobs already run the
   tests that exist for each feature (`--features affinity` twice, `--features tls` in its
   own job with its own kernel), and a test that fails only under a *pair* of features is
   a behaviour that depends on a feature, which is a different class from "did not
   compile" or "a link did not resolve". Running the suite eleven times per commit to
   guard a class never observed is a cost this decision refuses on purpose.
4. **The tool is `cargo-hack`, installed by `taiki-e/install-action@cargo-hack`.** It is a
   CI binary, not a dependency of any crate: `Cargo.lock` does not change, `deny.toml`
   does not see it, and non-negotiable 6 is untouched. The alternative — a shell loop over
   `cargo metadata`'s feature lists — is a second implementation of feature
   deduplication for this repository to maintain, and the reason `check-scratch-fixtures.sh`
   stayed a regex (ADR-0061) does not apply: here the tool that already exists is the
   one every sibling project uses.
5. The two `cargo doc` lines in the `docs` job are **subsumed** by decision 1 — the default
   set by the powerset, and `--all-features` by decision 1's explicit steps, not by the
   powerset — and removed; the job's comment, which records item 61, moves to the new job.
   The test lines in the `affinity` job stay. `[corrected 2026-09-13]` this said both were
   in the powerset, which is how the `--all-features` doc build was lost.

## Consequences

**Good**

- Items 78 and 79 become the same red, produced by the same command, and the class is
  closed rather than the two instances. A new feature added to any crate is in the
  powerset the next commit, with no `ci.yml` edit.
- Today depth two plus `--all-features` covers every combination of the three named
  features on the largest crate — the seven below three by the powerset, the one at three
  by the explicit steps — so the boundary costs nothing yet. It is stated now so that the
  day it stops covering them, it is a decision on record and not a number someone picked.
  `[corrected 2026-09-13]` this said depth two alone was exhaustive, "every one of its 8
  sets"; depth two alone never built `standard,affinity,tls`.
- The job's own log names the set that failed: cargo-hack prints `info: running cargo
  clippy --no-default-features --features affinity on fixbolt-engine` before each run, so
  a red is attributable without re-running anything.

**Bad**

- **A third-party GitHub Action, pinned by tag.** `taiki-e/install-action` downloads a
  release binary; this repository already trusts `EmbarkStudios/cargo-deny-action@v2` the
  same way, but it is one more supply-chain trust, and a moved tag is a moved trust. Pin
  by tag as the sibling action is pinned; a hash pin for both is a separate decision.
- **Job time.** `[to be measured in step 4 before this is accepted]` eleven clippy builds
  and eleven doc builds of `fixbolt-engine` — ten powerset sets and `--all-features` — plus
  the smaller crates, sharing one target
  directory. The estimate is under fifteen minutes on `ubuntu-latest`; if the measurement
  says otherwise, `--depth 1` for `doc` and `2` for `clippy` is the first thing to try,
  and the ADR is revised in place while `Proposed`.
- **Depth two is a bet, stated as one.** A defect needing three named features passes
  this gate green, and the gate will not say so. The `DESIGN.md` §6 row names the depth
  so the limit is read every time the row is.
- `target_os = "linux"` is not a feature and is not enumerated. The job runs on
  `ubuntu-latest`, so `mod shard` and `mod affinity` are compiled; a `cfg` that is wrong
  on another platform is outside this gate, as it was before.

## Revision 2026-09-13 — measured at step 0

- **cargo-hack 0.6.45 lists `default` as a feature of its own**, so `fixbolt-engine`'s
  powerset to depth two is **10 sets, not 8**: the "Good" section above assumed `default`
  and `--all-features` were the only two beyond the three named features and their pairs;
  cargo-hack additionally enumerates `default` alone and each pair that includes it.
- **Its first run found 14 broken intra-doc links**, every one in a set built without
  `standard`: 11 in `fixbolt-engine`, 3 in `fixbolt`. Fixed in commit `3404114`.
- **On a macOS desk the `tls` sets cannot be built at all** — `ktls-core` is Linux-only —
  and any `--target x86_64-unknown-linux-gnu` build that reaches a dev-dependency needs a
  C cross-compiler for `ring`, which this desk does not have. So the gate as a whole is
  only fully observable on CI; the desk can prove the non-`tls` sets and no more.
- **Wall time on the desk**: clippy 52 s, doc 48 s — both runs failed early (on the 14
  broken links, and on sets that cannot cross-compile), so these are a **lower bound**, not
  the CI estimate the "Bad" section above still owes.

## Revision 2026-09-13 — the powerset never held `--all-features` (review D2)

- **What was wrong**: decisions 1 and 5, the first "Good" bullet and the Context's last
  sentence all said `--feature-powerset --depth 2` includes "all features". It does not.
  cargo-hack's depth counts named features, and it counts `default` as one: for
  `fixbolt-engine` the ten sets are none, `default`, `standard`, `affinity`, `tls`,
  `affinity,default`, `affinity,standard`, `affinity,tls`, `default,tls`, `standard,tls` —
  and `standard,affinity,tls` is three, one past the depth. `[measured 2026-09-13]` CI run
  34750085195's log lists exactly those ten, and `cargo hack check --workspace
  --feature-powerset --depth 2 --print-command-list` on the desk prints the same ten for
  `fixbolt-engine` and for `tools/w2w`. Removing the `docs` job therefore removed the only
  rustdoc build under `--all-features` — item 61's set — while every document said it had
  been kept.
- **What changed**: two explicit steps at the end of the `feature-sets` job,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` and
  `cargo doc --workspace --no-deps --all-features` under the same `RUSTDOCFLAGS`. The
  decisions and consequences above are corrected in place, each correction marked. The
  depth-two boundary and its reasoning (decision 2) are unchanged.
