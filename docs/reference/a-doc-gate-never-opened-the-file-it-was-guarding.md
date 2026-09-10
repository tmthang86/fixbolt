# A documentation gate that never opened the file it was guarding

`[measured 2026-09-10]` **`[to testing-skills]`**

## The shape

A CI job compiled the whole workspace's documentation with broken links promoted to errors.
It had been green for a week. One module in that workspace sits behind a feature flag that is
**off by default**, so the job never compiled that module, never read its documentation, and
therefore could never fail on it.

The first line of that module's file carried a broken link for as long as the file had existed.

```
//! TLS as a second [`Transport`] implementation, …
                      ^^^^^^^^^ no item named `Transport` in scope
```

The job's own name — *"rustdoc has no broken intra-doc links"* — was a claim about the
workspace. What it actually checked was *the part of the workspace that the default feature
set compiles*, and nothing in the job said so.

## How it was found, and it was not by looking for it

Not by an audit. A **different** broken link, in a file the default build *does* compile,
turned the job red. Fixing that meant running the command locally, and running it locally
meant running it a second time with the feature on — out of ordinary caution about a
feature-gated module, not out of suspicion. The second run failed immediately.

So the finding was a by-product of a failure elsewhere. Without that unrelated red, nothing
was scheduled to ever look.

**And the by-product was larger than the thing that caused it.** Adding `--all-features` to
the job surfaced two more broken links in a second feature-gated file. One of them pointed at
a type that **had been deleted**: the documentation still linked to it, and every green run
of the gate had been about a smaller set of files than the one being changed.

## Why the gate could not see it

The rule is the ordinary one and it is easy to state:

> A checker is handed an input. A feature flag decides what is in that input. A checker that
> is never handed a file cannot have an opinion about it, and its silence looks exactly like
> approval.

The failure mode is not that the tool is weak. The tool worked perfectly on everything it was
given. The defect is in the **invocation**, and the invocation is the part nobody re-reads
after it goes green the first time.

## The fix, and why both invocations are kept

The job now runs the same command twice: once with the default feature set, once with
everything on.

Not one command with everything on. **A link can resolve under one feature set and fail under
another**, in both directions — an item that exists only when a feature is enabled, linked
from documentation that exists whether or not it is. That is exactly the second bug found
here, in the opposite direction from the first: a doc comment on an ungated type linked to a
function that only exists when the feature is on, so the *default* run failed and the
all-features run was fine.

Running only the wider set would have hidden it just as thoroughly as running only the
narrower one hid the others. **Neither invocation is a superset of the other**, and treating
one as "the thorough version" of the other is the mistake.

## The reversal, run rather than described

The rule this page ends on — *make it fail on purpose* — is not advice given from a
distance. A broken link was pasted into the feature-gated module and each invocation was run
against it:

| Invocation | Result |
|---|---|
| default feature set | **exit 0 — green.** It never compiles the module, so it has no opinion |
| `--all-features` | **exit 101 — red**, `unresolved link to ThisTypeDoesNotExist` |

That table is the blind spot **demonstrated**. Without it, the first row would have been a
claim about what the gate does not see, argued from how feature flags work — which is the
same kind of reasoning that produced the wrong prediction in the sibling case, and was wrong
there.

## What generalises

1. **A gate's name describes its intent; its command line describes its coverage.** When
   they disagree, everybody reads the name. Write the scope into the job, beside the command,
   where somebody changing the command will see it.
2. **Every conditional-compilation switch is a hole in every static check.** Enumerate the
   switches once and ask, per gate: *is this gate run under each of them?* The answer is
   usually no, and usually nobody has asked.
3. **Neither feature set is the thorough one.** The instinct is to run the maximal
   configuration and call the narrower one redundant. Both directions produce real failures,
   and the narrow one is the configuration most users actually get.
4. **A green gate that has never been handed the file is indistinguishable from a passing
   one.** The only way to tell them apart is to make it fail on purpose — feed it something
   broken from inside the gated region and confirm it goes red. That is the reversal, and it
   is the step that was skipped when the gate was written.
5. **Byproduct findings deserve their own write-up.** This one arrived while fixing an
   unrelated failure. It would have been natural to fix both quietly in the same commit and
   record only the one that had a ticket. The second is the more expensive of the two, and it
   had no ticket precisely because nothing could see it.

## Sibling cases in this repository

- [feature-flags-unify-across-a-workspace](feature-flags-unify-across-a-workspace.md) — the
  same class from the other side: a flag under test switched back on by a sibling crate, so
  the gate measured a build that never happened.
- [an-allow-at-the-top-of-a-file-silenced-the-whole-crate](an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md)
  — a ratchet whose counter read correctly while its enforcing half was off.
- `STATUS.md` item 57 — a licence gate that cannot see dev-dependencies and prints `ok`
  rather than saying so.

Three of the four share one sentence: **the gate reported on a set smaller than the set it
was believed to cover, and the number it printed was correct for the set it actually saw.**
