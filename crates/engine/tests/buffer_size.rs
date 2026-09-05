//! **The caller sizes the buffers, or nobody does.**
//!
//! `CLAUDE.md` §6 says *"the caller picks `N`. Aliases for the common sizes; no
//! hidden constant."* `docs/CONFIGURATION.md` tells a reader to *"instantiate
//! `Engine<..., N, RX, TX>` directly"*. Neither is true through the front door:
//! all six entry points go via aliases that pin `256, 4096, 8192`, and the
//! `fixbolt` crate does not re-export `Engine` at all. A deployment whose
//! counterparty sends messages larger than 4 KiB had no move but to fork.
//!
//! This file is the specification of the way out. It drives `serve_with`, not
//! `Engine`, because a test that builds an `Engine` by hand proves nothing the
//! entry points can do — `tests/wire.rs` already does that and has always
//! compiled.
//!
//! # Two claims, two colours of red
//!
//! [`a-reversal-that-must-not-compile`] draws the line, and this plan sits
//! exactly on it:
//!
//! * *"`RX` is the caller's choice"* is a claim about **what the type system
//!   permits**. Its only honest reversal is the compiler refusing, and one is
//!   quoted in the plan's delivery log.
//! * *"a 5 KiB message reaches the session"* is a claim about **behaviour**. It
//!   is red at an assertion, and [`a_message_larger_than_the_default_buffer`]
//!   is that assertion.
//!
//! The second test is what stops the first from being a tautology: the same
//! bytes, through the same code, differing in `RX` alone.
//!
//! [`a-reversal-that-must-not-compile`]: ../../../docs/reference/a-reversal-that-must-not-compile.md
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::time::Duration;

use fixbolt_engine::Application;
use fixbolt_engine::observe::Handles;
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_session::Config;

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

fn stamp() -> String {
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = *cache.format(now_ms());
    core::str::from_utf8(&full[..17]).expect("ascii").to_owned()
}

fn framed(inner: &str) -> Vec<u8> {
    let msg = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(msg.as_bytes())
}

fn logon() -> Vec<u8> {
    framed(&format!(
        "35=A\u{1}34=1\u{1}49=TW44\u{1}52={}\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}",
        stamp()
    ))
}

/// A `NewOrderSingle` padded to roughly `bytes` with a single long `58=Text`.
///
/// One long field rather than many, so that `N = 256` is never the binding
/// constraint — the thing under test is `RX`, and a message that failed for
/// running out of index slots would look identical from the wire.
fn big_order(bytes: usize, seq: u32) -> Vec<u8> {
    let head = format!(
        "35=D\u{1}34={seq}\u{1}49=TW44\u{1}52={s}\u{1}56=ISLD\u{1}\
         11=ORD-BIG\u{1}21=1\u{1}38=100\u{1}40=1\u{1}54=1\u{1}55=BIGCO\u{1}60={s}\u{1}",
        s = stamp()
    );
    let pad = bytes.saturating_sub(head.len() + 8);
    framed(&format!("{head}58={}\u{1}", "X".repeat(pad)))
}

fn table() -> Table {
    Table::new().serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
}

/// Everything the acceptor said, until it goes quiet or closes.
fn drain(c: &mut TcpStream) -> String {
    let mut all = Vec::new();
    let mut buf = [0u8; 16384];
    for _ in 0..4 {
        match c.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => all.extend_from_slice(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
            Err(e) if matches!(e.kind(), ErrorKind::ConnectionReset) => break,
            Err(e) => panic!("neither an answer nor a close: {e}"),
        }
        if all.len() > 4096 {
            break;
        }
    }
    String::from_utf8_lossy(&all).replace('\u{1}', "|")
}

/// A `TestRequest`, which the session answers by itself.
fn test_request(seq: u32) -> Vec<u8> {
    framed(&format!(
        "35=1\u{1}34={seq}\u{1}49=TW44\u{1}52={}\u{1}56=ISLD\u{1}112=PING\u{1}",
        stamp()
    ))
}

/// **The specification.** A 5 KiB application message is framed and consumed
/// when the caller asked for a 16 KiB receive buffer, and is not when they took
/// the 4 KiB default.
///
/// # The observable is the *next* message, and the first draft of this test got
/// that wrong
///
/// `[measured 2026-09-05]` the obvious observable — *did the echo come back?* —
/// is not a test of `RX` at all. It reads false for **every** size above about
/// 900 bytes even through a 16 KiB buffer, because
/// [`fixbolt_session::out::Outbound::app`] is a hard-coded `[u8; 1024]` and an
/// application that cannot lay out its reply simply returns `None`. Silence on
/// the wire, and the wrong cause: this test would have "proved" that a 1 KiB
/// ceiling was a 4 KiB one.
///
/// So the question asked here is *did the session stay in sequence?* A
/// `TestRequest` at `34=3` is answered with a `Heartbeat` echoing `112=` only if
/// the message at `34=2` was framed and consumed. When it is not framed it is
/// `Cut::Garbage`, which takes **everything buffered behind it**, the
/// `TestRequest` included — so the two cases differ in what comes back rather
/// than in how long it takes, and no timeout separates them.
#[test]
fn a_message_larger_than_the_default_buffer() {
    // --- 16 KiB, named by the caller: framed, consumed, still in sequence ---
    let addr = free_port();
    let serving = addr.clone();
    std::thread::spawn(move || {
        let _ = fixbolt_engine::serve_with::<256, 16384, 8192, 1024, _, _>(
            &serving,
            table(),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            Handles::new(),
        );
    });

    let mut c = connect(&addr);
    c.write_all(&logon()).expect("send logon");
    assert!(
        drain(&mut c).contains("|35=A|"),
        "the logon was not answered"
    );

    c.write_all(&big_order(5000, 2))
        .expect("send the big order");
    c.write_all(&test_request(3))
        .expect("send the test request");
    let reply = drain(&mut c);
    assert!(
        reply.contains("|112=PING|"),
        "a 5 KiB message did not reach the session through a 16 KiB buffer: the \
         TestRequest behind it went unanswered, so the big message was framed as \
         garbage and took it along; got: {}",
        &reply[..reply.len().min(200)]
    );

    // --- 4 KiB, the default: the same bytes, and the session never sees them ---
    let addr = free_port();
    let serving = addr.clone();
    std::thread::spawn(move || {
        let _ = fixbolt_engine::serve(
            &serving,
            table(),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            Handles::new(),
        );
    });

    let mut c = connect(&addr);
    c.write_all(&logon()).expect("send logon");
    assert!(
        drain(&mut c).contains("|35=A|"),
        "the logon was not answered"
    );

    c.write_all(&big_order(5000, 2))
        .expect("send the big order");
    c.write_all(&test_request(3))
        .expect("send the test request");
    let reply = drain(&mut c);
    assert!(
        !reply.contains("|112=PING|"),
        "the default buffer is 4 KiB and a 5 KiB message got through anyway — \
         then this test is not measuring RX; got: {}",
        &reply[..reply.len().min(200)]
    );
}

/// **The pre-session buffer is the same `RX`, and only a pipelined message can
/// tell.**
///
/// `[measured 2026-09-05]` this test exists because a reversal came back green.
/// `pump` used to carry `const PRE: usize = 4096;` under a comment promising it
/// *"matches the engine's RX"*, and `shard.rs` carried a second copy of both.
/// Pinning that back to `4096` while `RX` was 16 384 left
/// [`a_message_larger_than_the_default_buffer`] **passing**, because that test
/// sends its big message *after* the logon is answered — by which point the
/// pre-session stage has handed the socket over and the connection's own
/// `Framer<RX>` is doing the framing. `PRE` was never on that path.
///
/// The invariant only binds where the counterparty **pipelines**: bytes that
/// arrive behind the `Logon`, in the same read, are cut by the pre-session
/// buffer and carried across. Two writes with no read between them is the
/// closest a test can get to one TCP segment, and it is enough — the acceptor
/// does not read until it is polled.
///
/// Reverse it by writing `4096` in place of `RX` **in both places `grep` finds**
/// — `lib.rs` and `shard.rs`. One alone is a partial reversal and gives a
/// plausible red that overstates what the suite holds
/// ([a-reversal-that-must-not-compile], the near-miss).
///
/// [a-reversal-that-must-not-compile]: ../../../docs/reference/a-reversal-that-must-not-compile.md
#[test]
fn a_big_message_pipelined_behind_the_logon() {
    let addr = free_port();
    let serving = addr.clone();
    std::thread::spawn(move || {
        let _ = fixbolt_engine::serve_with::<256, 16384, 8192, 1024, _, _>(
            &serving,
            table(),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            Handles::new(),
        );
    });

    let mut c = connect(&addr);
    // One buffer, written before anything is read back: the logon, a 5 KiB
    // order behind it, and a TestRequest behind that.
    let mut pipelined = logon();
    pipelined.extend_from_slice(&big_order(5000, 2));
    pipelined.extend_from_slice(&test_request(3));
    c.write_all(&pipelined).expect("send all three at once");

    let reply = drain(&mut c);
    assert!(
        reply.contains("|35=A|"),
        "the pipelined logon was not answered; got: {}",
        &reply[..reply.len().min(200)]
    );
    assert!(
        reply.contains("|112=PING|"),
        "a 5 KiB message pipelined behind the logon did not survive the \
         pre-session hand-over — the pre-session buffer is smaller than the \
         connection's RX and cut it short; got: {}",
        &reply[..reply.len().min(200)]
    );
}

/// **The reply buffer is the fourth constant, and it binds before `RX` does.**
///
/// `[measured 2026-09-05]` `fixbolt_session::out::Outbound::app` was a
/// hard-coded `[u8; 1024]`: an acceptor could *receive* 4 KiB and could not
/// *answer* with much over 900 bytes. An application that cannot lay out its
/// reply returns `None`, which is a legal answer, so the failure was silence —
/// the same silence as a message that never framed. See
/// [a-ceiling-has-more-than-one-floor].
///
/// Here the echo **is** the right observable, because the echo is what the
/// constant under test bounds.
///
/// # Why 3 KiB and not 5
///
/// `[measured 2026-09-05]` raising `APP` to 8 KiB moved the wall from ~900
/// bytes to somewhere between 3 000 and 5 000, not to 8 KiB — because
/// `fixbolt_conformance::echo` lays its reply out in a
/// `TemplateBuilder::<128, 4096>`. **That ceiling belongs to the measuring
/// instrument, not to the engine**; the module says so itself. A third sweep
/// found it in a minute, which is the whole argument for sweeping rather than
/// picking one size and believing the answer.
///
/// [a-ceiling-has-more-than-one-floor]: ../../../docs/reference/a-ceiling-has-more-than-one-floor.md
#[test]
fn a_reply_larger_than_the_default_scratch() {
    // --- APP = 8192: the 5 KiB order comes back echoed ---
    let addr = free_port();
    let serving = addr.clone();
    std::thread::spawn(move || {
        let _ = fixbolt_engine::serve_with::<256, 16384, 16384, 8192, _, _>(
            &serving,
            table(),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            Handles::new(),
        );
    });

    let mut c = connect(&addr);
    c.write_all(&logon()).expect("send logon");
    assert!(
        drain(&mut c).contains("|35=A|"),
        "the logon was not answered"
    );

    c.write_all(&big_order(3000, 2))
        .expect("send the big order");
    let reply = drain(&mut c);
    assert!(
        reply.contains("|11=ORD-BIG|"),
        "a 3 KiB reply did not fit an 8 KiB application scratch; got: {}",
        &reply[..reply.len().min(200)]
    );

    // --- APP = 1024, the default: framed and consumed, but unanswerable ---
    let addr = free_port();
    let serving = addr.clone();
    std::thread::spawn(move || {
        let _ = fixbolt_engine::serve_with::<256, 16384, 16384, 1024, _, _>(
            &serving,
            table(),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            fixbolt_engine::msglog::NoLog,
            Handles::new(),
        );
    });

    let mut c = connect(&addr);
    c.write_all(&logon()).expect("send logon");
    assert!(
        drain(&mut c).contains("|35=A|"),
        "the logon was not answered"
    );

    c.write_all(&big_order(3000, 2))
        .expect("send the big order");
    c.write_all(&test_request(3))
        .expect("send the test request");
    let reply = drain(&mut c);
    assert!(
        !reply.contains("|11=ORD-BIG|"),
        "the default scratch is 1 KiB and a 3 KiB echo came back — then this \
         test is not measuring APP; got: {}",
        &reply[..reply.len().min(200)]
    );
    // And the discriminator: it was framed, so the session stayed in sequence.
    assert!(
        reply.contains("|112=PING|"),
        "the message was framed through a 16 KiB buffer, so the TestRequest \
         behind it must still be answered — a silent reply is the application \
         declining, not the framer failing; got: {}",
        &reply[..reply.len().min(200)]
    );
}
