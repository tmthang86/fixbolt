//! Changing a session's sequence numbers on an engine that is running.
//!
//! **Step 1 of [three-in-the-morning], red at an assertion.**
//!
//! # The specification, stated as the counterparty sees it
//!
//! An operator is told at 3 a.m. that the two ends disagree and that the next
//! message this engine sends must carry `34=4812`. Everything else here —
//! handles, threads, command queues — is mechanism. **The only thing that
//! settles whether the operation happened is the number on the wire.** So that
//! is what these tests assert, and the assertion does not change between the
//! step that fails and the step that passes.
//!
//! # Why the administration is a closure
//!
//! Two tests, the same body, differing only in what the operator does: one does
//! nothing and must read the ordinary number, one administers and must read
//! 4812. Today **both closures are empty, because there is nothing to put in
//! them** — `Engine` has no public function that touches a running session's
//! sequence numbers, `conns` is private, and `next_out`/`next_in` are
//! read-only. So the control passes and the specification fails, which is what
//! says the harness can see the difference it is about to be asked to prove.
//!
//! [three-in-the-morning]: ../../../docs/plans/2026-09-02-sequence-numbers-at-three-in-the-morning.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::observe::{Admin, Change, Command, EventKind, Observer, Outcome};
use fixbolt_engine::transport::TcpTransport;
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Acceptor, Application, Config, Engine};

const N: usize = 256;
const RX: usize = 4096;
const TX: usize = 8192;

/// The number the counterparty says is right, and the number this engine has no
/// way to reach today.
const AGREED: u32 = 4812;

type Acc = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    ManualClock,
    Yield,
    Store,
    N,
    RX,
    TX,
>;

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

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

fn message(body: &str) -> Vec<u8> {
    let stamp = fixbolt_conformance::script::FIXED_TIME_IN;
    let cut = body.find('\u{1}').expect("a first field") + 1;
    let (head, rest) = body.split_at(cut);
    let inner = format!("{head}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}{rest}");
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    with_real_checksum(framed.as_bytes())
}

/// An engine on its own thread, and the two handles an operator would hold.
struct Running {
    addr: String,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    observer: Observer,
    admin: Admin,
}

impl Running {
    fn start() -> Self {
        let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
        let addr = acceptor.local_addr().expect("bound").to_string();
        let mut engine: Acc = Engine::new(
            cfg(),
            InlineDispatch::new(EchoApp::default()),
            ManualClock::at(FIXED_TIME_MILLIS),
            Yield,
            8,
        );
        let observer = engine.observer();
        // The second handle over the same `Arc`. Capability, not mechanism.
        let admin = engine.admin();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                while let Some(t) = acceptor.accept() {
                    engine.add(t);
                }
                if !engine.turn() {
                    std::thread::yield_now();
                }
            }
        });
        Self {
            addr,
            stop,
            handle: Some(handle),
            observer,
            admin,
        }
    }

    /// Wait until the engine reports a logged-on session, so the numbers being
    /// asserted on are a session's rather than an empty engine's.
    fn wait_logged_on(&self) -> fixbolt_engine::dispatch::ConnId {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(s) = self.observer.request() {
                if let Some(on) = s.sessions().iter().find(|x| x.logged_on()) {
                    return on.id();
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("no session logged on");
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Read until a Heartbeat comes back, and give back its `34=`.
///
/// `34=` follows `35=` in a FIX header, so the search runs forward from the
/// message type rather than backwards from the buffer's end — a backwards
/// search would find the *previous* message's number.
fn heartbeat_seq(sock: &mut TcpStream) -> u32 {
    sock.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let n = sock.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf).replace('\u{1}', "|");
        if let Some(at) = text.find("|35=0|") {
            let rest = &text[at..];
            if let Some(i) = rest.find("|34=") {
                let after = &rest[i + 4..];
                let end = after.find('|').unwrap_or(after.len());
                if let Ok(v) = after[..end].parse::<u32>() {
                    return v;
                }
            }
        }
    }
    panic!(
        "no heartbeat came back; saw: {}",
        String::from_utf8_lossy(&buf).replace('\u{1}', "|")
    );
}

/// Read past the Logon reply, so the next read is the message under test.
fn drain_logon_reply(sock: &mut TcpStream) {
    sock.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut chunk = [0u8; 4096];
    let n = sock.read(&mut chunk).unwrap_or(0);
    assert!(n > 0, "the engine answered the Logon with nothing");
}

/// Log on, let the operator do whatever they can, then ask the engine to speak
/// and report the sequence number the counterparty actually saw.
fn number_the_counterparty_sees<F: FnOnce(&Admin, fixbolt_engine::dispatch::ConnId)>(
    administer: F,
) -> u32 {
    let engine = Running::start();
    let mut sock = TcpStream::connect(&engine.addr).expect("connect");
    sock.set_nodelay(true).expect("nodelay");
    sock.write_all(&message("35=A\u{1}34=1\u{1}98=0\u{1}108=30\u{1}"))
        .expect("send");
    let id = engine.wait_logged_on();
    // Drain the Logon reply so the heartbeat below is the message being read.
    drain_logon_reply(&mut sock);

    administer(&engine.admin, id);
    // The command is applied on a turn, so give the engine one before asking it
    // to speak. Nothing here waits on the engine's behalf.
    std::thread::sleep(Duration::from_millis(50));

    sock.write_all(&message("35=1\u{1}34=2\u{1}112=are-you-there\u{1}"))
        .expect("send");
    heartbeat_seq(&mut sock)
}

/// **The control.** Nobody administers anything, so the counterparty sees the
/// ordinary next number. This is what says the assertion below is about the
/// administration and not about the harness.
#[test]
fn without_administration_the_counterparty_sees_the_ordinary_number() {
    let seen = number_the_counterparty_sees(|_admin, _id| {
        // Nothing. An operator who does not intervene changes nothing.
    });
    assert_ne!(
        seen, AGREED,
        "the control must not accidentally produce the administered number"
    );
    assert!(
        seen >= 2,
        "the Logon reply was 1, so the heartbeat is at least 2: {seen}"
    );
}

/// **The specification.** The counterparty says the next message must carry
/// `34=4812`. An operator holding a handle to the running engine makes it so,
/// from another thread, without stopping anything.
///
/// `[verified 2026-09-02]` there is no such call, so the closure is empty and
/// this fails on the number.
#[test]
fn an_operator_can_set_the_next_outbound_number_on_a_running_engine() {
    let seen = number_the_counterparty_sees(|admin, id| {
        assert!(admin.submit(Command::SetNextOut { id, n: AGREED }));
    });
    assert_eq!(
        seen, AGREED,
        "the operator set the next outbound number to {AGREED} and the \
         counterparty must see it on the very next message"
    );
}

// --- the deterministic half ----------------------------------------------
//
// Everything above drives a real socket, because that is the shape the
// operation actually takes. The tests below drive `turn()` by hand, because
// **the order within one turn is the thing being asserted** and a real socket
// cannot pin it down.

use fixbolt_engine::transport::{Io, Loopback, Transport};

type Local = Engine<
    Loopback,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    ManualClock,
    Yield,
    Store,
    N,
    RX,
    TX,
>;

fn local() -> (Local, Loopback) {
    let mut engine: Local = Engine::new(
        cfg(),
        InlineDispatch::new(EchoApp::default()),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let (peer, side) = Loopback::pair();
    engine.add(side);
    (engine, peer)
}

/// Log the session on and give back its id, leaving nothing unread.
///
/// **The id is read from a snapshot, not assumed to be 0.** A hardcoded id
/// would make every test below pass or fail on a numbering convention rather
/// than on the thing it names.
fn log_on(engine: &mut Local, peer: &mut Loopback) -> fixbolt_engine::dispatch::ConnId {
    let mut sink = [0u8; 8192];
    let watch = engine.observer();
    let _ = peer.send(&message("35=A\u{1}34=1\u{1}98=0\u{1}108=30\u{1}"));
    engine.turn();
    let _ = peer.recv(&mut sink);
    // Ask, then turn: a snapshot is built on the turn *after* the request, so
    // asking before the Logon was processed would describe an empty engine.
    let _ = watch.request();
    engine.turn();
    let snap = watch.request().expect("the engine published");
    snap.sessions()
        .iter()
        .find(|x| x.logged_on())
        .expect("a logged-on session")
        .id()
}

/// Read everything the engine has said and return it with `|` separators.
fn said(peer: &mut Loopback) -> String {
    let mut sink = [0u8; 8192];
    let n = match peer.recv(&mut sink) {
        Io::Ready(n) => n,
        _ => 0,
    };
    String::from_utf8_lossy(&sink[..n]).replace('\u{1}', "|")
}

/// **The order within a turn.** A command must be applied *before* the turn's
/// messages are numbered. Applied afterwards, the heartbeat below carries the
/// old number and the operator's change silently misses by one message —
/// which is the reversal this test exists for.
#[test]
fn a_command_lands_before_the_same_turn_numbers_anything() {
    let (mut engine, mut peer) = local();
    let id = log_on(&mut engine, &mut peer);
    let admin = engine.admin();

    // Both in flight for the **same** turn: the request is already in the
    // socket and the command is already in the queue.
    let _ = peer.send(&message("35=1\u{1}34=2\u{1}112=now\u{1}"));
    assert!(admin.submit(Command::SetNextOut { id, n: AGREED }));

    engine.turn();

    let reply = said(&mut peer);
    assert!(reply.contains("|35=0|"), "a heartbeat came back: {reply}");
    assert!(
        reply.contains(&format!("|34={AGREED}|")),
        "the command was applied before this turn numbered anything: {reply}"
    );
}

/// The outcome reaches the event stream, naming what was changed and to what.
#[test]
fn an_applied_command_reports_itself_on_the_event_stream() {
    let (mut engine, mut peer) = local();
    let id = log_on(&mut engine, &mut peer);
    let admin = engine.admin();
    let watch = engine.observer();

    assert!(admin.submit(Command::SetNextIn { id, n: 77 }));
    engine.turn();

    let mut seen = Vec::new();
    watch.events(&mut seen);
    assert!(
        seen.iter().any(|e| e.kind()
            == EventKind::Administered {
                change: Change::NextIn,
                to: 77,
                outcome: Outcome::Applied,
            }),
        "the audit trail must name the change and its outcome: {seen:?}"
    );
}

/// **A command that raced a disconnect says so.** It is the ordinary answer,
/// not an error, and it is the reason `submit` cannot report the outcome
/// itself.
#[test]
fn a_command_for_a_connection_that_is_gone_says_so() {
    let (mut engine, mut peer) = local();
    let _ = log_on(&mut engine, &mut peer);
    let admin = engine.admin();
    let watch = engine.observer();

    assert!(admin.submit(Command::SetNextOut { id: 9999, n: 5 }));
    engine.turn();

    let mut seen = Vec::new();
    watch.events(&mut seen);
    assert!(
        seen.iter().any(|e| matches!(
            e.kind(),
            EventKind::Administered {
                outcome: Outcome::NoSuchConnection,
                ..
            }
        )),
        "{seen:?}"
    );
}

/// A refusal is reported as a refusal, not as success. `n == 0` is the
/// reachable case, and an operator who fat-fingers a field must be told.
#[test]
fn a_refused_command_is_not_reported_as_applied() {
    let (mut engine, mut peer) = local();
    let id = log_on(&mut engine, &mut peer);
    let admin = engine.admin();
    let watch = engine.observer();

    assert!(admin.submit(Command::SetNextOut { id, n: 0 }));
    engine.turn();

    let mut seen = Vec::new();
    watch.events(&mut seen);
    assert!(
        seen.iter().any(|e| matches!(
            e.kind(),
            EventKind::Administered {
                outcome: Outcome::Refused,
                ..
            }
        )),
        "{seen:?}"
    );
}

/// **The honest form, end to end.** `SendSequenceReset` puts `35=4` with
/// `123=N` on the wire *and* moves the number, which is what distinguishes it
/// from the two silent commands.
#[test]
fn a_sequence_reset_command_reaches_the_counterparty() {
    let (mut engine, mut peer) = local();
    let id = log_on(&mut engine, &mut peer);
    let admin = engine.admin();

    assert!(admin.submit(Command::SendSequenceReset { id, n: AGREED }));
    engine.turn();

    let reply = said(&mut peer);
    assert!(reply.contains("|35=4|"), "a SequenceReset: {reply}");
    assert!(
        reply.contains("|123=N|"),
        "a reset, not a gap fill: {reply}"
    );
    assert!(reply.contains(&format!("|36={AGREED}|")), "{reply}");

    // And the promise in `36=` is kept on the next message out.
    let _ = peer.send(&message("35=1\u{1}34=2\u{1}112=now\u{1}"));
    engine.turn();
    let hb = said(&mut peer);
    assert!(
        hb.contains(&format!("|34={AGREED}|")),
        "the next message must carry what 36= promised: {hb}"
    );
}

/// **A full queue is never silent.** Unlike a lost event, a command that
/// vanished is an action that did not happen, so `submit` refuses at the call
/// rather than accepting and dropping.
#[test]
fn a_full_command_queue_refuses_at_the_call() {
    let (mut engine, mut peer) = local();
    let id = log_on(&mut engine, &mut peer);
    let admin = engine.admin();

    let cap = fixbolt_engine::observe::COMMAND_CAPACITY;
    for k in 0..cap {
        assert!(
            admin.submit(Command::SetNextIn { id, n: 2 }),
            "the queue holds {cap}, and {k} is inside it"
        );
    }
    assert!(
        !admin.submit(Command::SetNextIn { id, n: 2 }),
        "the {}th must be refused rather than swallowed",
        cap + 1
    );

    // And a turn empties it, so the refusal was back-pressure and not a wall.
    engine.turn();
    assert!(admin.submit(Command::SetNextIn { id, n: 2 }));
}
