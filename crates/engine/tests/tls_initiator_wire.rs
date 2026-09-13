//! An **initiator** that dials a TLS venue, through its own front door.
//!
//! **Step 5b of the `tls` plan** (`docs/plans/2026-09-04-tls.md`, Sửa 6, table
//! *Chia việc*, row 5b). Step 5a gave `TlsTransport` a client side; until
//! `connect_and_serve_tls` nothing could put one inside an initiator engine, so
//! an initiator could not speak TLS at all.
//!
//! # The trap this step exists for (plan 6.2 item 5)
//!
//! ADR-0060's check — raise `TlsFellBackToUserspace`, and refuse the connection
//! under `TlsRequireKernel=Y` — runs **when a connection is added to the
//! engine**, on the premise that the handover has already happened. That is
//! true for an acceptor, whose pre-session stage completes the handshake. The
//! initiator's `dial` loop used to add a socket **straight after `connect`**;
//! a client `TlsTransport` still mid-handshake reports `Userspace`, so every
//! dialled connection would raise a false fallback event and a deployment that
//! demanded the kernel would refuse all of them.
//!
//! | Trap (plan 6.9) | Test here |
//! |---|---|
//! | `dial` adds before the handshake is decided | `the_dial_loop_adds_no_connection_before_the_handshake_is_decided` |
//! | A venue that accepts TCP and never speaks hangs `dial` for ever | `a_counterparty_that_accepts_and_never_speaks_tls_is_dropped_at_the_deadline` |
//! | A handshake that really fell back is not reported, or not refused | `an_initiator_that_fell_back_is_reported_and_refused_when_the_kernel_was_demanded` |
//! | `standard` spins while the handshake waits on the venue | `the_dial_loop_sleeps_rather_than_spins_while_the_handshake_waits` |
//!
//! # What this file does NOT prove
//!
//! - **That the engine thread never sleeps in the kernel during the handshake
//!   in `hft` mode.** `connect_and_serve_tls` is `standard` only, and
//!   `scripts/check-no-kernel-sleep.sh` traces `tools/w2w`, an acceptor; it
//!   cannot see this loop at all.
//! - **No latency and no allocation count.** The handshake is the carve-out of
//!   non-negotiable 1; after it the session path is the one `tls_client.rs`
//!   already exercises.
#![cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`. A
// setup step that cannot be completed is a failing test, which is the point.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::indexing_slicing)]

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use fixbolt_engine::observe::{EventKind, Handles, Observer};
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::reconnect::Policy;
use fixbolt_engine::tls::{Client, ClientTls, TlsProbe, TlsTransport};
use fixbolt_engine::{Application, Config, ServeError, Shutdown};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};

/// `dial` runs the whole initiator on the caller's thread and no step of this
/// design moves a transport across threads — but a caller that builds its own
/// engine may, and 5a left the question open. Answered by the compiler.
const _: fn() = || {
    fn is_send<T: Send>() {}
    is_send::<TlsTransport<Client>>();
};

/// Everything here is administrative; no application message is ever sent.
struct Never;

impl Application for Never {
    fn on_message(
        &mut self,
        _msg: &[u8],
        _hdr: fixbolt_session::Header<'_>,
        _out: &mut [u8],
    ) -> Option<Range<usize>> {
        None
    }
}

fn initiator_cfg() -> Config {
    Config::initiator(b"FIX.4.4", b"FIXBOLT", b"VENUE").with_heart_bt_int(30)
}

fn venue_cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"VENUE", b"FIXBOLT")
}

fn free_addr() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

fn pki() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("a self-signed certificate");
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der());
    (ck.cert.der().clone(), PrivateKeyDer::Pkcs8(key))
}

fn client_tls(root: CertificateDer<'static>, require_kernel: bool) -> ClientTls {
    ClientTls {
        roots: vec![root],
        identity: None,
        server_name: ServerName::try_from("localhost").expect("a valid server name"),
        require_kernel,
    }
}

/// `/proc/net/tls_stat`'s cumulative counters, as `tests/tls_client.rs` reads
/// them.
fn tls_stat(key: &str) -> u64 {
    let Ok(text) = std::fs::read_to_string("/proc/net/tls_stat") else {
        return 0;
    };
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() == Some(key) {
            return parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        }
    }
    0
}

/// **Every test in this file holds this.** `/proc/net/tls_stat` is one counter
/// for the namespace and the harness runs these tests on parallel threads, so
/// a handover in one test would move the counter another test reads.
static KERNEL_COUNTERS: Mutex<()> = Mutex::new(());

fn kernel_counters() -> MutexGuard<'static, ()> {
    KERNEL_COUNTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// A fixbolt TLS acceptor on `addr`: `serve_tls`, the door step 4a built.
fn venue(
    addr: &str,
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    handles: Handles,
) -> JoinHandle<Result<Shutdown, ServeError>> {
    let addr = addr.to_owned();
    std::thread::spawn(move || {
        fixbolt_engine::serve_tls(
            &addr,
            Table::with_capacity(1).serving(venue_cfg()),
            Never,
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            handles,
            vec![cert],
            key,
        )
    })
}

/// The initiator under test, through the full door so a test can name the probe.
fn initiator(
    addr: &str,
    cfg: Config,
    tls: ClientTls,
    probe: TlsProbe,
    handles: Handles,
) -> JoinHandle<Result<Shutdown, ServeError>> {
    let addr = addr.to_owned();
    std::thread::spawn(move || {
        fixbolt_engine::connect_and_serve_tls_with::<
            256,
            4096,
            8192,
            1024,
            _,
            fixbolt_engine::journal::Store,
            _,
            _,
        >(
            &addr,
            cfg,
            Never,
            Policy::new(50, 200).expect("a legal pair"),
            fixbolt_engine::recovery::NoRecovery,
            fixbolt_engine::msglog::NoLog,
            handles,
            tls,
            probe,
        )
    })
}

/// Collect events until every one of `kinds` has been seen, or time runs out.
///
/// **One accumulator per stream**: `Observer::events` drains the ring, so two
/// waits on one stream would share its events rather than each see them
/// (`tests/tls_mode.rs::wait_for_all` paid for that).
fn wait_for_all(observer: &Observer, kinds: &[EventKind], within: Duration) -> Vec<EventKind> {
    let mut seen = Vec::new();
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        let mut out = Vec::new();
        if observer.events(&mut out) > 0 {
            seen.extend(out.iter().map(fixbolt_engine::observe::Event::kind));
            if kinds.iter().all(|k| seen.contains(k)) {
                return seen;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    seen
}

/// Join a serving thread, or fail saying which one hung.
fn join_within<T>(handle: JoinHandle<T>, within: Duration, who: &str) -> T {
    let deadline = Instant::now() + within;
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        handle.is_finished(),
        "{who} did not return within {within:?} of Admin::shutdown"
    );
    handle.join().expect("the serving thread did not panic")
}

/// **fixbolt dials fixbolt over TLS, and both ends are on the kernel.**
///
/// Asserted three ways, because "a session came up" is passable by two engines
/// that never offloaded: `LoggedOn` on **both** streams, no fallback event on
/// either, and the kernel's own counters moving by one context per end.
#[test]
fn connect_and_serve_tls_brings_a_session_up_against_serve_tls() {
    let _counters = kernel_counters();
    let before_tx = tls_stat("TlsTxSw");
    let before_rx = tls_stat("TlsRxSw");

    let addr = free_addr();
    let (cert, key) = pki();

    let venue_handles = Handles::new();
    let venue_seen = venue_handles.observer();
    let venue_admin = venue_handles.admin();
    let venue_thread = venue(&addr, cert.clone(), key, venue_handles);

    let handles = Handles::new();
    let seen = handles.observer();
    let admin = handles.admin();
    let dialled = addr.clone();
    let tls = client_tls(cert, false);
    // The eight-parameter door, not the full one: it is the one a deployment
    // calls, and it is otherwise untested.
    let engine = std::thread::spawn(move || {
        fixbolt_engine::connect_and_serve_tls::<_, fixbolt_engine::journal::Store, _, _>(
            &dialled,
            initiator_cfg(),
            Never,
            Policy::new(50, 200).expect("a legal pair"),
            fixbolt_engine::recovery::NoRecovery,
            fixbolt_engine::msglog::NoLog,
            handles,
            tls,
        )
    });

    let ours = wait_for_all(&seen, &[EventKind::LoggedOn], Duration::from_secs(10));
    let theirs = wait_for_all(&venue_seen, &[EventKind::LoggedOn], Duration::from_secs(5));
    assert!(
        ours.contains(&EventKind::LoggedOn),
        "the initiator never logged on over TLS; its stream held {ours:?}"
    );
    assert!(
        theirs.contains(&EventKind::LoggedOn),
        "the venue never saw a session; its stream held {theirs:?}"
    );
    assert!(
        !ours.contains(&EventKind::TlsFellBackToUserspace)
            && !theirs.contains(&EventKind::TlsFellBackToUserspace),
        "an end reported a fallback on a kernel that offloads: initiator {ours:?}, venue {theirs:?}"
    );
    assert!(
        tls_stat("TlsTxSw") >= before_tx + 2,
        "/proc/net/tls_stat TlsTxSw moved by {} — both ends must have handed \
         their send keys to the kernel",
        tls_stat("TlsTxSw") - before_tx
    );
    assert!(
        tls_stat("TlsRxSw") >= before_rx + 2,
        "/proc/net/tls_stat TlsRxSw moved by {} — both ends must have handed \
         their receive keys to the kernel",
        tls_stat("TlsRxSw") - before_rx
    );

    admin.shutdown(2_000);
    let stopped = join_within(engine, Duration::from_secs(10), "connect_and_serve_tls");
    assert!(
        stopped.is_ok(),
        "connect_and_serve_tls came back with an error: {stopped:?}"
    );
    venue_admin.shutdown(0);
    let _ = join_within(venue_thread, Duration::from_secs(10), "serve_tls");
}

/// **A venue that accepts the TCP connection and never answers the
/// `ClientHello` is given up on at `LogonTimeout`, and the loop comes back.**
///
/// Without a deadline `dial` waits for that handshake for ever — no session
/// exists yet, so the session's own logon timer is not running. The venue here
/// is a bare `TcpListener`, so the only thing that can end the first
/// connection is this end.
///
/// The policy is not observable from outside, so "the policy was told" is read
/// from what it causes: **a second dial**. A loop that closed the socket and
/// forgot to call `Policy::dropped` would sit on `Next::Now` against a stale
/// `not_before` — and a loop that never closed it produces no second dial at
/// all. Every wait is bounded, so a hang is a red with a sentence rather than a
/// harness killed at sixty seconds (`docs/reference/a-reversal-can-fail-by-hanging.md`).
#[test]
fn a_counterparty_that_accepts_and_never_speaks_tls_is_dropped_at_the_deadline() {
    let _counters = kernel_counters();
    let (cert, _key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound").to_string();

    // (elapsed from accept to EOF, ciphertext bytes the initiator sent, ended by EOF)
    let (first_tx, first_rx) = std::sync::mpsc::channel::<(Duration, usize, bool)>();
    let (second_tx, second_rx) = std::sync::mpsc::channel::<()>();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let silent = std::thread::spawn(move || {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        let accepted = Instant::now();
        sock.set_read_timeout(Some(Duration::from_secs(10))).ok();
        let mut heard = 0usize;
        let mut buf = [0u8; 4096];
        let eof = loop {
            match sock.read(&mut buf) {
                Ok(0) => break true,
                Ok(n) => heard += n,
                Err(_) => break false,
            }
        };
        let _ = first_tx.send((accepted.elapsed(), heard, eof));
        let Ok((second, _)) = listener.accept() else {
            return;
        };
        let _ = second_tx.send(());
        // Hold the second one open, silent too, until the test is done.
        let _ = stop_rx.recv_timeout(Duration::from_secs(20));
        drop(second);
    });

    let handles = Handles::new();
    let admin = handles.admin();
    // `LogonTimeout=1`, in the unit `Config` carries.
    let cfg = initiator_cfg().with_logon_timeout_ms(1_000);
    let engine = initiator(&addr, cfg, client_tls(cert, false), TlsProbe::Real, handles);

    let (elapsed, heard, eof) = first_rx.recv_timeout(Duration::from_secs(15)).expect(
        "the initiator neither closed the silent connection nor let it time out: \
         dial has no deadline for a handshake the venue never answers",
    );
    assert!(
        heard > 0,
        "the initiator sent nothing on the socket, so it never started a TLS handshake"
    );
    assert!(
        eof,
        "the silent venue's read timed out after {elapsed:?} rather than seeing the \
         initiator close: nothing ended the handshake"
    );
    assert!(
        elapsed >= Duration::from_millis(900),
        "the connection was closed after {elapsed:?}, well before LogonTimeout=1 s: \
         something other than the deadline ended it"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the connection was closed only after {elapsed:?}, far past LogonTimeout=1 s"
    );
    second_rx.recv_timeout(Duration::from_secs(10)).expect(
        "the initiator closed the silent connection and never dialled again: the \
         reconnect policy was not told the connection ended",
    );

    // Shut down while the second handshake is still pending, which is the
    // one state in which the engine holds no connection.
    admin.shutdown(0);
    let stopped = join_within(
        engine,
        Duration::from_secs(10),
        "connect_and_serve_tls_with",
    );
    assert!(stopped.is_ok(), "came back with an error: {stopped:?}");
    let _ = stop_tx.send(());
    silent.join().expect("the silent venue did not panic");
}

/// **A dialled handshake that lands in userspace is reported, and ended when
/// the kernel was demanded** — ADR-0060 decision 1's per-connection half, for
/// the initiator.
///
/// The kernel here offloads, so the fallback is provoked with
/// [`TlsProbe::PretendHandshakeFallsBack`]; without it the path is code that
/// has never run.
#[test]
fn an_initiator_that_fell_back_is_reported_and_refused_when_the_kernel_was_demanded() {
    let _counters = kernel_counters();
    let addr = free_addr();
    let (cert, key) = pki();

    let venue_handles = Handles::new();
    let venue_admin = venue_handles.admin();
    let venue_thread = venue(&addr, cert.clone(), key, venue_handles);

    let handles = Handles::new();
    let seen = handles.observer();
    let admin = handles.admin();
    let engine = initiator(
        &addr,
        initiator_cfg(),
        client_tls(cert, true), // TlsRequireKernel=Y
        TlsProbe::PretendHandshakeFallsBack,
        handles,
    );

    let refused = EventKind::Ended(fixbolt_session::DropReason::RefusedByDeployment);
    let kinds = wait_for_all(
        &seen,
        &[EventKind::TlsFellBackToUserspace, refused],
        Duration::from_secs(10),
    );
    assert!(
        kinds.contains(&EventKind::TlsFellBackToUserspace),
        "the initiator's fallback was never reported; the stream held {kinds:?}"
    );
    assert!(
        kinds.contains(&refused),
        "TlsRequireKernel=Y reported the fallback and then served it anyway; \
         the stream held {kinds:?}"
    );
    assert!(
        !kinds.contains(&EventKind::LoggedOn),
        "a connection refused for TlsRequireKernel must not have logged on; \
         the stream held {kinds:?}"
    );
    // **And the refusal backs off.** A refused connection is gone within its
    // first turn, so a loop that judged "was it up" only after that turn would
    // never tell the policy, and would redial at once, for ever.
    // `[measured 2026-09-13]` over a 1.5 s hold, 7 dials with the ending
    // recorded at the add and 879 without it. The ladder here is 50 ms doubling
    // to 200 ms, so one second allows about six.
    let more = wait_for_all(&seen, &[EventKind::LoggedOn], Duration::from_secs(1));
    let refusals = more.iter().filter(|k| **k == refused).count();
    assert!(
        refusals <= 20,
        "{refusals} refused connections in one second against a 50-200 ms ladder: \
         the dial loop is not telling the policy a refused connection ended"
    );

    admin.shutdown(0);
    let stopped = join_within(
        engine,
        Duration::from_secs(10),
        "connect_and_serve_tls_with",
    );
    assert!(stopped.is_ok(), "came back with an error: {stopped:?}");
    venue_admin.shutdown(0);
    let _ = join_within(venue_thread, Duration::from_secs(10), "serve_tls");
}

/// **The trap of plan 6.2 item 5.** No `TlsFellBackToUserspace` may appear on
/// the stream of a connection whose handshake ends on the kernel.
///
/// `TlsRequireKernel=N` on purpose: under `Y` an early add would be refused and
/// the test would go red on the missing `LoggedOn` instead, which names the
/// symptom rather than the cause. Under `N` an early add still logs on — the
/// engine's own turns finish the handshake — so the only thing that separates
/// it from a correct loop is the event, and the event is what is asserted
/// first.
#[test]
fn the_dial_loop_adds_no_connection_before_the_handshake_is_decided() {
    let _counters = kernel_counters();
    let addr = free_addr();
    let (cert, key) = pki();

    let venue_handles = Handles::new();
    let venue_admin = venue_handles.admin();
    let venue_thread = venue(&addr, cert.clone(), key, venue_handles);

    let handles = Handles::new();
    let seen = handles.observer();
    let admin = handles.admin();
    let engine = initiator(
        &addr,
        initiator_cfg(),
        client_tls(cert, false),
        TlsProbe::Real,
        handles,
    );

    let kinds = wait_for_all(&seen, &[EventKind::LoggedOn], Duration::from_secs(10));
    assert!(
        !kinds.contains(&EventKind::TlsFellBackToUserspace),
        "a connection whose handshake ends on the kernel was reported as a fallback: \
         the dial loop added it to the engine before the handshake was decided. \
         The stream held {kinds:?}"
    );
    assert!(
        kinds.contains(&EventKind::LoggedOn),
        "no session came up, so the absence above proves nothing; the stream held {kinds:?}"
    );

    admin.shutdown(2_000);
    let stopped = join_within(
        engine,
        Duration::from_secs(10),
        "connect_and_serve_tls_with",
    );
    assert!(stopped.is_ok(), "came back with an error: {stopped:?}");
    venue_admin.shutdown(0);
    let _ = join_within(venue_thread, Duration::from_secs(10), "serve_tls");
}

// ---------------------------------------------------------------------------
// From a configuration file. Step 5c of the `tls` plan (Sửa 6, 6.4 item 6).
//
// Step 5b's door took a `ClientTls` built in Rust; a deployment configures
// fixbolt from a file. `Settings::into_tls_initiator` → `tls::load_client_pem`
// is the joint, and these two tests are what say it holds on a real socket —
// the initiator's counterpart of `tests/tls_settings_wire.rs`.
// ---------------------------------------------------------------------------

/// A directory of its own per test, removed on `Drop` so a failing assertion
/// does not leave PEMs behind.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "fixbolt-tls-initiator-wire-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("a scratch directory");
        Self(p)
    }

    fn path(&self, leaf: &str) -> std::path::PathBuf {
        self.0.join(leaf)
    }

    fn write(&self, leaf: &str, body: &str) -> std::path::PathBuf {
        let p = self.path(leaf);
        std::fs::write(&p, body).expect("the scratch file is writable");
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The `.cfg` an operator writes to dial a TLS venue.
fn initiator_cfg_text(host: &str, port: &str, extra: &str) -> String {
    format!(
        "[DEFAULT]\n\
         ConnectionType=initiator\n\
         BeginString=FIX.4.4\n\
         SenderCompID=FIXBOLT\n\
         SocketConnectHost={host}\n\
         SocketConnectPort={port}\n\
         ReconnectInterval=1\n\
         ReconnectCeiling=1\n\
         SocketUseSSL=Y\n\
         {extra}\
         [SESSION]\n\
         TargetCompID=VENUE\n"
    )
}

/// **The whole initiator path from a file: a `.cfg` on disk, and a FIX session
/// on an encrypted socket.**
///
/// `Settings::parse` → `into_tls_initiator` → `load_client_pem` →
/// `connect_and_serve_tls` against `serve_tls`. The venue's self-signed
/// certificate **is** the certification authority the file names, and the file
/// also names a client certificate pair, so the identity half of the loader
/// runs too — the venue asks for no client certificate, so this does not prove
/// a venue that demands one accepts it.
#[test]
fn a_configuration_file_brings_a_tls_initiator_up() {
    let _counters = kernel_counters();
    let scratch = Scratch::new("up");

    let venue_ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("a self-signed certificate");
    let ca_path = scratch.write("venue-ca.pem", &venue_ck.cert.pem());
    let client_ck = rcgen::generate_simple_self_signed(vec!["fixbolt-client".to_string()])
        .expect("a client certificate");
    let client_cert = scratch.write("client.pem", &client_ck.cert.pem());
    let client_key = scratch.write("client.key", &client_ck.signing_key.serialize_pem());

    let addr = free_addr();
    let (_, port) = addr.rsplit_once(':').expect("host:port");
    let text = initiator_cfg_text(
        "localhost",
        port,
        &format!(
            "CertificationAuthoritiesFile={}\nClientCertificateFile={}\nClientCertificateKeyFile={}\n",
            ca_path.display(),
            client_cert.display(),
            client_key.display()
        ),
    );
    let cfg_path = scratch.write("initiator.cfg", &text);

    // Read back from disk, so the file is what is under test.
    let on_disk = std::fs::read_to_string(&cfg_path).expect("the .cfg is readable");
    let settings = fixbolt_engine::settings::Settings::parse(&on_disk)
        .unwrap_or_else(|e| panic!("the .cfg does not parse: {e}"));
    let (cfg, dial, policy, tls) = settings
        .into_tls_initiator()
        .unwrap_or_else(|e| panic!("the TLS initiator door refused the file: {e}"));
    assert_eq!(
        dial,
        format!("localhost:{port}"),
        "the host survives as written"
    );
    assert_eq!(
        tls.ca(),
        ca_path.as_path(),
        "the path the file named came back"
    );
    assert!(
        !tls.require_kernel(),
        "TlsRequireKernel is absent, so off — ADR-0060"
    );

    let client = fixbolt_engine::tls::load_client_pem(&tls, "localhost")
        .unwrap_or_else(|e| panic!("load_client_pem refused the PEM written by rcgen: {e}"));
    assert_eq!(client.roots.len(), 1, "one certification authority");
    assert_eq!(
        client.identity.as_ref().map(|(chain, _)| chain.len()),
        Some(1),
        "the client certificate pair the file named was read"
    );

    let venue_der = venue_ck.cert.der().clone();
    let venue_key = PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
        venue_ck.signing_key.serialize_der(),
    ));
    let venue_handles = Handles::new();
    let venue_seen = venue_handles.observer();
    let venue_admin = venue_handles.admin();
    let venue_thread = venue(&addr, venue_der, venue_key, venue_handles);

    let handles = Handles::new();
    let seen = handles.observer();
    let admin = handles.admin();
    let engine = std::thread::spawn(move || {
        fixbolt_engine::connect_and_serve_tls::<_, fixbolt_engine::journal::Store, _, _>(
            &dial,
            cfg,
            Never,
            policy,
            fixbolt_engine::recovery::NoRecovery,
            fixbolt_engine::msglog::NoLog,
            handles,
            client,
        )
    });

    let ours = wait_for_all(&seen, &[EventKind::LoggedOn], Duration::from_secs(10));
    let theirs = wait_for_all(&venue_seen, &[EventKind::LoggedOn], Duration::from_secs(5));
    assert!(
        ours.contains(&EventKind::LoggedOn),
        "the initiator configured from a file never logged on over TLS; its stream held {ours:?}"
    );
    assert!(
        theirs.contains(&EventKind::LoggedOn),
        "the venue never saw the session; its stream held {theirs:?}"
    );

    admin.shutdown(2_000);
    let stopped = join_within(engine, Duration::from_secs(10), "connect_and_serve_tls");
    assert!(stopped.is_ok(), "came back with an error: {stopped:?}");
    venue_admin.shutdown(0);
    let _ = join_within(venue_thread, Duration::from_secs(10), "serve_tls");
}

/// **A certification-authority path that names no file says so, and names the
/// key** — before anything dials. It must not read like "the PEM was empty":
/// one is a typo in a path, the other is the wrong file at a right path.
#[test]
fn a_ca_path_that_names_no_file_is_refused_by_name() {
    let scratch = Scratch::new("noca");
    let missing = scratch.path("not-here-ca.pem");
    let text = initiator_cfg_text(
        "localhost",
        "9880",
        &format!("CertificationAuthoritiesFile={}\n", missing.display()),
    );

    let settings =
        fixbolt_engine::settings::Settings::parse(&text).unwrap_or_else(|e| panic!("{e}"));
    let (_, _, _, tls) = settings
        .into_tls_initiator()
        .unwrap_or_else(|e| panic!("{e}"));

    let err = fixbolt_engine::tls::load_client_pem(&tls, "localhost")
        .expect_err("a certification authority that is not on disk cannot verify anything");
    let said = format!("{err}");
    assert!(
        said.contains("CertificationAuthoritiesFile") && said.contains("not-here-ca.pem"),
        "the refusal names neither the key nor the path an operator must fix: {said}"
    );
    assert!(
        said.contains("could not be opened"),
        "a missing file must not read like a file with the wrong contents: {said}"
    );
}

// ---------------------------------------------------------------------------
// Non-negotiable 4's second half, for the dial loop.
//
// `[added 2026-09-13]` a senior review read `connect_and_serve_tls`'s rustdoc —
// "the loop idles on the socket's readiness through the engine's own wait
// strategy, so `standard` sleeps rather than spins" — and found no test behind
// it, and this file's own header saying "nothing here measures CPU". Prose does
// not hold a constraint (CLAUDE.md §4): either something reads it or the
// sentence goes. This is the something.
//
// `scripts/check-standard-gives-the-core-back.sh` cannot cover this loop: it
// traces `tools/w2w`, which is an acceptor, and there is no binary that dials.
// So the measurement is the script's, in Rust, on the one thread that matters.
// ---------------------------------------------------------------------------

/// USER_HZ. `utime` and `stime` in `/proc/<pid>/task/<tid>/stat` are in these
/// units on Linux whatever `CONFIG_HZ` the kernel was built with, which is what
/// `getconf CLK_TCK` reports and what
/// `scripts/check-standard-gives-the-core-back.sh` reads from it.
const CLK_TCK: f64 = 100.0;

/// How long the engine thread's CPU is measured for.
///
/// Three seconds is thirty of the `standard` engine's 100 ms poll timeouts, so
/// a sleeping thread has done thirty full turns inside the window and the
/// figure is not one about a thread that happened to be between wakeups.
const CPU_WINDOW: Duration = Duration::from_secs(3);

/// How many times the thread's scheduler state is read across that window.
const CPU_SAMPLES: usize = 30;

/// **The ceiling, and why it is this number and not a tighter one.**
///
/// `[measured 2026-09-13]` on this desk the loop reads **0.00%** as written and
/// **100.00%** with the `idle_with` call removed, so anything between the two
/// separates them. The gap is 100 points wide and the ceiling sits at 20, which
/// buys margin in both directions:
///
/// * **Against a false red.** A sleeping thread's cost is thirty turns in three
///   seconds; it is 0.00% here and would have to become two hundred times
///   busier to reach this line. A loaded machine does not make a sleeping
///   thread burn CPU — it makes it wait longer, which shows up as *less*.
/// * **Against a false green.** A spinning thread asks for a whole core. To
///   read under 20% it would have to be given less than a fifth of one, which
///   needs a machine oversubscribed more than fivefold — and this test binary's
///   own tests are serialised by [`kernel_counters`], so the contention it can
///   create is bounded.
const CPU_CEILING_PCT: f64 = 20.0;

/// `utime + stime` in clock ticks, and the scheduler state letter, for one
/// thread of this process — `None` if the thread is gone.
///
/// **Counted from the last `)`.** The `comm` field is parenthesised and may
/// itself contain spaces, so a field index taken from the start of the line is
/// wrong for any thread whose name has one.
fn task_cpu_and_state(tid: &str) -> Option<(u64, char)> {
    let text = std::fs::read_to_string(format!("/proc/self/task/{tid}/stat")).ok()?;
    let (_, rest) = text.rsplit_once(')')?;
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    // After the state (field 3) come ppid, pgrp, session, tty_nr, tpgid, flags,
    // minflt, cminflt, majflt, cmajflt — ten — and then utime (14), stime (15).
    let utime: u64 = fields.nth(10)?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some((utime.saturating_add(stime), state))
}

/// Accept one connection within `within`, or say nothing came.
fn accept_within(listener: &TcpListener, within: Duration) -> Option<TcpStream> {
    let deadline = Instant::now() + within;
    loop {
        match listener.accept() {
            Ok((sock, _)) => return Some(sock),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
}

/// **`standard` sleeps rather than spins while the initiator's TLS handshake
/// waits on the venue** — non-negotiable 4's second half, on the one loop no
/// script can reach.
///
/// The venue is a bare `TcpListener`: it accepts the TCP connection, never
/// answers the `ClientHello`, and `LogonTimeout` is off, so the dial loop sits
/// in exactly the state the rustdoc describes for as long as the measurement
/// needs. The engine thread announces its own tid before it starts, and its
/// `utime + stime` is read from `/proc` either side of a [`CPU_WINDOW`].
///
/// # Four assertions, because a low CPU figure is passable by three broken loops
///
/// The lesson is `scripts/check-standard-gives-the-core-back.sh`'s, and it is
/// the same lesson here:
///
/// 1. **CPU under [`CPU_CEILING_PCT`]** — a loop that spins fails this and
///    nothing else.
/// 2. **The thread was found sleeping** at least once across [`CPU_SAMPLES`]
///    reads of its scheduler state. A thread that has *died* also costs 0%.
/// 3. **It never logged on** inside the window, so the figure is about the
///    handshake and not about an idle established session, which is a different
///    branch of the same loop (`engine.idle()`).
/// 4. **It was still wakeable afterwards.** The venue's socket is closed and a
///    second dial must arrive — a loop asleep on a timeout it never notices
///    passes 1, 2 and 3 and is not the loop the rustdoc claims.
///
/// `[measured 2026-09-13]` as written: 0.00%, found sleeping 30 times out of
/// 30. With the `idle_with(&[Interest::readable(source)])` arm of `dial`
/// deleted: 100.00%, found sleeping 0 times out of 30.
#[test]
fn the_dial_loop_sleeps_rather_than_spins_while_the_handshake_waits() {
    let _counters = kernel_counters();
    let (cert, _key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    listener
        .set_nonblocking(true)
        .expect("a bounded accept, so a hang is a red with a sentence");
    let addr = listener.local_addr().expect("bound").to_string();

    let (tid_tx, tid_rx) = std::sync::mpsc::channel::<String>();
    let handles = Handles::new();
    let seen = handles.observer();
    let admin = handles.admin();
    let dialled = addr.clone();
    let tls = client_tls(cert, false);
    // `LogonTimeout=0` — off — so nothing but this test ends the handshake.
    let cfg = initiator_cfg().with_logon_timeout_ms(0);
    let engine = std::thread::spawn(move || {
        // The tid of the thread that runs `dial`, from inside it: the dial loop
        // is the caller's thread, so this is the thread under measurement.
        // `/proc/thread-self` resolves to `<pid>/task/<tid>`, as `tools/w2w`
        // reads it — no dependency and no `gettid` binding.
        let link = std::fs::read_link("/proc/thread-self").expect("/proc is mounted");
        let path = link.to_string_lossy().into_owned();
        let tid = path.rsplit('/').next().unwrap_or_default().to_owned();
        let _ = tid_tx.send(tid);
        fixbolt_engine::connect_and_serve_tls::<_, fixbolt_engine::journal::Store, _, _>(
            &dialled,
            cfg,
            Never,
            Policy::new(50, 200).expect("a legal pair"),
            fixbolt_engine::recovery::NoRecovery,
            fixbolt_engine::msglog::NoLog,
            handles,
            tls,
        )
    });

    let tid = tid_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the engine thread never announced its tid");
    let silent = accept_within(&listener, Duration::from_secs(10))
        .expect("the initiator never dialled, so there is no handshake to measure");
    // Read nothing from `silent`: the `ClientHello` stays in the kernel's
    // buffer, this end sends nothing back, and the initiator's socket stays
    // unreadable — which is the state under test.
    //
    // Settle first. The dial itself builds a rustls connection and writes, and
    // that work is real but is not what the rustdoc claims.
    std::thread::sleep(Duration::from_millis(300));

    let (before, _) = task_cpu_and_state(&tid).expect("the engine thread's /proc/.../stat");
    let started = Instant::now();
    let mut sleeping = 0usize;
    let mut alive = 0usize;
    let mut states = String::new();
    for _ in 0..CPU_SAMPLES {
        std::thread::sleep(CPU_WINDOW / u32::try_from(CPU_SAMPLES).unwrap_or(1));
        if let Some((_, state)) = task_cpu_and_state(&tid) {
            alive += 1;
            states.push(state);
            if state == 'S' {
                sleeping += 1;
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    let (after, _) =
        task_cpu_and_state(&tid).expect("the engine thread vanished during the window");
    let pct = 100.0 * (after - before) as f64 / CLK_TCK / elapsed;

    // 3, before the two that are about the window: a session that came up
    // would make the figure be about `engine.idle()` instead.
    let mut events = Vec::new();
    seen.events(&mut events);
    let kinds: Vec<EventKind> = events
        .iter()
        .map(fixbolt_engine::observe::Event::kind)
        .collect();
    assert!(
        !kinds.contains(&EventKind::LoggedOn),
        "a session came up against a venue that never spoke TLS, so the window \
         above is not about a waiting handshake: {kinds:?}"
    );
    // Then that the measurement happened at all: a thread that has gone costs
    // 0% CPU and reads no state, and both of the assertions below would pass
    // on nothing.
    assert!(
        alive > 0,
        "the engine thread could not be read at all across the window, so \
         nothing was measured"
    );
    // 1 — the assertion a spinning loop fails, and the one the rustdoc claims.
    // Ahead of 2 so that a spin is named as a spin, with its figure, rather
    // than as "never found sleeping".
    assert!(
        pct < CPU_CEILING_PCT,
        "the dial loop burned {pct:.2}% of a core over {elapsed:.2} s while \
         waiting for a venue that never answered, against a ceiling of \
         {CPU_CEILING_PCT}% — it is spinning, not idling on the socket's \
         readiness, and `standard` must give the core back (non-negotiable 4, \
         ADR-0013). Found sleeping {sleeping} of {alive} reads ({states})"
    );
    // 2.
    assert!(
        sleeping > 0,
        "the engine thread was never found sleeping across {alive} reads \
         ({states}) — 0% CPU is also what a thread that has died reads, and a \
         thread always in R is spinning below this test's ceiling"
    );

    // 4. Asleep is not the same as unwakeable: close the venue's socket, and
    // the loop must notice the ending and dial again.
    drop(silent);
    assert!(
        accept_within(&listener, Duration::from_secs(10)).is_some(),
        "the dial loop slept through the venue closing its socket: {pct:.2}% \
         of a core is what an engine that never wakes reads too"
    );

    admin.shutdown(0);
    let stopped = join_within(engine, Duration::from_secs(10), "connect_and_serve_tls");
    assert!(stopped.is_ok(), "came back with an error: {stopped:?}");
    println!(
        "dial-loop idle: {pct:.2}% of a core over {elapsed:.2} s, found \
         sleeping {sleeping}/{alive} ({states})"
    );
}
