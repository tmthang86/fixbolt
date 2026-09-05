//! A journal on disk, reached through the serving loop, remembering when the
//! session was last alive.
//!
//! **Step 1 of [recovery-reaches-the-disk], red at an assertion.**
//!
//! # The two halves, and why they are one test
//!
//! `STATUS.md` item 32 (b) and (c). Persisting *when a session was last alive*
//! into a `FileJournal` means nothing while the serving loop cannot use a
//! `FileJournal`; a `FileJournal` reachable through the serving loop still
//! cannot answer the boundary question if it does not remember the instant. So
//! the specification asserts both at once, on the file, after a restart.
//!
//! # Why `Async` rather than `Fsync`
//!
//! `FileJournal` joins its writer thread on drop, which is the settle point.
//! `Fsync` is the strong knob and it is not the settle point —
//! `reference/the-strongest-knob-is-not-the-settle-point.md`.
//!
//! [recovery-reaches-the-disk]: ../../../docs/plans/2026-09-02-recovery-reaches-the-disk.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use fixbolt_engine::journal::{Durability, FileJournal};
use fixbolt_session::journal::Journal;

const N: usize = 8;
const LEN: usize = 512;

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "fixbolt-ondisk-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

/// **The specification.** A journal on disk remembers when its session was last
/// alive, and the answer survives the process that wrote it.
///
/// `[verified 2026-09-02]` nothing records the instant at all: `Journal` has
/// `put`, `get`, `highest`, `mark_in` and `highest_in`, and **no notion of
/// time**. The engine hands `Session::last_active_ms()` to `Resumed` and
/// nothing on disk holds it, so a restart across a trading-day boundary cannot
/// know a boundary was crossed.
///
/// # Why this asserts on the file's bytes
///
/// There is no API to ask, so asking through one would be red at the compiler
/// — which says only that the function has not been written, not what the
/// system does today. **The file is the thing that has to carry the instant**,
/// so the file is what is examined: after everything a session can record
/// today, are those eight bytes anywhere in it?
#[test]
fn a_journal_on_disk_remembers_when_its_session_was_last_alive() {
    let path = tmp("last-active");
    const WAS_ALIVE_AT: u64 = 63_849_600_000_000;

    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
        j.put(1, b"8=FIX.4.4\x0135=D\x0134=1\x01");
        j.mark_in(2);
        j.mark_active(WAS_ALIVE_AT);
    }

    // Read back through the API as well, which is what a `Recovery` would do.
    let reopened: FileJournal<N, LEN> =
        FileJournal::open(&path, Durability::Async).expect("reopen");
    let through_the_api = reopened.last_active();
    drop(reopened);

    let bytes = std::fs::read(&path).expect("the file is there");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        through_the_api,
        Some(WAS_ALIVE_AT),
        "and a Recovery must be able to ask for it without parsing the file"
    );
    let wanted = WAS_ALIVE_AT.to_le_bytes();
    assert!(
        bytes.windows(wanted.len()).any(|w| w == wanted),
        "the instant this session was last alive must be in the file — without \
         it a restart cannot tell whether a trading day ended while the process \
         was down, and ADR-0033's boundary reset has nothing to compare against"
    );
}

/// The control: the numbers this journal has always carried **do** survive, so
/// the assertion above is about the instant rather than about the file.
#[test]
fn the_sequence_numbers_already_survive_a_restart() {
    let path = tmp("numbers-survive");
    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
        j.put(1, b"8=FIX.4.4\x0135=D\x0134=1\x01");
        j.mark_in(9);
    }
    let reopened: FileJournal<N, LEN> =
        FileJournal::open(&path, Durability::Async).expect("reopen");
    let (highest, highest_in) = (reopened.highest(), reopened.highest_in());
    drop(reopened);
    let _ = std::fs::remove_file(&path);

    assert_eq!(highest, Some(1), "the premise");
    assert_eq!(highest_in, Some(9), "the premise");
}

/// **The outbound count survives a restart too, and it is not the same number
/// as `highest`.** A session that sends one order and then three
/// administrative messages has spent `34=4` and journalled `34=1`; a restart
/// deriving `next_out` from `highest` starts at 2 and is refused by the
/// counterparty.
///
/// `STATUS.md` item 48, ADR-0053.
#[test]
fn the_outbound_count_survives_a_restart_and_is_not_highest() {
    let path = tmp("outbound-count-survives");
    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
        j.put(1, b"8=FIX.4.4\x0135=D\x0134=1\x01");
        // Three administrative messages: bytes nowhere, numbers spent.
        j.mark_out(4);
    }
    let reopened: FileJournal<N, LEN> =
        FileJournal::open(&path, Durability::Async).expect("reopen");
    let (highest, highest_out) = (reopened.highest(), reopened.highest_out());
    drop(reopened);
    let _ = std::fs::remove_file(&path);

    assert_eq!(highest, Some(1), "one message is all that can be replayed");
    assert_eq!(
        highest_out,
        Some(4),
        "and four numbers were spent — the difference is the whole item"
    );
}

/// The same under `Durability::Fsync`, which takes the other branch of every
/// write in this type.
#[test]
fn the_outbound_count_survives_a_restart_under_fsync_too() {
    let path = tmp("outbound-count-fsync");
    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Fsync).expect("open");
        j.put(1, b"8=FIX.4.4\x0135=D\x0134=1\x01");
        j.mark_out(4);
    }
    let reopened: FileJournal<N, LEN> =
        FileJournal::open(&path, Durability::Fsync).expect("reopen");
    let highest_out = reopened.highest_out();
    drop(reopened);
    let _ = std::fs::remove_file(&path);

    assert_eq!(highest_out, Some(4));
}

/// **A kept message writes no outbound mark**, because `put` already raised the
/// count. Without this the file would carry a second record per application
/// message for a fact it already holds — the trap the plan named.
#[test]
fn a_kept_message_does_not_also_write_an_outbound_mark() {
    use fixbolt_engine::journal::{Reader, Record};

    let path = tmp("no-double-write");
    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
        for seq in 1..=10 {
            j.put(
                seq,
                format!("8=FIX.4.4\u{1}35=D\u{1}34={seq}\u{1}").as_bytes(),
            );
            // Exactly what the session does after every application message.
            j.mark_out(seq);
        }
    }
    let reader = Reader::open(&path).expect("read");
    let (messages, out_marks) = reader
        .records()
        .fold((0usize, 0usize), |(m, o), r| match r {
            Record::Message { .. } => (m + 1, o),
            Record::OutboundMark { .. } => (m, o + 1),
            _ => (m, o),
        });
    let _ = std::fs::remove_file(&path);

    assert_eq!(messages, 10, "ten messages written");
    assert_eq!(
        out_marks, 0,
        "and not one outbound mark, because `put` already said the number was spent"
    );
}

/// **A `put` the journal refuses still spends the number**, and that is exactly
/// when the mark is the only record of it. A reply longer than a slot is the
/// case: legal on the wire, gap-filled on a resend, and invisible to `highest`.
#[test]
fn a_refused_put_still_moves_the_outbound_count() {
    let path = tmp("refused-put");
    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
        j.put(1, b"8=FIX.4.4\x0135=D\x0134=1\x01");
        let too_long = vec![b'x'; LEN + 1];
        assert!(!j.put(2, &too_long), "the premise: it does not fit a slot");
        j.mark_out(2);
    }
    let reopened: FileJournal<N, LEN> =
        FileJournal::open(&path, Durability::Async).expect("reopen");
    let (highest, highest_out) = (reopened.highest(), reopened.highest_out());
    drop(reopened);
    let _ = std::fs::remove_file(&path);

    assert_eq!(highest, Some(1), "the refused message is not held");
    assert_eq!(highest_out, Some(2), "and its number was still spent");
}

/// **`Resumed::from_journal` computes what every example used to compute by
/// hand**, and it is the outbound *count* it reads, not the highest kept
/// message. `STATUS.md` item 48, ADR-0053.
#[test]
fn from_journal_reads_the_count_and_not_the_highest_kept_message() {
    use fixbolt_engine::recovery::Resumed;

    let path = tmp("from-journal");
    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
        j.put(1, b"8=FIX.4.4\x0135=D\x0134=1\x01");
        j.mark_in(6);
        j.mark_out(4);
        j.mark_active(1_700_000_000_000);
    }
    let j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("reopen");
    let r = Resumed::from_journal(j).expect("this journal knows things");
    let (next_out, next_in, last) = (r.next_out, r.next_in, r.last_active_ms);
    drop(r);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        next_out, 5,
        "highest_out is 4, and `highest` would have said 2 — the defect"
    );
    assert_eq!(next_in, 7);
    assert_eq!(last, Some(1_700_000_000_000));
}

/// **A journal that knows nothing answers `None`**, which is the *"start
/// fresh"* answer `Recovery::recover` gives. Without this, a first-ever
/// connection would resume from numbers nobody set.
#[test]
fn from_journal_on_an_empty_journal_says_start_fresh() {
    use fixbolt_engine::recovery::Resumed;

    let path = tmp("from-journal-empty");
    {
        let _j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
    }
    let j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("reopen");
    let none = Resumed::from_journal(j).is_none();
    let _ = std::fs::remove_file(&path);

    assert!(
        none,
        "nothing was left behind, so there is nothing to resume"
    );
}

/// **A file written before outbound marks existed reads exactly as it did**,
/// and is short by exactly as much as it was — not worse, not better. The
/// number was never written and cannot be reconstructed; what must not happen
/// is the reader mistaking the new mark's shape for something else in an old
/// file, or refusing to open one.
#[test]
fn a_file_with_no_outbound_mark_reads_as_it_always_did() {
    let path = tmp("no-outbound-mark");
    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
        for seq in 1..=5 {
            j.put(
                seq,
                format!("8=FIX.4.4\u{1}35=D\u{1}34={seq}\u{1}").as_bytes(),
            );
        }
        j.mark_in(7);
    }
    let reopened: FileJournal<N, LEN> =
        FileJournal::open(&path, Durability::Async).expect("reopen");
    let (highest, highest_in, highest_out) = (
        reopened.highest(),
        reopened.highest_in(),
        reopened.highest_out(),
    );
    drop(reopened);
    let _ = std::fs::remove_file(&path);

    assert_eq!(highest, Some(5), "unchanged");
    assert_eq!(highest_in, Some(7), "unchanged");
    assert_eq!(
        highest_out,
        Some(5),
        "what the kept messages say, which is what this file could ever have said"
    );
}

/// **A file written before activity marks existed still reads exactly as it
/// did.** The format did not change; the reader gained a branch.
///
/// This is asserted rather than assumed because the discriminator is a
/// *sequence number of zero*, and a reader that got the condition wrong would
/// silently reinterpret ordinary records.
#[test]
fn a_file_with_no_activity_mark_reads_as_it_always_did() {
    use fixbolt_engine::journal::{Reader, Record};

    let path = tmp("old-format");
    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
        for seq in 1..=20 {
            j.put(
                seq,
                format!("8=FIX.4.4\u{1}35=D\u{1}34={seq}\u{1}").as_bytes(),
            );
        }
        j.mark_in(21);
    }

    let reader = Reader::open(&path).expect("read");
    let messages = reader
        .records()
        .filter(|r| matches!(r, Record::Message { .. }))
        .count();
    let marks = reader
        .records()
        .filter(|r| matches!(r, Record::InboundMark { .. }))
        .count();
    let activity = reader
        .records()
        .filter(|r| matches!(r, Record::ActivityMark { .. }))
        .count();
    let torn = reader.torn_tail_bytes();

    let reopened: FileJournal<N, LEN> =
        FileJournal::open(&path, Durability::Async).expect("reopen");
    let none = reopened.last_active();
    drop(reopened);
    let _ = std::fs::remove_file(&path);

    assert_eq!(messages, 20, "every message still a message");
    assert_eq!(marks, 1, "the inbound mark still a mark");
    assert_eq!(activity, 0, "and no activity mark invented");
    assert_eq!(torn, 0, "nor read as damage");
    assert_eq!(
        none, None,
        "`None` means this journal does not know — not that the session was \
         never active, which a caller must not confuse"
    );
}

/// The **latest** mark wins. They are appended, so a reader that kept the first
/// would answer with the moment the session started rather than the moment it
/// stopped — which is the wrong end of exactly the question being asked.
#[test]
fn the_latest_activity_mark_is_the_one_that_answers() {
    let path = tmp("latest-wins");
    const AT_LOGON: u64 = 63_849_600_000_000;
    const AT_SHUTDOWN: u64 = AT_LOGON + 8 * 3_600_000;
    {
        let mut j: FileJournal<N, LEN> = FileJournal::open(&path, Durability::Async).expect("open");
        j.mark_active(AT_LOGON);
        j.put(1, b"8=FIX.4.4\x0135=D\x0134=1\x01");
        j.mark_active(AT_SHUTDOWN);
    }
    let reopened: FileJournal<N, LEN> =
        FileJournal::open(&path, Durability::Async).expect("reopen");
    let seen = reopened.last_active();
    drop(reopened);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        seen,
        Some(AT_SHUTDOWN),
        "the moment it stopped, not started"
    );
}

/// An in-memory journal answers `None` and records nothing, and the default
/// implementations are what make that true without every journal having to
/// pretend.
#[test]
fn a_journal_that_does_not_survive_a_restart_does_not_pretend() {
    use fixbolt_engine::journal::MemJournal;

    let mut j: MemJournal<N, LEN> = MemJournal::new();
    j.mark_active(63_849_600_000_000);
    assert_eq!(
        j.last_active(),
        None,
        "a journal that cannot outlive the process has nothing to say about \
         when the process was last alive"
    );
}

/// The serving loop with a journal on disk. **`standard` only**, for the reason
/// `engine_recovery.rs` records: `serve*` builds the blocking engine, which does
/// not exist without that feature, and `cargo test --all --no-default-features`
/// cannot see it because a sibling crate switches the flag back on.
#[cfg(all(feature = "standard", unix))]
mod serving {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::ops::Range;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use fixbolt_engine::journal::{Durability, FileJournal};
    use fixbolt_engine::observe::Handles;
    use fixbolt_engine::presession::{Limits, Table};
    use fixbolt_engine::recovery::{Recovery, Resumed};
    use fixbolt_engine::{Application, Config};

    /// The sizes this test's journal uses. Small on purpose: the ring is not
    /// what is being tested, the **file** is.
    type Disk = FileJournal<8, 512>;

    #[derive(Default)]
    struct EchoApp(fixbolt_conformance::echo::Echo);

    impl Application for EchoApp {
        fn on_message(
            &mut self,
            msg: &[u8],
            seq: u32,
            stamp: &[u8],
            out: &mut [u8],
        ) -> Option<Range<usize>> {
            self.0.reply(msg, seq, stamp, out)
        }
    }

    fn cfg() -> Config {
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
    }

    /// **A `Recovery` that keeps a real file per counterparty.**
    ///
    /// This is the type item 32 (b) existed to make possible. It has no
    /// `Default` and needs none: `fresh` opens the path this deployment has
    /// chosen for that counterparty, which is knowledge only it has.
    struct OnDisk {
        dir: PathBuf,
        opened: Arc<AtomicUsize>,
    }

    impl OnDisk {
        fn path_for(&self, cfg: &Config) -> PathBuf {
            // One file per counterparty. The identity is what names it, which
            // is the whole reason recovery is asked after the registry has
            // chosen — ADR-0034.
            let mut p = self.dir.clone();
            p.push(if cfg.serves(b"TW44", b"ISLD") {
                "TW44.journal"
            } else {
                "other.journal"
            });
            p
        }
    }

    impl Recovery<Disk> for OnDisk {
        fn fresh(&mut self, cfg: &Config) -> Disk {
            self.opened.fetch_add(1, Ordering::Relaxed);
            FileJournal::open(&self.path_for(cfg), Durability::Async)
                .unwrap_or_else(|e| panic!("open journal: {e}"))
        }

        fn recover(&mut self, cfg: &Config) -> Option<Resumed<Disk>> {
            // `from_journal`, which is the arithmetic this example used to do
            // by hand and get wrong — ADR-0053. `None` is the ordinary answer
            // when nothing was left behind, and the journal just opened is
            // handed back through `fresh`.
            Resumed::from_journal(self.fresh(cfg))
        }
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "fixbolt-ondisk-serve-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("scratch dir");
        p
    }

    fn free_port() -> String {
        let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let a = l.local_addr().expect("bound").to_string();
        drop(l);
        a
    }

    fn connect(addr: &str) -> TcpStream {
        for _ in 0..200 {
            if let Ok(s) = TcpStream::connect(addr) {
                s.set_nodelay(true).expect("nodelay");
                s.set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("timeout");
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the serving loop never came up on {addr}");
    }

    /// A Logon stamped now, because the serving loop uses the real clock and
    /// `max_skew_ms` would refuse the corpus's fixed instant.
    fn logon_now(seq: u32) -> Vec<u8> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after 1970")
            .as_millis() as u64;
        let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
        let full = *cache.format(now);
        let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
        let inner = format!(
            "35=A\u{1}34={seq}\u{1}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}"
        );
        let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
        fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
    }

    /// **The specification, end to end.** A deployment using `FileJournal` runs
    /// through `serve_with_recovery` — which `[verified 2026-09-02]` it could
    /// not, because the loop required `J: Default` — and the instant the
    /// session was last alive is on disk afterwards.
    #[test]
    fn a_file_journal_runs_through_the_serving_loop_and_records_when_it_lived() {
        let dir = scratch_dir("full");
        let addr = free_port();
        let opened = Arc::new(AtomicUsize::new(0));

        let (serving, home, counter) = (addr.clone(), dir.clone(), Arc::clone(&opened));
        std::thread::spawn(move || {
            let table = Table::with_capacity(1).serving(cfg());
            let _ = fixbolt_engine::serve_with_recovery(
                &serving,
                table,
                EchoApp::default(),
                4,
                Limits::new(8, 30_000).expect("both above zero"),
                OnDisk {
                    dir: home,
                    opened: counter,
                },
                fixbolt_engine::msglog::NoLog,
                Handles::new(),
            );
        });

        let mut client = connect(&addr);
        client.write_all(&logon_now(1)).expect("send");
        let mut buf = [0u8; 4096];
        let n = client.read(&mut buf).expect("a Logon back");
        let reply = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");
        assert!(reply.contains("|35=A|"), "the premise: logged on: {reply}");

        // Give the engine a turn or two to write the mark, then read the file
        // as an operator would — with the tool, not with the engine.
        let path = dir.join("TW44.journal");
        let mut found = None;
        for _ in 0..200 {
            if let Ok(r) = fixbolt_engine::journal::Reader::open(&path) {
                found = r.records().find_map(|rec| match rec {
                    fixbolt_engine::journal::Record::ActivityMark { at_ms } => Some(at_ms),
                    _ => None,
                });
                if found.is_some() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(
            opened.load(Ordering::Relaxed) > 0,
            "the deployment's own journal type must have been asked for"
        );
        assert!(
            found.is_some_and(|at| at > 0),
            "the file must say when this session was alive; without it a \
             restart cannot tell whether a trading day ended in between — \
             saw {found:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------------------
// The journal's on-disk format, version 1: a header and a CRC per record.
//
// **What this buys, in one sentence:** a flipped byte in the middle of a
// journal used to be replayed to a counterparty as a real message, correctly
// framed and correctly numbered, with nothing anywhere saying it had changed.
// Now the read stops at the record before it, exactly as it already stopped at
// a torn tail, and the count is published.
//
// **What it must not break:** a file written before this exists. Those files
// have no header, and they are read exactly as they were — the fixture below
// is bytes, committed, produced by `df94c08`, which is the commit before the
// format changed. Regenerating it with the new code would make this test
// assert that the new code agrees with itself.
// ---------------------------------------------------------------------------

/// 94 bytes: two messages, one inbound mark, one activity mark, no header.
///
/// Generated by `FileJournal<4096, 512>` at commit `df94c08` with
/// `Durability::Fsync`, writing `put(1, …)`, `mark_in(7)`,
/// `mark_active(1_788_431_527_120)`, `put(2, …)`.
const V0_JOURNAL: &[u8] = &[
    0x01, 0x00, 0x00, 0x00, 0x1b, 0x00, 0x00, 0x00, 0x38, 0x3d, 0x46, 0x49, 0x58, 0x2e, 0x34, 0x2e,
    0x34, 0x01, 0x33, 0x35, 0x3d, 0x44, 0x01, 0x33, 0x34, 0x3d, 0x31, 0x01, 0x31, 0x30, 0x3d, 0x30,
    0x30, 0x30, 0x01, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    0x00, 0x00, 0x00, 0xd0, 0x6c, 0xd3, 0x66, 0xa0, 0x01, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x1b,
    0x00, 0x00, 0x00, 0x38, 0x3d, 0x46, 0x49, 0x58, 0x2e, 0x34, 0x2e, 0x34, 0x01, 0x33, 0x35, 0x3d,
    0x44, 0x01, 0x33, 0x34, 0x3d, 0x32, 0x01, 0x31, 0x30, 0x3d, 0x30, 0x30, 0x31, 0x01,
];

/// A file with no header is version 0, and nothing about reading it changed.
#[test]
fn a_file_without_the_header_reads_exactly_as_before() {
    let p = tmp("v0-compat");
    std::fs::write(&p, V0_JOURNAL).expect("writable");

    let j: FileJournal<4096, 512> =
        FileJournal::open(&p, Durability::Fsync).expect("a v0 file still opens");
    assert_eq!(j.highest(), Some(2), "both messages are still there");
    assert_eq!(j.highest_in(), Some(7), "and the inbound mark");
    assert_eq!(
        j.last_active(),
        Some(1_788_431_527_120),
        "and the activity mark"
    );
    assert_eq!(j.torn_tail_bytes(), 0, "a clean v0 file is not torn");
    assert_eq!(
        j.corrupt_records(),
        0,
        "and a format with no checksums cannot report one"
    );
    let _ = std::fs::remove_file(&p);
}

/// A flipped byte stops the read at the record before it.
///
/// **The reversal that says what this bought**: the same byte flipped in a v0
/// file is not detected, and the message is replayed as though it were real.
/// That is the whole difference, and the test asserts both halves so the
/// second one cannot quietly stop being true.
#[test]
fn a_flipped_byte_stops_the_read_at_the_record_before_it() {
    let p = tmp("crc-flip");
    let _ = std::fs::remove_file(&p);
    {
        let mut j: FileJournal<4096, 512> = FileJournal::open(&p, Durability::Fsync).expect("open");
        assert!(j.put(1, b"8=FIX.4.4\x0135=D\x0134=1\x0110=000\x01"));
        assert!(j.put(2, b"8=FIX.4.4\x0135=D\x0134=2\x0110=001\x01"));
        assert!(j.put(3, b"8=FIX.4.4\x0135=D\x0134=3\x0110=002\x01"));
        j.close();
    }

    // Flip one byte inside the *second* message's payload. It is still a whole
    // record, correctly framed and correctly numbered — which is exactly why
    // nothing could see it before.
    let mut bytes = std::fs::read(&p).expect("readable");
    let at = bytes
        .windows(4)
        .position(|w| w == b"34=2")
        .expect("the second message is in the file");
    bytes[at + 3] ^= 0x01;
    std::fs::write(&p, &bytes).expect("writable");

    let j: FileJournal<4096, 512> = FileJournal::open(&p, Durability::Fsync).expect("open");
    assert_eq!(
        j.highest(),
        Some(1),
        "the read must stop at the record before the corrupt one"
    );
    assert_eq!(j.corrupt_records(), 1, "and say that it did");
    assert!(
        j.get(2).is_none() && j.get(3).is_none(),
        "nothing after the corruption is trusted"
    );
    let _ = std::fs::remove_file(&p);
}

/// The same flip in a v0 file is **not** detected, and that is what step 4 buys.
#[test]
fn the_same_flip_in_a_version_zero_file_is_replayed_as_though_it_were_real() {
    let p = tmp("v0-flip");
    let mut bytes = V0_JOURNAL.to_vec();
    let at = bytes
        .windows(4)
        .position(|w| w == b"34=2")
        .expect("the second message is in the fixture");
    bytes[at + 3] ^= 0x01;
    std::fs::write(&p, &bytes).expect("writable");

    let j: FileJournal<4096, 512> = FileJournal::open(&p, Durability::Fsync).expect("open");
    assert_eq!(
        j.highest(),
        Some(2),
        "a v0 file has no checksum, so the changed message is still read"
    );
    assert_eq!(j.corrupt_records(), 0, "and nothing can say otherwise");
    let _ = std::fs::remove_file(&p);
}

/// A v1 journal reopened and appended to stays readable, both halves.
#[test]
fn a_version_one_journal_survives_being_reopened_and_appended_to() {
    let p = tmp("v1-append");
    let _ = std::fs::remove_file(&p);
    {
        let mut j: FileJournal<4096, 512> = FileJournal::open(&p, Durability::Fsync).expect("open");
        assert!(j.put(1, b"8=FIX.4.4\x0135=D\x0134=1\x0110=000\x01"));
        j.close();
    }
    {
        let mut j: FileJournal<4096, 512> = FileJournal::open(&p, Durability::Fsync).expect("open");
        assert_eq!(j.highest(), Some(1), "what was there is read back");
        assert!(j.put(2, b"8=FIX.4.4\x0135=D\x0134=2\x0110=001\x01"));
        j.close();
    }
    let j: FileJournal<4096, 512> = FileJournal::open(&p, Durability::Fsync).expect("open");
    assert_eq!(j.highest(), Some(2));
    assert_eq!(j.corrupt_records(), 0, "appending wrote no bad checksums");
    let _ = std::fs::remove_file(&p);
}

/// A v0 file appended to stays v0 — one file, one format, for ever.
///
/// The alternative is a file whose first half has no checksums and whose second
/// half does, which no reader can parse without guessing where the change
/// happened.
#[test]
fn appending_to_a_version_zero_file_keeps_it_version_zero() {
    let p = tmp("v0-append");
    std::fs::write(&p, V0_JOURNAL).expect("writable");
    {
        let mut j: FileJournal<4096, 512> = FileJournal::open(&p, Durability::Fsync).expect("open");
        assert!(j.put(3, b"8=FIX.4.4\x0135=D\x0134=3\x0110=002\x01"));
        j.close();
    }
    let j: FileJournal<4096, 512> = FileJournal::open(&p, Durability::Fsync).expect("open");
    assert_eq!(j.highest(), Some(3), "the appended message is readable");
    assert_eq!(j.torn_tail_bytes(), 0, "and nothing is torn");
    let _ = std::fs::remove_file(&p);
}
