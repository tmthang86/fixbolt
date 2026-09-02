# ADR-0038 — An ordered shutdown is a state, not a flag

- **Status:** Accepted
- **Date:** 2026-09-02
- **Related:** [ADR-0011](ADR-0011-a-full-ring-disconnects.md),
  [ADR-0035](ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md),
  [ADR-0036](ADR-0036-one-mechanism-two-capabilities.md),
  [ADR-0037](ADR-0037-reading-a-journal-is-not-recovering-from-one.md)
- **Plan:** [2026-09-02-an-ordered-shutdown.md](../plans/2026-09-02-an-ordered-shutdown.md)
- **Closes:** `STATUS.md` open item 30 (a) — **and item 30 entirely**

## Context

`[verified 2026-09-02]` **there was no way to stop this engine.** `Engine::run` returned `!`;
`serve` and `serve_hft` returned `Result<Infallible, ServeError>`. The only exit was killing the
process, and that has three consequences, none of them theoretical:

| What happens | Why it is bad |
|---|---|
| The counterparty never receives a `Logout` | To them this is a **dead line**, not a planned close. They reconnect, possibly for hours |
| Bytes numbered but still in `tx` are lost | The sequence numbers were already spent. Next session, the counterparty sees a gap and asks to resend something that never went on the wire |
| The journal can be left with a torn tail | `[2026-09-02]` ADR-0037 made that **visible**, and visible means there should be less of it |

And one trap has already bitten: `[measured 2026-08-30]` dropping the engine while another
thread held a `WakeHandle` closed the self-pipe's read end, and `libc::write` into the write end
raised `SIGPIPE`, which terminates. So the shutdown path was **under-designed**, not merely
absent.

## Decision

### 1. `LoggingOut` is its own state, and that is the load-bearing decision

`State::AwaitingLogout` already existed, and reusing it was the obvious move. It is wrong.

`AwaitingLogout` reports the link **down at once** and reads-and-ignores everything that
arrives afterwards. That is right for the paths it serves — `2i_BeginStringValueUnexpected.def`
runs the same sequence with and without a reply and the link must go down either way, and
D10's backpressure paths are cutting on purpose.

`[measured 2026-09-02]` folding an ordered shutdown into it **made every wait vacuous**: the
next `tick` returned `Dropped` from the state check with no reason recorded, so *"they
answered"* and *"they never answered"* became the same observable. That is the shape this
repository has now recorded three times in one week — two rules sharing one observable — and
the fix is the same each time: give them different ones.

So `LoggingOut` keeps the link up, keeps the heartbeat running, and lets the counterparty's own
`Logout` be judged and reported as `DropReason::PeerLogout`.

### 2. `logout_now` is left exactly as it is

It is D10's path. One function serving both *cut now* and *wait for an answer* is how both come
to be wrong, and `crates/session/tests/goodbye.rs::logout_now_still_gives_up_the_link_at_once`
asserts the contrast rather than leaving it to the next reader's care.

### 3. Shutdown is asked through `Admin`, but is not a `Command`

Same `Arc`, same capability split as ADR-0036 — an `Observer` cannot stop the engine and an
`Admin` can. But it is not a `Command`, because every `Command` is about one connection and this
is about the engine's own life.

Asking twice is harmless and **the first grace period stands**: a second call must not be able
to extend a shutdown already under way.

### 4. The deadline belongs to the caller

A counterparty that has already died never answers, and nothing in the engine can tell that
apart from one that is merely slow. `grace_ms` is on the engine's clock, so a test drives it
with `ManualClock` rather than by waiting.

Without it, the reversal is not a failing test but a **hang** — `[measured 2026-09-02]` removing
the deadline made the suite run until it was killed at 600 seconds. The plan wrote that reversal
as *"must hang and fail, not pass"* for exactly this reason.

### 5. The report names what it could not do

```rust
Shutdown { sessions, said_goodbye, acked, timed_out }   // and clean()
```

*"We stopped"* and *"we stopped while two counterparties never answered"* are different facts.
The second means an operator may have to reconcile sequence numbers by hand before restarting,
and folding both into a bare return would hide exactly the case that needs a human.

### 6. Nothing leaves without a reason

`DropReason::EngineShutdown` is new. FIX has no `Logout` before a `Logon`, so a connection that
never got that far is **ended rather than sent a message it must not receive** — and it is ended
with a reason, because a shutdown that closed sockets anonymously would appear on ADR-0035's
event stream as `EndedWithoutReason`. Sessions still present at the deadline are given the same
reason and **emit an `Ended` event before the vector is cleared**; clearing it would take them
away without a word.

### 7. `run` returns, and the blast radius is zero

`-> !` became `-> Shutdown`, and `serve`/`serve_hft`/`pump` followed. `[verified 2026-09-02]`
**nothing in the repository called `run()`**, so this is a signature change with no in-repo
caller — a different situation from the one ADR-0034 deliberately routed around, and the reason
that precedent does not apply here.

**`serve_sharded_hft` is not touched.** It is Linux-only and cannot be run on the machine this
was built on. It still cannot be stopped, and that is in *Not proven* rather than implied to
work.

## Consequences

**Good**

- A deployment can stop cleanly, and knows whether it did.
- `[measured 2026-09-02]` the three reversals fail three different ways — 7 tests red, a hang,
  and 2 tests red — and the middle one is a failure mode a green-or-red table cannot express.
- `benches/alloc.rs` case `shutdown` reads **0**, with a control asserting every session was
  told *and* that the goodbye reached the wire.
- The `WakeHandle` sequence — shut down, then drop with a handle still out — is now driven by a
  test instead of being the thing that killed the process once.

**Bad, and named**

- **`serve_sharded_hft` still cannot be stopped.** A sharded deployment has no ordered shutdown.
- **The `SIGPIPE` test asserts the weaker thing.** The Rust runtime sets `SIG_IGN` before
  `main`, so an ordinary test cannot observe the original bug. What it proves is that the
  sequence completes and the wake afterwards is not an error — not that a host with default
  signal handling survives.
- **Nothing stops accepting during a shutdown.** `pump` keeps admitting sockets until the engine
  reports finished; they are dropped rather than told anything.
- **The application is not consulted.** D10 has a policy for a full ring, and there is no
  equivalent *"let the dispatcher drain"* phase. A shutdown with an out-of-band dispatcher can
  discard work the application had accepted.
- **`grace_ms` is on the engine's clock**, so an engine whose clock is not advancing never
  reaches its deadline. That is right for testing and is a foot-gun for a custom `Clock`.
- **The journal is not explicitly flushed.** `Durability::Async` joins its writer on drop, so
  dropping the engine after `run` returns is what makes it durable — a sequencing rule the type
  system does not enforce, named in `GUIDE.md`.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| Reuse `AwaitingLogout` | `[measured 2026-09-02]` it made every wait vacuous: the answer and the silence became one observable |
| A flag on `logout_now` | One function for *cut now* and *wait for an answer*; both end up wrong |
| Wait for ever for a goodbye | A dead counterparty never answers. The reversal is a hang, not a red test |
| Keep `run() -> !` and add `run_until` | Two entry points where one has no caller. The break costs nothing here |
| Signal handling inside the engine | The engine is a library; catching `SIGTERM` is the host process's business |
| Return a bare `()` from `run` | Hides the case that needs a human — the counterparty that never answered |
