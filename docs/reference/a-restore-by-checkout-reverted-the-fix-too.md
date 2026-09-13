# A restore by checkout reverted the fix too

> `[measured 2026-09-13]` A reversal was run inside a working tree whose fix was not
> yet committed, and the plan's restore step put the file back to the last commit —
> which was from before the fix.
>
> **`[to testing-skills]`**

## What happened

A test was written to guard a number quoted in a document: it counts the matching
lines in a source file and fails if the document quotes anything else. The work was
done the usual way — test first, red against the old document, then edit the document,
then green — and then the guard was proven by reversal.

The plan named the reversal and its restore in one table row:

| Break | Expect | Restore |
|---|---|---|
| change the quoted number `23` → `24` | `quotes 24 … has 23` | `git checkout <the document>` |

The reversal went red on exactly the predicted sentence. The restore ran. **The
re-check that followed went red too**, with a different sentence: the document no
longer carried the count at all. `git checkout <file>` does not undo *the last edit*; it
restores the file **from the index or the last commit**, and the edit that added the
count had never been committed. The restore removed the reversal and the fix in one
step, and put the document back to the day before the work.

It was caught because the step ran the gate again after restoring, rather than taking
"restored" as the end of the reversal.

## Why it is easy to walk into

A reversal is usually described against a **clean tree**: the guard is committed, you
break it, you restore it. In that world `git checkout`, `git restore` and `git stash`
all mean "undo the break". A plan written ahead of time naturally uses them.

But the order that proves a guard — test red first, then the fix, then reversals —
puts the reversals **before** the commit. In that window the file under reversal holds
two uncommitted edits, the fix and the break, and every version-control restore treats
them as one.

The same plan had the same restore on two other rows, both on a document another step
of the same pull request was editing. Each would have deleted that step's paragraph and
looked, at first glance, like a merge problem.

## The rule

- **Restore a reversal by undoing the reversal's own edit**, or by copying back a
  backup taken immediately before the break — never by asking version control for "the
  file as it was", unless the tree is clean.
- **A restore is not finished until the gate is green again.** That second run is the
  only thing that saw this one.
- When a plan writes a restore column ahead of time, it should say *which* state it is
  restoring to. "Checkout" silently means *the last commit*.

## Related

- [a-reversal-that-removed-the-guard-s-label-not-the-guard](a-reversal-that-removed-the-guard-s-label-not-the-guard.md)
  — a reversal that changed less than it claimed; this one's *restore* changed more.
- [a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for](a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for.md)
  — why the red sentence, not the red, is what a reversal is checked against.
