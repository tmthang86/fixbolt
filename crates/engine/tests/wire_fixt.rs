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
use std::time::{Duration, Instant};

use fixbolt_codec::TagValue;
use fixbolt_conformance::runner::{Conn, Input, Link, Report, SessionUnderTest, run_scenario};
use fixbolt_conformance::script::{Corpus, FIXED_TIME_MILLIS, fixt_corpora, load_corpus};
use fixbolt_dict::Fixt11Fix50Sp2Tables;
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::msglog::NoLog;
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

/// The counterparty: one client socket per `Conn`, and the engine on the
/// other side of the loopback interface — `wire.rs::Wire`, with `E` bound to
/// [`Fixt`] instead of the engine's default (FIX 4.4 tag=value).
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
        NoLog,
        APP,
        Fixt,
    >,
    /// The listener, so a blocking engine learns about a new connection when
    /// it arrives rather than when its timeout expires.
    listener: Option<Interest>,
    clients: Vec<(Conn, Option<TcpStream>)>,
}

use fixbolt_engine::transport::Interest;
use fixbolt_engine::wait::{Waiting, Yield};

impl<W: Waiting> Wire<W> {
    fn with(wait: W, corpus: &Corpus) -> Self {
        let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
        let listener = acceptor.source().map(Interest::readable);
        let cfg = Config::acceptor_fixt(
            b"FIXT.1.1",
            b"ISLD",
            corpus.comp_id.as_bytes(),
            corpus.default_appl_ver_id.as_bytes(),
        );
        Self {
            acceptor,
            engine: Engine::new(
                cfg,
                InlineDispatch::new(EchoApp::new(corpus)),
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

    /// Turn the engine until nothing has moved for `quiet`, or `deadline` is
    /// up — `wire.rs::Wire::pump`, unchanged in shape. See that file's module
    /// doc for what was measured and what was not.
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
                let extra = self.listener.as_slice();
                self.engine.idle_with(extra);
            }
            if now.duration_since(start) >= deadline {
                return;
            }
        }
    }

    /// Everything the engine has written to this client, cut into messages —
    /// `wire.rs::Wire::drain`, unchanged.
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

/// How long the engine must be idle before a step is considered finished —
/// `wire.rs::STEP_QUIET`'s own value, checked flat at 1 ms and 20 ms there.
const STEP_QUIET: Duration = Duration::from_millis(1);
/// Lifeline for one step — `wire.rs::STEP_DEADLINE`'s own value.
const STEP_DEADLINE: Duration = Duration::from_millis(50);

/// Run every file in `corpus` over the socket, one fresh [`Wire`] per file —
/// `fixbolt_conformance::runner::run`'s own body, sourcing scenarios from
/// [`load_corpus`] instead of the hardcoded FIX 4.4 `scenarios()`. Duplicated
/// rather than shared for the same reason `load_corpus` itself duplicates
/// `load`'s body (see its doc): `runner::run` is `wire.rs`'s own gate and
/// must keep passing unmodified, and a shared helper would be one file two
/// gates could break from at once.
fn run_corpus<S: SessionUnderTest>(corpus: &Corpus, mut make: impl FnMut() -> S) -> Report {
    let all = load_corpus(corpus).unwrap_or_else(|e| panic!("{e}"));
    let mut report = Report {
        scenarios: all.len(),
        ..Report::default()
    };
    for s in &all {
        let mut session = make();
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
    let report = run_corpus(&corpus, || Wire::with(Yield, &corpus));
    println!("{report}");
    assert_eq!(report.passed, 60, "over TCP, not in process:\n{report}");
    assert_eq!(
        report.scenarios, 60,
        "fix50sp2 is 60 files, not {}",
        report.scenarios
    );
}
