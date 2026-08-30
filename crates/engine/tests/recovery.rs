//! What survives a restart. `STATUS.md` open item 16.
//!
//! `DESIGN.md` D7 ships three journal tiers and `Durability::Fsync` pays for a
//! `sync_data` per message. Until this file existed **nothing ever read a
//! journal back**, so that payment bought an audit trail rather than a recovery
//! mechanism — the worst kind of cost, because it looks like a guarantee.
//!
//! These tests use `FileJournal` and **drop it between the write and the read**.
//! A test that used `MemJournal` here would prove exactly nothing: the bytes are
//! in memory either way, and the restart is the whole question.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_engine::journal::{Durability, FileJournal};
use fixbolt_session::journal::Journal;

fn tmp(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "fixbolt-recovery-{name}-{}",
        std::process::id() as u64 * 1000
            + std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| u64::from(d.subsec_nanos()) % 1000)
                .unwrap_or(0)
    ));
    p
}

const A: &[u8] = b"8=FIX.4.4\x019=10\x0135=D\x0134=7\x0110=000\x01";
const B: &[u8] = b"8=FIX.4.4\x019=10\x0135=D\x0134=8\x0110=000\x01";

/// The number a resumed session has to start from.
#[test]
fn a_reopened_journal_knows_the_highest_sequence_it_holds() {
    let path = tmp("highest");
    {
        let mut j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Fsync).expect("open");
        j.put(7, A);
        j.put(8, B);
        assert_eq!(j.highest(), Some(8), "before the restart");
    }
    // Dropped. Everything below is a different process's view of the same file.
    let j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Fsync).expect("reopen");
    assert_eq!(
        j.highest(),
        Some(8),
        "a journal that cannot say this cannot resume a session"
    );
    let _ = std::fs::remove_file(&path);
}

/// And the messages themselves, because a resumed session must answer a
/// `ResendRequest` for what it sent before the restart.
#[test]
fn a_reopened_journal_replays_what_it_held() {
    let path = tmp("replay");
    {
        let mut j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Fsync).expect("open");
        j.put(7, A);
        j.put(8, B);
    }
    let j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Fsync).expect("reopen");
    assert_eq!(j.get(7), Some(A), "message 7 after the restart");
    assert_eq!(j.get(8), Some(B), "message 8 after the restart");
    let _ = std::fs::remove_file(&path);
}

/// An empty journal has nothing to say, and must say that rather than guess.
#[test]
fn an_empty_journal_has_no_highest() {
    let path = tmp("empty");
    let j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Fsync).expect("open");
    assert_eq!(j.highest(), None);
    let _ = std::fs::remove_file(&path);
}

/// **A journal cut in half must not be believed to the end.** A process killed
/// mid-write leaves a partial record, and reading it as a message would replay
/// bytes that never went on the wire.
#[test]
fn a_truncated_record_is_dropped_not_guessed() {
    let path = tmp("truncated");
    {
        let mut j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Fsync).expect("open");
        j.put(7, A);
        j.put(8, B);
    }
    let whole = std::fs::read(&path).expect("read");
    // Cut three bytes off the last record: enough to lose the tail, not enough
    // to lose the record before it.
    std::fs::write(&path, &whole[..whole.len() - 3]).expect("truncate");

    let j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Fsync).expect("reopen");
    assert_eq!(j.highest(), Some(7), "the whole record survives");
    assert_eq!(j.get(7), Some(A), "and is readable");
    assert_eq!(j.get(8), None, "the torn one is gone, not half-read");
    let _ = std::fs::remove_file(&path);
}
