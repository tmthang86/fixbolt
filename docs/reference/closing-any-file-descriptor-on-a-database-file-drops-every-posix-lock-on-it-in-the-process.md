# Closing any file descriptor on a database file drops every POSIX lock on it, in the whole process

> `[researched 2026-09-24]` — found while designing
> [plans/2026-09-24-p4-sqlite-store.md](../plans/2026-09-24-p4-sqlite-store.md),
> [ADR-0180](../decisions/ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md)
> decisions 5 and *Consequences*.

## What the research found

`fixbolt-store-sqlite`'s one-appender rule rests on SQLite's own `locking_mode=EXCLUSIVE`: the
first write takes a POSIX advisory lock on the database file and holds it until the connection
closes, so a second `open` — in this process or another — fails at once (`SqliteJournal::open`
maps `SQLITE_BUSY`/`SQLITE_LOCKED` to `WouldBlock`). `sqlite.org/howtocorrupt.html` §2.2, read
while writing ADR-0180, says what that rests on:

> *"the `close()` system call will cancel all POSIX advisory locks on the same file for all
> threads and all file descriptors in the process, even file descriptors and threads that have
> nothing to do with the file descriptor that was closed."*

This is not a SQLite bug and not something `fixbolt-store-sqlite` can guard against from
inside: it is what `close(2)` does to advisory locks on Linux, for **any** file descriptor on
that inode, opened by **any** call, held by **any** thread. A process that has the database
open through `SqliteJournal` and, in the same process, separately does

```rust
let bytes = std::fs::read(&db_path)?; // opens the file, reads it, drops it — closing it
```

drops the `EXCLUSIVE` lock `SqliteJournal`'s connection is relying on the instant that `File` is
dropped — even though nothing about that line touched SQLite, and even though the `File` was
opened read-only. A second writer — another process, or a second `SqliteJournal::open` retried
after the first `WouldBlock` — can then open the same file and both write it, which is exactly
the corruption `EXCLUSIVE` mode exists to prevent (howtocorrupt §2.2's own next sentence: this is
how two connections can corrupt one file despite believing they hold exclusive access).

The same mechanism reaches a database file opened by hand for a peek (`sqlite3 the.db`, `cat`,
an editor's "open read-only") while the engine holds it, and reaches a **second** SQLite build
linked into the same process (§2.3, a related but distinct hazard: two copies of SQLite cannot
see each other's locks at all, so one's `close()` clears a lock the other still believes it
holds) — `links = "sqlite3"` in `Cargo.toml` stops Cargo linking a second `libsqlite3-sys`, but a
C library that brings its own bundled SQLite is invisible to that check.

## The rule now

**Nothing in `crates/store-sqlite/src/` opens the database file by any means other than through
the one `rusqlite::Connection` `SqliteJournal::open` creates and hands to the writer thread.**
No `File::open`, no `fs::read`, no probe that peeks at the file to decide whether it is busy —
unlike `FileJournal::file_busy`'s open-and-close probe, which is safe only because it targets a
*different* file (the resend ring's own on-disk copy has no such probe; the journal file itself
is never opened outside SQLite). `GUIDE.md` §6d and ADR-0180 *Consequences* tell an operator: back
the database up through SQLite's own backup API, or only ever after `close()` has returned —
never with a filesystem tool while the store is running.

## The tests and checks that guard it

| Guard | What it proves | Kind |
|---|---|---|
| `tests/store.rs::a_message_carrying_a_secret_leaves_only_its_number` | reads the `.db` and `-wal` bytes with `std::fs::read`, but **only after** `j.close()` has returned in the block above it — the test itself follows the rule it is not testing | automated, but proves the test's own discipline, not the crate's |
| a reviewer reading `src/journal.rs`, `src/writer.rs`, `src/schema.rs` for `File::open`, `fs::read`, `fs::write`, `fs::OpenOptions` outside the one `rusqlite::Connection` | that nothing in the crate's own code opens the database file by another route | **hand check — no script.** The plan's *Bẫy đã lường trước* table names this as "reviewer grep `File::open`/`fs::read` in `src/`"; no `scripts/check-*.sh` runs that grep today |

**Not guarded**: a *user's* code doing exactly what this page warns against — `GUIDE.md` states
the rule; nothing in the type system or in CI can stop a caller from opening the file itself.
