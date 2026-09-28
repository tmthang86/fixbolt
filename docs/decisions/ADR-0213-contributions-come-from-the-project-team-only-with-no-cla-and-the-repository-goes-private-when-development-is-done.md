# ADR-0213 — Contributions come from the project team only, with no CLA, and the repository goes private when development is done

- **Status**: **Accepted — 2026-09-28 (owner's decision)**
- **Date**: 2026-09-28
- **Deciders**: Tran Manh Thang (owner). Written by the architect (Opus).
- **Related**: [ADR-0206](ADR-0206-the-documentation-is-an-mdbook-over-docs-in-place-split-by-diataxis-and-no-cited-file-moves.md)
  decision 4 (outside contributions waited for a licensing ADR; this is it);
  [ADR-0161](ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)
  (the release is a git tag a stranger clones); plan
  [docs-for-embedders](../plans/2026-09-26-docs-for-embedders.md), owner's answer to Q1;
  [prior-art-for-embedders](../reference/prior-art-for-embedders.md) §3 (licence models; CLA
  versus DCO); `STATUS.md` open items 123 (closed by this ADR) and 124 (the switch to private).
- **Out of scope**: **which licence a future version of fixbolt is under**, and when development
  counts as done. Neither is decided here.

## Context

`CONTRIBUTING.md` said contributions from outside the project team were not accepted until a
licensing ADR chose between a contributor licence agreement (CLA) and a Developer Certificate of
Origin (DCO). A first draft of this ADR, written on 2026-09-28 on the owner's first answer,
chose a CLA — Project Harmony's individual and entity templates with Option Five, signed by a
pull-request comment through a CLA-signing workflow — and added `CLA.md` and `CCLA.md`.

The same day, before that draft was merged, the owner changed the decision. fixbolt is built
mainly for the owner's own projects; there is no plan to sell a licence now. A CLA exists to let a
project relicense contributed code or sell it on other terms; with no outside contributor there
is nothing for it to cover, and it would have cost a lawyer's review, a signing workflow running
third-party code on every pull request, and paperwork for entity signers. The draft was withdrawn
unmerged. The research behind it — CLA versus DCO, Harmony, Apache's ICLA and CCLA, how Qt and
Grafana relicensed, the signing tools — is summarised in
[prior-art-for-embedders](../reference/prior-art-for-embedders.md) §3, the starting point if a
licensing decision is ever needed again.

The repository became public on 2026-09-04, and part of the tooling now leans on that. What
public buys, checked on 2026-09-28:

- **Actions minutes.** "GitHub Actions usage is free for self-hosted runners and for public
  repositories that use standard GitHub-hosted runners"; a private repository on GitHub Free gets
  **2,000 minutes a month**, on Pro or Team 3,000
  ([GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions),
  [GitHub's plans](https://docs.github.com/en/get-started/learning-about-github/githubs-plans)).
  This project has already hit the Free limit once: while private, CI stopped starting jobs
  mid-day on 2026-09-04, and making the repository public is what unblocked it.
  `[measured 2026-09-28]` the CI run for the commit that closed phase 5
  ([`36373978028`](https://github.com/tmthang86/fixbolt/actions/runs/36373978028)) was 21 jobs,
  all `ubuntu-latest`, totalling **62 job-minutes** rounded up per job as billing counts them; the
  repository started **851 CI runs** between 2026-09-01 and 2026-09-28
  (`gh api "repos/tmthang86/fixbolt/actions/runs?created=2026-09-01..2026-09-28"`, 864 runs in all
  with Pages). Not every run is a full 62 minutes, but 2,000 minutes are about 32 full runs —
  days of this pace, not a month.
- **GitHub Pages.** "GitHub Pages is available in public repositories with GitHub Free …, and in
  public and private repositories with GitHub Pro, GitHub Team, GitHub Enterprise Cloud, and
  GitHub Enterprise Server"
  ([About GitHub Pages](https://docs.github.com/en/pages/getting-started-with-github-pages/what-is-github-pages)).
  The book at <https://tmthang86.github.io/fixbolt/> is deployed by `.github/workflows/pages.yml`.
  On Pro the site of a private repository is still published publicly; "to publish a GitHub
  Pages site privately, you need to have an organization account" on Enterprise Cloud
  ([GitHub's plans](https://docs.github.com/en/get-started/learning-about-github/githubs-plans)).
- **Private vulnerability reporting.** "Owners and administrators of public repositories can allow
  security researchers to report vulnerabilities" through it
  ([Configuring private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository)).
  `SECURITY.md` names it as the only channel.
- **Protected branches.** GitHub's plans page lists protected branches, required reviewers and
  code owners **in private repositories** under Pro and Team, not Free
  ([GitHub's plans](https://docs.github.com/en/get-started/learning-about-github/githubs-plans)).
  `main` is where CLAUDE.md §8 says nothing is implemented and every merge waits for CI.
- **Anonymous clone.** The `stranger-git` job in `.github/workflows/ci.yml` runs
  `scripts/stranger-check.sh --from git --tag v0.1.0`, which clones fixbolt from GitHub with no
  credentials, as a stranger would (ADR-0161 decision 4); `README.md` *As a user: install the
  crate* tells a user to `cargo add fixbolt --git https://github.com/tmthang86/fixbolt --tag v0.1.0`.
  Both need a repository anyone can read.

## Decision

1. **Contributions come from the internal project team only** — today, the owner alone. A pull
   request from outside the team is closed without review, however small. Issues may still be
   opened while the repository is public.

2. **No CLA and no DCO.** The team's intellectual property is covered by each member's
   employment or contract terms, which live outside this repository and are not recorded in it.
   No sign-off line is required on commits.

3. **The repository stays public until development is done, then goes private.** The account is
   GitHub Free; when the switch comes, the owner decides then whether to upgrade to Pro or Team,
   against the checklist below.

4. **`v0.1.0` stays MIT OR Apache-2.0.** A release already published stays available under those
   terms to anyone who received it, whatever the repository's visibility later
   ([prior-art-for-embedders](../reference/prior-art-for-embedders.md) §3). Every commit pushed
   while the repository is public is under the same terms.

5. **The switch to private is a checklist, owner-owned**, tracked as `STATUS.md` open item 124.
   Each row is done, or decided and recorded, in the same change that makes the repository
   private — not after it:
   - **CI minutes** — trim the workflows to fit 2,000 minutes a month (fewer jobs on
     `pull_request`, heavy gates on `push` to `main` only, or self-hosted runners, which are
     free), or upgrade the plan; at the September pace neither Free nor Pro covers the current
     CI unchanged.
   - **Pages** — switch `.github/workflows/pages.yml` off and remove the book's URL from
     `README.md`, or upgrade to Pro or Team and accept that the book stays public.
   - **`SECURITY.md`** — private vulnerability reporting stops; name another channel or say
     reports come from the team only.
   - **`stranger-git`** — remove the job or give it credentials; an anonymous clone of a private
     repository fails, and the job would be red on every pull request.
   - **`README.md` install lines** — they no longer work for anyone outside; rewrite them for the
     team (an authenticated git URL) or remove them.
   - **`CLAUDE.md` opening paragraph** — "This repository is meant to be open-sourced … Treat
     every commit as already public" stays true while public; revisit it at the switch. The rule
     that nothing confidential enters the repository is kept either way: every earlier commit is
     already public.
   - **`CONTRIBUTING.md`** and `docs/contributing.md` — drop what speaks to readers outside the
     team.
   - **Branch protection on `main`** — on Free it does not apply to a private repository; upgrade,
     or record that `main` is protected by discipline only.

## Consequences

**Good**

- No lawyer, no signing workflow, no third-party action with write access on
  `pull_request_target`, no signatures branch, no entity paperwork.
- Every line of the repository stays the owner's or the team's, so any licence for future
  versions remains the owner's choice without asking a contributor — the reason a CLA was drafted
  is met by having no outside contributor.
- While public, nothing changes: CI is free and unlimited on GitHub-hosted runners, the book is
  published, private vulnerability reporting works, and a stranger can clone the tag.

**Bad — and accepted**

- **No outside fixes.** An embedder who finds a bug can report it but cannot send the fix; the
  team writes every change. Closing a pull request from outside costs the author their work, and
  some goodwill.
- **Going private costs, on Free, most of what the tooling assumes**: CI minutes for about 32
  full runs a month, the book, the security channel, branch protection on `main`, and the
  stranger gate. Keeping them needs Pro or Team, paid, or reworked workflows; which one is the
  owner's call at the switch, and the checklist in decision 5 exists so it is not discovered one
  red run at a time, as it was on 2026-09-04.
- **Going private does not take anything back.** `v0.1.0`, every public commit and any fork made
  while public stay available under MIT OR Apache-2.0. Privacy at the switch hides later work
  only.
- **Team members' IP rests on terms outside the repository.** Nothing in the tree shows who holds
  what; if the team grows, each new member's contract must say it, and the repository cannot check
  that.
- **If outside contributions are ever wanted**, CLA versus DCO is open again and must be decided
  before the first one is merged, in a new ADR that supersedes this one;
  [prior-art-for-embedders](../reference/prior-art-for-embedders.md) §3 is where that starts.
