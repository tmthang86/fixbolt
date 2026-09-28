# Contributing to fixbolt

fixbolt is developed by a project team — today, one developer, its owner. This file is for
**developers on that team**, and for anyone contributing from outside it: how to build the
workspace, which gates a change must pass, and how a change gets into `main`. The engineering rules themselves are in [CLAUDE.md](CLAUDE.md), each
in one place; this file points at them by section and does not restate them.

## Who can contribute

Changes come from the project team, which today is the owner alone. **Contributions from
outside the team come in under a contributor licence agreement (CLA)**, not a Developer
Certificate of Origin
([ADR-0213](docs/decisions/ADR-0213-outside-contributions-come-in-under-a-cla-a-transferable-licence-the-owner-may-relicense-signed-in-the-pull-request.md)):

- **Sign once, before your first pull request is merged.** Read [CLA.md](CLA.md), then post a
  comment on your pull request reading exactly `I have read the CLA Document and I hereby sign
  the CLA`. The CLA check on the pull request turns green, and the signature covers your later
  contributions until a new version of the CLA is published.
- **If your employer, or anyone else, may own rights in your work**, the employer also signs the
  entity agreement, [CCLA.md](CCLA.md), once, naming the GitHub accounts that contribute for it;
  CCLA.md says how.
- You keep the copyright in what you contribute; what you grant, and why, is in CLA.md and
  ADR-0213.

**No outside pull request is merged yet.** Two things must happen first, and until both have, a
signature does not get a pull request merged: **a lawyer has reviewed CLA.md and CCLA.md** (each
carries a *Draft* notice until then), and **the CLA signing check is installed** on this
repository and required on `main`. Both are the owner's, tracked in [STATUS.md](STATUS.md) open
item 124.

### Work that is not yours

Submit only work you wrote, or have the right to grant under the CLA. Code, text or data from
someone else — another project, a specification, a counterparty — goes in a separate pull request
that names its source and licence in its description and says who holds the rights; it is merged
only if its licence allows it and the owner accepts it. Nothing confidential is ever submitted
(see *Before the first change*).

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
its green. CI runs them as named jobs in `.github/workflows/ci.yml`; the ones a change most often
turns red:

| CI job | What a green run proves |
|---|---|
| `gates` | fmt, clippy `-D warnings` and `cargo test --all`, with the FIXT corpus behind `fix50sp2` |
| `no-default-features` | the workspace builds and tests with nothing optional installed, per crate |
| `lint-config`, `indexing-debt` | the no-panic lints still deny, and the indexing debt only goes down |
| `bench` | the benchmarks run, every `benches/alloc.rs` reads zero, machine-independent bounds hold |
| `no-kernel-sleep`, `standard-blocks` | `hft` never sleeps in the kernel; `standard` gives the core back |
| `links`, `book` | no dead internal link; the book builds without a warning and its rendered links hold |

The job names are the ones a pull request shows; the list of jobs in `ci.yml` is the complete
one.

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
