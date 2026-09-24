# ADR-0181 — A journal outside the engine joins the engine's writer bookkeeping through three public handles, and `FileJournal` uses the same three

- **Status**: **Proposed — 2026-09-24.** Written by the architect (Opus) for row 3 of
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
   allocated where the journal is opened (never on the engine thread), with four states:
   *running*, *retired*, *retired, stop when dry*, *finished*.
   - `WriterTicket::new() -> Self`.
   - `retire(&self, stop_pushed: bool) -> bool` — **engine thread**: atomics only, no syscall, no
     allocation, no spin. The first call moves *running* → *retired* (or *retired, stop when
     dry* when `stop_pushed` is false), raising the process-wide retired count **before** the
     state is published (ADR-0153 decision 3's order), and returns `true`; any later call returns
     `false` and changes nothing.
   - `state(&self)` — **writer thread**: what it has been told.
   - `finish(&self)` — **writer thread, last act**: moves a retired ticket to *finished* and
     lowers the count exactly once (compare-and-swap); on a ticket never retired it does nothing.
   A count can therefore never be lowered without having been raised, nor twice.
   `wait_for_retired_writers` and `writers_retired` keep their signatures and meaning.
2. **`fixbolt_engine::journal::Releaser`** and a constructor pair
   `Released::pair() -> (Releaser, Released)` over one `Arc<AtomicBool>`. `Releaser::release(self)`
   stores `true` (Release) and consumes the releaser, so only its owner — the writer — can set it.
   `Released` keeps `is_released()` unchanged.
3. **`fixbolt_engine::ring::Idle`** becomes `pub` with `new`, `reset`, `wait`, and the two
   constants `IDLE_SPINS` and `IDLE_SLEEP` become `pub`. The rule and its gate
   (`crates/engine/tests/writer_idle.rs`) stay where they are.
4. **`FileJournal` moves onto all three**, replacing its private `told` flag and its direct use of
   the statics, so both implementations run the same code; its existing tests
   (`one_appender`, `after_serving`, `retire`, `writer_idle`, `journal`, `on_disk`,
   `engine_recovery`, `secrets_stay_off_disk`) and the `retire` case of `benches/alloc.rs` are the proof that the
   move changed nothing, **unmodified**.
5. **Additive only.** No existing public item changes signature. `CHANGELOG.md` names the four
   new items; `cargo semver-checks` must read no break.

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
- **Copy the idle constants into the store.** Two rules that must agree and nothing that checks
  they do. Rejected.

## Consequences

**Good**

- `serve*` waits for any journal's writer that retires through a ticket, with no new call for
  the user; a second journal cannot get ADR-0153's barrier wrong.
- One idle rule, one retired count, one release flag, whatever the journal.

**Bad — and accepted**

- **Four more public items on a published crate**, which `cargo semver-checks` now holds
  stable. A later redesign of the writer bookkeeping is a breaking change.
- **`FileJournal`'s internals change** in a PR whose subject is another crate. The eight engine
  test binaries and the `retire` alloc case above are the guard; a reviewer must see them run
  unmodified.
- **A third-party journal can still skip the ticket** and detach a writer the engine never
  waits for. The API makes the right thing available; it cannot make it compulsory. `GUIDE.md`
  says so.
