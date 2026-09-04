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
    use fixbolt_engine::presession::{Limits, Table};
    use fixbolt_engine::recovery::{Recovery, Resumed};
    use fixbolt_engine::{Application, Config};
    use fixbolt_session::journal::Journal;

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
            let journal = self.fresh(cfg);
            let next_out = journal.highest().map_or(1, |h| h + 1);
            let next_in = journal.highest_in().unwrap_or(1);
            let last_active_ms = journal.last_active();
            if next_out == 1 && last_active_ms.is_none() {
                // Nothing was left behind. `None` is the ordinary answer, and
                // the journal just opened is handed back through `fresh`.
                return None;
            }
            Some(Resumed {
                journal,
                next_out,
                next_in,
                last_active_ms,
            })
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
