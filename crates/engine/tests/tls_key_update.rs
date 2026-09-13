//! A TLS 1.3 **KeyUpdate from the counterparty** is handled on the kernel path,
//! on both sides, and handling it does not grow a buffer on the engine thread.
//!
//! **Step 6c of the `tls` plan** (`docs/plans/2026-09-04-tls.md`, Sửa 6, table
//! *Chia việc*, row 6c) — the phase-1 answer to ADR-0005 open question 6: *a
//! KeyUpdate from the counterparty is handled and tested; the engine never
//! initiates one.*
//!
//! # The defect this file was written against
//!
//! ktls-core 0.0.5 answers a KeyUpdate with a fatal `internal_error` alert
//! unless its feature `tls13-key-update` is on (`context.rs:418-432`), and the
//! engine enabled only `shim-rustls`. A counterparty's TLS library sends a
//! KeyUpdate when *it* reaches its own record limit, so a long enough session
//! died for no reason either end would name. The two survival tests below were
//! run **red** against that build before the feature was turned on.
//!
//! # What proves each test is about the kernel
//!
//! "A reply came back" is passable by a KeyUpdate that was never sent, so every
//! test here also reads `/proc/net/tls_stat`: `TlsRxRekeyReceived` says the
//! kernel saw the KeyUpdate and paused decryption, `TlsRxRekeyOk` says this
//! engine gave it the new receive key, and `TlsTxRekeyOk` says it rekeyed its
//! own send side in answer to `update_requested` — which rustls always asks for.
//!
//! # What this file does NOT prove
//!
//! - **That the engine ever initiates a KeyUpdate.** It does not, by decision;
//!   the AES-GCM record limit that makes that matter is a documentation item.
//! - **A kernel without TLS 1.3 rekey.** There `setsockopt(TLS_RX)` on a keyed
//!   socket is `EBUSY`. The engine cannot report that errno — ktls-core turns
//!   it into an alert and `Io::Failed` — so the tests infer it from the kernel
//!   having no `TlsRxRekey*` counters at all, and say so in a sentence of
//!   their own. No such kernel was available to run that sentence red.
//! - **That a KeyUpdate allocates literally nothing.** rustls's key schedule
//!   boxes four HKDF expanders per rekey, which is not this engine's code and
//!   is ADR-0063's second named carve-out from non-negotiable 1. The
//!   allocation test counts those four **exactly** and asserts nothing else.
//!   A client's session tickets, which rustls would store, are dropped unread
//!   and count zero (ADR-0063 decision 1, step 6c-2).
//! - **What a rekey costs in latency.** Nothing here times one.
#![cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`. A
// setup step that cannot be completed is a failing test, which is the point.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::indexing_slicing)]
// The counting allocator. Sound for the reasons `benches/alloc.rs` gives: every
// method forwards to `System` unchanged but for a relaxed counter behind a
// thread-local flag; this is a test binary, so nothing ships it; and its
// counter is proven live twice — by `the_counter_is_live` inside the test, and
// by reversal: `Context::new(kconn, None)` in `tls.rs` read
// `Window { count: 5, largest: 65540 }` on the acceptor side and
// `Window { count: 17, largest: 65540 }` over a client's session tickets
// (`[measured 2026-09-13]`, step 6c, before the tickets were dropped unread).
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use fixbolt_engine::observe::{EventKind, Handles, Observer};
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::reconnect::Policy;
use fixbolt_engine::tls::{Client, ClientTls, Handshake, TlsMode, TlsTransport};
use fixbolt_engine::transport::{Io, TcpTransport, Transport};
use fixbolt_engine::{Application, Config, ServeError, Shutdown};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};

// ---------------------------------------------------------------------------
// Counting, per thread
// ---------------------------------------------------------------------------

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static LARGEST: AtomicUsize = AtomicUsize::new(0);

/// How many individual allocation **sizes** one counting window records.
///
/// `[measured 2026-09-13]` a senior review found the failure messages here
/// diagnosable only with a debugger: they printed the count and the largest
/// size, so a window of five said nothing about what the fifth allocation was.
/// Every size in the window is recorded instead, up to this many — the window
/// under test holds four, and a window that holds more is the failure this is
/// here to name. Fixed size, in statics: recording a size must not itself
/// allocate, so no `Vec` can be involved.
const RECORDED: usize = 16;

/// The sizes of the first [`RECORDED`] allocations of the window now open, and
/// how many the window has seen. Reset by [`counted`], written by [`note`].
static SIZE_OF: [AtomicUsize; RECORDED] = [const { AtomicUsize::new(0) }; RECORDED];
static SIZE_N: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Whether allocations on **this** thread are counted. Const-initialised
    /// and `Drop`-free, so reading it neither allocates nor registers a
    /// destructor. Per thread, because the harness runs other tests — and
    /// their allocations — concurrently in this binary.
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

fn note(size: usize) {
    if ARMED.try_with(Cell::get).unwrap_or(false) {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        LARGEST.fetch_max(size, Ordering::Relaxed);
        // `get`, not an index: past `RECORDED` the size is dropped and the
        // count below says so. Allocation-free, which is the whole point.
        let i = SIZE_N.fetch_add(1, Ordering::Relaxed);
        if let Some(slot) = SIZE_OF.get(i) {
            slot.store(size, Ordering::Relaxed);
        }
    }
}

struct Counting;

// SAFETY: see the `allow(unsafe_code)` comment at the top of this file.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        note(l.size());
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        note(n);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        note(l.size());
        unsafe { System.alloc_zeroed(l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// What one counting window saw on this thread.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Window {
    /// Allocations and reallocations.
    count: usize,
    /// The largest size requested.
    largest: usize,
    /// The size of each of the first [`RECORDED`] of them, in order.
    sizes: [usize; RECORDED],
    /// How many of `sizes` are meaningful — `count`, capped at [`RECORDED`].
    kept: usize,
}

impl Window {
    fn add(&mut self, other: Window) {
        self.count += other.count;
        self.largest = self.largest.max(other.largest);
        for size in other.sizes.iter().take(other.kept) {
            if self.kept < RECORDED {
                self.sizes[self.kept] = *size;
                self.kept += 1;
            }
        }
    }
}

/// **Every size, not only the largest** — the failure message is the whole
/// diagnosis, so it carries the shape of the window and not a summary of it.
/// Four boxes of 184 reads `count 4, largest 184, sizes [184, 184, 184, 184]`;
/// a fifth allocation this engine added shows up as itself.
impl std::fmt::Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Window {{ count: {}, largest: {}, sizes: {:?}",
            self.count,
            self.largest,
            &self.sizes[..self.kept]
        )?;
        if self.count > self.kept {
            write!(f, " and {} more, unrecorded", self.count - self.kept)?;
        }
        f.write_str(" }")
    }
}

/// Run `f` with this thread's allocations counted. Nothing else runs inside
/// the window: no assertion, no format.
fn counted<T>(f: impl FnOnce() -> T) -> (T, Window) {
    let before = ALLOCS.load(Ordering::Relaxed);
    LARGEST.store(0, Ordering::Relaxed);
    SIZE_N.store(0, Ordering::Relaxed);
    ARMED.with(|a| a.set(true));
    let out = f();
    ARMED.with(|a| a.set(false));
    let kept = SIZE_N.load(Ordering::Relaxed).min(RECORDED);
    let mut sizes = [0usize; RECORDED];
    for (slot, seen) in sizes.iter_mut().zip(SIZE_OF.iter()).take(kept) {
        *slot = seen.load(Ordering::Relaxed);
    }
    let w = Window {
        count: ALLOCS.load(Ordering::Relaxed) - before,
        largest: LARGEST.load(Ordering::Relaxed),
        sizes,
        kept,
    };
    (out, w)
}

/// What ktls-core 0.0.5 `recv_tls_record` reserves on its buffer before it
/// reads any control record (`ffi.rs:110`): `u16::MAX + 5`. An allocation this
/// large inside a window is that buffer growing.
const CONTROL_RECORD_BUF: usize = u16::MAX as usize + 5;

/// rustls 0.23.44 with `ring` (the version `Cargo.lock` resolves):
/// `KeyScheduleTraffic::refresh_traffic_secret` boxes an HKDF expander twice
/// (`expander_for_okm`, reached from `derive_next` and from `expand_secret`),
/// once per direction — the receive key for the peer's KeyUpdate and the send
/// key for `update_requested`: `rustls-0.23.44/src/tls13/key_schedule.rs:565-580,
/// 623-628, 808-814`. `[measured 2026-09-13]` 4 allocations, by backtrace.
/// Not this engine's code and not removable from it — ADR-0063 decision 2.
const RUSTLS_KEY_SCHEDULE_ALLOCS: usize = 4;

/// The size of each of those boxes: a `Box<dyn HkdfExpander>` returned by
/// `Hkdf::expander_for_okm` (`rustls-0.23.44/src/crypto/tls13.rs:134-168`),
/// holding `ring`'s HMAC key for the 32-byte PRK. `[measured 2026-09-13]` 184
/// bytes, by backtrace.
///
/// **Two crates own this number.** rustls decides that there is a box; `ring`
/// 0.17.14 decides how big the key inside it is. A different size is a
/// different rustls **or** a different ring, and `assert_key_schedule_only`
/// names both, at the versions `Cargo.lock` resolved, rather than sending the
/// next reader to one changelog.
const RUSTLS_HKDF_EXPANDER_BOX: usize = 184;

/// The workspace lock file, located at **compile** time so that a test run from
/// any working directory finds it.
const CARGO_LOCK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock");

/// Every version of `name` that `Cargo.lock` resolves, as `"0.23.44"`, or a
/// sentence saying where to look when it cannot be read.
///
/// `[measured 2026-09-13]` a senior review found the two messages below naming
/// only rustls: 184 bytes is a `Box<dyn HkdfExpander>` **wrapping a `ring` HMAC
/// key**, so a `ring` bump moves the size and a rustls bump moves the count,
/// and each was being reported as the other's fault — or as this engine's.
/// Neither crate publishes its version to a dependent at compile time, so the
/// number comes from the lock file the build resolved, read on the failing
/// path only. Called from a failure message, never inside a counting window.
fn locked_versions(name: &str) -> String {
    let Ok(text) = std::fs::read_to_string(CARGO_LOCK) else {
        return format!("<unread: {CARGO_LOCK}>");
    };
    let wanted = format!("name = \"{name}\"");
    let mut found: Vec<&str> = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim() == wanted
            && let Some(v) = lines.next()
            && let Some(rest) = v.trim().strip_prefix("version = ")
        {
            found.push(rest.trim_matches('"'));
        }
    }
    if found.is_empty() {
        format!("<not in {CARGO_LOCK}>")
    } else {
        found.join(" and ")
    }
}

/// **Serialises the tests of this file.** `/proc/net/tls_stat` is one set of
/// counters for the namespace, and `ALLOCS` is one counter for the process.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// The kernel's own account
// ---------------------------------------------------------------------------

/// One `/proc/net/tls_stat` counter, or `None` when this kernel has no such
/// line — which for the `TlsRxRekey*` family means it predates TLS 1.3 rekey.
fn tls_stat(key: &str) -> Option<u64> {
    let text = std::fs::read_to_string("/proc/net/tls_stat").ok()?;
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        (parts.next() == Some(key))
            .then(|| parts.next().and_then(|v| v.parse().ok()))
            .flatten()
    })
}

#[derive(Clone, Copy, Debug)]
struct Rekeys {
    received: u64,
    rx_ok: u64,
    tx_ok: u64,
}

/// The three rekey counters, **or the environment sentence**.
///
/// The kernel added `TlsRxRekeyReceived` and its siblings with TLS 1.3 rekey
/// itself (`Documentation/networking/tls.rst`). A kernel without them answers
/// ktls-core's `setsockopt(TLS_RX)` on an already-keyed socket with `EBUSY`,
/// which the engine turns into an alert and never reports — so the missing
/// counters are the only honest way this file has to name that case.
fn rekeys() -> Rekeys {
    match (
        tls_stat("TlsRxRekeyReceived"),
        tls_stat("TlsRxRekeyOk"),
        tls_stat("TlsTxRekeyOk"),
    ) {
        (Some(received), Some(rx_ok), Some(tx_ok)) => Rekeys {
            received,
            rx_ok,
            tx_ok,
        },
        _ => panic!(
            "kernel refused the rekey (EBUSY): environment, not engine — \
             /proc/net/tls_stat has no TlsRxRekey* counters, so this kernel \
             predates TLS 1.3 rekey (linux commit 47069594e67e) and \
             setsockopt(TLS_RX) on a keyed socket is EBUSY"
        ),
    }
}

/// The kernel saw the peer's KeyUpdate and this end rekeyed both directions.
fn assert_rekeyed(before: Rekeys, who: &str) {
    let after = rekeys();
    assert!(
        after.received > before.received,
        "{who}: TlsRxRekeyReceived did not move ({before:?} -> {after:?}) — the \
         kernel never saw a KeyUpdate, so this test did not test one"
    );
    assert!(
        after.rx_ok > before.rx_ok,
        "{who}: TlsRxRekeyOk did not move ({before:?} -> {after:?}) — the kernel \
         paused decryption for a KeyUpdate and was never given the new key"
    );
    assert!(
        after.tx_ok > before.tx_ok,
        "{who}: TlsTxRekeyOk did not move ({before:?} -> {after:?}) — the peer \
         asked for update_requested and this end did not rekey its send side"
    );
}

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

fn pki() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("a self-signed certificate");
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der());
    (ck.cert.der().clone(), PrivateKeyDer::Pkcs8(key))
}

/// TLS 1.3, `AES-128-GCM` only: the suite the kernel carries.
fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    let mut p = rustls::crypto::ring::default_provider();
    p.cipher_suites
        .retain(|cs| cs.suite() == rustls::CipherSuite::TLS13_AES_128_GCM_SHA256);
    Arc::new(p)
}

/// The **counterparty's** client configuration — a plain rustls client.
fn peer_client_config(cert: CertificateDer<'static>) -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert).expect("the certificate parses");
    Arc::new(
        rustls::ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .expect("TLS 1.3 is available")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

/// The **counterparty's** server configuration — a plain rustls server.
fn peer_server_config(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
) -> Arc<rustls::ServerConfig> {
    Arc::new(
        rustls::ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .expect("TLS 1.3 is available")
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .expect("the certificate matches the key"),
    )
}

fn free_addr() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

/// A FIX message stamped at the wall clock — `serve_tls` and
/// `connect_and_serve_tls` both run a `SystemClock`, and a stale `SendingTime`
/// is refused in silence.
fn fix_now(sender: &str, target: &str, seq: u32, body: &str) -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = cache.format(now, 0);
    let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
    let (msg_type, rest) = body.split_once('|').unwrap_or((body, ""));
    let rest = rest.replace('|', "\u{1}");
    let inner = format!(
        "35={msg_type}\u{1}34={seq}\u{1}49={sender}\u{1}52={stamp}\u{1}56={target}\u{1}{rest}"
    );
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

/// Read from a blocking rustls stream until `wanted` appears, printable with
/// `|` for SOH. Returns what was read and, if it stopped short, why.
fn read_until(
    tls: &mut impl Read,
    seen: &mut String,
    wanted: &str,
    within: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + within;
    let mut buf = [0u8; 4096];
    while !seen.contains(wanted) {
        if Instant::now() > deadline {
            return Err(format!("nothing more within {within:?}"));
        }
        match tls.read(&mut buf) {
            Ok(0) => return Err("the connection closed".to_string()),
            Ok(n) => seen.push_str(&String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|")),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(format!("{:?}: {e}", e.kind())),
        }
    }
    Ok(())
}

fn connect_blocking(addr: &str) -> TcpStream {
    for _ in 0..500 {
        if let Ok(s) = TcpStream::connect(addr) {
            s.set_nodelay(true).expect("nodelay");
            s.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("a read timeout, so a hang fails instead of hanging");
            return s;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("nothing ever listened on {addr}");
}

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

fn join_within<T>(handle: JoinHandle<T>, within: Duration, who: &str) -> T {
    let deadline = Instant::now() + within;
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        handle.is_finished(),
        "{who} did not return within {within:?}"
    );
    handle.join().expect("the serving thread did not panic")
}

/// Everything here is administrative.
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

// ---------------------------------------------------------------------------
// 1. The acceptor
// ---------------------------------------------------------------------------

/// **`serve_tls` keeps a session after the counterparty rekeys.**
///
/// The client is plain buffered rustls. After `LoggedOn` it calls
/// `refresh_traffic_keys()` — a KeyUpdate with `update_requested` on the wire
/// ahead of the next record — and sends a `TestRequest` under the new key. A
/// `Heartbeat` naming that `TestReqID` can only come back if the engine gave
/// the kernel the new receive key (or the request would never decrypt) **and**
/// rekeyed its send side (or rustls, which rekeyed its receive side on reading
/// the engine's answering KeyUpdate, could not decrypt the reply).
#[test]
fn an_acceptor_session_survives_a_key_update_from_the_counterparty() {
    let _serial = serial();
    let addr = free_addr();
    let (cert, key) = pki();
    let handles = Handles::new();
    let seen = handles.observer();
    let admin = handles.admin();
    let serving = addr.clone();
    let serving_cert = cert.clone();
    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve_tls(
            &serving,
            Table::with_capacity(1).serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
            Never,
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            handles,
            vec![serving_cert],
            key,
        )
    });

    let name = "localhost".try_into().expect("a valid server name");
    let mut conn = rustls::ClientConnection::new(peer_client_config(cert), name).expect("client");
    let mut sock = connect_blocking(&addr);
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);

    tls.write_all(&fix_now("TW44", "ISLD", 1, "A|98=0|108=30|"))
        .expect("the Logon goes out");
    tls.flush().expect("flushed");
    let mut wire = String::new();
    let logon = read_until(&mut tls, &mut wire, "10=", Duration::from_secs(5));
    assert!(
        logon.is_ok() && wire.contains("35=A"),
        "no Logon came back before any KeyUpdate: {logon:?}, read {wire}"
    );
    let events = wait_for_all(&seen, &[EventKind::LoggedOn], Duration::from_secs(5));
    assert!(
        events.contains(&EventKind::LoggedOn),
        "no session came up; the stream held {events:?}"
    );

    let before = rekeys();
    tls.conn
        .refresh_traffic_keys()
        .expect("rustls queues a KeyUpdate on an established TLS 1.3 connection");
    tls.write_all(&fix_now("TW44", "ISLD", 2, "1|112=KU-ACCEPTOR|"))
        .expect("the TestRequest goes out");
    tls.flush().expect("flushed");

    let mut reply = String::new();
    let heartbeat = read_until(
        &mut tls,
        &mut reply,
        "112=KU-ACCEPTOR|",
        Duration::from_secs(5),
    );
    if heartbeat.is_err() {
        // Names the environment first when it is the environment.
        let _ = rekeys();
    }
    assert!(
        heartbeat.is_ok() && reply.contains("35=0"),
        "no Heartbeat answered the TestRequest sent after a KeyUpdate: {heartbeat:?}, \
         read {reply:?} — the acceptor's kTLS session did not survive the \
         counterparty rekeying"
    );
    assert_rekeyed(before, "acceptor");

    admin.shutdown(2_000);
    let stopped = join_within(engine, Duration::from_secs(10), "serve_tls");
    assert!(
        stopped.is_ok(),
        "serve_tls came back with an error: {stopped:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. The initiator
// ---------------------------------------------------------------------------

/// **`connect_and_serve_tls` keeps a session after the venue rekeys.**
///
/// The full initiator door (step 5b), against a venue that is plain buffered
/// rustls with a hand-written FIX side: it reads the initiator's `Logon`,
/// answers it, rekeys with `refresh_traffic_keys()`, and sends a
/// `TestRequest`. The initiator's `TlsTransport<Client>` is on the kernel —
/// asserted by the absence of `TlsFellBackToUserspace` and by
/// `TlsRxRekeyOk`, which no userspace connection moves.
#[test]
fn an_initiator_session_survives_a_key_update_from_the_venue() {
    let _serial = serial();
    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound").to_string();

    let handles = Handles::new();
    let seen = handles.observer();
    let admin = handles.admin();
    let dialled = addr.clone();
    let tls_cfg = ClientTls {
        roots: vec![cert.clone()],
        identity: None,
        server_name: ServerName::try_from("localhost").expect("a valid server name"),
        require_kernel: false,
    };
    let engine = std::thread::spawn(move || {
        fixbolt_engine::connect_and_serve_tls::<_, fixbolt_engine::journal::Store, _, _>(
            &dialled,
            Config::initiator(b"FIX.4.4", b"FIXBOLT", b"VENUE").with_heart_bt_int(30),
            Never,
            Policy::new(50, 200).expect("a legal pair"),
            fixbolt_engine::recovery::NoRecovery,
            fixbolt_engine::msglog::NoLog,
            handles,
            tls_cfg,
        )
    });

    let (mut sock, _) = listener.accept().expect("the initiator dials");
    sock.set_nodelay(true).expect("nodelay");
    sock.set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a read timeout");
    let mut conn =
        rustls::ServerConnection::new(peer_server_config(cert, key)).expect("a server connection");
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);

    let mut wire = String::new();
    let logon = read_until(&mut tls, &mut wire, "10=", Duration::from_secs(10));
    assert!(
        logon.is_ok() && wire.contains("35=A"),
        "the initiator's Logon never arrived: {logon:?}, read {wire}"
    );
    tls.write_all(&fix_now("VENUE", "FIXBOLT", 1, "A|98=0|108=30|"))
        .expect("the venue's Logon goes out");
    tls.flush().expect("flushed");
    let events = wait_for_all(&seen, &[EventKind::LoggedOn], Duration::from_secs(5));
    assert!(
        events.contains(&EventKind::LoggedOn),
        "the initiator never logged on; its stream held {events:?}"
    );
    assert!(
        !events.contains(&EventKind::TlsFellBackToUserspace),
        "the initiator fell back to userspace, so nothing below is about kTLS: {events:?}"
    );

    let before = rekeys();
    tls.conn
        .refresh_traffic_keys()
        .expect("rustls queues a KeyUpdate on an established TLS 1.3 connection");
    tls.write_all(&fix_now("VENUE", "FIXBOLT", 2, "1|112=KU-INITIATOR|"))
        .expect("the TestRequest goes out");
    tls.flush().expect("flushed");

    let mut reply = String::new();
    let heartbeat = read_until(
        &mut tls,
        &mut reply,
        "112=KU-INITIATOR|",
        Duration::from_secs(5),
    );
    if heartbeat.is_err() {
        let _ = rekeys();
    }
    assert!(
        heartbeat.is_ok() && reply.contains("35=0"),
        "no Heartbeat answered the TestRequest sent after a KeyUpdate: {heartbeat:?}, \
         read {reply:?} — the initiator's kTLS session did not survive the venue \
         rekeying"
    );
    assert_rekeyed(before, "initiator");

    admin.shutdown(2_000);
    let stopped: Result<Shutdown, ServeError> =
        join_within(engine, Duration::from_secs(10), "connect_and_serve_tls");
    assert!(
        stopped.is_ok(),
        "connect_and_serve_tls came back with an error: {stopped:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. Allocation
// ---------------------------------------------------------------------------

/// A rustls peer driven **on this thread**, non-blocking — the counterparty.
/// On one thread, so the counting window can open around the engine side's
/// calls alone and every allocation inside it is attributable to them.
struct Peer<C> {
    conn: C,
    sock: TcpStream,
}

impl<C, D> Peer<C>
where
    C: std::ops::DerefMut<Target = rustls::ConnectionCommon<D>>,
    D: rustls::SideData,
{
    fn absorb(&mut self) {
        loop {
            match self.conn.read_tls(&mut self.sock) {
                Ok(0) => break,
                Ok(_) => {
                    self.conn
                        .process_new_packets()
                        .expect("the engine side's records parse");
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("read_tls: {e}"),
            }
        }
    }

    fn flush_all(&mut self) {
        let mut spins = 0usize;
        while self.conn.wants_write() {
            spins += 1;
            assert!(spins < 100_000, "the engine side stopped reading");
            match self.conn.write_tls(&mut self.sock) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::yield_now(),
                Err(e) => panic!("write_tls: {e}"),
            }
        }
    }

    fn send_app(&mut self, data: &[u8]) {
        self.conn.writer().write_all(data).expect("rustls takes it");
        self.flush_all();
    }

    fn read_app(&mut self, want: usize) -> Vec<u8> {
        let mut got = Vec::new();
        let mut buf = [0u8; 256];
        let mut sweeps = 0usize;
        while got.len() < want {
            sweeps += 1;
            assert!(sweeps < 200_000, "the peer never read the reply: {got:?}");
            self.absorb();
            match self.conn.reader().read(&mut buf) {
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::yield_now(),
                Err(e) => panic!("peer read: {e}"),
            }
        }
        got
    }
}

/// Complete the handshake with the peer on this thread and hand over to the
/// kernel. Nothing is counted.
fn handshake<S, C, D>(tls: &mut TlsTransport<S>, peer: &mut Peer<C>)
where
    S: fixbolt_engine::tls::Side,
    C: std::ops::DerefMut<Target = rustls::ConnectionCommon<D>>,
    D: rustls::SideData,
{
    let mut buf = [0u8; 256];
    let mut sweeps = 0usize;
    while !tls.is_ready() {
        sweeps += 1;
        assert!(sweeps < 100_000, "the handshake never finished");
        match tls.recv(&mut buf) {
            Io::Idle => {}
            other => panic!("recv during the handshake said {other:?}"),
        }
        peer.absorb();
        peer.flush_all();
    }
    assert_eq!(tls.mode(), TlsMode::Kernel, "no kernel, nothing to measure");
    peer.absorb();
    peer.flush_all();
}

/// Read whatever the peer owed after its side of the handshake — a server's
/// session tickets, which reach a **client** only after the handover — with
/// every `recv` counted, and return that window. Eight reads:
/// `tests/tls_client.rs` shows two tickets read as `Idle`, and an empty socket
/// reads the same.
fn first_records<S: fixbolt_engine::tls::Side>(tls: &mut TlsTransport<S>) -> Window {
    let mut buf = [0u8; 256];
    let mut window = Window::default();
    let mut last = Io::Idle;
    for _ in 0..8 {
        let (r, w) = counted(|| tls.recv(&mut buf));
        window.add(w);
        if r != Io::Idle {
            last = r;
            break;
        }
    }
    assert_eq!(
        last,
        Io::Idle,
        "a post-handover control record did not read as Idle"
    );
    window
}

/// Prove the path carries bytes both ways, uncounted, so every first-use cost
/// is paid before a later window opens.
fn warm<S, C, D>(tls: &mut TlsTransport<S>, peer: &mut Peer<C>)
where
    S: fixbolt_engine::tls::Side,
    C: std::ops::DerefMut<Target = rustls::ConnectionCommon<D>>,
    D: rustls::SideData,
{
    peer.send_app(b"warm");
    assert_eq!(recv_uncounted(tls, 4), b"warm");
    send_uncounted(tls, b"back");
    assert_eq!(peer.read_app(4), b"back");
}

/// [`handshake`], [`first_records`], [`warm`], in that order.
fn establish<S, C, D>(tls: &mut TlsTransport<S>, peer: &mut Peer<C>) -> Window
where
    S: fixbolt_engine::tls::Side,
    C: std::ops::DerefMut<Target = rustls::ConnectionCommon<D>>,
    D: rustls::SideData,
{
    handshake(tls, peer);
    let window = first_records(tls);
    warm(tls, peer);
    window
}

/// An accepted `TlsTransport<Server>` and a plain rustls client as its peer.
fn acceptor_pair(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
) -> (TlsTransport, Peer<rustls::ClientConnection>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let name = "localhost".try_into().expect("a valid server name");
    let conn = rustls::ClientConnection::new(peer_client_config(cert.clone()), name)
        .expect("a client connection");
    let sock = TcpStream::connect(listener.local_addr().expect("addr")).expect("connects");
    sock.set_nonblocking(true).expect("non-blocking");
    let peer = Peer { conn, sock };
    let (accepted, _) = listener.accept().expect("accepted");
    let cfg = fixbolt_engine::tls::server_config(vec![cert], key).expect("a server config");
    let tls: TlsTransport = TlsTransport::new(
        TcpTransport::new(accepted).expect("non-blocking"),
        Handshake::new(
            rustls::server::UnbufferedServerConnection::new(cfg).expect("a server connection"),
        ),
    );
    (tls, peer)
}

/// A dialled `TlsTransport<Client>` built from `client_cfg`, with the kernel
/// offload on or off, and a plain rustls server built from `server_cfg` as its
/// peer. Both configurations are taken as `Arc`s so a redial can reuse them —
/// which is the only way a TLS session could ever be resumed.
fn initiator_pair(
    client_cfg: Arc<rustls::ClientConfig>,
    server_cfg: Arc<rustls::ServerConfig>,
    offload: bool,
) -> (TlsTransport<Client>, Peer<rustls::ServerConnection>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let sock = TcpStream::connect(listener.local_addr().expect("addr")).expect("connects");
    let (accepted, _) = listener.accept().expect("accepted");
    accepted.set_nonblocking(true).expect("non-blocking");
    let conn = rustls::ServerConnection::new(server_cfg).expect("a server connection");
    let peer = Peer {
        conn,
        sock: accepted,
    };
    let tls: TlsTransport<Client> = TlsTransport::with_offload(
        TcpTransport::new(sock).expect("non-blocking"),
        Handshake::new(
            rustls::client::UnbufferedClientConnection::new(
                client_cfg,
                ServerName::try_from("localhost").expect("a valid server name"),
            )
            .expect("a client connection"),
        ),
        offload,
    );
    (tls, peer)
}

/// `the_counter_is_live`: one `Vec::with_capacity(3)` inside a window counts
/// once, at 3 bytes, **and is recorded as a size of its own**. Without it a
/// window reading zero proves nothing — and without the third assertion the
/// `sizes` every failure message below prints could be empty for a live count.
fn assert_the_counter_is_live() {
    let (probe, live) = counted(|| Vec::<u8>::with_capacity(3));
    drop(probe);
    assert_eq!(
        (live.count, live.largest),
        (1, 3),
        "the_counter_is_live: one Vec::with_capacity(3) must count once, at 3 \
         bytes; the window read {live:?}"
    );
    assert_eq!(
        &live.sizes[..live.kept],
        &[3],
        "the_counter_is_live: the per-allocation sizes every message below \
         prints must hold that one allocation; the window read {live:?}"
    );
}

fn recv_uncounted<S: fixbolt_engine::tls::Side>(tls: &mut TlsTransport<S>, want: usize) -> Vec<u8> {
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    let mut sweeps = 0usize;
    while got.len() < want {
        sweeps += 1;
        assert!(sweeps < 200_000, "nothing arrived: {got:?}");
        match tls.recv(&mut buf) {
            Io::Ready(n) => got.extend_from_slice(&buf[..n]),
            Io::Idle => std::thread::yield_now(),
            other => panic!("recv said {other:?}"),
        }
    }
    got
}

fn send_uncounted<S: fixbolt_engine::tls::Side>(tls: &mut TlsTransport<S>, data: &[u8]) {
    let mut sent = 0usize;
    let mut sweeps = 0usize;
    while sent < data.len() {
        sweeps += 1;
        assert!(sweeps < 200_000, "the bytes never went out");
        match tls.send(&data[sent..]) {
            Io::Ready(n) => sent += n,
            Io::Idle => std::thread::yield_now(),
            other => panic!("send said {other:?}"),
        }
    }
}

const DATA: &[u8] = b"8=FIX.4.4|35=1|112=KU|";

/// The peer rekeys and sends `DATA`; the engine side reads it with every call
/// counted, then answers with `DATA`, also counted. Returns the window over the
/// engine side's calls, what they read, and the last non-progress `Io`.
fn rekey_counted<S, C, D>(tls: &mut TlsTransport<S>, peer: &mut Peer<C>) -> (Window, Vec<u8>, Io)
where
    S: fixbolt_engine::tls::Side,
    C: std::ops::DerefMut<Target = rustls::ConnectionCommon<D>>,
    D: rustls::SideData,
{
    peer.conn
        .refresh_traffic_keys()
        .expect("rustls queues a KeyUpdate on an established TLS 1.3 connection");
    // **The sender has written and gone quiet** before the window opens: the
    // KeyUpdate and the record under the new key are both on the wire.
    peer.send_app(DATA);

    let mut total = Window::default();
    let mut got = [0u8; 64];
    let mut len = 0usize;
    let mut last = Io::Idle;
    let mut sweeps = 0usize;
    while len < DATA.len() && sweeps < 200_000 {
        sweeps += 1;
        let (r, w) = counted(|| tls.recv(&mut got[len..]));
        total.add(w);
        match r {
            Io::Ready(k) => len += k,
            Io::Idle => std::thread::yield_now(),
            other => {
                last = other;
                break;
            }
        }
    }
    if len == DATA.len() {
        let mut sent = 0usize;
        while sent < DATA.len() && sweeps < 400_000 {
            sweeps += 1;
            let (r, w) = counted(|| tls.send(&DATA[sent..]));
            total.add(w);
            match r {
                Io::Ready(k) => sent += k,
                Io::Idle => std::thread::yield_now(),
                other => {
                    last = other;
                    break;
                }
            }
        }
    }
    (total, got[..len].to_vec(), last)
}

/// **A KeyUpdate after the handover allocates, on either side of
/// `TlsTransport`, exactly the boxes rustls's key schedule forces — and nothing
/// in this engine or in ktls-core.** ADR-0063 decision 2, the second named
/// carve-out from non-negotiable 1; asserted **exactly**, decision 3.
///
/// # Why exactly, and not "nothing" or "at most"
///
/// `[measured 2026-09-13]` with ktls-core's control-record buffer pre-sized,
/// the window still read **4**, and a backtrace per allocation put all four in
/// rustls 0.23.44: `KernelConnection::update_rx_secret` and `update_tx_secret`
/// → `KeyScheduleTraffic::refresh_traffic_secret` → `expander_for_okm`, which
/// returns a `Box<dyn HkdfExpander>` — two per direction, 184 bytes each. The
/// `Hkdf` trait's signature returns a box, so no provider avoids it and no
/// engine code can. Step 6c shipped `<= 4` under a name that said "nothing";
/// a ceiling is green when the number moves in either direction, so this test
/// asserts `== 4` and `== 184`, and a rustls bump that changes either is a red
/// that says to re-derive both constants and update ADR-0063.
///
/// On the initiator side the session tickets are the first control records,
/// not the KeyUpdate; they are read and set aside before this window opens,
/// and `a_session_ticket_after_the_handover_allocates_nothing` owns them.
///
/// # Attribution
///
/// Both ends live on this thread and the counter is armed only around
/// `TlsTransport::recv`/`send`; the peer's rustls work runs between those calls,
/// uncounted, and other tests' threads are never armed.
#[test]
fn a_key_update_allocates_only_the_boxes_rustls_key_schedule_forces() {
    let _serial = serial();
    assert_the_counter_is_live();
    let (cert, key) = pki();

    // --- The acceptor: TlsTransport<Server>, a rustls client as the peer.
    {
        let (mut tls, mut peer) = acceptor_pair(cert.clone(), key.clone_key());
        let _nothing_owed = establish(&mut tls, &mut peer);

        let before = rekeys();
        let (window, got, last) = rekey_counted(&mut tls, &mut peer);
        assert_eq!(
            got, DATA,
            "acceptor: the record after the KeyUpdate never arrived; last Io {last:?}"
        );
        assert_eq!(
            peer.read_app(DATA.len()),
            DATA,
            "acceptor: the reply did not decrypt"
        );
        assert_rekeyed(before, "acceptor");
        assert_key_schedule_only(window, "acceptor");
    }

    // --- The initiator: TlsTransport<Client>, a rustls server as the peer.
    {
        let client_cfg =
            fixbolt_engine::tls::client_config(vec![cert.clone()], None).expect("a client config");
        let (mut tls, mut peer) = initiator_pair(client_cfg, peer_server_config(cert, key), true);
        let _tickets = establish(&mut tls, &mut peer);

        let before = rekeys();
        let (window, got, last) = rekey_counted(&mut tls, &mut peer);
        assert_eq!(
            got, DATA,
            "initiator: the record after the KeyUpdate never arrived; last Io {last:?}"
        );
        assert_eq!(
            peer.read_app(DATA.len()),
            DATA,
            "initiator: the reply did not decrypt"
        );
        assert_rekeyed(before, "initiator");
        assert_key_schedule_only(window, "initiator");
    }
}

/// The three assertions on a KeyUpdate window, in the order that names the
/// cause best: ktls-core's buffer growing first, then the count, then the size.
///
/// **Both numbers are owned by two crates, and the messages say which.**
/// `RUSTLS_KEY_SCHEDULE_ALLOCS` is how many boxes rustls's key schedule makes;
/// `RUSTLS_HKDF_EXPANDER_BOX` is how big `ring`'s HMAC key inside one of them
/// is. A bump of **either** moves a number here, so each message names both at
/// their resolved versions and says which shape points at which — a message
/// that blamed only rustls sent the next reader to the wrong changelog.
fn assert_key_schedule_only(window: Window, who: &str) {
    assert!(
        window.largest < CONTROL_RECORD_BUF,
        "{who}: {window:?} — an allocation of {} bytes while handling the \
         counterparty's KeyUpdate is ktls-core's control-record buffer growing \
         on the engine thread",
        window.largest
    );
    assert_eq!(
        window.count,
        RUSTLS_KEY_SCHEDULE_ALLOCS,
        "{who}: {window:?} — TlsTransport::recv/send allocated a number of times \
         other than the {RUSTLS_KEY_SCHEDULE_ALLOCS} rustls key-schedule boxes \
         while handling the counterparty's KeyUpdate and answering under the new \
         key. Resolved here: rustls {rustls}, ring {ring}, ktls-core {ktls} \
         (from {CARGO_LOCK}). Read `sizes` above: {RUSTLS_KEY_SCHEDULE_ALLOCS} \
         entries of {RUSTLS_HKDF_EXPANDER_BOX} plus something else is an \
         allocation this engine or ktls-core added, and the odd size names it; \
         a different count of {RUSTLS_HKDF_EXPANDER_BOX}-byte entries is rustls \
         changing how many expanders its key schedule boxes \
         (tls13/key_schedule.rs). Re-derive both constants and update ADR-0063",
        rustls = locked_versions("rustls"),
        ring = locked_versions("ring"),
        ktls = locked_versions("ktls-core"),
    );
    assert_eq!(
        window.largest,
        RUSTLS_HKDF_EXPANDER_BOX,
        "{who}: {window:?} — a rustls key-schedule box is no longer \
         {RUSTLS_HKDF_EXPANDER_BOX} bytes. Resolved here: rustls {rustls}, ring \
         {ring} (from {CARGO_LOCK}). The box is a `Box<dyn HkdfExpander>` from \
         rustls's `Hkdf::expander_for_okm` (crypto/tls13.rs) **wrapping ring's \
         HMAC key**, so its size is ring's to change as much as rustls's: a \
         `ring` bump moves it with rustls untouched. Re-derive \
         RUSTLS_HKDF_EXPANDER_BOX and update ADR-0063",
        rustls = locked_versions("rustls"),
        ring = locked_versions("ring"),
    );
}

/// **A client's session tickets allocate nothing after the handover.**
/// ADR-0063 decision 1: the kernel-side session of a `TlsTransport<Client>`
/// counts a `NewSessionTicket` and drops it unread, so the initiator never
/// stores — and never resumes — a TLS session.
///
/// # Why the counter is asserted too
///
/// A window that reads zero because no ticket arrived looks exactly like one
/// that reads zero because the ticket cost nothing. A rustls server sends two
/// tickets by default (`rustls-0.23.44/src/server/builder.rs:123`), and
/// `tickets_ignored()` must move from **0 before** the window to **2 across
/// it**: the tickets reached this client, inside this window, and were set
/// aside.
///
/// `[measured 2026-09-13]` step 6c, before this decision: the same window read
/// `count: 16` with the rustls `KernelConnection` storing both tickets.
#[test]
fn a_session_ticket_after_the_handover_allocates_nothing() {
    let _serial = serial();
    assert_the_counter_is_live();
    let (cert, key) = pki();
    let client_cfg =
        fixbolt_engine::tls::client_config(vec![cert.clone()], None).expect("a client config");
    let (mut tls, mut peer) = initiator_pair(client_cfg, peer_server_config(cert, key), true);

    handshake(&mut tls, &mut peer);
    let before = tls.tickets_ignored();
    let window = first_records(&mut tls);
    let within = tls.tickets_ignored() - before;
    warm(&mut tls, &mut peer);

    assert_eq!(
        before, 0,
        "a session ticket was read before the counting window opened, so the \
         window below is not about tickets"
    );
    assert!(
        window.largest < CONTROL_RECORD_BUF,
        "initiator: {window:?} — an allocation of {} bytes while reading the \
         session tickets is ktls-core's control-record buffer growing on the \
         engine thread",
        window.largest
    );
    assert_eq!(
        window,
        Window::default(),
        "initiator: reading the session tickets after the handover allocated \
         ({window:?}) — the kernel-side session parsed or stored a ticket"
    );
    assert_eq!(
        within, 2,
        "no ticket reached the client, so this window is not about tickets \
         (tickets_ignored moved by {within}, expected the rustls server's default 2)"
    );
    assert_eq!(
        tls.tickets_ignored(),
        2,
        "a ticket arrived after the counting window closed"
    );
}

/// **A redial is a full TLS handshake, never a resumption.** ADR-0063
/// decision 1's rule, read from the server's side: two dials with the **same**
/// `Arc<ClientConfig>` to the **same** `Arc<ServerConfig>` — whose session
/// cache would resume — and the second handshake is still `Full`.
///
/// # Both halves of the decision, each on the path it guards
///
/// - **Kernel.** Tickets arrive after the handover and the kernel-side session
///   drops them unread, so nothing is stored whatever the configuration says.
///   `tickets_ignored() == 2` proves they arrived.
/// - **Userspace fallback** (`with_offload(false)`). rustls reads the tickets
///   itself, so the only thing between them and a resumption is
///   `Resumption::disabled()` in `tls::client_config`. The exchange after the
///   handshake forces them to be read: they precede the application data on
///   the stream.
///
/// A green here names the property, not a mechanism. `[measured 2026-09-13]`
/// reverting only `Resumption::disabled()` turns the **userspace** arm red
/// (`Some(Resumed)`); reverting only the kernel newtype — tickets handed back
/// to rustls — stays green, because rustls then stores them in a no-op store;
/// reverting both turns the **kernel** arm red. What the newtype alone buys is
/// the zero in `a_session_ticket_after_the_handover_allocates_nothing`.
#[test]
fn a_redial_does_a_full_handshake_not_a_resumption() {
    let _serial = serial();
    let (cert, key) = pki();

    for offload in [true, false] {
        let path = if offload { "kernel" } else { "userspace" };
        let client_cfg =
            fixbolt_engine::tls::client_config(vec![cert.clone()], None).expect("a client config");
        let server_cfg = peer_server_config(cert.clone(), key.clone_key());

        for dial in 1..=2 {
            let (mut tls, mut peer) =
                initiator_pair(Arc::clone(&client_cfg), Arc::clone(&server_cfg), offload);
            converse(&mut tls, &mut peer);
            if offload {
                assert_eq!(
                    tls.mode(),
                    TlsMode::Kernel,
                    "{path} dial {dial}: not on the kernel"
                );
                assert_eq!(
                    tls.tickets_ignored(),
                    2,
                    "{path} dial {dial}: no ticket reached the client, so nothing \
                     here could have been resumed"
                );
            } else {
                assert!(
                    tls.fell_back() && tls.mode() == TlsMode::Userspace,
                    "{path} dial {dial}: expected the userspace fallback"
                );
            }
            assert_eq!(
                peer.conn.handshake_kind(),
                Some(rustls::HandshakeKind::Full),
                "{path} dial {dial}: the server did not see a full handshake — the \
                 initiator resumed a TLS session (ADR-0063 decision 1)"
            );
        }
    }
}

/// Handshake on either path, then one exchange each way. The exchange is what
/// makes a userspace client read its session tickets, which precede `warm` on
/// the stream.
fn converse(tls: &mut TlsTransport<Client>, peer: &mut Peer<rustls::ServerConnection>) {
    let mut buf = [0u8; 256];
    let mut sweeps = 0usize;
    while peer.conn.is_handshaking() {
        sweeps += 1;
        assert!(sweeps < 100_000, "the handshake never finished");
        match tls.recv(&mut buf) {
            Io::Idle => {}
            other => panic!("recv during the handshake said {other:?}"),
        }
        peer.absorb();
        peer.flush_all();
    }
    warm(tls, peer);
}
