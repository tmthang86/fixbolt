# Contributing to fixbolt

fixbolt is developed by a project team — today, one developer, its owner. This file is for
**developers on that team**: how to build the workspace, which gates a change must pass, and how
a change gets into `main`. The engineering rules themselves are in [CLAUDE.md](CLAUDE.md), each
in one place; this file points at them by section and does not restate them.

## Who can contribute

Changes come from the internal project team only, which today is the owner alone. **A pull
request from outside the team is closed without review**, however small; a bug found outside the
team is reported as an issue, and the team writes the fix. There is no contributor licence
agreement (CLA) and no Developer Certificate of Origin (DCO) sign-off: a team member's
intellectual property is covered by their employment or contract terms, outside this repository
([ADR-0213](docs/decisions/ADR-0213-contributions-come-from-the-project-team-only-with-no-cla-and-the-repository-goes-private-when-development-is-done.md)).

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
  `book` job in `.github/workflows/docs.yml` pins, to build the site from `book.toml`.
- **Linux** for anything that measures, and for the mode checks: `hft`, `affinity`, `shard`,
  kTLS code is Linux-only, and a `cargo check` on macOS does not compile it.

## The gates

Which gates a change owes depends on what it touches: the table in **CLAUDE.md §7** decides, and
widening scope means naming more cases, not running everything. Its first row, run on every
commit:

```sh
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo test --no-default-features
```

Every other row — session, hot-path, dispatch, wait-strategy and documentation changes — is in
CLAUDE.md §7, with the command for each; the machine check behind each non-negotiable, and what
each script cannot see, is CLAUDE.md §2 *Machine checks*. Read a script's header before trusting
its green. CI runs them as named jobs in two workflows, `.github/workflows/docs.yml` (`Docs`) and
`.github/workflows/ci.yml` (`CI`); the ones a change most often turns red:

| CI job | Tier | What a green run proves |
|---|---|---|
| `gates` | fast | fmt, clippy `-D warnings` and `cargo test --all`, with the FIXT corpus behind `fix50sp2` |
| `no-default-features` | fast | the workspace builds and tests with nothing optional installed, per crate |
| `lint-config`, `indexing-debt` | fast | the no-panic lints still deny, and the indexing debt only goes down |
| `bench` | full | the benchmarks run, every `benches/alloc.rs` reads zero, machine-independent bounds hold |
| `no-kernel-sleep`, `standard-blocks` | full | `hft` never sleeps in the kernel; `standard` gives the core back |
| `book` (`Docs`) | every PR | no dead internal link; the book builds without a warning and its rendered links hold; `docs/GETTING-STARTED.md`'s code builds against the release tag |

The job names are the ones a pull request shows; the lists of jobs in the two workflow files are
the complete ones. Which job runs when is [ADR-0214](docs/decisions/ADR-0214-ci-runs-a-fast-tier-on-every-pull-request-push-a-full-tier-before-a-plan-closes-and-a-docs-only-diff-runs-only-the-docs-workflow.md),
tabled in [DESIGN.md §6](docs/DESIGN.md#6-gates):

- **`Docs`** runs on every pull request and every push to `main`.
- **`CI`'s fast tier** runs on every push to a pull request that changes anything besides
  documentation. A pull request that changes only files under `docs/`, `*.md` at the root or
  `book.toml` runs `Docs` alone — except `docs/CONFIGURATION.md` and `docs/reference/prior-art.md`,
  which tests read and which therefore count as code.
- **`CI`'s full tier** adds the other eleven jobs, on a push to `main`, a manual
  `workflow_dispatch`, or a pull request carrying the label **`full-ci`**. The label removes
  itself at the end of the run it caused, so the next push runs the fast tier; attach it again to
  run the full tier again.

Two habits the gates depend on (CLAUDE.md §7, §10): **read the output, not the exit status**, and
**prove a guard by reversal** — break it, see it red on the assertion you meant, restore it.

## How a change gets in

1. **Plan first.** No code without an approved plan, however small (CLAUDE.md §1). A plan is
   `docs/plans/YYYY-MM-DD-<topic>.md`, from [docs/plans/_template.md](docs/plans/_template.md),
   and waits for the owner's approval. A typo, comment or link repair needs no plan.
2. **An ADR for a decision** that is expensive, hard to reverse or contested, in
   [docs/decisions/](docs/decisions/) (CLAUDE.md §5). An accepted ADR is superseded, never edited.
3. **A branch per plan, never `main`.** Open the pull request **as a draft at the first commit**:
   CI runs only on pull requests, on pushes to `main` and on a manual dispatch, so a branch
   without a pull request has no CI (CLAUDE.md §8).
4. **Commit and push at every step that ends green.** The fast tier must be green for each
   commit, not only for the branch tip.
5. **Review, then merge** once the plan's exit criteria are met and the closing commit has a green
   **full-tier** `CI` run and a green `Docs` run: attach `full-ci` before pushing it, then name
   both run ids with `scripts/ci-evidence.sh <sha>`, which refuses a run whose full-tier jobs were
   skipped. The full checklist is CLAUDE.md §9, *Definition of Done*.

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
