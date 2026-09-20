# Two branches can take the same ADR number without a conflict

`[measured 2026-09-19]`

## The shape

Three sessions worked phase 2 in parallel (the plan's *Chạy song song*). Two of them needed an
ADR on the same afternoon. Both read `ls docs/decisions/`, both saw `ADR-0081` as the highest,
and both wrote an `ADR-0082`:

```
docs/decisions/ADR-0082-the-session-is-generic-over-tag-value-encodings-and-…   (PR A, desk)
docs/decisions/ADR-0082-ten-field-types-one-field-spelled-two-ways-and-…       (PR B, cloud)
```

Different subjects, different slugs — and therefore **different filenames**. Git merges two
different filenames without a conflict. Neither branch's tests, `cargo clippy`, `cargo doc`
nor `scripts/check-links.py` says a word: every link resolves, because each ADR resolves to a
file that exists. The result is two ADR-0082s in one tree, which
[CLAUDE.md](../../CLAUDE.md) §5 forbids in one sentence — *"Numbered sequentially, never
reused"* — with nothing enforcing it.

A conflict is how git tells you two people edited the same thing. Here they did not edit the
same thing; they **allocated the same name** in a namespace git does not model. That is the
whole trap: the shared resource is the *number*, and the number is not a file.

## Why the obvious check does not catch it

`ls docs/decisions/` answers "what is the highest number **on my branch**". With sessions on
three branches, that is the wrong question; the right one is "what is the highest number on
any branch, merged or not", and no command in this repository asked it. The window is open
from the moment a branch is cut to the moment it merges — hours or days, not seconds.

It is also invisible in review: a reviewer of PR B sees one ADR-0082 and a tree where that is
the only one.

## What to do

Before writing an ADR while any sibling branch is open, ask every branch, not just yours:

```
for b in $(git branch -r --format='%(refname:short)' | grep -v HEAD); do
  git ls-tree -r --name-only "$b" docs/decisions/ | grep -oE 'ADR-[0-9]{4}'
done | sort -u | tail -5
```

On a collision, the branch that merges **later** renumbers — the merge order is the plan's
(`A → B → C → D`), so the earlier PR keeps the number. Renaming costs a `git mv` plus every
reference: the ADR's own title line, the other ADRs' *Related* lists, `docs/reference/` pages,
the plan's delivery log, and the pull request body. `check-links.py` catches a path left
behind; it cannot catch prose that still says the old number, so grep for the bare string too.

## What guards it

`scripts/check-adr-numbers.sh` fails when two files in `docs/decisions/` share an `ADR-NNNN`
prefix, or when a file does not match `ADR-NNNN-<slug>.md`; it runs in the `docs` job of
`.github/workflows/ci.yml`, directly after `check-links.py`. It still cannot see a collision
with a sibling branch that has not merged into the tree it reads — that is what the
`for b in $(git branch -r ...)` loop above stays for.

## Related

- [CLAUDE.md](../../CLAUDE.md) §5 (ADR numbering) and §12 *Sessions and handoff* (two sessions,
  one working tree — this is its cross-branch twin).
- [a-bare-filename-is-not-evidence-of-a-repository](a-bare-filename-is-not-evidence-of-a-repository.md)
  — the other trap where a filename read as an identity claim it does not carry.
