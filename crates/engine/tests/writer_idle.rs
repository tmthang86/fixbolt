//! The two writer threads give their core back when there is nothing to write.
//!
//! `[2026-09-23]` the `Async` journal's writer spun on `spin_loop` and the
//! message log's on `yield_now` whenever their ring was empty, in every mode —
//! a whole core each on an idle `standard` engine, which is the mode that
//! promises to give the core back. No gate saw it: the mode scripts watch the
//! engine thread only. ADR-0150 decision 4; plan
//! `docs/plans/2026-09-23-phase-3-found-defects.md` row D5.
//!
//! **A file of its own**, so it is a test process of its own: a writer thread
//! left running by a test in another file cannot be mistaken for the one under
//! measurement. Inside the file the tests are serialised by [`ONE_AT_A_TIME`]
//! for the same reason.
//!
//! Linux only: the figures come from `/proc/self/task/<tid>/stat`, as
//! `tls_initiator_wire.rs` reads them.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use fixbolt_engine::journal::{Durability, FileJournal, SLOT_LEN};
use fixbolt_engine::msglog::{Direction, FileLog, MessageLog};
use fixbolt_session::journal::Journal;

/// One writer thread in this process at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// `sysconf(_SC_CLK_TCK)`: 100 on every Linux this project builds on. Read as
/// a constant, as `tls_initiator_wire.rs` does, rather than through a binding.
const CLK_TCK: f64 = 100.0;

/// How long the idle writer's CPU is measured for.
const WINDOW: Duration = Duration::from_secs(1);

/// How many times the thread is looked for across that window.
const SAMPLES: u32 = 10;

/// **The ceiling: 50 ms of CPU in the 1 s window, 5% of a core.**
///
/// A spinning writer asks for the whole second; one that sleeps 1 ms at a time
/// wakes ~1 000 times a second for a few microseconds each. The gap is two
/// orders of magnitude, and the ceiling sits where neither side can reach it
/// by accident: a loaded machine makes a sleeping thread cost *less*, not more,
/// and a spinning one would need to be given under a twentieth of a core.
const CEILING_MS: f64 = 50.0;

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "fixbolt-writer-idle-{name}-{}.log",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// The tid of the one thread of this process named `comm`.
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
        "{} threads named {comm}, so the one under measurement is ambiguous: {found:?}",
        found.len()
    );
    found.pop()
}

/// `utime + stime` in clock ticks for one thread of this process — `None` if
/// the thread is gone. Counted from the last `)`: `comm` may contain spaces.
fn task_cpu(tid: &str) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/self/task/{tid}/stat")).ok()?;
    let (_, rest) = text.rsplit_once(')')?;
    let mut fields = rest.split_whitespace();
    // state (3), then ppid .. cmajflt — ten — then utime (14), stime (15).
    let _state = fields.next()?;
    let utime: u64 = fields.nth(10)?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some(utime.saturating_add(stime))
}

/// Milliseconds of CPU the thread named `comm` used across [`WINDOW`], after
/// asserting it could be read throughout — a thread that has died costs
/// nothing, and would pass the ceiling on nothing.
fn idle_cpu_ms(comm: &str) -> f64 {
    let tid = tid_named(comm).unwrap_or_else(|| panic!("no thread named {comm}"));
    let before = task_cpu(&tid).expect("the writer's /proc/.../stat");
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
    assert_eq!(
        alive, SAMPLES,
        "the writer could not be read throughout the window, so nothing was measured"
    );
    // Scaled to the nominal window, so the sentence compares like with like.
    let ms = (after - before) as f64 * 1000.0 / CLK_TCK * (WINDOW.as_secs_f64() / elapsed);
    eprintln!("{comm}: {ms:.0} ms of CPU in {elapsed:.2} s idle");
    ms
}

fn message(seq: u32) -> Vec<u8> {
    format!("8=FIX.4.4\x019=5\x0135=D\x0134={seq}\x0110=000\x01").into_bytes()
}

#[test]
fn an_idle_async_journal_writer_gives_its_core_back() {
    let _one = lock();
    let path = temp("journal");
    let mut j: FileJournal<8, SLOT_LEN> =
        FileJournal::open(&path, Durability::Async).expect("open");
    // One message, so the writer has certainly started and run its loop.
    assert!(j.put(1, &message(1)));
    std::thread::sleep(Duration::from_millis(200));

    let ms = idle_cpu_ms("fixbolt-journal");
    j.close();
    drop(j);
    let _ = std::fs::remove_file(&path);
    assert!(
        ms < CEILING_MS,
        "the journal writer used {ms:.0} ms of CPU in 1 s while idle"
    );
}

#[test]
fn an_idle_message_log_writer_gives_its_core_back() {
    let _one = lock();
    let path = temp("msglog");
    let mut log = FileLog::open(&path).expect("open");
    log.record(Direction::In, 0, 0, 0, &message(1));
    std::thread::sleep(Duration::from_millis(200));

    let ms = idle_cpu_ms("fixbolt-msglog");
    drop(log);
    let _ = std::fs::remove_file(&path);
    assert!(
        ms < CEILING_MS,
        "the message log writer used {ms:.0} ms of CPU in 1 s while idle"
    );
}

/// The sleep does not cost a message: a writer that has gone to sleep still
/// drains a burst of 1 000 into the file. Green before the fix and after — it
/// guards the waking, not the sleeping.
#[test]
fn an_async_journal_still_writes_a_burst_after_it_slept() {
    let _one = lock();
    let path = temp("burst");
    let mut j: FileJournal<8, SLOT_LEN> =
        FileJournal::open(&path, Durability::Async).expect("open");
    // Past 1 024 empty polls many times over: the writer is asleep by now.
    std::thread::sleep(Duration::from_millis(50));
    for seq in 1..=1000u32 {
        assert!(j.put(seq, &message(seq)), "put {seq}");
    }
    j.close();
    drop(j);

    let back: FileJournal<1024, SLOT_LEN> =
        FileJournal::open(&path, Durability::Fsync).expect("reopen");
    let kept = (1..=1000u32).filter(|s| back.get(*s).is_some()).count();
    let highest = back.highest_out();
    drop(back);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        (kept, highest),
        (1000, Some(1000)),
        "a burst after the writer slept did not all reach the file"
    );
}
