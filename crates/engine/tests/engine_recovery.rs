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
