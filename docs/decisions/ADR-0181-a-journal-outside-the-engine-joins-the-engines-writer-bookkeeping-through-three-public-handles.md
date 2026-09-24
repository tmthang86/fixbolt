# ADR-0181 — A journal outside the engine joins the engine's writer bookkeeping through three public handles, and `FileJournal` uses the same three

- **Status**: **Accepted — 2026-09-24, by the manager under the owner's delegation of
  2026-09-18.** Proposed 2026-09-24, revised in place the same day, twice (see *Revision 1*,
  *Revision 2*), both revisions folded into the accepted text.
  Written by the architect (Opus) for row 3 of
  [docs/plans/2026-09-23-phase-4-scope.md](../plans/2026-09-23-phase-4-scope.md), planned in
  [docs/plans/2026-09-24-p4-sqlite-store.md](../plans/2026-09-24-p4-sqlite-store.md) step 1.
  Supersedes nothing: ADR-0150 decision 4, ADR-0153 decisions 3–4 and ADR-0155 decision 2 keep
  their substance; this ADR only moves their mechanisms behind public names so a second
  implementation uses the same ones.
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: [ADR-0150](ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md)
  decision 4, [ADR-0153](ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md)
  decisions 3–4, [ADR-0155](ADR-0155-a-recovery-learns-its-writer-let-go-from-the-writer-not-from-the-filesystem.md)
  decisions 2–3, [ADR-0180](ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md)
  decision 9, [ADR-0160](ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md)
  decision 7 (semver).

## Revision 1 — 2026-09-24, after step 1 was built (`98bf3d7`)

**What building it found** (senior developer, step 1): decision 1 as first written required the
engine to call `retire(stop_pushed)` **before** its `STOP` record is visible to the writer, so it
had to know whether `STOP` would fit before pushing it. Pushed the other way round, the writer
can pop `STOP`, call `finish()` on a ticket still *running* — which, as first written, did
nothing — and then `retire` raises the count that nothing will ever lower:
`wait_for_retired_writers` runs to its timeout on every shutdown. `FileJournal` kept the safe
order through a `pub(crate)` `Producer::fits(len)`; a journal outside the crate cannot call it
and does not know the ring's 4-byte record header. The step-1 build also made `TicketState`
public (the return type of `state()`) and gave `WriterTicket` and `Idle` a `Default`
(clippy `new_without_default`).

**Changed:** decision 1's `finish` bullet — `finish` on a *running* ticket now moves it to
*finished* without touching the count, and a later `retire` then returns `false` and counts
nothing. With that, **no order of push and retire can strand the count**, and every journal
follows one rule: push `STOP` once, then `retire(pushed)` with the push's result (decision 1,
*The rule for a journal*). `FileJournal` adopts the same rule; `Producer::fits` stays
`pub(crate)` for `push`'s own use and is no longer part of the retire protocol. Decision 5
names the items actually made public. **Not taken:** making `Producer::fits` public (below).
The state machine argument is in decision 1; `doc.rust-lang.org`'s atomic pages answered 404
when read on 2026-09-24, so it rests on the `compare_exchange` / `fetch_update` semantics the
step-1 code already relies on, not on a fresh citation.

## Revision 2 — 2026-09-24, after the Revision 1 fix round (`8d6b21c`)

**What building it found** (senior developer): with *push `STOP`, then `retire`*, a writer that
pops `STOP` and calls `finish()` before the engine thread's compare-and-swap leaves the ticket
*finished*; `retire` then returns `false` and, as Revision 1 wrote it, **counts nothing at all** —
so `writers_retired()` (*"every writer `Journal::retire` has let go, finished or not"*) does not
rise. Three things read that counter and lose to the race: the liveness check of the `retire`
case in `crates/engine/benches/alloc.rs` (`retired_after == retired_before + 1`, shown red with a
5 ms delay injected between push and retire; unaided the window is tens of nanoseconds on the
engine thread against at least two syscalls on the writer — a rare flake on a loaded two-vCPU
runner), `writers_retired`'s own rustdoc, and the plan's step-4 case `sqlite-retire`.

**Changed** — decision 1, taking the developer's proposal: the two counters answer two different
questions and Revision 1 had tied them together. `RETIRED_WRITERS` (what
`wait_for_retired_writers` waits for) counts **writers still to finish**; `WRITERS_RETIRED`
(`writers_retired()`) counts **retire acts**. A ticket gains a fifth, internal state,
*finished, never retired*. `finish` from *running* moves there (not to *finished*); the first
`retire` that meets it moves it to *finished*, raises `WRITERS_RETIRED` by one, leaves
`RETIRED_WRITERS` alone and returns `false`. `TicketState` (public) is unchanged: `state()`
reports the internal state as `Finished`. The rule for a journal is unchanged.

## Context

`[read 2026-09-24, main 094bfc3]` Three mechanisms a journal with its own writer thread must
share with the engine are private to `crates/engine/src/journal.rs` and `ring.rs`:

1. **The retired-writer count.** `static RETIRED_WRITERS` is raised by `FileJournal::retire`
   and lowered by `FileJournal`'s writer; `wait_for_retired_writers(timeout)` — called by every
   `serve*`, `connect_and_serve*` and the shard's serve after serving — waits for it to reach
   zero. A writer outside the crate cannot raise or lower it, so **`serve*` would return while the
   SQLite writer still holds uncommitted records**, and a normal exit would lose them: the loss
   ADR-0153 *Options not taken* rejects (*"Detach with no barrier"*).
2. **The release flag.** `Released(Arc<AtomicBool>)` has a private field; only `FileJournal`
   can make one. A recovery for another journal would need a type of its own for the same
   one-atomic-load answer ADR-0155 decision 3 prescribes.
3. **The idle rule.** `ring::Idle` (1 024 spins, then 1 ms sleeps; ADR-0150 decision 4) is
   `pub(crate)`. Another writer would re-implement the constants, and *"one rule, one place"*
   (`CLAUDE.md` preamble) would become two.

## Decision

1. **`fixbolt_engine::journal::WriterTicket`** — a `Clone` handle over one `Arc<AtomicU8>`
   allocated where the journal is opened (never on the engine thread), with five states:
   *running*, *retired*, *retired, stop when dry*, *finished, never retired* (internal: `state()`
   reports it as `TicketState::Finished`), *finished* (*Revision 2*).
   - `WriterTicket::new() -> Self`.
   - `retire(&self, stop_pushed: bool) -> bool` — **engine thread**: atomics only, no syscall, no
     allocation, no spin. The first call moves *running* → *retired* (or *retired, stop when
     dry* when `stop_pushed` is false), raising the process-wide retired count **before** the
     state is published (ADR-0153 decision 3's order), and returns `true`; any later call returns
     `false` and changes nothing.
   - `state(&self)` — **writer thread**: what it has been told.
   - `finish(&self)` — **writer thread, last act**, after its data is durable, its file closed
     and its `Releaser` released: moves the ticket to *finished*, once, by compare-and-swap. From
     *retired* or *retired, stop when dry* it lowers the count; **from *running* it lowers
     nothing** (the writer stopped before anyone retired it), and every later `retire` then
     fails its compare-and-swap, returns `false` and counts nothing (*Revision 1*).
     **Revision 2:** from *running*, `finish` moves to *finished, never retired*, lowering
     nothing.

   **The two counters, exactly (Revision 2).** Every transition below happens at most once per
   ticket, by compare-and-swap (atomics only, no syscall, no allocation):

   | Call | From | To | `RETIRED_WRITERS` (waited for) | `WRITERS_RETIRED` (`writers_retired()`) | returns |
   |---|---|---|---|---|---|
   | `retire(pushed)` | *running* | *retired* / *retired, stop when dry* | +1, **before** the state is published | +1 | `true` |
   | `retire(pushed)` | *finished, never retired* | *finished* | unchanged | +1 | `false` |
   | `retire(pushed)` | any other | unchanged | unchanged (a speculative +1 is taken back) | unchanged | `false` |
   | `finish()` | *retired* / *retired, stop when dry* | *finished* | −1 | unchanged | — |
   | `finish()` | *running* | *finished, never retired* | unchanged | unchanged | — |
   | `finish()` | any other | unchanged | unchanged | unchanged | — |

   Hence, **whatever the interleaving of the engine's push, its `retire` and the writer's
   `finish`**: `RETIRED_WRITERS` is raised only by a retire that found the writer still running
   and lowered only by that writer's finish — never lowered without having been raised, never
   twice, never left raised after the writer finished; and **the first `retire` of a ticket
   raises `writers_retired()` by exactly one**, whether the writer had already finished or not.
   `retire`'s return value says only *"this call left a writer for `wait_for_retired_writers`
   to wait for"*; nothing may use it as evidence that a retire happened.

   **What the code and its checks must satisfy (Revision 2):**

   - `writers_retired()`'s rustdoc stays true as written: *every writer `Journal::retire` has let
     go, finished or not; only rises*.
   - `crates/engine/benches/alloc.rs` case `retire` keeps its liveness assertion unchanged —
     `writers_retired()` after `== before + 1` around exactly one `Journal::retire` — and it now
     holds deterministically. The fix is proven by re-running that bench **with the 5 ms delay
     between push and retire injected** (green, where it was red), then without it (green).
   - `crates/engine/tests/writer_hooks.rs` holds the new row: push `STOP`, let the writer
     `finish` first, then `retire(true)` → returns `false`, `writers_retired()` rose by exactly
     one, a second `retire` raises nothing, `wait_for_retired_writers(Duration::ZERO)` is `true`.
     Reversal: drop the *finished, never retired* state (finish from *running* goes straight to
     *finished*) → red on that test's `writers_retired()` assertion.
   - The step-4 case `sqlite-retire` of the store plan proves its path live by
     `writers_retired()` rising by one, never by `retire`'s return value.

   **The rule for a journal** (`FileJournal` and any journal outside the crate — the step-3
   SQLite store follows it word for word):

   - *Engine thread, in `Journal::retire`*: push the one-byte stop record **once**
     (`Producer::push`, never a loop), then call `ticket.retire(pushed)` with the push's
     result, then detach the writer. Never push anything after.
   - *Writer thread*: a popped stop record means stop. When a pop finds the ring empty, read
     `ticket.state()`; if it is *retired, stop when dry*, **pop once more** and stop only if
     that pop is empty too — *read the state, then find the ring empty*, never the other way
     round, because a record pushed between an empty pop and the state read would otherwise be
     lost. A popped record that reports a drop (`Some(0)`) is skipped (ADR-0150 decision 2).
   - *Writer thread, stopping*: flush/commit, close the file, `releaser.release()`, then
     `ticket.finish()` — in that order (ADR-0155 decision 2).
   `wait_for_retired_writers` and `writers_retired` keep their signatures and meaning.
2. **`fixbolt_engine::journal::Releaser`** and a constructor pair
   `Released::pair() -> (Releaser, Released)` over one `Arc<AtomicBool>`. `Releaser::release(self)`
   stores `true` (Release) and consumes the releaser, so only its owner — the writer — can set it.
   `Released` keeps `is_released()` unchanged.
3. **`fixbolt_engine::ring::Idle`** becomes `pub` with `new`, `reset`, `wait`, and the two
   constants `IDLE_SPINS` and `IDLE_SLEEP` become `pub`. The rule and its gate
   (`crates/engine/tests/writer_idle.rs`) stay where they are.
4. **`FileJournal` moves onto all three and follows *the rule for a journal***, replacing its
   private `told` flag and its direct use of the statics, so both implementations run the same
   code; its existing tests
   (`one_appender`, `after_serving`, `retire`, `writer_idle`, `journal`, `on_disk`,
   `engine_recovery`, `secrets_stay_off_disk`) and the `retire` case of `benches/alloc.rs` are the proof that the
   move changed nothing, **unmodified**.
5. **Additive only.** No existing public item changes signature. New public items, all named in
   `CHANGELOG.md`: `journal::WriterTicket` (with `Default`), `journal::TicketState`,
   `journal::Releaser`, `Released::pair`, `ring::Idle` (with `Default`), `ring::IDLE_SPINS`,
   `ring::IDLE_SLEEP`. `cargo semver-checks` must read no break. `Producer::fits` stays
   `pub(crate)`.

## Options not taken

- **Two free functions, `count_retired_writer()` / `uncount_retired_writer()`.** Minimal, but a
  caller can lower the count without raising it; the count wraps and `wait_for_retired_writers`
  runs to its timeout on every shutdown. The ticket makes that unrepresentable. Rejected.
- **A token type whose `Drop` lowers the count.** It must be created on the engine thread in
  `retire` and then reach the writer, which needs a slot or a channel — an allocation or a
  futex on the engine thread. The ticket is allocated at open and shared from the start.
  Rejected.
- **The store keeps its own count and its own `wait_for_…` function.** Every `serve*` caller
  would have to call a second function after serving, which the compiler cannot require;
  `GUIDE.md` would carry a rule that is one call away from being forgotten. Rejected.
- **Count `WRITERS_RETIRED` in `Journal::retire` itself, outside the ticket (Revision 2).**
  Works for `FileJournal`, but every journal outside the crate would have to remember to do it,
  and the counter is private; the ticket is the one place every journal already calls. Rejected.
- **Make `retire` return `true` on a finished-never-retired ticket (Revision 2).** Callers could
  then read the return value as *"retired"*, but `true` would stop meaning *"something to wait
  for"*, and the Revision 1 test asserting `false` would change meaning. Rejected; the counter,
  not the return value, is the evidence.
- **Make `Producer::fits` public (Revision 1).** It would keep the first draft's
  *decide, then push* order working outside the crate, but it adds a public item whose only
  purpose is to make an order safe that `finish`-from-*running* makes safe for every order, and
  a journal that forgot to call it would strand the count again. Rejected.
- **Copy the idle constants into the store.** Two rules that must agree and nothing that checks
  they do. Rejected.

## Consequences

**Good**

- `serve*` waits for any journal's writer that retires through a ticket, with no new call for
  the user; a second journal cannot get ADR-0153's barrier wrong.
- One idle rule, one retired count, one release flag, whatever the journal.

**Bad — and accepted**

- **Four more public items on a released crate** (tagged, ADR-0161), which `cargo semver-checks` now holds
  stable. A later redesign of the writer bookkeeping is a breaking change.
- **`FileJournal`'s internals change** in a PR whose subject is another crate. The eight engine
  test binaries and the `retire` alloc case above are the guard; a reviewer must see them run
  unmodified.
- **A fifth ticket state no one outside sees (Revision 2).** `TicketState` has four variants
  and the atomic has five values; a reader of the code must know that *finished, never retired*
  is reported as `Finished`. The price of keeping the public type unchanged.
- **A ticket finished from *running* hides a mistake.** A writer that stops on its own before
  retirement (a `close()`, or a bug) turns every later `retire` into a no-op that returns
  `false`; the count stays correct, but nothing reports that the retire came too late. The
  return value is there to be asserted by tests (`writer_hooks.rs`), not by the engine.
- **A third-party journal can still skip the ticket** and detach a writer the engine never
  waits for. The API makes the right thing available; it cannot make it compulsory. `GUIDE.md`
  says so.
