# A test skipped itself on every machine that ran it, and reported `ok`

> `[measured 2026-09-02]` — `STATUS.md` item 37, opened the same day with the cause **wrong**,
> and closed by measuring instead of guessing.
> **`[to testing-skills]`**

## The shape

A test needs something the machine may not have — N cores, a GPU, a display, a licence. The
usual accommodation:

```rust
#[test]
fn the_thing() {
    let Some(resource) = resource_for(2) else { return };   // ← skip
    …
}
```

**A test that returns early reports `ok`.** Rust has no *skipped* verdict, so the summary line
reads `7 passed; 0 failed` whether the body ran or not. Nothing in the output distinguishes
*checked and correct* from *never executed*.

That is survivable while the machines vary. It stops being survivable when **every** machine
takes the skip.

## The measurement

| Machine | Physical cores | Ran the test? |
|---|---|---|
| GitHub `ubuntu-latest` | too few — 2 vCPUs are 2 SMT threads of 1 core | **no**, skipped |
| The reference desktop | 8 | yes — but last ran the full suite the day *before* the change that broke it |
| A 4-vCPU cloud VM | 4 | **yes** — first machine with room since |

The test had been failing for **a day and a half**. Every gate was green, and one of them was
green because it had not run.

The break itself was ordinary: a registry moved into an earlier stage, and the helper that
builds a connection was given a registry serving **one** identity. The one test needing a
*second* identity handed it something the registry was right to refuse.

## Two things made the failure unreadable once it did appear

**1. A deadline turned a refusal into a slow machine.** The helper waited five seconds for the
connection to settle, then said *"the Logon arrived over loopback"*. So the first reading was
*this VM is slow* — and it went into the tracker that way. The connection had been refused in
microseconds; the five seconds were spent waiting for something that was never coming.

The fix is to say which outcome happened, not that the wanted one didn't:

```
the pre-session stage never settled a Logon from TW45. What it did instead, over
the whole wait: Progress { settled: 0, timed_out: 0, not_logon: 0, unknown: 1, gone: 0 }.
`unknown` means the registry refused the identity — look at `registry()`, not at the socket.
```

**2. A `Progress` counter read at the wrong moment said the opposite of the truth.** The first
instrumentation printed the **last** turn's counters. The refused slot is removed when it is
refused, so every turn after that reports zeros — and the diagnostic read
`unknown: 0`, which is exactly the evidence *against* the true cause. Accumulating across the
whole loop reads `unknown: 1`. **A counter sampled after the event it counts is not a counter.**

## And the fix exposed a second blind assertion

Making the test run turned another one red. A shard asserted that the configuration handed to it
had travelled with the connection:

```rust
assert!(cfg.serves(b"TW44", b"ISLD"), "the configuration travels with the connection");
```

`TW44` was the **only** identity a one-counterparty registry could produce, so the assertion was
true of every configuration the system could hand out — whether or not it had travelled. Reading
the identity back off this connection's own wire makes it a claim about *this* connection:

```rust
let sender = field(prefix, b"49=").expect("a Logon carries a SenderCompID");
assert!(cfg.serves(sender, b"ISLD"), …);
```

Proven by making the property actually false — the runtime patched to hand every shard a
hard-coded configuration instead of the one that travelled:

| Runtime | Assertion | Result |
|---|---|---|
| config travels | wire-based | **pass** |
| config does **not** travel | wire-based | **fail** — "TW45 got a config that does not serve it" |
| config does **not** travel | old constant | **pass** ← the blind spot, demonstrated |

The third row is the finding. The old assertion was green about the exact property its own
comment claimed it proved.

## The lesson, stated without FIX

1. **A conditional skip is a silent `ok`, so it needs a receipt.** Print a line naming what did
   not run. It costs nothing, it turns *"CI is green"* into *"CI is green about these"*, and it
   is the only way a reader of the output can tell the two apart.
2. **Count the machines your skips run on.** A skip is a sampling strategy, and it fails silently
   when the sample becomes empty. Ask, per skip: *which machine in this project's rotation
   actually takes the other branch?* If the answer is "the developer's box, sometimes", the test
   is closer to unrun than to covered.
3. **A deadline converts every cause into "slow".** Where a wait can end for several reasons,
   report which one — otherwise the first person to read the failure will diagnose the timer.
4. **An assertion against a constant is only as strong as the number of values the system can
   produce.** With one possible value it is a tautology. It becomes a test the moment a second
   value exists — which is usually the moment it goes red and looks like a regression.

## The process rule that did not hold

`git checkout <file>` was used to undo a scratch reversal in this file and destroyed the whole
uncommitted fix. **Third time in this repository, and the first since the rule "commit before
running a reversal" was written down** — on the previous day, by the person who then broke it.

The rule was read and still did not hold, because *the reversal loop and the undo share a
target*: reverting the experiment and reverting the work are the same command, and only the
timing distinguishes them. A rule that depends on remembering which of two identical actions you
are performing is not a strong rule. What actually works is the smaller habit the rule was
shorthand for: **commit, then experiment** — so `git restore` can only ever undo the experiment.
