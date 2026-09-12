//! The TLS handshake completes on a non-blocking socket, driven the way
//! `PendingSet::turn` drives it: a slice of work per sweep, never a spin.
//!
//! **What this file is really testing is the inversion.** `spikes/ktls` proved
//! `rustls` + `ktls-core` work on a plain non-blocking socket (ADR-0018) — with
//! a driver that spins until the handshake finishes. That driver cannot be used
//! here: it runs on the acceptor thread inside a sweep over every waiting
//! socket, so a spin on one connection stalls every other one behind it for a
//! whole round trip. `crates/engine/src/tls.rs` turns the loop inside out, and
//! these tests are about that turn rather than about TLS.
#![cfg(all(feature = "tls", target_os = "linux"))]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`. A
// setup step that cannot be completed is a failing test, which is the point.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use fixbolt_engine::tls::{Handshake, Step, TlsMode};
use fixbolt_engine::transport::TcpTransport;

/// A self-signed `localhost` certificate, made now rather than committed.
fn pki() -> (
    rustls::pki_types::CertificateDer<'static>,
    rustls::pki_types::PrivateKeyDer<'static>,
) {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("a self-signed certificate");
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der());
    (
        ck.cert.der().clone(),
        rustls::pki_types::PrivateKeyDer::Pkcs8(key),
    )
}

/// TLS 1.3, `AES-128-GCM` only — the same narrowing the spike made, for the
/// same reason: kTLS carries fewer suites than rustls will negotiate, so a test
/// that let the ends negotiate freely would be measuring the negotiation.
fn provider() -> rustls::crypto::CryptoProvider {
    let mut p = rustls::crypto::ring::default_provider();
    p.cipher_suites
        .retain(|cs| cs.suite() == rustls::CipherSuite::TLS13_AES_128_GCM_SHA256);
    p
}

fn server_config(
    cert: rustls::pki_types::CertificateDer<'static>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Arc<rustls::ServerConfig> {
    let mut cfg = rustls::ServerConfig::builder_with_provider(Arc::new(provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3 is available")
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .expect("the certificate matches the key");
    // Required by `dangerous_into_kernel_connection`, which fails rather than
    // handing out no keys if this is off.
    cfg.enable_secret_extraction = true;
    Arc::new(cfg)
}

fn client_config(cert: rustls::pki_types::CertificateDer<'static>) -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert).expect("the certificate parses");
    let mut cfg = rustls::ClientConfig::builder_with_provider(Arc::new(provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3 is available")
        .with_root_certificates(roots)
        .with_no_client_auth();
    cfg.enable_secret_extraction = true;
    Arc::new(cfg)
}

/// A blocking `rustls` client on its own thread — the *counterparty*, not the
/// code under test. Deliberately the ordinary buffered API: the thing being
/// proven is that this engine's non-blocking acceptor completes a handshake
/// with a perfectly normal TLS client.
fn client_thread(
    addr: std::net::SocketAddr,
    cert: rustls::pki_types::CertificateDer<'static>,
) -> std::thread::JoinHandle<std::io::Result<()>> {
    std::thread::spawn(move || {
        let cfg = client_config(cert);
        let name = "localhost".try_into().expect("a valid server name");
        let mut conn = rustls::ClientConnection::new(cfg, name)
            .map_err(|e| std::io::Error::other(format!("{e}")))?;
        let mut sock = TcpStream::connect(addr)?;
        let mut tls = rustls::Stream::new(&mut conn, &mut sock);
        // Writing is what forces the handshake to completion, and it is all
        // this counterparty does.
        //
        // **It deliberately does not read back**, and the first version of this
        // test did. `rustls::Stream::read` blocks until bytes arrive; the code
        // under test is a handshake driver that sends no application data, so
        // the client waited forever and `joiner.join()` waited with it. The
        // test hung instead of failing — which is how a test fails to prove
        // anything at all (`docs/reference/a-reversal-can-fail-by-hanging.md`),
        // and it looked exactly like the non-blocking loop having a bug.
        tls.write_all(b"hello")?;
        tls.flush()?;
        Ok(())
    })
}

/// **Step 6 of the TLS-CI plan's Sửa 1: a measurement, not a fix.**
///
/// Two tests in this file are red on GitHub's runner and green twenty times on
/// the owner's desk, and `[measured 2026-09-12]` the *same* commit `aa4f46e`
/// produced one green run and one red run on that runner minutes apart. Two
/// explanations were open and the shape of the code was not allowed to pick
/// between them:
///
/// - **(a)** the tests assert a scheduling outcome — the peer's application
///   record simply had not arrived when `pump` reported [`Step::Done`], and
///   nothing was lost;
/// - **(b)** the bytes did arrive and `take_early_data` dropped them, which is
///   a defect in `crates/engine/src/tls.rs` and exactly what the test's name
///   claims to guard.
///
/// This prints what separates them. It is temporary, it asserts nothing, and
/// the CI step that reads it passes `--nocapture`.
///
/// **Reading the socket directly here is sound only because this connection is
/// then thrown away.** Bytes taken off the socket behind rustls's back are what
/// the spike's `hand-draining-desyncs-the-kernel` check fails on — see
/// [`fixbolt_engine::tls::Handshake::take_early_data`]. No handover follows.
fn probe_after_done(
    label: &str,
    sweeps: usize,
    pendings: usize,
    early_len: usize,
    leftover: usize,
    transport: &mut TcpTransport,
) {
    use std::io::Read;
    // Only worth waiting when nothing arrived in time: in the green case the
    // record is already decrypted and this would just cost every run a second.
    let late = if early_len == 0 {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut buf = [0u8; 4096];
        let mut n = 0usize;
        while std::time::Instant::now() < deadline {
            match (&mut transport.socket()).read(&mut buf) {
                Ok(0) => break,
                Ok(k) => {
                    n += k;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::yield_now();
                }
                Err(_) => break,
            }
        }
        n
    } else {
        0
    };
    println!(
        "probe {label}: sweeps {sweeps} pendings {pendings} early {early_len} leftover {leftover} late_ciphertext {late}"
    );
}

#[test]
fn a_handshake_completes_without_the_acceptor_ever_blocking() {
    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let addr = listener.local_addr().expect("an address");
    let joiner = client_thread(addr, cert.clone());

    let (sock, _) = listener.accept().expect("the client connects");
    let mut transport = TcpTransport::new(sock).expect("non-blocking");
    let conn = rustls::server::UnbufferedServerConnection::new(server_config(cert, key))
        .expect("a server connection");
    let mut hs = Handshake::new(conn);

    // **The assertion that this test exists for.** The handshake is driven in
    // slices, and a slice that has nothing to do reports `Pending` and returns
    // — it does not wait. A bounded number of sweeps stands in for the engine's
    // own loop; an implementation that blocked would still pass, so the count
    // below is what says it did not.
    let mut sweeps = 0usize;
    let mut pendings = 0usize;
    let outcome = loop {
        sweeps += 1;
        assert!(sweeps < 100_000, "the handshake never finished");
        match hs.pump(&mut transport) {
            Step::Pending => {
                pendings += 1;
                std::thread::yield_now();
            }
            other => break other,
        }
    };

    assert_eq!(outcome, Step::Done, "the handshake did not complete");
    assert_eq!(
        hs.leftover(),
        0,
        "ciphertext left in this engine's buffer is ciphertext the kernel will \
         never see, and its receive sequence number has already counted it — \
         ADR-0018's third condition"
    );
    // A handshake that finished on the first call never gave the socket a
    // chance to be empty, so it proves nothing about not blocking. Every run
    // observed here needs the peer's flight to arrive, which takes at least one
    // round trip that this thread did not wait for.
    probe_after_done(
        "no_blocking",
        sweeps,
        pendings,
        hs.take_early_data().len(),
        hs.leftover(),
        &mut transport,
    );
    assert!(
        pendings > 0,
        "the handshake completed without ever yielding — this test cannot \
         distinguish that from a spin, and the point of the inversion is \
         that it yields"
    );

    let _ = joiner.join();
}

#[test]
fn a_peer_that_hangs_up_mid_handshake_is_a_failure_and_not_a_hang() {
    // The reversal that matters for a non-blocking driver: it must end. A loop
    // that returns `Pending` forever on a dead socket is exactly as broken as
    // one that blocks, and it fails by hanging — the shape
    // `docs/reference/a-reversal-can-fail-by-hanging.md` is about.
    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let addr = listener.local_addr().expect("an address");
    let hangup = std::thread::spawn(move || {
        let sock = TcpStream::connect(addr).expect("connects");
        drop(sock);
    });

    let (sock, _) = listener.accept().expect("the client connects");
    let mut transport = TcpTransport::new(sock).expect("non-blocking");
    let conn = rustls::server::UnbufferedServerConnection::new(server_config(cert, key))
        .expect("a server connection");
    let mut hs = Handshake::new(conn);

    let mut sweeps = 0usize;
    let outcome = loop {
        sweeps += 1;
        assert!(sweeps < 100_000, "a dead socket never ended the handshake");
        match hs.pump(&mut transport) {
            Step::Pending => std::thread::yield_now(),
            other => break other,
        }
    };
    assert!(
        matches!(outcome, Step::Failed(_)),
        "a peer that hung up must be a failure, got {outcome:?}"
    );
    let _ = hangup.join();
}

#[test]
fn a_logon_sent_straight_after_finished_is_not_lost() {
    // **The trap the plan named before any code was written, and the first run
    // of the test above walked into it.** A counterparty that sends its `Logon`
    // immediately after `Finished` puts application records on the wire while
    // this end is still completing the handshake. rustls then reports
    // `ReadTraffic` rather than `WriteTraffic`, and an implementation that
    // treated only `WriteTraffic` as completion refused the connection outright
    // — which is what happened here, as `Failed(InvalidData)`.
    //
    // Losing those bytes instead would have been worse than refusing them: a
    // FIX session whose `Logon` vanished does not fail, it **times out**, tens
    // of seconds later, with nothing naming the cause.
    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let addr = listener.local_addr().expect("an address");
    let joiner = client_thread(addr, cert.clone());

    let (sock, _) = listener.accept().expect("the client connects");
    let mut transport = TcpTransport::new(sock).expect("non-blocking");
    let conn = rustls::server::UnbufferedServerConnection::new(server_config(cert, key))
        .expect("a server connection");
    let mut hs = Handshake::new(conn);

    let mut sweeps = 0usize;
    let mut pendings = 0usize;
    let outcome = loop {
        sweeps += 1;
        assert!(sweeps < 100_000, "the handshake never finished");
        match hs.pump(&mut transport) {
            Step::Pending => {
                pendings += 1;
                std::thread::yield_now();
            }
            other => break other,
        }
    };
    assert_eq!(outcome, Step::Done);

    // The client writes `hello` the instant the handshake allows it, standing
    // in for a `Logon`. It must survive the handover, and it is plaintext:
    // rustls decrypted it, which is also what keeps the kernel's sequence
    // number right — see `Handshake::take_early_data`.
    let early = hs.take_early_data();
    probe_after_done(
        "logon_after_finished",
        sweeps,
        pendings,
        early.len(),
        hs.leftover(),
        &mut transport,
    );
    assert_eq!(
        early, b"hello",
        "application data that arrived before the handover was lost — a real          counterparty's Logon would vanish and the session would time out"
    );
    // Taken means taken: a second call must not hand the same bytes to the
    // session twice, which on a FIX session is a duplicate sequence number.
    assert!(hs.take_early_data().is_empty());
    // And the ciphertext buffer is empty, which is ADR-0018's third condition
    // and the thing that is NOT satisfied by having read the plaintext.
    assert_eq!(hs.leftover(), 0);

    let _ = joiner.join();
}

#[test]
fn only_kernel_and_plain_keep_the_hot_path() {
    // ADR-0005 open question 3, and what `TlsRequireKernel=Y` will read.
    assert!(TlsMode::Plain.keeps_the_hot_path());
    assert!(TlsMode::Kernel.keeps_the_hot_path());
    assert!(
        !TlsMode::Userspace.keeps_the_hot_path(),
        "userspace copies once per direction and allocates; ADR-0005 decision 3 \
         requires that be named rather than discovered in a histogram"
    );
}

/// `/proc/net/tls_stat`'s cumulative counters. Cumulative, not the `TlsCurr*`
/// gauges: a gauge reads whatever is open at the instant it is sampled, and the
/// spike recorded `[measured 2026-08-31]` that asserting on `TlsCurrTxSw` read
/// `0 -> 1` for a pair that had plainly offloaded both ends.
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

#[test]
fn after_the_handover_the_kernel_holds_the_keys_and_read_returns_plaintext() {
    // Step 3's whole claim, and it is asserted three ways rather than one,
    // because "it worked" is passable by an implementation that never offloaded
    // anything: plaintext round trip, the mode this engine reports, and the
    // kernel's own counters.
    use fixbolt_engine::tls::TlsTransport;
    use fixbolt_engine::transport::{Io, Transport};

    let before_tx = tls_stat("TlsTxSw");
    let before_rx = tls_stat("TlsRxSw");

    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let addr = listener.local_addr().expect("an address");

    // A blocking rustls client that speaks first and then listens, so both
    // directions are exercised.
    let cc = cert.clone();
    let joiner = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let cfg = client_config(cc);
        let name = "localhost".try_into().expect("a valid server name");
        let mut conn = rustls::ClientConnection::new(cfg, name)
            .map_err(|e| std::io::Error::other(format!("{e}")))?;
        let mut sock = TcpStream::connect(addr)?;
        let mut tls = rustls::Stream::new(&mut conn, &mut sock);
        tls.write_all(b"8=FIX.4.4|LOGON|")?;
        tls.flush()?;
        let mut back = vec![0u8; 32];
        let n = std::io::Read::read(&mut tls, &mut back)?;
        back.truncate(n);
        Ok(back)
    });

    let (sock, _) = listener.accept().expect("the client connects");
    let transport = TcpTransport::new(sock).expect("non-blocking");
    let conn = rustls::server::UnbufferedServerConnection::new(server_config(cert, key))
        .expect("a server connection");
    let mut tls = TlsTransport::new(transport, Handshake::new(conn));

    // Drive it the way the engine does: ask, and take `Idle` for an answer.
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    let mut sweeps = 0usize;
    while got.len() < b"8=FIX.4.4|LOGON|".len() {
        sweeps += 1;
        assert!(sweeps < 200_000, "nothing arrived: got {got:?}");
        match tls.recv(&mut buf) {
            Io::Ready(n) => got.extend_from_slice(buf.get(..n).unwrap_or_default()),
            Io::Idle => std::thread::yield_now(),
            other => panic!("recv said {other:?}"),
        }
    }

    assert_eq!(
        got, b"8=FIX.4.4|LOGON|",
        "the bytes the session sees must be plaintext, in order, with the \
         pre-handover Logon first"
    );
    assert!(tls.is_ready(), "the handover did not happen");
    assert_eq!(
        tls.mode(),
        TlsMode::Kernel,
        "ADR-0005 open question 3: a session that silently stayed in userspace \
         publishes a latency number about a different code path"
    );

    // Write back through the kernel, which is the other direction and a
    // different `setsockopt`.
    let mut sent = 0usize;
    let reply = b"35=A|";
    let mut sweeps = 0usize;
    while sent < reply.len() {
        sweeps += 1;
        assert!(sweeps < 200_000, "the reply never went out");
        match tls.send(reply.get(sent..).unwrap_or_default()) {
            Io::Ready(n) => sent += n,
            Io::Idle => std::thread::yield_now(),
            other => panic!("send said {other:?}"),
        }
    }

    let echoed = joiner.join().expect("the client thread").expect("a read");
    assert_eq!(
        echoed, reply,
        "the client decrypted what the kernel encrypted, so the TX keys are \
         really in the kernel and not merely accepted by it"
    );

    // **The assertion no amount of application-level success can fake.** A
    // userspace fallback would pass every line above and move neither counter.
    assert!(
        tls_stat("TlsTxSw") > before_tx,
        "/proc/net/tls_stat TlsTxSw did not move: the kernel never took the \
         send keys, so this connection was not offloaded"
    );
    assert!(
        tls_stat("TlsRxSw") > before_rx,
        "/proc/net/tls_stat TlsRxSw did not move: the kernel never took the \
         receive keys"
    );
}

#[test]
fn the_userspace_fallback_carries_the_same_bytes_and_says_it_is_not_the_kernel() {
    // ADR-0005 decision 3. **This path is reached only when the kernel refuses
    // the offload, and this desk's kernel does not refuse** — so without
    // `with_offload(false)` the fallback would be code that compiles and has
    // never run once. `CLAUDE.md` §10: a check that nothing reads proves
    // nothing.
    //
    // The assertions are deliberately the same ones the kTLS test makes, plus
    // the two that must come out the other way: it works, and it says it is not
    // the kernel.
    use fixbolt_engine::tls::TlsTransport;
    use fixbolt_engine::transport::{Io, Transport};

    let (cert, key) = pki();
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let addr = listener.local_addr().expect("an address");

    let cc = cert.clone();
    let joiner = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let cfg = client_config(cc);
        let name = "localhost".try_into().expect("a valid server name");
        let mut conn = rustls::ClientConnection::new(cfg, name)
            .map_err(|e| std::io::Error::other(format!("{e}")))?;
        let mut sock = TcpStream::connect(addr)?;
        let mut tls = rustls::Stream::new(&mut conn, &mut sock);
        tls.write_all(b"8=FIX.4.4|LOGON|")?;
        tls.flush()?;
        let mut back = vec![0u8; 32];
        let n = std::io::Read::read(&mut tls, &mut back)?;
        back.truncate(n);
        Ok(back)
    });

    let (sock, _) = listener.accept().expect("the client connects");
    let transport = TcpTransport::new(sock).expect("non-blocking");
    let conn = rustls::server::UnbufferedServerConnection::new(server_config(cert, key))
        .expect("a server connection");
    let mut tls = TlsTransport::with_offload(transport, Handshake::new(conn), false);

    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    let mut sweeps = 0usize;
    while got.len() < b"8=FIX.4.4|LOGON|".len() {
        sweeps += 1;
        assert!(sweeps < 200_000, "nothing arrived: got {got:?}");
        match tls.recv(&mut buf) {
            Io::Ready(n) => got.extend_from_slice(buf.get(..n).unwrap_or_default()),
            Io::Idle => std::thread::yield_now(),
            other => panic!("recv said {other:?}"),
        }
    }
    assert_eq!(
        got, b"8=FIX.4.4|LOGON|",
        "the fallback must carry the same bytes in the same order, including \
         the Logon that arrived during the handshake"
    );

    let mut sent = 0usize;
    let reply = b"35=A|";
    let mut sweeps = 0usize;
    while sent < reply.len() {
        sweeps += 1;
        assert!(sweeps < 200_000, "the reply never went out");
        match tls.send(reply.get(sent..).unwrap_or_default()) {
            Io::Ready(n) => sent += n,
            Io::Idle => std::thread::yield_now(),
            other => panic!("send said {other:?}"),
        }
    }
    let echoed = joiner.join().expect("the client thread").expect("a read");
    assert_eq!(echoed, reply, "the client could not decrypt the reply");

    // **The two that must come out the other way.** A fallback that worked and
    // reported `Kernel` would be worse than one that failed: every latency
    // figure published from it would describe a code path the session is not
    // on. ADR-0005 open question 3.
    assert_eq!(tls.mode(), TlsMode::Userspace);
    assert!(!tls.mode().keeps_the_hot_path());
    assert!(
        tls.fell_back(),
        "the engine has to be able to say this happened, or nobody finds out \
         until a histogram looks wrong"
    );
    assert!(!tls.is_ready(), "is_ready means the keys are in the kernel");
}
