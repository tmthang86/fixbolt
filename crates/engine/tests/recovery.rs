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
use fixbolt_session::{Acceptor, Config, Session};

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

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

// ------------------------------------- a reconnect is not a restart (ADR-0010)

/// [ADR-0010](../../../docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)
/// decision 1: a session built from what a journal reports keeps its numbers,
/// where a genuinely new one starts at 1.
///
/// **This is the test the corpus cannot write.** FIX 4.4 numbers a *session*,
/// not a *connection*, but all seven `iCONNECT`s across the three files that
/// reconnect expect `34=1` back — because the harness starts each one from a
/// clean store. That is a property of how the tests are run, not a statement
/// about what an acceptor owes a counterparty.
#[test]
fn a_session_resumed_from_a_journal_keeps_counting() {
    let path = tmp("resume");
    {
        let mut j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Fsync).expect("open");
        j.put(7, A);
        j.put(8, B);
    }
    // The restart. Everything above is gone; only the file is left.
    let j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Fsync).expect("reopen");
    let highest = j.highest().expect("the journal held something");

    let mut s: Session<Acceptor, 64> = Session::resume(cfg(), highest + 1, 12);
    assert_eq!(s.next_out(), 9, "carried in from the journal");
    assert_eq!(s.next_in(), 12);

    // And the connection that follows the restart must NOT wipe them.
    s.connect(|_| {});
    assert_eq!(
        (s.next_out(), s.next_in()),
        (9, 12),
        "connect on a resumed session keeps the count — a reconnect is not a \
         restart, and this is the whole of ADR-0010"
    );
}

/// The other half, and the one that keeps the acceptance gate meaning what it
/// means: a session nobody resumed still resets on every connect, so the corpus
/// needs no exemption and none is granted (ADR-0010 decision 3).
///
/// **The counters are moved off 1 before the reconnect, and that is the whole
/// test.** `[cost 2026-08-31]` the first version of this simply connected a
/// fresh session twice and asserted `(1, 1)` — which is true whether `connect`
/// resets or does nothing at all, because a new session is already there. It
/// passed against a deliberately broken `connect` that never reset, and only
/// the acceptance score noticed, dropping 59 → 56. That is `false-greens.md`
/// §17: a test asserting about state it assembled itself.
#[test]
fn a_new_session_still_restarts_on_every_connect() {
    let mut fresh: Session<Acceptor, 64> = Session::new(cfg());
    fresh.connect(|_| {});
    // Move it off 1 the only way this layer offers without a whole handshake.
    fresh.logout_now(b"bye", |_| {});
    assert!(
        fresh.next_out() > 1,
        "the reconnect below proves nothing unless the count actually moved"
    );

    fresh.connect(|_| {});
    assert_eq!(
        (fresh.next_out(), fresh.next_in()),
        (1, 1),
        "a second connection to a session that never persisted anything resets, \
         which is exactly what every iCONNECT in the corpus expects"
    );
}

/// And the same again from the resumed side: numbers that have moved stay
/// moved. Together with the test above this pins both arms of the branch, so
/// neither `if true` nor `if false` can pass.
#[test]
fn a_resumed_session_keeps_counting_across_a_reconnect() {
    let mut s: Session<Acceptor, 64> = Session::resume(cfg(), 40, 50);
    s.connect(|_| {});
    s.logout_now(b"bye", |_| {});
    let after = (s.next_out(), s.next_in());
    assert_eq!(after, (41, 50), "logout consumed one outbound number");

    s.connect(|_| {});
    assert_eq!(
        (s.next_out(), s.next_in()),
        after,
        "reconnecting a resumed session touches neither count"
    );
}
