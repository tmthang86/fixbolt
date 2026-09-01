# Silence before a Logon has many causes, and on the wire they are identical

> `[measured 2026-09-01]` — found while writing the specification test for
> [plans/2026-09-01-counterparty-registry.md](../plans/2026-09-01-counterparty-registry.md)
> step 1. **`[to testing-skills]`**

## The protocol fact underneath it

Before a `Logon` there is no session to answer with, so the acceptance corpus expects the
engine to **say nothing and hang up** for every fault it can have at that point.
`1c_InvalidLogonBadSenderCompID`, `1d_InvalidLogonWrongBeginString`,
`1d_InvalidLogonBadSendingTime`, `1e_NotLogonMessage`, `1b_DuplicateIdentity` and
`AlreadyLoggedOn` all score on the same observable: **zero bytes back**. The session layer
says so in its own comment — *"these are all one thing: hang up in silence"*
(`crates/session/src/lib.rs`, the `AwaitingLogon` branch).

That is correct protocol behaviour. It is also **a channel that carries one bit**, and a
test that reads it is reading a value with at least six causes.

## What it cost

The step-1 test asks for something the engine cannot do yet: two counterparties on one
acceptor. Its whole job is to be **red for one specific reason** — the second counterparty
is refused, because `Config` carries one `target_comp_id`. The failure message said exactly
that.

The first run came back red, with that message, on **the wrong counterparty**:

```
---- two_counterparties_log_on_to_one_acceptor stdout ----
TW44 was refused in silence: the acceptor sent it nothing.
An acceptor holds one `Config` and therefore one `target_comp_id`, so it can serve one
counterparty. This is what ADR-0026's registry is for.
```

`TW44` is the identity the acceptor **was** configured for. The real cause was three type
parameters away:

```rust
const N: usize = 8;     // the field-index size
```

A `Logon` carries nine fields. The index had room for eight, the parse failed, the session
refused what it could not read — in silence, like everything else at that point — and a
message naming a missing registry printed over the top of it.

Nothing was wrong with the engine. The test was red, the diff would have looked right, and
the number it was actually measuring was `MAX_FIELDS`.

## Why review would not have caught it

`N = 8` is a plausible number sitting in a list of four plausible numbers
(`PRE`, `N`, `RX`, `TX`), and the test it broke was **supposed to fail**. Every signal a
reviewer has — the test is red, the message names the right cause, the other tests are
green — pointed the wrong way. `CLAUDE.md` §10's line applies literally: *review of a diff
catches almost nothing.*

## The rule

**A test whose expected result is a silent refusal needs a control that is not refused, in
the same harness, in the same run.** Without one, the harness's own defects are
indistinguishable from the gap the test exists to name — and they print the gap's error
message while doing it.

## The regression test

`crates/engine/tests/registry.rs::the_corpus_counterparty_still_logs_on` — the corpus's own
counterparty, alone, through the same `Gateway`, asserting the `Logon` comes back. It goes
red the moment the harness stops being able to serve anybody, and it is what turns *"BETA
was refused"* from a claim into a difference.

Its sibling `relabelling_to_the_same_sender_reproduces_the_corpus_bytes` is the same idea
one layer down: the second counterparty's `Logon` is the corpus's bytes with `49=` rewritten
and `9=`/`10=` recomputed, so rewriting `TW44` to `TW44` must give the corpus bytes back
exactly. A malformed message is also refused in silence.

## Where else this bites

Anywhere a gate's pass condition is *nothing happened*: a connection that should be dropped,
a message that should not be replayed, a benchmark whose optimised-away body reads zero. The
guard is the same shape every time — **assert the positive case beside the negative one, or
the negative one is unfalsifiable.** See also
[a-benchmark-can-delete-its-own-work.md](a-benchmark-can-delete-its-own-work.md) and
[the-guard-measured-a-window-that-excluded-the-thing.md](the-guard-measured-a-window-that-excluded-the-thing.md).
