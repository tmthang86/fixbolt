//! The `fix50sp2` FIXT 1.1 corpus — 60 definitions — **through a real
//! socket.**
//!
//! `crates/session/tests/score_fixt.rs` runs all three FIXT corpora (`fix50`,
//! `fix50sp1`, `fix50sp2`) in process, against a bare `Session`. This is the
//! second half `tests/wire.rs`'s own module doc describes for FIX 4.4: the
//! same 60 files, over TCP, through the real [`fixbolt_engine::Acceptor`] and
//! [`fixbolt_engine::Engine`] — kernel, framer, session, all real, only the
//! clock injected. `fix50sp2` only, not all three: the plan's row B5 asks for
//! one corpus proven over the wire, and `score_fixt.rs` is already the gate
//! for whether the state machine is right on all three — this file is about
//! whether the *wiring* — `settings.rs`'s new `DefaultApplVerID` key feeding
//! `Config::acceptor_fixt`, and the engine actually carrying `E =
//! TagValue<Fixt11Fix50Sp2Tables, N>` end to end — is right.
//!
//! # Two traps this file inherits from `tests/wire.rs`, not rediscovers
//!
//! `[measured 2026-08-30]`, recorded in full in that file's module doc and
//! `docs/reference/measured-costs.md`:
//!
//! 1. **`TCP_NODELAY` on the client socket.** Without it the FIX 4.4 gate
//!    scored 39 / 59 on Linux — Nagle, not the pump. Set on [`Wire::step`]'s
//!    `Input::Connect` arm below, exactly where `wire.rs` sets it.
//! 2. **A bounded-turns pump is not a fix, wall-time quiet is.** `Wire::pump`
//!    below is copied from `wire.rs::Wire::pump` unchanged in shape, for the
//!    same reason: a spin count is a bound on a machine, wall time is a bound
//!    on the thing actually being waited for.
#![cfg(feature = "fix50sp2")]
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

use fixbolt_codec::TagValue;
use fixbolt_conformance::runner::{Conn, Input, Link, Report, SessionUnderTest, run_scenario};
use fixbolt_conformance::script::{Corpus, FIXED_TIME_MILLIS, fixt_corpora, load_corpus};
use fixbolt_dict::Fixt11Fix50Sp2Tables;
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::{ConnId, InlineDispatch};
use fixbolt_engine::journal::Store;
use fixbolt_engine::msglog::{Direction, MessageLog, NoLog};
use fixbolt_engine::transport::TcpTransport;
use fixbolt_engine::{Acceptor, Engine};
use fixbolt_session::{Application, Config};

/// The encoding this file runs under — the same alias
/// `session/tests/score_fixt.rs` spells, because `dict` names no alias for it
/// (`Fix44TagValue` is the only published one). `CLAUDE.md` §6: the caller
/// picks `N`.
type Fixt = TagValue<Fixt11Fix50Sp2Tables, N>;

const N: usize = 256;
/// `[measured]` `score_fixt.rs` names 200 bytes the longest message in the
/// three FIXT corpora combined; this leaves an order of magnitude of room,
/// matching `wire.rs`'s own `RX`.
const RX: usize = 4096;
const TX: usize = 8192;
/// [`Engine`]'s default, spelled out because naming its last type parameter
/// (`E`) means naming every one before it — including this one.
const APP: usize = 1024;

/// The acceptance server's own application, echoing under the FIXT / FIX 5.0
/// SP2 tables — `EchoApp`'s FIXT twin in `wire.rs`, which echoes under
/// `Fix44Echo` and cannot be reused here for the same reason
/// `score_fixt.rs`'s own copy cannot: the encoding is baked into the type.
struct EchoApp(fixbolt_conformance::echo::Echo<Fixt>);

impl EchoApp {
    /// The fixture is told the corpus's `1137=` because the corpus's own
    /// `35=j` carries it, exactly as `score_fixt.rs::EchoApp::new` does.
    fn new(corpus: &Corpus) -> Self {
        Self(
            fixbolt_conformance::echo::Echo::default()
                .speaking(corpus.default_appl_ver_id.as_bytes()),
        )
    }
}

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

/// Every frame the engine read off a socket, and every frame it queued,
/// counted — `wire.rs::CountingLog`, duplicated for the reason that file's own
/// copy states (ADR-0087 Consequences: two gates must not be breakable from one
/// file).
///
/// **ADR-0087 decision 1.** A step used to settle on a quiet interval: the
/// engine had moved nothing for [`STEP_QUIET`] of wall time. Over a socket that
/// sentence has two meanings — "there is nothing to say" and "the answer is
/// late" — and under contention the harness read the second as the first. The
/// runner then does what it does for silence (`runner.rs`'s `E` arm advances
/// the clock a whole `HeartBtInt` when nothing is pending), the engine
/// correctly answers the question it was asked, and a positional comparator
/// reports every line after that extra message as wrong.
///
/// `In` is recorded by `crates/engine/src/conn.rs` **before the session judges
/// the frame**, garbage included, and `Out` after the bytes are copied into the
/// send queue. Counting them says what the engine has consumed and what it has
/// produced without asking the engine anything.
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
/// **A lifeline is never a settle and never a pass** (ADR-0087 decision 3). It
/// is counted rather than only printed so that a run which reached one is a run
/// that says so, and the number is asserted at zero by the test below.
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

/// The counterparty: one client socket per `Conn`, and the engine on the
/// other side of the loopback interface — `wire.rs::Wire`, with `E` bound to
/// [`Fixt`] instead of the engine's default (FIX 4.4 tag=value).
struct Wire<W: Waiting> {
    acceptor: Acceptor,
    engine: Counted<W>,
    /// The listener, so a blocking engine learns about a new connection when
    /// it arrives rather than when its timeout expires.
    listener: Option<Interest>,
    clients: Vec<Client>,
    /// The engine's own two counters, read from this side.
    counts: CountingLog,
    /// Frames written to the engine that **this harness's own framer** could
    /// cut. An `I` line it cannot frame — the corpus carries a handful with a
    /// deliberately wrong `9=` — is not counted, so the step settles on the
    /// quiet interval exactly as it used to. The race is narrowed there, not
    /// proven absent (ADR-0087 Consequences).
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
type Plain<W> = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    ManualClock,
    W,
    Store,
    N,
    RX,
    TX,
    NoLog,
    APP,
    Fixt,
>;

/// The same engine, counting.
type Counted<W> = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    ManualClock,
    W,
    Store,
    N,
    RX,
    TX,
    CountingLog,
    APP,
    Fixt,
>;

use fixbolt_engine::transport::Interest;
use fixbolt_engine::wait::{Waiting, Yield};

impl<W: Waiting> Wire<W> {
    fn with(wait: W, corpus: &Corpus, file: &str) -> Self {
        let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
        let listener = acceptor.source().map(Interest::readable);
        let cfg = Config::acceptor_fixt(
            b"FIXT.1.1",
            b"ISLD",
            corpus.comp_id.as_bytes(),
            corpus.default_appl_ver_id.as_bytes(),
        );
        let counts = CountingLog::default();
        let plain: Plain<W> = Engine::new(
            cfg,
            InlineDispatch::new(EchoApp::new(corpus)),
            ManualClock::at(FIXED_TIME_MILLIS),
            wait,
            4,
        );
        Self {
            acceptor,
            engine: plain.with_log(counts.clone()),
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
    /// and then until nothing has moved for `quiet` — `wire.rs::Wire::pump`,
    /// unchanged in shape.
    ///
    /// **The fact is the settle; the interval is belt and braces** (ADR-0087
    /// decision 1). `quiet` alone cannot tell "nothing to say" from "the answer
    /// is late", and this file's own module doc used to praise it for being
    /// wall time rather than a spin count. It is still wall time and still not
    /// a spin count; it is simply no longer what decides anything.
    ///
    /// Returns `true` if `deadline` was reached first. That is a lifeline, not
    /// a settle: the step's `E` line fails afterwards exactly as it does today.
    fn pump(&mut self, quiet: Duration, deadline: Duration) -> bool {
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
            } else if self.counts.inbound() >= self.sent && now.duration_since(last_move) >= quiet {
                return false;
            } else {
                let extra = self.listener.as_slice();
                self.engine.idle_with(extra);
            }
            if now.duration_since(start) >= deadline {
                return true;
            }
        }
    }

    /// Everything the engine has written to this client, cut into messages —
    /// `wire.rs::Wire::drain`, with the same persistent `held` buffer and the
    /// same count of what it emitted.
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

/// Where the message starting at `bytes[0]` ends, by its own `9=` and
/// trailer — `wire.rs::next_message`, unchanged.
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
            // This file runs the acceptor corpus, which never originates —
            // `wire.rs`'s own arm, same reason.
            Input::Originate(intent) => {
                panic!("the acceptor corpus must never be driven: {intent:?}")
            }
            Input::Connect => {
                let addr = self.acceptor.local_addr().expect("bound");
                let sock = TcpStream::connect(addr).expect("loopback");
                sock.set_nonblocking(true).expect("non-blocking");
                // **Trap 1, inherited, not rediscovered — see the module
                // doc.** Without this the FIX 4.4 gate scored 39 / 59 on
                // Linux; Nagle, not the pump below.
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
                // **ADR-0087 decision 2: the clock does not move while the
                // engine owes an answer.** A tick arriving while sent bytes are
                // unread is the exact event that produces a `Heartbeat` no
                // definition asked for, because `conn.rs` fires the session's
                // timers *before* it reads the socket — so the unrequested
                // message is queued ahead of the requested one, and every `34=`
                // after it is off by one.
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

impl<W: Waiting> Wire<W> {
    fn engine_clock(&mut self) -> &mut ManualClock {
        self.engine.clock_mut()
    }
}

/// How long the engine must be idle **after both facts hold** before a step is
/// considered finished — `wire.rs::STEP_QUIET`'s own value, checked flat at
/// 1 ms and 20 ms there, and no longer what decides a pass.
const STEP_QUIET: Duration = Duration::from_millis(1);
/// Lifeline for one step — `wire.rs::STEP_LIFELINE`'s own value.
///
/// Reaching it is reported and counted, never a settle (ADR-0087 decision 3).
const STEP_LIFELINE: Duration = Duration::from_secs(5);

/// Run every file in `corpus` over the socket, one fresh [`Wire`] per file —
/// `fixbolt_conformance::runner::run`'s own body, sourcing scenarios from
/// [`load_corpus`] instead of the hardcoded FIX 4.4 `scenarios()`. Duplicated
/// rather than shared for the same reason `load_corpus` itself duplicates
/// `load`'s body (see its doc): `runner::run` is `wire.rs`'s own gate and
/// must keep passing unmodified, and a shared helper would be one file two
/// gates could break from at once.
fn run_corpus<S: SessionUnderTest>(corpus: &Corpus, mut make: impl FnMut(&str) -> S) -> Report {
    let all = load_corpus(corpus).unwrap_or_else(|e| panic!("{e}"));
    let mut report = Report {
        scenarios: all.len(),
        ..Report::default()
    };
    for s in &all {
        let mut session = make(&s.file);
        let failures = run_scenario(s, &mut session);
        if failures.is_empty() {
            report.passed += 1;
            report.passed_files.push(s.file.clone());
        }
        report.failures.extend(failures);
    }
    report
}

/// **60 / 60, `fix50sp2`, over a real socket.**
///
/// `settings.rs`'s new `DefaultApplVerID` key is not read here directly —
/// this builds `Config` the same way `Settings::into_table` would once a file
/// carrying `BeginString=FIXT.1.1` and `DefaultApplVerID=9` parsed — so a
/// wiring break between that key and `Config::acceptor_fixt` would show up as
/// a wrong `1137=` on the wire and a failed Logon file, not as a compile
/// error.
#[test]
fn the_sixty_fix50sp2_definitions_pass_through_a_real_socket() {
    let corpus = fixt_corpora()
        .into_iter()
        .find(|c| c.dir == "fix50sp2")
        .expect("fix50sp2 is one of the three FIXT corpora");
    let report = run_corpus(&corpus, |f| Wire::with(Yield, &corpus, f));
    println!("{report}");
    let lifelines = LIFELINE_HITS.load(Ordering::Relaxed);
    println!("lifeline hit: {lifelines}");
    assert_eq!(report.passed, 60, "over TCP, not in process:\n{report}");
    assert_eq!(
        report.scenarios, 60,
        "fix50sp2 is 60 files, not {}",
        report.scenarios
    );
    // **A settle that was a clock is a result this file does not accept.**
    // ADR-0087 decision 3: the lifeline is reached only when the engine is
    // broken or the machine is unusable, and a green scored on top of one is
    // the flake this harness was rebuilt to remove.
    assert_eq!(
        lifelines, 0,
        "a step settled on the 5 s lifeline instead of on a counted record"
    );
}
