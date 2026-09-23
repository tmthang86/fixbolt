//! **Row M, Sửa 5 item 2**: a test that would notice if `after_serving`
//! (`crates/engine/src/lib.rs:3370`) stopped waiting for retired journal
//! writers.
//!
//! `cargo test -p fixbolt-engine --features standard` was 372/0 with
//! `after_serving` doing nothing — J's fix (ADR-0153 decision 4) had no gate
//! of its own. This file is that gate, for the three families of serving loop
//! that call it: [`fixbolt_engine::serve_with_recovery`],
//! [`fixbolt_engine::connect_and_serve`], and the shard runtime's serve path
//! (`fixbolt_engine::shard::Shards`).
//!
//! **Required reversal**: make `after_serving` an empty body. All three tests
//! here must go red on their own assertion, quoting
//! *"serve returned while N retired writers were still writing"* or the
//! equivalent record-count shortfall — then `git checkout --
//! crates/engine/src/lib.rs` restores it.
//!
//! # Why the shard test cannot ask for `Duration::ZERO`
//!
//! `serve_with_recovery` and `connect_and_serve` are ordinary blocking calls:
//! `after_serving` runs **inside** them, on the calling thread, before they
//! return — so the instant `.join()` on the thread that called them unblocks,
//! the wait has already happened and `wait_for_retired_writers(Duration::ZERO)`
//! is the honest check.
//!
//! The sharded runtime offers no such moment. Its only public door,
//! `serve_sharded_hft_with_recovery`, returns `Result<Infallible, _>` — it
//! cannot return on success, so there is nothing to join. `Shards`' own
//! rustdoc says shutdown is dropping it: *"Each thread's loop ends when its
//! channel disconnects... that is process shutdown, and it is the only
//! shutdown this offers"* — ADR-0088 decision 5 left it that way on purpose.
//! Dropping `Shards` only signals the shard thread; noticing the disconnect,
//! dropping the engine, and calling `crate::after_serving(None)` happens
//! asynchronously on that thread, with no handle left to join it by. So this
//! test's third case drives `Shards` directly (the same building block
//! `shard.rs`'s own serve loop uses) and, after dropping it, calls
//! `wait_for_retired_writers` with a real timeout rather than `Duration::ZERO`
//! — the function polls every 1 ms regardless, so a working `after_serving`
//! still returns `true` almost at once; a hollowed-out one exhausts the
//! timeout and returns `false`, which is exactly the reversal this row needs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

// ---------------------------------------------------------------------------
// Shared wire-building helpers, in the shape every other file in this crate
// already uses (`engine_recovery.rs`, `on_disk.rs`, `reconnect_wire.rs`).
// ---------------------------------------------------------------------------

/// A scratch path that does not collide between tests or runs.
fn scratch_path(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "fixbolt-after-serving-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

/// A `Logon`, stamped now — every engine here runs the real clock.
fn logon_now(seq: u32, sender: &str, target: &str) -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = cache.format(now, 0);
    let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
    let inner = format!(
        "35=A\u{1}34={seq}\u{1}49={sender}\u{1}52={stamp}\u{1}56={target}\u{1}98=0\u{1}108=30\u{1}"
    );
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

/// A `NewOrderSingle`, stamped now, numbered `seq`. What the echo application
/// answers with an outbound message of its own — the one journal `put`
/// records.
fn order_now(seq: u32, sender: &str, target: &str) -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = cache.format(now, 0);
    let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
    let inner = format!(
        "35=D\u{1}34={seq}\u{1}49={sender}\u{1}52={stamp}\u{1}56={target}\u{1}\
         11=ord{seq}\u{1}21=1\u{1}38=100\u{1}40=1\u{1}54=1\u{1}55=X\u{1}60={stamp}\u{1}"
    );
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

/// How many application replies (`35=D`) are on disk, as a real operator would
/// read them — with `Reader`, not with the engine.
fn message_records_on_disk(path: &std::path::Path) -> usize {
    let Ok(reader) = fixbolt_engine::journal::Reader::open(path) else {
        return 0;
    };
    reader
        .records()
        .filter(|r| matches!(r, fixbolt_engine::journal::Record::Message { .. }))
        .count()
}

const RECORDS_WANTED: u32 = 2_000;

// ---------------------------------------------------------------------------
// serve_with_recovery / connect_and_serve — both are blocking calls that run
// `after_serving` before they return, so `.join()` on the thread that called
// them is the moment to check.
// ---------------------------------------------------------------------------

#[cfg(all(feature = "standard", unix))]
mod through_the_blocking_doors {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::ops::Range;
    use std::time::Duration;

    use fixbolt_engine::journal::{Durability, FileJournal};
    use fixbolt_engine::observe::Handles;
    use fixbolt_engine::presession::{Limits, Table};
    use fixbolt_engine::reconnect::Policy;
    use fixbolt_engine::recovery::{Recovery, Resumed};
    use fixbolt_session::{Application, Config};

    use super::{RECORDS_WANTED, logon_now, message_records_on_disk, order_now, scratch_path};

    type Disk = FileJournal<4_096, 512>;

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

    /// Always fresh, on a real file. There is nothing to resume in this test —
    /// what matters is only that `fresh` opens `Durability::Async`, the
    /// default D7 names and the one row K's `try_lock` will change the
    /// meaning of `open` under.
    struct FreshFile {
        path: std::path::PathBuf,
    }

    impl Recovery<Disk> for FreshFile {
        fn fresh(&mut self, _cfg: &Config) -> Disk {
            FileJournal::open(&self.path, Durability::Async)
                .unwrap_or_else(|e| panic!("open journal: {e}"))
        }

        fn recover(&mut self, _cfg: &Config) -> Option<Resumed<Disk>> {
            None
        }
    }

    fn acceptor_cfg() -> Config {
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
    }

    fn initiator_cfg() -> Config {
        Config::initiator(b"FIX.4.4", b"FIXBOLT", b"VENUE").with_heart_bt_int(30)
    }

    fn free_addr() -> String {
        let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let a = l.local_addr().expect("bound").to_string();
        drop(l);
        a
    }

    fn connect(addr: &str) -> TcpStream {
        for _ in 0..500 {
            if let Ok(s) = TcpStream::connect(addr) {
                s.set_nodelay(true).expect("nodelay");
                s.set_read_timeout(Some(Duration::from_secs(10)))
                    .expect("timeout");
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("nobody ever bound {addr}");
    }

    /// Read from `sock` until at least `want` occurrences of `|35=D|` have
    /// been seen (echoed application replies), or the deadline passes.
    fn drain_echoes(sock: &mut TcpStream, want: usize, deadline: std::time::Instant) -> usize {
        let mut seen = 0usize;
        let mut buf = [0u8; 65_536];
        while seen < want && std::time::Instant::now() < deadline {
            match sock.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");
                    seen += text.matches("|35=D|").count();
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => break,
            }
        }
        seen
    }

    /// **`serve_with_recovery` — the acceptor door.**
    ///
    /// Repeated five times (in one test, since each iteration owns its own
    /// scratch file and port) so a red caused by scheduling luck rather than by
    /// a hollowed-out `after_serving` cannot hide.
    #[test]
    fn serve_with_recovery_returns_after_its_writers_finished() {
        for attempt in 0..5 {
            let path = scratch_path(&format!("serve-with-recovery-{attempt}"));
            let addr = free_addr();
            let handles = Handles::new();
            let admin = handles.admin();

            let serving = addr.clone();
            let recovery_path = path.clone();
            let handle = std::thread::spawn(move || {
                let table = Table::with_capacity(1).serving(acceptor_cfg());
                fixbolt_engine::serve_with_recovery(
                    &serving,
                    table,
                    EchoApp::default(),
                    4,
                    Limits::new(8, 30_000).expect("both above zero"),
                    FreshFile {
                        path: recovery_path,
                    },
                    fixbolt_engine::msglog::NoLog,
                    handles,
                )
            });

            let mut client = connect(&addr);
            client
                .write_all(&logon_now(1, "TW44", "ISLD"))
                .expect("send Logon");
            let mut buf = [0u8; 4096];
            let n = client.read(&mut buf).expect("a Logon back");
            let reply = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");
            assert!(reply.contains("|35=A|"), "the premise: logged on: {reply}");

            let mut batch = Vec::new();
            for seq in 2..=(RECORDS_WANTED + 1) {
                batch.extend_from_slice(&order_now(seq, "TW44", "ISLD"));
            }
            client.write_all(&batch).expect("send the orders");

            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            let got = drain_echoes(&mut client, RECORDS_WANTED as usize, deadline);
            assert_eq!(
                got, RECORDS_WANTED as usize,
                "attempt {attempt}: the engine must have echoed every order \
                 before shutdown is asked, or a later mismatch would be about \
                 timing rather than about after_serving"
            );

            admin.shutdown(0);
            let result = handle
                .join()
                .expect("the serving thread must not panic")
                .expect("serve_with_recovery must not itself error");
            let _ = result;

            // The moment the call has returned, with nothing else awaited:
            assert!(
                fixbolt_engine::journal::wait_for_retired_writers(Duration::ZERO),
                "attempt {attempt}: serve_with_recovery returned while a \
                 retired writer was still writing — after_serving did not wait"
            );
            let on_disk = message_records_on_disk(&path);
            assert_eq!(
                on_disk, RECORDS_WANTED as usize,
                "attempt {attempt}: all {RECORDS_WANTED} records must be on \
                 disk the instant the call returns; found {on_disk}"
            );

            let _ = std::fs::remove_file(&path);
        }
    }

    /// **`connect_and_serve` — the initiator door, `[[Sửa 5]]`'s second
    /// family.** The venue here is a hand-rolled acceptor in this test (the
    /// same shape `reconnect_wire.rs` uses), because the point is whether
    /// `dial`'s own call to `after_serving` (`lib.rs:2618`) is exercised, not
    /// whether the protocol is right end to end.
    #[test]
    fn connect_and_serve_with_recovery_returns_after_its_writers_finished() {
        for attempt in 0..5 {
            let path = scratch_path(&format!("connect-and-serve-{attempt}"));
            let venue = TcpListener::bind("127.0.0.1:0").expect("a free port");
            let addr = venue.local_addr().expect("bound").to_string();

            let venue_thread = std::thread::spawn(move || {
                let (mut sock, _) = venue.accept().expect("the initiator dials");
                sock.set_nodelay(true).expect("nodelay");
                sock.set_read_timeout(Some(Duration::from_secs(10)))
                    .expect("timeout");

                // The Logon: read it, answer it.
                let mut buf = [0u8; 4096];
                let n = sock.read(&mut buf).expect("the initiator's Logon");
                let seen = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");
                assert!(seen.contains("|35=A|"), "the premise: got a Logon: {seen}");
                sock.write_all(&logon_now(1, "VENUE", "FIXBOLT"))
                    .expect("answer the Logon");

                // 2 000 orders, the venue's own outbound numbers 2..=2001,
                // which land as the initiator's *inbound* stream.
                let mut batch = Vec::new();
                for seq in 2..=(RECORDS_WANTED + 1) {
                    batch.extend_from_slice(&order_now(seq, "VENUE", "FIXBOLT"));
                }
                sock.write_all(&batch).expect("send the orders");

                let deadline = std::time::Instant::now() + Duration::from_secs(30);
                let got = drain_echoes(&mut sock, RECORDS_WANTED as usize, deadline);
                assert_eq!(
                    got, RECORDS_WANTED as usize,
                    "the initiator's own echo must have answered every order"
                );
                sock
            });

            let handles = Handles::new();
            let admin = handles.admin();
            let recovery_path = path.clone();
            let dialled = addr.clone();
            let handle = std::thread::spawn(move || {
                fixbolt_engine::connect_and_serve(
                    &dialled,
                    initiator_cfg(),
                    EchoApp::default(),
                    // A stopped policy still dials once; nothing here relies
                    // on a reconnect, only on the first session ending
                    // cleanly.
                    Policy::new(50, 200).expect("a legal pair"),
                    FreshFile {
                        path: recovery_path,
                    },
                    fixbolt_engine::msglog::NoLog,
                    handles,
                )
            });

            // Wait for the venue to have both answered the Logon and drained
            // every echo before asking for shutdown, or the count on disk
            // could be a race against sends still in flight rather than a
            // fact about after_serving.
            let _sock = venue_thread
                .join()
                .expect("the venue thread must not panic");

            admin.shutdown(0);
            let result = handle
                .join()
                .expect("the initiator thread must not panic")
                .expect("connect_and_serve must not itself error");
            let _ = result;

            assert!(
                fixbolt_engine::journal::wait_for_retired_writers(Duration::ZERO),
                "attempt {attempt}: connect_and_serve returned while a \
                 retired writer was still writing — after_serving did not wait"
            );
            let on_disk = message_records_on_disk(&path);
            assert_eq!(
                on_disk, RECORDS_WANTED as usize,
                "attempt {attempt}: all {RECORDS_WANTED} records must be on \
                 disk the instant the call returns; found {on_disk}"
            );

            let _ = std::fs::remove_file(&path);
        }
    }
}

// ---------------------------------------------------------------------------
// The shard serve path. See the module doc for why this drives `Shards`
// directly instead of `serve_sharded_hft_with_recovery`.
// ---------------------------------------------------------------------------

#[cfg(all(feature = "affinity", feature = "standard", target_os = "linux"))]
mod through_the_shard_runtime {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::ops::Range;
    use std::time::Duration;

    use fixbolt_engine::affinity::{CoreId, ShardPlan, Topology};
    use fixbolt_engine::clock::{Clock, SystemClock};
    use fixbolt_engine::journal::{Durability, FileJournal};
    use fixbolt_engine::presession::{Limits, Table};
    use fixbolt_engine::recovery::Start;
    use fixbolt_engine::shard::Shards;
    use fixbolt_engine::{Acceptor, Application, Config, Engine};
    use fixbolt_session::Config as SessionConfig;

    use super::{RECORDS_WANTED, logon_now, message_records_on_disk, order_now, scratch_path};

    const PRE: usize = 4_096;
    type Disk = FileJournal<4_096, 512>;

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

    fn cfg() -> SessionConfig {
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
    }

    /// One physical core, or `None` on a machine with none to give — the same
    /// walk `shard_hft.rs`/`shard_recovery.rs` do, for the same reason: a
    /// `#[test]` that skipped itself on every runner would never be red.
    fn one_shard() -> Option<ShardPlan> {
        let topology = Topology::read().expect("reading /sys on Linux");
        let first = topology.online().first().copied()?;
        Some(ShardPlan::new(vec![first as CoreId]).allow_unisolated())
    }

    fn free_addr() -> String {
        let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let a = l.local_addr().expect("bound").to_string();
        drop(l);
        a
    }

    fn connect(addr: &str) -> TcpStream {
        for _ in 0..500 {
            if let Ok(s) = TcpStream::connect(addr) {
                s.set_nodelay(true).expect("nodelay");
                s.set_read_timeout(Some(Duration::from_secs(10)))
                    .expect("timeout");
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("nobody ever bound {addr}");
    }

    fn drain_echoes(sock: &mut TcpStream, want: usize, deadline: std::time::Instant) -> usize {
        let mut seen = 0usize;
        let mut buf = [0u8; 65_536];
        while seen < want && std::time::Instant::now() < deadline {
            match sock.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");
                    seen += text.matches("|35=D|").count();
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => break,
            }
        }
        seen
    }

    /// **The shard serve path.** `Shards::start` + a hand-rolled pre-session
    /// accept, mirroring the acceptor half of
    /// `shard::serve_sharded_hft_with_recovery_with` exactly (`shard.rs`
    /// ~782-806) because that half is what a real deployment runs and there is
    /// no public front door this test could call instead and still have
    /// anything to `drop`.
    #[test]
    fn shard_serve_returns_after_its_writers_finished() {
        let Some(plan) = one_shard() else {
            panic!(
                "no online core to host one shard — Topology::read said so, \
                 and a skip here would be a test that skipped itself on every \
                 machine that ran it"
            );
        };

        let path = scratch_path("shard-serve");
        let addr = free_addr();
        let acceptor = Acceptor::bind(&addr).expect("bind");

        let dialled = addr.clone();
        let client = std::thread::spawn(move || {
            let mut sock = connect(&dialled);
            sock.write_all(&logon_now(1, "TW44", "ISLD"))
                .expect("send Logon");
            let mut buf = [0u8; 4096];
            let n = sock.read(&mut buf).expect("a Logon back");
            let reply = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");
            assert!(reply.contains("|35=A|"), "the premise: logged on: {reply}");

            let mut batch = Vec::new();
            for seq in 2..=(RECORDS_WANTED + 1) {
                batch.extend_from_slice(&order_now(seq, "TW44", "ISLD"));
            }
            sock.write_all(&batch).expect("send the orders");

            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            let got = drain_echoes(&mut sock, RECORDS_WANTED as usize, deadline);
            assert_eq!(
                got, RECORDS_WANTED as usize,
                "the shard's own echo must have answered every order before \
                 the shard runtime is torn down"
            );
            sock
        });

        // The acceptor half of the shard front door, run by hand so this test
        // can `drop(shards)` afterwards — `serve_sharded_hft_with_recovery`
        // never returns on success and offers nothing to drop.
        let table = Table::with_capacity(1).serving(cfg());
        let limits = Limits::new(8, 30_000).expect("both above zero");
        let mut set = fixbolt_engine::presession::PendingSet::<
            fixbolt_engine::transport::TcpTransport,
            Table,
            PRE,
        >::new(limits, table);
        let mut clock = SystemClock;

        let taken = loop {
            while set.len() < limits.pending() {
                let Some(t) = acceptor.accept() else { break };
                drop(set.admit(t, Clock::now_ms(&mut clock)));
            }
            let now = Clock::now_ms(&mut clock);
            set.turn(now);
            if let Some(i) = set.settled() {
                if let Some(p) = set.take(i) {
                    break p;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let settled_cfg = taken.config().expect("the Logon named a counterparty");
        assert!(
            settled_cfg.serves(b"TW44", b"ISLD"),
            "the one counterparty this table serves"
        );

        let journal = FileJournal::open(&path, Durability::Async)
            .unwrap_or_else(|e| panic!("open journal: {e}"));

        let mut shards = Shards::<PRE, Disk>::start(&plan, |_shard: usize| {
            let bare: fixbolt_engine::TcpAcceptorEngine<
                EchoApp,
                fixbolt_engine::wait::Spin,
                Disk,
                fixbolt_engine::msglog::NoLog,
            > = Engine::new(
                cfg(),
                fixbolt_engine::dispatch::InlineDispatch::new(EchoApp::default()),
                SystemClock,
                fixbolt_engine::wait::Spin,
                4,
            );
            bare.with_shard(0)
        })
        .expect("one core, validated by one_shard()");

        shards
            .hand_started(taken, Start::Fresh(journal))
            .expect("the one shard this plan has");

        // Every echo must be on the wire before the runtime is torn down, or
        // "all 2 000 records" would be a race against the shard thread rather
        // than a fact about after_serving.
        let _sock = client.join().expect("the client thread must not panic");

        // **Dropping `Shards` is the only shutdown this offers** (its own
        // rustdoc). It signals the shard thread; noticing, retiring the
        // journal, and calling `after_serving(None)` happens on that thread,
        // asynchronously — see the module doc for why the wait below is not
        // `Duration::ZERO`.
        drop(shards);

        assert!(
            fixbolt_engine::journal::wait_for_retired_writers(Duration::from_secs(5)),
            "the shard's retired writer did not finish within 5 s of the \
             runtime being torn down — after_serving did not wait"
        );
        let on_disk = message_records_on_disk(&path);
        assert_eq!(
            on_disk, RECORDS_WANTED as usize,
            "all {RECORDS_WANTED} records must be on disk once the retired \
             writer is accounted for; found {on_disk}"
        );

        let _ = std::fs::remove_file(&path);
    }
}
