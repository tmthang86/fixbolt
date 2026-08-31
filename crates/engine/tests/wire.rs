//! The 59 acceptance definitions, **through a real socket**.
//!
//! `crates/session/tests/score.rs` runs the same files in process, handing the
//! session a slice and collecting its answers through a closure. This runs them
//! over TCP: the bytes go through the kernel, come back through a framer, and
//! nothing in the path is a test double except the clock.
//!
//! # Why the clock is injected and nothing else is
//!
//! Every `I` line in the corpus carries a fixed instant. Against the wall clock
//! that is two days of skew and every message is refused for a reason that has
//! nothing to do with the engine. `fixbolt_engine::clock::ManualClock` is the
//! one seam; the sockets, the framing, the session and the application are all
//! the real ones.
//!
//! # And why there is no thread
//!
//! `Engine::turn` is one non-blocking pass. Driving it by hand makes this test
//! as deterministic as the in-process gate — no sleeps, no timing window, no
//! flake. A background thread would have bought nothing and cost that.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::ops::Range;
use std::time::{Duration, Instant};

use fixbolt_conformance::runner::{Conn, Input, Link, SessionUnderTest, run};
use fixbolt_conformance::script::FIXED_TIME_MILLIS;
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::TcpTransport;
use fixbolt_engine::{Acceptor, Engine};
use fixbolt_session::{Application, Config};

const N: usize = 256;
const RX: usize = 4096;
const TX: usize = 8192;

/// The acceptance server's own application.
///
/// `[2026-08-31]` **this used to be written out here, and identically in
/// `crates/session/tests/score.rs`.** Two copies of a test oracle are two
/// oracles that will eventually disagree, and the one that disagrees is the one
/// nobody is looking at. It now lives once, in
/// `fixbolt_conformance::echo::Echo`; this is the five-line impl that forwards
/// to it. The score is unchanged — 59 / 59 before and after, which is what
/// makes the move a refactor rather than an edit.
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

/// The counterparty: one client socket per `Conn`, and the engine on the other
/// side of the loopback interface.
struct Wire<W: Waiting> {
    acceptor: Acceptor,
    engine: Engine<
        TcpTransport,
        fixbolt_session::Acceptor,
        InlineDispatch<EchoApp>,
        ManualClock,
        W,
        Store,
        N,
        RX,
        TX,
    >,
    /// The listener, so a blocking engine learns about a new connection when it
    /// arrives rather than when its timeout expires.
    listener: Option<Interest>,
    clients: Vec<(Conn, Option<TcpStream>)>,
}

/// The engine idles by whatever strategy the run names. A spinning engine in a
/// test suite is a test suite that pins a core for no reason — `wait::Spin` is
/// for `tools/w2w`.
use fixbolt_engine::transport::Interest;
use fixbolt_engine::wait::{Waiting, Yield};

impl<W: Waiting> Wire<W> {
    fn with(wait: W) -> Self {
        let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
        let listener = acceptor.source().map(Interest::readable);
        Self {
            acceptor,
            engine: Engine::new(
                Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
                InlineDispatch::new(EchoApp::default()),
                ManualClock::at(FIXED_TIME_MILLIS),
                wait,
                4,
            ),
            listener,
            clients: Vec::new(),
        }
    }

    fn at(&mut self, conn: Conn) -> usize {
        if let Some(i) = self.clients.iter().position(|(c, _)| *c == conn) {
            return i;
        }
        self.clients.push((conn, None));
        self.clients.len() - 1
    }

    /// Turn the engine until nothing has moved for `quiet`, or `deadline` is up.
    ///
    /// **Idle once is not settled**, because loopback delivery is not
    /// instantaneous. The previous version answered that with a run of 200
    /// turns that moved nothing, and a count of turns is a question about how
    /// fast this CPU spins rather than about whether anything has been
    /// delivered. Both bounds here are wall time instead, which is the unit the
    /// thing being waited on is actually measured in.
    ///
    /// **This is insurance, not the fix.** `[measured 2026-08-30]` the spin
    /// count was not what made this gate score 39 / 59 on Linux — Nagle was, on
    /// the client socket above. With `set_nodelay` in place the original
    /// spin-count pump also scores 59 / 59, and with it removed this pump also
    /// scores 39 / 59. The change is kept because a bound in turns is a bound
    /// on a machine, and this one is not; nothing in the corpus on this machine
    /// demonstrates that it matters, and that is said here rather than implied.
    ///
    /// `deadline` is a lifeline, not a criterion. Hitting it means the engine
    /// never went quiet, which is a real failure and is left for the comparator
    /// to report.
    fn pump(&mut self, quiet: Duration, deadline: Duration) {
        let start = Instant::now();
        let mut last_move = start;
        loop {
            let mut moved = false;
            while let Some(t) = self.acceptor.accept() {
                let _ = self.engine.add(t);
                moved = true;
            }
            moved |= self.engine.turn();
            let now = Instant::now();
            if moved {
                last_move = now;
            } else if now.duration_since(last_move) >= quiet {
                return;
            } else {
                // **The mode under test.** For `Yield` this is a scheduler
                // yield and the loop is what it always was. For a blocking
                // strategy the engine actually sleeps here, woken by the
                // client's next write or by its own timeout — which is the
                // only way this file exercises `standard` at all, because
                // every other line drives `turn` by hand.
                let extra = self.listener.as_slice();
                self.engine.idle_with(extra);
            }
            if now.duration_since(start) >= deadline {
                return;
            }
        }
    }

    /// Everything the engine has written to this client, cut into messages.
    ///
    /// Returns `true` if the engine closed the connection — `Ok(0)` on a
    /// non-blocking read is end-of-stream and nothing else, which is the
    /// distinction `transport::Io` exists to keep.
    fn drain(&mut self, i: usize, emit: &mut impl FnMut(&[u8])) -> bool {
        let mut buf = [0u8; 16384];
        let mut held: Vec<u8> = Vec::new();
        let mut closed = false;
        if let Some(sock) = self.clients[i].1.as_mut() {
            loop {
                match sock.read(&mut buf) {
                    Ok(0) => {
                        closed = true;
                        break;
                    }
                    Ok(n) => held.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
        }
        let mut at = 0;
        while let Some(end) = next_message(&held[at..]) {
            emit(&held[at..at + end]);
            at += end;
        }
        closed
    }
}

/// Where the message starting at `bytes[0]` ends, by its own `9=` and trailer.
fn next_message(bytes: &[u8]) -> Option<usize> {
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

impl<W: Waiting> SessionUnderTest for Wire<W> {
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, mut emit: F) -> Link {
        let i = self.at(conn);
        match input {
            Input::Connect => {
                let addr = self.acceptor.local_addr().expect("bound");
                let sock = TcpStream::connect(addr).expect("loopback");
                sock.set_nonblocking(true).expect("non-blocking");
                // **Nagle must be off on this side too, and it is not cosmetic.**
                //
                // `[measured 2026-08-30]` with it on, `2m_BodyLengthValueNotCorrect`
                // fails and no other file does. Its `I` lines include one whose
                // `9=` is too long and which the corpus expects to swallow exactly
                // the message after it. That message produces no reply — an
                // incomplete frame has nothing to answer — so no outbound segment
                // carries a piggybacked ACK, the peer's delayed ACK holds for tens
                // of milliseconds, and Nagle keeps every subsequent small write
                // queued behind it. Four `I` lines then arrive as one 477-byte read
                // and the framer discards all four, which is the correct answer to
                // the wrong question.
                //
                // The engine already sets this on the sockets it accepts
                // (`transport.rs`), so leaving it off here made the harness the only
                // Nagle-enabled peer in the test — a property of the test rig that
                // the corpus never intended to describe.
                sock.set_nodelay(true).expect("nodelay");
                self.clients[i].1 = Some(sock);
            }
            Input::Disconnect => {
                self.clients[i].1 = None;
            }
            Input::Bytes(b) => {
                if let Some(sock) = self.clients[i].1.as_mut() {
                    let _ = sock.write_all(b);
                }
            }
            Input::Tick(ms) => {
                // The corpus's clock is the engine's clock, and it is the only
                // thing in this test that is not the real component.
                *self.engine_clock() = ManualClock::at(ms);
            }
        }
        self.pump(STEP_QUIET, STEP_DEADLINE);
        let closed = self.drain(i, &mut emit);
        if closed {
            self.clients[i].1 = None;
            return Link::Dropped;
        }
        Link::Up
    }
}

impl<W: Waiting> Wire<W> {
    fn engine_clock(&mut self) -> &mut ManualClock {
        self.engine.clock_mut()
    }
}

/// How long the engine must be idle before a step is considered finished.
///
/// Well above a loopback round trip, which is the quantity actually being
/// waited on.
///
/// **Neither this nor [`STEP_DEADLINE`] is load-bearing, and that is how they
/// were checked.** `[measured 2026-08-30]` the gate scores **59 / 59 at both**
/// 1 ms and 20 ms — a 20× span in which only the run time moves, 0.8 s against
/// 14.5 s. The old spin-count version scored 39, 43 and 59 over its own 100×
/// span, and **that climb was Nagle being outwaited, not a bound being tuned.**
///
/// A score that is flat across its bounds is measuring the protocol. One that
/// climbs is measuring something else, and the next question is what — not a
/// third value of the bound.
const STEP_QUIET: Duration = Duration::from_millis(1);
/// Lifeline for one step. Reaching it is a failure, not a settle.
const STEP_DEADLINE: Duration = Duration::from_millis(50);

/// `[measured 2026-08-30]` **59 / 59 on Apple M5 and on Linux x86_64.** It read
/// 39 / 59 on Linux until the client socket above was given `TCP_NODELAY`;
/// `STATUS.md` item 17 and `reference/measured-costs.md` carry the diagnosis,
/// including the first one, which was wrong.
#[test]
fn the_fifty_nine_definitions_pass_through_a_real_socket() {
    let report = run(|_| Wire::with(Yield)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.passed, 59, "over TCP, not in process:\n{report}");
}

/// The same 59, with the engine **actually blocking** between steps.
///
/// ADR-0013's bad consequence, stated when the two modes were accepted: *"two
/// modes is two things to test, for ever."* This is that bill, and it is the
/// only place the corpus meets `standard` — everywhere else in this file drives
/// `turn` by hand, where the idle strategy is never reached.
///
/// # What it proves, and what it does not
///
/// It proves the **protocol** is unchanged when the engine blocks: same 59, same
/// comparator, same bytes. That is the bill ADR-0013 knew it was signing.
///
/// **It does not prove the wiring, and the first version of this comment claimed
/// it did.** That claim said a wiring failure would show up as this test taking
/// minutes instead of seconds. `[measured 2026-08-30]` it was refuted by
/// reversal, twice: with `Block` made to ignore readiness entirely, and with the
/// listener removed from the poll set, the run took **3.30 s and 3.34 s against a
/// baseline of 3.28 s**. Neither is a difference.
///
/// The reason is in the settle criterion. A step ends when the engine has moved
/// nothing for `STEP_QUIET` = 1 ms, and the blocking timeout here is the floor,
/// 5 ms. So **one block always satisfies the criterion**, whether it returned
/// after 0.1 ms because data arrived or after 5 ms because it timed out — the
/// harness cannot tell those apart, and the run time is `steps × 5 ms` either
/// way. Raising the timeout does not help; it scales both arms together.
///
/// So the wiring is proven elsewhere, on purpose: `tests/standard.rs` reads the
/// interest list directly rather than timing it, and
/// `scripts/check-standard-gives-the-core-back.sh` asserts a round-trip p50
/// against the poll timeout, which is the assertion that actually separates
/// "woken by the data" from "woken by the clock".
///
/// Behind the feature and `cfg(unix)`, because `wait::Block` does not exist
/// without them — ADR-0014 decision 2. `[measured 2026-08-30]` an unconditional
/// `use` of it here broke `cargo test -p fixbolt-engine --no-default-features`,
/// and `scripts/check-no-optional-deps.sh` caught it, which is the gate that
/// exists for exactly this.
#[cfg(all(feature = "standard", unix))]
#[test]
fn the_fifty_nine_definitions_pass_in_standard_mode_too() {
    use fixbolt_engine::block::Block;
    let report =
        run(|_| Wire::with(Block::with_timeout_ms(8, 5))).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.passed, 59,
        "blocking between steps must not change what the protocol does:\n{report}"
    );
}
