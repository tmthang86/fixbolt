# The guard could not match the thing it was written to catch

> `[measured 2026-09-12]` — a new check's assertion went **green** on its own
> reversal, because a hand-written character class excluded the one character
> every real instance of what it looked for contains.
>
> **`[to testing-skills]`**

## What happened

`scripts/check-no-crate-root-allow.sh` was written to machine-check the class behind
[an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md](an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md):
an inner attribute at a crate root that *widens* a lint switches that lint off for the whole
crate, clean submodules included. Its second assertion, A2, covers the quieter half of the
class — not `#![allow(...)]`, but `#![warn(...)]` naming a lint the workspace currently
**denies**. `warn` reads as tightening; on a lint that is already `deny` it is a downgrade,
and it is the shape nobody looks twice at.

A2's deny list is derived from `Cargo.toml` rather than hard-coded, which was the right
decision and is not what went wrong. What went wrong is the line that asks whether a
`#![warn(...)]` line names one of those lints:

```sh
# the first implementation
if echo "$line" | grep -qE "(^|[^A-Za-z0-9_:])${lint}\b"; then
```

The intent was *"the lint name, not as a substring of a longer identifier"*. The character
class lists what may precede the name, and it excludes `:` — so `panic` would not match inside
`no_panic`, and would not match inside `some::panic` either. But **`some::panic` is how every
real clippy lint is written**. `clippy::unwrap_used`, `clippy::panic`, `clippy::indexing_slicing`:
the character immediately before the name is always `:`, which is exactly the character the
class refuses.

The assertion that exists to catch `#![warn(clippy::unwrap_used)]` was blind to `clippy::`.
It could match a bare `unwrap_used` — a spelling that appears in `Cargo.toml`'s `[lints.clippy]`
table and nowhere in a source attribute.

## How it was caught

By its own reversal, R-A2: put `#![warn(clippy::unwrap_used)]` as line 1 of a crate root and run
the script. The expected failure sentence had been written down **first**, in the plan:

```
check-no-crate-root-allow: FAIL — crate-root warn lowers a workspace deny at crates/dict/src/lib.rs:1
```

What the run produced was no failure at all: exit 0, and the same
`check-no-crate-root-allow: ok — 6 crate roots, 9 manifests` line a clean tree prints. Not red on
the wrong assertion — **no red.**

The fix is the whole of the diff:

```sh
if echo "$line" | grep -qE "\b${lint}\b"; then
```

`\b` is a word boundary, and `:` is not a word character, so the boundary is exactly where the
name begins whether or not a path precedes it. All three A-reversals were re-run after it, and
the one that had been green was re-run independently by a second reader:

```
check-no-crate-root-allow: FAIL — crate-root warn lowers a workspace deny at crates/dict/src/lib.rs:1
exit=1
```

## Why it was nearly invisible, and what is new here

The obvious hiding place is that everything else about the check was healthy. A1 and A3 were red
on their reversals, on their own sentences; the script's `ok` line printed a plausible count of
files it really had opened; the deny list it built from `Cargo.toml` was correct and complete.
**The scan happened, the list was right, the loop ran — and the comparison inside the loop could
not return true.** Nothing about the shape of the output distinguishes that from a clean tree,
because a clean tree is what it is supposed to look like.

Two entries here are next to this one, and neither covers it:

- In [an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md](an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md)
  the **guarded thing** was switched off while the counter that everybody reads stayed exactly
  right. The guard worked; its subject had been disabled underneath it.
- In [a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for.md](a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for.md)
  the reversal went **red for the wrong reason** — a different assertion was doing the work, and
  the one under examination could not have failed.

Here it is the **guard itself** that was off, from the first line it was ever run. And the only
thing that could see it was a reversal whose expected failure sentence was written down *in
advance*. A reversal judged on "did it go red or not" reports *not red* as *nothing is wrong* —
which is the answer a correct check gives to a correct tree. That run would have been recorded as
a pass, and the assertion would have shipped, in CI, never able to fire.

## The generalisation

> **A matcher written to exclude something excludes it everywhere, including from the spelling
> the real input actually uses.** A hand-written character class is a list of assumptions about
> what the input looks like, and the assumption that breaks is nearly always the separator: a
> namespace `::`, a package `.`, a path `/`, a domain `-`, a scope `@`. The thing under test
> is never a bare name in the wild; it is a qualified one, and the qualifier is punctuation the
> class was written to keep out.
>
> A guard that cannot match has no observable of its own. It scans the right files, reports the
> right counts, and returns *nothing found* — which is indistinguishable from the healthy answer
> and is the answer it will give for ever. The count that proves the scan happened does not prove
> the comparison can succeed.
>
> **The defence is not a better regex, it is the order of two steps.** Write down the exact
> failure message you expect *before* running the reversal, then compare the text. Colour is a
> boolean and it agrees with a clean system; a predicted message either appears or it does not.
> Every check that greps owes two proofs — that it refuses an empty result as a pass, and that
> it can produce a hit at all on an input that really occurs.
>
> Prefer a boundary the language defines (`\b`) to a character class you enumerate. An
> enumeration is a claim about every character that will ever appear; a boundary is a claim
> about where a word starts, which is the thing actually meant.

## Guarded by

`scripts/check-no-crate-root-allow.sh` assertion A2, and the reversal R-A2 recorded in
[the plan](../plans/2026-09-12-crate-root-allow-scratch-fixture-and-tls-4c.md). `STATUS.md`
item 58.
