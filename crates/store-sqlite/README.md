# fixbolt-store-sqlite

A SQLite message store for [fixbolt](../../README.md):
the engine's `Async` journal with a database for a file, **one database per session**.
`SqliteJournal` implements the session's `Journal` at the same cost to the engine thread as
`FileJournal` under `Durability::Async` — memory answers every resend, one record goes onto a
ring — and a writer thread of its own commits to SQLite in batches (WAL,
`synchronous = NORMAL` by default, `FULL` as a setting). **No mode makes `put` wait for the
disk**; a deployment that must have a message on disk before it is sent keeps `FileJournal`
with `Durability::Fsync`.

Not yet part of the tagged release family (`publish = false`): it joins once its measured kill
line passes. Depend on it by path or by git from the repository.

## Features

| Feature | Default | What it adds |
|---|---|---|
| `sqlite` | on | Everything: `SqliteJournal`, `SqliteStore`, `SqliteOptions`; takes `rusqlite` with SQLite compiled in (`bundled`). Off, the crate is empty and compiles no C |

## What a deployment must honour

- Never open the database file with `std::fs` (or anything but SQLite) while the store runs:
  closing any descriptor on it drops SQLite's locks for the whole process. Back up with SQLite's
  backup API, or after `close`.
- One SQLite library per process.
- A `Recovery` answers `ready` from `SqliteJournal::released()`, so a quick reconnect is parked
  until the last session's writer has let the database go.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
