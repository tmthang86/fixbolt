//! A configuration file, a real listener, and two counterparties logging on.
//!
//! **Step 4 of [a-registry-from-a-file].** Steps 1–3 prove the parser against
//! `Settings::parse`; this proves the whole path — file on disk, `Table`,
//! `serve`, kernel sockets, the real framer, the real session — because
//! `tests/registry.rs` and `tests/on_disk.rs` both found that a layer can be
//! finished while the seam above it is never asked about.
//!
//! # Why the hours are computed rather than written
//!
//! One test needs a counterparty whose trading window has **already closed**,
//! and the serving loop uses the real clock. A window written into the file as
//! a constant would be open or closed depending on what time the suite runs —
//! green all morning and red at six. So the file is written with a window that
//! ended a couple of hours ago, whatever "now" is.
//!
//! [a-registry-from-a-file]: ../../../docs/plans/2026-09-02-a-registry-from-a-file.md
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

use fixbolt_engine::Application;
use fixbolt_engine::presession::Limits;
use fixbolt_engine::settings::Settings;

#[derive(Default)]
struct EchoApp(fixbolt_conformance::echo::Echo);

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        self.0.reply(msg, seq, stamp, out)
    }
}

fn scratch(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "fixbolt-settings-wire-{name}-{}.cfg",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

fn free_port() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

fn connect(addr: &str) -> TcpStream {
    for _ in 0..200 {
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64
}

/// A `Logon` stamped now, from `who`.
///
/// Stamped now rather than taken from the corpus because the serving loop uses
/// the real clock and `max_skew_ms` would refuse the corpus's fixed instant —
/// **and a clock refusal and an unknown counterparty are the same silence**,
/// which is the confusion `two-time-rules-share-one-observable` records.
fn logon_now(who: &str) -> Vec<u8> {
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = *cache.format(now_ms());
    let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
    let inner =
        format!("35=A\u{1}34=1\u{1}49={who}\u{1}52={stamp}\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}");
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

/// Start `serve` on its own thread with the counterparties `text` names.
///
/// `counterparties` is asserted against the table that is actually served, and
/// it is not a formality. `[measured 2026-09-02]` a reversal that kept only the
/// **first** entry of the file left every test here green: an identity the
/// registry does not serve is refused with exactly the same silence as one
/// whose trading window has closed, so the wire cannot tell them apart. This is
/// where they are told apart — and asserting it on the parsed `Settings`
/// instead was not enough, because the loss was in `into_table`.
fn serving(name: &str, text: &str, counterparties: usize) -> String {
    let path = scratch(name);
    std::fs::write(&path, text).expect("the scratch directory is writable");
    let table = Settings::load(&path)
        .unwrap_or_else(|e| panic!("{e}"))
        .into_table();
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        table.len(),
        counterparties,
        "the acceptor must serve every counterparty the file names — a missing \
         one is refused in the same silence as one outside its hours"
    );

    let addr = free_port();
    let serving = addr.clone();
    std::thread::spawn(move || {
        let _ = fixbolt_engine::serve(
            &serving,
            table,
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
        );
    });
    addr
}

/// What the acceptor said back, as a readable string. Empty means it said
/// nothing and **closed the socket**.
///
/// A read timeout is not folded into that empty string. The acceptor holding a
/// socket open and saying nothing is a different fault from refusing it, and
/// this whole change is about two causes that share one silence — collapsing
/// them here would put the same defect in the harness that the tests exist to
/// catch.
fn reply_to(addr: &str, who: &str) -> String {
    let mut c = connect(addr);
    c.write_all(&logon_now(who)).expect("send");
    let mut buf = [0u8; 4096];
    match c.read(&mut buf) {
        Ok(0) => String::new(),
        Ok(n) => String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|"),
        Err(e) if matches!(e.kind(), ErrorKind::ConnectionReset) => String::new(),
        Err(e) => panic!(
            "{who}: the acceptor neither answered nor closed the socket within \
             the read timeout — that is a hung connection, not a refusal: {e}"
        ),
    }
}

/// **The specification, end to end.** Two counterparties named only in a file,
/// one acceptor, two sessions.
#[test]
fn two_counterparties_from_a_file_log_on_through_a_real_socket() {
    let addr = serving(
        "two",
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\n\
         [SESSION]\nTargetCompID=BANZAI\n",
        2,
    );

    for who in ["TW44", "BANZAI"] {
        let reply = reply_to(&addr, who);
        assert!(
            reply.contains("|35=A|"),
            "{who} is named in the file and must get a Logon back: {reply:?}"
        );
        assert!(
            reply.contains(&format!("|56={who}|")),
            "and it must be addressed to {who}: {reply:?}"
        );
    }
}

/// The control: an identity the file does not name gets nothing, which is what
/// says the acceptor is reading the file rather than admitting everybody.
#[test]
fn an_identity_the_file_does_not_name_gets_nothing() {
    let addr = serving(
        "unnamed",
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\n",
        1,
    );

    assert!(
        reply_to(&addr, "TW44").contains("|35=A|"),
        "the premise: the configured counterparty is served"
    );
    assert_eq!(
        reply_to(&addr, "NOBODY"),
        "",
        "an identity nobody configured is refused in silence — ADR-0026 \
         decision 3"
    );
}

/// **Hours written in the file reach the wire.** A counterparty whose window
/// closed two hours ago is refused; one that is open right now is served.
///
/// The two run against the same acceptor, so the only difference between them
/// is the schedule the file gave each.
#[test]
fn a_window_that_has_already_closed_refuses_the_logon() {
    // Seconds since midnight UTC, from the same clock the serving loop uses.
    let sod = ((now_ms() / 1_000) % 86_400) as u32;
    let closed_from = (sod + 86_400 - 4 * 3_600) % 86_400;
    let closed_to = (sod + 86_400 - 3 * 3_600) % 86_400;

    let text = format!(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\
         [SESSION]\nTargetCompID=TW44\nStartTime=00:00:00\nEndTime=23:59:59\n\
         [SESSION]\nTargetCompID=BANZAI\n\
         StartTime={:02}:{:02}:{:02}\nEndTime={:02}:{:02}:{:02}\n",
        closed_from / 3_600,
        (closed_from / 60) % 60,
        closed_from % 60,
        closed_to / 3_600,
        (closed_to / 60) % 60,
        closed_to % 60,
    );

    // BANZAI must be in the table before its silence means anything, and
    // `serving` asserts that. Here only the second block's identity is
    // checked, so a refusal below is about its hours.
    let parsed = Settings::parse(&text).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        parsed.configs()[1].serves(b"BANZAI", b"ISLD"),
        "the second [SESSION] is BANZAI's"
    );

    let addr = serving("hours", &text, 2);

    assert!(
        reply_to(&addr, "TW44").contains("|35=A|"),
        "the premise: a counterparty whose window is open right now is served, \
         so a refusal below is about the hours and not about the harness"
    );
    assert_eq!(
        reply_to(&addr, "BANZAI"),
        "",
        "its window closed two hours ago and the file is the only place that \
         was ever said"
    );
}

/// **The harness's own guard, proven able to fire.**
///
/// [`reply_to`] treats an empty answer as *"refused"* only when the socket was
/// closed. A listener that accepts and then says nothing for ever must make it
/// fail rather than read as a refusal — otherwise a hung acceptor would pass
/// every test in this file.
#[test]
#[should_panic(expected = "neither answered nor closed")]
fn a_socket_that_is_held_open_and_silent_is_not_read_as_a_refusal() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound").to_string();
    std::thread::spawn(move || {
        // Accept and hold. Dropping the stream would close it, which is the
        // observable this test exists to distinguish from.
        let mut held = Vec::new();
        while let Ok((s, _)) = listener.accept() {
            held.push(s);
        }
    });
    let _ = reply_to(&addr, "TW44");
}
