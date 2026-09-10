//! A FIX session brought up **through the front door, over TLS**.
//!
//! **Step 4a of [tls], the step [Sửa 3] added because no step owned it.**
//!
//! # What was missing, and it was not the TLS code
//!
//! Steps 1-3 built `TlsTransport`: a handshake that yields instead of spinning,
//! a kTLS handover, and a userspace fallback that says it is not the kernel.
//! `crates/engine/tests/tls.rs` drives all three and is green.
//!
//! **And no deployment could reach any of it.** `[measured 2026-09-10]` every
//! `serve*` built `PendingSet<TcpTransport, presession::Table, RX>` outright
//! (`crates/engine/src/lib.rs:2207`), so the only way to get a `TlsTransport`
//! into an engine was to assemble the engine by hand — the same shape that made
//! every published `hft` figure describe a hand-built engine until
//! `hft_wire.rs`. The plan's own delivery log said so plainly: *"no `serve*`
//! reaches TLS; `presession.rs`, `lib.rs` and `settings.rs` are untouched."*
//!
//! So this file is the gate for the sentence *"a deployment can turn TLS on"*,
//! and before it that sentence was false.
//!
//! # What this file does NOT prove
//!
//! - **Not which [`TlsMode`] carried the bytes.** A session that fell back to
//!   userspace `rustls` comes up exactly like one on kTLS — that is the whole
//!   point of the fallback — so a test that reads a `Logon` back cannot tell
//!   them apart. Saying otherwise is the mode-mixing ADR-0013 decision 4
//!   forbids. The event and `TlsRequireKernel` are **step 4b**;
//!   `tests/tls.rs::after_the_handover_the_kernel_holds_the_keys_and_read_returns_plaintext`
//!   is what proves the kernel path at the transport level today.
//!
//!   **But it does not follow that every kernel refusal is invisible here**, and
//!   assuming it did was wrong: reversal 1 below expected a green run and got a
//!   red one. Only a `setup_ulp` refusal reaches the fallback. A refusal from
//!   `dangerous_into_kernel_connection` arrives after that call has consumed the
//!   rustls connection, so it ends the connection instead — which this file
//!   *does* see, as a dead socket rather than as a mode.
//! - **No latency claim.** Nothing here is timed and no figure belongs in
//!   `DESIGN.md` §8. Those are step 6, and they need `w2w --tls`.
//! - **Nothing about the engine thread.** The handshake runs on the acceptor
//!   thread, which ADR-0020 permits to block. Whether the engine thread stays
//!   out of the kernel under TLS is `scripts/check-no-kernel-sleep.sh` with an
//!   arm it does not have yet — step 6, and named as not done.
//!
//! # The four reversals
//!
//! `[measured 2026-09-10]` the test was **red first**, on
//! `cannot find function serve_tls` — this file was written before the function
//! it calls.
//!
//! 1. **`enable_secret_extraction = false` in `tls::server_config`.** Expected
//!    to stay **green**, on the reasoning that the session would simply fall
//!    back to userspace and serve. It went **red**: `ConnectionReset`. The
//!    prediction was wrong and the rustdoc written from it was wrong with it —
//!    both are corrected, and the correction is a better argument for
//!    `serve_tls` owning the config than the guess was.
//! 2. **`wrap` returns `None` for every socket.** Red: `UnexpectedEof`.
//! 3. **`serve_tls` swapped for `serve`.** Red: `WouldBlock` on the client's
//!    first write, because a plain acceptor never answers a `ClientHello`. This
//!    is the one that proves the test goes through TLS at all.
//! 4. **`wait_for_event` asked for `EndedWithoutReason`.** Red, reporting
//!    `the stream held [LoggedOn]` — so that assertion reads the event stream
//!    rather than returning a blind yes. **It needed its own reversal**:
//!    reversals 1-3 all died at the socket, before reaching it.
//!
//! [tls]: ../../../docs/plans/2026-09-04-tls.md
//! [Sửa 3]: ../../../docs/plans/2026-09-04-tls.md
//! [`TlsMode`]: fixbolt_engine::tls::TlsMode

// kTLS is Linux, `rustls` is behind `tls`, and the pre-session poller `serve*`
// needs is behind `standard` — non-negotiable 6 wants the `mod` gated and not
// only the manifest. `[measured 2026-09-10]` `shard_hft.rs` shipped a gate
// copied from a sibling whose target depended on less and CI caught it in the
// `--no-default-features --features affinity` step; this one names all three
// itself rather than borrowing.
#![cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fixbolt_engine::observe::{EventKind, Handles};
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::{Application, Config};

/// Answers an application message and nothing else.
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

/// A self-signed certificate, generated in the test.
///
/// The same helper as `tests/tls.rs`, deliberately copied rather than shared:
/// integration tests in this crate are each self-contained, and a `tests/common`
/// module for two callers would be the only one in 33 files.
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

/// TLS 1.3, `AES-128-GCM` only — the narrowing kTLS forces, on the client side.
///
/// The **server** side of this narrowing is not the test's to make any more:
/// `serve_tls` builds its own `rustls::ServerConfig`. `[measured 2026-09-10]`
/// reversal 1 of this file is what settled why that matters — flipping
/// `enable_secret_extraction` to `false` does not demote the session to
/// userspace, it **kills the connection**, and the counterparty reads
/// `ConnectionReset`. A constraint the type system cannot hold, held by not
/// offering the caller the choice.
fn provider() -> rustls::crypto::CryptoProvider {
    let mut p = rustls::crypto::ring::default_provider();
    p.cipher_suites
        .retain(|cs| cs.suite() == rustls::CipherSuite::TLS13_AES_128_GCM_SHA256);
    p
}

fn client_config(cert: rustls::pki_types::CertificateDer<'static>) -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert).expect("the certificate parses");
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3 is available")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(cfg)
}

/// A `Logon` stamped at the wall clock.
///
/// **Stamped now, not from the corpus.** `serve_tls` builds a `SystemClock`, so
/// the corpus's fixed instant would be refused by `max_skew_ms` — and a clock
/// refusal and an unknown counterparty are the same silence on the wire, which
/// is what `docs/reference/two-time-rules-share-one-observable.md` records.
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

/// Wait for `kind` on the stream, or say what did arrive.
///
/// **This is what separates "it served" from "it did not crash".** A test that
/// only reads bytes off the socket passes against an engine that answered from
/// the pre-session stage and never brought a session up.
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

/// **`serve_tls` binds, completes a TLS handshake with an ordinary `rustls`
/// client, and brings a FIX session up on the encrypted socket.**
///
/// The counterparty is deliberately the plain buffered `rustls::Stream` API: the
/// thing under test is this engine's non-blocking acceptor, and it has to work
/// with a client that knows nothing about it.
#[test]
fn serve_tls_brings_a_session_up_through_tls() {
    let addr = free_addr();
    let (cert, key) = pki();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();
    let serving_cert = cert.clone();

    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve_tls(
            &serving,
            Table::with_capacity(1).serving(cfg()),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            serving_handles,
            vec![serving_cert],
            key,
        )
    });

    // The client, on this thread. It blocks; it is not the code under test.
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

    tls.write_all(&logon_now(1)).expect("the Logon goes out");
    tls.flush().expect("flushed");

    let mut buf = [0u8; 4096];
    let n = tls.read(&mut buf).expect("a Logon comes back");
    let reply = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");

    assert!(
        reply.contains("35=A") && reply.contains("49=ISLD") && reply.contains("56=TW44"),
        "the reply that came back over TLS was not this acceptor's Logon: {reply}"
    );

    let seen = wait_for_event(&handles, &EventKind::LoggedOn, Duration::from_secs(5));
    assert!(
        seen.iter().any(|k| k == &EventKind::LoggedOn),
        "no session came up; the stream held {seen:?}"
    );

    admin.shutdown(2_000);
    let stopped = engine.join().expect("the serving thread did not panic");
    assert!(
        stopped.is_ok(),
        "serve_tls came back with an error: {stopped:?}"
    );
}
