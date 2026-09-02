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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fixbolt_conformance::echo::Echo;
use fixbolt_conformance::runner::{Conn, Input, Link, SessionUnderTest, run};
use fixbolt_conformance::script::FIXED_TIME_MILLIS;
use fixbolt_engine::Engine;
use fixbolt_engine::affinity::{CoreId, ShardPlan, Topology};
use fixbolt_engine::clock::Clock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::presession::{Limits, One, PendingSet, Progress};
use fixbolt_engine::shard::Shards;
use fixbolt_engine::transport::TcpTransport;
use fixbolt_engine::wait::Yield;
use fixbolt_session::{Application, Config};

const PRE: usize = 4096;

/// The counterparty the acceptance corpus logs on as: `49=TW44` in, `56=ISLD`
/// in, so this end is `ISLD` and the counterparty is `TW44`.
///
/// Before [ADR-0026] the pre-session stage let every identity through and the
/// session refused the wrong ones. Now the stage asks a [`Registry`] first, so
/// these tests have to say who this acceptor serves — and it is the same
/// counterparty the corpus was always logging on as.
///
/// [ADR-0026]: ../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md
/// [`Registry`]: fixbolt_engine::presession::Registry
fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// How the pre-session stage disposed of every socket, across a whole run:
/// `[settled, timed_out, not_logon, gone, unrouted, unknown]`.
///
/// **Not decoration, and this is the reason.** `1b_DuplicateIdentity.def` and
/// `AlreadyLoggedOn.def` both expect *no response at all* on the second
/// connection — and a socket this stage quietly threw away produces exactly
/// that. Without these counts, **59/59 could mean "the session refused the
/// duplicate" or "the stage dropped it before the session ever saw it"**, and
/// the two are indistinguishable from the wire. `CLAUDE.md` §10: a check that
/// passed for a reason other than the thing under test.
///
/// Static because `run` builds a fresh `ShardWire` per scenario and there is
/// nowhere else that outlives them; [`alone`] serialises the two tests that
/// read it.
static DISPOSAL: [AtomicUsize; 6] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];

fn disposal_reset() {
    for c in &DISPOSAL {
        c.store(0, Ordering::Relaxed);
    }
}

fn disposal_read() -> [usize; 6] {
    core::array::from_fn(|i| DISPOSAL[i].load(Ordering::Relaxed))
}
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
    shards: Shards<PRE>,
    /// The pre-session stage, on this thread — which is the acceptor thread.
    ///
    /// The limits are deliberately unreachable: `u64::MAX` milliseconds to log
    /// on, and room for far more connections than the corpus opens. **Neither
    /// is under test here** and a deadline that could fire would make this gate
    /// depend on how fast the machine is. `tests/pending.rs` is where the
    /// limits are exercised, with a clock the test moves by hand.
    pending: PendingSet<TcpTransport, One, PRE>,

    clock: Arc<AtomicU64>,
    clients: Vec<(Conn, Option<TcpStream>)>,
    quiet: Duration,
    deadline: Duration,
}

impl ShardWire {
    /// `None` when this machine has fewer physical cores than `shards`.
    ///
    /// `[measured 2026-08-31]` a GitHub runner has two vCPUs that are two
    /// threads of one physical core, so it cannot host two shards — and
    /// `ShardPlan::validate` refuses that plan, correctly. The caller asserts
    /// the refusal rather than skipping; see `tests/shard.rs`.
    fn plan_for(shards: usize) -> Option<ShardPlan> {
        let topology = Topology::read().expect("reading /sys on Linux");
        let mut cores: Vec<CoreId> = Vec::new();
        for candidate in topology.online() {
            if cores
                .iter()
                .any(|taken| topology.siblings_of(*taken).contains(candidate))
            {
                continue;
            }
            cores.push(*candidate);
            if cores.len() == shards {
                break;
            }
        }
        // CI has no isolcpus and neither does a laptop. Said out loud rather
        // than quietly exempt.
        (cores.len() == shards).then(|| ShardPlan::new(cores).allow_unisolated())
    }

    fn with(plan: &ShardPlan, quiet: Duration, deadline: Duration) -> Self {
        let clock = Arc::new(AtomicU64::new(FIXED_TIME_MILLIS));
        let for_shards = Arc::clone(&clock);
        let shards = Shards::<PRE>::start(plan, move |_| -> ShardEngine {
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
            pending: PendingSet::new(
                Limits::new(64, u64::MAX).expect("both above zero"),
                One::new(cfg()),
            ),

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
    /// Advance the pre-session stage and route whatever has identified itself.
    ///
    /// On the acceptor thread, which is this one. A `Logon` that names nobody,
    /// or a route naming a shard that does not exist, drops the connection —
    /// there is no session yet with which to say anything about it.
    fn pump(&mut self) {
        // Destructured field by field and **without `..`**, so a new way for
        // this stage to dispose of a socket breaks the build here rather than
        // disappearing.
        //
        // `[measured 2026-09-01]` that is not a precaution, it is a repair.
        // ADR-0026's registry added `Progress::unknown`, this function read
        // four fields and not the fifth, and **two connections vanished with
        // every assertion below still green** — CI run 33509748294. The comment
        // at the bottom of this file says a third disappearance would be *"a
        // new defect wearing the same green"*, and it was right about the shape
        // and blind to the instance. A counter that has to be remembered is not
        // a counter.
        let Progress {
            settled,
            timed_out,
            not_logon,
            unknown,
            gone,
        } = self.pending.turn(0);
        DISPOSAL[0].fetch_add(settled, Ordering::Relaxed);
        DISPOSAL[1].fetch_add(timed_out, Ordering::Relaxed);
        DISPOSAL[2].fetch_add(not_logon, Ordering::Relaxed);
        DISPOSAL[3].fetch_add(gone, Ordering::Relaxed);
        DISPOSAL[5].fetch_add(unknown, Ordering::Relaxed);
        while let Some(k) = self.pending.settled() {
            let Some(p) = self.pending.take(k) else { break };
            if self.shards.hand(p).is_err() {
                DISPOSAL[4].fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn settle(&mut self, i: usize, emit: &mut impl FnMut(&[u8])) -> bool {
        let mut buf = [0u8; 16384];
        let mut held: Vec<u8> = Vec::new();
        let mut closed = false;
        let start = Instant::now();
        let mut last = start;

        loop {
            self.pump();
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
                // NOT handed to a shard yet. Nothing here knows whose socket
                // this is until its `Logon` arrives, which is the whole point of
                // the pre-session stage — and what `Assign` could never do.
                assert!(
                    self.pending.admit(transport, 0).is_ok(),
                    "the pending table is sized well above what the corpus opens"
                );
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
            // This file runs the **acceptor** corpus through the shard runtime,
            // and `Input::Originate` is fed only to a mirrored scenario. In the
            // acceptor direction every `E` line is an answer, and a harness that
            // could speak would be able to make a broken session look correct —
            // so this fails loudly rather than quietly doing nothing.
            //
            // `[measured 2026-09-02]` **this arm was added because CI found it
            // and `cargo test --all` could not**: `shard_wire` is behind
            // `--features affinity`, which is off by default and Linux-only, so
            // nothing local compiled it. Same shape as the `affinity` step's own
            // comment in `.github/workflows/ci.yml`, from the other side.
            Input::Originate(intent) => {
                panic!("the acceptor corpus must not be driven: {intent:?}")
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

/// Well above the scheduling floor described on
/// [`one_shard_passes_all_fifty_nine_at_any_settle_bound`], including on a CI
/// runner with one physical core.
const STEP_QUIET: Duration = Duration::from_millis(10);

/// The two tests in this file must not run at the same time.
///
/// Cargo runs the tests inside one binary in parallel, and each of these starts
/// **real engine threads**. `[measured 2026-08-31]` with them concurrent, the
/// one-shard run scored **58 / 59** once, losing part of a reply at
/// `quiet = 1 ms`; run alone it scored 59 eighteen times out of eighteen, at
/// 1, 2 and 4 ms.
///
/// **The bound was not the fault and was not the fix.** Raising it until the
/// red went away would have been tuning a number rather than removing the
/// interference — `tests/wire.rs` carries the incident that lesson comes from,
/// where a gate whose score walked 39 → 43 → 59 with its own timeout was
/// failing on Nagle the whole time. The bound is left where it was.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take the lock, ignoring poisoning: a panic in the other test is that test's
/// failure to report, not a reason to turn this one into a different one.
fn alone() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}
/// Lifeline for one step. Reaching it is a failure, not a settle.
const STEP_DEADLINE: Duration = Duration::from_millis(500);

/// **One shard: all 59.** This is the gate.
///
/// It proves the shard runtime's own path end to end — the channel, the shared
/// clock, the pinned thread, the settle — with nothing in it driven by hand.
///
/// # This test has a floor, and it is the machine's rather than the protocol's
///
/// `tests/wire.rs` drives `turn` by hand and is therefore deterministic. This
/// one cannot: the engine is on another thread, so "settled" is a statement
/// about the wire and has to be waited for in wall time. A step ends when
/// nothing has arrived for `quiet`, and a gap **inside** a reply sequence longer
/// than `quiet` ends the step early and loses the rest of it.
///
/// So `quiet` has a floor set by how fast this machine can schedule the peer
/// thread, and `[measured 2026-08-31]` that floor is real:
///
/// * reference desktop, 8 physical cores — **59 at 1, 2, 4, 8 and 20 ms**,
///   eighteen runs across the first three;
/// * GitHub runner, **two vCPUs that are two threads of one physical core** —
///   **58 at 1 ms**, losing part of one file's replies.
///
/// `[2026-08-31]` **Two corrections, kept because the second one is the finding.**
/// This test briefly required two physical cores and skipped where there were
/// none, because a CI job *appeared* to be hung. It was not. Nor, as the first
/// correction claimed, was GitHub's API stale. The run timestamps settle it:
///
/// ```text
/// 33393632071  created 12:48:47Z  cancelled 12:50:59Z   ran 2m12s
/// 33393962624  created 12:52:43Z  completed 12:53:55Z   ran 1m12s, success
/// ```
///
/// **I cancelled a healthy run after two minutes believing it had been going for
/// thirty-five**, because I never read a clock — elapsed time was inferred from
/// how long my own sequence of waits *felt*, and those waits overlapped. Every
/// conclusion downstream inherited that. The requirement and the CI timeout are
/// both gone.
///
/// What survives is the 1 ms figure above, which came from a real failed run's
/// **log** — an assertion and a score — rather than from anything I inferred.
///
/// The two bounds below are set well above that floor and 5× apart. **This is
/// not the move `tests/wire.rs` warns against.** That warning is about raising a
/// bound until a *protocol* failure disappears — `[measured 2026-08-30]` a wire
/// gate whose score walked 39 → 43 → 59 with its own timeout was failing on
/// Nagle the whole time. The protocol is gated deterministically next door, 59
/// in two modes, with no bound at all. What is avoided here is a test that
/// reports on a CI runner's scheduler and calls it FIX.
#[test]
fn one_shard_passes_all_fifty_nine_at_any_settle_bound() {
    let _alone = alone();
    let plan = ShardWire::plan_for(1).expect("every machine has one physical core");
    for quiet in [Duration::from_millis(10), Duration::from_millis(50)] {
        let report = run(|_| ShardWire::with(&plan, quiet, STEP_DEADLINE))
            .unwrap_or_else(|e| panic!("at quiet={quiet:?}: {e}"));
        assert_eq!(
            report.passed, 59,
            "one shard, quiet={quiet:?}, nothing driven by hand:\n{report}"
        );
    }
}

/// **Two shards: 59. The defect this test used to record is fixed.**
///
/// `[measured 2026-08-31]` this file asserted **57**, and named the two that
/// failed — `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def` — at both
/// settle bounds, so it was not timing.
///
/// **Why it failed, and it was not a sloppy rule.** An `Engine` carries exactly
/// one `Config`, so every connection it serves is the **same** FIX identity.
/// That is why `Engine::turn` can enforce "this identity is already logged on"
/// by counting *the other connections in this engine* that are up: with one
/// identity per engine, "any other" and "this identity" are the same question.
/// Sharding split the connections for that one identity across engines that
/// cannot see each other, and the question stopped being answerable where it
/// was asked. **The rule was right; sharding invalidated its premise.**
///
/// **Why it passes now.** `presession::PendingSet` holds each socket until its
/// `Logon` arrives, and `Shards::hand` routes on the identity in it through a
/// **stable** hash — so both connections claiming one identity reach the same
/// engine, and `others_on` can see them both again
/// ([ADR-0020](../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)).
///
/// **What was deliberately not done, then and now.** The 2026-08-31 version
/// could have been made green with an assignment policy that kept both
/// connections on one shard, and it would have proved nothing —
/// `CLAUDE.md` §10 names that exact move. Instead it stayed red-in-waiting: a
/// characterisation test that would fail when the defect was fixed, and had to
/// be rewritten. `[measured 2026-09-01]` **it did fail, on 59 against 57**, and
/// this is the rewrite.
#[test]
fn two_shards_pass_all_fifty_nine_because_identity_decides_the_shard() {
    let _alone = alone();
    let Some(plan) = ShardWire::plan_for(2) else {
        // One physical core: two shards cannot be hosted here at all, and
        // `tests/shard.rs` asserts that the runtime refuses to try.
        return;
    };
    disposal_reset();
    let report = run(|_| ShardWire::with(&plan, STEP_QUIET, STEP_DEADLINE))
        .unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(
        report.passed, 59,
        "sharding must not cost a single definition:\n{report}"
    );
    assert!(
        report.failures.is_empty(),
        "and nothing may fail for a new reason:\n{report}"
    );

    // And it passed for the right reason.
    //
    // `[measured 2026-09-01]` this assertion started as `[0; 4]` and went red at
    // `[0, 1, 1, 0]`, which is exactly what it was written to catch: two
    // connections never reached an engine, and a socket the stage throws away
    // is indistinguishable from a duplicate the SESSION refused. Both turned
    // out to be legitimate and are named below — but "legitimate" is a thing
    // you find out, not a thing you assume, and 59/59 said nothing about it.
    //
    // Pinned rather than relaxed to a range: a THIRD connection disappearing
    // here would be a new defect wearing the same green.
    let [settled, timed_out, not_logon, gone, unrouted, unknown] = disposal_read();
    assert!(
        settled > 59,
        "every scenario connects at least once: {settled}"
    );
    assert_eq!(
        [timed_out, unrouted],
        [0, 0],
        "no connection may expire or fail to route: [timed_out, unrouted]"
    );
    assert_eq!(
        not_logon, 1,
        "exactly one: 1e_NotLogonMessage.def, whose first message is 35=0 and \
         whose own comment says `if first message is not a Logon, we must \
         disconnect` — ADR-0022"
    );
    assert_eq!(
        gone, 1,
        "exactly one: 1d_InvalidLogonLengthInvalid.def, whose 9=40 is a lie the \
         framer takes at its word, leaving a frame that can never be a message \
         — ADR-0022"
    );
    // Pinned, `[measured 2026-09-01]` by CI run 33512983304 on Linux with
    // `--features affinity`. ADR-0026's registry refuses an identity it does not
    // serve, one stage earlier than the session used to, and these two are the
    // only Logons in the corpus a one-counterparty registry does not recognise.
    // ADR-0029 is the amendment that made ADR-0022's count four.
    assert_eq!(
        unknown, 2,
        "exactly two: 1c_InvalidSenderCompID.def (49=WT) and \
         1c_InvalidTargetCompID.def (56=DLSI), both of whose own comments say the \
         link must be dropped, and both of which the registry now refuses before \
         a session exists — ADR-0029"
    );

    // Which leaves the two that mattered. `1b_DuplicateIdentity.def` and
    // `AlreadyLoggedOn.def` both connect twice, both settled, both reached an
    // engine — so their second Logon was refused by the SESSION, which is the
    // rule this whole plan existed to make true again under sharding.
}
