//! **A journal file has one appender**, and a quick reconnect waits for it by
//! being parked — never by the engine thread waiting.
//!
//! `[2026-09-24]` ADR-0153 retired a departing connection's journal without
//! joining its writer, so the writer can still be flushing (it may be asleep for
//! a millisecond between looks) when the counterparty logs on again. The
//! recovery's `FileJournal::open` then read the file **before** the old writer
//! had finished: the senior review's probe saw *"reopen saw highest_out=Some(1),
//! wanted Some(2)"*, 50 reconnects in 50, and for as long as both lived two
//! writers appended to one file. The counterparty was told a `MsgSeqNum` it had
//! already seen.
//! [ADR-0154](../../../docs/decisions/ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md);
//! plan `docs/plans/2026-09-23-phase-3-found-defects.md` *Sửa 5*, row K.
//!
//! Four tests, one per decision: the lock (`open` refuses while a writer holds
//! the file), the same lock across processes, the reconnect that resumes from
//! the finished file, and the parking that costs the `hft` engine thread no
//! wait.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use fixbolt_engine::journal::{Durability, FileJournal, wait_for_retired_writers};
use fixbolt_session::journal::Journal;

type Disk = FileJournal<64, 1024>;

/// **The tests that spawn a process or bind a port take turns.** A child
/// process holds a copy of every descriptor this process has open until its
/// `exec` — a listener `free_port` has just let go of, or a journal file —
/// so a test spawning one beside a test binding a port can see that port still
/// in use. Not the engine's behaviour, the harness's; serialised here rather
/// than retried.
static TURNS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn my_turn() -> std::sync::MutexGuard<'static, ()> {
    TURNS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "fixbolt-one-appender-{name}-{}.log",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn message(seq: u32) -> Vec<u8> {
    format!("8=FIX.4.4\x019=5\x0135=D\x0134={seq}\x0110=000\x01").into_bytes()
}

/// **Decision 1.** A retired journal's writer still owns its file until its
/// last flush; a second `open` of that path in the meantime is refused with
/// `WouldBlock` rather than reading a file that is still being written. Once
/// the writer is done, the path opens and reads back everything.
///
/// Five thousand inbound marks ahead of the outbound mark give the writer
/// milliseconds of unbuffered writes, so the reopen below lands while it is
/// still at work — on every run, not on a lucky one.
#[test]
fn a_journal_file_has_one_appender() {
    let path = temp("in-process");
    let mut first = Disk::open(&path, Durability::Async).expect("open");
    assert!(
        first.put(1, &message(1)),
        "the premise: the message was kept"
    );
    for seq in 1..=5_000 {
        first.mark_in(seq);
    }
    // Last, so a reopen that reads the file before the writer reached it sees
    // an older outbound count than the one this journal was given.
    first.mark_out(2);
    first.retire();
    drop(first);

    match Disk::open(&path, Durability::Async) {
        Ok(second) => {
            let seen = second.highest_out();
            drop(second);
            let _ = wait_for_retired_writers(Duration::from_secs(5));
            let _ = std::fs::remove_file(&path);
            panic!(
                "a second appender opened the file while the first had not finished \
                 (it read highest_out={seen:?})"
            );
        }
        Err(e) => assert_eq!(
            e.kind(),
            std::io::ErrorKind::WouldBlock,
            "a file its writer still holds is refused as WouldBlock, not as {e}"
        ),
    }

    assert!(
        wait_for_retired_writers(Duration::from_secs(5)),
        "the retired writer finished within 5 s"
    );
    let reopened = Disk::open(&path, Durability::Async).expect("the finished file opens");
    assert_eq!(
        reopened.highest_out(),
        Some(2),
        "the file the writer finished reads back the outbound mark it was given"
    );
    assert_eq!(reopened.highest_in(), Some(5_000));
    drop(reopened);
    let _ = std::fs::remove_file(&path);
}

/// **ADR-0155 decision 2: `ready` is answered by the writer, not by the
/// filesystem.** The handle is taken before the journal is retired, and the
/// file is deleted while the writer is still at work: a probe that asked the
/// filesystem would now find nothing there and call the file free. The handle
/// says "not yet" until the writer has closed the file, and "yes" after —
/// with no file left to ask.
#[test]
fn ready_is_answered_by_the_writer_not_the_filesystem() {
    let path = temp("released");
    let mut j = Disk::open(&path, Durability::Async).expect("open");
    let released = j.released();
    assert!(j.put(1, &message(1)), "the premise: the message was kept");
    // Work in hand for the writer: thousands of unbuffered writes, so it is
    // still writing when the file is deleted below.
    for seq in 1..=5_000 {
        j.mark_in(seq);
    }
    j.retire();
    drop(j);
    std::fs::remove_file(&path).expect("delete the journal under its writer");
    let early = released.is_released();
    let finished = wait_for_retired_writers(Duration::from_secs(5));
    assert!(
        !early,
        "ready said released while the writer was still writing — it asked the filesystem"
    );
    assert!(finished, "the retired writer finished within 5 s");
    assert!(
        released.is_released(),
        "the writer finished and closed its file, and the handle did not say so"
    );
}

/// The variable naming the file the child process tries to open.
const CHILD_ENV: &str = "FIXBOLT_ONE_APPENDER_CHILD";

/// **Not a test on its own**: the body [`a_second_process_cannot_append`] runs
/// in a child process. Without its environment variable it does nothing.
#[test]
fn child_opens_a_file_another_process_holds() {
    let Some(path) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    match Disk::open(Path::new(&path), Durability::Async) {
        Ok(_) => println!("CHILD-OPEN: Ok"),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            println!("CHILD-OPEN: WouldBlock");
        }
        Err(e) => println!("CHILD-OPEN: Err({:?}) {e}", e.kind()),
    }
}

/// **Decision 1, across processes** — a hazard older than ADR-0153: two
/// processes pointed at one journal path both appended to it. The lock is the
/// operating system's, so it refuses the second process exactly as it refuses a
/// second appender in this one.
#[test]
fn a_second_process_cannot_append() {
    let _turn = my_turn();
    let path = temp("two-processes");
    let held = Disk::open(&path, Durability::Async).expect("open");
    let out = std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--exact",
            "child_opens_a_file_another_process_holds",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, &path)
        .output()
        .expect("the child process ran");
    drop(held);
    let _ = std::fs::remove_file(&path);
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The harness prints the test's name on the same line, so the marker is
    // found where it is rather than at the start of a line.
    let said = stdout
        .lines()
        .find_map(|l| l.find("CHILD-OPEN:").map(|at| &l[at..]))
        .unwrap_or("CHILD-OPEN: (nothing)");
    assert_eq!(
        said, "CHILD-OPEN: WouldBlock",
        "a second process opened a journal this process holds; child said: {stdout}"
    );
}

/// What the wire tests share: a real socket, a real clock, a Logon stamped now.
mod wire {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::ops::Range;
    use std::time::Duration;

    use fixbolt_engine::Application;

    #[derive(Default)]
    pub struct EchoApp(fixbolt_conformance::echo::Echo);

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

    pub fn free_port() -> String {
        let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let a = l.local_addr().expect("bound").to_string();
        drop(l);
        a
    }

    pub fn connect(addr: &str) -> TcpStream {
        for _ in 0..500 {
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

    /// A message from `sender` to `ISLD`, stamped now: the serving loop uses
    /// the real clock and `max_skew_ms` would refuse the corpus's fixed instant.
    pub fn now_msg(sender: &str, seq: u32, body: &str) -> Vec<u8> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after 1970")
            .as_millis() as u64;
        let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
        let full = cache.format(now, 0);
        let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
        // The header in the order the session checks, then the body.
        let (kind, rest) = body.split_once('\u{1}').unwrap_or((body, ""));
        let rest = if rest.is_empty() {
            String::new()
        } else {
            format!("{rest}\u{1}")
        };
        let inner =
            format!("{kind}\u{1}34={seq}\u{1}49={sender}\u{1}52={stamp}\u{1}56=ISLD\u{1}{rest}");
        let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
        fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
    }

    pub fn logon(sender: &str, seq: u32) -> Vec<u8> {
        now_msg(sender, seq, "35=A\u{1}98=0\u{1}108=30")
    }

    /// Read until `needle` appears in what arrived, `|` for SOH.
    pub fn read_until(s: &mut TcpStream, needle: &str) -> String {
        read_until_any(s, &[needle])
    }

    /// Read until any of `needles` appears in what arrived.
    pub fn read_until_any(s: &mut TcpStream, needles: &[&str]) -> String {
        let mut seen = String::new();
        let mut buf = [0u8; 4096];
        while !needles.iter().any(|n| seen.contains(n)) {
            match s.read(&mut buf) {
                Ok(0) => panic!("the engine closed the socket before {needles:?}; saw {seen}"),
                Ok(n) => seen.push_str(&String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|")),
                Err(e) => panic!("none of {needles:?} within the read timeout ({e}); saw {seen}"),
            }
        }
        seen
    }

    /// Everything the engine sends until it closes the socket.
    pub fn read_to_end(s: &mut TcpStream) -> String {
        let mut seen = String::new();
        let mut buf = [0u8; 4096];
        loop {
            match s.read(&mut buf) {
                Ok(0) | Err(_) => return seen,
                Ok(n) => seen.push_str(&String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|")),
            }
        }
    }

    /// The highest `34=` in `text`.
    pub fn max_seq(text: &str) -> Option<u32> {
        text.split("|34=")
            .skip(1)
            .filter_map(|v| v.split('|').next().and_then(|n| n.parse().ok()))
            .max()
    }

    /// `34=` of the first message in `text`.
    pub fn first_seq(text: &str) -> u32 {
        let first = text.split("|10=").next().unwrap_or(text);
        let at = first.find("|34=").expect("a MsgSeqNum") + 4;
        first[at..]
            .split('|')
            .next()
            .and_then(|v| v.parse().ok())
            .expect("a number")
    }

    pub fn send(s: &mut TcpStream, bytes: &[u8]) {
        s.write_all(bytes).expect("send");
    }
}

/// **Decisions 2 and 3, end to end, in `standard`.** Fifty times: a session
/// logs on, sits quiet long enough for its writer to fall asleep, answers one
/// test request, the counterparty hangs up, and logs on again at once. The
/// recovery answers `ready` with [`fixbolt_engine::journal::file_busy`], so a
/// reconnect that arrives while the last session's writer is still flushing is
/// parked until the file is whole — and the new session neither sends a number
/// this end already sent nor asks for one the counterparty already sent.
#[cfg(feature = "standard")]
#[test]
fn a_reconnect_resumes_from_the_finished_file() {
    let _turn = my_turn();
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use fixbolt_engine::Config;
    use fixbolt_engine::journal::Released;
    use fixbolt_engine::observe::Handles;
    use fixbolt_engine::presession::{Limits, Table};
    use fixbolt_engine::recovery::{Recovery, Resumed};

    const ROUNDS: usize = 50;

    /// A recovery as `GUIDE.md` §6b shows it: it keeps the `released` handle of
    /// the journal it handed out, and `ready` is one atomic load (ADR-0155).
    struct Quick {
        path: PathBuf,
        asked: Arc<AtomicUsize>,
        handed_out: Option<Released>,
    }

    impl Recovery<Disk> for Quick {
        fn ready(&mut self, _cfg: &Config) -> bool {
            self.asked.fetch_add(1, Ordering::Relaxed);
            self.handed_out.as_ref().is_none_or(Released::is_released)
        }

        fn fresh(&mut self, _cfg: &Config) -> Disk {
            let j: Disk = FileJournal::open(&self.path, Durability::Async)
                .unwrap_or_else(|e| panic!("open journal: {e}"));
            self.handed_out = Some(j.released());
            j
        }

        fn recover(&mut self, cfg: &Config) -> Option<Resumed<Disk>> {
            Resumed::from_journal(self.fresh(cfg))
        }
    }

    let path = temp("reconnect");
    let addr = wire::free_port();
    let handles = Handles::new();
    let admin = handles.admin();
    let asked = Arc::new(AtomicUsize::new(0));
    let (serving, file, counter) = (addr.clone(), path.clone(), Arc::clone(&asked));
    let server = std::thread::spawn(move || {
        let table = Table::with_capacity(1).serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"));
        fixbolt_engine::serve_with_recovery(
            &serving,
            table,
            wire::EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            Quick {
                path: file,
                asked: counter,
                handed_out: None,
            },
            fixbolt_engine::msglog::NoLog,
            handles,
        )
    });

    let mut client_seq = 1;
    let mut last_out: Option<u32> = None;
    let mut missed = Vec::new();
    for round in 0..ROUNDS {
        let mut s = wire::connect(&addr);
        wire::send(&mut s, &wire::logon("TW44", client_seq));
        client_seq += 1;
        let reply = wire::read_until(&mut s, "|35=A|");
        let got = wire::first_seq(&reply);
        // **Outbound: a number already spent is the defect**; a gap is not —
        // the counterparty asks for it. So the check is "above", not "exactly
        // one above": a message the engine journalled but never got onto the
        // wire before the hang-up legitimately moves the count past what this
        // side saw.
        if let Some(last) = last_out
            && got <= last
        {
            missed.push(format!(
                "round {round}: Logon went out as 34={got}, 34={last} was already sent"
            ));
        }
        // **Inbound: a count read short asks for what it already has**, with a
        // `ResendRequest` on the heels of the `Logon`.
        let mut seen = reply;
        if seen.contains("|35=2|") {
            missed.push(format!(
                "round {round}: the resumed session asked for a resend: {seen}"
            ));
        } else {
            // Long enough for the writer to have written everything and gone
            // to sleep between looks, which is when it is slowest to notice
            // the next record — and the next record is the last before the
            // hang-up.
            std::thread::sleep(Duration::from_millis(5));
            wire::send(
                &mut s,
                &wire::now_msg("TW44", client_seq, &format!("35=1\u{1}112=R{round}")),
            );
            client_seq += 1;
            seen.push_str(&wire::read_until(&mut s, &format!("|112=R{round}|")));
        }
        // Hang up the moment the answer is in, read what the engine sent until
        // it lets go, and come back at once.
        let _ = s.shutdown(std::net::Shutdown::Write);
        seen.push_str(&wire::read_to_end(&mut s));
        last_out = wire::max_seq(&seen);
    }

    admin.shutdown(0);
    let _ = server.join();
    let _ = std::fs::remove_file(&path);
    assert!(
        missed.is_empty(),
        "{} misses in {ROUNDS} reconnects: {missed:#?}",
        missed.len()
    );
    assert!(
        asked.load(Ordering::Relaxed) >= ROUNDS,
        "the engine asked `ready` before each recovery"
    );
}

/// **ADR-0155 decision 5, in `hft`: a parked connection costs the engine thread
/// nothing it can measure.** One session is up; a second counterparty's
/// `Logon` is parked by a recovery whose `ready` reads a flag this test holds.
///
/// What is asserted is what the engine controls, not what a loaded machine does
/// to it (the voluntary-switch count this replaced read 1 and 7 in two runs of
/// ~300 during a parallel build, cause unknown — plan *Sửa 6*):
/// 1. **the engine thread stays runnable** while the connection is parked: its
///    time on a CPU plus its time waiting for one (`/proc/self/task/<tid>/schedstat`)
///    covers at least half the 50 ms window. A spinning engine is runnable the
///    whole time, however busy the machine; one that slept a millisecond per
///    ask is runnable for a few per cent of it. A rare short sleep — a page
///    fault during a parallel build — moves this by microseconds, not by half;
///    and the running session is still answered;
/// 2. **`ready` is asked at most once per millisecond** of the window, and at
///    least once;
/// 3. **the flag flipping admits the connection at the next ask**: `recover`
///    runs in the same pass as the `ready` that said yes.
///
/// **Not the plan's "≥ 10 000 turns in 50 ms".** `[measured 2026-09-24]` turns
/// counted by snapshot ping-pong read ~8 400 on an idle desk (debug build) and
/// **8** during a parallel `cargo build --release`: both threads of the
/// ping-pong are preempted, so the count measures the scheduler. Runnable time
/// does not depend on getting a CPU.
#[cfg(target_os = "linux")]
#[test]
fn a_parked_reconnect_does_not_slow_the_engine_thread() {
    let _turn = my_turn();
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use fixbolt_engine::Config;
    use fixbolt_engine::journal::Store;
    use fixbolt_engine::observe::Handles;
    use fixbolt_engine::presession::{Limits, Table};
    use fixbolt_engine::recovery::{Recovery, Resumed};

    /// How long the parked stretch is measured for.
    const WINDOW: Duration = Duration::from_millis(50);

    /// This thread's id, as `/proc/self/task/<tid>` names it.
    fn this_tid() -> String {
        let link = std::fs::read_link("/proc/thread-self").expect("/proc is mounted");
        link.file_name()
            .and_then(|n| n.to_str())
            .expect("a tid")
            .to_owned()
    }

    /// Nanoseconds thread `tid` has spent on a CPU plus waiting for one.
    fn runnable_ns(tid: &str) -> u64 {
        let stat = std::fs::read_to_string(format!("/proc/self/task/{tid}/schedstat"))
            .expect("the kernel reports schedstat");
        stat.split_whitespace()
            .take(2)
            .map(|v| v.parse::<u64>().expect("a number"))
            .sum()
    }

    struct Gate {
        open: Arc<AtomicBool>,
        asks: Arc<AtomicUsize>,
        said_yes: Arc<Mutex<Option<Instant>>>,
        recovered: Arc<Mutex<Option<Instant>>>,
    }

    impl Recovery<Store> for Gate {
        fn ready(&mut self, cfg: &Config) -> bool {
            if cfg.serves(b"TW44", b"ISLD") {
                return true;
            }
            self.asks.fetch_add(1, Ordering::Relaxed);
            let yes = self.open.load(Ordering::Acquire);
            if yes {
                let mut at = self.said_yes.lock().expect("lock");
                at.get_or_insert_with(Instant::now);
            }
            yes
        }

        fn recover(&mut self, cfg: &Config) -> Option<Resumed<Store>> {
            if !cfg.serves(b"TW44", b"ISLD") {
                let mut at = self.recovered.lock().expect("lock");
                at.get_or_insert_with(Instant::now);
            }
            None
        }

        fn fresh(&mut self, _cfg: &Config) -> Store {
            Store::default()
        }
    }

    let open = Arc::new(AtomicBool::new(false));
    let asks = Arc::new(AtomicUsize::new(0));
    let said_yes = Arc::new(Mutex::new(None));
    let recovered = Arc::new(Mutex::new(None));
    let gate = Gate {
        open: Arc::clone(&open),
        asks: Arc::clone(&asks),
        said_yes: Arc::clone(&said_yes),
        recovered: Arc::clone(&recovered),
    };

    let addr = wire::free_port();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving = addr.clone();
    let (tid_tx, tid_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        tid_tx.send(this_tid()).expect("the test is listening");
        let table = Table::with_capacity(2)
            .serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
            .serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW45").with_logon_timeout_ms(30_000));
        fixbolt_engine::serve_hft_with_recovery(
            &serving,
            table,
            wire::EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            gate,
            fixbolt_engine::msglog::NoLog,
            handles,
        )
    });

    let tid = tid_rx.recv().expect("the engine thread said who it is");

    // The session that keeps running.
    let mut up = wire::connect(&addr);
    wire::send(&mut up, &wire::logon("TW44", 1));
    let _ = wire::read_until(&mut up, "|35=A|");

    // The counterparty whose recovery says "not yet": parked, and asked again.
    let mut waiting = wire::connect(&addr);
    wire::send(&mut waiting, &wire::logon("TW45", 1));
    for _ in 0..2_000 {
        if asks.load(Ordering::Relaxed) >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        asks.load(Ordering::Relaxed) >= 2,
        "the premise: the second connection was parked and asked again"
    );

    // The window: this thread sleeps; the engine thread's runnable time is
    // read from the kernel on either side.
    let (asks0, runnable0) = (asks.load(Ordering::Relaxed), runnable_ns(&tid));
    let started = Instant::now();
    std::thread::sleep(WINDOW);
    let (asks1, runnable1) = (asks.load(Ordering::Relaxed), runnable_ns(&tid));
    let elapsed = started.elapsed();
    let runnable = Duration::from_nanos(runnable1 - runnable0);
    let asked = asks1 - asks0;
    let elapsed_ms = elapsed.as_micros().div_ceil(1_000) as usize;

    // The running session is still answered.
    wire::send(&mut up, &wire::now_msg("TW44", 2, "35=1\u{1}112=UP"));
    let _ = wire::read_until(&mut up, "|112=UP|");

    // The flag flips: admitted at the next ask.
    open.store(true, Ordering::Release);
    let reply = wire::read_until(&mut waiting, "|35=A|");
    let yes_at = said_yes.lock().expect("lock").expect("ready said yes");
    let recovered_at = recovered
        .lock()
        .expect("lock")
        .expect("recover ran for the parked counterparty");

    admin.shutdown(0);
    let _ = server.join();

    assert!(
        runnable * 2 >= elapsed,
        "the engine thread was runnable for {runnable:?} of {elapsed:?} while a connection was \
         parked — under half: it is sleeping, not spinning"
    );
    assert!(
        (1..=elapsed_ms + 1).contains(&asked),
        "ready was asked {asked} times in {elapsed:?} ({elapsed_ms} ms): at most once per \
         millisecond, and at least once"
    );
    assert!(
        reply.contains("|35=A|"),
        "the parked counterparty logged on: {reply}"
    );
    let lag = recovered_at.saturating_duration_since(yes_at);
    assert!(
        lag < Duration::from_millis(2),
        "recover ran {lag:?} after ready said yes: admitted in the same pass, not a later one"
    );
}
