# A masked diff compared the padding, not the format

> `[measured 2026-09-14]` — found while proving that `tools/w2w` with no new flag prints what it
> printed before `--listen`, `--connect` and `--interval` existed,
> [plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md) A3a.

## The practice

To show that a change left a program's output *format* alone, run the old binary and the new one
with the same arguments, replace every number with a placeholder, and `diff` the two. Timings
differ from run to run; the words around them must not.

## What happened

The mask was `sed -E 's/[0-9]+/N/g'`. Two runs of the same format produced a diff:

```
14c14
<      pN.N      N ns
---
>      pN.N     N ns
```

The line is printed with `{:>9}`, a right-aligned field nine characters wide. `87196` is five
digits and gets four spaces of padding; `291835` is six and gets three. **Replacing the digits
with one `N` removes the number's width but keeps the padding that depended on it**, so two
identical format strings compared unequal whenever one sample had a different number of digits.

A mask that was meant to hide the measurement hid half of it and turned the other half into a
difference in whitespace.

## The fix

Mask the padding together with the digits:

```
sed -E 's/ +[0-9]+/ N/g; s/[0-9]+/N/g'
```

With it, `--messages 2000 --warmup 200`, `--tls ktls` and `--path app --mode standard` each
compared `IDENTICAL (25 vs 25 lines)` between the pre-change and post-change binaries.

## What to carry forward

- **A right-aligned or zero-padded field ties its whitespace to its value.** A mask that
  replaces the value has to replace the whitespace that depends on it, or the diff is really
  comparing the numbers again, just less directly.
- **The false diff is the easy case.** The same mask could just as well hide a real change: if a
  format string's width changed from `{:>9}` to `{:>8}`, the corrected mask erases that too. It
  proves the words and the line order, not column widths. Say which one the gate proves.
- **A run that matches can still be misleading.** With a digit-only mask, two runs whose samples
  happen to have the same number of digits compare equal, so a pass depended on the timings.
  Read the diff when it goes red. Do not just change the mask until it passes.

## Regression guard

Honestly: **none that runs.** The mask is not in any committed script — it was typed at a shell
for step A3a's gate, and `grep -rn` over `scripts/`, `tools/`, `crates/` and `.github/` on
2026-09-14 finds neither the broken spelling nor the fixed one outside this file. A trap in a
command nobody commits has nothing for a test to hold on to.

What exists is a one-line reproduction, which anyone about to write a masked diff can run first
(output from 2026-09-14, `cat -A` so the padding shows):

```
$ printf '     p99.9  %9s ns\n' 87196 291835 | sed -E 's/[0-9]+/N/g' | cat -A
     pN.N      N ns$
     pN.N     N ns$
$ printf '     p99.9  %9s ns\n' 87196 291835 | sed -E 's/ +[0-9]+/ N/g; s/[0-9]+/N/g' | cat -A
     pN.N N ns$
     pN.N N ns$
```

The day a masked diff becomes a committed gate, it gets a fixture with two widths of the same
field, and this section names it. The blind spot the fixed mask introduces — a changed field width
compares equal — stays unguarded either way, and a gate built on it must say so.
