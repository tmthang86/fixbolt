# ADR-0087 — A socket harness settles on counted records, and the clock does not move while the engine still owes an answer

- **Status**: Accepted — 2026-09-20; **superseded in part by [ADR-0091](ADR-0091-the-socket-harness-race-is-the-loopback-stacks-not-the-schedulers-and-its-reversal-runs-on-macos.md)** the same day: the *Context* sentence naming a descheduled process, and decision 5's reversal clause, are withdrawn there — the race is the loopback stack's and reachable on macOS only; decisions 1–4 stand. The owner delegated the technical decisions of
  [the-desk-free-residue](../plans/2026-09-20-the-desk-free-residue.md) to the architect on
  2026-09-20; this ADR is written from that mandate, before the plan is built. **Nothing here
  is built**: every test and script named below is *to be written* in step 4 of that plan.
- **Date**: 2026-09-20
- **Deciders**: Tran Manh Thang (mandate); written by the architect.
- **Related**: [ADR-0001](ADR-0001-relationship-to-quickfix.md) (the `.def` corpus is the
  oracle), [ADR-0042](ADR-0042-a-second-implementation-is-the-only-independent-opinion.md),
  `CLAUDE.md` §2 non-negotiables 2 and 3,
  [the-same-commit-went-red-and-green-in-the-same-minute](../reference/the-same-commit-went-red-and-green-in-the-same-minute.md)
  (the counts this ADR answers),
  [quickfix-acceptance-def-format](../reference/quickfix-acceptance-def-format.md).
- **Touches**: `crates/engine/tests/wire.rs`, `crates/engine/tests/wire_fixt.rs`, a new
  `scripts/check-socket-corpus-under-contention.sh`, one CI step. **Not**
  `crates/conformance/`, not `crates/engine/src/`, not `crates/session/`.

## Context

`the_sixty_fix50sp2_definitions_pass_through_a_real_socket` (`crates/engine/tests/wire_fixt.rs`)
is load-dependent: `[measured 2026-09-20]` 0 red in 40 sequential runs, **11 red in 50** as ten
concurrent copies, **8 red in 50** the same way on `main` at `64ea6c2`. The failure's own output
is one comparison shifted by one message — `FieldCount { expected: 14, actual: 8 }` followed by
`FieldCount { expected: 8, actual: 14 }` and a trailing `35=5|34=5` nobody asked for — so the
engine sent one extra message with a sequence number of its own, and every `34=` after it is off
by one.

The corpus is the acceptor half of the QuickFIX acceptance definitions, replayed by
`crates/conformance`'s runner. That runner is written for a **pure** session (`CLAUDE.md` §2
non-negotiable 2): it owns the clock, and `runner.rs:441-447` moves that clock forward **one
whole `HeartBtInt`** whenever an `E` line is reached and `pending` — the messages the session
has produced and no `E` line has claimed — is empty. In process that is exactly right: a pure
session answers synchronously, so an empty `pending` means "the session has nothing to say", and
only time can make it speak.

Over a socket the same sentence has two meanings. `Wire::pump` returns when nothing has moved for
`STEP_QUIET = 1 ms` of **wall time** or after `STEP_DEADLINE = 50 ms`. Under contention the
process is descheduled, the loopback bytes have not been read by the engine when the pump gives
up, and `pending` is empty **because the answer is late, not because there is none**. The runner
then does what it does for silence — advances `ManualClock` by 30 s — and the engine, seeing 30 s
without inbound traffic, correctly emits a heartbeat. The engine is not misbehaving. The harness
told it half a minute had passed.

So the item as filed — *"the oracle cannot say this message may appear anywhere"* — names the
wrong layer. The oracle was never asked to tolerate anything; the harness asked the engine a
question it had no right to ask yet.

### What the search found

The architect read the upstream runners on 2026-09-20 to see how they treat an unsolicited but
legal message in a positionally compared transcript.

- **QuickFIX (C++), `test/ReflectorClient.rb`**: an `E` line calls `@parsers[cid].readFixMessage`
  — the next message off the socket, blocking — and compares it at once (`compareAction`).
  Nothing is skipped or filtered by message type. There is no synthetic clock at all: the
  runner waits on the wire.
- **QuickFIX/J, `ExpectMessageStep.java`**: `connection.readMessage(clientId, TIMEOUT_IN_MS)` with
  `TIMEOUT_IN_MS = 10000`, settable through `atest.timeout` (`AcceptanceTestSuite.java`).
  `TestConnection.getNextMessage` is `messages.poll(timeout, MILLISECONDS)` over a queue every
  received message is added to — **no message type is dropped**. The only heartbeat-specific
  code is `heartBeatOverride`, which skips the *value* of field `108` in the comparison; it does
  not skip heartbeat messages.
- **quickfixgo, `_test/`**: the same Ruby files as QuickFIX C++ (`Reflector.rb`,
  `ReflectorClient.rb`, `Comparator.rb`, `Runner.rb`), so it inherits the same behaviour. Not
  read line by line.
- The QuickFIX acceptance documentation says what `E` does and nothing about unexpected messages,
  heartbeats or timing beyond the `<TIME±n>` macro.

**No upstream runner tolerates an unsolicited message.** They avoid provoking one: they never
simulate time, they wait up to ten real seconds for the expected message, and the definitions
that do not test timers say `108=30`, longer than any test runs. This repository simulates time
so the in-process gate is deterministic — a strength the upstream runners do not have — and the
socket harness inherited the simulation without inheriting the precondition that makes it safe:
**the clock may move only when the engine is idle.**

## Decision

### 1. The socket harness settles on two counted facts, not on a quiet interval

The socket `Wire` in both `tests/wire.rs` and `tests/wire_fixt.rs` installs a counting
`MessageLog` (`Engine::with_log`) and a step is *settled* when

1. **the engine has consumed everything the harness sent** — records with `Direction::In`
   (`crates/engine/src/conn.rs:448`, "read off the socket, before the session judged it") are at
   least the number of framable `I` lines written on that connection so far; and
2. **the harness has read everything the engine wrote** — messages drained off the client socket
   are at least the records with `Direction::Out` (`conn.rs:840`).

Only then does the existing 1 ms quiet apply, and only as belt and braces. The facts are the
settle; the interval is no longer load-bearing, and the test says so in its own doc comment
where the old comment praised wall time.

### 2. The clock does not move while the engine owes an answer

`Wire::step(Input::Tick)` first waits for fact 1 on every connection, and only then sets
`ManualClock`. A tick that arrives while bytes are unread is the exact event that produced the
extra heartbeat, and refusing it is what makes the runner's "empty `pending` means silence"
true over a socket again.

### 3. The deadline becomes a lifeline, and reaching it is reported

`STEP_DEADLINE` rises from 50 ms to **5 s**, and reaching it prints one line naming the file,
the line and which fact was unmet. It is not a settle and never a pass: a step that hits it
will fail its `E` line the way it does today. The number is generous because it is reached
only when the engine is broken or the machine is unusable, and a lifeline that trips on a busy
CI runner is the flake this ADR exists to remove.

**This is not the move `tests/wire.rs` warns against.** That warning — a score that climbs with
its own timeout is measuring something else — applies to a bound that *decides* a pass. After
this ADR no bound decides anything; a fact does, and the timeout only stops a broken run from
hanging.

### 4. The runner is not changed, and the oracle is not softened

`crates/conformance` is the gate for `CLAUDE.md` §2 non-negotiable 3 and stays byte for byte.
No message type becomes ignorable, no `E` line becomes optional, and the in-process tests keep
their zero-tolerance positional comparison. A harness that skipped `35=0` would also have to
forgive every later `34=`, and at that point it would forgive a real defect.

### 5. Contention is a gate, not an anecdote

`scripts/check-socket-corpus-under-contention.sh <rounds> <copies>` runs the prebuilt socket
test binaries `rounds × copies` times, `copies` at a time, and exits non-zero on any red,
printing `N red in M`. It runs in CI's `gates` job at a size a two-vCPU runner can carry and is
the reversal for this ADR: on the tree before step 4 it must read at least one red in fifty; on
the tree after, zero.

## Consequences

**Good**

- The known cause is removed rather than tolerated. The engine's behaviour, the oracle and the
  in-process gate are untouched; only the harness's premise is repaired.
- The mechanism is *observed*, not inferred: step 4 of the plan reproduces the red first and
  prints the extra message's `35=`, and the ADR is falsified if it is not a heartbeat.
- The socket tests gain a settle that says what it waited for. "Timed out" no longer has two
  causes: an unmet fact is named on the line that hit the lifeline.
- The same `CountingLog` is a reusable instrument for `tests/shard_wire.rs`, whose own comment
  admits a wall-time floor set by the machine's scheduler.

**Bad — and accepted**

- **The socket harness now depends on `MessageLog` firing for every frame.** If a future change
  stops recording an inbound frame (say, garbage the framer skips), the harness waits to the
  lifeline on that line and the run slows to 5 s per such line rather than failing. The lifeline
  line printed in the log is the only signal; the plan's step 4 asserts that the sequential run
  hits it zero times.
- **`I` lines the harness's own framer cannot frame have no fact.** `[measured 2026-09-20]`
  a handful of the 290 `I` lines in the `fix50sp2` corpus carry a wrong `9=` or a garbled tag on
  purpose. For those the step settles on the 1 ms quiet exactly as today, so the race is
  narrowed, not proven absent, on those lines.
- **A residual window remains between "the engine wrote its reply" and "the engine closed the
  socket"** for definitions ending in `eDISCONNECT`. Fact 2 covers the reply; the close is
  detected by `drain`'s `Ok(0)` and still waits on wall time. If it ever shows, the contention
  script will count it and the failure output will name the file.
- **CI gets longer.** The contention step is one more minute in the `gates` job, and a
  genuinely broken engine now fails slowly — 5 s per step — instead of in 50 ms.
- **Three harnesses, three copies of the fix.** `wire.rs` and `wire_fixt.rs` duplicate their
  pumps deliberately (the FIX 4.4 gate must pass unmodified while the FIXT one changes), so the
  counting settle is written twice, and `shard_wire.rs` is left with its wall-time settle by
  scope. A shared helper would be one file two gates can break from at once, which is the
  reason the duplication exists; the price is that a later fix to one pump must be carried to
  the other by hand.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| Teach the comparator to skip `35=0` / `35=1` | Every later `34=` is off by one, so the skip has to forgive sequence numbers too, and then it forgives a real defect. No upstream runner does this either |
| Raise `STEP_QUIET` / `STEP_DEADLINE` until the red goes away | The move `tests/wire.rs` records as measuring the scheduler and calling it FIX. A bound that decides a pass is the disease |
| Give `SessionUnderTest` a `settled()` method and let the runner ask before ticking | Correct, but it edits `crates/conformance`, the non-negotiable 3 gate, to fix a socket-only problem. The `Wire` already owns both ends of the socket and can hold the same rule alone |
| Stop the runner ticking on an empty `pending` | Breaks the 33 `E` lines that *are* a wait — a heartbeat after silence, a test request — which only a tick can produce |
| Use `HeartBtInt` large enough that no tick can fire | The tick is one whole `HeartBtInt` by design; a larger value moves the clock further, not less |
