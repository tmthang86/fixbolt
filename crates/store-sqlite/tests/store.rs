//! What the store keeps, what it gives back after a restart, and how its
//! writer stops — every test through the public API, and the database read
//! only **after** the store has closed it (ADR-0180 decision 5: nothing in
//! this process may open that file beside SQLite while SQLite holds it).
//!
//! Plan `docs/plans/2026-09-24-p4-sqlite-store.md`, *Chia việc* row 2;
//! [ADR-0180](../../../docs/decisions/ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md).
//!
//! **One test at a time** ([`one_at_a_time`]): the retired-writer count is
//! process-wide, and `the_writer_sleeps_when_idle` looks for the one writer
//! thread in the process by name.
#![cfg(feature = "sqlite")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use fixbolt_engine::journal::{wait_for_retired_writers, writers_retired};
use fixbolt_engine::recovery::Resumed;
use fixbolt_session::Config;
use fixbolt_session::journal::Journal;
use fixbolt_store_sqlite::{SqliteJournal, SqliteOptions};

type Db = SqliteJournal<8, 512>;

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// A fresh directory for one test's database and its `-wal`.
fn dir(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "fixbolt-store-sqlite-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("scratch dir");
    p
}

fn open<const N: usize, const LEN: usize>(path: &Path) -> SqliteJournal<N, LEN> {
    SqliteJournal::open(path, &cfg(), SqliteOptions::default())
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
}

fn message(seq: u32) -> Vec<u8> {
    format!("8=FIX.4.4\x019=5\x0135=D\x0134={seq}\x0111=ORD{seq}\x0110=000\x01").into_bytes()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Wait until `done` holds or five seconds pass; `true` if it held.
fn within_5s(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    done()
}

/// Everything a restart needs comes back: the messages, both counts, and the
/// instant the session was last alive — and `Resumed` computes the numbers
/// from them as it does for a file.
#[test]
fn a_reopened_store_answers_what_it_was_told() {
    let _one = one_at_a_time();
    let path = dir("reopen").join("s.db");
    {
        let mut j: Db = open(&path);
        for seq in 1..=3 {
            assert!(j.put(seq, &message(seq)));
        }
        j.mark_in(7);
        j.mark_out(5);
        j.mark_active(1_788_431_527_120);
        j.close();
    }
    let j: Db = open(&path);
    assert_eq!(j.highest(), Some(3), "the three messages are back");
    assert_eq!(j.get(2), Some(message(2).as_slice()), "byte for byte");
    assert_eq!(j.oldest(), Some(1));
    assert_eq!(j.highest_in(), Some(7), "the inbound mark");
    assert_eq!(j.highest_out(), Some(5), "the outbound mark");
    assert_eq!(
        j.last_active(),
        Some(1_788_431_527_120),
        "the activity mark"
    );
    let resumed = Resumed::from_journal(j).expect("something was left behind");
    assert_eq!(resumed.next_out, 6);
    assert_eq!(resumed.next_in, 8);
}

/// `Admin::SetNextOut` can wind the count back, and the number is used again:
/// the newest bytes must win after a restart, as they win in the ring (D7).
#[test]
fn a_reused_number_keeps_the_newest_bytes() {
    let _one = one_at_a_time();
    let path = dir("reused").join("s.db");
    let (old, new) = (
        message(1),
        b"8=FIX.4.4\x0135=D\x0134=1\x0111=NEWER\x0110=000\x01",
    );
    {
        let mut j: Db = open(&path);
        assert!(j.put(1, &old));
        let progress = j.progress();
        assert!(
            within_5s(|| progress.committed() >= 1),
            "premise: the first copy was committed on its own"
        );
        assert!(j.put(1, new));
        j.close();
        assert_eq!(
            j.unwritten(),
            0,
            "premise: nothing failed to reach the database"
        );
    }
    let j: Db = open(&path);
    assert_eq!(
        j.get(1),
        Some(new.as_slice()),
        "the second copy of number 1 is the one a restart replays"
    );
}

/// The database keeps everything; the ring after a restart holds the last `N`.
#[test]
fn reopen_loads_only_the_last_n_into_the_ring() {
    let _one = one_at_a_time();
    let path = dir("last-n").join("s.db");
    {
        let mut j: Db = open(&path);
        for seq in 1..=20 {
            assert!(j.put(seq, &message(seq)));
        }
        j.close();
    }
    let j: Db = open(&path);
    for seq in 13..=20 {
        assert_eq!(
            j.get(seq),
            Some(message(seq).as_slice()),
            "{seq} is among the last 8"
        );
    }
    for seq in 1..=12 {
        assert_eq!(j.get(seq), None, "{seq} is older than the ring holds");
    }
    assert_eq!(j.oldest(), Some(13));
    assert_eq!(j.highest(), Some(20));
    assert_eq!(j.highest_out(), Some(20));
}

/// One appender is SQLite's own lock, and a second `open` is refused **at
/// once** — never a wait on the thread that asked (ADR-0180 decision 5).
#[test]
fn a_second_open_of_a_held_database_is_refused_at_once() {
    let _one = one_at_a_time();
    let path = dir("held").join("s.db");
    let mut first: Db = open(&path);
    let asked = Instant::now();
    let second = Db::open(&path, &cfg(), SqliteOptions::default());
    let took = asked.elapsed();
    let err = second.err().expect("a held database is refused");
    assert_eq!(err.kind(), ErrorKind::WouldBlock, "{err}");
    assert!(
        took < Duration::from_millis(100),
        "the refusal took {took:?}: open waited for the lock"
    );
    first.close();
    drop(first);
    let _again: Db = open(&path);
}

/// A database carries the identity of the session it belongs to; opening it
/// for another session would continue the wrong numbers.
#[test]
fn a_database_of_another_session_is_refused() {
    let _one = one_at_a_time();
    let path = dir("identity").join("s.db");
    {
        let mut j: Db = open(&path);
        assert!(j.put(1, &message(1)));
        j.close();
    }
    let other = Config::acceptor(b"FIX.4.4", b"ISLD", b"OTHER");
    let err = Db::open(&path, &other, SqliteOptions::default())
        .err()
        .expect("another session's database is refused");
    assert_eq!(err.kind(), ErrorKind::InvalidData, "{err}");
    let text = err.to_string();
    assert!(
        text.contains("TW44") && text.contains("OTHER"),
        "the refusal names both identities: {text}"
    );
    let _mine: Db = open(&path);
}

/// A schema version this build does not know is refused, not guessed at.
#[test]
fn a_database_of_an_unknown_schema_version_is_refused() {
    let _one = one_at_a_time();
    let path = dir("version").join("s.db");
    {
        let mut j: Db = open(&path);
        j.close();
    }
    {
        let db = rusqlite::Connection::open(&path).expect("open with SQLite");
        db.execute_batch("PRAGMA user_version = 99")
            .expect("set a future version");
    }
    let err = Db::open(&path, &cfg(), SqliteOptions::default())
        .err()
        .expect("an unknown schema version is refused");
    assert_eq!(err.kind(), ErrorKind::InvalidData, "{err}");
    assert!(err.to_string().contains("99"), "{err}");
}

/// The engine's rule for a retired writer, through the engine's handles
/// (ADR-0181): `retire` lets it go without waiting, the retire is counted,
/// and once `wait_for_retired_writers` says done, everything is committed,
/// the database is let go, and it opens again.
#[test]
fn a_retired_writer_commits_everything_then_releases_then_is_counted_done() {
    let _one = one_at_a_time();
    let path = dir("retire").join("s.db");
    let mut j: SqliteJournal<128, 512> = open(&path);
    let released = j.released();
    for seq in 1..=100 {
        assert!(j.put(seq, &message(seq)));
    }
    let ever = writers_retired();
    j.retire();
    assert_eq!(
        writers_retired(),
        ever + 1,
        "retire() retired the writer through its ticket"
    );
    drop(j);
    assert!(
        wait_for_retired_writers(Duration::from_secs(5)),
        "the retired writer finished"
    );
    assert!(
        released.is_released(),
        "a finished writer has let the database go"
    );
    let j: SqliteJournal<128, 512> = open(&path);
    for seq in 1..=100 {
        assert_eq!(
            j.get(seq),
            Some(message(seq).as_slice()),
            "{seq} was committed before the writer finished"
        );
    }
}

/// ADR-0110 decision 4 in database form: a message carrying a secret is kept
/// in memory and leaves only its number on disk — in the database **and** its
/// `-wal`.
#[test]
fn a_message_carrying_a_secret_leaves_only_its_number() {
    let _one = one_at_a_time();
    let home = dir("secret");
    let path = home.join("s.db");
    let report = b"8=FIX.4.4\x019=5\x0135=8\x0134=1\x0117=EXEC1\x0110=000\x01";
    let secret: &[u8] = b"pwSQLITEq7Xk2";
    let with_secret = [
        b"8=FIX.4.4\x019=5\x0135=BE\x0134=2\x01553=alice\x01554=".as_slice(),
        secret,
        b"\x01923=UR1\x0110=000\x01",
    ]
    .concat();
    {
        let mut j: Db = open(&path);
        assert!(j.put(1, report));
        assert!(j.put(2, &with_secret));
        assert_eq!(
            j.get(2),
            Some(with_secret.as_slice()),
            "memory keeps the secret: a resend in this process replays it"
        );
        j.close();
    }
    let mut disk = std::fs::read(&path).expect("the database");
    let wal = home.join("s.db-wal");
    disk.extend(std::fs::read(&wal).unwrap_or_default());
    assert!(
        contains(&disk, b"\x0135=8\x01"),
        "premise: the ExecutionReport is on disk"
    );
    assert!(
        !contains(&disk, secret),
        "the password reached the database or its WAL"
    );
    let j: Db = open(&path);
    assert_eq!(
        j.highest_out(),
        Some(2),
        "its number is spent after a restart"
    );
    assert_eq!(j.get(2), None, "and it has no bytes to replay: a gap fill");
    assert_eq!(j.get(1), Some(report.as_slice()));
}

/// A record the writer's ring has no room for is kept in memory, counted, and
/// `put` still says it was kept (ADR-0154 decision 4).
#[test]
fn a_record_larger_than_the_ring_is_counted_and_put_still_answers_true() {
    let _one = one_at_a_time();
    let path = dir("ring").join("s.db");
    let opts = SqliteOptions {
        ring_bytes: 64,
        ..SqliteOptions::default()
    };
    let mut j = Db::open(&path, &cfg(), opts).expect("open");
    let big = [
        b"8=FIX.4.4\x0135=D\x0158=".as_slice(),
        &[b'x'; 200],
        b"\x01",
    ]
    .concat();
    assert!(j.put(1, &big), "memory kept it");
    assert_eq!(j.unwritten(), 1, "and the database will not have it");
    assert_eq!(j.get(1), Some(big.as_slice()));
}

/// A batch the database refuses (here: full) is rolled back and counted in
/// `unwritten`, while memory still answers (ADR-0180 decision 8).
#[test]
fn a_failed_commit_is_counted_in_unwritten() {
    let _one = one_at_a_time();
    let path = dir("full").join("s.db");
    let opts = SqliteOptions {
        // Below what the schema already takes, so SQLite holds it at the
        // current size: any growth is SQLITE_FULL.
        max_db_pages: Some(1),
        ..SqliteOptions::default()
    };
    let mut j = Db::open(&path, &cfg(), opts).expect("open");
    let body = [
        b"8=FIX.4.4\x0135=D\x0158=".as_slice(),
        &[b'y'; 480],
        b"\x01",
    ]
    .concat();
    for seq in 1..=64 {
        assert!(j.put(seq, &body), "memory kept {seq}");
    }
    assert!(
        within_5s(|| j.unwritten() > 0),
        "a full database is counted in unwritten, saw {}",
        j.unwritten()
    );
    assert_eq!(j.get(64), Some(body.as_slice()), "memory still answers");
}

/// The writer gives its core back when idle, by the engine's own rule
/// (`ring::Idle`) — measured as `crates/engine/tests/writer_idle.rs` measures
/// the file journal's writer.
#[cfg(target_os = "linux")]
#[test]
fn the_writer_sleeps_when_idle() {
    let _one = one_at_a_time();
    let path = dir("idle").join("s.db");
    let mut j: Db = open(&path);
    assert!(j.put(1, &message(1)));
    std::thread::sleep(Duration::from_millis(200));
    let ms = idle::cpu_ms("fixbolt-sqlite");
    j.close();
    assert!(
        ms < idle::CEILING_MS,
        "the SQLite writer used {ms:.0} ms of CPU in 1 s while idle"
    );
}

/// Copied from `crates/engine/tests/writer_idle.rs`, the gate for the same
/// rule on the file journal's writer.
#[cfg(target_os = "linux")]
mod idle {
    use std::time::{Duration, Instant};

    const CLK_TCK: f64 = 100.0;
    const WINDOW: Duration = Duration::from_secs(1);
    const SAMPLES: u32 = 10;
    /// 50 ms of CPU in the 1 s window: a spinning writer asks for all of it.
    pub const CEILING_MS: f64 = 50.0;

    fn tid_named(comm: &str) -> Option<String> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir("/proc/self/task").ok()? {
            let Ok(entry) = entry else { continue };
            let tid = entry.file_name().to_string_lossy().into_owned();
            let name =
                std::fs::read_to_string(format!("/proc/self/task/{tid}/comm")).unwrap_or_default();
            if name.trim_end() == comm {
                found.push(tid);
            }
        }
        assert!(
            found.len() <= 1,
            "{} threads named {comm}: {found:?}",
            found.len()
        );
        found.pop()
    }

    fn task_cpu(tid: &str) -> Option<u64> {
        let text = std::fs::read_to_string(format!("/proc/self/task/{tid}/stat")).ok()?;
        let (_, rest) = text.rsplit_once(')')?;
        let mut fields = rest.split_whitespace();
        let _state = fields.next()?;
        let utime: u64 = fields.nth(10)?.parse().ok()?;
        let stime: u64 = fields.next()?.parse().ok()?;
        Some(utime.saturating_add(stime))
    }

    pub fn cpu_ms(comm: &str) -> f64 {
        let tid = tid_named(comm).unwrap_or_else(|| panic!("no thread named {comm}"));
        let before = task_cpu(&tid).expect("the writer's stat");
        let started = Instant::now();
        let mut alive = 0u32;
        for _ in 0..SAMPLES {
            std::thread::sleep(WINDOW / SAMPLES);
            if task_cpu(&tid).is_some() {
                alive += 1;
            }
        }
        let elapsed = started.elapsed().as_secs_f64();
        let after = task_cpu(&tid).expect("the writer vanished during the window");
        assert_eq!(alive, SAMPLES, "the writer could not be read throughout");
        let ms = (after - before) as f64 * 1000.0 / CLK_TCK * (WINDOW.as_secs_f64() / elapsed);
        eprintln!("{comm}: {ms:.0} ms of CPU in {elapsed:.2} s idle");
        ms
    }
}
