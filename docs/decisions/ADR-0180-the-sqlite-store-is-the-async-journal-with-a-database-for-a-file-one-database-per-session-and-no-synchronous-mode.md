# ADR-0180 — The SQLite store is the `Async` journal with a database for a file: one database per session, batched commits on a writer thread, and no synchronous mode

- **Status**: **Proposed — 2026-09-24.** Written by the architect (Opus) for row 3 of
  [docs/plans/2026-09-23-phase-4-scope.md](../plans/2026-09-23-phase-4-scope.md), planned in
  [docs/plans/2026-09-24-p4-sqlite-store.md](../plans/2026-09-24-p4-sqlite-store.md). It decides
  the *shape* ADR-0098 item 4 left open; it supersedes nothing.
- **Date**: 2026-09-24
- **Deciders**: proposed by the architect; accepted by the manager under the owner's standing
  mandate, or by the owner.
- **Related**: [ADR-0098](ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
  item 4 (the store and its kill line), owner answers Q1 and Q4;
  [ADR-0008](ADR-0008-journal-is-a-trait.md), [ADR-0017](ADR-0017-the-inbound-count-is-persisted-after-delivery.md),
  [ADR-0046](ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md),
  [ADR-0053](ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md),
  [ADR-0110](ADR-0110-a-secret-is-masked-in-the-message-log-and-leaves-only-its-number-in-the-journal-file.md) decision 4,
  [ADR-0150](ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md),
  [ADR-0153](ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md),
  [ADR-0154](ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md),
  [ADR-0155](ADR-0155-a-recovery-learns-its-writer-let-go-from-the-writer-not-from-the-filesystem.md);
  drafted with it: [ADR-0181](ADR-0181-a-journal-outside-the-engine-joins-the-engines-writer-bookkeeping-through-three-public-handles.md)
  (the engine hooks this store needs) and
  [ADR-0182](ADR-0182-the-sqlite-store-is-born-publish-shaped-behind-a-default-feature-and-joins-the-lockstep-release-only-when-its-kill-line-passes.md)
  (its feature gate and release). `DESIGN.md` D7.

## Context

ADR-0098 item 4 admitted a database-backed store as **`FileJournal` `Async`'s shape with a
different sink**: the engine thread writes to the in-memory ring and pushes a record onto a
pre-allocated SPSC ring; a writer thread commits in batches; a full ring is a record not stored,
counted; no `Fsync`-equivalent mode; recovery reads the database through `Recovery`. The owner
chose SQLite (Q4) and required the kill line be written before code (Q1): engine-thread
allocations 0, wire p50 within the band of `FileJournal` `Async`, 50 000 msg/s for 60 s with no
dropped record.

Since ADR-0098 was written, four ADRs settled what a durable journal with a writer thread owes
the engine: the writer holds the largest record a slot allows and stops only on a record no
message can be (ADR-0150); a departing connection's journal is **retired** without waiting and
its writer awaited only after serving (ADR-0153); a journal file has **one appender**, a
reconnect that finds it busy is **parked**, and what the file missed is **counted** (ADR-0154);
a recovery learns the writer let go **from the writer**, by one atomic load (ADR-0155). And
ADR-0110 decision 4: a message carrying a secret leaves **only its number** in the journal file.
A store that did not honour all five would be a second, weaker journal. What remains open is the
database-specific shape — schema, one database or many, pragmas, batching, how the one-appender
rule maps onto SQLite's locking — and what the engine must expose for a journal outside its
crate (ADR-0181).

**Facts, read from the code on `main` `094bfc3`** (`crates/session/src/journal.rs`,
`crates/engine/src/journal.rs`, `crates/engine/src/ring.rs`, `crates/engine/src/redact.rs`):

1. `Journal` is 12 methods; `highest`, `oldest`, `highest_in`, `highest_out`, `put`, `get`,
   `mark_in` have no default. `mark_in` and `mark_out` are **high-water marks** (`MemJournal`
   keeps `max`), so coalescing them to the largest value per batch writes exactly what the file
   journal would read back.
2. `FileJournal` `Async` keeps a `MemJournal<N, LEN>` (the resend store, D7) and pushes
   `seq ‖ len ‖ bytes` onto a `ring::Producer`; a full ring increments `unwritten` and `put`
   still answers `true` (ADR-0154 decision 4).
3. `ring::{pair, Producer, Consumer}` and `redact::{carries_secret, mask}` are public;
   `ring::Idle` (ADR-0150 decision 4's idle rule), the retired-writer count behind
   `wait_for_retired_writers`, and `Released`'s constructor are **not** — the reason for
   ADR-0181.

## Research

| Source | What it says | Bearing |
|---|---|---|
| <https://sqlite.org/pragma.html#pragma_synchronous> (read 2026-09-24) | *"Transactions are durable across application crashes regardless of the synchronous setting or journal mode."* *"WAL mode is always consistent with synchronous=NORMAL, but WAL mode does lose durability. A transaction committed in WAL mode with synchronous=NORMAL might roll back following a power loss or system crash."* With NORMAL in WAL, *"no sync operations occur during most transactions"* — only around checkpoints. | Decision 4: NORMAL is `FileJournal` `Async`'s durability class (survives the process, not the power) |
| <https://sqlite.org/pragma.html#pragma_locking_mode> | EXCLUSIVE: *"The first time the database is written, an exclusive lock is obtained and held"*; *"WAL databases can be accessed in EXCLUSIVE mode without the use of shared memory."* | Decision 5: the one-appender rule is SQLite's own lock, held for the connection's life |
| <https://sqlite.org/howtocorrupt.html> §2.2 | *"the close() system call will cancel all POSIX advisory locks on the same file for all threads and all file descriptors in the process"* — a thread that merely `open`s, `read`s and `close`s the database file drops SQLite's locks, and two writers can then corrupt it | Decision 5 and a trap: nothing in this process may open the database file except through SQLite, so `journal::file_busy`'s open-and-close probe must **never** be pointed at it |
| same page, §2.3 | Two copies of SQLite linked into one process cannot see each other's open files and *"A close() operation on one connection might unknowingly clear the locks on a different database connection"* | *Consequences*: a user binary that links a second SQLite is a corruption hazard `GUIDE.md` must name |
| <https://phiresky.github.io/blog/2020/sqlite-performance-tuning/> | WAL + `synchronous=normal` is *"completely corruption safe"*; the WAL grows without bound when `wal_autocheckpoint` never gets to run because of lock contention | Decision 5: one connection, no reader → the auto-checkpoint always completes; the soak run prints the WAL size |
| <https://avi.im/blag/2021/fast-sqlite-inserts/> | rusqlite, prepared statement reused inside batched transactions: 100 M rows in 34.3 s (MacBook Pro 2019), **with durability off** (`journal_mode=OFF`, `synchronous=0`) | Order of magnitude for batched inserts; not comparable to a durable configuration |
| <https://github.com/rusqlite/rusqlite/blob/master/README.md> | `bundled` makes `libsqlite3-sys` compile SQLite from source with `cc` — SQLite 3.53.2 as of rusqlite 0.40.1 / libsqlite3-sys 0.38.1; MSRV is *"Latest stable Rust version at the time of release"* | ADR-0182: C is compiled whenever this crate is built with its feature; MSRV must be checked, not assumed |
| crates.io API, `cargo info rusqlite@0.40.2` (2026-09-24) | rusqlite **0.40.2**, 2026-08-08, MIT, `rust-version: unknown`; default features `cache` (hashlink) and `ffi-sqlite-wasm-rs` (wasm only); `libsqlite3-sys` declares `links = "sqlite3"` | Dependency is `rusqlite = { version = "0.40", default-features = false, features = ["bundled"] }`; `links` means one libsqlite3-sys per dependency graph |
| <https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html> | `Connection` is `Send`, not `Sync`; `prepare_cached` reuses a statement from a small LRU cache | Decision 3: the connection is opened on the opening thread and **moved** into the writer; the writer holds its prepared statements for its whole life instead of using the cache |
| QuickFIX/J `quickfixj-core/src/main/resources/config/sql/mysql/{sessions,messages}_table.sql` (read through `gh api` 2026-09-24) | `sessions` keyed by the eight-part session id with `creation_time`, `incoming_seqnum`, `outgoing_seqnum`; `messages` keyed by session id + `msgseqnum`, `message TEXT` | Decision 2's two tables, keyed by `seq` alone because a database holds one session |
| QuickFIX `src/C++/MySQLStore.cpp` (read through `gh api` 2026-09-24) | Every `set` is its own `INSERT` (falling back to `UPDATE` on a duplicate) and every sequence-number change its own `UPDATE sessions`, SQL built by string concatenation | The per-message round trip is the prior art's cost; decision 3 batches it off the engine thread and binds parameters |
| <https://github.com/quickfix-j/quickfixj/issues/357>, QFJ-119 (cited by ADR-0098) | `JdbcStore`: two I/O operations per message, no transaction around message + sequence number; users batch it themselves | Decision 3: one transaction per batch, messages and marks together |

`[measured 2026-09-24]` **An indicative probe, not a figure** (non-negotiable 10: the probe is a
scratch crate, not committed; label it someone else's claim until row 4 repeats it with the
committed harness). Desk `tmt-B450-I-AORUS-PRO-WIFI`, kernel `7.0.0-31-generic`, **desktop grub
line, not §9**, database on `/dev/nvme0n1p6` ext4; rusqlite 0.40.2 `bundled` (SQLite 3.53.2),
`journal_mode=WAL`, `synchronous=NORMAL`, `locking_mode=EXCLUSIVE`; one held prepared
`INSERT OR REPLACE INTO m(seq, body) VALUES (?1, ?2)`, 200-byte blobs, 512 rows per
`BEGIN`…`COMMIT`: 3 000 000 rows in **5.640 s and 5.485 s** (531 902 and 546 926 rows/s), a
648 MB database. A second connection to the same file, `busy_timeout` 0, `BEGIN IMMEDIATE` →
`Err("database is locked")` at once. The same crate builds on Rust **1.88.0** and 1.98.0.

**Searched, found nothing:** a FIX engine that stores its journal in SQLite (QuickFIX/J's and
QuickFIX's SQL stores target MySQL/Postgres/ODBC via JDBC/ODBC); a published throughput figure
for SQLite with `synchronous=NORMAL` in WAL and per-row blobs of FIX size.

## Decision

1. **The crate.** `crates/store-sqlite`, package `fixbolt-store-sqlite`. Its journal is
   `SqliteJournal<const N: usize, const LEN: usize>`, with `SqliteStore =
   SqliteJournal<SLOTS, SLOT_LEN>` as `FileJournal`'s default is. It implements
   `fixbolt_session::journal::Journal` with every method `FileJournal` implements, including
   `retire`, `mark_active`, `last_active`, `unwritten`. It is not a dependency of
   `fixbolt-engine` or of `fixbolt`, and depends on `fixbolt-engine` with
   `default-features = false`.
2. **One database file per session**, keyed by `seq` alone, like `FileJournal`'s one file per
   path. Schema version 1, set in `PRAGMA user_version`:
   - `messages(seq INTEGER PRIMARY KEY, body BLOB NOT NULL)` — `INSERT OR REPLACE`, because a
     wound-back count (`Admin::SetNextOut`) reuses a number and the newest copy must win, as the
     ring's slot does (D7);
   - `session(id INTEGER PRIMARY KEY CHECK (id = 1), begin_string BLOB, sender BLOB, target BLOB,
     highest_in INTEGER, highest_out INTEGER, last_active_ms INTEGER)` — one row.
   `open` writes the identity from the `Config` it is given on a new database and **refuses**
   (`io::ErrorKind::InvalidData`, naming the path and both identities) a database whose identity
   differs, and one whose `user_version` it does not know. One database per session because
   SQLite has one writer per database: sessions sharing a file would serialise their writer
   threads on its lock, and ADR-0153's retire and ADR-0154's one appender are per file. An
   operator who wants one queryable view `ATTACH`es the files.
3. **The engine thread's half is `FileJournal` `Async`'s, byte for byte in cost.** `put` stores
   in the `MemJournal`, then pushes `seq ‖ len ‖ bytes` onto the store's `ring::Producer`;
   `mark_in`, `mark_out`, `mark_active` push their marks; a full ring increments `unwritten` and
   `put` still answers `true` (ADR-0154 decision 4); `get`/`oldest`/`highest` answer from memory.
   **`SqliteJournal` holds no `rusqlite` type**: the `Connection` is opened on the opening thread
   and moved into the writer thread's closure, so no engine-thread call can reach SQLite (whose
   C heap no Rust counting allocator sees). The ring defaults to `ring::DEFAULT_CAPACITY`
   (4 MiB), larger than `FileJournal`'s 1 MiB, because a database commit and a WAL checkpoint
   stall the writer longer than an `append` does; it is a setting.
4. **Durability: WAL, `synchronous = NORMAL` by default, `FULL` as a setting — and never on the
   engine thread.** NORMAL is the class `FileJournal` `Async` is in: a committed batch survives
   the process being killed, and may roll back on a power loss or OS crash. `FULL` syncs the WAL
   at every commit, so a committed batch survives a power loss; it slows the writer only. **There
   is no mode in which `put` waits for a commit** (ADR-0098 item 4): a deployment that must have a
   message on disk before it is sent keeps `FileJournal` `Fsync`. What a process kill loses is
   what was still in the ring or in the open batch — bounded by the ring and by decision 6.
5. **One appender is SQLite's own lock.** `open` sets `journal_mode=WAL`, the `synchronous` of
   decision 4, `locking_mode=EXCLUSIVE` and `busy_timeout=0`, then runs one `BEGIN IMMEDIATE`
   … `COMMIT` (creating or checking the schema and reading the state). In EXCLUSIVE mode the
   write lock taken there is held until the connection closes, so a second `open` — in this
   process or another — fails at once; `open` maps `SQLITE_BUSY`/`SQLITE_LOCKED` to
   `io::ErrorKind::WouldBlock` naming the path, as `FileJournal::open` does (ADR-0154 decision 1).
   **Nothing in this crate opens the database file other than through SQLite**, and no
   `file_busy`-style probe exists for it (howtocorrupt §2.2). With one connection and no reader,
   the automatic checkpoint (1 000 pages) always completes; the writer runs `PRAGMA
   wal_checkpoint(TRUNCATE)` before it closes.
6. **Batching: one transaction per drain.** The writer pops into a buffer of `8 + LEN` bytes
   allocated once on its own thread before its loop (ADR-0150 decision 1), opens a transaction at
   the first record, executes one held prepared `INSERT OR REPLACE` per message, keeps the
   largest `mark_in` and `mark_out` and the last `mark_active` of the batch in locals, and
   commits — after one `UPDATE session` — when the ring runs dry or the batch reaches
   `batch_max` records (default 4 096, a setting). Batch size follows the load: one record when
   idle, thousands under a burst. When dry it waits by ADR-0150 decision 4's rule — the engine's
   own `ring::Idle`, made public by ADR-0181 — so the engine thread never wakes it. It stops on
   the one-byte `STOP` record, and a `pop` that reports a dropped record is skipped, never read as
   stop (ADR-0150 decision 2).
7. **Secrets: only the number.** On the writer thread, a message record for which
   `redact::carries_secret` is true is **not** inserted; its `seq` raises the batch's
   `highest_out` instead — ADR-0110 decision 4's outbound mark, in database form. After a restart
   `get` has nothing for that number and the session gap-fills it. The in-memory ring keeps the
   bytes, so a resend inside the process replays them, as `FileJournal` does.
8. **A failed batch is counted, not retried.** If a statement or the commit fails (disk full, I/O
   error), the writer rolls the transaction back, adds the batch's record count to a shared
   counter that `unwritten()` adds to its own (one relaxed atomic load, no syscall), and carries
   on with the next batch. The engine reports increases as `EventKind::JournalUnwritten`, exactly
   as for a full ring.
9. **Retire, release, wait — the engine's rules, through the engine's handles** (ADR-0181).
   `retire` makes no syscall and never spins: it takes the writer's ticket (counted before the
   writer can learn it), pushes `STOP` once or marks *stop when dry*, and detaches the writer.
   The writer, after its last commit and checkpoint, **drops the `Connection`** (releasing the
   lock), then marks its `Releaser`, then finishes its ticket — ADR-0155 decision 2's order.
   `SqliteJournal::released()` hands out the engine's `Released`; a recovery answers `ready` with
   it (ADR-0155 decision 3); `serve*`'s existing `wait_for_retired_writers` waits for this writer
   too. `close()` and a plain `Drop` of an unretired journal join, as `FileJournal`'s do.
10. **Recovery reads the database once, at `open`.** `open` loads the session row and the last
    `N` messages (`ORDER BY seq DESC LIMIT N`) into the `MemJournal`, so `highest_out`,
    `highest_in`, `last_active` and a post-restart `ResendRequest` answer exactly as they did
    before the restart, within the ring. `Resumed::from_journal` then computes the counts, as it
    does for a file. `open` runs wherever the caller's `Recovery::recover` runs — on the engine
    thread in the single-engine `serve*` loops, a known cost ADR-0154 *Consequences* records.
11. **A progress handle.** `SqliteJournal::progress()` hands out a clone of two atomics allocated
    at open — records committed, and the highest outbound number committed — for the
    crash-and-recover test, the soak harness and an operator's *how far behind is the database*.

## Options not taken

- **One shared database for every session, with session-id columns (QuickFIX/J's schema).**
  One file to query, but SQLite has one writer per database: N writer threads would contend on
  its lock (`SQLITE_BUSY`), or one shared writer would outlive every connection and break
  ADR-0153's per-journal retire and ADR-0154's per-file appender. Rejected for this row;
  `ATTACH` gives the operator the view.
- **A synchronous mode (commit before `put` returns).** A database commit on the engine thread
  is a blocking call non-negotiable 4 forbids in `hft` and ADR-0098 item 4 excluded. Rejected.
- **`synchronous = FULL` as the default.** It buys power-loss durability of committed batches —
  but the store is always at least one batch behind the wire, so it never buys *"on disk before
  sent"*, which is what a deployment asking for power-loss safety means. NORMAL matches
  `FileJournal` `Async`, the default it is compared against. `FULL` stays a setting.
- **`prepare_cached`.** Needs rusqlite's `cache` feature (`hashlink`) — a dependency for a cache
  of two statements the writer can simply hold. `[measured 2026-09-24]` without `cache`,
  `prepare_cached` does not exist (`E0599`). Rejected.
- **A sidecar lock file (`File::try_lock`, as `FileJournal`).** A second lock beside SQLite's,
  on a second file that can be deleted or left behind; SQLite's EXCLUSIVE mode already refuses a
  second writer in and across processes (probe above). Rejected.
- **Retry a failed batch.** A disk that is full stays full; retrying holds records the ring needs
  room for, and a retry loop on a broken disk is a spinning writer. Counted instead.
- **Pruning old rows.** `FileJournal` never prunes either; a store that did would be a policy
  (how long must history be kept?) this row has no owner decision for. Out of scope, stated.

## Consequences

**Good**

- The engine thread cannot tell this store from `FileJournal` `Async`: the same ring push, the
  same counters, the same retire; the kill line's *engine-thread allocations 0* and *wire p50 in
  band* are properties of a path it already measured.
- The five journal ADRs (0110, 0150, 0153, 0154, 0155) hold for a second implementation, and the
  engine's own bookkeeping — not a copy of it — enforces three of them (ADR-0181).
- An operator gets the journal as SQL: `SELECT` the messages of a session, check
  `highest_out` without a tool.

**Bad — and accepted**

- **A process kill loses up to a batch more than `FileJournal` `Async` does.** The file writer's
  `write` reaches the page cache per record; the store's rows exist only once their batch
  commits. Bounded by the ring and `batch_max`, and every lost record is still in memory until
  the crash and is gap-filled after it — the same class of loss, a larger window.
- **No power-loss safety by default**, and none at all *before sending*. Stated in `GUIDE.md`
  and `CONFIGURATION.md`.
- **Per-session memory rises**: a 4 MiB ring beside the ~2 MiB `MemJournal`.
- **The database grows forever**, as the journal file does. `[measured 2026-09-24, probe]` about
  216 bytes per 200-byte message: a 60 s soak at 50 000 msg/s writes ~650 MB.
- **One SQLite per process.** `links = "sqlite3"` makes cargo refuse a second `libsqlite3-sys`
  in the same build, and a C library that brings its own SQLite is the corruption hazard of
  howtocorrupt §2.3. Neither is visible to the compiler; `GUIDE.md` names both.
- **The one-appender guard rests on a POSIX lock any `close()` in the process can drop**
  (howtocorrupt §2.2). A user who opens the database file with `std::fs` while the store runs —
  to back it up, to peek at it — can silently remove it. `GUIDE.md` says: back up with SQLite's
  own backup API or after `close`.
- **`open` on the engine thread costs a database read** in the single-engine `serve*` loops, as
  a file journal's open already does; the sharded runtime moves it to the acceptor thread.
- **C in the build** of anyone who opts in (ADR-0182).
