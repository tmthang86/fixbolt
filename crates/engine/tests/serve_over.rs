//! `serve_over` over FIX 4.4 is `serve` (ADR-0207 decision 5; plan
//! `2026-09-26-docs-for-embedders`, steps 25 and 26).
//!
//! **What this proves:** the same bytes, sent over a real socket to `serve`
//! and to `serve_over::<…, TagValue<Fix44, 256>, …>`, get the same answers
//! back, once `SendingTime (52)` and the `CheckSum (10)` that depends on it
//! are masked. The plan's trap is a `_over` door written as a copy of the old
//! one, so that the two bodies drift apart; the old door calling the new one
//! is the fix, and this test is what notices a drift that changes the wire.
//!
//! **What it does not prove:** that `serve_over` over a dictionary other than
//! FIX 4.4 answers by that dictionary — `examples/custom-dictionary` does
//! that (plan step 27) — nor anything about `serve_hft_over` or
//! `connect_and_serve_over`, which have no test here.
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use fixbolt_engine::journal::Store;
use fixbolt_engine::msglog::NoLog;
use fixbolt_engine::observe::Handles;
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::recovery::NoRecovery;
use fixbolt_engine::{Application, Fix44, ServeError, Shutdown, TagValue};
use fixbolt_session::Config;

#[derive(Default)]
struct EchoApp(fixbolt_conformance::echo::Echo);

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        self.0.reply(msg, hdr.seq, hdr.stamp, out)
    }
}

fn free_port() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

fn table() -> Table {
    Table::new().serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
}

fn limits() -> Limits {
    Limits::new(8, 30_000).expect("both above zero")
}

fn stamp() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = cache.format(ms, 0);
    core::str::from_utf8(&full[..17]).expect("ascii").to_owned()
}

fn framed(inner: &str) -> Vec<u8> {
    let msg = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(msg.as_bytes())
}

/// A `Logon`, a `NewOrderSingle` the echo application answers, and a
/// `TestRequest` the session answers by itself — the application path and
/// the session path, each once. Built once, so both doors get the same bytes.
fn script() -> [Vec<u8>; 3] {
    let s = stamp();
    [
        framed(&format!(
            "35=A\u{1}34=1\u{1}49=TW44\u{1}52={s}\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}"
        )),
        framed(&format!(
            "35=D\u{1}34=2\u{1}49=TW44\u{1}52={s}\u{1}56=ISLD\u{1}\
             11=ORD-1\u{1}21=1\u{1}38=100\u{1}40=1\u{1}54=1\u{1}55=IBM\u{1}60={s}\u{1}"
        )),
        framed(&format!(
            "35=1\u{1}34=3\u{1}49=TW44\u{1}52={s}\u{1}56=ISLD\u{1}112=PING\u{1}"
        )),
    ]
}

/// A connection to `addr`, or `None` once the serving thread has returned —
/// a door that fails at startup is a result to report, not a hang.
fn connect(addr: &str, serving: &JoinHandle<Result<Shutdown, ServeError>>) -> Option<TcpStream> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(s) = TcpStream::connect(addr) {
            s.set_nodelay(true).expect("nodelay");
            s.set_read_timeout(Some(Duration::from_millis(200)))
                .expect("timeout");
            return Some(s);
        }
        if serving.is_finished() {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

/// Read until `messages` complete messages have arrived in `all`, or five
/// seconds pass.
fn read_until(c: &mut TcpStream, all: &mut Vec<u8>, messages: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut buf = [0u8; 16384];
    while count_messages(all) < messages && Instant::now() < deadline {
        match c.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => all.extend_from_slice(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => panic!("neither an answer nor a close: {e}"),
        }
    }
}

fn count_messages(all: &[u8]) -> usize {
    all.windows(4).filter(|w| *w == b"\x0110=").count()
}

/// Everything the door said to [`script`], one message at a time, with the
/// two fields that depend on the clock masked.
fn transcript(c: &mut TcpStream, script: &[Vec<u8>; 3]) -> String {
    let mut all = Vec::new();
    for (i, msg) in script.iter().enumerate() {
        c.write_all(msg).expect("send");
        read_until(c, &mut all, i + 1);
    }
    mask(&String::from_utf8_lossy(&all).replace('\u{1}', "|"))
}

/// `52=…` and `10=…` replaced by placeholders; nothing else touched.
fn mask(wire: &str) -> String {
    wire.split('|')
        .map(|f| {
            if f.starts_with("52=") {
                "52=<time>"
            } else if f.starts_with("10=") {
                "10=<sum>"
            } else {
                f
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn spawn<F>(serve: F) -> JoinHandle<Result<Shutdown, ServeError>>
where
    F: FnOnce() -> Result<Shutdown, ServeError> + Send + 'static,
{
    std::thread::spawn(serve)
}

/// **The specification of the door.** `serve_over` named with FIX 4.4 answers
/// a logon, an order and a test request with the messages `serve` answers
/// them with.
#[test]
fn serve_over_with_fix44_answers_like_serve() {
    let script = script();

    // --- the old door: the baseline ---
    let plain_addr = free_port();
    let a = plain_addr.clone();
    let plain = spawn(move || {
        fixbolt_engine::serve(
            &a,
            table(),
            EchoApp::default(),
            4,
            limits(),
            NoLog,
            Handles::new(),
        )
    });
    let mut c = connect(&plain_addr, &plain).expect("serve never came up");
    let want = transcript(&mut c, &script);
    assert!(
        want.contains("|35=A|") && want.contains("|112=PING|") && want.matches("|10=").count() == 3,
        "the baseline itself is not a logon, an answer and a heartbeat: {want}"
    );

    // --- the new door, over the same encoding ---
    let over_addr = free_port();
    let a = over_addr.clone();
    let over = spawn(move || {
        fixbolt_engine::serve_over::<256, 4096, 8192, 1024, TagValue<Fix44, 256>, _, Store, _, _>(
            &a,
            table(),
            EchoApp::default(),
            4,
            limits(),
            NoRecovery,
            NoLog,
            Handles::new(),
        )
    });
    let conn = connect(&over_addr, &over);
    let returned = if conn.is_none() && over.is_finished() {
        format!("{:?}", over.join())
    } else {
        String::from("still running")
    };
    assert!(
        conn.is_some(),
        "serve_over never accepted a connection on {over_addr}; the door returned: {returned}"
    );
    let mut c = conn.expect("asserted above");
    let got = transcript(&mut c, &script);

    assert_eq!(
        got, want,
        "serve_over over FIX 4.4 did not answer as serve does"
    );
}
