# A counter that must be remembered is not a counter

> `[measured 2026-09-01]` — found while adding the counterparty registry,
> [plans/2026-09-01-counterparty-registry.md](../plans/2026-09-01-counterparty-registry.md)
> step 2. Recorded as [ADR-0029](../decisions/ADR-0029-the-pre-session-stage-enforces-four-definitions.md).
> **`[to testing-skills]`**

## The guard, and it was a good one

`crates/engine/tests/shard_wire.rs` runs the 59 acceptance definitions through the shard
runtime. [ADR-0022](../decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md)
established that 59/59 alone is not enough there, for a sharp reason:

> `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` both expect **no response at all** on
> the second connection. So does a socket the pre-session stage quietly threw away. From the
> wire the two are identical, and 59/59 cannot tell them apart.

So the test was made to count how the stage disposed of every socket, and to pin each count
by name. It even wrote down the failure it was defending against:

> *"Pinned rather than relaxed to a range: a THIRD connection disappearing here would be a
> new defect wearing the same green."*

That is a correct prediction, in prose, sitting three lines above the code that could not
act on it.

## What happened

The counterparty registry added a fifth way for the stage to dispose of a socket —
`Progress::unknown`, an identity nobody configured. The collector read four:

```rust
let p = self.pending.turn(0);
DISPOSAL[0].fetch_add(p.settled,    Ordering::Relaxed);
DISPOSAL[1].fetch_add(p.timed_out,  Ordering::Relaxed);
DISPOSAL[2].fetch_add(p.not_logon,  Ordering::Relaxed);
DISPOSAL[3].fetch_add(p.gone,       Ordering::Relaxed);
// p.unknown is never read
```

`[measured 2026-09-01]` CI run
[33509748294](https://github.com/tmthang86/fixbolt/actions/runs/33509748294) — Linux, with
`--features affinity`, so the test really ran — came back **green**. Two connections were
being dropped, and:

- `[timed_out, unrouted] == [0, 0]` — still true.
- `not_logon == 1` — still true.
- `gone == 1` — still true.
- 59/59 through one shard and through two — still true.

Nothing lied. Every assertion was about a counter that was still correct. The two missing
connections were in a counter nobody had thought to add to the list, and **the list was
maintained by hand.**

## Why this is worse than an ordinary missed case

The green was *load-bearing*. The commit under test carried an explicit prediction — that
this number would go from 2 to 4 — and CI's green was read as the prediction being wrong.
A guard that fails silently does not merely stop protecting; **it actively supplies evidence
for the wrong conclusion.** One more minute and an accepted ADR would have been left
unamended on the strength of it.

## The fix, and it is not "remember next time"

Destructure the struct field by field, with no `..`:

```rust
let Progress { settled, timed_out, not_logon, unknown, gone } = self.pending.turn(0);
```

Now adding a field to `Progress` **breaks this build**. The compiler maintains the list.

`[measured 2026-09-01]` with that in place, CI run
[33512983304](https://github.com/tmthang86/fixbolt/actions/runs/33512983304) reported
`unknown == 2` and 59/59 unchanged — the number the probe had predicted, from the test that
is entitled to state it.

## The rule

**A guard that enumerates cases must be forced to enumerate them, or it guards only the
cases that existed the day it was written.**

Three shapes that do the forcing, cheapest first:

1. **Exhaustive destructuring with no `..`** — for a struct of counts, as here.
2. **A `match` with no `_` arm** — for an enum of outcomes.
3. **A total that must reconcile** — assert `settled + every_refusal == connections_opened`,
   so an uncounted disposal makes the sum wrong even if the categories are incomplete.

`..`, `_`, and `#[non_exhaustive]` on your own type are all ways of telling the compiler not
to help. In a *test whose whole purpose is completeness*, each of them removes the only
mechanism that keeps it complete.

## Where else this bites

Anywhere a test asserts *"exactly these N things happened"*: disposal reasons, error
variants, event kinds, rejected-message categories, metric labels. The test passes forever
and covers less every release.

See also
[silence-before-a-logon-has-many-causes.md](silence-before-a-logon-has-many-causes.md) — the
same week, the same file, the other half of the lesson: that one is about a *negative*
assertion with no control, this one is about an *enumerating* assertion with no compiler
behind it. Both were green. Neither was evidence.
