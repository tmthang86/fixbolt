# ADR-0022 — The pre-session stage enforces two acceptance definitions, and the framing rule now has two homes

> **Status:** **Accepted — 2026-09-01** · **Amends
> [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)
> decision 9.** Everything else in ADR-0020 stands.
>
> §5 forbids editing an accepted ADR's substance, so this arrives as its own
> decision. **It exists because a test found it, not because anyone predicted
> it** — which is the part worth keeping in the record.
>
> **Accepted by standing delegation**, `[2026-08-30]`.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md),
  `crates/engine/src/frame.rs` module docs, `CLAUDE.md` §2 non-negotiable 3 and §10,
  [plans/2026-08-31-pre-session-routing.md](../plans/2026-08-31-pre-session-routing.md) step 5

## Context

Step 5 got the acceptance corpus to **59/59 through two shards**, which is what the
plan was for. The number on its own was not evidence.

`1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` both expect **no response at all**
on the second connection. So does a socket the pre-session stage quietly threw away.
From the wire the two are identical, and 59/59 cannot tell them apart — `CLAUDE.md` §10,
*a check that passed for a reason other than the thing under test*.

So the test was made to count how the stage disposed of every socket, and to assert
zero. `[measured 2026-09-01]` it went red at **`[timed_out 0, not_logon 1, gone 1,
unrouted 0]`**: two connections never reached an engine at all.

Both turned out to be legitimate, and both are definitions whose entire subject is that
the link must be dropped:

| Definition | Its own comment | What the stage does |
|---|---|---|
| `1e_NotLogonMessage.def` | *"If first message is not a Logon, we must disconnect"* | first whole message is `35=0` → dropped, no reply |
| `1d_InvalidLogonLengthInvalid.def` | *"If the length of a logon message is invalid, we must disconnect"* | `9=40` is a lie; `Framer` takes `9=` at its word and reports `Cut::Garbage` → dropped, no reply |

**The behaviour is right. The place it happens moved**, and one of the two moves
contradicts a comment this repository wrote on purpose. `frame.rs` says of unreadable
bytes:

> *"Dropping it here would lose `1d_InvalidLogonLengthInvalid.def`, which wants the link
> dropped because the unreadable frame claims to be a Logon. That rule lives in
> `fixbolt_session` and is not duplicated here."*

It is duplicated now, in the stage in front of the session.

## Decision

**1. The pre-session stage enforces both definitions, deliberately.** For a first message
that is not a `Logon`, and for a first frame that can never be a message, the stage closes
the socket without a reply.

**2. It could not do otherwise, and that is the argument.** The stage exists because
nothing knows whose socket it is until the `Logon` arrives. A frame with no readable
identity cannot be routed to *any* shard — there is no default, and inventing one would
put a counterparty on a shard chosen by an unreadable message. The alternatives are
"drop it" or "hold it until the deadline", and holding a frame that can never become a
message is a slot spent on a connection that has already failed.

**3. `frame.rs`'s comment is corrected in the same commit as this ADR**, because it now
asserts something false. The rule it describes — *the session decides whether an
unreadable frame is fatal* — remains true for every connection that has logged on. It is
no longer true of the first frame on a connection, and the comment says which.

**4. The corpus test asserts `not_logon == 1` and `gone == 1`, by name, not a range.**
Relaxing it to "some connections may be dropped" would give back exactly the false green
that found this. A **third** disposal is a new defect wearing the same 59/59, and the
test names the two files that are allowed.

**5. What this does not change**: `Engine`'s own rules. A session that is up still gets
its garbled frames handed to it and still decides. `others_on` is untouched. The stage's
reach is the **first** frame on a connection, and nothing after it.

## Consequences

**Good**

- Two definitions are enforced one layer earlier and no session object is built for a
  connection that was never going to have one.
- The corpus test now says *why* it is green, not only that it is. The disposal counts
  are the difference between "the session refused the duplicate" and "the stage dropped
  it", which was the whole risk of this plan.
- A rule that had drifted into two places is written down as being in two places, with
  the boundary named, rather than being discovered by whoever next reads `frame.rs`.

**Bad, and named**

- **The framing-garbage rule genuinely has two homes now**, and `CLAUDE.md` §4's *one
  rule, one place* is worse off for it. The mitigation is that the two homes cover
  disjoint cases — first frame versus every frame after — and both say so. It is a real
  cost, taken because the alternative is routing unreadable bytes to an arbitrary shard.
- **The stage's behaviour for these two cases is now load-bearing on the 59.** If it ever
  stopped dropping them, `1d` and `1e` would depend on the session receiving bytes the
  stage no longer forwards, and the failure would be a timeout rather than a clear error.
  Decision 4's exact counts are what would catch it.
- **This was found by a test written on suspicion, not by design.** Nothing in ADR-0020
  predicted it. That is an argument for the suspicion, not evidence that the next one will
  also be caught.

## Alternatives considered

**Forward an unreadable first frame to a shard anyway, and let the session decide.**
Rejected in decision 2: there is no identity, so there is no shard, and picking one means
an unroutable message chooses where a counterparty lands.

**Hold an unreadable first frame until the logon deadline.** Behaviourally close — the
socket still closes with no reply — and worse in two ways: it spends a slot from the
ceiling on a connection that has already failed, and it turns an immediate, deterministic
outcome into one that depends on a timer.

**Relax the corpus assertion to a range so this stops being a decision.** Rejected in
decision 4. That is the false green, put back by hand.
