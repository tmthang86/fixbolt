# A shell error inside an `if` drops the row, and the report exits zero

> `[measured 2026-09-22]` — senior review finding F9 on
> [plans/2026-09-22-the-detector-and-the-campaign-preconditions.md](../plans/2026-09-22-the-detector-and-the-campaign-preconditions.md),
> closed at `83b53e3`. Reproduced by the manager on bash 5.2.21 **after** the brief that
> described it had got the behaviour wrong twice. **`[to testing-skills]`**
>
> The sibling of [a-machine-check-narrowed-its-own-scope-when-the-link-bounced](a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md):
> that one is a check whose *scope* shrank without saying so; this one is a check whose *row*
> disappeared without saying so. Neither is a wrong verdict. Both are a verdict that is not
> there, in a report that still ends `exit 0`.

## The shape

`scripts/check-machine.sh` prints one row per §9 condition and exits by counting `FAIL` and
`UNKNOWN` rows. The `no timer due` row read its window from `FIXBOLT_TIMER_WINDOW` and, before
`83b53e3`, handed it straight to shell arithmetic:

```sh
TIMER_WINDOW_H=${FIXBOLT_TIMER_WINDOW:-12}                  # check-machine.sh:648 at 99e8564
…
    window_sec=$((TIMER_WINDOW_H * 3600))                   # :664 — 0.5 → "syntax error in expression"
    tv=$(timers_verdict "$now_usec" "$window_sec" "$lt_out")
    row … "no timer due" …
```

(The two lines are quoted from `git show 99e8564:scripts/check-machine.sh`; the arithmetic sat
inside the `if` that had just established `systemctl` could reach PID 1.)

What a failing `$(( ))` does there is not what two people in a row assumed:

- It does **not** end the script. On bash 5.2.21 the arithmetic error **abandons the enclosing
  block at the failing line** — the `row` call after it never runs — and execution continues
  **after the `if`**. Every later row (kTLS, IRQ affinity, the summary) prints as usual.
- The counters never saw a row for the timer, so `fail` and `unknown` are unchanged, and the
  script **exits 0**.
- At top level, outside any block, the next command simply runs. The explanation "a top-level
  failure ends the script" is also wrong.

So the report was one row shorter than it should have been, said nothing about it, and was
green. A reader who did not already know the row existed had no way to miss it.

## Why it has teeth beyond one row

`scripts/ab-rotation.sh`'s preflight reads that row by `grep` and refuses to start a campaign
on `FAIL`. **An absent row matched no case.** A mistyped knob — `0.5`, `' '`, `abc` — therefore
let a rotation of twelve hours begin with **no timer check and no message at all**, which is
precisely the condition the row was built to prevent
([a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired](a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired.md)).

A gate that fails is read. A gate that vanishes is not, because nothing reads a line that is
not there. That is the general shape, and it is not about timers: any row-per-check report
whose rows are produced inside a block that shell can abandon has it.

## Two claims the brief made, and what measurement said

Both corrected by running the thing, not by argument — recorded because the wrong versions
were confidently written by the person who had reproduced the finding:

- *"An empty value dies too."* It does not: `${VAR:-12}` treats empty as unset. Measured:
  `timer_window_sec ''` is refused, and the caller's default reads 12.
- *"The script dies mid-report."* It does not; the truth above is worse. Also, `' '` and `-1`
  had been accepted **silently** as a 0 s and a −3 600 s window — no error at all.

## The rule

- **Validate a knob before arithmetic, in a pure function with its own test.** `timer_window_sec`
  accepts a non-negative decimal and prints whole seconds; anything else is refused and costs
  **one `UNKNOWN` row naming the knob** — a row that *exists* and is counted.
- **A row-per-check report must be unable to lose a row silently.** Either every branch of a
  check ends in a `row` call, or the consumer counts rows against the number it expects. The
  driver's preflight now prints `timers: unknown — FIXBOLT_TIMER_WINDOW='abc' is not a number
  of hours …; not refusing` — a sentence, where before there was nothing.
- **A failing `$(( ))` is a control-flow event, not an exit.** Do not reason about it from
  memory; run it inside an `if` and read what still prints.

## Guarded by

`scripts/check-machine-verdicts.sh`, section `=== timer_window_sec` (12 assertions, read at
`83b53e3`: five accepted — `12`, `0.5`, `.5`, `12.`, `0` — and seven refused — `''`, `' '`,
`-1`, `abc`, `12h`, `1.2.3`, `1e3`), since `83b53e3`; `scripts/check-machine.sh`'s timer block now calls
`timer_window_sec` before any arithmetic and emits the `UNKNOWN` row on refusal, so the row
is present on every path.

**What it does not guard**: the *general* shape — another row, in this or any other
report, produced inside a block that a future edit makes abandonable. That is a review
question, and this page is what the reviewer is meant to have read.

Related: [a-machine-check-narrowed-its-own-scope-when-the-link-bounced](a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md),
[a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired](a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired.md),
[wc-l-counts-an-empty-capture-as-one-line](wc-l-counts-an-empty-capture-as-one-line.md),
[ADR-0093](../decisions/ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md)
decision 3.
