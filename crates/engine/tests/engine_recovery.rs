//! Recovery **through an `Engine`**, which is the part nobody ever asked for.
//!
//! `STATUS.md` item 31, step 1 of [an-engine-can-resume].
//!
//! # Why this file exists beside `recovery.rs`
//!
//! `crates/engine/tests/recovery.rs` proves that a journal reads back and that
//! `Session::resume` carries the numbers. `[verified 2026-09-02]` it does so
//! with **zero occurrences of `Engine` in the file** — it drives a `Session`
//! directly. Item 16 closed on it, truthfully, and the seam above was never
//! asked about: both of `Engine`'s `add` methods build `Session::new`, which
//! resets, and there is no public path to anything else.
//!
//! So `Session::resume`, `resume_at`, ADR-0010, ADR-0017 and
//! `Durability::Fsync` are all real, all tested, and all **unreachable through
//! the engine**. A layer was finished and the join was not.
//!
//! **Every test here builds an `Engine` and goes through its public API.** That
//! is the point, and it is deliberately not something a `grep` can assert.
//!
//! [an-engine-can-resume]: ../../../docs/plans/2026-09-02-an-engine-can-resume.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use fixbolt_conformance::script::FIXED_TIME_MILLIS;
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::{Io, Loopback, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};
use fixbolt_session::journal::Journal;
use fixbolt_session::schedule::Schedule;

const N: usize = 256;
const RX: usize = 4096;
const TX: usize = 8192;

type Acc = Engine<
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

fn engine() -> Acc {
    Engine::new(
        cfg(),
        InlineDispatch::new(EchoApp::default()),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    )
}

/// A `Logon` from the counterparty, numbered `seq`. Built with a real body
/// length and checksum by the conformance helper.
fn logon(seq: u32) -> Vec<u8> {
    wire(&format!("35=A\u{1}34={seq}\u{1}98=0\u{1}108=30\u{1}"))
}

/// An application message, numbered `seq`.
fn order(seq: u32) -> Vec<u8> {
    wire(&format!(
        "35=D\u{1}34={seq}\u{1}11=ord{seq}\u{1}55=X\u{1}54=1\u{1}38=100\u{1}40=1\u{1}"
    ))
}

/// Wrap a body in a FIX.4.4 frame from `TW44` to `ISLD` at the corpus's fixed
/// instant, with `9=` and `10=` computed rather than guessed.
fn wire(body: &str) -> Vec<u8> {
    let stamp = fixbolt_conformance::script::FIXED_TIME_IN;
    let (head, rest) = body.split_at(body.find('\u{1}').expect("a first field") + 1);
    let inner = format!("{head}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}{rest}");
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

/// Read whatever the engine has put on the wire, as a `|`-separated string.
fn drain(peer: &mut Loopback) -> String {
    let mut out = String::new();
    let mut buf = [0u8; 8192];
    while let Io::Ready(n) = peer.recv(&mut buf) {
        if n == 0 {
            break;
        }
        out.push_str(&String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|"));
    }
    out
}

/// A journal that already holds a session's history — what a restart finds.
///
/// `MemJournal` rather than a file, because the question here is whether the
/// **engine** consults a journal at all, not whether bytes survive a `drop`.
/// `recovery.rs` already owns the second question and uses `FileJournal` for
/// exactly that reason.
fn a_journal_with_history() -> Store {
    let mut j = Store::new();
    j.put(7, &order(7));
    j.put(8, &order(8));
    j.mark_in(11);
    assert_eq!(
        j.highest(),
        Some(8),
        "the premise: it holds outbound history"
    );
    assert_eq!(j.highest_in(), Some(11), "and inbound history");
    j
}

/// **The specification.** An engine handed a journal with history must continue
/// the session rather than start a new one.
///
/// `[verified 2026-09-02]` today it cannot: `Engine::add_with_journal` builds
/// `Session::new(cfg)`, so the counts restart whatever the journal holds and
/// `Durability::Fsync` buys an audit trail rather than a recovery mechanism —
/// one layer above where `Journal::highest`'s own rustdoc warns about it.
#[test]
fn an_engine_given_a_journal_with_history_continues_the_session() {
    let mut e = engine();
    let (mut peer, engine_side) = Loopback::pair();
    let journal = a_journal_with_history();

    let next_out = journal.highest().expect("history") + 1;
    let next_in = journal.highest_in().expect("history") + 1;
    e.add_resumed(engine_side, cfg(), journal, next_out, next_in, None);

    // The counterparty comes back where it left off.
    let _ = peer.send(&logon(next_in));
    e.turn();
    let reply = drain(&mut peer);

    assert!(
        reply.contains("|35=A|"),
        "the acceptor answered the Logon: {reply}"
    );
    assert!(
        reply.contains(&format!("|34={next_out}|")),
        "and it answered as message {next_out}, not as message 1: {reply}"
    );
}

/// **The control, and it is what keeps the test above honest.** The ordinary
/// `add` must still reset — ADR-0010 says a session nobody resumed restarts,
/// and the acceptance corpus depends on it.
#[test]
fn an_engine_given_a_plain_connection_still_starts_at_one() {
    let mut e = engine();
    let (mut peer, engine_side) = Loopback::pair();
    e.add(engine_side);

    let _ = peer.send(&logon(1));
    e.turn();
    let reply = drain(&mut peer);

    assert!(
        reply.contains("|34=1|"),
        "a session nobody resumed starts at one: {reply}"
    );
}

/// **Numbers without the messages is half a recovery**, and the half that looks
/// finished. A resumed session must be able to answer a `ResendRequest` for
/// what it sent before the restart — with a replay, not a gap fill.
///
/// `[2026-09-02]` this is why `add_resumed` takes the journal rather than only
/// the two counts: every connection gets `J::default()` today, so correct
/// numbers over an empty journal answer the first `ResendRequest` with
/// `35=4`.
#[test]
fn a_resumed_session_replays_what_it_sent_before_the_restart() {
    let mut e = engine();
    let (mut peer, engine_side) = Loopback::pair();
    let journal = a_journal_with_history();
    let next_out = journal.highest().expect("history") + 1;
    let next_in = journal.highest_in().expect("history") + 1;
    e.add_resumed(engine_side, cfg(), journal, next_out, next_in, None);

    let _ = peer.send(&logon(next_in));
    e.turn();
    let _ = drain(&mut peer);

    // "Send me 7 through 8 again" — messages this session put on the wire
    // before the process restarted.
    let _ = peer.send(&wire("35=2\u{1}34=12\u{1}7=7\u{1}16=8\u{1}"));
    e.turn();
    let replayed = drain(&mut peer);

    assert!(
        replayed.contains("|35=D|"),
        "the held messages came back, rather than a gap fill: {replayed}"
    );
    assert!(
        replayed.contains("|43=Y|"),
        "and they are marked as replays: {replayed}"
    );
}

/// **A restart across a trading-day boundary must start again at `34=1`**, even
/// though the journal is full of yesterday's numbers.
///
/// This is the case [ADR-0033](../../../docs/decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)
/// exists for, and until `add_resumed` it could not be reached from an engine
/// at all. `last_active_ms` is what makes it decidable: the numbers alone say
/// nothing about whether a day has ended since they were reached.
#[test]
fn a_resumed_session_that_crossed_a_boundary_starts_again_at_one() {
    let hours = Schedule::daily(8 * 3_600, 17 * 3_600).expect("08:00 to 17:00");
    let scheduled = cfg().with_schedule(hours);
    let mut e = engine();
    let (mut peer, engine_side) = Loopback::pair();
    let journal = a_journal_with_history();

    // The engine's clock reads the corpus's instant, midday. This session was
    // last active at 16:00 the *previous* day.
    let yesterday_close = FIXED_TIME_MILLIS - 86_400_000 + 4 * 3_600_000;
    e.add_resumed(
        engine_side,
        scheduled,
        journal,
        9,
        12,
        Some(yesterday_close),
    );

    // A new trading day, so the counterparty opens at one too.
    let _ = peer.send(&logon(1));
    e.turn();
    let reply = drain(&mut peer);

    assert!(
        reply.contains("|35=A|"),
        "the acceptor answered the Logon: {reply}"
    );
    assert!(
        reply.contains("|34=1|"),
        "a new trading day starts at one, not at nine: {reply}"
    );
}

/// The other half, and without it the test above passes for an engine that
/// simply always resets when handed a schedule. Same trading day, same
/// journal — the numbers are kept.
#[test]
fn a_resumed_session_inside_the_same_trading_day_keeps_its_numbers() {
    let hours = Schedule::daily(8 * 3_600, 17 * 3_600).expect("08:00 to 17:00");
    let scheduled = cfg().with_schedule(hours);
    let mut e = engine();
    let (mut peer, engine_side) = Loopback::pair();
    let journal = a_journal_with_history();

    // Active at 09:00 today; the clock reads midday. One trading day, so this
    // is a reconnect and not a restart — ADR-0010.
    let this_morning = FIXED_TIME_MILLIS - 3 * 3_600_000;
    e.add_resumed(engine_side, scheduled, journal, 9, 12, Some(this_morning));

    let _ = peer.send(&logon(12));
    e.turn();
    let reply = drain(&mut peer);

    assert!(
        reply.contains("|34=9|"),
        "the same trading day keeps counting: {reply}"
    );
}

/// **The discriminator.** A resumed session whose journal holds *nothing*
/// answers the same `ResendRequest` with a gap fill.
///
/// Without this, `a_resumed_session_replays_what_it_sent_before_the_restart`
/// proves only that *something* came back. The two outcomes are both legal FIX
/// and they are the difference between recovering a session and quietly losing
/// everything the counterparty asked for — so they are told apart here rather
/// than assumed.
///
/// It is also the closest thing to a reversal that the journal argument admits:
/// `add_resumed` **requires** a journal, so there is no version of the engine
/// that forgets to pass one. What can be varied is what the journal holds, and
/// this varies it.
#[test]
fn a_resumed_session_with_an_empty_journal_fills_the_gap_instead() {
    let mut e = engine();
    let (mut peer, engine_side) = Loopback::pair();

    // The numbers of a session with history; the journal of one without.
    let mut empty = Store::new();
    empty.mark_in(11);
    assert_eq!(empty.highest(), None, "the premise: it holds no messages");
    e.add_resumed(engine_side, cfg(), empty, 9, 12, None);

    let _ = peer.send(&logon(12));
    e.turn();
    let _ = drain(&mut peer);

    let _ = peer.send(&wire("35=2\u{1}34=13\u{1}7=7\u{1}16=8\u{1}"));
    e.turn();
    let answer = drain(&mut peer);

    assert!(
        answer.contains("|35=4|"),
        "nothing to replay, so a SequenceReset gap fill: {answer}"
    );
    assert!(
        !answer.contains("|35=D|"),
        "and definitely not the messages, which this journal never held: {answer}"
    );
}

/// The serving loop, and **`standard` only**.
///
/// `serve` and `serve_with_recovery` build the blocking engine, which does not
/// exist without that feature — non-negotiable 6. `[measured 2026-09-02]` this
/// module was not gated at first and `cargo test --all --no-default-features`
/// **passed anyway**: `tools/w2w` depends on `fixbolt-engine` with defaults and
/// cargo unifies features across one invocation, so the flag under test was
/// switched back on by a sibling crate. `scripts/check-no-optional-deps.sh`
/// asks per crate and is what caught it.
#[cfg(all(feature = "standard", unix))]
mod serving {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use fixbolt_engine::journal::Store;
    use fixbolt_engine::presession::{Limits, Table};
    use fixbolt_engine::recovery::Resumed;
    use fixbolt_session::Config;

    use super::{EchoApp, a_journal_with_history, cfg};

    // --- through the serving loop --------------------------------------------
    //
    // Everything above drives `Engine::add*` directly. **A deployment does not.**
    // It calls `serve`, which accepts connections itself — so these two go through
    // the real listener, a real socket, and `serve_with_recovery`, which is the only
    // way to find out whether the seam is actually wired.

    /// A `Recovery` that hands back one counterparty's history, and records how many
    /// times it was asked.
    struct OneCounterparty {
        asked: Arc<AtomicUsize>,
    }

    impl fixbolt_engine::recovery::Recovery<Store> for OneCounterparty {
        // `[2026-09-02]` `fresh` became a required method when `J: Default` had
        // to leave the serving loop — a `where` clause on a default body lands
        // on callers, so it had only moved. Item 32 (b).
        fn fresh(&mut self, _cfg: &Config) -> Store {
            Store::default()
        }

        fn recover(&mut self, cfg: &Config) -> Option<Resumed<Store>> {
            self.asked.fetch_add(1, Ordering::Relaxed);
            // Only for the counterparty this test is about, so "it answered for
            // everybody" and "it answered for the right one" are different results.
            if !cfg.serves(b"TW44", b"ISLD") {
                return None;
            }
            Some(Resumed {
                journal: a_journal_with_history(),
                next_out: 9,
                next_in: 12,
                last_active_ms: None,
            })
        }
    }

    /// **The specification for step 3.** A connection accepted by the serving loop
    /// resumes, without the embedder ever seeing the transport.
    #[test]
    fn a_connection_through_the_serving_loop_resumes() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let addr = listener.local_addr().expect("bound").to_string();
        drop(listener);

        let asked = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&asked);
        let serving = addr.clone();
        std::thread::spawn(move || {
            let table = Table::with_capacity(1).serving(cfg());
            let _ = fixbolt_engine::serve_with_recovery(
                &serving,
                table,
                EchoApp::default(),
                4,
                Limits::new(8, 30_000).expect("both above zero"),
                OneCounterparty { asked: counter },
                fixbolt_engine::msglog::NoLog,
            );
        });

        let mut client = connect(&addr);
        // The engine's clock is the real one here, so the Logon has to be stamped
        // now rather than at the corpus's fixed instant — `max_skew_ms` would
        // refuse it otherwise, and this test would go green or red on the clock
        // rather than on recovery. See
        // docs/reference/two-time-rules-share-one-observable.md.
        client.write_all(&logon_now(12)).expect("send the Logon");

        let reply = read_one(&mut client);
        assert!(
            reply.contains("|35=A|"),
            "the acceptor answered the Logon: {reply}"
        );
        assert!(
            reply.contains("|34=9|"),
            "and it resumed at nine rather than starting at one: {reply}"
        );
        assert!(
            asked.load(Ordering::Relaxed) >= 1,
            "the serving loop asked the Recovery at all"
        );
    }

    /// The control. **`serve` itself must be unchanged** — it passes `NoRecovery`,
    /// and a session nobody resumed still starts at one.
    #[test]
    fn the_plain_serving_loop_still_starts_at_one() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let addr = listener.local_addr().expect("bound").to_string();
        drop(listener);

        let serving = addr.clone();
        std::thread::spawn(move || {
            let table = Table::with_capacity(1).serving(cfg());
            let _ = fixbolt_engine::serve(
                &serving,
                table,
                EchoApp::default(),
                4,
                Limits::new(8, 30_000).expect("both above zero"),
                fixbolt_engine::msglog::NoLog,
            );
        });

        let mut client = connect(&addr);
        client.write_all(&logon_now(1)).expect("send the Logon");

        let reply = read_one(&mut client);
        assert!(
            reply.contains("|34=1|"),
            "no recovery, so the session starts at one: {reply}"
        );
    }

    /// Connect, retrying while the serving thread gets to its `bind`.
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

    /// A `Logon` stamped at the wall clock, for the tests that run against a real
    /// `SystemClock` rather than a `ManualClock`.
    fn logon_now(seq: u32) -> Vec<u8> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after 1970")
            .as_millis() as u64;
        let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
        let full = *cache.format(now);
        let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
        let inner = format!(
            "35=A\u{1}34={seq}\u{1}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}"
        );
        let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
        fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
    }
}
