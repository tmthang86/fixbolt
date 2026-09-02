# ADR-0043 — Backoff without jitter, and a reconnect asks recovery every time

- **Status**: Accepted — 2026-09-02
- **Date**: 2026-09-02
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md) — a reconnect is not a
  restart, which is the whole reason for decision 2 ·
  [ADR-0039](ADR-0039-a-fresh-journal-is-the-deployments-to-build.md) — the `Recovery` seam this
  reuses · [ADR-0033](ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md) —
  the `Schedule` decision 3 leans on · [ADR-0013](ADR-0013-two-modes-standard-and-hft.md) — why
  decision 5 is a limit and not an oversight · [plan](../plans/2026-09-02-an-initiator-that-comes-back.md)

## Context

`fixbolt_engine::connect(addr)` was the whole of the initiator's surface: one blocking
`TcpStream::connect`, one `TcpTransport`. An initiator that lost its connection was on its own.

**Nothing covered it, and that is the part worth stating.** The 59 acceptance definitions are
written for an acceptor and never reconnect an initiator; the mirrored corpus is at 2 / 50;
`scripts/interop.sh` connects once and logs out. So this is the first behaviour in the engine
with **no external oracle at all** — every test of it measures this project against its own
reading, which is the weakness `crates/dict/tests/field_types.rs` already carries and names.

## Decisions

### 1. Exponential backoff with a ceiling, and **no jitter**

`first_ms`, doubling, capped at `ceiling_ms`. `Policy::new` refuses a zero first delay and a
ceiling below the first — the second is almost always two arguments the wrong way round.

**No randomness.** `codec` has a zero-dependency rule, `engine` has no RNG, and adding one is a
dependency plus a source of nondeterminism in a test suite that currently has neither.

**What that costs is real and is not hidden**: N initiators that lost a *shared* counterparty
reconnect in lockstep, and the venue coming back up gets all of them at the same millisecond,
repeatedly, at every rung. That is the failure mode jitter exists to prevent. It is accepted
here because this engine's shape is *one session on a polling thread*
([ADR-0012](ADR-0012-latency-first-and-one-session-per-polling-thread.md)) rather than a fleet,
and because the fix is additive when a fleet appears. **It is `STATUS.md`'s to carry, not this
ADR's to forget.**

### 2. `recovery` is asked on **every** attempt, not only the first

ADR-0010: FIX 4.4 numbers a **session**, not a connection. Whether a new connection continues
the old numbering is therefore a question, and it already had an answer — `Recovery::recover`,
built for the acceptor's serving loop, which returns the journal *and* the counts *and* the
instant the session was last active.

So `connect_and_serve` asks it per attempt and needs **no new `Engine` API**. `add_resumed` had
one caller; now it has two, and both go through the same seam.

**The consequence is stated in the rustdoc rather than left to be discovered**: with
`NoRecovery` every reconnect starts at `34=1`. That is right for an in-memory journal — it could
not have replayed anything anyway — and **wrong for a counterparty that expects continuity**.
A deployment that wants numbers to survive passes a `Recovery` backed by a journal on disk,
exactly as `serve_with_recovery` does.

### 3. The schedule is asked before the ladder, and it re-asks rather than computing an opening

Outside its hours the answer is never *connect now*: dialling a shut venue earns a refusal, and
a refusal climbs the ladder for a reason that has nothing to do with the network.

It answers `At(now + ceiling)` — **ask again later** — rather than naming the instant the window
opens. `Schedule` can say whether an instant is inside a window and **cannot compute the next
opening**; adding `next_open` is a change to `session`'s public API and belongs to whoever needs
it. Asking twice a minute while a venue is shut costs nothing.

`[measured 2026-09-02]` the ordering is load-bearing and its test nearly was not: asserted at an
instant where the ladder had already come due, **both orderings give the same answer** and the
reversal was a no-op. They disagree only while the ladder is pending.
[a-reversal-needs-an-input-where-the-answers-differ.md](../reference/a-reversal-needs-an-input-where-the-answers-differ.md).

### 4. The policy answers a question; it never sleeps, and it owns no clock

`Next::At(instant)`, and the caller waits however it already waits. Non-negotiable 4 is about
the thread this loop runs on, so a policy that slept would put a block in the one place the
design is least willing to have one. Time arrives as an argument, as everywhere else here.

### 5. Every ending climbs the ladder, including a clean logout

A policy that counted only *failures* would reconnect instantly after a goodbye — a reconnect
storm with a polite name. A caller that meant to stop calls `stop`.

And `logged_on` — not *"a socket connected"* — is what resets it. A TCP connection that is
refused its `Logon` and dropped is a failure; counting it as success is how a policy hammers a
counterparty that is up but refusing, which is the case backoff exists for.

## Consequences

### Good

- **An initiator can be left running.** It survives a venue restart, a network blip and a
  deliberate hang-up, and it stops when told.
- **No new `Engine` API.** The reconnect loop reuses `Recovery` and `add_resumed`, so there is
  one way to continue a session and both loops use it.
- **The policy is testable with no I/O and no clock** — `tests/reconnect.rs` runs in
  microseconds and nothing in it sleeps.

### Bad, and accepted

- **No jitter.** Decision 1 names the failure mode it leaves open.
- **`NoRecovery` restarts the numbering on every reconnect.** Correct for what it is, and a
  loaded footgun for anyone who reads "reconnect" and assumes continuity. Rustdoc and `GUIDE.md`
  carry it; the type system cannot.
- **`standard` only. There is no `hft` initiator entry point.** `connect_and_serve` builds the
  blocking engine, exactly as `serve` does, and `serve_hft`'s counterpart does not exist. An
  `hft` deployment that dials out still drives `Engine` itself — which is what it did before
  this ADR, so nothing is taken away, but nothing is added either.
- **One session, one socket.** Many initiators in one process, sharding for the outbound
  direction, and a registry for it are all out of scope and stay out until something asks.
- **Every test of this is invented.** No corpus covers reconnect. A rule everybody would agree
  with but nobody wrote down here would pass, and only an interop scenario driving a real
  counterparty through a disconnect would close that — which `scripts/interop.sh` could grow and
  today does not.
