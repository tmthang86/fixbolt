# Two time-based rules, one observable, and the test could not tell them apart

> `[measured 2026-09-02]` — found writing step 1 of
> [plans/2026-09-02-session-schedules.md](../plans/2026-09-02-session-schedules.md).
> **`[to testing-skills]`**

## The shape

A FIX acceptor refuses a `Logon` **in silence** for every fault it can have before a session
exists — see
[silence-before-a-logon-has-many-causes.md](silence-before-a-logon-has-many-causes.md). Two of
those faults are about **time**:

| Rule | Refuses when |
|---|---|
| `max_skew_ms` (`SendingTime`, `52=`) | the message's stamp is far from the engine's clock |
| a session schedule | the engine's clock is outside trading hours |

They are different rules with different fixes — one is *your NTP has drifted*, the other is
*we are shut* — and on the wire they produce **the same nothing**.

## What it cost

The step-1 specification test asserts what the engine must do and does not: refuse a `Logon`
at 03:00 for a venue open 08:00–17:00. It drove a session to 3 a.m. and fed it a real corpus
`Logon`.

It came back **green** — the assertion it was written to fail passed on the first run.

The corpus's `Logon` is stamped `20260828-12:00:00`. Ticking the session to 03:00 put the
engine's clock **nine hours** from that stamp, `max_skew_ms` is 120 seconds, and the message
was refused for skew. The test read *"refused"*, which is what it asked for, and reported that
a schedule the engine does not have was working.

The neighbouring test failed the same way from the other side: a session resumed at `34=41`,
ticked a whole day forward, refused the same `Logon` for **a day** of skew — so the reset case
never reached the sequence-number rule it was written to measure either.

## Why the usual guard did not catch it

This file already had the control that
[silence-before-a-logon-has-many-causes.md](silence-before-a-logon-has-many-causes.md)
prescribes: `a_logon_inside_the_trading_day_is_accepted`, the same message through the same
harness at midday, asserting the `Logon` comes back. **It was green, and it was green
honestly** — at midday the stamp and the clock agree, so nothing was skewed.

That is the sharp part. A positive control proves the harness can succeed. It does **not**
prove the negative case failed for the reason claimed, because the control and the case
differ in the very variable that is confounded. Moving the clock is what makes the case
negative *and* what trips the other rule.

## The fix

Hold every variable but the one under test. The `Logon` is restamped to the instant being
tested, so `SendingTime` always agrees with the engine's clock and the only thing that varies
between 03:00 and 12:00 is the hour itself:

```rust
fn logon_stamped(at: u64) -> Vec<u8> { /* corpus bytes, 52= moved to `at`, checksum redone */ }
```

With that in place the test is red where it should be — `left: Up, right: Dropped` at 3 a.m.,
an engine that accepts a connection outside its trading hours — and the midday control stays
green.

**Two assertions inside the substitution** keep it honest: the field being replaced must be
present before, and the new value present after. The first version asserted only *the bytes
changed*, which is false at midday, where the new stamp equals the old one — a helper whose
correctness check fails on the one input where it has nothing to do.

## The rule

**When two rules share one observable, a test for either must pin the other's input, not just
add a positive control.** A control proves the harness works. It does not disambiguate a
shared output, because the manipulation that produces the negative case is usually the same
manipulation that trips the neighbouring rule.

Ask, before believing a red or a green: *what else in this system reacts to the variable I
just moved?*

## Where else this bites

- Clock skew against session schedule, against heartbeat timeout, against a logon deadline —
  all four watch the same clock and all four end a connection silently.
- A retry test and a rate-limit test that both key on elapsed time.
- A cache-expiry test whose fixture is also older than the auth token it carries.
- Any "this is rejected" assertion where the rejection reason is not in the output.

The cheapest structural defence is to make the reason observable. `[2026-09-02]` this engine
could not, and now can: `STATUS.md` item 30 (d) is closed —
[ADR-0035](../decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md) — and the case
above would today be a one-line read of `DropReason` instead of an hour.

## It happened twice more the same week, and the second time was not about time at all

`[measured 2026-09-02]` **the shape is not a property of clocks.** Two more instances landed
within days:

| Where | The two rules | The one observable |
|---|---|---|
| A `Logon` refused before a session existed | a `FieldIndex` too small, and a counterparty registry | silence, plus an error message that blamed the registry ([silence-before-a-logon-has-many-causes](silence-before-a-logon-has-many-causes.md)) |
| An ordered shutdown | *the counterparty answered our `Logout`*, and *the counterparty never answered* | `Link::Dropped` on the next tick, because both went through a state that reports the link down at once |

The third is the sharpest, because **it was created on purpose and caught immediately.** An
ordered shutdown needs to wait for a reply, and there was an existing state — `AwaitingLogout`
— that looked like exactly the right one. It was not: it reports the link down **at once** and
ignores what arrives afterwards, which is correct for the paths it already served. Reusing it
made every wait vacuous, and *they answered* and *they never answered* produced identical
observables.

The fix was a separate state, and the test that caught it was one asserting the **reason**
rather than the outcome — which only existed because of the first two cases in this file.

**So the generalisation is not "watch out for clocks".** It is: *when two conditions with
different remedies pass through one code path, they will end up with one observable unless
something forces them apart.* A shared enum variant, a shared error type, a shared state, a
shared `bool` — the mechanism varies and the failure does not. Ask, of any state you are about
to reuse: **what does this state already promise, and does my new case want that promise?**

See also
[a-counter-that-must-be-remembered-is-not-a-counter.md](a-counter-that-must-be-remembered-is-not-a-counter.md):
that one is an enumerating assertion with no compiler behind it, this one is a
discriminating assertion with no way to see which cause fired. Both were green. Neither was
evidence.
