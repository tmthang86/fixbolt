//! **A configuration file on disk, and a TLS session logging on from it.**
//!
//! Step 4c-2 of [tls] ([Sửa 5]'s table), and the reason 4c-1 does not merge
//! alone. 4c-1 added four settings keys — `SocketUseSSL`,
//! `ServerCertificateFile`, `ServerCertificateKeyFile`, `TlsRequireKernel` —
//! and [`Settings::into_tls_table`]. **Nothing read them.** A settings key
//! nothing reads is a promise in a document, and the promise here is the whole
//! path: a `.cfg` file an operator writes, ending in an encrypted FIX session.
//!
//! # The gap this file closes is a shape gap
//!
//! [`TlsSettings`] carries **paths** — deliberately, because a configuration
//! parser that reads the filesystem fails for two unrelated reasons with one
//! message. Every `serve_tls*` entry point takes **DER**. Nothing in the engine
//! read PEM, so the two ends of the documented path did not meet.
//! [`fixbolt_engine::tls::load_pem`] is the joint, and this file is what says
//! the joint holds under a real socket rather than under a unit test.
//!
//! # What this file does NOT prove
//!
//! - **Not which [`TlsMode`] carried the bytes**, for the same reason
//!   `tests/tls_wire.rs` says it does not: a userspace fallback logs on exactly
//!   like a kTLS session. `require_kernel` is passed as `false` here on
//!   purpose — this desk's kernel support is not the thing under test, and a
//!   test that needed it would be red on a machine where the path from a `.cfg`
//!   to a session is perfectly fine.
//! - **No latency claim.** Nothing here is timed.
//! - **Nothing about CI.** `[measured 2026-09-12]` the `tls` job runs this
//!   binary three times because the TLS tests have been flaky on GitHub's
//!   runner; a single green run on one desk is one observation.
//!
//! # The reversals
//!
//! `[measured 2026-09-12]` the file was **red first**, on
//! `cannot find function `load_pem` in module `fixbolt_engine::tls`` — it was
//! written before the function it calls.
//!
//! 1. **The plan's reversal: the certificate and the key the wrong way round.**
//!    Predicted red **at `load_pem`, before the listener binds** — not at the
//!    socket, because a loader that accepts a private key where a certificate
//!    belongs and says nothing is the finding, not the pass. Red as predicted,
//!    at the `load_pem` call:
//!    `ServerCertificateFile …/server.crt holds no CERTIFICATE section`, with
//!    the whole binary finishing in `0.00s` — nothing bound.
//!
//!    **And the literal wording of it does not reach `load_pem` at all.**
//!    Swapping the two *paths* in the `.cfg` is intercepted one assertion
//!    earlier, by `tls.certificate() == cert_path` — red on
//!    *"the path the file named is the path that came back"*, `load_pem` never
//!    called. Predicted before running, and it is the reason the swap above is
//!    done at the **bytes**: an assertion standing in front of the thing under
//!    test turns a reversal into a test of the assertion. Same class as
//!    `docs/reference/a-matcher-excluded-the-separator-every-real-name-uses.md`
//!    — the reversal ran, something went red, and it was not the thing.
//! 2. **A certificate that is taken off disk after the `.cfg` names it.** Paths
//!    unchanged, so nothing intercepts. Predicted red at the same `load_pem`
//!    call with a **different** sentence, and it was:
//!    `ServerCertificateFile …/server.crt could not be opened: No such file or
//!    directory (os error 2)`. A file that is missing and a file that holds the
//!    wrong thing are two operator mistakes and must not read the same.
//!
//!    The first draft of that sentence read `could not be opened: I/O error: No
//!    such file or directory` — `PemError`'s own `Display` prefixes `I/O
//!    error:` and the message said it twice. Only running the reversal showed
//!    it; the assertion in
//!    `a_certificate_path_that_names_no_file_is_refused_by_name` is green
//!    either way, because it checks the key and the path and not the prose.
//!
//! Both predictions were written down before either was run —
//! `docs/reference/a-matcher-excluded-the-separator-every-real-name-uses.md` is
//! a guard whose own reversal went green because nobody had written down what
//! red should look like.
//!
//! [tls]: ../../../docs/plans/2026-09-04-tls.md
//! [Sửa 5]: ../../../docs/plans/2026-09-04-tls.md
//! [`Settings::into_tls_table`]: fixbolt_engine::settings::Settings::into_tls_table
//! [`TlsSettings`]: fixbolt_engine::settings::TlsSettings
//! [`TlsMode`]: fixbolt_engine::tls::TlsMode

// Three gates, named here rather than copied from a sibling: `rustls` is behind
// `tls`, the pre-session poller every `serve*` needs is behind `standard`, and
// kTLS is Linux. Non-negotiable 6 wants the gate on the item itself.
#![cfg(all(feature = "tls", feature = "standard", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fixbolt_engine::Application;
use fixbolt_engine::observe::{EventKind, Handles};
use fixbolt_engine::presession::Limits;
use fixbolt_engine::settings::Settings;

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

/// A directory of its own per test, removed when the test ends.
///
/// **`Drop`, not a line at the end of the test.** A test that fails on an
/// assertion leaves its temporary PEMs behind for ever otherwise, and the
/// reversals below are *expected* to fail.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "fixbolt-tls-settings-wire-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("a scratch directory");
        Self(p)
    }

    fn path(&self, leaf: &str) -> PathBuf {
        self.0.join(leaf)
    }

    /// Write `body` to `leaf` and return the path, as an operator would.
    fn write(&self, leaf: &str, body: &str) -> PathBuf {
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

/// A self-signed certificate and its key, **as PEM text** — the form an
/// operator's `ServerCertificateFile` actually holds.
///
/// `tests/tls.rs` and `tests/tls_wire.rs` keep theirs in DER because they hand
/// it straight to `serve_tls`. The whole point of this file is the step those
/// two skip, so the PEM is the artefact and the DER is what `load_pem` must
/// produce from it.
fn pki_pem() -> (String, String) {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("a self-signed certificate");
    (ck.cert.pem(), ck.signing_key.serialize_pem())
}

fn free_addr() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

/// The `.cfg` an operator writes to turn TLS on, with the two paths in it.
fn cfg_text(cert: &Path, key: &Path) -> String {
    format!(
        "[DEFAULT]\n\
         BeginString=FIX.4.4\n\
         SenderCompID=ISLD\n\
         SocketUseSSL=Y\n\
         ServerCertificateFile={}\n\
         ServerCertificateKeyFile={}\n\
         [SESSION]\n\
         TargetCompID=TW44\n\
         StartTime=00:00:00\n\
         EndTime=23:59:59\n",
        cert.display(),
        key.display()
    )
}

/// TLS 1.3, `AES-128-GCM` only — the narrowing kTLS forces, on the client side.
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
/// **Stamped now, not from the corpus**: `serve_tls_requiring` builds a
/// `SystemClock`, and the corpus's fixed instant would be refused by
/// `max_skew_ms` — a clock refusal and an unknown counterparty are the same
/// silence on the wire (`docs/reference/two-time-rules-share-one-observable.md`).
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

/// **The whole path: a `.cfg` file on disk, and a FIX session on an encrypted
/// socket.**
///
/// `Settings::parse` → `into_tls_table` → `load_pem` → `serve_tls_requiring` →
/// an ordinary `rustls` client. Nothing between the file and the wire is
/// hand-assembled, which is the sentence 4c-1 could not say.
#[test]
fn a_configuration_file_brings_a_tls_session_up() {
    let scratch = Scratch::new("up");
    let (cert_pem, key_pem) = pki_pem();
    let cert_path = scratch.write("server.crt", &cert_pem);
    let key_path = scratch.write("server.key", &key_pem);
    let text = cfg_text(&cert_path, &key_path);
    let cfg_path = scratch.write("acceptor.cfg", &text);

    // Read back from disk, so the file is what is under test rather than the
    // string that wrote it.
    let on_disk = std::fs::read_to_string(&cfg_path).expect("the .cfg is readable");
    let settings =
        Settings::parse(&on_disk).unwrap_or_else(|e| panic!("the .cfg does not parse: {e}"));
    let (table, tls) = settings
        .into_tls_table()
        .unwrap_or_else(|e| panic!("the TLS door refused the file: {e}"));

    assert_eq!(
        tls.certificate(),
        cert_path.as_path(),
        "the path the file named is the path that came back"
    );
    assert!(
        !tls.require_kernel(),
        "TlsRequireKernel is absent from this file, so it is off — ADR-0060"
    );

    let (certs, key) = fixbolt_engine::tls::load_pem(&tls)
        .unwrap_or_else(|e| panic!("load_pem refused the PEM written by rcgen: {e}"));
    assert_eq!(certs.len(), 1, "one self-signed certificate, no chain");
    let client_cert = certs[0].clone();

    let addr = free_addr();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();

    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve_tls_requiring(
            &serving,
            table,
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            serving_handles,
            certs,
            key,
            false,
        )
    });

    // The client, on this thread. It blocks; it is not the code under test.
    let name = "localhost".try_into().expect("a valid server name");
    let mut conn =
        rustls::ClientConnection::new(client_config(client_cert), name).expect("a client");
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
    let mut tls_stream = rustls::Stream::new(&mut conn, &mut sock);

    tls_stream
        .write_all(&logon_now(1))
        .expect("the Logon goes out");
    tls_stream.flush().expect("flushed");

    let mut buf = [0u8; 4096];
    let n = tls_stream.read(&mut buf).expect("a Logon comes back");
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
        "serve_tls_requiring came back with an error: {stopped:?}"
    );
}

/// **A certificate path that names no file says so, and names the key.**
///
/// The first of the four operator mistakes `load_pem` separates. It must not
/// read like "the PEM was empty", because the fix is different: one is a typo
/// in a path, the other is the wrong file at a right path.
#[test]
fn a_certificate_path_that_names_no_file_is_refused_by_name() {
    let scratch = Scratch::new("nocert");
    let (_, key_pem) = pki_pem();
    let key_path = scratch.write("server.key", &key_pem);
    let missing = scratch.path("not-here.crt");
    let text = cfg_text(&missing, &key_path);

    let settings = Settings::parse(&text).unwrap_or_else(|e| panic!("{e}"));
    let (_table, tls) = settings.into_tls_table().unwrap_or_else(|e| panic!("{e}"));

    let err = fixbolt_engine::tls::load_pem(&tls)
        .expect_err("a certificate that is not on disk cannot be served");
    let said = format!("{err}");
    assert!(
        said.contains("ServerCertificateFile") && said.contains("not-here.crt"),
        "the refusal names neither the key nor the path an operator must fix: {said}"
    );
}

/// **A key file holding no private key says *that*, not "missing file".**
///
/// Here the path is right and the *contents* are wrong — the shape the plan's
/// reversal produces by swapping the two paths. Kept as a standing test so the
/// distinction survives whoever edits `load_pem` next.
#[test]
fn a_key_file_holding_no_private_key_is_refused_by_name() {
    let scratch = Scratch::new("nokey");
    let (cert_pem, _) = pki_pem();
    let cert_path = scratch.write("server.crt", &cert_pem);
    // The certificate, handed in as the key: a real file, real PEM, no key.
    let key_path = scratch.write("server.key", &cert_pem);
    let text = cfg_text(&cert_path, &key_path);

    let settings = Settings::parse(&text).unwrap_or_else(|e| panic!("{e}"));
    let (_table, tls) = settings.into_tls_table().unwrap_or_else(|e| panic!("{e}"));

    let err = fixbolt_engine::tls::load_pem(&tls)
        .expect_err("a file with no private key in it is not a private key");
    let said = format!("{err}");
    assert!(
        said.contains("ServerCertificateKeyFile") && said.contains("server.key"),
        "the refusal does not name the key an operator must fix: {said}"
    );
    assert!(
        said.contains("no PRIVATE KEY"),
        "a present-but-wrong file must not read like a missing one: {said}"
    );
}
