//! Put this engine and a real `libquickfix` on opposite ends of a socket, in
//! **both directions**.
//!
//! ```text
//! interop --role initiator --connect 127.0.0.1:15644   # this engine dials out
//! interop --role acceptor  --listen  127.0.0.1:15645   # this engine listens
//! ```
//!
//! `--role initiator` is phase 1 exit criterion 4 and is the whole of what this
//! binary did until 2026-09-04: it drives `Session<Initiator, 256>` into a C++
//! acceptor and scores seven steps itself.
//!
//! `--role acceptor` is the other half, and it is **the half that is the
//! product**. It runs the real acceptor — `fixbolt::serve`, the engine loop,
//! the pre-session registry — and scores nothing: the judge is
//! `tools/interop/initiator.cpp`, on the other end of the socket. Until it
//! existed, the acceptor's entire evidence was 59 `.def` files read by this
//! repository's own runner, and ADR-0042's sentence — *a second implementation
//! is the only independent opinion* — applied to the half of the engine that is
//! not the differentiator.
//!
//! [ADR-0004] decision 5 named the risk before any of the mirroring existed:
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
//! **`--role initiator`, under test: the session layer's initiator, over kernel
//! TCP.** Framing, sequence numbers, timestamps, the seven administrative
//! types, the resend machinery in both directions, and the six things an
//! operator can order.
//!
//! **`--role initiator`, not under test: the engine's polling loop.** It drives
//! `Session<Initiator, 256>` over a blocking `TcpStream` rather than through
//! `fixbolt_engine::Engine`, because criterion 4 is about the protocol and the
//! engine loop already has `crates/engine/tests/wire.rs` and `tools/w2w` over
//! the same kernel sockets. `STATUS.md` carries that limit rather than leaving
//! it to be discovered.
//!
//! **`--role acceptor`, under test: everything the other role leaves out.**
//! `fixbolt::serve` in `standard` mode — the poller, the pre-session table, the
//! settings file, the library's `Handler` — with the session layer's acceptor
//! underneath it. What is *not* under test here is `hft`: `serve_hft` spins a
//! core at 100% and a shared CI runner is the wrong place for that. That debt
//! stays named in `STATUS.md`.
//!
//! # Reading this binary's result
//!
//! **Read the lines, not the exit code.** In `--role initiator` every step
//! prints `ok` or `FAIL` with what it saw, and the last line is
//! `interop: PASS n/n` or `interop: FAIL`. `scripts/interop.sh` greps for
//! those. A binary that dies before printing anything and a binary that prints
//! seven failures both exit non-zero, and they are not the same result.
//!
//! In `--role acceptor` this process prints only `interop: listening on <addr>`
//! and then serves until it is killed. It has no verdict to give — the
//! `interop-acceptor:` lines come from the C++ initiator, and a fixbolt
//! acceptor scoring itself would be the mirrored corpus's mistake again.
//!
//! [ADR-0004]: ../../../docs/decisions/ADR-0004-bidirectional-engine.md

// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

// Non-negotiable 6: the feature gates the `mod` declaration itself, not only
// the manifest entry. Without `standard` on a unix target `fixbolt::serve` does
// not exist, so neither does the role that calls it, and this file still
// compiles.

#[cfg(all(feature = "standard", unix))]
mod desk;
// The third role, and the same gate on its `mod`: it calls
// `fixbolt_engine::connect_and_serve`, which like `fixbolt::serve` exists
// only behind `standard` on unix.
#[cfg(all(feature = "standard", unix))]
mod reconnect;

use std::io::{ErrorKind, Read, Write};
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
        _hdr: fixbolt_session::Header<'_>,
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
    highest_out: Option<u32>,
}

impl Journal for Kept {
    fn put(&mut self, seq: u32, bytes: &[u8]) -> bool {
        self.msgs.push((seq, bytes.to_vec()));
        self.highest_out = Some(self.highest_out.map_or(seq, |h| h.max(seq)));
        true
    }

    fn oldest(&self) -> Option<u32> {
        self.msgs.first().map(|(n, _)| *n)
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

    fn mark_out(&mut self, seq: u32) {
        self.highest_out = Some(self.highest_out.map_or(seq, |h| h.max(seq)));
    }

    fn highest_out(&self) -> Option<u32> {
        self.highest_out
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
    ///
    /// **It returns [`ReadOutcome`] rather than `Option<String>`, and that is
    /// the whole point of the type.** See the enum for what the single `None`
    /// used to hide.
    fn read_one(&mut self) -> ReadOutcome {
        loop {
            if let Some(end) = whole(&self.buf) {
                let msg: Vec<u8> = self.buf.drain(..end).collect();
                let text = readable(&msg);
                self.seen.push(text.clone());
                self.drive(What::Bytes(&msg));
                return ReadOutcome::Message(text);
            }
            let mut chunk = [0u8; 4096];
            match self.sock.read(&mut chunk) {
                Ok(0) => return ReadOutcome::PeerClosed.say(self.buf.len()),
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                // EINTR is not an ending. A signal arriving mid-read would
                // otherwise be reported as a broken socket, which is the same
                // class of mistake this enum exists to end.
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                // This socket is BLOCKING with an `SO_RCVTIMEO` on it, so
                // `WouldBlock` here means the timeout expired — not "nothing
                // yet". Linux reports it as `EAGAIN`, some platforms as
                // `ETIMEDOUT`; both are the same ending and both are named.
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return ReadOutcome::Timeout.say(self.buf.len());
                }
                Err(e) => return ReadOutcome::Failed(e.kind()).say(self.buf.len()),
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
    ///
    /// Running the bound out is [`ReadOutcome::NoMatch`], which is a **fifth**
    /// ending and not a socket one: the link was fine and the counterparty was
    /// talking, it just never said the thing being waited for. Collapsing it
    /// into the other four would put "your expectation was wrong" and "the
    /// counterparty died" back under one word.
    fn read_until(&mut self, limit: usize, want: impl Fn(&str) -> bool) -> ReadOutcome {
        for _ in 0..limit {
            match self.read_one() {
                ReadOutcome::Message(m) if want(&m) => return ReadOutcome::Message(m),
                ReadOutcome::Message(_) => {}
                ended => return ended,
            }
        }
        ReadOutcome::NoMatch.say(limit)
    }
}

/// How one attempt to read a whole FIX message ended.
///
/// **`tools/interop/src/main.rs` used to write `Ok(0) | Err(_) => return None`,
/// and that line gave three different failures the same word.** A scenario that
/// went red because the counterparty exited, one that went red because it
/// stopped answering, and one that went red because the socket broke all
/// printed *"the counterparty stopped answering"* — the sentence is only true
/// of the second. The shape was found on 2026-09-07 while reading another Rust
/// FIX engine's harness, which separates the same endings; this enum is that
/// idea rewritten from scratch, not its code.
///
/// Proven distinguishable by `read_outcome_tests`, over a real loopback socket.
#[derive(Debug)]
enum ReadOutcome {
    /// A whole message, framed and given to the session, as readable text.
    Message(String),
    /// The read timeout expired with no whole message in hand.
    Timeout,
    /// The peer closed cleanly — `read` returned `Ok(0)`.
    PeerClosed,
    /// The socket itself failed, and with which kind.
    Failed(ErrorKind),
    /// Not a socket ending: whole messages kept arriving and none matched.
    NoMatch,
}

impl ReadOutcome {
    /// Print the one sentence this ending — and only this ending — earns, then
    /// hand the value back.
    ///
    /// `n` is whatever number that sentence is about: bytes left unframed for
    /// the three socket endings, messages seen for [`Self::NoMatch`]. A partial
    /// message in the buffer at a close is worth saying out loud, because "the
    /// peer hung up" and "the peer hung up mid-message" are different bugs on
    /// the other end.
    fn say(self, n: usize) -> Self {
        match &self {
            Self::Message(_) => {}
            Self::Timeout => println!(
                "interop: read timed out after {READ_TIMEOUT:?}, link still up, {n} unframed bytes held"
            ),
            Self::PeerClosed => {
                println!("interop: the peer closed the connection, {n} unframed bytes held");
            }
            Self::Failed(kind) => println!("interop: the socket failed: {kind:?}"),
            Self::NoMatch => {
                println!("interop: {n} whole messages arrived and none was the one waited for");
            }
        }
        self
    }

    /// The message, if that is how it ended.
    ///
    /// Every caller that only needs *"did I get it?"* goes through here, so the
    /// four other endings have already printed themselves by the time the
    /// `Option` exists.
    fn found(self) -> Option<String> {
        match self {
            Self::Message(m) => Some(m),
            _ => None,
        }
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
    // `initiator` by default: `scripts/interop.sh` called this binary with no
    // `--role` for two weeks and that invocation still has to mean what it
    // meant. A new flag that silently changes an old command is a gate that
    // starts measuring something else.
    match arg(&args, "--role")
        .unwrap_or_else(|| "initiator".to_owned())
        .as_str()
    {
        "initiator" => initiator(&args),
        "acceptor" => acceptor(&args),
        "reconnect" => reconnect_role(&args),
        other => {
            println!("interop: FAIL unknown --role {other} (initiator | acceptor | reconnect)");
            std::process::ExitCode::FAILURE
        }
    }
}

/// **`--role acceptor`.** Serve until an operator stops it, and score nothing.
///
/// The judge is `tools/interop/initiator.cpp` at the other end of the socket.
/// This side's only output is the readiness line the script waits on, and what
/// `serve` says when it stops.
///
/// `[2026-09-05]` **it used to say "serve until killed"**, and that was not a
/// choice: `serve` returned only when `shutdown_finished()` answered, the only
/// thing that starts that is `Admin::shutdown`, and an `Admin` needed an
/// `Engine` this role never held (`STATUS.md` item 47). The stop now comes
/// through the front door, so the `Shutdown` line below is a real count of
/// sessions said goodbye to rather than a line nothing ever reached.
#[cfg(all(feature = "standard", unix))]
fn acceptor(args: &[String]) -> std::process::ExitCode {
    use fixbolt::{Limits, Settings};

    let addr = arg(args, "--listen").unwrap_or_else(|| "127.0.0.1:15645".to_owned());
    let Some(cfg) = arg(args, "--cfg") else {
        println!("interop: FAIL --role acceptor needs --cfg <settings file>");
        return std::process::ExitCode::FAILURE;
    };

    // A mistyped key, a mistyped path or a file naming no counterparty all stop
    // here with the line and what was written — ADR-0040. An acceptor that
    // starts cleanly and serves nobody is indistinguishable from a firewall.
    let table = match Settings::load(&cfg).and_then(Settings::into_table) {
        Ok(t) => t,
        Err(e) => {
            println!("interop: FAIL settings {cfg}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let limits = match Limits::new(PENDING, 10_000) {
        Ok(l) => l,
        Err(e) => {
            println!("interop: FAIL limits: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    println!(
        "interop: fixbolt acceptor on {addr}, {} counterparties",
        table.len()
    );

    // **The readiness line is printed by something that connected**, not by
    // this thread before it calls `serve`. `serve` binds inside itself, so a
    // line printed before the call would be a claim, and CLAUDE.md §10 is about
    // exactly that: a green that was inferred rather than observed. The probe
    // opens a socket, sees it accepted, and closes it — the pre-session stage
    // reaps a peer that left as `Step::Gone` and the slot goes back.
    announce_when_listening(addr.clone());

    // The handles exist before the engine does, which is the whole of item 47.
    // `stop_on_stdin` takes an `Admin` off them and the engine adopts the same
    // cell inside `serve`.
    let handles = fixbolt::Handles::new();
    stop_on_stdin(handles.admin());

    // **`--journal` is what makes `ResetOnLogon` observable at all.**
    //
    // `STATUS.md` item 53: an acceptor takes the `ResetOnLogon` branch only
    // when its session was **resumed**, and `fixbolt::serve` has no `Recovery`
    // seam, so under it the branch is unreachable — the key can be `Y` or `N`
    // in the file and the wire looks identical. Without the flag this role
    // still calls `serve`, unchanged, because eight scenarios are green
    // through that path and none of them should change route for this one.
    if let Some(path) = arg(args, "--journal") {
        return acceptor_with_recovery(&addr, table, limits, handles, &path);
    }

    match fixbolt::serve(
        &addr,
        table,
        fixbolt::app(desk::Desk::default()),
        CAPACITY,
        limits,
        fixbolt::NoLog,
        handles,
    ) {
        Ok(shutdown) => {
            println!("interop: acceptor stopped: {shutdown:?}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            println!("interop: FAIL serve on {addr}: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// The same acceptor, with a journal on disk and a `Recovery` seam.
///
/// **This exists so `ResetOnLogon` has somewhere to be observed.**
/// `STATUS.md` item 53 deferred that gate on 2026-09-05 for a reason it stated
/// plainly — the fixture had no recovery entry point, and building one was
/// judged larger than the knob it would test. On the same day, `d31db5e` built
/// exactly that for [`mod@reconnect`]'s initiator side: [`reconnect::Disk`],
/// [`reconnect::OnDisk`] and a `--no-recovery` reversal. This function is that
/// plumbing, pointed at [`fixbolt::serve_with_recovery`] instead of
/// `connect_and_serve`. Nothing new was designed for it.
///
/// The journal is opened once here so a bad path stops with a line rather than
/// from inside a trait method three seconds later — the same argument
/// [`reconnect::OnDisk::probe`] carries, and the same `Durability::Async`.
#[cfg(all(feature = "standard", unix))]
fn acceptor_with_recovery(
    addr: &str,
    table: fixbolt::Table,
    limits: fixbolt::Limits,
    handles: fixbolt::Handles,
    path: &str,
) -> std::process::ExitCode {
    use fixbolt_engine::journal::Durability;
    use reconnect::{Disk, OnDisk};

    let recovery = match OnDisk::probe(std::path::Path::new(path), Durability::Async, "interop") {
        Ok(r) => r,
        Err(e) => {
            println!("interop: FAIL journal {path}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    println!("interop: journal {path}, recovery on");

    match fixbolt::serve_with_recovery::<_, Disk, OnDisk, _>(
        addr,
        table,
        fixbolt::app(desk::Desk::default()),
        CAPACITY,
        limits,
        recovery,
        fixbolt::NoLog,
        handles,
    ) {
        Ok(shutdown) => {
            println!("interop: acceptor stopped: {shutdown:?}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            println!("interop: FAIL serve_with_recovery on {addr}: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Connections held at once, and sockets allowed to wait for a `Logon`.
///
/// Four rather than one: the readiness probe is a connection, a QuickFIX
/// initiator that reconnects opens another before the old one is reaped, and a
/// gate that fails on its own probe fails for a reason other than the thing
/// under test.
#[cfg(all(feature = "standard", unix))]
const CAPACITY: usize = 4;
#[cfg(all(feature = "standard", unix))]
const PENDING: usize = 4;

/// Stop the engine when the operator says so on stdin — a line, or EOF.
///
/// **Why stdin and not `SIGTERM`.** A signal handler here would need either
/// `libc` and an `unsafe extern "C"` — `CLAUDE.md` §2 rule 8 wants a plan and a
/// soundness argument for that — or a new dependency, and neither is what this
/// gate is about. What is under test is *"`serve` came back because
/// `Admin::shutdown` was called"*; **what pulls the trigger is not the
/// subject.** `scripts/interop.sh` holds the write end of a fifo open and writes
/// one line into it.
///
/// `Admin::shutdown` is two atomic stores, so this thread is not racing the
/// engine for anything.
///
/// # EOF is not a stop, and CI is what settled that
///
/// `[measured 2026-09-05]` **this used to treat end-of-input as the signal**,
/// with a comment arguing that a supervisor closing the pipe has stopped
/// supervising. The blocking `interop` job refuted it within the hour: a
/// background process on a runner has no terminal on stdin, so the read
/// returned immediately and the log read
///
/// ```text
/// interop: stopping on ""
/// interop: acceptor stopped: Shutdown { sessions: 0, said_goodbye: 0, acked: 0, timed_out: 0 }
/// ```
///
/// — the acceptor stopped before the counterparty had connected, and the whole
/// direction failed. **A launcher closing stdin is the ordinary case, not a
/// signal**: `nohup`, `systemd`, `docker` without `-i` and a shell's `&` all do
/// it. So only a **non-empty line** stops the engine now, and end-of-input
/// leaves it serving.
#[cfg(all(feature = "standard", unix))]
fn stop_on_stdin(admin: fixbolt::Admin) {
    std::thread::spawn(move || {
        let mut line = String::new();
        let read = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
        if !matches!(read, Ok(n) if n > 0) || line.trim().is_empty() {
            println!("interop: stdin ended without a stop line; still serving");
            return;
        }
        println!("interop: stopping on {:?}", line.trim());
        admin.shutdown(2_000);
    });
}

/// Print `interop: listening on <addr>` once a TCP connect to it succeeds.
///
/// On its own thread, because `serve` never returns.
#[cfg(all(feature = "standard", unix))]
fn announce_when_listening(addr: String) {
    std::thread::spawn(move || {
        for _ in 0..600 {
            if let Ok(probe) = TcpStream::connect(&addr) {
                drop(probe);
                println!("interop: listening on {addr}");
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        println!("interop: FAIL nothing accepted a connection on {addr}");
    });
}

/// Without `standard` on unix there is no `serve`, so there is no role either.
#[cfg(not(all(feature = "standard", unix)))]
fn acceptor(_args: &[String]) -> std::process::ExitCode {
    println!("interop: FAIL --role acceptor needs the `standard` feature on a unix target");
    std::process::ExitCode::FAILURE
}

/// **`--role reconnect`.** `STATUS.md` item 38 — see [`mod@reconnect`].
#[cfg(all(feature = "standard", unix))]
fn reconnect_role(args: &[String]) -> std::process::ExitCode {
    reconnect::run(args)
}

/// Without `standard` on unix there is no `connect_and_serve`, so there is no
/// role either. The same shape [`acceptor`] carries, for the same reason.
#[cfg(not(all(feature = "standard", unix)))]
fn reconnect_role(_args: &[String]) -> std::process::ExitCode {
    println!(
        "interop-reconnect: FAIL --role reconnect needs the `standard` feature on a unix target"
    );
    std::process::ExitCode::FAILURE
}

/// **`--role initiator`.** Phase 1 exit criterion 4, unchanged.
fn initiator(args: &[String]) -> std::process::ExitCode {
    let addr = arg(args, "--connect").unwrap_or_else(|| "127.0.0.1:15644".to_owned());
    let sender = arg(args, "--sender").unwrap_or_else(|| "FIXBOLT".to_owned());
    let target = arg(args, "--target").unwrap_or_else(|| "QFACC".to_owned());
    // `--next-expected` turns on `789=NextExpectedMsgSeqNum` on this end's
    // Logon and adds the step that judges the counterparty's.
    let next_expected = args.iter().any(|a| a == "--next-expected");

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
                .with_heart_bt_int(30)
                .with_next_expected(next_expected),
        ),
        app: Count::default(),
        journal: Kept::default(),
        seen: Vec::new(),
        buf: Vec::new(),
    };
    let mut score = Score { steps: Vec::new() };

    if run(&mut w, &mut score, &target, next_expected).is_none() {
        // A step that could not read is a failure of that step, not a crash:
        // the score below still prints, so the script sees which one.
        //
        // **This line used to name a cause it could not know.** It read *"the
        // counterparty stopped answering"*, which is true of one of the four
        // endings `ReadOutcome` now separates and false of the other three —
        // a peer that exited and a socket that broke both printed it. The
        // ending itself is printed by `ReadOutcome::say` at the moment it
        // happens, immediately above this line; this one says only that a step
        // gave up.
        println!("interop: a step ended without the message it was waiting for");
    }
    if score.finish() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

/// The scenario ADR-0004 named: logon, heartbeat, test request, resend, gap
/// fill, logout. Each step judges itself on what came **back**.
fn run(w: &mut Wire, score: &mut Score, target: &str, next_expected: bool) -> Option<()> {
    // ---- 1. Logon -----------------------------------------------------------
    //
    // `connect` records whose turn it is; `tick` is what makes an initiator
    // speak, because time enters the session layer nowhere else.
    w.drive(What::Connect);
    w.drive(What::Tick);
    let reply = w.read_until(4, |m| m.contains("|35=A|")).found()?;
    // `[2026-09-04]` **`49=` is compared against `--target`, not against a
    // hard-coded `QFACC`.** The literal was invisible for as long as this
    // binary had one counterparty; the moment `--role acceptor` gave it a
    // second, the step went red on a Logon that was correct. A check that names
    // its expectation in one place and reads it from another is the shape
    // `docs/reference/` keeps collecting.
    let from_them = format!("|49={target}|");
    score.step(
        "logon",
        w.session.is_logged_on() && reply.contains(&from_them),
        reply.clone(),
    );

    // ---- 1b. `789=` really crossed the wire -------------------------------
    //
    // **A precondition, not a feature test, and it exists because the gate it
    // guards can go green having tested nothing.** `[verified 2026-09-06]`
    // QuickFIX C++ reads settings by name on demand (`SessionFactory.cpp:228`)
    // with no validation pass, so a key it does not recognise is ignored **in
    // silence**. The two QuickFIX families spell this one differently —
    // `SendNextExpectedMsgSeqNum` in C++, `EnableNextExpectedMsgSeqNum` in
    // Java — and writing the wrong one produces a counterparty that sends no
    // `789`, a session that comes up perfectly, and every step below passing
    // over a field that was never there.
    //
    // So the scenario asserts it **saw** the field before it asserts anything
    // about the response to it.
    // `docs/reference/who-owns-the-outbound-header.md`.
    if next_expected {
        score.step(
            "next_expected",
            reply.contains("|789="),
            format!("the counterparty's Logon carries 789: {reply}"),
        );
    }

    // ---- 2. The acceptor's application messages ----------------------------
    //
    // Two `35=B` News, sent by the C++ side on `onLogon`, so step 5 has real
    // messages to ask back for rather than only a gap to fill.
    //
    // `[2026-09-04]` **This step no longer ends the run when nothing arrives.**
    // It used to carry `?`, so a counterparty that sent no News aborted every
    // step after it and the binary printed `PASS 1/1` — a green fraction over a
    // scenario that never ran. That stays: a step that cannot run must not be
    // able to hide the ones behind it.
    //
    // `[2026-09-05]` **What no longer stays is the exemption.** The trigger was
    // the fixbolt-with-fixbolt assembly check in `--role acceptor`, where
    // `fixbolt::serve` had no way for an application to originate a message, so
    // this step and step 5 were red there *by design* and said so.
    // [ADR-0048] built the door and `desk::Desk::on_logon` walks through it, so
    // **both steps are now expected green in both directions** and a red here
    // is a red. `STATUS.md` item 46.
    //
    // [ADR-0048]: ../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md
    let _ = w.read_until(6, |m| m.contains("|35=B|"));
    let _ = w.read_until(6, |m| m.contains("|35=B|"));
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
        answered = echo.found().is_some();
    }
    score.step(
        "testrequest",
        answered,
        "35=0 back with 112=INTEROP-1".to_owned(),
    );

    // ---- 5. ResendRequest, and what comes back ------------------------------
    //
    // Ask for the two News — `34=2` and `34=3`, which the acceptor sent on
    // logon. A counterparty that still holds them replays **those two**, as
    // `35=B` with `43=Y` and their original `52=` carried as `122=`.
    //
    // **The assertion is the two numbered messages, not "something with
    // `43=Y`".** `[measured 2026-09-02]` the first version of this step asked
    // only for a `43=Y` and a deliberate reversal — swapping `7=` and `16=` so
    // this end asked for `3` through `2` — **left it green**: QuickFIX answered
    // the inverted range with a `SequenceReset` gap fill, which also carries
    // `43=Y`. A legal answer to a question nobody asked passed a test named for
    // the question. See
    // `docs/reference/a-resend-answer-has-two-legal-shapes.md`.
    let mut replayed: Vec<u32> = Vec::new();
    if w.session.send_resend_request(2, 3, |b| {
        let _ = w.sock.write_all(b);
    }) {
        for _ in 0..8 {
            let Some(m) = w.read_one().found() else { break };
            if m.contains("|35=B|") && m.contains("|43=Y|") {
                if m.contains("|34=2|") {
                    replayed.push(2);
                } else if m.contains("|34=3|") {
                    replayed.push(3);
                }
            }
            if replayed.len() == 2 {
                break;
            }
        }
    }
    replayed.sort_unstable();
    score.step(
        "resend",
        replayed == [2, 3],
        format!("35=B with 43=Y replayed at 34={replayed:?}, wanted [2, 3]"),
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
    let requested = w.read_until(6, |m| m.contains("|35=2|")).found().is_some();
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
    let acked = w.read_until(6, |m| m.contains("|35=5|")).found().is_some();
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
                .found()
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

// ---------------------------------------------------------------------------

/// **Three socket endings that used to be one `None`.**
///
/// The meta-test [ADR-0004] never asked for and this repository needed anyway:
/// it proves the branches are distinguishable, which is the only thing that
/// makes the extra variants worth their code. Before them
/// `Ok(0) | Err(_) => return None` gave a scenario that failed because the
/// counterparty *died*, one that failed because it *went quiet*, and one that
/// failed because the *socket broke* exactly the same sentence to fail with.
///
/// Driven by a local `TcpListener` rather than by `libquickfix`: the subject is
/// this binary's own read path, and a C++ process on the other end would make
/// the peer-closed case depend on somebody else's shutdown sequence.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod read_outcome_tests {
    use super::*;
    use std::net::TcpListener;

    /// A whole FIX message whose `9=` and `10=` are computed rather than
    /// written by hand.
    ///
    /// `STATUS.md` item 50: two committed benchmarks timed a message the parser
    /// rejects, because the length and the checksum were literals. A fixture
    /// this test *frames* has the same failure available to it, so it is built
    /// by the same arithmetic `whole()` reads back.
    fn heartbeat() -> Vec<u8> {
        let body = b"35=0\x0149=QFACC\x0156=FIXBOLT\x0134=1\x0152=20260908-00:00:00.000\x01";
        let mut m = Vec::new();
        m.extend_from_slice(b"8=FIX.4.4\x01");
        m.extend_from_slice(format!("9={}\x01", body.len()).as_bytes());
        m.extend_from_slice(body);
        let sum: u32 = m.iter().map(|b| u32::from(*b)).sum();
        m.extend_from_slice(format!("10={:03}\x01", sum % 256).as_bytes());
        m
    }

    /// Stand up a listener, let `server` do one thing to the accepted socket,
    /// and return what this binary's read path made of it.
    fn outcome(server: impl FnOnce(TcpStream) + Send + 'static) -> ReadOutcome {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let joined = std::thread::spawn(move || {
            let (sock, _) = listener.accept().expect("accept");
            server(sock);
        });

        let sock = TcpStream::connect(addr).expect("connect");
        // Not `READ_TIMEOUT`: ten seconds is right for a gate against a real
        // counterparty on a shared VM and wrong for a unit test that is
        // *supposed* to time out.
        sock.set_read_timeout(Some(Duration::from_millis(250)))
            .expect("timeout");

        let mut w = Wire {
            sock,
            session: Session::new(Config::initiator(b"FIX.4.4", b"FIXBOLT", b"QFACC")),
            app: Count::default(),
            journal: Kept::default(),
            seen: Vec::new(),
            buf: Vec::new(),
        };
        let got = w.read_one();
        joined.join().expect("server thread");
        got
    }

    #[test]
    fn a_peer_that_closes_cleanly_is_not_a_timeout() {
        let got = outcome(drop);
        assert!(
            matches!(got, ReadOutcome::PeerClosed),
            "a clean close read as {got:?}"
        );
    }

    #[test]
    fn a_peer_that_goes_quiet_is_not_a_close() {
        let got = outcome(|sock| {
            // Hold it open, say nothing, outlast the reader's timeout.
            std::thread::sleep(Duration::from_millis(600));
            drop(sock);
        });
        assert!(
            matches!(got, ReadOutcome::Timeout),
            "a silent peer read as {got:?}"
        );
    }

    #[test]
    fn a_whole_message_is_neither() {
        let got = outcome(|mut sock| {
            sock.write_all(&heartbeat()).expect("write");
            std::thread::sleep(Duration::from_millis(600));
        });
        match got {
            ReadOutcome::Message(text) => assert!(text.contains("|35=0|"), "read back {text}"),
            other => panic!("a whole message read as {other:?}"),
        }
    }

    /// The fixture is valid before anything frames it — item 50's rule, applied
    /// to the message this file's own `whole()` is about to measure.
    #[test]
    fn the_fixture_is_a_whole_message() {
        let m = heartbeat();
        assert_eq!(
            whole(&m),
            Some(m.len()),
            "whole() disagrees with the fixture"
        );
    }
}
