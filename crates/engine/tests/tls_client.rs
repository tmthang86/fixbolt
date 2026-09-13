//! The **client** side of [`fixbolt_engine::tls::TlsTransport`]: the same
//! non-blocking handshake, the same kTLS handover, driven from the end that
//! dials.
//!
//! **Step 5a of the `tls` plan** (`docs/plans/2026-09-04-tls.md`, Sửa 6, table
//! *Chia việc*, row 5a). `tls.rs` was server-only; an initiator needs a client
//! transport, and the plan's 6.4 item 1 decided it is the *same* struct made
//! generic over the side — `TlsTransport<S: Side = Server>` — so the handover
//! ADR-0018's four conditions paid for exists once. The four older
//! `tests/tls*.rs` files are the regression net for the server half and are not
//! modified by this step; this file is the net for the client half.
//!
//! # Traps this file guards, one test each (plan 6.9)
//!
//! - A client handshake that **waits** on the socket —
//!   `a_client_handshake_completes_without_blocking`.
//! - A server that sends application data in the same flight as its `Finished`
//!   (TLS 1.3 "half-RTT"), whose bytes the client must not lose —
//!   `a_reply_sent_straight_after_the_servers_finished_is_not_lost`.
//! - Session tickets, which a TLS 1.3 server sends only after it has read the
//!   client's `Finished` and which therefore arrive at a client **after** the
//!   handover, as control records the kernel reports as `EIO` —
//!   `a_session_ticket_arriving_after_the_handover_reads_as_idle`.
//! - A certificate that does not name what the client dialled, which must read
//!   as a failed handshake rather than as a peer that hung up —
//!   `a_certificate_for_another_name_fails_the_handshake`.
//!
//! # What this file does NOT prove
//!
//! - **Nothing about an engine.** No `dial`, no session: that is step 5b.
//! - **No latency and no allocation count.** The `Context` buffer that grows on
//!   the first control record — and a client *always* receives one, the
//!   tickets — is step 6c's `a_key_update_allocates_nothing_after_the_handover`.
#![cfg(all(feature = "tls", target_os = "linux"))]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`. A
// setup step that cannot be completed is a failing test, which is the point.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use fixbolt_engine::tls::{Client, Handshake, Step, TlsMode, TlsTransport, client_config};
use fixbolt_engine::transport::{Io, TcpTransport, Transport};
use rustls::client::UnbufferedClientConnection;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};

/// A self-signed `localhost` certificate, made now rather than committed.
fn pki() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("a self-signed certificate");
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der());
    (ck.cert.der().clone(), PrivateKeyDer::Pkcs8(key))
}

/// The **counterparty's** configuration — deliberately not built by the code
/// under test. TLS 1.3 and `AES-128-GCM` only, so the suite the client offers
/// is the one the kernel carries.
fn server_config(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    half_rtt: bool,
) -> Arc<rustls::ServerConfig> {
    let mut p = rustls::crypto::ring::default_provider();
    p.cipher_suites
        .retain(|cs| cs.suite() == rustls::CipherSuite::TLS13_AES_128_GCM_SHA256);
    let mut cfg = rustls::ServerConfig::builder_with_provider(Arc::new(p))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3 is available")
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .expect("the certificate matches the key");
    cfg.send_half_rtt_data = half_rtt;
    Arc::new(cfg)
}

/// The client connection the code under test drives, built from
/// [`client_config`] — the library's, because that is half of what 5a adds.
fn client_conn(
    cert: CertificateDer<'static>,
    name: ServerName<'static>,
) -> UnbufferedClientConnection {
    let cfg = client_config(vec![cert], None).expect("a client config from one root");
    UnbufferedClientConnection::new(cfg, name).expect("a client connection")
}

fn localhost() -> ServerName<'static> {
    ServerName::try_from("localhost").expect("a valid server name")
}

/// `/proc/net/tls_stat`'s cumulative counters — the same reading, and for the
/// same reason, as `tests/tls.rs::tls_stat`.
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

/// **Every test in this file that offloads holds this.** `/proc/net/tls_stat`
/// is one counter for the whole network namespace, and the test harness runs
/// this file's tests on parallel threads: a `> before` check could be moved by
/// a *different* test's handover and read green about a connection that never
/// offloaded. Other test binaries do not run concurrently with this one; other
/// processes on the machine still can, and that residue is not closed here.
static KERNEL_COUNTERS: Mutex<()> = Mutex::new(());

fn kernel_counters() -> MutexGuard<'static, ()> {
    KERNEL_COUNTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// A buffered `rustls` server driven **on this thread** — the counterparty,
/// not the code under test. The shape and the reasons are
/// `tests/tls.rs::InThreadClient`'s, mirrored: one thread, so the client under
/// test cannot run between two of this server's writes, and a non-blocking
/// socket, so a server that waits fails the test instead of hanging it.
struct InThreadServer {
    conn: rustls::ServerConnection,
    sock: TcpStream,
}

impl InThreadServer {
    fn accept(listener: &TcpListener, cfg: Arc<rustls::ServerConfig>) -> Self {
        let (sock, _) = listener.accept().expect("the client connects");
        sock.set_nonblocking(true).expect("non-blocking");
        let conn = rustls::ServerConnection::new(cfg).expect("a server connection");
        Self { conn, sock }
    }

    /// Take what the client has put on the wire and let rustls act on it.
    /// Returns the ciphertext byte count, or the error rustls raised.
    fn absorb(&mut self) -> Result<usize, rustls::Error> {
        let mut got = 0usize;
        loop {
            match self.conn.read_tls(&mut self.sock) {
                Ok(0) => break,
                Ok(n) => {
                    got += n;
                    self.conn.process_new_packets()?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("read_tls: {e}"),
            }
        }
        Ok(got)
    }

    fn queue_app(&mut self, data: &[u8]) {
        self.conn
            .writer()
            .write_all(data)
            .expect("rustls takes the plaintext");
    }

    fn read_app(&mut self, buf: &mut [u8]) -> usize {
        match self.conn.reader().read(buf) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => 0,
            Err(e) => panic!("server read: {e}"),
        }
    }

    fn is_handshaking(&self) -> bool {
        self.conn.is_handshaking()
    }

    /// Put everything queued on the wire; returns how many bytes that was.
    /// Bounded, because a peer that stopped reading would otherwise hang the
    /// test (`docs/reference/a-reversal-can-fail-by-hanging.md`).
    fn flush_all(&mut self) -> usize {
        let mut sent = 0usize;
        let mut spins = 0usize;
        while self.conn.wants_write() {
            spins += 1;
            assert!(spins < 100_000, "the client stopped reading mid-flight");
            match self.conn.write_tls(&mut self.sock) {
                Ok(n) => sent += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::yield_now(),
                Err(e) => panic!("write_tls: {e}"),
            }
        }
        sent
    }
}

/// Send all of `data` through `tls`, taking `Idle` for an answer the way the
/// engine does.
fn send_all(tls: &mut impl Transport, data: &[u8]) {
    let mut sent = 0usize;
    let mut sweeps = 0usize;
    while sent < data.len() {
        sweeps += 1;
        assert!(sweeps < 200_000, "the bytes never went out");
        match tls.send(data.get(sent..).unwrap_or_default()) {
            Io::Ready(n) => sent += n,
            Io::Idle => std::thread::yield_now(),
            other => panic!("send said {other:?}"),
        }
    }
}

/// Receive exactly `want` bytes through `tls`.
fn recv_exactly(tls: &mut impl Transport, want: usize) -> Vec<u8> {
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    let mut sweeps = 0usize;
    while got.len() < want {
        sweeps += 1;
        assert!(sweeps < 200_000, "nothing arrived: got {got:?}");
        match tls.recv(&mut buf) {
            Io::Ready(n) => got.extend_from_slice(buf.get(..n).unwrap_or_default()),
            Io::Idle => std::thread::yield_now(),
            other => panic!("recv said {other:?}"),
        }
    }
    got
}

#[test]
fn a_client_handshake_completes_without_blocking() {
    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let sock = TcpStream::connect(listener.local_addr().expect("an address")).expect("connects");
    let mut server = InThreadServer::accept(&listener, server_config(cert.clone(), key, false));

    let mut transport = TcpTransport::new(sock).expect("non-blocking");
    let mut hs: Handshake<Client> = Handshake::new(client_conn(cert, localhost()));

    // **The assertion this test exists for, constructed rather than hoped
    // for.** The server lives on this thread and has not been given a turn, so
    // nothing will ever answer this pump. A client that waited for the
    // `ServerHello` would not return at all; this one must say `Pending` and
    // come back.
    let first = hs.pump(&mut transport);
    assert_eq!(
        first,
        Step::Pending,
        "a client pump whose server has not spoken must report Pending and return"
    );
    // And it did its half before returning: the `ClientHello` is on the wire.
    // `Pending` from a pump that sent nothing would pass the line above.
    let hello = server.absorb().expect("the ClientHello parses");
    assert!(
        hello > 0,
        "the client returned Pending without putting its ClientHello on the wire"
    );

    let mut pendings = 1usize;
    let mut sweeps = 0usize;
    let outcome = loop {
        server.flush_all();
        sweeps += 1;
        assert!(sweeps < 100_000, "the handshake never finished");
        match hs.pump(&mut transport) {
            Step::Pending => {
                pendings += 1;
                server.absorb().expect("the client's records parse");
                std::thread::yield_now();
            }
            other => break other,
        }
    };
    assert_eq!(outcome, Step::Done, "the client handshake did not complete");
    assert!(pendings > 0);
    assert_eq!(
        hs.leftover(),
        0,
        "ciphertext left in this engine's buffer is ciphertext the kernel will \
         never see — ADR-0018's third condition, on the client side"
    );
    // `Done` must mean the client's `Finished` is already on the wire, not
    // queued in this engine's buffer: `dangerous_into_kernel_connection`
    // refuses a connection that still owes TLS bytes.
    server.absorb().expect("the client's Finished parses");
    assert!(
        !server.is_handshaking(),
        "the client reported Done while its Finished had not reached the server"
    );
}

#[test]
fn a_client_hands_its_keys_to_the_kernel_and_reads_plaintext() {
    // Asserted three ways, as `tests/tls.rs` does for the server, because "the
    // bytes arrived" is passable by a client that never offloaded: the round
    // trip, the mode this engine reports, and the kernel's own counters.
    let _counters = kernel_counters();
    let before_tx = tls_stat("TlsTxSw");
    let before_rx = tls_stat("TlsRxSw");

    const REQUEST: &[u8] = b"8=FIX.4.4|35=A|";
    const REPLY: &[u8] = b"8=FIX.4.4|35=A|98=0|";

    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let addr = listener.local_addr().expect("an address");
    let cfg = server_config(cert.clone(), key, false);
    // A blocking rustls server on its own thread, reading first and replying
    // after: the request goes out through the kernel's TX keys and the reply
    // comes back through its RX keys.
    let server = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let (mut sock, _) = listener.accept()?;
        let mut conn = rustls::ServerConnection::new(cfg)
            .map_err(|e| std::io::Error::other(format!("{e}")))?;
        let mut tls = rustls::Stream::new(&mut conn, &mut sock);
        let mut got = vec![0u8; REQUEST.len()];
        tls.read_exact(&mut got)?;
        tls.write_all(REPLY)?;
        tls.flush()?;
        Ok(got)
    });

    let sock = TcpStream::connect(addr).expect("the listener is up");
    let mut tls: TlsTransport<Client> = TlsTransport::new(
        TcpTransport::new(sock).expect("non-blocking"),
        Handshake::new(client_conn(cert, localhost())),
    );

    send_all(&mut tls, REQUEST);
    let got = recv_exactly(&mut tls, REPLY.len());
    assert_eq!(got, REPLY, "the client must read plaintext, in order");
    let heard = server.join().expect("the server thread").expect("a read");
    assert_eq!(
        heard, REQUEST,
        "the server decrypted what the client's kernel encrypted"
    );

    assert!(tls.is_ready(), "the client never handed its keys over");
    assert_eq!(
        tls.mode(),
        TlsMode::Kernel,
        "ADR-0005 open question 3, client side: a connection that silently \
         stayed in userspace publishes a latency figure about another code path"
    );
    assert!(!tls.fell_back());
    // **The assertion no application-level success can fake.** A userspace
    // client would pass every line above and move neither counter.
    assert!(
        tls_stat("TlsTxSw") > before_tx,
        "/proc/net/tls_stat TlsTxSw did not move: the kernel never took the \
         client's send keys"
    );
    assert!(
        tls_stat("TlsRxSw") > before_rx,
        "/proc/net/tls_stat TlsRxSw did not move: the kernel never took the \
         client's receive keys"
    );
}

#[test]
fn a_client_told_not_to_offload_serves_from_userspace_and_says_so() {
    // ADR-0005 decision 3 for the client. The desk's kernel offloads, so
    // without `with_offload(false)` this path is code that has never run.
    const REQUEST: &[u8] = b"8=FIX.4.4|35=A|";
    const REPLY: &[u8] = b"8=FIX.4.4|35=A|98=0|";

    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let addr = listener.local_addr().expect("an address");
    let cfg = server_config(cert.clone(), key, false);
    let server = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let (mut sock, _) = listener.accept()?;
        let mut conn = rustls::ServerConnection::new(cfg)
            .map_err(|e| std::io::Error::other(format!("{e}")))?;
        let mut tls = rustls::Stream::new(&mut conn, &mut sock);
        let mut got = vec![0u8; REQUEST.len()];
        tls.read_exact(&mut got)?;
        tls.write_all(REPLY)?;
        tls.flush()?;
        Ok(got)
    });

    let sock = TcpStream::connect(addr).expect("the listener is up");
    let mut tls: TlsTransport<Client> = TlsTransport::with_offload(
        TcpTransport::new(sock).expect("non-blocking"),
        Handshake::new(client_conn(cert, localhost())),
        false,
    );

    send_all(&mut tls, REQUEST);
    let got = recv_exactly(&mut tls, REPLY.len());
    assert_eq!(
        got, REPLY,
        "the fallback must carry the same bytes in order"
    );
    let heard = server.join().expect("the server thread").expect("a read");
    assert_eq!(heard, REQUEST, "the server could not decrypt the request");

    // The ones that must come out the other way.
    assert_eq!(tls.mode(), TlsMode::Userspace);
    assert!(!tls.mode().keeps_the_hot_path());
    assert!(
        tls.fell_back(),
        "a client in userspace must be able to say so, or nobody finds out \
         until a histogram looks wrong"
    );
    assert!(!tls.is_ready(), "is_ready means the keys are in the kernel");
}

#[test]
fn a_reply_sent_straight_after_the_servers_finished_is_not_lost() {
    // **The server-side trap of 2026-09-09, mirrored.** A TLS 1.3 server may
    // put application data in the same flight as its `Finished` ("half-RTT"),
    // so the client receives it while it is still completing its own side.
    // rustls then reports `ReadTraffic` inside the client's pump, and those
    // bytes must survive the handover — a FIX acceptor that answers the Logon
    // this fast would otherwise have its answer vanish and the session time
    // out with nothing naming why.
    const REPLY: &[u8] = b"8=FIX.4.4|35=A|";

    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let sock = TcpStream::connect(listener.local_addr().expect("an address")).expect("connects");
    let mut server = InThreadServer::accept(&listener, server_config(cert.clone(), key, true));
    // **Queued before the server has seen a ClientHello**, so rustls emits it
    // immediately after the server's `Finished`, in the one flush below. The
    // client is not pumped between the flight and the data.
    server.queue_app(REPLY);

    let mut transport = TcpTransport::new(sock).expect("non-blocking");
    let mut hs: Handshake<Client> = Handshake::new(client_conn(cert, localhost()));

    let mut flights = 0usize;
    let mut sweeps = 0usize;
    let outcome = loop {
        sweeps += 1;
        assert!(sweeps < 100_000, "the handshake never finished");
        match hs.pump(&mut transport) {
            Step::Pending => {
                server.absorb().expect("the client's records parse");
                if server.flush_all() > 0 {
                    flights += 1;
                }
            }
            other => break other,
        }
    };
    assert_eq!(outcome, Step::Done);
    assert_eq!(
        flights, 1,
        "the server's flight and its data must leave in one flush, or this test \
         is not about data that arrives before the client finishes"
    );

    let early = hs.take_early_data();
    assert_eq!(
        early, REPLY,
        "application data the server sent straight after its Finished was lost \
         before the handover"
    );
    assert!(
        hs.take_early_data().is_empty(),
        "taken means taken: the same bytes twice is a duplicate sequence number"
    );
    assert_eq!(hs.leftover(), 0, "ADR-0018's third condition");
}

#[test]
fn a_session_ticket_arriving_after_the_handover_reads_as_idle() {
    // A TLS 1.3 server sends its session tickets (rustls: two) only once it has
    // read the client's `Finished` — by which time the client has handed its
    // keys to the kernel. Each ticket is a control record the kernel will not
    // decode; `read(2)` says `EIO`, and a client that took that for a dead
    // socket would drop every session it ever dialled.
    let _counters = kernel_counters();
    const DATA: &[u8] = b"8=FIX.4.4|35=0|";

    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let sock = TcpStream::connect(listener.local_addr().expect("an address")).expect("connects");
    let mut server = InThreadServer::accept(&listener, server_config(cert.clone(), key, false));

    let mut tls: TlsTransport<Client> = TlsTransport::new(
        TcpTransport::new(sock).expect("non-blocking"),
        Handshake::new(client_conn(cert, localhost())),
    );

    let mut buf = [0u8; 256];
    let mut sweeps = 0usize;
    while !tls.is_ready() {
        sweeps += 1;
        assert!(sweeps < 100_000, "the client never handed over");
        match tls.recv(&mut buf) {
            Io::Idle => {}
            other => panic!("recv during the handshake said {other:?}"),
        }
        if tls.is_ready() {
            break;
        }
        server.absorb().expect("the client's records parse");
        server.flush_all();
    }
    assert_eq!(tls.mode(), TlsMode::Kernel);
    // The construction: the server has not read the client's `Finished` yet,
    // so it cannot have sent a ticket — every ticket below arrives after the
    // handover.
    assert!(
        server.is_handshaking(),
        "the server finished before the client handed over, so the tickets may \
         have been consumed by rustls and this test is not about the kernel"
    );

    server.absorb().expect("the client's Finished parses");
    let tickets = server.flush_all();
    assert!(
        !server.is_handshaking() && tickets > 0,
        "the server sent nothing after the client's Finished ({tickets} bytes), \
         so no ticket is on the wire and nothing below tests the case"
    );

    // Every one of these reads meets a ticket or an empty socket. Neither is an
    // error, and neither is data.
    for i in 0..8 {
        let r = tls.recv(&mut buf);
        assert_eq!(
            r,
            Io::Idle,
            "recv #{i} after the tickets arrived said {r:?}: a session ticket \
             after the handover must read as Idle"
        );
    }

    // **And the connection is still alive**, which is the half `Idle` alone
    // does not prove: a control record left unconsumed would make every later
    // `read(2)` fail with `EIO` again, so data after the tickets is readable
    // only if they were really handled.
    server.queue_app(DATA);
    server.flush_all();
    let got = recv_exactly(&mut tls, DATA.len());
    assert_eq!(got, DATA);

    // The other direction still works too.
    send_all(&mut tls, b"35=0|");
    let mut back = [0u8; 16];
    let mut n = 0usize;
    let mut sweeps = 0usize;
    while n == 0 {
        sweeps += 1;
        assert!(sweeps < 200_000, "the server never read the client's reply");
        server.absorb().expect("the client's records parse");
        n = server.read_app(&mut back);
        std::thread::yield_now();
    }
    assert_eq!(back.get(..n).unwrap_or_default(), b"35=0|");
}

#[test]
fn a_certificate_for_another_name_fails_the_handshake() {
    // Plan 6.4 item 3 and 6.9: `SocketConnectHost` given as an IP literal makes
    // the server name an IP address, and a certificate with no IP SAN does not
    // name it. That must read as **a failed handshake**, not as a venue that
    // hung up — the two send an operator to different places.
    let (cert, key) = pki();

    // The control first: the same certificate, dialled by the name it carries,
    // completes. Without it, the failure below could be anything.
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let sock =
            TcpStream::connect(listener.local_addr().expect("an address")).expect("connects");
        let mut server = InThreadServer::accept(
            &listener,
            server_config(cert.clone(), key.clone_key(), false),
        );
        let mut transport = TcpTransport::new(sock).expect("non-blocking");
        let mut hs: Handshake<Client> = Handshake::new(client_conn(cert.clone(), localhost()));
        let mut sweeps = 0usize;
        let outcome = loop {
            sweeps += 1;
            assert!(sweeps < 100_000, "the control handshake never finished");
            match hs.pump(&mut transport) {
                Step::Pending => {
                    server.absorb().expect("the client's records parse");
                    server.flush_all();
                }
                other => break other,
            }
        };
        assert_eq!(
            outcome,
            Step::Done,
            "the control failed, so the certificate is wrong for a reason other \
             than its name"
        );
    }

    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let sock = TcpStream::connect(listener.local_addr().expect("an address")).expect("connects");
    let mut server = InThreadServer::accept(&listener, server_config(cert.clone(), key, false));
    let ip = ServerName::from(std::net::IpAddr::from([127, 0, 0, 1]));
    let mut tls: TlsTransport<Client> = TlsTransport::new(
        TcpTransport::new(sock).expect("non-blocking"),
        Handshake::new(client_conn(cert, ip)),
    );

    let mut buf = [0u8; 64];
    let mut sweeps = 0usize;
    let outcome = loop {
        sweeps += 1;
        assert!(
            sweeps < 100_000,
            "the handshake neither finished nor failed"
        );
        match tls.recv(&mut buf) {
            Io::Idle => {
                // The server may see an alert once the client gives up; that
                // is the server's business, not this test's.
                let _ = server.absorb();
                server.flush_all();
            }
            other => break other,
        }
    };
    assert!(
        matches!(outcome, Io::Failed(_)),
        "a certificate that does not name 127.0.0.1 must fail the handshake; \
         got {outcome:?} — `Closed` would read as a venue that hung up"
    );
    assert!(
        !tls.is_ready(),
        "no keys may reach the kernel for an unverified peer"
    );
    // And it stays failed: the engine asks again on the next sweep.
    assert!(matches!(tls.recv(&mut buf), Io::Failed(_)));
    assert!(matches!(tls.send(b"35=A|"), Io::Failed(_)));
}
