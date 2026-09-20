# A conflicting pull request has no CI

**A green run is evidence; the absence of a run is not a green.** Before trusting any pull
request, check that a run *exists* for the commit, not only that nothing on it is red.

## What happened

`[measured 2026-09-20]` pull request #88 was opened as a draft on
`plan/the-contended-corpus-and-sharded-recovery`, already five commits deep.

```
$ gh pr checks 88
no checks reported on the 'plan/the-contended-corpus-and-sharded-recovery' branch
$ gh run list --branch plan/the-contended-corpus-and-sharded-recovery --limit 5
(nothing)
```

Not a failed run — no run. Closing and reopening the pull request did not start one either.
The cause:

```
$ gh pr view 88 --json mergeable -q .mergeable
CONFLICTING
```

`.github/workflows/ci.yml` triggers `on: pull_request`, which GitHub evaluates against the
merge ref (`refs/pull/88/merge`); a conflicting pull request has no merge ref to evaluate
against, so the workflow never runs — silently, with no failed check to notice. After
`git merge origin/main` and resolving one conflict, `mergeable` read `MERGEABLE` and a run
started immediately (id `35506825366`).

The branch had been cut from an earlier state of another branch that later gained three more
commits — an ordinary stacked-pull-request situation. It cost extra because the draft was
opened after the fifth commit rather than the first (`CLAUDE.md` §8): opened at the first
commit, the conflict would have surfaced at the first push, not at merge time.

## Telling the three states apart

| State | Command | What it reads |
|---|---|---|
| No run | `gh run list --branch <branch> --limit 5` | empty |
| Running | `gh pr checks <n>` | `pending` / `in_progress` |
| Failed | `gh pr checks <n>` | `fail`, with a run id to open |
| Why no run | `gh pr view <n> --json mergeable -q .mergeable` | `CONFLICTING` explains it; `MERGEABLE` does not |

## What to do

- Open the draft pull request at the branch's **first** commit (§8), not after several — every
  commit then gets checked as it lands, and a conflict surfaces at the push that caused it.
- Whenever `gh pr checks` reports no checks, run `gh pr view --json mergeable` before doing
  anything else. `CONFLICTING` means the silence is expected, not a broken pipeline; rebase or
  merge `main` in, then re-check that a run started.

## Regression test

None is possible here, and claiming one would be worse than admitting the gap: this is
GitHub's own scheduling of `pull_request` runs against a merge ref, not code in this
repository, so no test in this repo can reverse it red and green. The check above —
`gh pr view <n> --json mergeable` whenever `gh pr checks` is silent — is a manual step, not a
guard, and stands in its place.
