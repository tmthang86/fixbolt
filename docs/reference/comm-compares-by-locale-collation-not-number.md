# `comm` compares by locale collation, not by number

> `[measured 2026-09-14]` — found while building the NIC IRQ affinity row for
> `scripts/check-machine.sh`, step A5 of
> [plans/2026-09-04-the-second-linux-desk.md](../plans/2026-09-04-the-second-linux-desk.md).
> **`[to testing-skills]`**

## What happened

Checking whether a NIC's `/proc/irq/<n>/smp_affinity_list` overlaps
`/sys/devices/system/cpu/isolated` needs the intersection of two CPU-number lists — e.g.
`0 1 2 … 15` against `6 7 14 15`. Both were expanded to one number per line and piped
through `sort -n`, which sorted them correctly: `6, 7, 8, 9, 10, 11, …` ascending. `comm
-12` on those two `sort -n`-ed streams still failed, loudly, on both inputs:

```
comm: file 2 is not in sorted order
comm: file 1 is not in sorted order
comm: input is not in sorted order
```

— even though the intersection it printed anyway (`6`, `7`) was the right answer.

## Why

`comm` does not re-derive "sorted" numerically; it compares adjacent lines as strings, by
the process's current locale collation. Under a normal (non-`C`) locale, `"10"` sorts
before `"6"` lexically, so a stream `sort -n` put in numeric order (`…6 7 8 9 10 11…`) reads,
to `comm`, as out of order the instant a two-digit number follows a one-digit one. The
warning went to stderr and the correct intersection still appeared on stdout here — but a
caller that runs under `set -e`, or that treats stderr as "something is wrong", would either
fail on a command that actually worked, or read a real input-ordering bug as this same
harmless noise.

## The fix

`grep -Fxf` instead of `comm -12`:

```sh
grep -Fxf <(printf '%s\n' "$isolated_expanded") <(printf '%s\n' "$al_expanded")
```

finds every line of `$al_expanded` that appears literally, as a whole line, in
`$isolated_expanded` — the same intersection, with no sort order required on either side.

## The rule

**`comm`, `join`, and anything else documented as needing "sorted input" mean sorted under
`LC_ALL=C` (byte) order, not whatever order produced the numbers you actually care about.**
`sort -n` and `comm`'s idea of "sorted" disagree the moment two-digit numbers are in the
mix. Either force both sides to agree (`LC_ALL=C sort`, never `-n`, on anything `comm` will
read) or sidestep the requirement with a membership test that does not care about order,
such as `grep -Fxf`.

## Regression guard

Honestly: there is no automated test for this. The guard that exists is the manual IRQ-row
reversal run by hand during step A5 — break one IRQ's `smp_affinity_list` to an isolated
CPU, confirm the row reads `FAIL` naming it, restore the recorded value, confirm it reads
back identical and the row returns to its prior verdict — run against the real
`/proc/irq/` tree on the `DESIGN.md` §9 desktop, not by a script CI runs. A machine with no
NIC, no isolated-CPU list, or no root to read `/proc/irq/*/smp_affinity_list` cannot
exercise this path at all, automated or not.
