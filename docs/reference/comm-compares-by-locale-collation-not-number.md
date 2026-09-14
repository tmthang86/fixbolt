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

`scripts/check-machine-verdicts.sh`, section `=== expand_cpulist, irq_overlap`, which CI runs
(`.github/workflows/ci.yml`). `[2026-09-14]` senior review of PR #72 found this entry had no test,
so the overlap check was moved out of the row's loop into a function, `irq_overlap` in
`scripts/check-machine.sh`, defined above the `MACHINE_SOURCE_ONLY` return so the verdict test can
source it without probing the machine. The cases need no NIC, no root and no isolated CPUs: the
§9 desk's `6-7,14-15` against `0-15`, a two-digit CPU after a one-digit one (`6,10` against
`9-10`), and whole-line membership both ways (`1` against `10-11`, `10-11` against `1`).

Proven by reversal, on throwaway copies of both scripts, 2026-09-14, `LANG=en_US.UTF-8`:

- `grep -Fxf` → `grep -Ff` (no whole-line match): red on
  `isolated cpu1, affinity cpu10-11: no overlap`, got `[10 11]`.
- `grep -Fxf` → the original `comm -12` over two `sort -n` streams: red on
  `affinity 0-15 overlaps every isolated CPU, two-digit ones included`, **got `[6 7]` where
  `[6 7 14 15]` was right**, and on `a two-digit CPU after a one-digit one`, got `[]`.

**That reversal corrects this entry.** *What happened* above says the intersection `comm` printed
anyway was right. Against `6-7,14-15` it is not: `comm` dropped `14` and `15`, the two CPUs after
the collation break — so the warning was not harmless noise, and a row built on it would have
passed an IRQ steered onto `cpu14`. The account above is kept as it was written; this paragraph is
the measurement that overrides it.

The row as a whole — reading the real `/proc/irq/*/smp_affinity_list` — is still only proven by
the manual step A5 reversal on the §9 desktop (write an isolated CPU into one IRQ's list, see the
row FAIL naming it, restore). A machine with no NIC cannot exercise that half.
