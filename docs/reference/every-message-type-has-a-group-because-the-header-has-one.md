# Every message type has a group, because the header has one

`[measured 2026-09-19]`

## The shape

Step B4b deferred `373=5` / `373=6` on a repeating-group member until after `373=1` and
`373=16`. That costs a per-field question — *is this tag a member of a group whose counter I
have already passed?* — and `benches/validate.rs` showed `validate Heartbeat` moving 140.7 ns
→ 166.2 ns while `validate NewOrderSingle` moved 877.5 → 906.2.

The obvious optimisation, and the manager proposed it: a Heartbeat cannot have a group, so ask
once per **message** instead of once per **field**. `FIX44.xml` says as much —

```
<message name='Heartbeat' msgtype='0' msgcat='admin'>
  <field name='TestReqID' required='N' />
</message>
```

one child, no `<group>`. Gate the whole deferral on a generated `has_groups(msg_type)` and a
Heartbeat pays nothing.

**It is wrong, and every gate in this repository is blind to it.**

## Why

`FIX44.xml` declares one `<group>` in its `<header>`: `NoHops(627)`, with `HopCompID(628)`,
`HopSendingTime(629)` and `HopRefID(630)`. The generator already says what that means, at
`crates/dict/src/codegen/emit.rs:283` (it was in `crates/dict/build.rs` until the generator moved
into `src/codegen/`, ADR-0207 decision 1):

> The header's one group, `NoHops(627)`, can appear in ANY message, so it is keyed under the
> empty message type and emitted without a `msg_type` arm.

So, measured against the generated FIX 4.4 table:

```
group_delimiter(b"0", 627) == Some(628)
group_members(b"0", 627)   == [628, 629, 630]
allows(b"0", 627) … allows(b"0", 630)  all true
```

A Heartbeat **can** carry a populated repeating group. `<message msgtype="0">` having one
child is true and is not the whole message: the header is part of what the scan walks, and D2's
flat index hands `627`, `628`, `629`, `630` to the validation pass exactly like a body field.

The honest answer to *"does this message type declare any group?"* is therefore **`true` for
all 93 FIX 4.4 message types**, and for all 164 in the FIXT pair table. A generated function
that is constant-true gates nothing; one built from the **per-message** `groups` map — which is
what a careless implementation produces — comes out as 75 arms with `b"0"` absent, and then
skips the deferral on precisely the messages that can carry the header group.

## What no gate would have caught

The gate was built for real rather than argued about. With it in place:

```
cargo test -p fixbolt-session --test score              4 passed        (59 / 59)
cargo test … --features fix50sp2 --test score_fixt      fix50 59/60 sp1 60/60 sp2 60/60
```

Both **green**. No `.def` in the 59 or the 180 sends a populated `NoHops`, so the corpora
cannot see it. It would have shipped on every gate this project has and broken the ordering
rule on the admin path — the same class of latent defect ADR-0084 decision 2 exists to close.

What caught it was a test written **before** the optimisation, from the table rather than from
the XML: a Heartbeat with `627=3`, two hops and a bad `630`, requiring `373=16` naming `627`.
With the gate it printed `373=6 371=630` instead.

## The rule to take away

**A message type's own `<message>` element is not the set of fields it can carry.** The header
is part of every message, and anything the header declares — a field, a group, that group's
members — belongs to all 93 of them. Any per-message-type question about structure has to be
asked of the *table*, which folds the header in, and not of the XML element with the matching
`msgtype`.

`ALLOWED` already works this way for tags: `build.rs` folds header and trailer bits into every
message's bitset so a caller asks once rather than three times. The group tables do the same
thing by keying the header group under `b""`. A reader who knows the first can still miss the
second.

## What guards it

`crates/session/tests/group_member_values.rs`:

- `the_header_group_is_on_every_message_type_including_a_heartbeat` — the three table facts
  above, asserted rather than described;
- `a_bad_hop_count_is_answered_before_a_bad_hop_value_on_a_heartbeat` — the ordering rule on
  an admin message;
- `a_hop_value_is_still_refused_once_the_hop_count_agrees` — the member is still checked.

## Related

- [ADR-0084](../decisions/ADR-0084-a-session-message-is-checked-against-the-layer-that-defines-it-a-members-value-waits-for-the-count-and-fix50-is-its-own-oracle.md)
  decision 2 — the ordering rule this would have broken.
- [a-feature-gated-test-is-a-test-ci-never-runs](a-feature-gated-test-is-a-test-ci-never-runs.md)
  — the other trap found this phase where the gates were green about something they never ran.
- `crates/dict/src/codegen/emit.rs:283` — the comment that already said it, to whoever went looking.
