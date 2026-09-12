//! **Which of ADR-0005's three modes carried the bytes, answered by something
//! that fails rather than something that prints.**
//!
//! Step 4b of [tls], deciding [ADR-0060], closing [ADR-0005] open question 3 —
//! open since 2026-08-27.
//!
//! # What was wrong after step 4a
//!
//! `serve_tls` worked and **nothing read [`TlsMode`]**. A session that
//! negotiated its way onto userspace `rustls` served correctly, looked identical
//! from outside, and would have published latency figures describing a different
//! code path. That is the outcome ADR-0005 decision 3 says must be impossible to
//! reach by accident, and after 4a it was reachable in silence.
//!
//! # The two refusals, and why one of them is not enough
//!
//! ADR-0060: **two different things deny you the kernel**, and they are knowable
//! at different moments.
//!
//! | What went wrong | Knowable | Test here |
//! |---|---|---|
//! | The kernel has no TLS ULP at all | before any connection | `a_deployment_that_demands_the_kernel_will_not_start_without_it` |
//! | The kernel is capable and *this handshake* landed in userspace | per connection | `a_session_that_fell_back_is_ended_when_the_kernel_was_demanded` |
//!
//! A startup probe alone would leave the second invisible while *appearing* to
//! have answered question 3 — worse than leaving it open. A per-connection check
//! alone would only ever report a misconfigured host through a dropped session,
//! against a counterparty that had nothing to do with it.
//!
//! # What this file does NOT prove
//!
//! - **No latency claim.** Reporting the mode makes a future figure legible; it
//!   does not make one. `DESIGN.md` §8's TLS row stays empty until step 6.
//! - **Nothing about the engine thread.** `check-no-kernel-sleep.sh` still has
//!   no TLS arm.
//! - **Nothing in CI.** `[measured 2026-09-10]` no job runs `cargo test` with
//!   `--features tls`; `STATUS.md` item 62 and its plan. This file has run on
//!   one desktop.
//!
//! [tls]: ../../../docs/plans/2026-09-04-tls.md
//! [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
//! [ADR-0005]: ../../../docs/decisions/ADR-0005-tls.md
//! [`TlsMode`]: fixbolt_engine::transport::TlsMode

#![cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fixbolt_engine::observe::{EventKind, Handles};
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::tls::TlsProbe;
use fixbolt_engine::transport::{TcpTransport, TlsMode, Transport};
use fixbolt_engine::{Application, Config};

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

fn free_addr() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

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

fn provider() -> rustls::crypto::CryptoProvider {
    let mut p = rustls::crypto::ring::default_provider();
    p.cipher_suites
        .retain(|cs| cs.suite() == rustls::CipherSuite::TLS13_AES_128_GCM_SHA256);
    p
}

fn client_config(cert: rustls::pki_types::CertificateDer<'static>) -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert).expect("the certificate parses");
    Arc::new(
        rustls::ClientConfig::builder_with_provider(Arc::new(provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .expect("TLS 1.3 is available")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

fn logon_now(seq: u32) -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = cache.format(now, 0);
    let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
    let inner = format!(
        "35=A\u{1}34={seq}\u{1}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}"
    );
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

/// Collect events until **every** `kinds` has been seen, or time runs out.
///
/// **Accumulating, and that is not a nicety.** `[measured 2026-09-10]`
/// `Observer::events` **drains** the ring, so two separate waits do not both see
/// the same event: the first call takes it and the second reports an empty
/// stream. Asserting one kind and then waiting again for another read
/// `the stream held []` about an event that had already arrived. One reader,
/// one accumulator.
fn wait_for_all(handles: &Handles, kinds: &[EventKind], within: Duration) -> Vec<EventKind> {
    let observer = handles.observer();
    let mut seen = Vec::new();
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        let mut out = Vec::new();
        if observer.events(&mut out) > 0 {
            for e in out {
                seen.push(e.kind());
            }
            if kinds.iter().all(|k| seen.iter().any(|s| s == k)) {
                return seen;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    seen
}

fn wait_for_event(handles: &Handles, kind: &EventKind, within: Duration) -> Vec<EventKind> {
    let observer = handles.observer();
    let mut seen = Vec::new();
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        let mut out = Vec::new();
        if observer.events(&mut out) > 0 {
            for e in out {
                seen.push(e.kind());
            }
            if seen.iter().any(|k| k == kind) {
                return seen;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    seen
}

/// **A plain socket answers `Plain`, and every transport in the workspace still
/// compiles.**
///
/// The point of the default body. If this needed a `#[cfg]` or a cast, the
/// trait method would be the wrong shape.
#[test]
fn a_transport_that_knows_nothing_about_tls_says_plain() {
    let l = TcpListener::bind("127.0.0.1:0").expect("a port");
    let addr = l.local_addr().expect("bound");
    let client = std::thread::spawn(move || TcpStream::connect(addr).expect("connected"));
    let (sock, _) = l.accept().expect("accepted");
    let t = TcpTransport::new(sock).expect("non-blocking");
    assert_eq!(
        t.tls_mode(),
        TlsMode::Plain,
        "a TcpTransport carries no TLS, so it must say so rather than guess"
    );
    drop(client.join().expect("the client thread"));
}

/// **`TlsRequireKernel=Y` on a host that cannot offload refuses to bind.**
///
/// ADR-0060 decision 1, first half. The operator learns their host is wrong
/// **before a counterparty is affected**, from a sentence about the kernel.
///
/// This desk's kernel *can* offload, so the refusal is provoked with
/// [`TlsProbe::PretendKernelCannotOffload`]. Without that seam this path would
/// be code that compiles and has never run.
///
/// **It runs on a thread with a deadline, and that is not caution — it is a
/// defect this test had.** `[measured 2026-09-10]` reversal B deletes the
/// refusal, and the first version of this test **hung** instead of failing:
/// `serve_tls_with_offload` bound and served forever, so `expect_err` never
/// returned and the harness printed only *"has been running for over 60
/// seconds"*. A reversal that hangs says nothing about what broke — see
/// `docs/reference/a-reversal-can-fail-by-hanging.md`, which this test walked
/// straight into. With the deadline the same reversal is red in two seconds and
/// names the reason.
#[test]
fn a_deployment_that_demands_the_kernel_will_not_start_without_it() {
    let (cert, key) = pki();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let outcome = fixbolt_engine::serve_tls_with_offload::<256, 4096, 8192, 1024, _, _>(
            &free_addr(),
            Table::with_capacity(1).serving(cfg()),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            Handles::new(),
            vec![cert],
            key,
            true, // TlsRequireKernel=Y
            TlsProbe::PretendKernelCannotOffload,
        );
        let _ = tx.send(outcome.err().map(|e| format!("{e}")));
    });
    let answer = rx.recv_timeout(Duration::from_secs(5)).expect(
        "serve_tls_with_offload never came back: TlsRequireKernel=Y on a kernel that \
         cannot offload must refuse before it binds, and instead it started serving",
    );
    let said = answer.expect("it returned Ok, so it served a deployment that demanded the kernel");
    assert!(
        said.contains("kernel"),
        "the refusal must be about the kernel, not about a socket: {said}"
    );
}

/// **A handshake that lands in userspace is ended when the kernel was
/// demanded**, and the reason reaches the event stream.
///
/// ADR-0060 decision 1, second half — the case a startup probe cannot see,
/// because the kernel was capable and the suite was not.
#[test]
fn a_session_that_fell_back_is_ended_when_the_kernel_was_demanded() {
    let addr = free_addr();
    let (cert, key) = pki();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();
    let serving_cert = cert.clone();

    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve_tls_with_offload::<256, 4096, 8192, 1024, _, _>(
            &serving,
            Table::with_capacity(1).serving(cfg()),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            serving_handles,
            vec![serving_cert],
            key,
            true, // TlsRequireKernel=Y
            // The host is fine; this handshake is not. The arm a single boolean
            // could not express — see TlsProbe's own documentation.
            TlsProbe::PretendHandshakeFallsBack,
        )
    });

    let name = "localhost".try_into().expect("a valid server name");
    let mut conn = rustls::ClientConnection::new(client_config(cert), name).expect("a client");
    let mut sock = None;
    for _ in 0..500 {
        if let Ok(s) = TcpStream::connect(&addr) {
            s.set_nodelay(true).expect("nodelay");
            s.set_read_timeout(Some(Duration::from_secs(5)))
                .expect("a read timeout, so a hang fails instead of hanging");
            sock = Some(s);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut sock = sock.expect("the serving loop never bound the address");
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);
    // The handshake completes; the Logon may or may not reach a session before
    // the connection is ended, and this test deliberately does not care which.
    let _ = tls.write_all(&logon_now(1));
    let _ = tls.flush();

    // **Both assertions read one accumulator**, because the stream drains — see
    // `wait_for_all`. And the second of them was **missing** from the first
    // version of this test, whose name promised it: a test called
    // `…_is_ended_…` that only checks an event is the shape
    // `docs/reference/a-test-that-cannot-fail-reads-as-coverage.md` is about.
    let refused = EventKind::Ended(fixbolt_session::DropReason::RefusedByDeployment);
    let seen = wait_for_all(
        &handles,
        &[EventKind::TlsFellBackToUserspace, refused],
        Duration::from_secs(5),
    );
    assert!(
        seen.iter().any(|k| k == &EventKind::TlsFellBackToUserspace),
        "the fallback was never reported; the stream held {seen:?}"
    );
    assert!(
        seen.iter().any(|k| k == &refused),
        "TlsRequireKernel=Y reported the fallback and then served it anyway; \
         the stream held {seen:?}"
    );
    // **The reason must not be a protocol one**, which is the whole of item 63.
    // `[measured 2026-09-10]` before the fix this read
    // `Ended(SendingTimeOutOfRange)` — an accusation against a counterparty that
    // had done nothing wrong, with `last_skew_ms` reading about 2026 years.
    assert!(
        !seen.iter().any(|k| matches!(
            k,
            EventKind::Ended(fixbolt_session::DropReason::SendingTimeOutOfRange)
        )),
        "a local policy refusal was reported as a protocol fault; \
         the stream held {seen:?}"
    );
    assert!(
        !seen.iter().any(|k| k == &EventKind::LoggedOn),
        "a connection refused for TlsRequireKernel must not have logged on first; \
         the stream held {seen:?}"
    );

    admin.shutdown(2_000);
    let _ = engine.join().expect("the serving thread did not panic");
}

/// **`TlsRequireKernel=N` — the default — serves the fallback and still says so.**
///
/// ADR-0060 decision 2. The two halves of that decision are one assertion apart
/// and the difference is the whole point: **the report does not depend on the
/// strictness**. An operator who chose nothing still finds out.
#[test]
fn a_permissive_deployment_serves_the_fallback_and_reports_it_anyway() {
    let addr = free_addr();
    let (cert, key) = pki();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();
    let serving_cert = cert.clone();

    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve_tls_with_offload::<256, 4096, 8192, 1024, _, _>(
            &serving,
            Table::with_capacity(1).serving(cfg()),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            serving_handles,
            vec![serving_cert],
            key,
            false, // TlsRequireKernel=N, the default
            TlsProbe::PretendHandshakeFallsBack,
        )
    });

    let name = "localhost".try_into().expect("a valid server name");
    let mut conn = rustls::ClientConnection::new(client_config(cert), name).expect("a client");
    let mut sock = None;
    for _ in 0..500 {
        if let Ok(s) = TcpStream::connect(&addr) {
            s.set_nodelay(true).expect("nodelay");
            s.set_read_timeout(Some(Duration::from_secs(5)))
                .expect("a read timeout");
            sock = Some(s);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut sock = sock.expect("the serving loop never bound the address");
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);
    tls.write_all(&logon_now(1)).expect("the Logon goes out");
    tls.flush().expect("flushed");

    let mut buf = [0u8; 4096];
    let n = tls
        .read(&mut buf)
        .expect("a Logon comes back over the fallback");
    let reply = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");
    assert!(
        reply.contains("35=A") && reply.contains("49=ISLD"),
        "the permissive deployment did not serve: {reply}"
    );

    let seen = wait_for_event(
        &handles,
        &EventKind::TlsFellBackToUserspace,
        Duration::from_secs(5),
    );
    assert!(
        seen.iter().any(|k| k == &EventKind::TlsFellBackToUserspace),
        "a permissive deployment must still be told it left the kernel; \
         the stream held {seen:?}"
    );
    assert!(
        seen.iter().any(|k| k == &EventKind::LoggedOn),
        "the session should have come up on the fallback; the stream held {seen:?}"
    );

    admin.shutdown(2_000);
    let _ = engine.join().expect("the serving thread did not panic");
}
