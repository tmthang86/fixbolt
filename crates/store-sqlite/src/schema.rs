//! What `open` does to the database before the writer gets it: the pragmas
//! that make SQLite's lock the one-appender rule, the schema, the identity
//! check, and the read-back a restart needs. **Runs on the thread that calls
//! `open`**, never on the writer and never per message. ADR-0180 decisions 2,
//! 5 and 10.

use std::io;
use std::path::Path;
use std::time::Duration;

use fixbolt_session::Config;
use rusqlite::{Connection, ErrorCode, OptionalExtension};

use crate::journal::{SqliteOptions, Synchronous};

/// `PRAGMA user_version` of the one schema this build writes and reads.
pub(crate) const USER_VERSION: i64 = 1;

/// The schema, version [`USER_VERSION`]. `messages` is keyed by `seq` alone
/// and written with `INSERT OR REPLACE`, so a number used again keeps its
/// newest bytes (D7); `session` has exactly one row.
const CREATE: &str = "
    CREATE TABLE messages (
        seq  INTEGER PRIMARY KEY,
        body BLOB NOT NULL
    );
    CREATE TABLE session (
        id             INTEGER PRIMARY KEY CHECK (id = 1),
        begin_string   BLOB NOT NULL,
        sender         BLOB NOT NULL,
        target         BLOB NOT NULL,
        highest_in     INTEGER,
        highest_out    INTEGER,
        last_active_ms INTEGER
    );
    PRAGMA user_version = 1;
";

/// What a restart reads back: the three marks and the last `N` messages,
/// oldest first.
#[derive(Debug, Default)]
pub(crate) struct Loaded {
    pub(crate) highest_in: Option<u32>,
    pub(crate) highest_out: Option<u32>,
    pub(crate) last_active: Option<u64>,
    pub(crate) messages: Vec<(u32, Vec<u8>)>,
}

/// Open, lock, check and read the database at `path` for the session `cfg`
/// names, keeping at most `n` messages of it.
///
/// **One appender is SQLite's own lock** (ADR-0180 decision 5):
/// `locking_mode = EXCLUSIVE` and `busy_timeout = 0` before the first access,
/// then one `BEGIN IMMEDIATE … COMMIT`, after which the write lock is held
/// until the connection closes. A second `open` — this process or another —
/// gets `SQLITE_BUSY` at once, answered as `WouldBlock`.
/// `tests/store.rs::a_second_open_of_a_held_database_is_refused_at_once`.
pub(crate) fn open(
    path: &Path,
    cfg: &Config,
    opts: &SqliteOptions,
    n: usize,
) -> io::Result<(Connection, Loaded)> {
    let conn = Connection::open(path).map_err(|e| to_io(&e, path))?;
    conn.busy_timeout(Duration::ZERO)
        .map_err(|e| to_io(&e, path))?;
    conn.execute_batch("PRAGMA locking_mode = EXCLUSIVE")
        .map_err(|e| to_io(&e, path))?;
    let mode: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
        .map_err(|e| to_io(&e, path))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(io::Error::other(format!(
            "sqlite store {}: journal_mode is {mode}, not wal",
            path.display()
        )));
    }
    let sync = match opts.synchronous {
        Synchronous::Normal => "PRAGMA synchronous = NORMAL",
        Synchronous::Full => "PRAGMA synchronous = FULL",
    };
    conn.execute_batch(sync).map_err(|e| to_io(&e, path))?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| to_io(&e, path))?;
    match check_and_load(&conn, path, cfg, n) {
        Ok(loaded) => {
            conn.execute_batch("COMMIT").map_err(|e| to_io(&e, path))?;
            // After the schema exists, so creating it is never what the
            // ceiling refuses. SQLite never sets it below the current size.
            if let Some(pages) = opts.max_db_pages {
                conn.execute_batch(&format!("PRAGMA max_page_count = {pages}"))
                    .map_err(|e| to_io(&e, path))?;
            }
            Ok((conn, loaded))
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// Inside the `BEGIN IMMEDIATE`: create the schema on a new database, or
/// check the version and the identity of an old one and read it back.
fn check_and_load(conn: &Connection, path: &Path, cfg: &Config, n: usize) -> io::Result<Loaded> {
    let sql = |e: rusqlite::Error| to_io(&e, path);
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(sql)?;
    match version {
        0 => {
            let tables: i64 = conn
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))
                .map_err(sql)?;
            if tables != 0 {
                return Err(invalid(format!(
                    "sqlite store {}: the database has tables but no schema version; \
                     it is not a fixbolt store",
                    path.display()
                )));
            }
            conn.execute_batch(CREATE).map_err(sql)?;
            conn.execute(
                "INSERT INTO session (id, begin_string, sender, target) VALUES (1, ?1, ?2, ?3)",
                (
                    cfg.begin_string(),
                    cfg.sender_comp_id(),
                    cfg.target_comp_id(),
                ),
            )
            .map_err(sql)?;
            Ok(Loaded::default())
        }
        USER_VERSION => load(conn, path, cfg, n),
        other => Err(invalid(format!(
            "sqlite store {}: schema version {other}; this build reads version {USER_VERSION}",
            path.display()
        ))),
    }
}

/// The session row, checked against `cfg`, then the last `n` messages.
fn load(conn: &Connection, path: &Path, cfg: &Config, n: usize) -> io::Result<Loaded> {
    let sql = |e: rusqlite::Error| to_io(&e, path);
    type Row = (
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Option<u32>,
        Option<u32>,
        Option<i64>,
    );
    let row: Option<Row> = conn
        .query_row(
            "SELECT begin_string, sender, target, highest_in, highest_out, last_active_ms \
             FROM session WHERE id = 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(sql)?;
    let Some((begin, sender, target, highest_in, highest_out, last_active)) = row else {
        return Err(invalid(format!(
            "sqlite store {}: the session row is missing",
            path.display()
        )));
    };
    if begin != cfg.begin_string()
        || sender != cfg.sender_comp_id()
        || target != cfg.target_comp_id()
    {
        return Err(invalid(format!(
            "sqlite store {} belongs to session {} and was opened for {}",
            path.display(),
            identity(&begin, &sender, &target),
            identity(
                cfg.begin_string(),
                cfg.sender_comp_id(),
                cfg.target_comp_id()
            ),
        )));
    }
    let limit = i64::try_from(n).unwrap_or(i64::MAX);
    let mut stmt = conn
        .prepare("SELECT seq, body FROM messages ORDER BY seq DESC LIMIT ?1")
        .map_err(sql)?;
    let mut messages = stmt
        .query_map([limit], |r| {
            Ok((r.get::<_, u32>(0)?, r.get::<_, Vec<u8>>(1)?))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    messages.reverse();
    Ok(Loaded {
        highest_in,
        highest_out,
        last_active: last_active.and_then(|ms| u64::try_from(ms).ok()),
        messages,
    })
}

/// `FIX.4.4 ISLD->TW44`, for an error message.
fn identity(begin: &[u8], sender: &[u8], target: &[u8]) -> String {
    format!(
        "{} {}->{}",
        String::from_utf8_lossy(begin),
        String::from_utf8_lossy(sender),
        String::from_utf8_lossy(target)
    )
}

fn invalid(text: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, text)
}

/// A SQLite error as an `io::Error`: **busy or locked is `WouldBlock`**, as
/// `FileJournal::open` answers a held file (ADR-0154 decision 1).
pub(crate) fn to_io(e: &rusqlite::Error, path: &Path) -> io::Error {
    match e.sqlite_error_code() {
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => io::Error::new(
            io::ErrorKind::WouldBlock,
            format!(
                "sqlite store {} is held by another appender: a writer still committing, \
                 or another process ({e})",
                path.display()
            ),
        ),
        _ => io::Error::other(format!("sqlite store {}: {e}", path.display())),
    }
}
