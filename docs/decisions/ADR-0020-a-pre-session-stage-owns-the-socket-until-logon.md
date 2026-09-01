# ADR-0020 — A pre-session stage owns the socket until `Logon`, and hands the bytes on with it

> **Status:** **Accepted — 2026-09-01.** Step 1 of
> [pre-session-routing](../plans/2026-08-31-pre-session-routing.md), which the owner
> approved on 2026-08-31 after choosing **option A** — a pre-session stage, *the way real
> engines do it* — over a shared registry the engine consults.
>
> **Accepted by standing delegation**, `[2026-08-30]`, as ADR-0015 and ADR-0019 were.
> The shape of the layer was the owner's decision; everything below is how it is built.

- **Date**: 2026-09-01
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md),
  [ADR-0013](ADR-0013-two-modes-standard-and-hft.md),
  [ADR-0015](ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md),
  `DESIGN.md` D8 and §3, `CLAUDE.md` §2 non-negotiables 1, 2, 3 and 4,
  `STATUS.md` open item 24

## Context

`[measured 2026-08-31]` the 59 QuickFIX acceptance definitions score **59 through one
shard and 57 through two**, at both settle bounds so it is not timing, failing exactly
`1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` —
`crates/engine/tests/shard_wire.rs`.

**The rule was right and sharding invalidated its premise.** An `Engine` carries one
`Config`, therefore one FIX identity, so it answers *"is this identity already logged
on"* by counting the connections **it** holds that are logged on (`others_on`,
`crates/engine/src/lib.rs`). Split those connections across engines and there is nothing
left to count, and both `Logon`s are accepted.

**`Assign` cannot fix this, and not for want of a feature.** It is asked at `accept`
time, when the `Logon` — the only thing that says who the connection belongs to — has not
arrived. Nothing at that moment knows whose socket this is.

So the question is not *"how do we assign more cleverly"* but **"who owns the socket
while nobody knows who it is"**. That is a stage, and every real FIX acceptor has one.

## Decision

### 1. A `PendingSet` owns the socket until the first whole message, on the acceptor thread

The stage lives in a new `crates/engine/src/presession.rs`. A `Pending` owns a
`TcpTransport` and a small fixed buffer, reads non-blocking, and **builds no session at
all**. It looks for the first complete message and nothing else.

It runs on the **acceptor** thread, which ADR-0013 leaves free to block in both modes.
Non-negotiable 4 is about the *engine* thread; putting this work there would be the
violation, and putting it on the acceptor thread is what keeps the engine thread's turn
at `[measured 2026-08-31]` 449 ns of pure sweep.

### 2. It reads bytes. It never becomes a second session layer

`35=`, `49=` and `56=` come off the buffer by direct scan, the way `msg_type_is_logon`
already does. **No dictionary, no parse, no `fixbolt_session` beyond `Config`.**

This is checkable rather than aspirational: `presession.rs` may not `use
fixbolt_session::` anything but `Config`, and the plan's exit criteria grep for it. A
stage that has to ask the session a question has been designed wrong, because the session
it would ask does not exist yet — that is the whole reason this stage exists.

### 3. The bytes are handed on with the socket, and this is the decision that carries the risk

**The stage reads the `Logon`. The session must still see it.** A stage that consumes the
message it routed on produces an acceptor that accepts connections and answers nothing —
the failure would appear as a hung counterparty, not as an error.

`Engine` gains **`add_with_prefix(transport, prefix) -> Result<ConnId, PrefixTooLong>`**.
It needs no new machinery underneath: `Framer` already exposes `spare()` and `filled(n)`,
which is exactly "here are bytes that arrived before you were watching".

**A prefix longer than `RX` is refused, not truncated.** Truncation would hand the session
half a message and the framer would report `Garbage` about bytes that were fine when they
arrived — a defect whose evidence has been destroyed by the code that caused it.

### 4. Two hard limits, and **neither has a default**

| Limit | Why it exists |
|---|---|
| **Time to `Logon`** | A connection that says nothing is closed. Without it this is a DoS hole: open sockets, send nothing, hold slots forever |
| **Number of concurrent `Pending`** | The table has a ceiling. Full means the next connection is refused **immediately**, not queued |

**The caller must state both.** No `Default`, no "sensible" value, for the same reason
`ShardPlan` makes the caller name its cores (ADR-0015 decision 1): a limit nobody chose is
a limit nobody has thought about, and both of these are the difference between an acceptor
and an open port.

### 5. The acceptor thread waits with a **bounded** wait, not in `accept`

This is the consequence that decision 4 would otherwise quietly lose. `accept_blocking`
parks the thread until a connection arrives; a thread parked there **cannot expire a
silent connection**, so the logon timeout would fire only when somebody else happened to
connect — the load-dependent behaviour `CLAUDE.md` §10 names.

So the loop waits on the listener **and** every pending socket through `Poller::wait`
with a timeout, and the timeout is what makes the deadline real. `serve_sharded_hft`
stops calling `accept_blocking`.

### 6. `Route` replaces `Assign`, and `RoundRobin` is deleted

```rust
pub struct Identity<'a> { pub sender: &'a [u8], pub target: &'a [u8] }

pub trait Route: Send {
    fn shard_for(&mut self, id: Identity<'_>, shards: usize) -> usize;
}
```

Out of `0..shards` is refused rather than taken modulo: a policy that names a shard which
does not exist has a bug, and silently folding it hides the bug and lands the connection
somewhere arbitrary — which is how the single-logon rule breaks again, quietly.

**`RoundRobin` is removed, with no compatibility shim.** It is the policy that produced
the defect. Keeping it after knowing that is leaving a documented trap in the public API,
and `Assign` was itself only a day old with no users outside this repository.

### 7. The default `Route` is a hand-written stable hash of `(sender, target)`

**Stable is the requirement, not fast.** The same identity must reach the same shard on
this run, on the next run, and after a reconnect — that is what makes the single-logon
rule true again, because the rule can only count connections one engine holds.

`std::collections::hash_map::DefaultHasher` is therefore **forbidden here**: it is seeded
per process, so two runs of the same binary would route the same counterparty differently
and the rule would hold within a run and fail across a restart. The hash is written out,
and a test asserts a specific identity reaches a specific shard rather than asserting
merely that it is deterministic within one process.

The pair is hashed **in wire order** — `49` then `56`, as they appear in the incoming
`Logon` — so both connections from one counterparty hash alike.

Hashing is the sensible default and explicitly **not** the final answer: real HFT
deployments shard by counterparty deliberately, and `Route` is the seam for that.

### 8. Time comes from the engine's `Clock`, injected

No second time source. `SystemClock` in production, `ManualClock` in the tests, so the
logon-timeout cases are deterministic rather than sleep-based — a timeout test that
sleeps is a timeout test that is flaky on a loaded machine.

### 9. A first message that is not a `Logon` is dropped in silence

No reply, socket closed. The same shape `Connection::turn`'s `refuse` closure already
uses, and the same shape `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` expect:
those files wait for **no response at all** on the second connection.

### 10. The single-logon rule inside `Engine` is not touched

`others_on` stays exactly as it is. The rule was never wrong; the place connections were
delivered to was. Changing both at once would leave nothing to attribute a fix to.

## Consequences

**Good**

- The 59 definitions become the gate for sharding as well as for the session, which is
  what non-negotiable 3 asks for. `two_shards_break_the_single_logon_rule_and_this_records_it`
  goes red when the defect is fixed, and that is the point of it.
- The engine thread gains nothing to do. The whole stage runs where blocking is allowed,
  so §8's budget is untouched.
- An acceptor gets a logon timeout and a connection ceiling, which it did not have at all.
  That is a security property, not a routing one, and it arrives with the layer that makes
  it expressible.
- `Route` is a better seam than `Assign` was, because it is asked the only question a
  router can usefully answer.

**Bad, and named**

- **A new stage between the socket and the session is a new place for bugs**, and the
  worst of them — losing the `Logon` — is invisible to a test that only checks the
  connection was routed. Decision 3 is why it is called out; the plan's step 5 is what
  proves it, by requiring the session to answer that `Logon` normally.
- **A breaking API change to something published a day earlier.** `Assign`, `RoundRobin`
  and `Shards::with_assign` all go. Cheap only because nothing outside this repository
  uses them, and it is recorded in `CHANGELOG.md` rather than smoothed over.
- **The stage costs a `Logon` some latency**, and how much is unmeasured until step 6.
  It is on the connection path rather than the message path, so it does not touch §8 —
  but "does not touch §8" is a claim that needs the measurement, not a reason to skip it.
- **The pending buffer is memory per half-open connection**, and the ceiling is what
  bounds it. A caller that sets the ceiling high has bought back the DoS it was given a
  defence against, and nothing can stop them.
- **Hashing fixes the identity to a shard count.** Change the number of shards and every
  identity may move. There is no rebalancing and none is planned; a deployment that
  changes shard count restarts.

## Alternatives considered

**A shared registry the engines consult** — the option the owner was offered and did not
take. Every engine asks a lock-free set "is this identity logged on" before accepting a
`Logon`. It keeps `Assign` and needs no new stage, and it puts a **shared, contended
structure on the engine hot path**, which is what D8 and non-negotiable 1 exist to
prevent. It also leaves the same identity spread across shards, so nothing else about the
deployment gets simpler.

**Let the first `Logon` create the routing entry, and route later connections to whatever
shard the first one landed on.** Simpler than a hash and genuinely stateful in the wrong
way: the mapping then depends on connection order, cannot be reproduced after a restart,
and needs its own eviction policy. A stable hash needs none of that.

**Give the two limits defaults so callers need not think.** Rejected in decision 4. Every
default here is a number chosen by somebody who does not know the deployment, and the
failure mode of the wrong one is an acceptor that either drops healthy counterparties or
holds sockets for anyone who asks.

**Parse the `Logon` properly, with the dictionary.** Rejected in decision 2. The stage
needs three fields and no validation — the session validates, and it is about to see the
same bytes. Parsing here would duplicate the rule and give two places for it to disagree.
