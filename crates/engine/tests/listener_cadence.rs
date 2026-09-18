//! The listener is asked on a cadence in the spin half, and on every wake in
//! the blocking half — ADR-0069, step 3 of
//! `docs/plans/2026-09-18-polling-the-listener-less-often-than-the-sessions.md`.
//!
//! # What these tests can see, and what they cannot
//!
//! `pump` is private and is reached only through the `serve_*` doors, so the
//! only observables are the ones a counterparty has: a socket, the bytes that
//! come back, and the event stream. **The number of turns the loop takes, and
//! the number of times `idle_with` returned, are not observable** — no public
//! seam carries them. So "accepted within N turns" is asserted as a wall-clock
//! bound that N turns fit inside many times over, and "did not spin through N
//! wakeups" is asserted with an N large enough that the spin *is* the clock:
//! `[2026-09-18]` five million `poll` returns cost seconds, one wake costs
//! microseconds, and the two are three orders of magnitude apart.
//!
//! # Why there is no red-first run here
//!
//! The default cadence is 1, which is the loop as it was written, so every
//! assertion below held before the counter existed. The evidence is the
//! reversals the plan names — remove the reset after `idle_with` and
//! `standard_does_not_spin_through_the_whole_cadence_before_accepting` goes
//! red; break the countdown so it never reaches zero and
//! `hft_accepts_within_n_turns_while_a_session_is_busy` goes red — exactly the
//! shape `crates/engine/tests/hft_wire.rs` records for the same reason.
//!
//! # What reversal B settled, and it is a limitation
//!
//! `[2026-09-18]` breaking the countdown so it never reaches zero
//! (`until_listener = max(1, until_listener - 1)`) left **all four tests
//! green**. The reason is decision 3 itself: the countdown is reset after every
//! return from `idle_with`, and an engine with a spare moment idles, so the
//! listener is reached through the reset rather than through the countdown.
//! The countdown is only load-bearing in a stretch of turns that all move —
//! which is where `accept4` was costing samples, and which no test here can
//! hold open. So: **these tests cannot tell a working countdown from a broken
//! one**; what they prove is that no cadence, however large, delays an accept
//! beyond a wake. `scripts/check-no-kernel-sleep.sh` with `strace -c` on Linux
//! (plan step 5) is what can count `accept4` and see the cadence at all.
//!
//! `serve_hft` and `serve` both need a poller for the pre-session stage, so
//! this file is gated as `hft_wire.rs` is: non-negotiable 6, the `mod` gated
//! rather than only the manifest.
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use fixbolt_engine::observe::Handles;
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::{Application, Config};

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

/// Two counterparties, so a second connection is a session of its own rather
/// than a duplicate of the first.
fn table() -> Table {
    Table::with_capacity(2)
        .serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
        .serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW55"))
}

fn free_addr() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

fn connect(addr: &str) -> TcpStream {
    for _ in 0..500 {
        if let Ok(s) = TcpStream::connect(addr) {
            s.set_nodelay(true).expect("nodelay");
            s.set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            return s;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the serving loop never bound {addr}");
}

fn read_one(client: &mut TcpStream) -> String {
    let mut buf = [0u8; 4096];
    let n = client.read(&mut buf).expect("a reply");
    String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|")
}

/// A message stamped at the wall clock — these doors build a `SystemClock`, so
/// the corpus's fixed instant would be refused for skew
/// (`docs/reference/two-time-rules-share-one-observable.md`).
fn stamped(body: &str) -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = cache.format(now, 0);
    let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
    let inner = body.replace("{52}", stamp);
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

fn logon(sender: &str, seq: u32) -> Vec<u8> {
    stamped(&format!(
        "35=A\u{1}34={seq}\u{1}49={sender}\u{1}52={{52}}\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}"
    ))
}

fn heartbeat(sender: &str, seq: u32) -> Vec<u8> {
    stamped(&format!(
        "35=0\u{1}34={seq}\u{1}49={sender}\u{1}52={{52}}\u{1}56=ISLD\u{1}"
    ))
}

/// **A new connection is accepted while a session is in full flow.**
///
/// `hft`, cadence 64. One counterparty holds a session up and keeps writing
/// `Heartbeat`s, so the loop has work on most turns and the cadence is the
/// thing deciding when the listener is asked. A second counterparty connects
/// and must be answered.
///
/// The bound is wall clock rather than turns, for the reason in the module
/// docs: 64 turns of a spinning loop with one session is under 50 µs
/// (`DESIGN.md` §8 budgets 449 ns a turn), so two seconds is four orders of
/// magnitude of headroom and only a loop that **never** asks the listener
/// fails it. That is the reversal: make the countdown never reach zero.
#[test]
fn hft_accepts_within_n_turns_while_a_session_is_busy() {
    let addr = free_addr();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();

    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve_hft(
            &serving,
            table(),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000)
                .expect("both above zero")
                .with_listener_every(64)
                .expect("above zero"),
            fixbolt_engine::msglog::NoLog,
            serving_handles,
        )
    });

    let mut busy = connect(&addr);
    busy.write_all(&logon("TW44", 1)).expect("send the Logon");
    let reply = read_one(&mut busy);
    assert!(reply.contains("|35=A|"), "the first session is up: {reply}");

    // Keep the loop fed for as long as the second connection is being made.
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let chatter = std::thread::spawn(move || {
        let mut seq = 2;
        while !stopping.load(Ordering::Relaxed) {
            if busy.write_all(&heartbeat("TW44", seq)).is_err() {
                break;
            }
            seq += 1;
        }
        busy
    });

    let began = Instant::now();
    let mut newcomer = connect(&addr);
    newcomer
        .write_all(&logon("TW55", 1))
        .expect("send the Logon");
    let reply = read_one(&mut newcomer);
    let took = began.elapsed();
    stop.store(true, Ordering::Relaxed);
    let _busy = chatter.join().expect("the chatter thread did not panic");

    assert!(
        reply.contains("|35=A|") && reply.contains("|56=TW55|"),
        "the busy engine still accepted and answered the new connection: {reply}"
    );
    assert!(
        took < Duration::from_secs(2),
        "the new connection was answered in {took:?}; 64 turns is tens of \
         microseconds, so this is a loop that stopped asking the listener"
    );

    admin.shutdown(2_000);
    let _ = engine.join().expect("the serving thread did not panic");
}

/// **An idle `hft` engine accepts on the next turn, whatever the cadence is.**
///
/// The trap the plan names: a cadence that also applied when there is nothing
/// to do would make a new connection wait N turns for no reason. It does not,
/// because `idle` returns at once in the spin half and the countdown is reset
/// after every return from it — the same line that keeps `standard` honest.
#[test]
fn hft_idle_engine_accepts_on_the_next_turn() {
    let addr = free_addr();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();

    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve_hft(
            &serving,
            table(),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000)
                .expect("both above zero")
                .with_listener_every(5_000_000)
                .expect("above zero"),
            fixbolt_engine::msglog::NoLog,
            serving_handles,
        )
    });

    let mut client = connect(&addr);
    let began = Instant::now();
    client.write_all(&logon("TW44", 1)).expect("send the Logon");
    let reply = read_one(&mut client);
    let took = began.elapsed();

    assert!(
        reply.contains("|35=A|"),
        "an idle engine with a huge cadence still answers: {reply}"
    );
    assert!(
        took < Duration::from_secs(2),
        "the idle engine took {took:?} to answer — five million turns of \
         waiting is exactly what the reset after idle exists to prevent"
    );

    admin.shutdown(2_000);
    let _ = engine.join().expect("the serving thread did not panic");
}

/// **`standard` accepts on the wake, not after the whole cadence.**
///
/// This is the rule-4 half of ADR-0069 decision 3. In `standard` a connect
/// wakes the poller because the listener is in the poll set; a wake that is not
/// answered by an `accept` leaves the listener readable, so the next `poll`
/// returns instantly, and the loop spins through the entire countdown before it
/// looks at the listener again.
///
/// The countdown is five million because that is what makes the spin
/// observable: with the reset, the accept happens on the wake and the reply is
/// back in milliseconds; without it, five million immediate `poll` returns are
/// seconds of wall clock and a busy core. `[2026-09-18]` the cadence of 64 the
/// plan first named is **not** observable through any public seam — 64 turns
/// cost microseconds either way. See the module docs.
#[test]
fn standard_does_not_spin_through_the_whole_cadence_before_accepting() {
    let addr = free_addr();
    let handles = Handles::new();
    let admin = handles.admin();
    let serving_handles = handles.clone();
    let serving = addr.clone();

    let engine = std::thread::spawn(move || {
        fixbolt_engine::serve(
            &serving,
            table(),
            EchoApp::default(),
            4,
            Limits::new(8, 30_000)
                .expect("both above zero")
                .with_listener_every(5_000_000)
                .expect("above zero"),
            fixbolt_engine::msglog::NoLog,
            serving_handles,
        )
    });

    // The engine is idle and blocked in `poll` before this connect, so the
    // accept that follows can only be the answer to the wake.
    let mut client = connect(&addr);
    let began = Instant::now();
    client.write_all(&logon("TW44", 1)).expect("send the Logon");
    let reply = read_one(&mut client);
    let took = began.elapsed();

    assert!(
        reply.contains("|35=A|"),
        "the blocking engine accepted on the wake: {reply}"
    );
    assert!(
        took < Duration::from_secs(2),
        "the Logon round trip took {took:?}. A blocking engine that answers a \
         wake by skipping the accept spins through its whole countdown first, \
         and that is what this number is measuring"
    );

    admin.shutdown(2_000);
    let _ = engine.join().expect("the serving thread did not panic");
}

/// **Cadence 1 is the loop as it was.**
///
/// A session over a kernel socket through `serve_hft` and through `serve`, with
/// the cadence named explicitly as 1 and with the default `Limits` that has
/// never been told about it — the same bytes back either way.
///
/// `[2026-09-18]` **this is not the 59 acceptance definitions.**
/// `crates/engine/tests/wire.rs` runs those over a socket, but it drives
/// `Engine::turn` by hand with a `ManualClock`, because every `I` line in the
/// corpus carries a fixed instant that a `SystemClock` refuses for skew. `pump`
/// is only reachable through the `serve_*` doors, and those build their own
/// `SystemClock`, so the corpus cannot be run through `pump` without a clock
/// seam on those doors. What that means here: the 59 defs gate the session
/// layer and are unchanged by this commit (`--test wire`, 59/59), and this test
/// gates the loop the defs cannot reach.
#[test]
fn listener_every_one_is_todays_loop() {
    for named in [true, false] {
        let base = Limits::new(8, 30_000).expect("both above zero");
        let limits = if named {
            base.with_listener_every(1).expect("above zero")
        } else {
            base
        };
        assert_eq!(
            limits.listener_every().get(),
            1,
            "the default cadence and the one named as 1 are the same number"
        );

        let addr = free_addr();
        let handles = Handles::new();
        let admin = handles.admin();
        let serving_handles = handles.clone();
        let serving = addr.clone();
        let engine = std::thread::spawn(move || {
            fixbolt_engine::serve_hft(
                &serving,
                table(),
                EchoApp::default(),
                4,
                limits,
                fixbolt_engine::msglog::NoLog,
                serving_handles,
            )
        });

        let mut client = connect(&addr);
        client.write_all(&logon("TW44", 1)).expect("send the Logon");
        let reply = read_one(&mut client);
        assert!(
            reply.contains("|35=A|") && reply.contains("|34=1|"),
            "cadence 1 (named: {named}) serves exactly as before: {reply}"
        );

        client
            .write_all(&heartbeat("TW44", 2))
            .expect("send a Heartbeat");
        admin.shutdown(2_000);
        let stopped = engine.join().expect("the serving thread did not panic");
        let shutdown = stopped.expect("serve_hft returned a Shutdown");
        assert_eq!(
            shutdown.sessions(),
            1,
            "the shutdown counted the session it was serving: {shutdown:?}"
        );
    }
}
