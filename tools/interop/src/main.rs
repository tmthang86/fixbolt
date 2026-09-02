//! Drive this engine's **initiator** into a real `libquickfix` acceptor.
//!
//! Phase 1 exit criterion 4, and [ADR-0004] decision 5 named it before any of
//! the mirroring existed:
//!
//! > the mirrored corpus is **this project's own reading** of a suite written
//! > for the other direction, and a wrong reading stays green in it
//!
//! `[measured 2026-08-30]` that risk is not hypothetical. The mirrored gate was
//! paused because reading it more carefully changed its ceiling twice — 51, then
//! 50, then 45. This binary is the gate that cannot be argued with: the
//! counterparty is twenty years of somebody else's C++, it validates against
//! `FIX44.xml`, and it does not care what this repository believes.
//!
//! # What is under test, and what is not
//!
//! **Under test: the session layer's initiator, over kernel TCP.** Framing,
//! sequence numbers, timestamps, the seven administrative types, the resend
//! machinery in both directions, and the six things an operator can order.
//!
//! **Not under test: the engine's polling loop.** This drives
//! `Session<Initiator, 256>` over a blocking `TcpStream` rather than through
//! `fixbolt_engine::Engine`, because criterion 4 is about the protocol and the
//! engine loop already has `crates/engine/tests/wire.rs` and `tools/w2w` over
//! the same kernel sockets. `STATUS.md` carries that limit rather than leaving
//! it to be discovered.
//!
//! # Reading this binary's result
//!
//! **Read the lines, not the exit code.** Every step prints `ok` or `FAIL` with
//! what it saw, and the last line is `interop: PASS n/n` or `interop: FAIL`.
//! `scripts/interop.sh` greps for those. A binary that dies before printing
//! anything and a binary that prints seven failures both exit non-zero, and
//! they are not the same result.
//!
//! [ADR-0004]: ../../../docs/decisions/ADR-0004-bidirectional-engine.md

use std::io::{Read, Write};
use std::net::TcpStream;
use std::ops::Range;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fixbolt_session::clock::MILLIS_YEAR_ZERO_TO_EPOCH;
use fixbolt_session::journal::Journal;
use fixbolt_session::{Application, Config, Initiator, Link, Session};

/// The `112=` this end chooses. Deliberately not `TEST`, which is what the
/// session writes for a request **it** raised: if the acceptor's answer echoed
/// the wrong one, a test looking only for "a `112=` came back" would pass.
const OUR_TEST_REQ_ID: &[u8] = b"INTEROP-1";

/// How long any single read waits before the run is called a failure.
///
/// Generous: this is a correctness gate on a shared VM, not a latency
/// measurement, and a flake here would be indistinguishable from a protocol
/// bug — which is the failure mode `CLAUDE.md` §10 names.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Counts what arrives, and answers nothing.
///
/// The acceptor sends `35=B` News on logon so step 5 has real messages to ask
/// back for. Replying to them is not this gate's business.
#[derive(Default)]
struct Count {
    app: usize,
}

impl Application for Count {
    fn on_message(
        &mut self,
        _msg: &[u8],
        _seq: u32,
        _stamp: &[u8],
        _out: &mut [u8],
    ) -> Option<Range<usize>> {
        self.app += 1;
        None
    }
}

/// Keeps every outbound application message, so a `ResendRequest` from the
/// acceptor could be answered by replay rather than by gap fill.
///
/// This end sends no application messages, so in this run it always gap-fills —
/// which is step 6, and is the answer being checked.
#[derive(Default)]
struct Kept {
    msgs: Vec<(u32, Vec<u8>)>,
    highest_in: Option<u32>,
}

impl Journal for Kept {
    fn put(&mut self, seq: u32, bytes: &[u8]) {
        self.msgs.push((seq, bytes.to_vec()));
    }

    fn get(&self, seq: u32) -> Option<&[u8]> {
        self.msgs
            .iter()
            .find(|(n, _)| *n == seq)
            .map(|(_, b)| b.as_slice())
    }

    fn highest(&self) -> Option<u32> {
        self.msgs.last().map(|(n, _)| *n)
    }

    fn mark_in(&mut self, seq: u32) {
        self.highest_in = Some(seq);
    }

    fn highest_in(&self) -> Option<u32> {
        self.highest_in
    }
}

/// One session, one socket, and the bookkeeping a step needs to judge itself.
struct Wire {
    sock: TcpStream,
    session: Session<Initiator, 256>,
    app: Count,
    journal: Kept,
    /// Every message received, newest last, as readable text.
    seen: Vec<String>,
    buf: Vec<u8>,
}

impl Wire {
    fn now_ms() -> u64 {
        let unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        u64::try_from(unix).unwrap_or(0) + MILLIS_YEAR_ZERO_TO_EPOCH
    }

    /// Feed the session one input and write whatever it answers to the socket.
    fn drive(&mut self, what: What<'_>) -> Link {
        let mut out: Vec<Vec<u8>> = Vec::new();
        let link = match what {
            What::Connect => self.session.connect(|b| out.push(b.to_vec())),
            What::Tick => self.session.tick(Self::now_ms(), |b| out.push(b.to_vec())),
            What::Bytes(b) => {
                self.session
                    .received_with(b, &mut self.app, &mut self.journal, |o| {
                        out.push(o.to_vec());
                    })
            }
        };
        for m in &out {
            if self.sock.write_all(m).is_err() {
                return Link::Dropped;
            }
        }
        link
    }

    /// Read exactly one whole FIX message and give it to the session.
    ///
    /// Returns the message as read, so a step can assert on what arrived rather
    /// than on what the session did with it — the two are different claims and
    /// only the first says the counterparty agreed with us.
    fn read_one(&mut self) -> Option<String> {
        loop {
            if let Some(end) = whole(&self.buf) {
                let msg: Vec<u8> = self.buf.drain(..end).collect();
                let text = readable(&msg);
                self.seen.push(text.clone());
                self.drive(What::Bytes(&msg));
                return Some(text);
            }
            let mut chunk = [0u8; 4096];
            match self.sock.read(&mut chunk) {
                Ok(0) | Err(_) => return None,
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
            }
        }
    }

    /// Read messages until one satisfies `want`, or `limit` have gone by.
    ///
    /// A counterparty interleaves heartbeats with everything else, so a step
    /// that read exactly one message would fail on timing rather than on
    /// protocol. Bounded rather than open-ended so a wrong expectation ends the
    /// run instead of hanging it — `[measured 2026-09-02]` a reversal in this
    /// repository has already failed by hanging once.
    fn read_until(&mut self, limit: usize, want: impl Fn(&str) -> bool) -> Option<String> {
        for _ in 0..limit {
            let m = self.read_one()?;
            if want(&m) {
                return Some(m);
            }
        }
        None
    }
}

enum What<'a> {
    Connect,
    Tick,
    Bytes(&'a [u8]),
}

/// The steps, their results, and the one line a script reads.
struct Score {
    steps: Vec<(&'static str, bool, String)>,
}

impl Score {
    fn step(&mut self, name: &'static str, ok: bool, saw: String) {
        println!(
            "interop: {name:<12} {}  {saw}",
            if ok { "ok  " } else { "FAIL" }
        );
        self.steps.push((name, ok, saw));
    }

    fn finish(&self) -> bool {
        let passed = self.steps.iter().filter(|(_, ok, _)| *ok).count();
        let all = self.steps.len();
        if passed == all && all > 0 {
            println!("interop: PASS {passed}/{all}");
            true
        } else {
            println!("interop: FAIL {passed}/{all}");
            false
        }
    }
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let addr = arg(&args, "--connect").unwrap_or_else(|| "127.0.0.1:15644".to_owned());
    let sender = arg(&args, "--sender").unwrap_or_else(|| "FIXBOLT".to_owned());
    let target = arg(&args, "--target").unwrap_or_else(|| "QFACC".to_owned());

    println!("interop: fixbolt initiator -> libquickfix acceptor at {addr}");
    println!("interop: {sender} -> {target}, FIX.4.4");

    let Ok(sock) = TcpStream::connect(&addr) else {
        println!("interop: FAIL could not connect to {addr}");
        return std::process::ExitCode::FAILURE;
    };
    if sock.set_read_timeout(Some(READ_TIMEOUT)).is_err() {
        println!("interop: FAIL could not set a read timeout");
        return std::process::ExitCode::FAILURE;
    }
    let _ = sock.set_nodelay(true);

    let mut w = Wire {
        sock,
        session: Session::new(
            Config::initiator(b"FIX.4.4", sender.as_bytes(), target.as_bytes())
                .with_heart_bt_int(30),
        ),
        app: Count::default(),
        journal: Kept::default(),
        seen: Vec::new(),
        buf: Vec::new(),
    };
    let mut score = Score { steps: Vec::new() };

    if run(&mut w, &mut score).is_none() {
        // A step that could not read is a failure of that step, not a crash:
        // the score below still prints, so the script sees which one.
        println!("interop: the counterparty stopped answering");
    }
    if score.finish() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

/// The scenario ADR-0004 named: logon, heartbeat, test request, resend, gap
/// fill, logout. Each step judges itself on what came **back**.
fn run(w: &mut Wire, score: &mut Score) -> Option<()> {
    // ---- 1. Logon -----------------------------------------------------------
    //
    // `connect` records whose turn it is; `tick` is what makes an initiator
    // speak, because time enters the session layer nowhere else.
    w.drive(What::Connect);
    w.drive(What::Tick);
    let reply = w.read_until(4, |m| m.contains("|35=A|"))?;
    score.step(
        "logon",
        w.session.is_logged_on() && reply.contains("|49=QFACC|"),
        reply,
    );

    // ---- 2. The acceptor's application messages ----------------------------
    //
    // Two `35=B` News, sent by the C++ side on `onLogon`, so step 5 has real
    // messages to ask back for rather than only a gap to fill.
    w.read_until(6, |m| m.contains("|35=B|"))?;
    w.read_until(6, |m| m.contains("|35=B|"))?;
    score.step(
        "news",
        w.app.app == 2,
        format!("{} application messages delivered", w.app.app),
    );

    // ---- 3. A heartbeat nobody asked for ------------------------------------
    //
    // The weakest of the six ordered messages and the one most likely to be
    // silently wrong: a `35=0` carrying a `112=` it was never given would be
    // rejected by a strict counterparty and ignored by a lenient one.
    let sent = w.session.send_heartbeat(|b| {
        let _ = w.sock.write_all(b);
    });
    let alive = w.round_trip(b"ALIVE-AFTER-BEAT")?;
    score.step(
        "heartbeat",
        sent && alive,
        "unprompted 35=0, session still answering".to_owned(),
    );

    // ---- 4. A TestRequest with our own 112= ---------------------------------
    let mut answered = false;
    if w.session.send_test_request(OUR_TEST_REQ_ID, |b| {
        let _ = w.sock.write_all(b);
    }) {
        let echo = w.read_until(6, |m| m.contains("|35=0|") && m.contains("|112=INTEROP-1|"));
        answered = echo.is_some();
    }
    score.step(
        "testrequest",
        answered,
        "35=0 back with 112=INTEROP-1".to_owned(),
    );

    // ---- 5. ResendRequest, and what comes back ------------------------------
    //
    // Ask for the two News. A conforming counterparty replays them with
    // `43=Y`; one that has dropped them gap-fills. Either is legal, so both
    // count — what is under test is that this end asked properly and survived
    // the answer.
    let mut resent = String::new();
    if w.session.send_resend_request(2, 3, |b| {
        let _ = w.sock.write_all(b);
    }) {
        if let Some(m) = w.read_until(8, |m| m.contains("|43=Y|")) {
            resent = m;
        }
    }
    score.step(
        "resend",
        !resent.is_empty(),
        if resent.is_empty() {
            "nothing carrying 43=Y came back".to_owned()
        } else {
            "a message came back with 43=Y".to_owned()
        },
    );

    // ---- 6. A gap this end opens, and the gap fill it answers with ----------
    //
    // Move `next_out` forward without telling anybody — which is what
    // `set_next_out` is for and why its own docs call it a lie — then speak.
    // The acceptor sees a number it did not expect and asks for what it missed;
    // this end holds no application messages for that range, so it answers
    // `35=4` with `123=Y`.
    //
    // **The check is that the session survives it**, not merely that a `35=2`
    // arrived: a gap fill the counterparty refused would leave the link up for
    // one more message and then drop it, so the round trip afterwards is the
    // assertion that matters.
    let skipped = w.session.next_out() + 3;
    w.session.set_next_out(skipped);
    let asked = w.session.send_heartbeat(|b| {
        let _ = w.sock.write_all(b);
    });
    let requested = w.read_until(6, |m| m.contains("|35=2|")).is_some();
    let survived = w.round_trip(b"ALIVE-AFTER-GAP")?;
    score.step(
        "gapfill",
        asked && requested && survived,
        format!("35=2 in: {requested}, session survived: {survived}"),
    );

    // ---- 7. Logout ----------------------------------------------------------
    let said = w.session.begin_logout(b"interop done", |b| {
        let _ = w.sock.write_all(b);
    }) == Link::Up;
    let acked = w.read_until(6, |m| m.contains("|35=5|")).is_some();
    score.step("logout", said && acked, "35=5 out, 35=5 back".to_owned());

    Some(())
}

impl Wire {
    /// A `TestRequest` out and its `Heartbeat` back, used as *"is this session
    /// still working?"*.
    ///
    /// A step that only checked its own message went out would pass against a
    /// counterparty that had already stopped listening.
    fn round_trip(&mut self, id: &[u8]) -> Option<bool> {
        if !self.session.send_test_request(id, |b| {
            let _ = self.sock.write_all(b);
        }) {
            return Some(false);
        }
        let want = format!("|112={}|", String::from_utf8_lossy(id));
        Some(
            self.read_until(8, |m| m.contains("|35=0|") && m.contains(&want))
                .is_some(),
        )
    }
}

fn arg(args: &[String], name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1).cloned()
}

fn readable(b: &[u8]) -> String {
    let mut s = String::from_utf8_lossy(b).replace('\u{1}', "|");
    s.insert(0, '|');
    s
}

/// The length of one whole FIX message at the front of `bytes`, by its own `9=`
/// and its trailer. `None` while more bytes are needed.
fn whole(bytes: &[u8]) -> Option<usize> {
    let at = bytes.windows(3).position(|w| w == b"\x019=")?;
    let digits = &bytes[at + 3..];
    let end = digits.iter().position(|b| *b == 1)?;
    let len: usize = core::str::from_utf8(&digits[..end]).ok()?.parse().ok()?;
    let stop = at + 3 + end + 1 + len;
    if bytes.len() < stop + 4 || bytes.get(stop..stop + 3) != Some(b"10=") {
        return None;
    }
    let k = bytes[stop + 3..].iter().position(|b| *b == 1)?;
    Some(stop + 3 + k + 1)
}
