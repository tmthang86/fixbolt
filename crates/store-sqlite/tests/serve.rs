//! The store behind the real serving loop, over a socket: a session resumed
//! from the database, the secrets of a real logon kept out of it, and a quick
//! reconnect parked until the last session's writer has let the database go.
//!
//! Patterns copied from `crates/engine/tests/on_disk.rs` (a `Recovery` over a
//! path), `secrets_stay_off_disk.rs` (the secrets exchange) and
//! `one_appender.rs::a_reconnect_resumes_from_the_finished_file` (the parked
//! reconnect). **Every database file is read after its server has shut down
//! and been joined**: `serve*` waits for retired writers after serving, so
//! by then no connection is open on it (ADR-0180 decision 5).
//!
//! `standard` only: `serve*` builds the blocking engine.
//!
//! Plan `docs/plans/2026-09-24-p4-sqlite-store.md`, *Chia việc* row 2.
#![cfg(all(feature = "sqlite", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use fixbolt_engine::journal::Released;
use fixbolt_engine::observe::{Admin, Handles};
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::recovery::{Recovery, Resumed};
use fixbolt_engine::{Application, Config};
use fixbolt_store_sqlite::{SqliteJournal, SqliteOptions};

type Db = SqliteJournal<64, 1024>;

/// The tests bind ports; one at a time.
static TURNS: Mutex<()> = Mutex::new(());

fn my_turn() -> MutexGuard<'static, ()> {
    TURNS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

fn scratch(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "fixbolt-store-sqlite-serve-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("scratch dir");
    p
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// A recovery over one database path, as `GUIDE.md` §6b shows it for a file:
/// it keeps the `released` handle of the journal it handed out, and `ready`
/// is one atomic load (ADR-0155 decision 3).
struct OnDb {
    path: PathBuf,
    handed_out: Option<Released>,
    asked: Arc<AtomicUsize>,
    parked: Arc<AtomicUsize>,
}

impl OnDb {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            handed_out: None,
            asked: Arc::new(AtomicUsize::new(0)),
            parked: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Recovery<Db> for OnDb {
    fn ready(&mut self, _cfg: &Config) -> bool {
        self.asked.fetch_add(1, Ordering::Relaxed);
        let ready = self.handed_out.as_ref().is_none_or(Released::is_released);
        if !ready {
            self.parked.fetch_add(1, Ordering::Relaxed);
        }
        ready
    }

    fn fresh(&mut self, cfg: &Config) -> Db {
        let j = Db::open(&self.path, cfg, SqliteOptions::default())
            .unwrap_or_else(|e| panic!("open {}: {e}", self.path.display()));
        self.handed_out = Some(j.released());
        j
    }

    fn recover(&mut self, cfg: &Config) -> Option<Resumed<Db>> {
        Resumed::from_journal(self.fresh(cfg))
    }
}

/// A server on its own thread, and the handle that stops it.
fn serve<A: Application + Send + 'static>(
    addr: &str,
    app: A,
    recovery: OnDb,
) -> (Admin, JoinHandle<()>) {
    let handles = Handles::new();
    let admin = handles.admin();
    let serving = addr.to_owned();
    let server = std::thread::spawn(move || {
        let table = Table::with_capacity(1).serving(cfg());
        let _ = fixbolt_engine::serve_with_recovery(
            &serving,
            table,
            app,
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            recovery,
            fixbolt_engine::msglog::NoLog,
            handles,
        );
    });
    (admin, server)
}

/// Stop a server and wait for it: after this, its writers have finished.
fn stop(admin: &Admin, server: JoinHandle<()>) {
    admin.shutdown(0);
    server.join().expect("the server thread");
}

fn free_port() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

fn connect(addr: &str) -> TcpStream {
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

/// `SendingTime` now: the serving loop uses the real clock.
fn now_stamp() -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    cache.format(now, 0)[..21].to_vec()
}

/// A whole FIX 4.4 message: `8=`, `9=` over `body`, `body`, a real `10=`.
fn frame(body: &[u8]) -> Vec<u8> {
    let mut m = format!("8=FIX.4.4\u{1}9={}\u{1}", body.len()).into_bytes();
    m.extend_from_slice(body);
    let sum: u8 = m.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    m.extend_from_slice(format!("10={sum:03}\u{1}").as_bytes());
    m
}

fn field(out: &mut Vec<u8>, tag: u32, value: &[u8]) {
    out.extend_from_slice(format!("{tag}=").as_bytes());
    out.extend_from_slice(value);
    out.push(0x01);
}

/// A message from `TW44` to `ISLD`: the header, then `rest` (tag, value).
fn msg(kind: &[u8], seq: u32, rest: &[(u32, &[u8])]) -> Vec<u8> {
    let mut b = Vec::new();
    field(&mut b, 35, kind);
    field(&mut b, 34, seq.to_string().as_bytes());
    field(&mut b, 49, b"TW44");
    field(&mut b, 52, &now_stamp());
    field(&mut b, 56, b"ISLD");
    for (tag, value) in rest {
        field(&mut b, *tag, value);
    }
    frame(&b)
}

fn logon(seq: u32) -> Vec<u8> {
    msg(b"A", seq, &[(98, b"0"), (108, b"30")])
}

fn order(seq: u32, id: &[u8]) -> Vec<u8> {
    msg(
        b"D",
        seq,
        &[
            (11, id),
            (21, b"1"),
            (38, b"100"),
            (40, b"1"),
            (54, b"1"),
            (55, b"FXB"),
            (60, &now_stamp()),
        ],
    )
}

/// Read until `seen` holds `needle`, or five seconds pass.
fn read_until(client: &mut TcpStream, seen: &mut Vec<u8>, needle: &[u8]) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut buf = [0u8; 4096];
    while Instant::now() < deadline {
        if contains(seen, needle) {
            return true;
        }
        match client.read(&mut buf) {
            Ok(0) => return contains(seen, needle),
            Ok(n) => seen.extend_from_slice(&buf[..n]),
            Err(_) => {}
        }
    }
    contains(seen, needle)
}

/// The whole messages in `wire` whose `35=` is `kind`.
fn messages_of<'a>(wire: &'a [u8], kind: &[u8]) -> Vec<&'a [u8]> {
    let needle = [b"\x0135=".as_slice(), kind, b"\x01"].concat();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = find(&wire[at..], b"8=FIX.4.4\x01").map(|i| i + at) {
        let end = find(&wire[start..], b"\x0110=")
            .map(|i| start + i + 8)
            .unwrap_or(wire.len())
            .min(wire.len());
        let one = &wire[start..end];
        if contains(one, &needle) {
            out.push(one);
        }
        at = end;
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// A message's fields without the ones a resend rewrites or a frame
/// recomputes: what must come back **byte for byte**.
fn application_fields(m: &[u8]) -> Vec<&[u8]> {
    m.split(|b| *b == 0x01)
        .filter(|f| !f.is_empty())
        .filter(|f| {
            let tag = f.split(|b| *b == b'=').next().unwrap_or_default();
            !matches!(
                tag,
                b"8" | b"9" | b"10" | b"34" | b"43" | b"52" | b"97" | b"122"
            )
        })
        .collect()
}

/// An application that answers an order with an echo of it
/// (`fixbolt_conformance::echo::Echo`).
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

/// **The point of the store.** A session answers an order, the server stops;
/// a second server over the same database resumes the session and answers a
/// `ResendRequest` for that reply with `43=Y` and the reply's own bytes — read
/// back from the database, since nothing else survived.
#[test]
fn a_session_resumed_from_the_database_replays_what_it_sent() {
    let _turn = my_turn();
    let path = scratch("resume").join("TW44.db");

    // Run one: log on, one order, one reply, hang up, stop.
    let addr = free_port();
    let (admin, server) = serve(&addr, EchoApp::default(), OnDb::new(&path));
    let mut client = connect(&addr);
    let mut seen = Vec::new();
    client.write_all(&logon(1)).expect("send Logon");
    assert!(
        read_until(&mut client, &mut seen, b"\x0135=A\x01"),
        "premise: logged on: {:?}",
        String::from_utf8_lossy(&seen)
    );
    seen.clear();
    client.write_all(&order(2, b"RESUME1")).expect("send order");
    assert!(
        read_until(&mut client, &mut seen, b"\x0111=RESUME1\x01"),
        "premise: the order was answered: {:?}",
        String::from_utf8_lossy(&seen)
    );
    let original = messages_of(&seen, b"D")
        .into_iter()
        .find(|m| contains(m, b"\x0111=RESUME1\x01"))
        .expect("the reply")
        .to_vec();
    assert!(
        contains(&original, b"\x0134=2\x01"),
        "premise: the reply went out as 34=2 (Logon was 1): {:?}",
        String::from_utf8_lossy(&original)
    );
    // Hang up and read until the engine lets go, as `one_appender.rs` does:
    // stopped before it has seen the hang-up, the engine's ordered shutdown
    // would spend 34=3 on a Logout (`[measured 2026-09-24]`: the database
    // then held highest_out 3, and run two's Logon went out as 34=4).
    hang_up(client);
    stop(&admin, server);

    // Run two: a new server, the same database.
    let addr = free_port();
    let (admin, server) = serve(&addr, EchoApp::default(), OnDb::new(&path));
    let mut client = connect(&addr);
    let mut seen = Vec::new();
    client.write_all(&logon(3)).expect("send Logon");
    assert!(
        read_until(&mut client, &mut seen, b"\x0135=A\x01"),
        "the resumed session logged on at the next inbound number: {:?}",
        String::from_utf8_lossy(&seen)
    );
    assert!(
        contains(&seen, b"\x0134=3\x01"),
        "and its Logon went out as 34=3, after the 2 already sent: {:?}",
        String::from_utf8_lossy(&seen)
    );
    seen.clear();
    client
        .write_all(&msg(b"2", 4, &[(7, b"2"), (16, b"2")]))
        .expect("send ResendRequest");
    let replayed = read_until(&mut client, &mut seen, b"\x0143=Y\x01");
    drop(client);
    stop(&admin, server);
    assert!(
        replayed,
        "the ResendRequest for 2 was answered with a replay: {:?}",
        String::from_utf8_lossy(&seen)
    );
    let replay = messages_of(&seen, b"D")
        .into_iter()
        .find(|m| contains(m, b"\x0143=Y\x01"))
        .unwrap_or_else(|| panic!("no replayed order in {:?}", String::from_utf8_lossy(&seen)));
    assert_eq!(
        application_fields(replay),
        application_fields(&original),
        "the replay carries the original's fields byte for byte"
    );
}

/// The secrets a real exchange carries — a `Logon` with `554`/`96`, and an
/// application reply carrying `554`/`925`/`96` — reach neither the database
/// nor its `-wal`; the `ExecutionReport` beside them does.
#[test]
fn secrets_through_a_real_logon_stay_out_of_the_database() {
    let _turn = my_turn();
    let home = scratch("secrets");
    let path = home.join("TW44.db");
    const S1: &[u8] = b"pwLOGONsqlQ7";
    const S2: &[u8] = b"rawLOGONsqlW2";
    const S3: &[u8] = b"pwUSERREQsqlM4";
    const S4: &[u8] = b"newpwUSERREQsqlV8";
    const S5: &[u8] = b"rawUSERREQsqlP3";

    /// Answers a `UserRequest` with one carrying three secrets, and an order
    /// with an `ExecutionReport` carrying none.
    struct App;
    impl Application for App {
        fn on_message(
            &mut self,
            msg: &[u8],
            hdr: fixbolt_session::Header<'_>,
            out: &mut [u8],
        ) -> Option<Range<usize>> {
            let mut b = Vec::new();
            if contains(msg, b"\x0135=BE\x01") {
                field(&mut b, 35, b"BE");
                field(&mut b, 34, hdr.seq.to_string().as_bytes());
                field(&mut b, 49, b"ISLD");
                field(&mut b, 52, hdr.stamp);
                field(&mut b, 56, b"TW44");
                field(&mut b, 95, S5.len().to_string().as_bytes());
                field(&mut b, 96, S5);
                field(&mut b, 553, b"alice");
                field(&mut b, 554, S3);
                field(&mut b, 923, b"UR1");
                field(&mut b, 924, b"3");
                field(&mut b, 925, S4);
            } else if contains(msg, b"\x0135=D\x01") {
                field(&mut b, 35, b"8");
                field(&mut b, 34, hdr.seq.to_string().as_bytes());
                field(&mut b, 49, b"ISLD");
                field(&mut b, 52, hdr.stamp);
                field(&mut b, 56, b"TW44");
                for (tag, v) in [
                    (6, b"0".as_slice()),
                    (11, b"ORD1"),
                    (14, b"0"),
                    (17, b"EXEC1"),
                    (37, b"ORDER1"),
                    (39, b"0"),
                    (54, b"1"),
                    (55, b"FXB"),
                    (150, b"0"),
                    (151, b"100"),
                ] {
                    field(&mut b, tag, v);
                }
            } else {
                return None;
            }
            let reply = frame(&b);
            out.get_mut(..reply.len())?.copy_from_slice(&reply);
            Some(0..reply.len())
        }
    }

    let addr = free_port();
    let (admin, server) = serve(&addr, App, OnDb::new(&path));
    let mut client = connect(&addr);
    let mut seen = Vec::new();
    client
        .write_all(&msg(
            b"A",
            1,
            &[
                (95, S2.len().to_string().as_bytes()),
                (96, S2),
                (98, b"0"),
                (108, b"30"),
                (553, b"alice"),
                (554, S1),
            ],
        ))
        .expect("send Logon");
    assert!(
        read_until(&mut client, &mut seen, b"\x0135=A\x01"),
        "premise: logged on: {:?}",
        String::from_utf8_lossy(&seen)
    );
    client
        .write_all(&msg(
            b"BE",
            2,
            &[
                (95, S5.len().to_string().as_bytes()),
                (96, S5),
                (553, b"alice"),
                (554, S3),
                (923, b"UR1"),
                (924, b"3"),
                (925, S4),
            ],
        ))
        .expect("send UserRequest");
    let s3_on_the_wire = [b"\x01554=".as_slice(), S3, b"\x01"].concat();
    assert!(
        read_until(&mut client, &mut seen, &s3_on_the_wire),
        "premise: the reply carried the password on the wire: {:?}",
        String::from_utf8_lossy(&seen)
    );
    client.write_all(&order(3, b"ORD1")).expect("send order");
    assert!(
        read_until(&mut client, &mut seen, b"\x0135=8\x01"),
        "premise: the order was answered: {:?}",
        String::from_utf8_lossy(&seen)
    );
    // Inside the process a resend replays the secret verbatim: memory keeps it.
    seen.clear();
    client
        .write_all(&msg(b"2", 4, &[(7, b"2"), (16, b"2")]))
        .expect("send ResendRequest");
    let replayed = read_until(&mut client, &mut seen, b"\x0143=Y\x01")
        && read_until(&mut client, &mut seen, &s3_on_the_wire);
    drop(client);
    stop(&admin, server);
    assert!(
        replayed,
        "a resend inside the process replays the UserRequest with the real password: {:?}",
        String::from_utf8_lossy(&seen)
    );

    let mut disk = std::fs::read(&path).expect("the database");
    disk.extend(std::fs::read(home.join("TW44.db-wal")).unwrap_or_default());
    assert!(
        contains(&disk, b"\x0135=8\x01"),
        "premise: the ExecutionReport is in the database"
    );
    let leaked: Vec<&str> = [
        ("S1 (554 on Logon)", S1),
        ("S2 (96 on Logon)", S2),
        ("S3 (554 on UserRequest)", S3),
        ("S4 (925 on UserRequest)", S4),
        ("S5 (96 on UserRequest)", S5),
    ]
    .iter()
    .filter(|(_, s)| contains(&disk, s))
    .map(|(name, _)| *name)
    .collect();
    assert!(
        leaked.is_empty(),
        "secrets in the database or its WAL: {leaked:?}"
    );
}

/// ADR-0154/0155 with the store: a counterparty that hangs up and logs on
/// again at once finds the last session's writer still committing; the
/// reconnect is **parked** — `ready` answers from `released()` — and admitted
/// once the writer lets go, resuming with numbers neither side has used.
#[test]
fn a_reconnect_while_the_writer_flushes_is_parked_then_admitted() {
    let _turn = my_turn();
    const ROUNDS: u32 = 20;
    let path = scratch("reconnect").join("TW44.db");
    let recovery = OnDb::new(&path);
    let (asked, parked) = (Arc::clone(&recovery.asked), Arc::clone(&recovery.parked));
    let addr = free_port();
    let (admin, server) = serve(&addr, EchoApp::default(), recovery);

    let mut client_seq = 1;
    let mut last_out: Option<u32> = None;
    let mut missed = Vec::new();
    for round in 0..ROUNDS {
        let mut s = connect(&addr);
        let mut seen = Vec::new();
        s.write_all(&logon(client_seq)).expect("send Logon");
        client_seq += 1;
        assert!(
            read_until(&mut s, &mut seen, b"\x0135=A\x01"),
            "round {round}: logged on: {:?}",
            String::from_utf8_lossy(&seen)
        );
        let got = first_seq(&seen);
        if let Some(last) = last_out
            && got <= last
        {
            missed.push(format!(
                "round {round}: Logon went out as 34={got}, 34={last} was already sent"
            ));
        }
        if contains(&seen, b"\x0135=2\x01") {
            missed.push(format!(
                "round {round}: the resumed session asked for a resend"
            ));
        } else {
            std::thread::sleep(Duration::from_millis(5));
            let id = format!("R{round}");
            s.write_all(&order(client_seq, id.as_bytes()))
                .expect("send order");
            client_seq += 1;
            let needle = format!("\x0111={id}\x01");
            assert!(
                read_until(&mut s, &mut seen, needle.as_bytes()),
                "round {round}: the order was answered"
            );
        }
        let _ = s.shutdown(std::net::Shutdown::Write);
        let mut buf = [0u8; 4096];
        loop {
            match s.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => seen.extend_from_slice(&buf[..n]),
            }
        }
        last_out = max_seq(&seen);
    }
    stop(&admin, server);
    assert!(
        missed.is_empty(),
        "{} misses in {ROUNDS} reconnects: {missed:#?}",
        missed.len()
    );
    assert!(
        asked.load(Ordering::Relaxed) >= ROUNDS as usize,
        "the engine asked `ready` before each recovery"
    );
    assert!(
        parked.load(Ordering::Relaxed) > 0,
        "no reconnect in {ROUNDS} found the last writer still committing: the case was not reached"
    );
}

/// Close our half, then read until the engine closes its own: after this
/// the engine has dropped the connection.
fn hang_up(mut client: TcpStream) {
    let _ = client.shutdown(std::net::Shutdown::Write);
    let mut buf = [0u8; 4096];
    while let Ok(n) = client.read(&mut buf) {
        if n == 0 {
            break;
        }
    }
}

fn seqs(wire: &[u8]) -> impl Iterator<Item = u32> + '_ {
    wire.split(|b| *b == 0x01)
        .filter_map(|f| f.strip_prefix(b"34="))
        .filter_map(|v| std::str::from_utf8(v).ok()?.parse().ok())
}

fn first_seq(wire: &[u8]) -> u32 {
    seqs(wire).next().expect("a MsgSeqNum")
}

fn max_seq(wire: &[u8]) -> Option<u32> {
    seqs(wire).max()
}
