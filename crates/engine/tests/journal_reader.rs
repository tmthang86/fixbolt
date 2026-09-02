//! Answering *"we sent order X, did you receive it?"* from the file.
//!
//! **Step 1 of [what-the-journal-can-answer], red at an assertion.**
//!
//! # Why the ring cannot answer it
//!
//! `FileJournal<N, LEN>` reloads the file into a fixed ring on open, because
//! its job is **recovery**: answering the next `ResendRequest`, which is about
//! recent messages. The operations question is about a message that may be
//! very old, and the ring has long since dropped it.
//!
//! # Why the journal is dropped before anything is read
//!
//! `Durability::Async` hands each write to another thread, so reading straight
//! after writing can read a file the writer has not finished. `FileJournal`
//! **joins that thread on drop**, which is what makes the drop the settle
//! point rather than a sleep. The same discipline `tests/recovery.rs` uses.
//!
//! `[measured 2026-09-02]` `Fsync` was the first choice and it made one test
//! take **39.9 seconds** for 5 000 messages, which is a gate nobody will run.
//! Async plus the join is the same guarantee for this purpose — the bytes are
//! in the file before the read — at a hundredth of the cost.
//!
//! [what-the-journal-can-answer]: ../../../docs/plans/2026-09-02-what-the-journal-can-answer.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use fixbolt_engine::journal::{Durability, FileJournal, Reader, Record};
use fixbolt_session::journal::Journal;

/// A scratch path that does not collide between tests in one run.
fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "fixbolt-jrnl-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

/// One message per sequence number, distinguishable by content.
fn body(seq: u32) -> Vec<u8> {
    format!("8=FIX.4.4\u{1}35=D\u{1}34={seq}\u{1}11=ORDER-{seq}\u{1}").into_bytes()
}

/// Write `count` messages and an inbound mark, then let the journal go.
fn write_history(path: &std::path::Path, count: u32) {
    let mut j: FileJournal<8, 512> =
        FileJournal::open(path, Durability::Async).expect("open for writing");
    for seq in 1..=count {
        j.put(seq, &body(seq));
    }
    j.mark_in(count + 1);
    drop(j);
}

/// **The specification.** A message written long ago is still answerable from
/// the file, after the ring that held it has moved on.
///
/// `[verified 2026-09-02]` the ring is 8 deep, so message 3 of 5 000 is gone
/// from it, and nothing else can read the file.
#[test]
fn a_message_written_long_ago_is_still_answerable_from_the_file() {
    let path = tmp("old-message");
    write_history(&path, 5_000);

    let reader = Reader::open(&path).expect("read the file");
    let found = reader
        .records()
        .find_map(|r| match r {
            Record::Message { seq: 3, bytes } => Some(bytes.to_vec()),
            _ => None,
        })
        .unwrap_or_default();

    let _ = std::fs::remove_file(&path);
    assert_eq!(
        found,
        body(3),
        "the operations desk asked what message 3 was, and the file has it"
    );
}

/// The control: the *recent* end is answerable today, which is what says this
/// harness works and the test above is about age rather than about plumbing.
#[test]
fn a_recent_message_is_answerable_today() {
    let path = tmp("recent-message");
    write_history(&path, 5_000);

    let reopened: FileJournal<8, 512> =
        FileJournal::open(&path, Durability::Async).expect("reopen");
    let found = reopened.get(5_000).map(<[u8]>::to_vec).unwrap_or_default();

    let _ = std::fs::remove_file(&path);
    assert_eq!(found, body(5_000), "the last one is in the ring");
}

/// Every record comes back, in order, with its content intact — not merely the
/// right *number* of them.
#[test]
fn the_whole_history_reads_back_in_order() {
    let path = tmp("in-order");
    write_history(&path, 1_000);

    let reader = Reader::open(&path).expect("read");
    let messages: Vec<(u32, Vec<u8>)> = reader
        .records()
        .filter_map(|r| match r {
            Record::Message { seq, bytes } => Some((seq, bytes.to_vec())),
            Record::InboundMark { .. } => None,
        })
        .collect();
    let _ = std::fs::remove_file(&path);

    assert_eq!(messages.len(), 1_000, "one per message written");
    for (i, (seq, bytes)) in messages.iter().enumerate() {
        let want = u32::try_from(i).expect("fits") + 1;
        assert_eq!(*seq, want, "in the order they were written");
        assert_eq!(bytes, &body(want), "and with the bytes that went out");
    }
}

/// **ADR-0017's inbound mark is a record, not an empty message.** A reader that
/// treated `len == 0` as a zero-byte message would lose the inbound count
/// entirely, and that count is the one the file exists to carry beside the
/// messages.
#[test]
fn an_inbound_mark_is_read_as_a_mark() {
    let path = tmp("inbound-mark");
    write_history(&path, 10);

    let reader = Reader::open(&path).expect("read");
    let marks: Vec<u32> = reader
        .records()
        .filter_map(|r| match r {
            Record::InboundMark { seq } => Some(seq),
            Record::Message { .. } => None,
        })
        .collect();
    let empties = reader
        .records()
        .filter(|r| matches!(r, Record::Message { bytes, .. } if bytes.is_empty()))
        .count();
    let _ = std::fs::remove_file(&path);

    assert_eq!(marks, vec![11], "the mark written by write_history");
    assert_eq!(
        empties, 0,
        "and it must not read back as a zero-byte message"
    );
}

/// **A torn tail is reported, not swallowed** — and the tail is made by
/// truncating a **real** file rather than by inventing bytes, because a parser
/// proved against bytes nobody writes is proved against nothing.
#[test]
fn a_torn_tail_is_counted_rather_than_hidden() {
    let path = tmp("torn");
    write_history(&path, 100);

    let whole = std::fs::metadata(&path).expect("stat").len();
    let clean = Reader::open(&path).expect("read");
    let clean_records = clean.records().count();
    assert_eq!(
        clean.torn_tail_bytes(),
        0,
        "the premise: a file written and closed cleanly is whole"
    );

    // Cut 7 bytes off: enough to leave a header claiming more than is there.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open for truncate");
    file.set_len(whole - 7).expect("truncate");
    drop(file);

    let cut = Reader::open(&path).expect("read the cut file");
    let cut_records = cut.records().count();
    let _ = std::fs::remove_file(&path);

    assert!(
        cut.torn_tail_bytes() > 0,
        "a process killed mid-write leaves a tail, and it must be reported"
    );
    assert_eq!(
        cut_records,
        clean_records - 1,
        "exactly the torn record is missing — the ones before it are intact"
    );
}

/// The same fact, from the engine's own side: `FileJournal` opening a torn file
/// **says so**, where it used to compute the number and throw it away.
#[test]
fn a_file_journal_reports_the_tail_it_could_not_use() {
    let path = tmp("journal-torn");
    write_history(&path, 100);
    let whole = std::fs::metadata(&path).expect("stat").len();

    let clean: FileJournal<8, 512> = FileJournal::open(&path, Durability::Async).expect("open");
    assert_eq!(clean.torn_tail_bytes(), 0, "the premise");
    drop(clean);

    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open for truncate");
    file.set_len(whole - 7).expect("truncate");
    drop(file);

    let torn: FileJournal<8, 512> = FileJournal::open(&path, Durability::Async).expect("reopen");
    let n = torn.torn_tail_bytes();
    drop(torn);
    let _ = std::fs::remove_file(&path);

    assert!(
        n > 0,
        "the engine skipped bytes on recovery and must not do it silently"
    );
}

/// An empty or absent file is not an error and not a torn tail — it is a
/// session that has sent nothing.
#[test]
fn an_empty_file_reads_as_nothing_rather_than_as_damage() {
    let path = tmp("empty");
    {
        let j: FileJournal<8, 512> = FileJournal::open(&path, Durability::Async).expect("open");
        drop(j);
    }
    let reader = Reader::open(&path).expect("read");
    let n = reader.records().count();
    let torn = reader.torn_tail_bytes();
    let empty = reader.is_empty();
    let _ = std::fs::remove_file(&path);

    assert_eq!(n, 0);
    assert_eq!(torn, 0, "nothing written is not the same as damage");
    assert!(empty);
}
