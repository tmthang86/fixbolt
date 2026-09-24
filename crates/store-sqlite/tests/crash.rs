//! **A killed process resumes from what it committed.** The store's promise
//! under `synchronous = NORMAL` is about a process kill, not a power loss
//! (ADR-0180 decision 4), so that is what is done to it: a child process
//! writes as fast as its writer commits, and the parent `SIGKILL`s it
//! mid-stream.
//!
//! The child is this same test binary, run again with [`CHILD`] set. It prints
//! `committed <n>` every time `progress().highest_out()` rises; the parent
//! kills it once `n` ≥ [`KILL_AT`] and then asserts only on numbers the child
//! printed — never on timing.
//!
//! Plan `docs/plans/2026-09-24-p4-sqlite-store.md`, *Chia việc* row 2.
#![cfg(feature = "sqlite")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use fixbolt_engine::recovery::Resumed;
use fixbolt_session::Config;
use fixbolt_session::journal::Journal;
use fixbolt_store_sqlite::{SqliteJournal, SqliteOptions};

type Db = SqliteJournal<64, 512>;

/// Set to the database path, it makes this test the child.
const CHILD: &str = "FIXBOLT_STORE_SQLITE_CRASH_CHILD";
/// The parent kills the child once it has printed a number at least this high.
const KILL_AT: u32 = 5_000;
/// Messages in flight past the last commit, at most: keeps the ring from
/// filling, so the database has no hole the kill did not make.
const IN_FLIGHT: u32 = 1_000;
/// The child gives up on its own after this many, so a parent that never
/// kills it does not hang the test run.
const CHILD_LIMIT: u32 = 2_000_000;

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

fn body(seq: u32) -> Vec<u8> {
    format!(
        "8=FIX.4.4\x019=5\x0135=D\x0134={seq}\x0111=CRASH{seq}\x0138=100\x0155=FXB\x0110=000\x01"
    )
    .into_bytes()
}

/// The child: write, pace against the writer, print what is committed.
fn child(path: &Path) -> ! {
    let mut j = Db::open(path, &cfg(), SqliteOptions::default()).expect("child open");
    let progress = j.progress();
    let mut out = std::io::stdout().lock();
    let mut said = 0u32;
    for seq in 1..=CHILD_LIMIT {
        assert!(j.put(seq, &body(seq)));
        loop {
            let committed = progress.highest_out().unwrap_or(0);
            if committed > said {
                said = committed;
                writeln!(out, "committed {said}").expect("stdout");
                out.flush().expect("stdout");
            }
            if seq.saturating_sub(committed) < IN_FLIGHT {
                break;
            }
            std::thread::sleep(Duration::from_micros(100));
        }
        if j.unwritten() > 0 {
            writeln!(out, "unwritten {}", j.unwritten()).expect("stdout");
            std::process::exit(3);
        }
    }
    writeln!(out, "done").expect("stdout");
    std::process::exit(4);
}

fn scratch() -> PathBuf {
    let p = std::env::temp_dir().join(format!("fixbolt-store-sqlite-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("scratch dir");
    p.join("s.db")
}

#[test]
fn a_killed_process_resumes_from_what_it_committed() {
    if let Some(path) = std::env::var_os(CHILD) {
        child(Path::new(&path));
    }
    let path = scratch();
    let mut kid = Command::new(std::env::current_exe().expect("this binary"))
        .args([
            "--exact",
            "a_killed_process_resumes_from_what_it_committed",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, &path)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn the child");
    let lines = BufReader::new(kid.stdout.take().expect("piped"));
    let started = Instant::now();
    let mut last = 0u32;
    let mut other = Vec::new();
    for line in lines.lines() {
        let line = line.expect("utf-8 from the child");
        if let Some(n) = line.strip_prefix("committed ") {
            last = n.parse().expect("a number");
            if last >= KILL_AT {
                kid.kill().expect("SIGKILL the child");
                break;
            }
        } else if line.starts_with("unwritten") || line == "done" {
            other.push(line);
            break;
        }
    }
    let status = kid.wait().expect("reap the child");
    assert!(
        other.is_empty() && last >= KILL_AT,
        "the child was not killed mid-stream: last printed {last}, then {other:?}, {status}, after {:?}",
        started.elapsed()
    );

    // Read with SQLite directly first — the store's own `open` holds the file
    // exclusively once it has it.
    let highest_out = {
        let db = rusqlite::Connection::open(&path).expect("open after the kill");
        let check: String = db
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .expect("integrity_check");
        assert_eq!(check, "ok", "the database survived the kill whole");
        let highest_out: u32 = db
            .query_row("SELECT highest_out FROM session WHERE id = 1", [], |r| {
                r.get(0)
            })
            .expect("the session row");
        assert!(
            highest_out >= last,
            "the database says {highest_out} was committed, the child saw {last} committed"
        );
        let mut stmt = db
            .prepare("SELECT seq, body FROM messages WHERE seq <= ?1 ORDER BY seq")
            .expect("prepare");
        let mut want = 1u32;
        let rows = stmt
            .query_map([highest_out], |r| {
                Ok((r.get::<_, u32>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .expect("query");
        for row in rows {
            let (seq, bytes) = row.expect("a row");
            assert_eq!(
                seq, want,
                "every number up to {highest_out} is there, in order"
            );
            assert_eq!(bytes, body(seq), "{seq} came back byte for byte");
            want += 1;
        }
        assert_eq!(want, highest_out + 1, "and none is missing at the end");
        highest_out
    };

    let j = Db::open(&path, &cfg(), SqliteOptions::default()).expect("the store reopens");
    let resumed = Resumed::from_journal(j).expect("something was left behind");
    assert_eq!(
        resumed.next_out,
        highest_out + 1,
        "the next number is the one after"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("a directory"));
}
