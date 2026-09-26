# Contributing to fixbolt

This file is for **developers on the fixbolt project team**. It says how to build the workspace,
which gates a change must pass, and how a change gets into `main`. The engineering rules
themselves are in [CLAUDE.md](CLAUDE.md), each in one place; this file points at them by section
and does not restate them.

## Who can contribute

Changes come from the project team. **Contributions from outside the team are not accepted
yet**: a pull request from outside the team is not merged until a licensing ADR decides between
a contributor licence agreement (CLA) and a Developer Certificate of Origin (DCO). That ADR is due
before the first outside contribution is accepted
([ADR-0206](docs/decisions/ADR-0206-the-documentation-is-an-mdbook-over-docs-in-place-split-by-diataxis-and-no-cited-file-moves.md)
decision 4).

A security problem is never reported in an issue or a pull request: see [SECURITY.md](SECURITY.md).

## Before the first change

**Read, by section:** [ARCHITECTURE.md](ARCHITECTURE.md) (where things live and what must not
break), [CLAUDE.md](CLAUDE.md) §1 and §2 (plan first; the ten non-negotiables and the machine
check behind each), then the [DESIGN.md](docs/DESIGN.md) section your change touches and the ADRs
it names. [docs/internals/](docs/internals/README.md) says which file of a crate holds what.
[STATUS.md](STATUS.md) says where the work stands and what is not proven.

**Everything in this repository is treated as public.** Nothing confidential is committed — no
exchange specification, no capture, no counterparty configuration. `.gitignore` is a safety net;
the control is you, before `git add` (CLAUDE.md, preamble).

## Toolchain and assets

- **Rust**: `rust-toolchain.toml` pins the toolchain, with `rustfmt` and `clippy`, and `rustup`
  uses it inside the checkout. It is upgraded deliberately; the file's header says why. The minimum supported Rust version is
  `rust-version` in the root `Cargo.toml`, and the `package` CI job builds on it. `fuzz/` needs
  nightly and is its own workspace.
- **Test oracles**: two scripts fetch pinned third-party assets into `vendor/`, which is
  gitignored. **Never commit anything under `vendor/`**
  ([ADR-0001](docs/decisions/ADR-0001-relationship-to-quickfix.md); CLAUDE.md §8).

  ```sh
  scripts/fetch-quickfix-assets.sh   # before `cargo test --all`: the 59 acceptance definitions
                                     # and QuickFIX's generated C++, the oracle for the tables
  scripts/fetch-sbe-assets.sh        # before `cargo clippy --all-targets`: the SBE 1.0 spec
                                     # and reference implementation the `sbe` tests read
  ```

  The crates an application depends on build without either; the test suite does not.
- **Documentation**: `python3` for the scripts under `scripts/`, and mdBook at the version the
  `book` job in `.github/workflows/ci.yml` pins, to build the site from `book.toml`.
- **Linux** for anything that measures, and for the mode checks: `hft`, `affinity`, `shard`,
  kTLS and `io-uring` code is Linux-only, and a `cargo check` on macOS does not compile it.

## The gates

Which gates a change owes depends on what it touches — the table in **CLAUDE.md §7** decides, and
widening scope means naming more cases, not running everything. The commands, grouped the way that
table groups them:

```sh
# Every change
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo test --all --no-default-features
scripts/check-no-optional-deps.sh       # the same question per crate: cargo unifies features

# A session-layer change: the 59 acceptance definitions, and the FIXT corpus
cargo test -p fixbolt-session --test score
cargo test -p fixbolt-engine --test wire
for p in fixbolt-codec fixbolt-dict fixbolt-session fixbolt-engine; do
  cargo test -p "$p" --tests --features fix50sp2
done

# A hot-path change: the Criterion suite and every benches/alloc.rs
scripts/bench.sh

# A dispatch, transport or engine-thread change: benches/dispatch.rs, then tools/w2w on Linux
cargo bench -p fixbolt-engine --bench dispatch

# A change to the wait strategy, readiness or the mode split: both modes, on Linux
scripts/check-no-kernel-sleep.sh
scripts/check-no-kernel-sleep-by-ctxt.sh
scripts/check-standard-gives-the-core-back.sh

# A documentation change
python3 scripts/check-links.py
python3 scripts/gen-book-summary.py --check
mdbook build
python3 scripts/check-links.py --rendered target/book
```

The machine check behind each non-negotiable, and what each script cannot see, is the table in
CLAUDE.md §2 *Machine checks*; read a script's header before trusting its green. CI runs the same
gates as named jobs (`gates`, `no-default-features`, `bench`, `no-kernel-sleep`,
`standard-blocks`, `links`, `book`, and others) in `.github/workflows/ci.yml`.

Two habits the gates depend on (CLAUDE.md §7, §10): **read the output, not the exit status**, and
**prove a guard by reversal** — break it, see it red on the assertion you meant, restore it.

## How a change gets in

1. **Plan first.** No code without an approved plan, however small (CLAUDE.md §1). A plan is
   `docs/plans/YYYY-MM-DD-<topic>.md`, from [docs/plans/_template.md](docs/plans/_template.md),
   and waits for the owner's approval. A typo, comment or link repair needs no plan.
2. **An ADR for a decision** that is expensive, hard to reverse or contested, in
   [docs/decisions/](docs/decisions/) (CLAUDE.md §5). An accepted ADR is superseded, never edited.
3. **A branch per plan, never `main`.** Open the pull request **as a draft at the first commit**:
   CI runs only on pull requests and on pushes to `main`, so a branch without one has no CI
   (CLAUDE.md §8).
4. **Commit and push at every step that ends green.** Gates must be green for each commit, not
   only for the branch tip.
5. **Review, then merge** once the plan's exit criteria are met and CI is green on the closing
   commit, named by run id. The full checklist is CLAUDE.md §9, *Definition of Done*.

If the plan turns out wrong partway, stop, fix the plan and get it re-approved; never diverge
silently (CLAUDE.md §1).

This repository is built with Claude Code, with one role per model — a managing session that
plans, routes and verifies, and subagents that design, implement and review. CLAUDE.md §12
describes the roles and how a finding is verified before it is acted on.

## Commits

- [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) (`feat(engine): …`,
  `fix(session): …`, `docs(plan): …`).
- **One commit is one coherent change, including its documentation.** The body records what was
  measured — machine, OS settings, command — and what was *not* proven (CLAUDE.md §8).
- `cargo fmt` and `cargo clippy -D warnings` are clean before every commit (CLAUDE.md §6).

## Documentation goes in the same commit

A stale document is worse than none. Docs change in the same commit as the code they describe,
and **CLAUDE.md §4** has the two tables that decide where: which file answers what, and which
change obliges which update — walk the second one row by row before closing a plan. A trap that
cost you time goes into [docs/reference/](docs/reference/) at once, with a regression test.

Every page under `docs/` except `docs/plans/` is part of the documentation site; a new page is
added to `docs/SUMMARY.md` (generated for `reference/`, `internals/` and `decisions/` by
`scripts/gen-book-summary.py`).

## Language

Code, identifiers, comments, commit messages and every document describing the system are in
English. Plans under `docs/plans/`, addressed to the owner, are in Vietnamese (CLAUDE.md §6).
