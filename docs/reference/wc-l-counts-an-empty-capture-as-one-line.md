# `wc -l` counts an empty capture as one line

> `[measured 2026-09-22]` — step 1 of
> [plans/2026-09-22-the-detector-and-the-campaign-preconditions.md](../plans/2026-09-22-the-detector-and-the-campaign-preconditions.md),
> found when a reversal read `got [1]` where the extractor had produced **nothing**.
> **`[to testing-skills]`**
>
> The sibling of [a-green-fraction-over-a-scenario-that-never-ran](a-green-fraction-over-a-scenario-that-never-ran.md)
> and [reading-the-output-you-grepped-for](reading-the-output-you-grepped-for.md). Those are
> about a *reader* accepting a number without looking behind it. This one is about an
> **instrument that cannot express zero**: the reader looked, and the number was wrong anyway.

## The shape

A shell test captures a function's output into a variable and asserts how many rows came out:

```sh
extracted=$(ab_extract w1s 7 < fixture.txt)
same "4" "$(printf '%s\n' "$extracted" | wc -l)"   # how many measurement rows?
```

`printf '%s\n'` appends a newline to whatever `$extracted` holds — **including when it holds
nothing**. `wc -l` counts newlines. So an empty capture is one line:

```
$ printf '%s\n' "" | wc -l
1
$ printf '%s'   "" | grep -c .
0
```

Reproduced by the manager on 2026-09-22 before the finding was accepted. `echo "$var" | wc -l`
has the same shape, for the same reason.

The consequence is not an off-by-one. It is that **the assertion cannot tell "one row" from
"no rows"** — the two answers that matter most to a test of an extractor are the two it
merges. A reversal that breaks the extractor completely reads as *partially working*.

## What it cost

The plan's step 1 pinned `ab_extract` (the pure row extractor of `scripts/ab-rotation.sh`,
[ADR-0092](../decisions/ADR-0092-the-rotation-driver-reads-a-row-by-its-shape-and-a-panicking-finish-is-a-verdict-not-a-lost-round.md))
with a captured harness fixture and two reversals. The second reversal broke the row anchor
and was predicted to read `want [4] got [2]`. It read **`got [1]`**. That disagreement was
reported as read, and the first explanation — the fixture had no in-band row, so fewer rows
survived than predicted — was true and was written into the plan's *Sửa 1*. It was also
incomplete: the anchor being broken, **every** row had stopped matching, `$extracted` was
empty, and the `1` was `printf`'s newline. The number was not a row count at all. Two
readings of one number were needed before it said what it meant, and the second one came from
running the two-line probe above, not from re-reading the test.

Nothing wrong was published: the assertion in question is a test of a script, and the test
was red on its first reversal for the right reason. What the trap would have cost is the next
reversal — one where the extractor produced nothing and the test read `got [1]`, and someone
concluded the anchor still matched one shape.

## The rule

- **Count rows with `grep -c .` on a capture emitted without a trailing newline**:
  `printf '%s' "$var" | grep -c .` — a `.` needs a character, so an empty capture is 0 and a
  capture of one empty line is also 0. `wc -l` is for files and for pipes that carry the
  producer's own newlines; it is not for a variable re-emitted by `printf '%s\n'` or `echo`.
- **A direct pipe from the producer is not this trap.** `f | wc -l` counts 0 when `f` prints
  nothing, because no newline is added. The trap needs the *re-emission* of an empty variable.
- **An assertion on a count must have a reversal that reads `0`.** If the expected FAIL of
  "the extractor produces nothing" is not `got [0]`, the counter cannot see nothing, and the
  test proves less than it says.

## Guarded by

`scripts/check-ab-rotation.sh`, section `=== ab_extract`, since `85262e4`: the row count is
`printf '%s' "$extracted" | grep -c .`, the comment above it names this page's mechanism, and
the anchor reversal reads **`got [0]`**. `scripts/ab-rotation.sh`'s own `ab_summary` already
counted `n` that way (`n=$(printf '%s\n' "$vals" | grep -c .)`, `:167`) — the rule existed
in one place in the same file family and was not carried to the test beside it, which is the
usual way a known trap recurs.

## Where the same shape is, and is not, in `scripts/` today

`grep -n 'wc -l' scripts/*.sh`, 2026-09-22, read site by site:

- `w2w-baseline.sh:578` — `printf '%s\n' "$TREE_STATUS" | wc -l`, the exact shape, **guarded**:
  the line sits inside `else` of `if [ -z "$TREE_STATUS" ]`, so the empty case never reaches
  it. Correct, and fragile by one moved line; not changed.
- `check-sudo-verdicts.sh:101,121` — `f … | wc -l`, direct pipes from a function whose
  expected output is one line; an empty output there counts 0. Not the trap. (A file being
  written by a developer at the time of this page; read, not touched.)
- `w2w-baseline.sh:331-332`, `fetch-quickfix-assets.sh:76,103-107`, `check-no-optional-deps.sh:162`
  — `wc -l < file`, `find … | wc -l`, `ls … | wc -l`, `wc -l <<<"$out"`: file and pipe
  counts. The `<<<` form **is** the trap shape (a here-string appends a newline), guarded at
  that site by the `out` check that precedes it; noted, not changed.

Related: [a-green-fraction-over-a-scenario-that-never-ran](a-green-fraction-over-a-scenario-that-never-ran.md),
[reading-the-output-you-grepped-for](reading-the-output-you-grepped-for.md),
[perf-record-exits-zero-when-sudo-cannot-find-the-workload](perf-record-exits-zero-when-sudo-cannot-find-the-workload.md).
