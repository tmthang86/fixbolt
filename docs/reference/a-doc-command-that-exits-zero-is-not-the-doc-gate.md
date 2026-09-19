# A doc command that exits 0 is not the doc gate

`[measured 2026-09-19]`

## The shape

CI failed on a commit whose author had run `cargo test --all`, `cargo test
--no-default-features`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`,
all green, plus the gate the plan row named. The failing CI step was:

```
failed commands:
    fixbolt-conformance:
        `cargo doc --no-deps --manifest-path crates/conformance/Cargo.toml`
```

Run by hand, **that exact command exits 0**:

```
$ cargo doc --no-deps --manifest-path crates/conformance/Cargo.toml
warning: public documentation for `load_corpus` links to private item `load`
warning: `fixbolt-conformance` (lib doc) generated 2 warnings
    Finished `dev` profile
$ echo $?
0
```

The two lines say `warning`. rustdoc's `private_intra_doc_links` is warn-by-default, so the
command succeeds. The gate is not the command — it is the command **plus its environment**:
`ci.yml`'s `feature-sets` job runs both rustdoc steps under `RUSTDOCFLAGS="-D warnings"`
([ADR-0066](../decisions/ADR-0066-the-rustdoc-gate-denies-every-warning.md)),
which turns those same two warnings into the failure above.

So "I ran `cargo doc` and it passed" and "the doc gate passes" are different claims, and the
first is the one that is easy to make by accident. This is
[CLAUDE.md](../../CLAUDE.md) §10's *"read the output, not the exit status"* in the one place
where reading the exit status feels safe, because the command being run **is** the one CI runs.

## The second half: a plan row's gate list is not CI's gate list

The step that tripped this was closed against a plan row whose *Gate (lệnh)* column named
`cargo test -p fixbolt-conformance` and nothing else. That column is the row's own exit
criterion, not an inventory of what CI will run over the commit. A sibling row in the same
table (`A1`) does name `cargo doc`, which is exactly the trap: the presence of `cargo doc` in
one row's gate reads as evidence that its absence from another row's gate means that row does
not face it. It faces it. `feature-sets` documents every crate in the workspace, over every
feature set to depth two, on every pull request.

Any step that adds or edits a **public** item's doc comment faces the rustdoc gate, whether or
not its plan row says so.

## What to run instead

```
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
```

and, for a crate with features, the powerset step as `ci.yml` spells it (`cargo hack doc
--workspace --no-deps --feature-powerset --depth 2`). A crate that declares no features — as
`fixbolt-conformance` does — has a one-member powerset, so the plain `--no-deps` run under
`RUSTDOCFLAGS` covers it completely.

## What guards it

The `feature-sets` job in `.github/workflows/ci.yml`, which is what caught this. There is no
unit test to add: the defect was a doc comment, and the lint that finds it already runs. What
this page adds is the reason a green local run did not predict it — so the next reader spends
no time wondering how the same command disagreed with itself.

## Related

- [ADR-0066](../decisions/ADR-0066-the-rustdoc-gate-denies-every-warning.md)
  — why the gate denies every warning rather than a named list.
- [a-doc-gate-never-opened-the-file-it-was-guarding](a-doc-gate-never-opened-the-file-it-was-guarding.md)
  — the other way this gate reads as stronger than it is: a feature-gated module it never
  compiled. That one is about *coverage*; this one is about *severity*.
