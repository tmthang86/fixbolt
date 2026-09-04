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

// ---------------------------------------------------------------------------
// The hooks, driven against a real `Connection`.
//
// **These exist because their absence was not noticed.** The outbound hook went
// in with the plumbing and the inbound one did not, and the commit that shipped
// them said both were there. Nothing read the code and nothing ran it, because
// every step-2 test had been deferred to a later commit — so "the hook is
// present" was a claim with no observer, which `CLAUDE.md` §10 says is not a
// result. One test per hook, and each one fails if its hook is removed.
// ---------------------------------------------------------------------------

use std::collections::VecDeque;

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::conn::Connection;
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::{Io, Transport};
use fixbolt_session::{Acceptor, Config, Session, Silent};

/// A socket that hands over whatever it was primed with and swallows the rest.
#[derive(Default)]
struct Wire {
    inbox: VecDeque<u8>,
    sent: Vec<u8>,
}

impl Transport for Wire {
    fn recv(&mut self, buf: &mut [u8]) -> Io {
        let n = buf.len().min(self.inbox.len());
        if n == 0 {
            return Io::Idle;
        }
        for slot in buf.iter_mut().take(n) {
            *slot = self.inbox.pop_front().unwrap_or(0);
        }
        Io::Ready(n)
    }

    fn send(&mut self, buf: &[u8]) -> Io {
        self.sent.extend_from_slice(buf);
        Io::Ready(buf.len())
    }
}

type Conn = Connection<Wire, Acceptor, Store, 256, 4096, 8192>;

/// A whole message with a real body length and checksum, at the fixed instant
/// the corpus uses — a `52=` more than 120 seconds away is refused for skew,
/// and then there is nothing outbound to assert about.
fn wire(msg_type: &str, seq: u32, body: &str) -> Vec<u8> {
    let body = format!(
        "35={msg_type}\x0134={seq}\x0149=TW44\x0152=20260828-12:00:00.000\x0156=ISLD\x01{body}"
    );
    with_real_checksum(format!("8=FIX.4.4\x019={}\x01{body}10=0\x01", body.len()).as_bytes())
}

fn logon() -> Vec<u8> {
    wire("A", 1, "98=0\x01108=30\x01")
}

fn wired(inbox: &[u8]) -> Conn {
    let transport = Wire {
        inbox: inbox.iter().copied().collect(),
        sent: Vec::new(),
    };
    Connection::new(
        1,
        transport,
        Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
        Store::new(),
    )
}

/// A logon arrives and is answered. Both halves are in the file.
///
/// **The inbound half is the one that was missing.** Deleting the `record` call
/// before `refuse` leaves this red on the `IN` count while every other test in
/// this file and all 475 others stay green.
#[test]
fn a_connection_logs_what_it_read_and_what_it_answered() {
    let tmp = Tmp::new("both-ways");
    let logon = logon();

    let mut log = FileLog::open(tmp.path()).unwrap();
    let mut c = wired(&logon);
    c.opened(FIXED_TIME_MILLIS, &mut log);
    let _ = c.turn(FIXED_TIME_MILLIS, &mut Silent, |_| false, 0, &mut log);
    log.close();

    let lines = tmp.lines();
    let ins = lines.iter().filter(|l| l.contains(" IN  ")).count();
    let outs = lines.iter().filter(|l| l.contains(" OUT ")).count();
    assert_eq!(
        ins, 1,
        "the logon that arrived is not in the log: {lines:?}"
    );
    assert_eq!(outs, 1, "the logon that went back is not either: {lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains(" IN  ") && l.contains("35=A")),
        "the inbound line is not the message that arrived: {lines:?}"
    );
}

/// A frame the session never sees is still in the log.
///
/// This is the plan's headline case: `refuse` returning `true` ends the
/// connection **without a reply**, so a log written after the session would
/// hold nothing at all about the most disputed message there is.
#[test]
fn a_refused_frame_is_in_the_log_even_though_the_session_never_saw_it() {
    let tmp = Tmp::new("refused");
    let logon = logon();

    let mut log = FileLog::open(tmp.path()).unwrap();
    let mut c = wired(&logon);
    c.opened(FIXED_TIME_MILLIS, &mut log);
    // `true` is the engine's own rule, ADR-0030: a second Logon on a second
    // connection is dropped in silence.
    let _ = c.turn(FIXED_TIME_MILLIS, &mut Silent, |_| true, 0, &mut log);
    log.close();

    let lines = tmp.lines();
    assert_eq!(
        lines.iter().filter(|l| l.contains(" IN  ")).count(),
        1,
        "the refused frame vanished, which is the whole defect: {lines:?}"
    );
    assert_eq!(
        lines.iter().filter(|l| l.contains(" OUT ")).count(),
        0,
        "a refusal is silent on the wire, and the log must not invent a reply"
    );
}

/// `shard` reaches the line from the engine, not from the connection's guess.
#[test]
fn the_shard_the_engine_names_is_the_shard_on_the_line() {
    let tmp = Tmp::new("shard-through");
    let logon = logon();

    let mut log = FileLog::open(tmp.path()).unwrap();
    let mut c = wired(&logon);
    let _ = c.turn(FIXED_TIME_MILLIS, &mut Silent, |_| false, 5, &mut log);
    log.close();

    let lines = tmp.lines();
    assert!(!lines.is_empty(), "nothing was logged at all");
    assert!(
        lines.iter().all(|l| l.contains("shard=5")),
        "left: {lines:?}"
    );
}

/// A socket that queues, then dies.
///
/// `Io::Idle` first, so bytes sit in `tx` and the log has already written the
/// `OUT` line for them; then `Io::Closed`, which is a dying counterparty and
/// makes the engine discard everything still queued.
#[derive(Default)]
struct Dying {
    inbox: VecDeque<u8>,
    dead: bool,
}

impl Transport for Dying {
    fn recv(&mut self, buf: &mut [u8]) -> Io {
        let n = buf.len().min(self.inbox.len());
        if n == 0 {
            return Io::Idle;
        }
        for slot in buf.iter_mut().take(n) {
            *slot = self.inbox.pop_front().unwrap_or(0);
        }
        Io::Ready(n)
    }

    fn send(&mut self, _buf: &[u8]) -> Io {
        if self.dead { Io::Closed } else { Io::Idle }
    }
}

/// `OUT` means queued, and what was queued and never sent is counted.
///
/// **The log errs in the worst direction without this.** `Out::push` writes the
/// `OUT` line when the bytes reach `tx`, and `conn.rs` discards whatever is
/// still in `tx` when the socket dies — so the file claims a send that never
/// left the machine, which is precisely the claim a counterparty disputes.
/// Nothing can un-write the line; what the engine can do is say how many bytes
/// the tail of the file is wrong about.
#[test]
fn bytes_still_queued_when_the_socket_dies_are_counted_not_claimed_as_sent() {
    let tmp = Tmp::new("unsent");
    let mut log = FileLog::open(tmp.path()).unwrap();
    let mut c: Connection<Dying, Acceptor, Store, 256, 4096, 8192> = Connection::new(
        1,
        Dying {
            inbox: logon().iter().copied().collect(),
            dead: false,
        },
        Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
        Store::new(),
    );

    // The engine tells the session the link is up before anything is judged;
    // without it an acceptor ignores what arrives, and the test would be
    // measuring a session that was never opened.
    c.opened(FIXED_TIME_MILLIS, &mut log);
    // The logon is read and answered; the answer cannot go out, so it stays in
    // `tx` — and the log has already called it `OUT`.
    let _ = c.turn(FIXED_TIME_MILLIS, &mut Silent, |_| false, 0, &mut log);
    assert!(
        c.unsent_bytes() == 0,
        "nothing has been discarded yet: {}",
        c.unsent_bytes()
    );

    c.transport.dead = true;
    let _ = c.turn(FIXED_TIME_MILLIS, &mut Silent, |_| false, 0, &mut log);
    log.close();

    let outs = tmp.lines().iter().filter(|l| l.contains(" OUT ")).count();
    assert_eq!(outs, 1, "the log claimed a send: {:?}", tmp.lines());
    assert!(
        c.unsent_bytes() > 0,
        "and nothing counted what never left: {}",
        c.unsent_bytes()
    );
}

/// A socket that takes nothing and then hangs up, on its own schedule.
///
/// The engine owns its connections and hands out no mutable reference to one,
/// which is right — a test that reached in could hold a connection in a state
/// the engine never produces. So the dying is the transport's own business:
/// `Idle` on the first `send`, so the answer sits in `tx` with an `OUT` line
/// already written for it, and `Closed` after.
#[derive(Default)]
struct DiesAfterOneSend {
    inbox: VecDeque<u8>,
    sends: usize,
}

impl Transport for DiesAfterOneSend {
    fn recv(&mut self, buf: &mut [u8]) -> Io {
        let n = buf.len().min(self.inbox.len());
        if n == 0 {
            return Io::Idle;
        }
        for slot in buf.iter_mut().take(n) {
            *slot = self.inbox.pop_front().unwrap_or(0);
        }
        Io::Ready(n)
    }

    fn send(&mut self, _buf: &[u8]) -> Io {
        self.sends += 1;
        if self.sends > 1 { Io::Closed } else { Io::Idle }
    }
}

/// The discarded bytes reach an operator, not just a field on a connection.
///
/// **Written because the last hook shipped without one.** A count nothing reads
/// is the same as no count, and `EventKind::MessageLogUnsent` is only useful if
/// it actually arrives in the event stream. Driven against a real `Engine` with
/// a transport that queues and then dies, so the whole path is exercised: the
/// connection sets the number, the engine reads it on `Turn::Gone`, and the
/// observer sees it before the connection is dropped.
#[test]
fn what_the_log_promised_and_the_socket_never_took_reaches_the_observer() {
    use fixbolt_engine::clock::ManualClock;
    use fixbolt_engine::dispatch::InlineDispatch;
    use fixbolt_engine::observe::EventKind;
    use fixbolt_engine::wait::Yield;

    let tmp = Tmp::new("unsent-event");
    let log = FileLog::open(tmp.path()).unwrap();
    // Annotated before `with_log`, not after: `Engine::new` builds its own `L`
    // from `Default`, so naming only the final type leaves `new`'s parameter
    // ambiguous. Written out as the two steps it is.
    type Bare = fixbolt_engine::Engine<
        DiesAfterOneSend,
        Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Yield,
        Store,
        256,
        4096,
        8192,
    >;
    let bare: Bare = fixbolt_engine::Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let mut engine = bare.with_log(log);

    let observer = engine.observer();
    engine.add(DiesAfterOneSend {
        inbox: logon().iter().copied().collect(),
        sends: 0,
    });

    // The logon is answered into a queue the socket will not take.
    engine.turn();
    // And on the next turn the socket hangs up with that answer still in it.
    engine.turn();

    let mut events = Vec::new();
    let _ = observer.events(&mut events);
    let unsent: Vec<usize> = events
        .iter()
        .filter_map(|e| match e.kind() {
            EventKind::MessageLogUnsent { bytes } => Some(bytes),
            _ => None,
        })
        .collect();
    assert_eq!(
        unsent.len(),
        1,
        "an operator must be told the log's tail is wrong: {events:?}"
    );
    assert!(unsent[0] > 0, "and by how much: {unsent:?}");
}

/// One file per shard, named from the path the operator gave.
#[test]
fn each_shard_gets_its_own_file_beside_the_one_that_was_named() {
    use fixbolt_engine::msglog::shard_path;
    let base = std::path::Path::new("/var/log/fixbolt/messages.log");
    assert_eq!(
        shard_path(base, 0),
        std::path::PathBuf::from("/var/log/fixbolt/messages.log.0")
    );
    assert_eq!(
        shard_path(base, 11),
        std::path::PathBuf::from("/var/log/fixbolt/messages.log.11")
    );
    assert_ne!(
        shard_path(base, 0),
        shard_path(base, 1),
        "two shards sharing a path is the defect this exists to prevent"
    );
}

/// Two shards, two connections that are both `conn=0`, and no ambiguity.
///
/// **This is the collision the review found.** `ConnId` restarts at zero in
/// every engine, so a sharded acceptor writing one file has two `conn=0` rows
/// for two different counterparties, and nothing in the line to tell them
/// apart. Two files, and `shard=` on every line, are the two halves of the fix
/// — and either one alone leaves an operator guessing.
#[test]
fn two_shards_write_two_files_and_conn_ids_do_not_collide() {
    let a = Tmp::new("shard-a");
    let b = Tmp::new("shard-b");
    let mut log_a = FileLog::open(a.path()).unwrap();
    let mut log_b = FileLog::open(b.path()).unwrap();

    // The same connection number on two shards, which is what really happens.
    let mut ca = wired(&logon());
    let mut cb = wired(&logon());
    let _ = ca.turn(FIXED_TIME_MILLIS, &mut Silent, |_| false, 0, &mut log_a);
    let _ = cb.turn(FIXED_TIME_MILLIS, &mut Silent, |_| false, 1, &mut log_b);
    log_a.close();
    log_b.close();

    let (la, lb) = (a.lines(), b.lines());
    assert!(!la.is_empty() && !lb.is_empty(), "both files have lines");
    assert!(
        la.iter().all(|l| l.contains("shard=0 conn=1")),
        "left: {la:?}"
    );
    assert!(
        lb.iter().all(|l| l.contains("shard=1 conn=1")),
        "left: {lb:?}"
    );
    // Concatenated, the way an operator would to see one timeline, the rows
    // are still distinguishable — which is the whole point of `shard=`.
    let both: Vec<&String> = la.iter().chain(lb.iter()).collect();
    assert_eq!(
        both.iter().filter(|l| l.contains("shard=0")).count(),
        la.len()
    );
    assert_eq!(
        both.iter().filter(|l| l.contains("shard=1")).count(),
        lb.len()
    );
}
