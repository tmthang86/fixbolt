//! When a session is open, and when both sides start again at `34=1`.
//!
//! **Step 1 of [session-schedules], and it is written to be red at an
//! assertion.** `[verified 2026-09-02]` this engine has no notion of a trading
//! day: `Session::new` resets and `Session::resume` does not
//! ([ADR-0010](../../../docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)),
//! and nothing anywhere tells the embedder *when* to choose. `GUIDE.md` §9 says
//! so in as many words.
//!
//! # Why this file names no type that does not exist yet
//!
//! A test asking for `Schedule` before `Schedule` exists can only be red at the
//! compiler, and a test that does not compile has measured nothing. So step 1
//! uses **only today's API** and asserts what the engine must do — refuse a
//! `Logon` at 3 a.m. — which it does not. The arithmetic arrives in step 2,
//! with the type it is about.
//!
//! # Why the reset is asked as a comparison and not as an alarm
//!
//! QuickFIX decides this with `isSameSession(t1, t2)` — *do these two instants
//! fall in the same interval* — and the reason is worth restating. An engine
//! that slept through midnight, or a process that started at 06:00, gets no
//! alarm. It has only two facts: the last instant it remembers, and now. **The
//! moment a reset matters most is the moment nobody was running to hear a
//! bell.**
//!
//! # Why the corpus cannot help here
//!
//! All 59 acceptance definitions run inside one interval, so none of them can
//! tell a schedule that works from one that is never consulted. Everything in
//! this file is held by this file.
//!
//! [session-schedules]: ../../../docs/plans/2026-09-02-session-schedules.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_codec::timestamp::TimestampCache;
use fixbolt_conformance::script::{
    FIXED_TIME_IN, FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum,
};
use fixbolt_session::clock::MILLIS_YEAR_ZERO_TO_EPOCH;
use fixbolt_session::{Acceptor, Config, Link, Session};

/// One whole day, in the milliseconds `Input::Tick` carries.
const DAY_MS: u64 = 86_400_000;
const HOUR_MS: u64 = 3_600_000;

/// Midnight of the corpus's day, derived from its fixed instant rather than
/// written down.
fn midnight() -> u64 {
    FIXED_TIME_MILLIS - (FIXED_TIME_MILLIS % DAY_MS)
}

/// A real Logon from the corpus, its one deliberately wrong field corrected,
/// **and its `SendingTime` restamped to `at`**.
///
/// `[measured 2026-09-02]` **the restamping is not a detail; without it this
/// whole file measures the wrong rule.** The corpus's Logon is stamped
/// `20260828-12:00:00`, and `max_skew_ms` refuses anything more than 120 s from
/// the engine's clock. So a session ticked to 03:00 refused it — and the first
/// version of `a_logon_outside_the_trading_day_is_refused` **passed**, on a
/// clock-skew rule, in a file about a calendar. Two time-based rules, one
/// observable, and the test could not tell them apart.
///
/// A hand-written packet would prove the parser handles a packet nobody sends
/// (`CLAUDE.md` §7), so the bytes stay the corpus's and only `52=` moves.
fn logon_stamped(at: u64) -> Vec<u8> {
    let wire = scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|s| s.file == "1c_InvalidTargetCompID.def")
        .expect("the corpus has it")
        .steps
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m) => Some(m.wire),
            _ => None,
        })
        .expect("it has an I line");
    let s = String::from_utf8(wire).expect("ascii");
    let fixed = s.replace("56=DLSI", "56=ISLD");
    assert_ne!(
        fixed, s,
        "the file's wrong field is the one being corrected"
    );

    // The seconds form, 17 bytes, exactly as long as the one already there — so
    // `9=` does not move and only the checksum has to be recomputed.
    let mut cache = TimestampCache::new();
    let full = *cache.format(at - MILLIS_YEAR_ZERO_TO_EPOCH);
    let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
    let old = format!("52={FIXED_TIME_IN}");
    assert!(
        fixed.contains(&old),
        "the corpus Logon must carry {old} for this substitution to mean anything: {fixed}"
    );
    let restamped = fixed.replace(&old, &format!("52={stamp}"));
    assert!(
        restamped.contains(&format!("52={stamp}")),
        "and it must carry the new one afterwards: {restamped}"
    );
    with_real_checksum(restamped.as_bytes())
}

/// The configuration a venue open 08:00–17:00 UTC would want. **Today it can
/// only say who it is**, which is the gap.
fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// Drive a session to `now_ms` and offer it a real Logon. Returns whether the
/// link survived and how many messages went out.
fn logon_at(cfg: Config, now_ms: u64) -> (Link, usize) {
    let mut session: Session<Acceptor, 256> = Session::new(cfg);
    let mut sent = 0usize;
    session.connect(|_| sent += 1);
    session.tick(now_ms, |_| sent += 1);
    let link = session.received(&logon_stamped(now_ms), |_| sent += 1);
    (link, sent)
}

/// **The specification.** An acceptor outside its trading hours is not a FIX
/// endpoint. Before a `Logon` there is nothing to answer with, so it says
/// nothing and hangs up — like every other pre-Logon refusal.
#[test]
fn a_logon_outside_the_trading_day_is_refused() {
    let (link, sent) = logon_at(cfg(), midnight() + 3 * HOUR_MS);

    assert_eq!(
        link,
        Link::Dropped,
        "3 a.m. is not inside a venue's 08:00-17:00 session"
    );
    assert_eq!(sent, 0, "and the refusal is silent, like every other one");
}

/// **The control, and it is why the test above means anything.** Same
/// configuration, same message, an hour any schedule would call open. If this
/// one ever goes red with the other, the harness broke rather than the engine.
#[test]
fn a_logon_inside_the_trading_day_is_accepted() {
    let (link, sent) = logon_at(cfg(), midnight() + 12 * HOUR_MS);

    assert_eq!(link, Link::Up, "midday is inside 08:00-17:00");
    assert_eq!(sent, 1, "and the acceptor answered with its own Logon");
}

/// **What the protocol actually cares about**, and the thing an
/// is-the-link-up assertion cannot see: the first `Logon` of a new trading day
/// carries `34=1`, even for a session resumed holding higher numbers.
#[test]
fn the_first_logon_of_a_new_trading_day_is_numbered_one() {
    // Yesterday this session reached 34=41 in each direction and it was
    // persisted; `Session::resume` is what carries that across a restart.
    let mut session: Session<Acceptor, 256> = Session::resume(cfg(), 41, 41);
    let mut out = Vec::new();
    session.connect(|_| {});
    // A whole day later, inside today's window.
    let tomorrow = midnight() + DAY_MS + 12 * HOUR_MS;
    session.tick(tomorrow, |_| {});
    let link = session.received(&logon_stamped(tomorrow), |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });

    assert_eq!(link, Link::Up, "a new trading day accepts a Logon");
    let reply = out.first().expect("the acceptor answered");
    assert!(
        reply.contains("|34=1|"),
        "a new trading day starts at one, not at 42: {reply}"
    );
    assert_eq!(
        session.next_in(),
        2,
        "and the inbound count restarted too — their Logon was this day's first"
    );
}

/// The other half, and without it the test above passes for a session that
/// simply always resets. Same day, resumed numbers, **no** reset —
/// [ADR-0010](../../../docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md)
/// is what this one protects.
#[test]
fn a_reconnect_inside_the_same_trading_day_keeps_its_numbers() {
    let mut session: Session<Acceptor, 256> = Session::resume(cfg(), 41, 41);
    session.connect(|_| {});
    session.tick(midnight() + 12 * HOUR_MS, |_| {});

    assert_eq!(
        session.next_out(),
        41,
        "the same trading day is a reconnect, not a restart"
    );
}
