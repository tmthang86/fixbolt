# A shallow clone emptied the field the check read

`[measured 2026-09-13]`

An informational CI step printed a blank line where it expected proof of a merge. Nothing was
wrong with the merge; the field it read had already been emptied before the step ran.

## What was being checked, and why

The `links` job's first step exists to turn GitHub's documented `pull_request` behaviour into
this repository's own measurement rather than a claim quoted from a vendor: on a `pull_request`
event, `actions/checkout` with no `ref:` checks out `refs/pull/<n>/merge` — the merge of the
head into the base, not the head commit itself. The step prints what actually landed in the
working tree so that behaviour is observed here, once, instead of trusted from documentation.
It asserts nothing; it is a line to read, not a gate to fail.

## What it printed

Run 34741376011, job "No dead internal links" (job id 103681528262), PR #67:

```
HEAD is now at 2e85121 Merge 82411efd50ae9eb18ceda7588f43114b43bc4d21 into 8b4763e162524ec2d430855618340d77d4f1890f
event        = pull_request
github.sha   = 2e85121d5d0966068ed5319be9b77891b1e8528d
pr head sha  = 82411efd50ae9eb18ceda7588f43114b43bc4d21
pr base sha  = 8b4763e162524ec2d430855618340d77d4f1890f
HEAD    = 2e85121d5d0966068ed5319be9b77891b1e8528d
parents =
```

The step's own `git log` line proved nothing: `parents` came back empty. The proof that actually
landed was `actions/checkout`'s own log line above it, `HEAD is now at 2e85121 Merge <head> into
<base>` — a line the step never asked for.

## Why the field was empty

`actions/checkout` defaults to `fetch-depth: 1`. A depth-1 fetch grafts the tip commit into a
local repository that has none of its history: the object is present, but its parents are not,
because they were never fetched. `%P` asks the local object store for a commit's parents, and a
grafted commit has none to give. The commit really is a merge — GitHub built it as one — the
local clone simply cannot see the edge to either parent.

## The generalised lesson

**The depth of your checkout decides which fields still exist to be asserted on.** A check can
read a field that the fetch, clone, or fixture setup already emptied, and the blank comes back
looking exactly like a negative result. The failure is not in the subject under test — the merge
happened — and not in the assertion's logic — asking a commit for its parents is a reasonable
question. The evidence was removed before the check ran, by a step earlier in the pipeline that
the check never looks at.

This bites in two ways, and the second is the one worth losing sleep over:

- **As a red about something that is not wrong.** If a step had asserted `parents` non-empty, it
  would have failed on a perfectly good merge, and whoever read the failure would have gone
  looking for a bug in the merge rather than in the fetch depth.
- **As a green, if the assertion happens to be phrased so that "empty" satisfies it.** A check
  for "no unexpected parents" or "at most one parent" would pass on a blank field for exactly the
  wrong reason — it was never handed the data that would let it be wrong.

## What was done instead

`fetch-depth` was deliberately **not** raised. A deeper fetch, paid by every job on every run, to
prove something the commit's own subject line already proves, is a cost with no return. Instead:

- `%s` — the subject line, `Merge <head> into <base>` for this kind of commit — carries the same
  fact and survives a shallow clone, because it is metadata on the grafted commit itself, not a
  reference to an ancestor.
- `git rev-parse --is-shallow-repository` is printed beside it, so a blank `parents` line can
  never again be misread as "this was not a merge" — the shallow flag says why the field is
  blank, in the same block that shows it blank.

## The thing that actually proved it, and was not asked for

`actions/checkout` printed `HEAD is now at 2e85121 Merge <head> into <base>` of its own accord,
as part of its normal checkout log — not because the step requested it. That line is what
actually demonstrated the behaviour under test. A proof you did not ask for is a proof you
cannot rely on: the action's log format is not a contract, and a future version could reword or
drop that line without notice. The fact is now captured by a field the step asks for itself,
rather than borrowed from another tool's incidental output.

## Related

- [a-doc-gate-never-opened-the-file-it-was-guarding](a-doc-gate-never-opened-the-file-it-was-guarding.md)
  — the same family from the input side: a gate that was never handed the data it needed to have
  an opinion, and its silence looked exactly like approval.
- [reading-the-output-you-grepped-for](reading-the-output-you-grepped-for.md) — a different way
  evidence goes missing before a check reads it: a filter built from what the run was expected to
  say, applied before the read rather than after.

[to testing-skills]
