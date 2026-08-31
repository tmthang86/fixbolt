//! The 59 acceptance definitions, **through the shard runtime**.
//!
//! `tests/wire.rs` runs them through one engine that this test drives by hand,
//! which is what makes that gate deterministic. This runs them through
//! `fixbolt_engine::shard`: two pinned threads, each with its own engine, each
//! spinning on its own, and connections handed across a channel. Nothing here
//! calls `turn`.
//!
//! # What is different, and why each difference is necessary
//!
//! * **The clock is shared.** `wire.rs` sets the engine's clock through
//!   `clock_mut()`; that engine is on another thread here, so the corpus's
//!   instant lives in an `AtomicU64` the shard threads read. It is still the
//!   only test double in the path.
//! * **Settling is by wall time on the client socket**, because there is no
//!   `turn` to count. That makes this the timing-sensitive sibling of a
//!   deterministic gate, so the bound is checked for flatness rather than
//!   trusted — see [`the_score_does_not_depend_on_the_settle_bound`].
//! * **One shard is the gate; two shards is a characterisation.**
//!   `[measured 2026-08-31]` across two engines the corpus scores **57**, at
//!   both settle bounds, because `Engine::turn` enforces "one session logged on
//!   at a time" by counting the other connections **in this engine**. That is a
//!   defect sharding exposes, recorded rather than routed around.
//!
//! Step 4 of [threads-and-affinity].
//!
//! [threads-and-affinity]: ../../../docs/plans/2026-08-30-threads-and-affinity.md

#![cfg(all(feature = "affinity", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use fixbolt_conformance::echo::Echo;
use fixbolt_conformance::runner::{Conn, Input, Link, SessionUnderTest, run};
use fixbolt_conformance::script::FIXED_TIME_MILLIS;
use fixbolt_engine::Engine;
use fixbolt_engine::affinity::{CoreId, ShardPlan, Topology};
use fixbolt_engine::clock::Clock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::shard::Shards;
use fixbolt_engine::transport::TcpTransport;
use fixbolt_engine::wait::Yield;
use fixbolt_session::{Application, Config};

const N: usize = 256;
const RX: usize = 4096;
const TX: usize = 8192;

/// The corpus's instant, readable from another thread.
///
/// `ManualClock` is a plain `u64` behind `&mut self`, which is exactly right for
/// a test that owns its engine and useless for one that does not.
#[derive(Debug, Clone)]
struct SharedClock(Arc<AtomicU64>);

impl Clock for SharedClock {
    fn now_ms(&mut self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
}

/// Five lines, forwarding to the one shared fixture — see
/// `fixbolt_conformance::echo::Echo` for why it is not an `Application` itself.
#[derive(Default)]
struct EchoApp(Echo);

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

type ShardEngine = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<EchoApp>,
    SharedClock,
    Yield,
    Store,
    N,
    RX,
    TX,
>;

struct ShardWire {
    listener: TcpListener,
    shards: Shards,
    clock: Arc<AtomicU64>,
    clients: Vec<(Conn, Option<TcpStream>)>,
    quiet: Duration,
    deadline: Duration,
}

impl ShardWire {
    fn with(shards: usize, quiet: Duration, deadline: Duration) -> Self {
        let topology = Topology::read().expect("reading /sys on Linux");
        let cores: Vec<CoreId> = topology.online().iter().copied().take(shards).collect();
        assert_eq!(cores.len(), shards, "this machine has too few online cores");
        // CI has no isolcpus and neither does a laptop. Said out loud rather
        // than quietly exempt.
        let plan = ShardPlan::new(cores).allow_unisolated();

        let clock = Arc::new(AtomicU64::new(FIXED_TIME_MILLIS));
        let for_shards = Arc::clone(&clock);
        let shards = Shards::start(&plan, move |_| -> ShardEngine {
            Engine::new(
                Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
                InlineDispatch::new(EchoApp::default()),
                SharedClock(Arc::clone(&for_shards)),
                // Not `Spin`: a corpus run that pins two cores at 100% for its
                // duration is a test suite nobody wants to run on a laptop, and
                // the corpus is about the protocol, not about the idle strategy.
                Yield,
                4,
            )
        })
        .expect("a plan this machine accepts");

        Self {
            // Blocking: this is the acceptor's thread, not an engine thread,
            // and every accept here follows a connect this test just made.
            listener: TcpListener::bind("127.0.0.1:0").expect("a free port"),
            shards,
            clock,
            clients: Vec::new(),
            quiet,
            deadline,
        }
    }

    fn at(&mut self, conn: Conn) -> usize {
        if let Some(i) = self.clients.iter().position(|(c, _)| *c == conn) {
            return i;
        }
        self.clients.push((conn, None));
        self.clients.len() - 1
    }

    /// Read this client until nothing has arrived for `quiet`, then cut what
    /// came back into messages.
    ///
    /// The engine is on another thread and cannot be stepped, so "settled" is a
    /// statement about the wire and has to be measured in the unit the wire is
    /// measured in. `deadline` is a lifeline: reaching it means the engine never
    /// went quiet, which the comparator will report as a missing reply.
    fn settle(&mut self, i: usize, emit: &mut impl FnMut(&[u8])) -> bool {
        let mut buf = [0u8; 16384];
        let mut held: Vec<u8> = Vec::new();
        let mut closed = false;
        let start = Instant::now();
        let mut last = start;

        loop {
            let mut got = false;
            if let Some(sock) = self.clients[i].1.as_mut() {
                loop {
                    match sock.read(&mut buf) {
                        Ok(0) => {
                            closed = true;
                            break;
                        }
                        Ok(n) => {
                            held.extend_from_slice(&buf[..n]);
                            got = true;
                        }
                        Err(_) => break,
                    }
                }
            }
            let now = Instant::now();
            if got {
                last = now;
            } else if closed || now.duration_since(last) >= self.quiet {
                break;
            }
            if now.duration_since(start) >= self.deadline {
                break;
            }
            std::thread::yield_now();
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

impl SessionUnderTest for ShardWire {
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, mut emit: F) -> Link {
        let i = self.at(conn);
        match input {
            Input::Connect => {
                let addr = self.listener.local_addr().expect("bound");
                let sock = TcpStream::connect(addr).expect("loopback");
                sock.set_nonblocking(true).expect("non-blocking");
                // Nagle off, for the reason `tests/wire.rs` measured: with it on,
                // `2m_BodyLengthValueNotCorrect` fails and nothing else does.
                sock.set_nodelay(true).expect("nodelay");
                let (accepted, _) = self.listener.accept().expect("the connect we just made");
                let transport = TcpTransport::new(accepted).expect("non-blocking");
                self.shards.hand(transport).expect("a shard took it");
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
                self.clock.store(ms, Ordering::Release);
            }
        }
        let closed = self.settle(i, &mut emit);
        if closed {
            self.clients[i].1 = None;
            return Link::Dropped;
        }
        Link::Up
    }
}

/// Well above a loopback round trip between two yielding threads.
const STEP_QUIET: Duration = Duration::from_millis(4);
/// Lifeline for one step. Reaching it is a failure, not a settle.
const STEP_DEADLINE: Duration = Duration::from_millis(200);

/// **One shard: all 59, and the bound does not move the score.**
///
/// This is the gate. It proves the shard runtime's own path end to end — the
/// channel, the shared clock, the pinned thread, the settle — with nothing in it
/// driven by hand. The two bounds are 20× apart and both read 59, which is what
/// separates *measuring the protocol* from *measuring a timeout*
/// (`tests/wire.rs` carries the incident that rule comes from: a wire gate whose
/// score walked 39 → 43 → 59 with its own bound was failing on Nagle).
#[test]
fn one_shard_passes_all_fifty_nine_at_any_settle_bound() {
    for quiet in [Duration::from_millis(1), Duration::from_millis(20)] {
        let report = run(|_| ShardWire::with(1, quiet, STEP_DEADLINE))
            .unwrap_or_else(|e| panic!("at quiet={quiet:?}: {e}"));
        assert_eq!(
            report.passed, 59,
            "one shard, quiet={quiet:?}, nothing driven by hand:\n{report}"
        );
    }
}

/// **Two shards: 57. This is a defect in the design, and the corpus found it.**
///
/// `[measured 2026-08-31]` running the same 59 files across two engines fails
/// exactly two, at **both** settle bounds, so it is not timing:
///
/// ```text
/// 1b_DuplicateIdentity.def:16  unexpected output: 35=A ...
/// AlreadyLoggedOn.def:13       FieldCount { expected: 8, actual: 10 }
/// ```
///
/// **Why, and it is not a sloppy rule.** An `Engine` carries exactly one
/// `Config`, so every connection it serves is the **same** FIX identity. That
/// is why `Engine::turn` can enforce "the identity is already logged on" by
/// counting *the other connections in this engine* that are up: with one
/// identity per engine, "any other" and "this identity" are the same question.
///
/// Sharding splits the connections for that one identity across engines that
/// cannot see each other, and the question stops being answerable where it is
/// asked. **The rule was right and sharding is what invalidated its premise.**
///
/// **What is deliberately not done here.** Giving this test an assignment policy
/// that keeps both connections on one shard would turn it green and prove
/// nothing — `CLAUDE.md` §10 names that exact move as the failure to watch for.
/// The rule is per-engine state, and sharding is what breaks it; the fix is a
/// decision about where that state lives, not a test parameter.
///
/// So this is a **characterisation test**: it pins the defect and its two files.
/// When the rule is made to span shards, this test goes red and has to be
/// rewritten to 59 — which is the point. It is not a target.
#[test]
fn two_shards_break_the_single_logon_rule_and_this_records_it() {
    let report =
        run(|_| ShardWire::with(2, STEP_QUIET, STEP_DEADLINE)).unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(
        report.passed, 57,
        "the shape of this defect changed; re-read it rather than moving the number:\n{report}"
    );

    let failed: Vec<&str> = report.failures.iter().map(|f| f.file.as_str()).collect();
    for expected in ["1b_DuplicateIdentity.def", "AlreadyLoggedOn.def"] {
        assert!(
            failed.iter().any(|f| f.contains(expected)),
            "expected {expected} to fail for the single-logon reason; failures were {failed:?}"
        );
    }
    assert!(
        failed.iter().all(|f| {
            f.contains("1b_DuplicateIdentity.def") || f.contains("AlreadyLoggedOn.def")
        }),
        "something ELSE broke under sharding, which is a different bug:\n{report}"
    );
}
