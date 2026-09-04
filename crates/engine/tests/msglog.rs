//! The two-directional message log. `docs/plans/2026-09-03-message-log.md` step 1.
//!
//! **The file is the assertion.** Every test here writes through the real
//! `record` path, closes the log so the writer has finished, and then reads the
//! file back with the same tools an operator has: line counts, prefixes, and
//! the bytes between the separators. Nothing asserts against an internal
//! buffer, because an internal buffer is not what somebody opens during a
//! dispute.
//!
//! Four of these exist because a gate that cannot fail is not a gate:
//!
//! * `record_touches_no_file_until_the_writer_runs` is the **direct** reversal
//!   for "record wrote straight to the file instead of pushing to the ring".
//!   The allocation bench cannot see a syscall and never will.
//! * `a_record_longer_than_the_writer_buffer_is_counted_not_silently_dropped`
//!   exists because `ring::Consumer::pop` drops an oversized record and returns
//!   `Some(0)`, which a producer-side counter cannot see.
//! * `a_torn_last_line_is_marked_not_merged_with_the_next` is the `kill -9`
//!   case: without it, two messages become one line that `grep` reads as one.
//! * `dropping_a_file_log_without_close_still_writes_what_was_queued` is why
//!   `close` takes `&mut self` — a by-value `close` cannot be called from
//!   `Drop`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write as _;
use std::path::PathBuf;

use fixbolt_engine::msglog::{Direction, FileLog, MessageLog};

/// A scratch path that cleans up after itself.
struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "fixbolt-msglog-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&p);
        Self(p)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn read(&self) -> String {
        std::fs::read_to_string(&self.0).unwrap()
    }

    fn lines(&self) -> Vec<String> {
        self.read().lines().map(str::to_owned).collect()
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// One logon, in and out, as an operator would read it back.
///
/// `at_ms` is milliseconds since year zero — D13's clock, the one `turn`
/// already has — so the formatted line must show 2026, not 1970 and not 0.
#[test]
fn a_record_becomes_one_line_with_direction_time_and_connection() {
    let tmp = Tmp::new("one-line");
    // 2026-09-03T10:32:07.120Z, in D13 milliseconds.
    let at = 1_788_431_527_120 + fixbolt_engine::msglog::MILLIS_YEAR_ZERO_TO_EPOCH;

    let mut log = FileLog::open(tmp.path()).unwrap();
    log.record(
        Direction::In,
        at,
        0,
        1,
        b"8=FIX.4.4\x019=12\x0135=A\x0110=000\x01",
    );
    log.record(
        Direction::Out,
        at,
        0,
        1,
        b"8=FIX.4.4\x019=12\x0135=A\x0110=001\x01",
    );
    log.close();

    let lines = tmp.lines();
    assert_eq!(lines.len(), 2, "one line per message, got {lines:?}");
    assert!(
        lines[0].starts_with("20260903-10:32:07.120 IN  shard=0 conn=1 "),
        "left: {:?}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("20260903-10:32:07.120 OUT shard=0 conn=1 "),
        "left: {:?}",
        lines[1]
    );
    assert!(
        lines[0].ends_with("10=000\u{1}"),
        "SOH is kept: {:?}",
        lines[0]
    );
}

/// `shard=` is always present, so one `awk` field layout reads every file.
#[test]
fn a_sharded_line_names_its_shard_and_both_are_readable_by_one_reader() {
    let tmp = Tmp::new("shard");
    let mut log = FileLog::open(tmp.path()).unwrap();
    log.record(Direction::In, msg_time(), 3, 0, b"8=FIX.4.4\x0135=0\x01");
    log.close();

    let lines = tmp.lines();
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains(" shard=3 conn=0 "),
        "left: {:?}",
        lines[0]
    );
}

/// A DATA field may legally carry `0x0A`. One message must stay one line.
#[test]
fn a_data_field_with_a_newline_stays_on_one_line() {
    let tmp = Tmp::new("newline");
    let mut log = FileLog::open(tmp.path()).unwrap();
    log.record(
        Direction::In,
        msg_time(),
        0,
        1,
        b"8=FIX.4.4\x0195=3\x0196=a\nb\x0110=000\x01",
    );
    log.record(
        Direction::In,
        msg_time(),
        0,
        1,
        b"8=FIX.4.4\x0195=3\x0196=a\rb\x0110=000\x01",
    );
    log.close();

    let lines = tmp.lines();
    assert_eq!(lines.len(), 2, "two messages, two lines, got {lines:?}");
    assert!(lines[0].contains("96=a\\nb"), "left: {:?}", lines[0]);
    assert!(lines[1].contains("96=a\\rb"), "left: {:?}", lines[1]);
}

/// The escape has to be reversible, or the line is not a record of anything.
///
/// **A DATA field containing the two bytes `\` `n` is the trap.** With only two
/// escape rules it is indistinguishable in the file from a real newline that
/// was escaped, and the log stops being evidence.
#[test]
fn a_backslash_in_a_data_field_round_trips() {
    let tmp = Tmp::new("backslash");
    let raw: &[u8] = b"8=FIX.4.4\x0196=a\\nb\x0196=c\nd\x0110=000\x01";
    let mut log = FileLog::open(tmp.path()).unwrap();
    log.record(Direction::In, msg_time(), 0, 1, raw);
    log.close();

    let lines = tmp.lines();
    assert_eq!(lines.len(), 1, "one message, one line, got {lines:?}");
    let payload = lines[0].split_once("conn=1 ").unwrap().1;
    assert_eq!(
        fixbolt_engine::msglog::unescape(payload),
        raw,
        "the escaped line must decode back to the exact bytes that arrived"
    );
}

/// A full ring drops and counts. It never waits for the disk.
///
/// The log is not allowed to be the reason a session stalls: ADR-0011 says the
/// same thing for the dispatch ring, and this is the same rule pointed the
/// other way. `deferred` hands the consumer back instead of spawning a writer,
/// so nothing drains and the ring genuinely fills.
#[test]
fn a_full_ring_drops_and_counts_and_never_blocks() {
    let tmp = Tmp::new("full-ring");
    let (mut log, _held) = FileLog::deferred(tmp.path(), 256).unwrap();

    let started = std::time::Instant::now();
    for i in 0..100u64 {
        log.record(
            Direction::In,
            msg_time(),
            0,
            i,
            b"8=FIX.4.4\x0135=0\x0110=000\x01",
        );
    }
    let spent = started.elapsed();

    assert!(
        log.lost() > 0,
        "a 256-byte ring cannot hold 100 records; lost() is {}",
        log.lost()
    );
    assert!(
        log.lost() < 100,
        "it should hold at least one; lost() is {}",
        log.lost()
    );
    assert!(
        spent < std::time::Duration::from_millis(100),
        "record() waited on something: {spent:?}"
    );
}

/// **The direct reversal for "wrote straight to the file".**
///
/// With no writer running, a correct `record` has touched nothing on disk. An
/// implementation that calls `write_all` from the engine thread fails here on
/// the file's own length — which is what the allocation bench cannot see,
/// because a syscall allocates nothing.
#[test]
fn record_touches_no_file_until_the_writer_runs() {
    let tmp = Tmp::new("no-syscall");
    let (mut log, _held) = FileLog::deferred(tmp.path(), 1 << 16).unwrap();

    log.record(
        Direction::Out,
        msg_time(),
        0,
        1,
        b"8=FIX.4.4\x0135=A\x0110=000\x01",
    );

    let len = std::fs::metadata(tmp.path()).unwrap().len();
    assert_eq!(len, 0, "record() reached the disk on the engine thread");
    assert_eq!(log.lost(), 0, "and it was not dropped either");
}

/// Loss on the consumer's side is loss too.
///
/// `ring::Consumer::pop` **drops** a record longer than the buffer it is given
/// and returns `Some(0)` so the queue can move on. A counter that only sees
/// `push` returning `false` reads zero while messages disappear, so the writer
/// counts `Some(0)` as well — and its buffer is sized from `RX`, not from a
/// number somebody typed.
#[test]
fn a_record_longer_than_the_writer_buffer_is_counted_not_silently_dropped() {
    let tmp = Tmp::new("oversize");
    let mut log = FileLog::open(tmp.path()).unwrap();

    // One byte past what the writer can hold, so `pop` refuses it.
    let huge = vec![b'x'; FileLog::MAX_RECORD + 1];
    log.record(Direction::In, msg_time(), 0, 1, &huge);
    log.record(Direction::In, msg_time(), 0, 1, b"8=FIX.4.4\x0135=0\x01");
    log.close();

    assert_eq!(tmp.lines().len(), 1, "the short one still got through");
    assert_eq!(log.lost(), 1, "and the long one was counted, not forgotten");
}

/// A `kill -9` mid-write leaves half a line. The next line must not join it.
///
/// This is `FileJournal`'s torn-tail accounting (`journal.rs:396-424`) done for
/// a text file, where it is cheaper: the last byte either is a newline or the
/// file is torn.
#[test]
fn a_torn_last_line_is_marked_not_merged_with_the_next() {
    let tmp = Tmp::new("torn");
    {
        let mut f = std::fs::File::create(tmp.path()).unwrap();
        f.write_all(b"20260903-10:32:07.120 IN  shard=0 conn=1 8=FIX.4.4\x0135=A")
            .unwrap();
    }

    let mut log = FileLog::open(tmp.path()).unwrap();
    assert!(log.torn_tail_bytes() > 0, "the tear was not noticed");
    log.record(Direction::In, msg_time(), 0, 2, b"8=FIX.4.4\x0135=0\x01");
    log.close();

    let lines = tmp.lines();
    assert_eq!(
        lines.len(),
        3,
        "the torn line, a marker, and the new one — got {lines:?}"
    );
    assert!(
        lines[0].ends_with("35=A"),
        "the torn bytes are kept as they were"
    );
    assert!(lines[1].starts_with("# torn tail"), "left: {:?}", lines[1]);
    assert!(lines[2].contains("conn=2"), "the new line stands alone");
}

/// A clean file is not reported as torn, or the counter means nothing.
#[test]
fn a_file_that_ends_in_a_newline_is_not_torn() {
    let tmp = Tmp::new("not-torn");
    {
        let mut log = FileLog::open(tmp.path()).unwrap();
        log.record(Direction::In, msg_time(), 0, 1, b"8=FIX.4.4\x0135=0\x01");
        log.close();
    }
    let log = FileLog::open(tmp.path()).unwrap();
    assert_eq!(log.torn_tail_bytes(), 0);
    assert_eq!(
        tmp.lines().len(),
        1,
        "and nothing was appended on reopening"
    );
}

/// Forgetting `close` must not leave the writer running.
///
/// `FileJournal::close` takes `&mut self` precisely so `Drop` can call it
/// (`journal.rs:486`, `:501`). A by-value `close(self)` cannot be, and then a
/// process that ends normally without calling it leaks the writer thread and
/// loses whatever it had not reached at exit.
///
/// **The obvious version of this test is a false green, and it was written
/// first.** Asserting only that the line reached the file passes with `Drop`
/// deleted: the writer drains and flushes in the background either way, so the
/// assertion measures a race that it usually wins. What only `close` can do is
/// *end the writer*, and the shared counter makes that observable — the writer
/// owns a clone of it, so a strong count of one means the thread has returned.
#[test]
fn dropping_a_file_log_without_close_still_writes_what_was_queued() {
    let tmp = Tmp::new("drop");
    let held;
    {
        let mut log = FileLog::open(tmp.path()).unwrap();
        held = log.counter();
        assert_eq!(
            std::sync::Arc::strong_count(&held),
            3,
            "the log, the writer, and this test each hold one"
        );
        log.record(Direction::In, msg_time(), 0, 1, b"8=FIX.4.4\x0135=0\x01");
        // No `close()`. `Drop` is the whole point.
    }
    assert_eq!(
        std::sync::Arc::strong_count(&held),
        1,
        "the writer thread outlived the log, so `close` was never called"
    );
    assert_eq!(
        tmp.lines().len(),
        1,
        "the queued record never reached the file"
    );
}

/// An `Open` record binds `conn=` to a peer, so a line answers "who".
///
/// `ConnId` is a per-process counter that restarts at zero, and a garbage frame
/// may carry no `49=`/`56=` at all. The address is pushed **once per
/// connection**, not once per message: the engine thread must not pay to copy
/// it every time.
#[test]
fn an_open_record_binds_a_connection_to_a_peer_address() {
    let tmp = Tmp::new("peer");
    let mut log = FileLog::open(tmp.path()).unwrap();
    log.record(Direction::Open, msg_time(), 0, 7, b"10.4.2.9:51422");
    log.record(Direction::In, msg_time(), 0, 7, b"8=FIX.4.4\x0135=0\x01");
    log.close();

    let lines = tmp.lines();
    assert_eq!(
        lines.len(),
        2,
        "a comment line and a message line: {lines:?}"
    );
    assert!(lines[0].starts_with('#'), "left: {:?}", lines[0]);
    assert!(lines[0].contains("conn=7"), "left: {:?}", lines[0]);
    assert!(
        lines[0].contains("peer=10.4.2.9:51422"),
        "left: {:?}",
        lines[0]
    );
    assert!(
        lines[1].contains("peer=10.4.2.9:51422"),
        "every message line carries it too: {:?}",
        lines[1]
    );
}

/// `NoLog` records nothing and loses nothing — the default must be free.
#[test]
fn no_log_holds_nothing_and_reports_nothing() {
    let mut log = fixbolt_engine::msglog::NoLog;
    log.record(Direction::In, msg_time(), 0, 1, b"8=FIX.4.4\x01");
    assert_eq!(log.lost(), 0);
    const {
        assert!(
            !<fixbolt_engine::msglog::NoLog as MessageLog>::LOGS,
            "LOGS is what lets a call site fold the whole hook away"
        );
    }
}

/// A time that formats to something a human recognises, for tests that do not
/// care which instant it is.
fn msg_time() -> u64 {
    1_788_431_527_120 + fixbolt_engine::msglog::MILLIS_YEAR_ZERO_TO_EPOCH
}
