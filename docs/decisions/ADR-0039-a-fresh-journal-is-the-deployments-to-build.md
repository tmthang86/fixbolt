# ADR-0039 — A fresh journal is the deployment's to build, and `seq == 0` is a timestamp

- **Status:** Accepted
- **Date:** 2026-09-02
- **Related:** [ADR-0010](ADR-0010-a-reconnect-is-not-a-restart.md),
  [ADR-0017](ADR-0017-the-inbound-count-is-persisted-after-delivery.md),
  [ADR-0033](ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md),
  [ADR-0034](ADR-0034-recovery-is-asked-once-the-counterparty-is-known.md),
  [ADR-0037](ADR-0037-reading-a-journal-is-not-recovering-from-one.md),
  [ADR-0038](ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md)
- **Plan:** [2026-09-02-recovery-reaches-the-disk.md](../plans/2026-09-02-recovery-reaches-the-disk.md)
- **Closes:** `STATUS.md` open item 32 **(b)** and **(c)**

## Context

ADR-0034 gave the serving loop a recovery seam and named two things it did not do. They turned
out to be one thing:

- **(b)** `pump` could only answer with `journal::Store`, so no deployment could use a
  `FileJournal` through `serve_with_recovery`.
- **(c)** nothing persisted `Session::last_active_ms()`, so ADR-0033's boundary reset survived a
  restart only if the caller kept that instant somewhere of its own.

Persisting the instant into a `FileJournal` means nothing while the serving loop cannot use one.
A `FileJournal` reachable through the serving loop still cannot answer the boundary question if
it does not remember when the session was last alive. **One piece of work.**

## Decision

### 1. `Recovery::fresh`, because the engine should not be inventing journals

`[verified 2026-09-02]` the whole of (b) was **one bound**. The engine called `J::default()` when
`recover` answered `None`, so `J: Default` sat on the serving loop — and **a `FileJournal` has no
honest `Default`**: it needs a path, and a `Default` that quietly gave an in-memory journal would
be an in-memory journal wearing a durable one's name.

So `Recovery` gains `fresh(&Config) -> J`. The deployment that knows which file belongs to which
counterparty is the one that builds it, which is the same reasoning ADR-0034 used to make
`Recovery` a trait rather than a map.

### 2. `fresh` is **required**, not defaulted

`fn fresh(&mut self, cfg: &Config) -> J where J: Default { J::default() }` was written first, and
it does not work. **A `where` clause on a default body lands on callers**, so the serving loop
still needed `J: Default` in order to call it, and the bound had merely moved house.

Requiring the method puts the constraint on the implementations that want it: `NoRecovery` and
`FromFn` implement it for any `J: Default`, so a `Store` deployment writes nothing extra.

`[measured 2026-09-02]` the reversal is not a failing test but a **compile error** — putting
`J::default()` back gives `the trait bound J: Default is not satisfied`. That is a stronger
proof than a test can give: it says the bound is *absent*, not merely unexercised.

### 3. `seq == 0` is an activity mark

Eight little-endian bytes of milliseconds, in a record whose **sequence number** is zero.

`34=0` is not a sequence number FIX has, so it cannot be confused with a message — precisely the
argument ADR-0017's `len == 0` inbound mark already made from the other side of the record
header. **The format did not change.** The reader is one branch longer, every file written
before this parses exactly as it did, and `Reader::records` gained a third variant rather than a
new file layout.

### 4. Written at two moments, and never per message

The engine records it when a session **logs on** and when an **ordered shutdown** says goodbye.
Nothing else.

Per message would be a disk write on the hot path, which D8 forbids outright. The shutdown case
is the one that earns its keep: it answers *"when was this session last alive?"* for a planned
restart — and ADR-0038 has only just made that moment exist.

### 5. `None` means *"this journal does not know"*

Not *"the session was never active"*. An in-memory journal answers `None`, and so does a durable
file written before this existed. A caller that treated the two alike would silently restart a
session's numbering, which is the failure ADR-0033's boundary reset exists to make deliberate.

### 6. The engine still does not decide

`Journal::last_active()` is a readable fact. Turning it into `Resumed::last_active_ms` remains
the `Recovery` implementation's choice, because ADR-0010 is explicit that choosing between a
restart and a continuation belongs to the caller.

## Consequences

**Good**

- A deployment can run `serve_with_recovery` with one `FileJournal` per counterparty, and an
  unattended restart across a trading-day boundary now has the instant it needs.
- `[measured 2026-09-02]` the end-to-end test goes through a real listener, a real socket and
  `serve_with_recovery`, and then reads the file **with `journal::Reader`, not with the engine**
   — the same separation ADR-0037 drew.
- Adding `Record::ActivityMark` broke **three exhaustive matches**, two in tests and one in
  `tools/jrnl`. That is ADR-0035's no-`_`-arm rule paying off in a crate written after it.
- `tools/jrnl --count` now reports `last-alive`, so the instant is answerable without Rust.

**Bad, and named**

- **`serve_sharded_hft` is still item 32 (a)** — Linux-only, untouched, and now missing a third
  thing after recovery and shutdown.
- **The mark is written at two moments and neither is periodic.** A process killed between logon
  and shutdown reports the logon instant, which may be a whole day stale. A periodic mark needs a
  frequency, and a frequency needs a measurement nobody has taken.
- **Nothing stops two processes opening the same journal file.** The engine appends; two
  appenders interleave records and the result is not defined. There is no lock.
- **`u64` milliseconds with no epoch stated in the file.** It is the engine's clock scale
  (D13, milliseconds since 0000-01-01), and a file carries no marker saying so — a reader with a
  different assumption reads a plausible wrong date.
- **`FromFn` still requires `J: Default`.** The convenience wrapper cannot be used for the
  file-backed case that this ADR exists to enable; such a deployment writes a named type.
- **`Recovery::fresh` may be called on the acceptor thread and may open a file**, which
  ADR-0034 already flagged as unbounded and still nothing bounds.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| `impl Default for FileJournal` | It would have to invent a path or be in-memory. Either is a durable journal that is not one |
| Keep `J: Default` and require deployments to use `Store` | That *is* the gap. `Store` does not survive a restart |
| A default body on `fresh` with `where J: Default` | Tried first. The bound lands on callers and had only moved |
| A new record **type byte** for timestamps | Every existing record would have to grow one. `seq == 0` costs nothing and the reader one branch |
| A sidecar file for the instant | Two files to keep consistent, and a torn tail in one of them is now two questions |
| Write the mark on every message | A disk write on the hot path. D8 |
| Have the engine compute `last_active_ms` itself | ADR-0010: the engine asks, it does not decide |
