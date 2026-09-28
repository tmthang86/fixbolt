# ADR-0214 — CI runs a fast tier on every pull-request push, a full tier before a plan closes, and a docs-only diff runs only the docs workflow

- **Status**: **Proposed — 2026-09-28.** Written by the architect (Opus) at the manager's request,
  from the four changes the owner approved on 2026-09-28 (in chat). Accepting it is the owner's.
- **Date**: 2026-09-28
- **Deciders**: Tran Manh Thang. Written by the architect (Opus).
- **Related**: ADR-0213 (on branch `docs/cla-adr`, pull request #130: the repository goes
  private on GitHub Free, 2,000 Actions minutes a month);
  plan [2026-09-28-ci-tiers](../plans/2026-09-28-ci-tiers.md);
  [ADR-0206](ADR-0206-the-documentation-is-an-mdbook-over-docs-in-place-split-by-diataxis-and-no-cited-file-moves.md)
  decision 7 (the `book` job is the documentation gate on every pull request);
  [ADR-0087](ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md)
  decision 5 (the socket corpus under contention, a step of `gates`);
  [ADR-0162](ADR-0162-the-semver-baseline-is-the-newest-release-tag-head-descends-from-and-zero-checks-are-excused-only-by-a-major-bump.md)
  (`semver`, blocking). **No Accepted ADR's substance is changed**: every job keeps what it
  asserts; this ADR decides *when* each job runs, merges two pairs of jobs into one job each
  without changing a step, and removes one advisory job.

## Context

`.github/workflows/ci.yml` runs 21 jobs on every `pull_request` push and every push to `main`
(`on:` block, lines 23–26 at `b3c6477`). `[measured 2026-09-28]` from the GitHub API:

- Run 36373978028 (the phase-5 closing commit `bcbaa22`): 21 jobs, **53.5 job-minutes of wall
  time, 62 billed** — GitHub "rounds the minutes and partial minutes each job uses up to the
  nearest whole minute" ([actions runner pricing](https://docs.github.com/en/billing/reference/actions-runner-pricing)).
  Nine jobs run under a minute and are billed a whole one each; `bench` (13), `gates` (10) and
  `semver` (6) are half the bill.
- September 2026 (to the 28th): 865 workflow runs, 853 of them `CI`. In the steady window
  14–28 September (after `push:` was narrowed to `main`): 313 pull-request runs and 64 pushes to
  `main` in 15 days. Of the 110 pull requests opened since 1 September whose file list the API
  returned, **22 changed documentation only** (the definition in decision 1), and their branches
  drew 43 CI runs.

The repository is public today, where Actions minutes are free
([billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions)).
ADR-0213 makes it private on GitHub Free: 2,000 minutes a month. At September's cadence the
current workflow bills roughly 47,000 minutes a month.

What protects `main` today, read 2026-09-28: `gh api repos/tmthang86/fixbolt/branches/main/protection`
returns **404** (no classic protection); ruleset 23642557 `main-branch-protect` holds only
`deletion` and `non_fast_forward`. **No status check is required.** On a private repository on
GitHub Free neither branch protection nor rulesets are available at all ("available in public
repositories with GitHub Free … and in public and private repositories with GitHub Pro, GitHub
Team …", [about protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches);
rulesets: [about rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets)).

## Decision

### 1. A docs-only diff runs only the docs workflow, decided by a workflow-level `paths` filter

The documentation jobs move to a new workflow, `.github/workflows/docs.yml`, which runs on
every pull request and every push to `main`, unfiltered. `ci.yml` gets the same `paths` list on
`push` and on `pull_request`:

```yaml
paths:
  - '**'
  - '!docs/**'
  - '!*.md'
  - '!book.toml'
  - 'docs/CONFIGURATION.md'
  - 'docs/reference/prior-art.md'
```

**Documentation** is therefore: anything under `docs/`, a `*.md` file at the repository root
(`*` does not match `/`), and `book.toml` — **minus** each file a code gate reads at run time.
Two exist at `b3c6477`: `docs/CONFIGURATION.md`, read by the unit test in
`crates/engine/src/settings.rs:2113` (`DOC_PATH`), and `docs/reference/prior-art.md`, read by
`crates/session/tests/drop_reason.rs:314`. Both run under `cargo test --all` in `gates`, so a
change to either must start `ci.yml`. A later match re-includes an excluded path ("A matching
positive pattern after a negative match will include the path again",
[workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax)).
Everything else is code, including `crates/*/README.md` (the `fixbolt` crate's README is its
crate documentation, `crates/library/src/lib.rs:1`, and so a doctest), `scripts/`, `benches/`,
`NOTICE`, `LICENSE-*`, `Cargo.*`, `deny.toml`, `rust-toolchain.toml` and `.github/`.

`scripts/check-ci-paths-filter.sh` (new, in `docs.yml`) holds the list to the code: every
repository-root-relative `docs/…` or root `*.md` path that a file under `crates/`, `tools/`,
`examples/`, `benches/`, `fuzz/` or `spikes/` reads must appear as a re-include line, and the
`push` and `pull_request` lists must be identical.

**Why the filter, not a `changes` job** (e.g. `dorny/paths-filter` gating the others): a job
that only classifies is billed a whole minute on every run — ~600 minutes a month at September's
pull-request cadence, 30 % of the budget, for a question GitHub answers before any runner starts.
**The known cost of the filter** is the one the `changes` pattern exists to avoid: "If a workflow
is skipped due to path filtering … checks associated with that workflow will remain in a
'Pending' state", while a job skipped by `if` "reports 'Success'"
([troubleshooting required status checks](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/collaborating-on-repositories-with-code-quality-features/troubleshooting-required-status-checks#handling-skipped-but-required-checks)).
No check is required today, and none can be once the repository is private on Free. If one is
ever required again, it is a `docs.yml` job, or this decision is superseded.

### 2. Two tiers inside `ci.yml`

**Fast tier** — every `pull_request` push (`opened`, `synchronize`, `reopened`):
`gates` (fmt · clippy · test, the 59 acceptance definitions, the FIXT corpus, the socket corpus
under contention, the codegen and affinity steps — unchanged), `no-default-features`,
`lint-config` (decision 3), `indexing-debt`, `dict-no-vendor`. Plus `docs.yml`'s `book`.

**Full tier** — push to `main`, `workflow_dispatch`, and a pull request carrying the label
`full-ci`: the fast tier plus `bench`, `semver`, `feature-sets`, `deny`, `interop`,
`interop-qfj`, `tls`, `no-kernel-sleep`, `standard-blocks`, `package`, `fixp-spike`.

Mechanism: `pull_request.types: [opened, synchronize, reopened, labeled]` (the default is the
first three only, [events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request)).
Each full-tier job carries
`if: (github.event_name != 'pull_request' || contains(github.event.pull_request.labels.*.name, 'full-ci')) && !(NOOP)`
and each fast-tier job `if: !(NOOP)`, where `NOOP` is
`github.event.action == 'labeled' && github.event.label.name != 'full-ci'` — any other label
starts a run whose every job is skipped and never takes a runner. The label is **sticky**: while
it is on the pull request every push runs the full tier.

Concurrency: the group becomes
`${{ github.workflow }}-${{ github.head_ref || github.ref_name }}` plus, for a `NOOP` event only,
`-noop-${{ github.run_id }}`. `head_ref || ref_name` puts a dispatch on a branch in the same
group as that branch's pull request, so one cancels the other rather than both billing; the
`noop` suffix keeps a label nobody meant for CI from cancelling a real run ("any currently
running job or workflow in the same concurrency group" is cancelled,
[concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)).

**Which commit a green is for.** The fast tier is green for every commit on a branch; the full
tier is green for the commit that closes a plan, because the manager adds `full-ci` before
pushing it (or dispatches on it). `scripts/ci-evidence.sh <sha>` (new) names the `CI` and `Docs`
runs for a commit and refuses one in which any full-tier job was skipped.

### 3. Four short jobs become two; the rest keep their own runner

- **`links` + `book` + `stranger-git` → `book`** in `docs.yml` (6 s + 17 s + 18 s at
  36373978028), checked out at `fetch-depth: 0`, the deepest either needed. `stranger-git` joins
  the documentation job because what it proves is that `docs/GETTING-STARTED.md` still runs
  against the tagged release, and its own comment rules out any path from the runner's checkout
  into its build. It thereby moves from the full tier into every pull request, at no billed cost.
- **`lint-config` + `script-logic` → `lint-config`** (43 s + 6 s). Neither fetches `vendor/`, so
  `check-dict-spec-pin.sh` still takes its sha256-only path there, as the step's comment says;
  `script-logic`'s checks are pure and its own comment makes the runner's kernel irrelevant.
- Every check step after setup in a merged job carries `if: ${{ !cancelled() }}`, so one red step
  does not hide the steps after it; the job is still red.

**Not merged**, because the separate runner is part of what the job proves: `dict-no-vendor` and
`package` (a runner that never saw `vendor/`), `no-default-features` ("its own runner, so it
cannot inherit anything a previous step put there"), `tls`, `interop-qfj`, `no-kernel-sleep`,
`standard-blocks` (the runner's kernel is asserted first, in its own words). `indexing-debt` is
not merged either: joined to `lint-config` it would take the pair past 60 s and save no billed
minute, while putting `vendor/` on the runner that relies on its absence.

### 4. `clippy-latest-stable` is removed

It is advisory (`continue-on-error: true`), costs a minute a run, and the owner chose to drop
it. The toolchain pin in `rust-toolchain.toml` stays; its comment stops naming the job.

## Alternatives considered

- **A `changes` job gating every other job** (`dorny/paths-filter` pinned by SHA, or `gh api
  …/pulls/N/files`): keeps required checks satisfiable and lets one workflow hold both tiers, but
  bills a minute on every run and adds a third-party action. Rejected on cost while no check is
  required (decision 1).
- **`paths-ignore`**: cannot re-include the two files tests read; `paths` with `!` can.
- **A one-shot label** (a final job removes `full-ci`): each closing push would need the label
  added again, and removing it needs `pull-requests: write`. Rejected for the sticky label the
  owner approved; revisited if a forgotten label shows in the bill.

## Consequences

**Good.**
- Per push, billed minutes (each job rounded up, from run 36373978028): a docs-only pull request
  **62 → 1**; a code pull request **62 → 17** (`ci.yml` fast 16 + `docs.yml` 1); a full run
  **62 → 58** (57 + 1); an unrelated label **0**.
- A docs-only merge to `main` starts no `ci.yml` run, so it does not cancel the full run of the
  code merge before it.
- A `docs/GETTING-STARTED.md` edit is judged against the tagged release on every pull request,
  documentation-only ones included.

**Bad.**
- **An intermediate commit on a branch is no longer checked by** `bench` (non-negotiable 1's
  machine check), `no-kernel-sleep` / `standard-blocks` (4), `feature-sets`, `deny`, `semver`,
  `package`, the interop jobs or `tls`. A regression there is found at the closing commit, not at
  the commit that made it. CLAUDE.md §8 "gates must be green for that commit" becomes "the fast
  tier for every commit, the full tier for the commit that closes".
- **The four changes do not fit 2,000 minutes at September's cadence.** Upper bound, every run
  completing: ~19,000 minutes a month after (from ~47,000), of which code-pull-request pushes are
  ~9,500 and full runs on `main` ~6,000. Fitting the budget needs a lever outside this ADR: fewer
  pushes, a build cache (the builds are cold — `gates` spends 195 s on the affinity step alone),
  a cheaper `main`, or a plan with more minutes.
- A docs-only change can break `package`'s `stranger-check.sh --from packaged`, which pastes
  `docs/GETTING-STARTED.md` against the branch's crates; that is found on the next full run.
- Evidence becomes two run ids per commit (`CI`, `Docs`), and a green `CI` run can be a fast or a
  no-op one. `scripts/ci-evidence.sh` exists because a reader will otherwise name the wrong one.
- If a required status check is ever configured on `ci.yml`, a docs-only pull request blocks
  forever (decision 1).
- A pull request with more than 3,000 changed files whose code files fall outside the first 3,000
  would skip `ci.yml` ([workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax)).
  Not reachable at this repository's size; stated, not guarded.

## Sources

- GitHub Docs, *Actions runner pricing* — per-job rounding up to the whole minute.
  <https://docs.github.com/en/billing/reference/actions-runner-pricing>
- GitHub Docs, *GitHub Actions billing* — free on public repositories; 2,000 minutes on Free.
  <https://docs.github.com/en/billing/concepts/product-billing/github-actions>
- GitHub Docs, *Workflow syntax* — `paths` with `!` and order; three-dot diff for pull requests;
  the 3,000-file limit; a path-skipped workflow leaves checks Pending.
  <https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax>
- GitHub Docs, *Troubleshooting required status checks* — a job skipped by `if` reports Success.
  <https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/collaborating-on-repositories-with-code-quality-features/troubleshooting-required-status-checks>
- GitHub Docs, *Events that trigger workflows* — `pull_request` default types; `labeled`; no run
  on a pull request with a merge conflict.
  <https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows>
- GitHub Docs, *Control workflow concurrency* — `cancel-in-progress`, `head_ref || run_id`.
  <https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency>
- GitHub Docs, *About protected branches*, *About rulesets* — not available on private Free.
- Measured: `gh api repos/tmthang86/fixbolt/actions/runs/36373978028/jobs`; `gh api …/actions/runs
  -f created=2026-09-01..2026-09-30`; `gh pr list --search created:>=2026-09-01 --json files`.
