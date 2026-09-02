# ADR-0036 — One mechanism, two capabilities

- **Status:** Accepted
- **Date:** 2026-09-02
- **Related:** [ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md),
  [ADR-0032](ADR-0032-observation-is-a-snapshot-taken-on-request.md),
  [ADR-0035](ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md)
- **Plan:** [2026-09-02-sequence-numbers-at-three-in-the-morning.md](../plans/2026-09-02-sequence-numbers-at-three-in-the-morning.md)
- **Closes:** `STATUS.md` open item 30 (c)

## Context

**The operation every FIX operator performs, and this engine had no path for it.** The
counterparty calls at 3 a.m.: *"our next is 4812, what is yours?"* In QuickFIX the person on
call sets `setNextSenderMsgSeqNum`. Here, `Session::resume` is a **constructor** — changing one
number meant rebuilding the session, which meant stopping the engine.

`[verified 2026-09-02]` `Engine` had no public function that touched a running session's
sequence numbers: `conns` is private, and `next_out`/`next_in` are read-only.

And every session lives on the engine thread, which non-negotiable 4 forbids from blocking. So
this was never a missing setter. It was a missing **direction**: ADR-0032 and ADR-0035 carry
facts outward, and nothing carried instructions in.

## Decision

### 1. One mechanism, two handles

`Commands` lives inside the existing `observe::Shared`, behind the same `Arc`, with the same
fixed shapes. The engine hands out **two different handles over it**: `Observer`, which can only
look, and `Admin`, which can change things.

ADR-0035 decision 3 rejected a second parallel mechanism because two mechanisms are two things
that will disagree. That still holds here — one `Arc`, one `try_lock` discipline, one fixed
container. **What is separated is the capability, not the mechanism**: hand `Observer` to
everything that watches, and `Admin` only to what administers.

### 2. Applied at the top of the turn, before anything is numbered

A command applied after the turn's messages sets a number that has already gone out, and the
operator's change misses by exactly one message — the kind of defect that is invisible until the
counterparty complains. `crates/engine/tests/admin.rs::a_command_lands_before_the_same_turn_numbers_anything`
holds the order, and `[measured 2026-09-02]` moving the call to the end of `turn()` turns it and
one other test red while the other six stay green.

### 3. The operator's thread may block; the engine's may not

`Admin::submit` takes `lock()`. The drain takes `try_lock()`. That asymmetry is the whole design:
non-negotiable 4 constrains **one** of the two threads, and the other is free to wait.

### 4. A refused lock loses nothing — the opposite of an event

ADR-0035 lets a full ring drop the oldest event and count it, because an event that is lost is a
fact somebody did not learn, and the counter tells them that instead.

**A command that is lost is an action that silently did not happen, and there is no counter that
makes that acceptable.** So a refused `try_lock` takes nothing and leaves the queue exactly as it
was; the next turn tries again. A full queue is refused **at the call**, so the operator learns
now rather than by the command quietly never happening.

`COMMAND_CAPACITY` is 32, small on purpose: a command is a human action, not a data stream, and a
full queue means something is submitting in a loop.

### 5. The outcome goes on the event stream, not back from `submit`

`submit` returns *queued or not*. What became of the command arrives as
`EventKind::Administered { change, to, outcome }`.

It cannot be otherwise: `Outcome::NoSuchConnection` is the ordinary answer for a command that
raced a disconnect, and at submit time there is nothing to check against. Putting it on the same
stream means **one channel records both what the engine did by itself and what it did because
somebody asked** — an operator reading only their own outcomes would not see the disconnect that
overtook their command.

### 6. Three commands, and each states whether it speaks

| Command | On the wire |
|---|---|
| `SetNextIn` | **Nothing.** What you expect is your own business |
| `SetNextOut` | **Nothing** — and it is a lie until the counterparty is told. Named after QuickFIX's `setNextSenderMsgSeqNum` rather than improved on, because an operator who knows that name knows what it does |
| `SendSequenceReset` | `35=4`, `123=N`, `36=n`. The honest way to change an outbound number |

`SendSequenceReset` sends at the current number and becomes `n` **after** it, which is what
`36=n` promises. A reset that moves the number **down** is permitted: it is a last resort, the
counterparty will accept numbers it has already seen, and an operator on the phone at 3 a.m.
sometimes needs exactly that. There is a test asserting the permission so it reads as deliberate.

### 7. No `_` arm

`Connection::administer` matches `Command` exhaustively. `Command` is `#[non_exhaustive]` for
callers outside the crate and exhaustive inside it, so a command added and not given behaviour
**will not compile**. `[measured 2026-09-02]` clippy found the catch-all already unreachable; it
was removed rather than silenced.

### 8. One relaxed load, and a counter that makes the claim falsifiable

The first version reached for the command mutex **on every turn** as soon as an `Observer`
existed. Every test passed. It was a worse bargain than ADR-0032 claims for this mechanism —
*"the cost while nobody is watching is one relaxed load"* — and nothing said so.

So `Commands` carries an `AtomicUsize` read before the lock is attempted, and an `AtomicU64`
counting the attempts, exposed as `Admin::drains()`.

**That counter is the point.** An engine that takes the lock every turn applies the same
commands, reports the same outcomes, and leaves every content assertion green.
`[measured 2026-09-02]` removing the relaxed check turns exactly one test red, reading
`left: 1002, right: 2`.

This is the *same* gap that was already found here once: `Observer::published()` exists because
removing the snapshot's `wanted` flag left every content assertion green while the engine built
84 555 snapshots nobody had asked for. **Two mechanisms, one blind spot, and it was not noticed
until the §2 walk** — the code review that found it was a hand-walk of the non-negotiables, not
a test.

## Consequences

**Good**

- The 3 a.m. operation exists, from another thread, without stopping the engine.
- `[measured 2026-09-02]` `benches/alloc.rs` cases `admin-idle` and `admin-busy` both read **0**,
  and `admin-busy` asserts the stream recorded something inside the counted window.
- The three reversals fail on **three different tests**, which is what says they discriminate
  rather than all measuring one thing.
- A `SequenceReset` goes out through the same bounded writer as everything else, so D10's
  backpressure applies: an operator cannot push past a consumer that has stopped reading.
- `[measured 2026-09-02]` a turn on an engine nobody is administering **does not touch the
  mutex**, and `Admin::drains()` is what proves it rather than a comment.

**Bad, and named**

- **Nothing authenticates the holder of an `Admin`.** The engine has no idea who is on the phone.
  Capability separation is all there is, and it is enforced by whoever passes the handle around.
- **`SetNextOut` is a loaded gun and the type system cannot say so.** It is documented as a lie
  until the counterparty is told; nothing prevents it being used when `SendSequenceReset` was
  meant, and the failure shows up as the counterparty's `ResendRequest`.
- **A reset downwards is permitted with no guard at all.** Anything the counterparty kept for a
  resend becomes ambiguous. This is deliberate, and it is still a foot-gun.
- **The outcome is not correlated to the submission.** Two identical commands produce two
  identical events; an operator submitting in a loop cannot tell which is which. A command id
  would fix it and was not added.
- **`COMMAND_CAPACITY = 32` has no measurement behind it**, like `EVENT_CAPACITY` before it.
- **A command cannot reach a counterparty that is not connected.** There is no `ConnId` to name.
  That is `Recovery`'s job (ADR-0034), and the two do not meet.
- **The order rule is held by one test.** Nothing structural prevents a later refactor moving
  `administer()` down the function.
- **`Admin::drains()` is a diagnostic that exists only to be asserted on.** It is public API
  that no deployment will read, and that is the price of making a cost claim falsifiable.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| A second `Arc` and a separate admin channel | ADR-0035 decision 3: two mechanisms are two things that will disagree |
| One handle with both powers | Everything that watches would also be able to reset sequence numbers. Least privilege costs one type here |
| `submit` returns the outcome | It cannot: `NoSuchConnection` is only knowable on the engine thread, on a later turn |
| `try_lock` on submit too | The operator's thread is allowed to block; refusing them for symmetry would lose commands for no benefit |
| Drop the oldest command when full, and count it | An action that silently did not happen. The counter that makes it acceptable does not exist |
| Apply commands at the end of the turn | The number set has already been used. Reversal 1 shows two tests catching it |
| Make `SetNextOut` always send a `SequenceReset` | Then the case it exists for — *the counterparty already told you their number* — becomes unreachable, and QuickFIX operators would find a familiar name behaving unfamiliarly |
