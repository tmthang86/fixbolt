# `store-sqlite` — internals

Package `fixbolt-store-sqlite`, beside the engine in [DESIGN.md §3](../DESIGN.md#3-crates): a
journal whose durable copy is a SQLite database, one database per session. It is the engine's
`FileJournal` under `Durability::Async` with a database for a file — the engine thread's half is
the same `MemJournal` plus one ring push, and a writer thread of its own commits in batches
([ADR-0180](../decisions/ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md)).
It retires, releases and idles through the engine's own public handles — `WriterTicket`,
`Releaser`/`Released::pair`, `ring::Idle`
([ADR-0181](../decisions/ADR-0181-a-journal-outside-the-engine-joins-the-engines-writer-bookkeeping-through-three-public-handles.md)).
Everything sits behind the default feature `sqlite`; without it the crate is empty and compiles no
C ([ADR-0182](../decisions/ADR-0182-the-sqlite-store-is-born-release-shaped-behind-a-default-feature-and-joins-the-tagged-release-family-only-when-its-kill-line-passes.md)).
`publish = false` until phase 4 row 4's kill line passes (ADR-0182 decision 3).

## Files, and what each keeps

| File | Keeps |
|---|---|
| `src/lib.rs` | Crate root: `#![forbid(unsafe_code)]`, the three `mod`s each behind `#[cfg(feature = "sqlite")]` (non-negotiable 6), the re-exports |
| `src/journal.rs` | The engine thread's half. `SqliteJournal<N, LEN>` (+ `SqliteStore` at the engine's default sizes), `SqliteOptions`, `Synchronous`, `Progress`; `open` (runs `schema::open`, fills the `MemJournal`, starts the writer and waits until it holds its buffer and statement), `released`, `progress`, `close`, `Drop`; the `Journal` impl — `put`/`mark_in`/`mark_out`/`mark_active` push one `seq ‖ len ‖ payload` record each, `retire` follows *the rule for a journal* (push `STOP` once, `ticket.retire(pushed)`, detach). **Holds no `rusqlite` type** |
| `src/writer.rs` | The writer thread, `fixbolt-sqlite`, and the only code that touches SQLite after `open`. The ring's record layout (module comment), `Counters` (committed, highest outbound committed, failed), `DryRule` (the stop-when-dry rule, a step at a time), `Batch` (one transaction per drain or per `batch_max`; a message carrying a secret raises `highest_out` and is not inserted; a failed batch is rolled back and counted), `run` (loop, then commit, `wal_checkpoint(TRUNCATE)`, close the connection, `release`, `finish`) |
| `src/schema.rs` | What `open` does before the writer gets the connection: `locking_mode = EXCLUSIVE`, `busy_timeout = 0`, `journal_mode = WAL`, `synchronous`; one `BEGIN IMMEDIATE … COMMIT` that creates schema version 1 or checks version and identity and reads back the session row and the last `N` messages; `max_page_count`; `to_io` (busy or locked → `WouldBlock`) |

## Read in this order

1. `src/lib.rs` — the module doc says what the store is and what it shares with the engine
2. `src/journal.rs` — the engine thread's half: what each `Journal` call costs, and `retire`
3. `src/writer.rs` — the module comment's record table, then `DryRule`, `Batch`, `run`
4. `src/schema.rs` — the lock, the schema, the identity check, the read-back

## Tests that guard it

- `src/writer.rs` `#[cfg(test)]` — `a_record_pushed_between_an_empty_pop_and_stop_when_dry_is_written`
  drives `DryRule` one step at a time (plan row 3, R5b)
- `tests/store.rs` — reopen answers what it was told; a reused number keeps the newest bytes;
  reopen loads only the last `N`; a second open is `WouldBlock` in under 100 ms; another session's
  database is `InvalidData`; a retired writer commits everything, releases, then is counted done;
  a secret leaves only its number in `.db` and `-wal`; a record larger than the ring and a failed
  commit are counted in `unwritten`; the idle writer gives its core back
- `tests/crash.rs` — a `SIGKILL`ed child resumes from what it committed: every number up to the
  committed `highest_out`, byte for byte, `integrity_check` = `ok`
- `tests/serve.rs` — through `serve_with_recovery` over a socket: a resumed session replays what it
  sent from the database; secrets of a real logon stay out of `.db` and `-wal`; a quick reconnect
  is parked on `released()` and then admitted
- `crates/engine/tests/writer_hooks.rs` — the three engine handles this crate builds on
- `scripts/check-no-optional-deps.sh` — `fixbolt-store-sqlite:rusqlite` and
  `fixbolt-store-sqlite:libsqlite3-sys` absent with `--no-default-features`
