//! A connection's journal is **retired** when it leaves, and its writer is
//! awaited only after serving.
//!
//! `[2026-09-23]` `Engine::turn` removed a finished connection with
//! `conns.swap_remove(i)`, which dropped its `FileJournal` on the engine
//! thread: `Drop` → `close()` → `join` the writer — a `futex` wait in the
//! middle of serving, in `hft` as in `standard`. Row W's windowed
//! `scripts/check-no-kernel-sleep.sh` caught it 2 runs in 8.
//! [ADR-0153](../../../docs/decisions/ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md);
//! plan `docs/plans/2026-09-23-phase-3-found-defects.md` *Sửa 3*, row J.
//!
//! **What is counted** is the engine thread's own `voluntary_ctxt_switches`
//! from `/proc/thread-self/status`: every wait in the kernel — a `futex` in a
//! `join` among them — is one. The turns are driven by hand and never idle, so
//! nothing else in the measured stretch has a reason to sleep.
//!
//! A file of its own, so its retired writers are the only ones the process-wide
//! count holds while the last test waits on it.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

use fixbolt_conformance::script::FIXED_TIME_MILLIS;
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::{Durability, FileJournal, Reader, wait_for_retired_writers};
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_engine::wait::{Spin, Waiting};
use fixbolt_engine::{Application, Config, Engine};
use fixbolt_session::journal::Journal;

const N: usize = 256;
const RX: usize = 4096;
const TX: usize = 8192;
/// Small, so twenty of them are cheap; the question is the writer, not the ring.
const SLOTS: usize = 64;
const LEN: usize = 512;
/// How many sessions end in the measured stretch.
const SESSIONS: usize = 20;

type File = FileJournal<SLOTS, LEN>;

#[derive(Default)]
struct EchoApp(fixbolt_conformance::echo::Echo);

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        let (seq, stamp) = (hdr.seq, hdr.stamp);
        self.0.reply(msg, seq, stamp, out)
    }
}

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// A `Logon` from the counterparty, with a real body length and checksum.
fn logon(seq: u32) -> Vec<u8> {
    let stamp = fixbolt_conformance::script::FIXED_TIME_IN;
    let inner = format!(
        "35=A\u{1}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}34={seq}\u{1}98=0\u{1}108=30\u{1}"
    );
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

fn drain(peer: &mut Loopback) -> String {
    let mut out = String::new();
    let mut buf = [0u8; 8192];
    while let Io::Ready(n) = peer.recv(&mut buf) {
        if n == 0 {
            break;
        }
        out.push_str(&String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|"));
    }
    out
}

fn temp(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("fixbolt-retire-{name}-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

/// This thread's voluntary context switches so far.
fn voluntary_switches() -> u64 {
    let status = std::fs::read_to_string("/proc/thread-self/status").expect("/proc is mounted");
    status
        .lines()
        .find_map(|l| l.strip_prefix("voluntary_ctxt_switches:"))
        .and_then(|v| v.trim().parse().ok())
        .expect("the kernel reports voluntary_ctxt_switches")
}

/// Twenty sessions, one after another: each logs on, then the counterparty
/// hangs up and the engine turns until the connection is gone. Only that last
/// stretch — the session ending — is counted, summed over the twenty.
fn switches_while_sessions_end<W: Waiting, J: Journal>(
    wait: W,
    mut journal: impl FnMut(usize) -> J,
) -> u64 {
    let mut e: Engine<
        Loopback,
        fixbolt_session::Acceptor,
        InlineDispatch<EchoApp>,
        ManualClock,
        W,
        J,
        N,
        RX,
        TX,
    > = Engine::new(
        cfg(),
        InlineDispatch::new(EchoApp::default()),
        ManualClock::at(FIXED_TIME_MILLIS),
        wait,
        4,
    );
    let mut total = 0;
    for i in 0..SESSIONS {
        let (mut peer, engine_side) = Loopback::pair();
        e.add_with_journal(engine_side, journal(i));
        let _ = peer.send(&logon(1));
        e.turn();
        let reply = drain(&mut peer);
        assert!(reply.contains("|35=A|"), "session {i} logged on: {reply}");
        assert_eq!(e.connections(), 1, "the premise: one session is up");

        peer.close();
        let before = voluntary_switches();
        let mut turns = 0;
        while e.connections() > 0 && turns < 100 {
            e.turn();
            turns += 1;
        }
        let after = voluntary_switches();
        assert_eq!(
            e.connections(),
            0,
            "the measured stretch really had session {i} end, after {turns} turns"
        );
        total += after - before;
    }
    total
}

fn file_journals(name: &'static str) -> impl FnMut(usize) -> File {
    move |i| File::open(&temp(&format!("{name}-{i}")), Durability::Async).expect("open")
}

/// After the writers are done, not before: a retired writer still owns its file.
fn remove_files(name: &str) {
    for i in 0..SESSIONS {
        let _ = std::fs::remove_file(temp(&format!("{name}-{i}")));
    }
}

/// **The defect, in `hft`.** Zero: the engine thread never waits in the kernel
/// while a session with a `FileJournal` ends.
#[test]
fn a_session_with_a_file_journal_ends_without_the_engine_thread_waiting() {
    let n = switches_while_sessions_end(Spin, file_journals("hft"));
    assert!(
        wait_for_retired_writers(Duration::from_secs(5)),
        "the retired writers finished after serving"
    );
    remove_files("hft");
    assert_eq!(
        n, 0,
        "the engine thread made {n} voluntary switches while sessions with a FileJournal ended — \
         a writer join on the serving path"
    );
}

/// **The same, in `standard`.** `standard` may sleep when idle, and these
/// turns never idle; it must not wait for a writer. Measured against the same
/// stretch with a `MemJournal`, which has no writer at all.
#[cfg(feature = "standard")]
#[test]
fn a_standard_engine_waits_no_longer_for_a_file_journal_than_for_a_memory_one() {
    use fixbolt_engine::block::Block;
    use fixbolt_engine::journal::MemJournal;
    type Mem = MemJournal<SLOTS, LEN>;
    let mem = switches_while_sessions_end(Block::new(8), |_| Mem::new());
    let file = switches_while_sessions_end(Block::new(8), file_journals("standard"));
    assert!(
        wait_for_retired_writers(Duration::from_secs(5)),
        "the retired writers finished after serving"
    );
    remove_files("standard");
    assert!(
        file <= mem,
        "the standard engine thread made {file} voluntary switches while sessions with a \
         FileJournal ended, against {mem} with a MemJournal — a writer join on the serving path"
    );
}

fn message(seq: u32) -> Vec<u8> {
    format!("8=FIX.4.4\x019=5\x0135=D\x0134={seq}\x0110=000\x01").into_bytes()
}

/// **What makes retiring safe**: the wait after serving is what puts
/// everything on disk. 10 000 short messages fit the writer's ring whole, so
/// none is dropped for want of room and every one must be in the file.
#[test]
fn a_retired_journal_reaches_the_disk_before_the_wait_returns() {
    const MESSAGES: u32 = 10_000;
    let path = temp("reaches-the-disk");
    let mut j: FileJournal<16, 128> = FileJournal::open(&path, Durability::Async).expect("open");
    for seq in 1..=MESSAGES {
        assert!(j.put(seq, &message(seq)), "message {seq} was accepted");
    }
    j.retire();
    // Dropping a retired journal does not join: nobody else is left to wait.
    drop(j);
    assert!(
        wait_for_retired_writers(Duration::from_secs(5)),
        "the retired writer finished within 5 s"
    );
    let on_disk = Reader::open(&path).expect("read").records().count();
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        on_disk, MESSAGES as usize,
        "only {on_disk} of {MESSAGES} messages were on disk when the wait for retired writers returned"
    );
}

/// **A journal its owner closes still joins** — ADR-0153 decision 5. `close()`
/// returns only once the writer has written everything.
#[test]
fn a_journal_closed_by_its_owner_still_joins() {
    const MESSAGES: u32 = 2_000;
    let path = temp("closed-by-owner");
    let mut j: FileJournal<16, 128> = FileJournal::open(&path, Durability::Async).expect("open");
    for seq in 1..=MESSAGES {
        assert!(j.put(seq, &message(seq)));
    }
    j.close();
    let on_disk = Reader::open(&path).expect("read").records().count();
    drop(j);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        on_disk, MESSAGES as usize,
        "close() returned with only {on_disk} of {MESSAGES} messages on disk"
    );
}
