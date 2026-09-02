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
use fixbolt_session::schedule::{Schedule, Weekday, Weekdays};
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

/// The configuration a venue open 08:00–17:00 UTC wants.
///
/// `[step 1]` this could only say who it is; the schedule is step 3.
fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_schedule(eight_to_five())
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
    // Yesterday this session reached 34=41 in each direction, and **the engine
    // persisted when it was last active as well as the numbers**. Without that
    // instant no reset can be decided at all — `Session::resume` is the version
    // that was not told, and it deliberately never resets.
    let yesterday = midnight() + 16 * HOUR_MS;
    let mut session: Session<Acceptor, 256> = Session::resume_at(cfg(), 41, 41, yesterday);
    let mut out = Vec::new();
    session.connect(|_| {});
    // A whole day later, inside today's window.
    let tomorrow = midnight() + DAY_MS + 12 * HOUR_MS;
    session.tick(tomorrow, |_| {});

    // **Asserted here, before any message is judged.** Without this the only
    // evidence of a reset is that the Logon below was accepted — and a session
    // that did not reset refuses `34=1` as too low, so the failure would read
    // as a dropped link and point at the wrong rule. `[measured 2026-09-02]`
    // suppressing the reset with this line absent turned exactly one test red,
    // on `a new trading day accepts a Logon`, which is a connection assertion.
    assert_eq!(
        session.next_out(),
        1,
        "the boundary passed on the tick, so the counts restarted before anything was numbered"
    );
    assert_eq!(session.next_in(), 1, "and the inbound count with it");

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

// --- the arithmetic ------------------------------------------------------
//
// Step 2. These name `Schedule`, which is why they could not exist in step 1.

/// 08:00 to 17:00 UTC, every day. The ordinary case, and where every venue
/// document starts.
fn eight_to_five() -> Schedule {
    Schedule::daily(8 * 3_600, 17 * 3_600).expect("08:00 before 17:00, both inside a day")
}

/// **Midnight is a boundary**, and two instants either side of it are not the
/// same trading day. This is the comparison the sequence-number reset is made
/// of.
#[test]
fn two_instants_across_a_boundary_are_not_the_same_session() {
    let s = eight_to_five();
    let today = midnight() + 9 * HOUR_MS;

    assert!(
        s.same_session(today, today + HOUR_MS),
        "09:00 and 10:00 on one day are one session"
    );
    assert!(
        !s.same_session(today, today + DAY_MS),
        "09:00 today and 09:00 tomorrow are not, and this is what resets 34="
    );
}

/// A schedule says when it is open, and the answer is not *always*.
#[test]
fn a_daily_schedule_is_open_between_its_hours_and_shut_outside_them() {
    let s = eight_to_five();
    let m = midnight();

    assert!(!s.contains(m + 7 * HOUR_MS), "07:00 is before the open");
    assert!(s.contains(m + 8 * HOUR_MS), "08:00 is the open itself");
    assert!(s.contains(m + 12 * HOUR_MS), "midday is inside");
    assert!(
        !s.contains(m + 17 * HOUR_MS),
        "17:00 is the close, exclusive"
    );
    assert!(!s.contains(m + 20 * HOUR_MS), "the evening is shut");
}

/// **A session that runs past midnight is what a naive `open <= t < close`
/// gets wrong**, and plenty of venues run one.
#[test]
fn a_session_that_crosses_midnight_is_open_on_both_sides_of_it() {
    let s = Schedule::daily(22 * 3_600, 6 * 3_600).expect("22:00 to 06:00 wraps, and is legal");
    let m = midnight();

    assert!(s.contains(m + 23 * HOUR_MS), "23:00 is inside");
    assert!(s.contains(m + DAY_MS + HOUR_MS), "01:00 is still inside");
    assert!(!s.contains(m + 12 * HOUR_MS), "midday is not");
    assert!(!s.contains(m + 7 * HOUR_MS), "and neither is 07:00");

    // The interval is one session, not two, which is the whole point.
    assert!(
        s.same_session(m + 23 * HOUR_MS, m + DAY_MS + HOUR_MS),
        "23:00 and 01:00 belong to one trading day"
    );
    assert!(
        !s.same_session(m + 23 * HOUR_MS, m + DAY_MS + 23 * HOUR_MS),
        "and the following night is the next one"
    );
}

/// **`always()` must be exactly neutral.** It is the default every existing
/// user gets, and the 59 acceptance definitions run under it.
#[test]
fn an_always_schedule_is_open_forever_and_never_resets() {
    let s = Schedule::always();

    assert!(s.contains(0), "the beginning of the timeline");
    assert!(s.contains(midnight() + 3 * HOUR_MS), "3 a.m.");
    assert!(s.contains(u64::MAX), "and the end of it");
    assert!(
        s.same_session(0, u64::MAX),
        "there is one session and it has no boundary"
    );
    assert_eq!(
        Schedule::default(),
        s,
        "and it is what a Config gets without asking"
    );
}

/// A schedule that cannot be honoured is refused where it is written, not
/// discovered at 3 a.m. Non-negotiable 7: `Option`, never a panic.
#[test]
fn a_schedule_that_makes_no_sense_is_refused_rather_than_repaired() {
    assert!(
        Schedule::daily(8 * 3_600, 8 * 3_600).is_none(),
        "a zero-length interval is open never, or always, and neither was meant"
    );
    assert!(
        Schedule::daily(0, 86_400).is_none(),
        "86400 is the next day's first second, not this day's last"
    );
    assert!(
        Schedule::daily(90_000, 100).is_none(),
        "and 90000 is not one"
    );
    assert!(
        eight_to_five().with_weekdays(Weekdays::NONE).is_none(),
        "open on no days is not a schedule"
    );
    assert!(
        Schedule::always()
            .with_weekdays(Weekdays::WEEKDAYS)
            .is_none(),
        "always() has no interval to restrict, and silently making one would be worse"
    );
    assert!(
        eight_to_five()
            .with_utc_offset_ms(25 * HOUR_MS as i64)
            .is_none(),
        "no zone is 25 hours from UTC"
    );
}

/// Weekdays filter which day an interval may **open** on — so a Friday-night
/// session runs into Saturday under `WEEKDAYS`, because it started on Friday.
#[test]
fn weekdays_select_the_day_a_session_opens_and_not_the_days_it_covers() {
    let s = eight_to_five()
        .with_weekdays(Weekdays::WEEKDAYS)
        .expect("Monday to Friday");

    // Find a known Monday by walking forward from the corpus's own day.
    let monday = (0..7)
        .map(|d| midnight() + d * DAY_MS)
        .find(|m| s.contains(m + 12 * HOUR_MS))
        .expect("one of any seven days is a weekday");

    assert!(
        s.contains(monday + 12 * HOUR_MS),
        "a weekday midday is open"
    );
    let mut open_days = 0;
    for d in 0..7 {
        if s.contains(monday + d * DAY_MS + 12 * HOUR_MS) {
            open_days += 1;
        }
    }
    assert_eq!(open_days, 5, "five days of any seven, not four and not six");

    // And a night session that opens on Friday runs into Saturday.
    let night = Schedule::daily(22 * 3_600, 6 * 3_600)
        .expect("legal")
        .with_weekdays(Weekdays::only(Weekday::Friday))
        .expect("Fridays");
    let friday = (0..7)
        .map(|d| midnight() + d * DAY_MS)
        .find(|m| night.contains(m + 23 * HOUR_MS))
        .expect("one of any seven days is a Friday");
    assert!(
        night.contains(friday + DAY_MS + HOUR_MS),
        "01:00 on Saturday belongs to the session that opened on Friday"
    );
    assert!(
        !night.contains(friday + DAY_MS + 23 * HOUR_MS),
        "but Saturday night itself never opens"
    );
}

/// A weekly window is **one** session spanning most of the week, so nothing
/// resets on Tuesday night.
#[test]
fn a_weekly_session_does_not_reset_in_the_middle_of_the_week() {
    let s = Schedule::weekly(Weekday::Sunday, 21 * 3_600, Weekday::Friday, 21 * 3_600)
        .expect("Sunday evening to Friday evening");

    // Walk a fortnight and count the distinct sessions seen at midday.
    let mut starts = Vec::new();
    for d in 0..14 {
        let t = midnight() + d * DAY_MS + 12 * HOUR_MS;
        if s.contains(t) && !starts.iter().any(|&x| s.same_session(x, t)) {
            starts.push(t);
        }
    }
    assert_eq!(
        starts.len(),
        2,
        "two weeks are two sessions, not ten and not one: {starts:?}"
    );
}

/// A fixed offset shifts the window, and **it is not daylight saving**. That
/// warning is in the rustdoc; this asserts the arithmetic it applies to.
#[test]
fn a_fixed_offset_moves_the_window_by_exactly_that_much() {
    let utc = eight_to_five();
    // A zone seven hours east: its 08:00 is 01:00 UTC.
    let east = utc
        .with_utc_offset_ms(7 * HOUR_MS as i64)
        .expect("inside a day");
    let m = midnight();

    assert!(east.contains(m + HOUR_MS), "01:00 UTC is 08:00 there");
    assert!(
        !east.contains(m + 12 * HOUR_MS),
        "midday UTC is 19:00, shut"
    );
    assert!(
        utc.contains(m + 12 * HOUR_MS),
        "and the unshifted schedule still is open then — the offset moved it, nothing else"
    );
}

/// **The fail-safe, and it had no test until it was looked for.**
///
/// `same_session` says *false* for an instant inside no interval — including
/// against another such instant. An engine that cannot place what it remembers
/// therefore resets rather than carrying a sequence number across a boundary it
/// could not see.
///
/// The direction matters and is not symmetric in cost: resetting when the
/// counterparty did not is a `Logon` argument, visible immediately. Failing to
/// reset when they did is a silent divergence that surfaces messages later.
#[test]
fn an_instant_inside_no_session_is_never_the_same_session_as_anything() {
    let s = eight_to_five();
    let m = midnight();
    let shut = m + 3 * HOUR_MS;
    let also_shut = m + 4 * HOUR_MS;
    let open = m + 12 * HOUR_MS;

    assert!(!s.contains(shut), "the premise: 3 a.m. is shut");
    assert!(!s.contains(also_shut), "and so is 4 a.m.");
    assert!(
        !s.same_session(shut, also_shut),
        "two instants in the same closed stretch are still not a session"
    );
    assert!(!s.same_session(shut, open), "nor is one of each");
    assert!(
        s.same_session(open, open + HOUR_MS),
        "and the control: two open instants on one day are"
    );
}
