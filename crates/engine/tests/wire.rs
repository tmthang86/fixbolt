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
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fixbolt_conformance::runner::{Conn, Input, Link, SessionUnderTest, run};
use fixbolt_conformance::script::FIXED_TIME_MILLIS;
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::{ConnId, InlineDispatch};
use fixbolt_engine::journal::Store;
use fixbolt_engine::msglog::{Direction, MessageLog, NoLog};
use fixbolt_engine::transport::{TcpTransport, Transport};
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
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        let (seq, stamp) = (hdr.seq, hdr.stamp);
        self.0.reply(msg, seq, stamp, out)
    }
}

/// Every frame the engine read off a socket, and every frame it queued, counted.
///
/// **ADR-0087 decision 1.** A step used to settle on a quiet interval: the
/// engine had moved nothing for [`STEP_QUIET`] of wall time. In process that is
/// exactly right — a pure session answers synchronously, so silence is silence.
/// Over a socket the same sentence has two meanings, "there is nothing to say"
/// and "the answer is late", and under contention this harness read the second
/// as the first. `conformance::runner`'s `E` arm then does what it does for
/// silence and advances the clock a whole `HeartBtInt`; the engine, told half a
/// minute has passed, correctly emits a `Heartbeat` no definition asked for;
/// and a positional comparator reports every line after it as wrong.
///
/// `In` is recorded by `crates/engine/src/conn.rs` **before the session judges
/// the frame**, garbage included, and `Out` after the bytes are copied into the
/// send queue. Counting them says what the engine has consumed and what it has
/// produced without asking the engine anything and without touching it.
///
/// `Arc<AtomicUsize>` because [`Engine::with_log`] takes the log by value: this
/// harness keeps a clone and reads the two numbers the engine is writing.
#[derive(Clone, Default)]
struct CountingLog {
    inbound: Arc<AtomicUsize>,
    outbound: Arc<AtomicUsize>,
}

impl CountingLog {
    /// Frames the engine has read off its sockets.
    fn inbound(&self) -> usize {
        self.inbound.load(Ordering::Relaxed)
    }

    /// Frames the engine has queued for sending.
    fn outbound(&self) -> usize {
        self.outbound.load(Ordering::Relaxed)
    }
}

impl MessageLog for CountingLog {
    fn record(&mut self, dir: Direction, _at_ms: u64, _shard: u16, _id: ConnId, _bytes: &[u8]) {
        match dir {
            Direction::In => {
                self.inbound.fetch_add(1, Ordering::Relaxed);
            }
            Direction::Out => {
                self.outbound.fetch_add(1, Ordering::Relaxed);
            }
            // Not a message: one record per connection, carrying its address.
            Direction::Open => {}
        }
    }
}

/// How many times a step gave up on a fact and settled on the clock instead.
///
/// **A lifeline is never a settle and never a pass** (ADR-0087 decision 3).
/// Shared by every test in this file, which run in parallel: a hit in any is a
/// hit, and each asserts it at zero.
static LIFELINE_HITS: AtomicUsize = AtomicUsize::new(0);

/// One client socket, and whatever bytes of a frame have arrived so far.
///
/// **`held` outlives the drain that filled it**, unlike the local buffer it
/// replaces. A step now drains more than once — once per turn of the settle
/// loop — and a frame split across two reads would otherwise be dropped by the
/// first drain and unparseable by the second.
struct Client {
    conn: Conn,
    sock: Option<TcpStream>,
    held: Vec<u8>,
}

/// The counterparty: one client socket per `Conn`, and the engine on the other
/// side of the loopback interface.
///
/// **Generic over the engine's transport `T` and the `wrap` that makes one
/// from an accepted socket** — the shape `pump`'s own `wrap` has in
/// `lib.rs`. The kernel arm wraps with `Some`, which is what this harness did
/// before it had the parameter; the `io_uring` arm registers the socket on a
/// ring (phase 4 row 5, ADR-0190).
struct Wire<T: Transport, W: Waiting, Wr: FnMut(TcpTransport) -> Option<T>> {
    acceptor: Acceptor,
    engine: Counted<T, W>,
    /// Turns an accepted socket into the engine's transport. `None` drops it.
    wrap: Wr,
    /// The listener, so a blocking engine learns about a new connection when it
    /// arrives rather than when its timeout expires.
    listener: Option<Interest>,
    clients: Vec<Client>,
    /// The engine's own two counters, read from this side.
    counts: CountingLog,
    /// Frames written to the engine that **this harness's own framer** could
    /// cut. An `I` line it cannot frame — `2m_BodyLengthValueNotCorrect` is the
    /// file that carries one on purpose — is not counted, so those steps settle
    /// on the quiet interval exactly as they used to. The race is narrowed
    /// there, not proven absent (ADR-0087 Consequences).
    sent: usize,
    /// Frames this harness has read back off the client sockets.
    read: usize,
    /// The definition being replayed, so a lifeline can name it.
    file: String,
}

/// The engine [`Engine::new`] builds, before a log is attached.
///
/// Spelled out because `with_log` changes the engine's type: without a name for
/// the type it starts from, `L` on `Engine::new` has nothing to infer from.
type Plain<T, W> = Engine<
    T,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    ManualClock,
    W,
    Store,
    N,
    RX,
    TX,
    NoLog,
>;

/// The same engine, counting.
type Counted<T, W> = Engine<
    T,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    ManualClock,
    W,
    Store,
    N,
    RX,
    TX,
    CountingLog,
>;

/// The engine idles by whatever strategy the run names. A spinning engine in a
/// test suite is a test suite that pins a core for no reason — `wait::Spin` is
/// for `tools/w2w`.
use fixbolt_engine::transport::Interest;
use fixbolt_engine::wait::{Waiting, Yield};

/// The kernel arm's `wrap`: the accepted socket is the transport.
type Kernel = fn(TcpTransport) -> Option<TcpTransport>;

impl<W: Waiting> Wire<TcpTransport, W, Kernel> {
    fn with(wait: W, file: &str) -> Self {
        Self::over(wait, Some, file)
    }
}

impl<T: Transport, W: Waiting, Wr: FnMut(TcpTransport) -> Option<T>> Wire<T, W, Wr> {
    fn over(wait: W, wrap: Wr, file: &str) -> Self {
        let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
        let listener = acceptor.source().map(Interest::readable);
        let counts = CountingLog::default();
        let plain: Plain<T, W> = Engine::new(
            Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
            InlineDispatch::new(EchoApp::default()),
            ManualClock::at(FIXED_TIME_MILLIS),
            wait,
            4,
        );
        Self {
            acceptor,
            engine: plain.with_log(counts.clone()),
            wrap,
            listener,
            clients: Vec::new(),
            counts,
            sent: 0,
            read: 0,
            file: file.to_owned(),
        }
    }

    fn at(&mut self, conn: Conn) -> usize {
        if let Some(i) = self.clients.iter().position(|c| c.conn == conn) {
            return i;
        }
        self.clients.push(Client {
            conn,
            sock: None,
            held: Vec::new(),
        });
        self.clients.len() - 1
    }

    /// How many whole frames this harness's own framer can cut out of `bytes`.
    fn framable(mut bytes: &[u8]) -> usize {
        let mut n = 0;
        while let Some(end) = next_message(bytes) {
            n += 1;
            bytes = &bytes[end..];
        }
        n
    }

    /// Both debts cancelled, because the link that owed them is gone.
    ///
    /// A socket that died discards whatever was queued for it, and whatever was
    /// written to it is never read. Without this, one dropped connection would
    /// leave a debt no turn can ever pay and **every later step** would wait out
    /// [`STEP_LIFELINE`].
    fn forgive(&mut self) {
        self.sent = self.counts.inbound();
        self.read = self.counts.outbound();
    }

    /// A step gave up on a fact. Say which, and count it.
    ///
    /// The `.def` line number is not reachable from here —
    /// [`SessionUnderTest::step`] is handed an input, not a step — so the file
    /// and the unmet fact are what a reader gets. Naming the line would mean
    /// changing `crates/conformance`, which ADR-0087 decision 4 keeps byte for
    /// byte.
    fn lifeline(&mut self, unmet: &str) {
        let n = LIFELINE_HITS.fetch_add(1, Ordering::Relaxed) + 1;
        println!(
            "lifeline: {} — {unmet}: engine read {} of {} frames, harness read {} of {} (hit {n})",
            self.file,
            self.counts.inbound(),
            self.sent,
            self.read,
            self.counts.outbound(),
        );
        self.forgive();
    }

    /// Turn the engine until it has read everything this harness has sent it,
    /// and then until nothing has moved for `quiet`.
    ///
    /// **`[2026-09-20]` the fact is the settle now; the interval is belt and
    /// braces** (ADR-0087 decision 1). What this comment used to say — that
    /// wall time is the unit the thing being waited on is measured in — is
    /// still true and was still not enough: a quiet interval cannot tell "the
    /// engine has nothing to say" from "the engine has not been scheduled
    /// yet", and under contention this harness read the second as the first
    /// often enough to move the score. `[measured 2026-09-20]` 11 red in 50 as
    /// ten concurrent copies, against 0 red in 40 sequential.
    /// `self.counts.inbound() >= self.sent` is the sentence that distinguishes
    /// them, and it costs nothing when the engine is keeping up.
    ///
    /// **`[measured 2026-08-30]`, unchanged and still the first trap here:**
    /// the spin count this pump replaced was not what made the gate score
    /// 39 / 59 on Linux — Nagle was, on the client socket above. With
    /// `set_nodelay` in place the original spin-count pump also scored 59 / 59,
    /// and with it removed this pump also scored 39 / 59.
    ///
    /// Returns `true` if `deadline` was reached first. That is a lifeline, not
    /// a settle: the step's `E` line fails afterwards exactly as it would have.
    fn pump(&mut self, quiet: Duration, deadline: Duration) -> bool {
        let start = Instant::now();
        let mut last_move = start;
        loop {
            let mut moved = false;
            while let Some(t) = self.acceptor.accept() {
                if let Some(t) = (self.wrap)(t) {
                    let _ = self.engine.add(t);
                }
                moved = true;
            }
            moved |= self.engine.turn();
            let now = Instant::now();
            if moved {
                last_move = now;
            } else if self.counts.inbound() >= self.sent && now.duration_since(last_move) >= quiet {
                return false;
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
                return true;
            }
        }
    }

    /// Everything the engine has written to this client, cut into messages, and
    /// counted — the second fact a step settles on.
    ///
    /// Returns `true` if the engine closed the connection — `Ok(0)` on a
    /// non-blocking read is end-of-stream and nothing else, which is the
    /// distinction `transport::Io` exists to keep.
    fn drain(&mut self, i: usize, emit: &mut impl FnMut(&[u8])) -> bool {
        let mut buf = [0u8; 16384];
        let mut closed = false;
        let client = &mut self.clients[i];
        if let Some(sock) = client.sock.as_mut() {
            loop {
                match sock.read(&mut buf) {
                    Ok(0) => {
                        closed = true;
                        break;
                    }
                    Ok(n) => client.held.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
        }
        let mut at = 0;
        let mut got = 0;
        while let Some(end) = next_message(&client.held[at..]) {
            emit(&client.held[at..at + end]);
            at += end;
            got += 1;
        }
        // A trailing part-frame stays for the next drain of this step rather
        // than being thrown away with the local buffer it used to live in.
        client.held.drain(..at);
        self.read += got;
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

impl<T: Transport, W: Waiting, Wr: FnMut(TcpTransport) -> Option<T>> SessionUnderTest
    for Wire<T, W, Wr>
{
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, mut emit: F) -> Link {
        let i = self.at(conn);
        match input {
            // **This file runs the acceptor corpus, which never originates.**
            // `Input::Originate` is fed only to a mirrored scenario, and every
            // scenario here comes from `scenarios()`, whose `mirrored` flag is
            // false. Reaching this arm would mean the runner had started
            // driving the acceptor direction, where every `E` line is an answer
            // and a harness that could speak would be able to make a broken
            // session look correct. So it fails loudly rather than quietly
            // doing nothing.
            Input::Originate(intent) => {
                panic!("the acceptor corpus must never be driven: {intent:?}")
            }
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
                self.clients[i].sock = Some(sock);
                self.clients[i].held.clear();
            }
            Input::Disconnect => {
                self.clients[i].sock = None;
                self.clients[i].held.clear();
                self.forgive();
            }
            Input::Bytes(b) => {
                if let Some(sock) = self.clients[i].sock.as_mut() {
                    let _ = sock.write_all(b);
                    self.sent += Self::framable(b);
                }
            }
            Input::Tick(ms) => {
                // The corpus's clock is the engine's clock, and it is the only
                // thing in this test that is not the real component.
                //
                // **ADR-0087 decision 2: it does not move while the engine owes
                // an answer.** A tick arriving while sent bytes are unread is
                // the exact event that produces a `Heartbeat` no definition
                // asked for, because `conn.rs` fires the session's timers
                // *before* it reads the socket — so the unrequested message is
                // queued ahead of the requested one, and every `34=` after it
                // is off by one.
                if self.pump(STEP_QUIET, STEP_LIFELINE) {
                    self.lifeline("a tick found the engine still owing an answer");
                }
                *self.engine_clock() = ManualClock::at(ms);
            }
        }
        // **Settled is two counted facts, and only then the quiet** (ADR-0087
        // decision 1): the engine has read everything sent, and this harness
        // has read everything the engine queued. The second needs a drain to
        // establish, which is why the loop exists.
        let start = Instant::now();
        let mut closed;
        loop {
            let hit = self.pump(STEP_QUIET, STEP_LIFELINE);
            if hit {
                self.lifeline("the engine had not read everything this step sent it");
            }
            closed = self.drain(i, &mut emit);
            if hit || closed || self.read >= self.counts.outbound() {
                break;
            }
            if start.elapsed() >= STEP_LIFELINE {
                self.lifeline("this harness had not read everything the engine queued");
                break;
            }
        }
        if closed {
            self.clients[i].sock = None;
            self.clients[i].held.clear();
            self.forgive();
            return Link::Dropped;
        }
        Link::Up
    }
}

impl<T: Transport, W: Waiting, Wr: FnMut(TcpTransport) -> Option<T>> Wire<T, W, Wr> {
    fn engine_clock(&mut self) -> &mut ManualClock {
        self.engine.clock_mut()
    }
}

/// How long the engine must be idle **after both facts hold** before a step is
/// considered finished.
///
/// Well above a loopback round trip, which is the quantity it was chosen for.
///
/// **Neither this nor [`STEP_LIFELINE`] decides a pass any more** (ADR-0087
/// decision 3), which is the strongest form of the property this comment used
/// to claim for them. `[measured 2026-08-30]` the gate scored **59 / 59 at
/// both** 1 ms and 20 ms — a 20× span in which only the run time moved, 0.8 s
/// against 14.5 s. The old spin-count version scored 39, 43 and 59 over its own
/// 100× span, and **that climb was Nagle being outwaited, not a bound being
/// tuned.**
///
/// A score that is flat across its bounds is measuring the protocol. One that
/// climbs is measuring something else, and the next question is what — not a
/// third value of the bound.
const STEP_QUIET: Duration = Duration::from_millis(1);
/// Lifeline for one step. Reaching it is reported and counted, never a settle.
///
/// **Five seconds, not fifty milliseconds** (ADR-0087 decision 3). It is
/// reached only when the engine is broken or the machine is unusable, and a
/// lifeline that trips on a busy CI runner would be the flake this harness was
/// rebuilt to remove. Nothing waits on it when the facts hold.
const STEP_LIFELINE: Duration = Duration::from_secs(5);

/// `[measured 2026-08-30]` **59 / 59 on Apple M5 and on Linux x86_64.** It read
/// 39 / 59 on Linux until the client socket above was given `TCP_NODELAY`;
/// `STATUS.md` item 17 and `reference/measured-costs.md` carry the diagnosis,
/// including the first one, which was wrong.
#[test]
fn the_fifty_nine_definitions_pass_through_a_real_socket() {
    let report = run(|s| Wire::with(Yield, &s.file)).unwrap_or_else(|e| panic!("{e}"));
    let lifelines = LIFELINE_HITS.load(Ordering::Relaxed);
    println!("lifeline hit: {lifelines}");
    assert_eq!(report.passed, 59, "over TCP, not in process:\n{report}");
    // **A settle that was a clock is a result this file does not accept.**
    // ADR-0087 decision 3.
    assert_eq!(
        lifelines, 0,
        "a step settled on the 5 s lifeline instead of on a counted record"
    );
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
/// The reason was in the settle criterion **of that day**. A step then ended
/// when the engine had moved nothing for `STEP_QUIET` = 1 ms of wall time, and
/// the blocking timeout here is the floor, 5 ms. So **one block always
/// satisfied that criterion**, whether it returned after 0.1 ms because data
/// arrived or after 5 ms because it timed out — the harness could not tell
/// those apart, and the run time was `steps × 5 ms` either way. Raising the
/// timeout did not help; it scaled both arms together.
///
/// `[changed 2026-09-20]` **that criterion is gone** (ADR-0087 decision 1). A
/// step now ends on two counted facts: [`CountingLog`] says the engine has
/// consumed every framable `I` line this harness sent it, and this harness has
/// drained every record the engine wrote. [`STEP_QUIET`] applies only after
/// both hold, and [`STEP_LIFELINE`] is not a settle at all.
///
/// **What proves that, rather than this comment asserting it**: the
/// `assert_eq!(lifelines, 0)` below, reading [`LIFELINE_HITS`]. A step that
/// gave up on a fact and settled on the 5 s lifeline is counted there and
/// turns this case red — so a green here is a run in which every step waited
/// on a counted record. The timing reversal above has **not** been re-run
/// against the new criterion, and nothing in this file needs it to be: the
/// wiring is proven elsewhere, as the next paragraph says.
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
    let report = run(|s| Wire::with(Block::with_timeout_ms(8, 5), &s.file))
        .unwrap_or_else(|e| panic!("{e}"));
    let lifelines = LIFELINE_HITS.load(Ordering::Relaxed);
    println!("lifeline hit: {lifelines}");
    assert_eq!(
        report.passed, 59,
        "blocking between steps must not change what the protocol does:\n{report}"
    );
    assert_eq!(
        lifelines, 0,
        "a step settled on the 5 s lifeline instead of on a counted record"
    );
}

/// The ring the two `io_uring` cases run on: 8 buffers of 4 KiB per
/// connection (ADR-0192, `tools/w2w`'s size), four connections — the corpus
/// opens at most two at once.
#[cfg(all(feature = "io-uring", target_os = "linux"))]
fn uring_config() -> fixbolt_engine::transport::uring::UringConfig {
    fixbolt_engine::transport::uring::UringConfig::new(8, 4096, 4).expect("a valid ring size")
}

/// The same 59, **`hft` over `io_uring`**: every accepted socket is registered
/// on a ring, and the idle turn is `UringSpin` — the reaper — rather than
/// `Yield`. Phase 4 row 5, ADR-0190; non-negotiable 3 for the new transport.
///
/// One ring per scenario, because the engine is one per scenario: a ring is
/// `!Send` and belongs to the engine thread that reaps it.
#[cfg(all(feature = "io-uring", target_os = "linux"))]
#[test]
fn the_fifty_nine_definitions_pass_over_io_uring_in_hft() {
    use fixbolt_engine::transport::uring::{HftArm, Uring};
    let report = run(|s| {
        let (uring, spin) = Uring::hft(uring_config(), HftArm::Enter)
            .unwrap_or_else(|e| panic!("Uring::hft refused: {e}"));
        Wire::over(spin, move |t| uring.register(t), &s.file)
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let lifelines = LIFELINE_HITS.load(Ordering::Relaxed);
    println!("lifeline hit: {lifelines}");
    assert_eq!(
        report.passed, 59,
        "hft over io_uring: {} / 59\n{report}",
        report.passed
    );
    assert_eq!(
        lifelines, 0,
        "a step settled on the 5 s lifeline instead of on a counted record"
    );
}

/// The same 59, **`standard` over `io_uring`**: the engine blocks in
/// `io_uring_enter` between steps, woken by a completion, by the listener's
/// one-shot `POLL_ADD`, or by its own 5 ms timeout — the timeout the kernel
/// arm's `standard` case uses.
#[cfg(all(feature = "io-uring", feature = "standard", target_os = "linux"))]
#[test]
fn the_fifty_nine_definitions_pass_over_io_uring_in_standard_mode() {
    use fixbolt_engine::transport::uring::Uring;
    let report = run(|s| {
        let (uring, block) = Uring::standard(uring_config())
            .unwrap_or_else(|e| panic!("Uring::standard refused: {e}"));
        Wire::over(
            block.with_timeout_ms(5),
            move |t| uring.register(t),
            &s.file,
        )
    })
    .unwrap_or_else(|e| panic!("{e}"));
    let lifelines = LIFELINE_HITS.load(Ordering::Relaxed);
    println!("lifeline hit: {lifelines}");
    assert_eq!(
        report.passed, 59,
        "standard over io_uring: {} / 59\n{report}",
        report.passed
    );
    assert_eq!(
        lifelines, 0,
        "a step settled on the 5 s lifeline instead of on a counted record"
    );
}
