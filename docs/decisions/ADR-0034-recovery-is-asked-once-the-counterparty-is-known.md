# ADR-0034 — Recovery is asked once the counterparty is known

- **Status:** Accepted
- **Date:** 2026-09-02
- **Related:** [ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md),
  [ADR-0017](ADR-0017-the-inbound-count-is-persisted-after-delivery.md),
  [ADR-0020](ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md),
  [ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md),
  [ADR-0033](ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)
- **Plan:** [2026-09-02-an-engine-can-resume.md](../plans/2026-09-02-an-engine-can-resume.md)
- **Closes:** `STATUS.md` open item 31

## Context

`[verified 2026-09-02]` **an `Engine` could not resume a session at all.** Both `add` methods
built `Session::new`, which resets; `conns` is private; nothing public reached
`Connection::new`. So `Journal::highest`/`highest_in`, `Session::resume`, ADR-0010, ADR-0017
and `Durability::Fsync` were every one of them real, tested, and unreachable through the type
a deployment actually uses.

**How that survived five days is the part worth recording.** Item 16 closed on 2026-08-31
truthfully — the journal *is* readable and `Session::resume` *does* work — and
`crates/engine/tests/recovery.rs` proves both with **zero occurrences of `Engine` in the
file**. A layer was finished and the seam above it was never asked about, by a plan whose exit
criteria were all satisfiable one layer down.

`Engine::add_resumed` closed the first half. It left a second: `serve`, `serve_hft` and
`serve_sharded_hft` **accept connections themselves**, so an embedder using the convenient
entry point never sees a transport to call it with. Recovery would have been a feature you had
to give up the serving loop to use.

## Decision

### 1. A `Recovery` trait, asked once per connection

```rust
pub trait Recovery<J> {
    fn recover(&mut self, cfg: &Config) -> Option<Resumed<J>>;
}
pub struct Resumed<J> { journal: J, next_out: u32, next_in: u32, last_active_ms: Option<u64> }
```

`None` means *"this counterparty left nothing"* and is a complete answer, not an error — the
same shape ADR-0026 chose for `Registry`, for the same reason: one deployment reads a
`FileJournal` off disk, another asks a database, a third is a test, and the engine has no
business knowing which.

### 2. Asked **after** the registry names the counterparty, and nowhere else

Before the `Logon` there is **no identity** — ADR-0020 has the pre-session stage own the
socket until one arrives, and ADR-0026 turns that identity into a `Config`. Until then there
is nothing to look a journal up by.

So there is exactly one call site: in `pump`, between `Registry::lookup` and the engine
receiving the connection. **On the acceptor thread**, which ADR-0020 explicitly allows to
block, so an implementation may read a file. It is not the engine thread and it is not a turn.

### 3. New entry points, not changed signatures

`serve_with_recovery` and `serve_hft_with_recovery`, with `serve` and `serve_hft` delegating
through `NoRecovery`.

Changing the existing signatures would have been defensible — nothing is published — but it
would ripple into `tools/w2w` and into `shard.rs`, which is Linux-only and cannot be run on
the machine this was built on. **A change whose blast radius includes code the author cannot
execute is a change to make smaller**, so this one was.

### 4. The journal travels with the numbers

`Resumed` carries `journal` as well as `next_out`/`next_in`. Correct counts over an empty
journal answer the first `ResendRequest` with a `SequenceReset` gap fill: legal FIX, and a
silent loss of exactly what the counterparty asked for. Two tests tell the outcomes apart
rather than assuming one.

### 5. `last_active_ms` is a separate field because the numbers cannot imply it

`next_out = 9` says nothing about whether a trading day has ended since 9 was reached. With
it, ADR-0033's boundary reset becomes reachable from an engine; `None` means no boundary is
ever noticed, which is right under `Schedule::always` and wrong under anything else.

### 6. The engine still does not guess

It does not read the journal for you and it does not compute `next_out` from `highest()`.
ADR-0010 is explicit that choosing between a restart and a continuation is the caller's; this
is where the engine **asks**, not where it decides.

## Consequences

**Good**

- A deployment can use `serve_with_recovery` and get sequence-number continuity, message
  replay and ADR-0033's boundary reset without giving up the serving loop.
- `[measured 2026-09-02]` the default is proven neutral by reversal: making `NoRecovery`
  return a fabricated session turns exactly one test red —
  `the_plain_serving_loop_still_starts_at_one` — and leaves 59/59 green.
- `[measured 2026-09-02]` making `pump` discard what `Recovery` answered turns exactly one
  other test red, and the control stays green. The two reversals fail on opposite tests, which
  is what says the pair discriminates.
- Both new tests go through a **real listener and a real socket**, because driving
  `Engine::add*` directly is precisely what hid this gap.

**Bad, and named**

- **`serve_sharded_hft` has no recovery variant.** It is Linux-only and was not touched.
  A sharded deployment still cannot resume.
- **Nothing persists `last_active_ms`.** `Session::last_active_ms()` is what a caller saves;
  no journal field holds it, so the instant must be kept somewhere of the caller's own or the
  boundary becomes undecidable after a restart.
- **`Recovery` is asked on the acceptor thread, and a slow implementation delays every
  connection behind it.** Reading a file per counterparty is fine; a network round trip is
  not, and nothing enforces the difference. ADR-0020 decision 4's pending deadline is the only
  backstop, and it refuses the socket rather than reporting why.
- **`Recovery<J>` is generic over the journal but `pump` fixes `J = journal::Store`**, because
  the serving loop's engine type is concrete. A deployment wanting `FileJournal` per
  counterparty through `serve_with_recovery` cannot have it yet.
- Two more public functions. The `_with_recovery` suffix pattern does not scale past a third
  axis, and a builder is the shape to reach for if one appears.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| Change `serve`'s signature | Ripples into `tools/w2w` and Linux-only `shard.rs`, neither runnable on the machine this was built on |
| Ask before the `Logon` | There is no identity yet — ADR-0020 |
| A `HashMap<Identity, Resumed>` parameter | Same objection ADR-0026 raised for `Registry`: it forecloses file-backed and lazy implementations |
| Have the engine read `journal.highest()` itself | ADR-0010: the engine never guesses. `usually` is not `always` |
| Return `Result` from `recover` | *"Nothing to recover"* is not a failure. A real I/O failure is the implementation's to handle, and it can still answer `None` |
