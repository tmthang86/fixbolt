//! Why a connection ended, and today the answer is one bit.
//!
//! **Step 1 of [why-a-connection-ended], red at an assertion.** `[verified
//! 2026-09-02]` `Link::Dropped` is returned from eighteen places in
//! `crates/session/src/lib.rs` — a wrong `BeginString`, a wrong identity, a
//! `SendingTime` too far out, a sequence number already used, a first message
//! that is not a `Logon`, an hour outside the schedule — and **nothing at the
//! other end tells them apart**.
//!
//! # What that has already cost, twice, this week
//!
//! A schedule test passed on `max_skew_ms` rather than on the schedule
//! (`reference/two-time-rules-share-one-observable.md`), and a `Logon` was
//! refused for a `FieldIndex` too small while the failure message blamed a
//! registry that did not exist
//! (`reference/silence-before-a-logon-has-many-causes.md`). Both write-ups end
//! on the same sentence: the cheapest structural defence is to make the reason
//! observable.
//!
//! # Why the corpus cannot help
//!
//! Six acceptance definitions expect **no response at all**, which is the same
//! observable for every cause. 59/59 is blind to everything in this file.
//!
//! [why-a-connection-ended]: ../../../docs/plans/2026-09-02-why-a-connection-ended.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_session::{Acceptor, Config, DropReason, Link, Session};

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// A real Logon from the corpus with its one deliberately wrong field
/// corrected. Every case below changes exactly one more thing and says which.
fn good_logon() -> Vec<u8> {
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
    with_real_checksum(s.replace("56=DLSI", "56=ISLD").as_bytes())
}

/// Replace one substring and recompute `9=` and `10=`.
fn reframe(wire: &[u8], from: &str, to: &str) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    let patched = s.replace(from, to);
    assert_ne!(patched, s, "{from} is not in the message");
    let after_9 = patched.find("\u{1}35=").expect("35= follows the frame") + 1;
    let at_10 = patched.find("\u{1}10=").map_or(patched.len(), |i| i + 1);
    let head_end = patched.find('\u{1}').expect("8= is a field") + 1;
    let body = at_10 - after_9;
    let rebuilt = format!(
        "{}9={body}\u{1}{}10=0\u{1}",
        &patched[..head_end],
        &patched[after_9..at_10]
    );
    with_real_checksum(rebuilt.as_bytes())
}

/// Everything one end can observe about the other's failure.
///
/// **Step 1 was `(Link, Vec<String>)`** — the link and the bytes, which was
/// genuinely all there was, and which made the two assertions below fail
/// because every fault looked the same. Step 2 adds the third element. The
/// tests did not change; what they can see did.
fn observable(cfg: Config, now_ms: u64, wire: &[u8]) -> (Link, Vec<String>, Option<DropReason>) {
    let mut s: Session<Acceptor, 256> = Session::new(cfg);
    let mut out = Vec::new();
    s.connect(|b| out.push(String::from_utf8_lossy(b).replace('\u{1}', "|")));
    s.tick(now_ms, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"))
    });
    let link = s.received(wire, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    (link, out, s.last_drop_reason())
}

/// **The specification.** Two connections, two genuinely different faults, and
/// an operator must be able to tell them apart.
///
/// A wrong `BeginString` means somebody is configured for the wrong FIX
/// version. A wrong `SenderCompID` means somebody is pointed at the wrong
/// counterparty. Different people fix them, on different days. `[verified
/// 2026-09-02]` the engine says exactly the same thing about both.
#[test]
fn two_connections_that_end_for_different_reasons_are_distinguishable() {
    let a = observable(
        cfg(),
        FIXED_TIME_MILLIS,
        &reframe(&good_logon(), "8=FIX.4.4", "8=FIX.4.2"),
    );
    let b = observable(
        cfg(),
        FIXED_TIME_MILLIS,
        &reframe(&good_logon(), "49=TW44", "49=NOPE"),
    );

    assert_eq!(a.0, Link::Dropped, "a wrong BeginString ends it");
    assert_eq!(b.0, Link::Dropped, "so does a wrong SenderCompID");
    assert_ne!(
        a, b,
        "and they are different faults with different fixes, so nothing that \
         reports on this session may describe them identically"
    );
}

/// **The one that has already cost hours this week.** A clock too far out and
/// an hour outside the schedule are different problems with different fixes —
/// one is your NTP, the other is a venue calendar — and on the wire both are
/// silence.
#[test]
fn a_clock_fault_and_a_calendar_fault_are_distinguishable() {
    use fixbolt_session::schedule::Schedule;

    let skewed = observable(cfg(), FIXED_TIME_MILLIS + 10 * 60 * 1_000, &good_logon());
    let hours = Schedule::daily(8 * 3_600, 17 * 3_600).expect("legal");
    let shut = observable(
        cfg().with_schedule(hours),
        FIXED_TIME_MILLIS - 9 * 3_600_000,
        &good_logon(),
    );

    assert_eq!(skewed.0, Link::Dropped, "a ten-minute skew ends it");
    assert_eq!(shut.0, Link::Dropped, "so does 3 a.m.");
    assert_ne!(
        skewed, shut,
        "check your NTP is not the same advice as check your venue calendar"
    );
}

/// The control: a connection that did **not** end is distinguishable from both,
/// which is what says this harness can see any difference at all.
#[test]
fn a_session_that_stays_up_is_distinguishable_from_one_that_does_not() {
    let good = observable(cfg(), FIXED_TIME_MILLIS, &good_logon());
    let bad = observable(
        cfg(),
        FIXED_TIME_MILLIS,
        &reframe(&good_logon(), "49=TW44", "49=NOPE"),
    );

    assert_eq!(good.0, Link::Up, "the premise: a good Logon is accepted");
    assert_ne!(
        good, bad,
        "and the harness can see a difference when there is one"
    );
}

// --- step 2: each reason names itself ------------------------------------

/// Drive one session and return only the reason.
fn reason_of(cfg: Config, now_ms: u64, wire: &[u8]) -> Option<DropReason> {
    observable(cfg, now_ms, wire).2
}

/// **One case per reason.** A single `assert_ne!` between two of them would
/// pass for an enum with two variants and eighteen call sites, so each fault is
/// named rather than merely distinguished.
#[test]
fn every_pre_session_fault_names_itself() {
    let cases: &[(&str, &str, DropReason)] = &[
        ("8=FIX.4.4", "8=FIX.4.2", DropReason::WrongBeginString),
        ("49=TW44", "49=NOPE", DropReason::WrongSenderCompId),
        ("56=ISLD", "56=NOPE", DropReason::WrongTargetCompId),
        ("35=A", "35=0", DropReason::NotALogon),
    ];
    for (from, to, want) in cases {
        let wire = reframe(&good_logon(), from, to);
        let (link, _, why) = observable(cfg(), FIXED_TIME_MILLIS, &wire);
        assert_eq!(link, Link::Dropped, "{from} -> {to} must end the link");
        assert_eq!(why, Some(*want), "{from} -> {to} reads as the wrong reason");
    }
}

/// The clock and the calendar, by name — the pair whose confusion cost hours.
#[test]
fn the_clock_and_the_calendar_are_named_apart() {
    use fixbolt_session::schedule::Schedule;

    assert_eq!(
        reason_of(cfg(), FIXED_TIME_MILLIS + 10 * 60 * 1_000, &good_logon()),
        Some(DropReason::SendingTimeOutOfRange),
        "ten minutes of skew is an NTP problem"
    );
    let hours = Schedule::daily(8 * 3_600, 17 * 3_600).expect("legal");
    assert_eq!(
        reason_of(
            cfg().with_schedule(hours),
            FIXED_TIME_MILLIS - 9 * 3_600_000,
            &good_logon()
        ),
        Some(DropReason::OutsideSchedule),
        "3 a.m. is a venue calendar problem"
    );
}

/// A session that is up has nothing to explain, and a **stale** reason read as
/// a current one is worse than none.
#[test]
fn a_live_session_reports_no_reason() {
    let (link, _, why) = observable(cfg(), FIXED_TIME_MILLIS, &good_logon());
    assert_eq!(link, Link::Up, "the premise: a good Logon is accepted");
    assert_eq!(why, None);
}

/// **The reason is the latest one.** A field written after the state change
/// rather than before would report the previous connection's cause.
#[test]
fn a_second_fault_replaces_the_first() {
    let mut s: Session<Acceptor, 256> = Session::new(cfg());
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let _ = s.received(&reframe(&good_logon(), "8=FIX.4.4", "8=FIX.4.2"), |_| {});
    assert_eq!(s.last_drop_reason(), Some(DropReason::WrongBeginString));

    s.connect(|_| {});
    assert_eq!(
        s.last_drop_reason(),
        None,
        "connect clears it — a live session has nothing to explain"
    );
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let _ = s.received(&reframe(&good_logon(), "49=TW44", "49=NOPE"), |_| {});
    assert_eq!(
        s.last_drop_reason(),
        Some(DropReason::WrongSenderCompId),
        "the newest cause, not the one before it"
    );
}

/// Two reasons the pre-session path never reaches: a heartbeat that timed out,
/// and a counterparty that said `Logout`. **They come from `tick` and from the
/// logged-on path**, which is a different funnel — and a version that recorded
/// only at the refusal funnel would pass every test above.
#[test]
fn a_timeout_and_a_peer_logout_are_named_too() {
    let mut s: Session<Acceptor, 256> = Session::new(cfg());
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    assert_eq!(s.received(&good_logon(), |_| {}), Link::Up, "logged on");
    // 108=30, and the session gives up at 2.4 intervals of silence.
    assert_eq!(
        s.tick(FIXED_TIME_MILLIS + 200_000, |_| {}),
        Link::Dropped,
        "nothing arrived for long enough"
    );
    assert_eq!(s.last_drop_reason(), Some(DropReason::HeartbeatTimeout));

    let mut t: Session<Acceptor, 256> = Session::new(cfg());
    t.connect(|_| {});
    t.tick(FIXED_TIME_MILLIS, |_| {});
    assert_eq!(t.received(&good_logon(), |_| {}), Link::Up, "logged on");
    // A real Logout, not a Logon wearing `35=5`: `98=` and `108=` are not
    // defined for it, and leaving them on gets a Reject rather than a goodbye.
    let logout = reframe(
        &reframe(
            &reframe(&good_logon(), "35=A", "35=5"),
            "98=0\u{1}108=30\u{1}",
            "",
        ),
        "34=1",
        "34=2",
    );
    assert_eq!(
        t.received(&logout, |_| {}),
        Link::Dropped,
        "they said goodbye"
    );
    assert_eq!(t.last_drop_reason(), Some(DropReason::PeerLogout));
}
