//! The score. This is the gate, and it is the only one that matters.
//!
//! `CLAUDE.md` §2 non-negotiable 3: a session change that has not run the 59
//! definitions is not done. Every step of
//! `docs/plans/2026-08-28-session-layer.md` predicts a number here, and a step
//! that misses its prediction — **or beats it** — stops until the difference is
//! understood.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nanofix_conformance::runner::{Conn, Input, Link, SessionUnderTest, run};
use nanofix_conformance::script::FIXED_TIME_MILLIS;
use nanofix_session::{Acceptor, Config, Session};

/// `session` cannot depend on `conformance` — that is the dev-dependency
/// direction, and reversing it is a cycle. So the two crates each own a `Link`
/// and this maps between them. Two names for one idea, and the alternative is
/// worse.
fn link(l: nanofix_session::Link) -> Link {
    match l {
        nanofix_session::Link::Up => Link::Up,
        nanofix_session::Link::Dropped => Link::Dropped,
    }
}

/// The orphan rule: `SessionUnderTest` belongs to `conformance` and `Session`
/// to `session`, so neither is local here. A local wrapper is the whole reason
/// this type exists.
struct Adapter(Session<Acceptor, 256>);

impl SessionUnderTest for Adapter {
    fn step<F: FnMut(&[u8])>(&mut self, _conn: Conn, input: Input<'_>, emit: F) -> Link {
        link(match input {
            Input::Connect => self.0.connect(emit),
            Input::Disconnect => self.0.disconnect(emit),
            Input::Bytes(b) => self.0.received(b, emit),
            Input::Tick(ms) => self.0.tick(ms, emit),
        })
    }
}

/// The acceptor the corpus talks to: it is ISLD and its counterparty is TW44.
fn acceptor() -> Adapter {
    Adapter(Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")))
}

/// The two crates agree on what time it is.
///
/// `conformance` states the corpus's instant as a number because the runner
/// needs one to tick with, and it has no timestamp parser to check it against.
/// `session` has the parser. Neither crate can prove this alone; this is the
/// only place that sees both.
#[test]
fn the_harness_clock_and_the_corpus_agree() {
    use nanofix_conformance::script::{FIXED_TIME_IN, FIXED_TIME_MILLIS, FIXED_TIME_OUT};

    assert_eq!(
        nanofix_session::clock::parse_utc(FIXED_TIME_IN.as_bytes()),
        Some(FIXED_TIME_MILLIS),
        "the runner would tick to an instant the corpus never writes"
    );
    assert_eq!(
        nanofix_session::clock::parse_utc(FIXED_TIME_OUT.as_bytes()),
        Some(FIXED_TIME_MILLIS),
        "the two widths must name the same instant"
    );
}

/// `[measured 2026-08-28]` **27 / 59** — step 3, and the revised prediction was
/// 27.
///
/// Step 3 is `Reject (35=3)`: all thirteen files whose expected output is
/// exactly `{A, 5, 3}`. Twelve `373` codes, eight of which are questions for
/// the dictionary rather than rules of the session — which is why
/// `docs/plans/2026-08-28-dict-validation.md` had to be written and closed
/// before this step could start.
///
/// The two files that still fail with a `3` in them — `14e_IncorrectEnumValue`
/// and `21_RepeatingGroupSpecifierWithValueOfZero` — also expect an application
/// message echoed back, and that is step 6.
#[test]
fn step_three_rejects_and_scores_twenty_seven() {
    let report = run(|_| acceptor()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.passed, 27,
        "step 3 of the plan predicts 27 / 59:\n{report}"
    );
    assert_eq!(
        report.passed_files,
        vec![
            "13b_UnsolicitedLogoutMessage.def",
            "14a_BadField.def",
            "14b_RequiredFieldMissing.def",
            "14c_TagNotDefinedForMsgType.def",
            "14d_TagSpecifiedWithoutValue.def",
            "14f_IncorrectDataFormat.def",
            "14g_HeaderBodyTrailerFieldsOutOfOrder.def",
            "14h_RepeatedTag.def",
            "14i_RepeatingGroupCountNotEqual.def",
            "1a_ValidLogonWithCorrectMsgSeqNum.def",
            "1c_InvalidSenderCompID.def",
            "1c_InvalidTargetCompID.def",
            "1d_InvalidLogonBadSendingTime.def",
            "1d_InvalidLogonLengthInvalid.def",
            "1d_InvalidLogonWrongBeginString.def",
            "1e_NotLogonMessage.def",
            "2a_MsgSeqNumCorrect.def",
            "2c_MsgSeqNumTooLow.def",
            "2e_PossDupAlreadyReceived.def",
            "2e_PossDupNotReceived.def",
            "2i_BeginStringValueUnexpected.def",
            "2k_CompIDDoesNotMatchProfile.def",
            "2o_SendingTimeValueOutOfRange.def",
            "2q_MsgTypeNotValid.def",
            "7_ReceiveRejectMessage.def",
            "ReverseRoute.def",
            "ReverseRouteWithEmptyRoutingTags.def",
        ],
        "and these are the twenty-seven, named"
    );
}

/// Every `373` code the corpus asks for is actually reached.
///
/// Not implied by the count: `14a` alone carries four cases and a session that
/// answered every one of them with the same code would still pass the file, so
/// long as the code happened to be `0`. This walks the corpus's own `E` lines
/// and checks each distinct `373` value is produced somewhere.
#[test]
fn all_twelve_session_reject_reasons_are_produced() {
    use nanofix_conformance::script::{Kind, scenarios};

    let mut wanted: Vec<u32> = Vec::new();
    for s in scenarios().unwrap_or_else(|e| panic!("{e}")) {
        for step in &s.steps {
            if let Kind::Expect(m) = &step.kind
                && let Some(code) = field(&m.wire, 373).and_then(|v| std::str::from_utf8(v).ok())
                && let Ok(n) = code.parse::<u32>()
                && !wanted.contains(&n)
            {
                wanted.push(n);
            }
        }
    }
    wanted.sort_unstable();
    assert_eq!(
        wanted,
        vec![0, 1, 2, 4, 5, 6, 9, 10, 11, 13, 14, 16],
        "the twelve 373 codes the corpus asks for"
    );

    let mut produced: Vec<u32> = Vec::new();
    for s in scenarios().unwrap_or_else(|e| panic!("{e}")) {
        let mut session = acceptor();
        let mut seen: Vec<u32> = Vec::new();
        for step in &s.steps {
            let conn = Conn(step.session.unwrap_or(1));
            let mut collect = |b: &[u8]| {
                if let Some(v) = field(b, 373)
                    && let Ok(n) = std::str::from_utf8(v).unwrap_or("x").parse::<u32>()
                {
                    seen.push(n);
                }
            };
            match &step.kind {
                Kind::Connect => {
                    session.step(conn, Input::Connect, &mut collect);
                    session.step(conn, Input::Tick(FIXED_TIME_MILLIS), &mut collect);
                }
                Kind::Disconnect => {
                    session.step(conn, Input::Disconnect, &mut collect);
                }
                Kind::Send(m) => {
                    session.step(conn, Input::Tick(FIXED_TIME_MILLIS), &mut collect);
                    session.step(conn, Input::Bytes(&m.wire), &mut collect);
                }
                Kind::Expect(_) | Kind::ExpectDisconnect => {}
            }
        }
        for n in seen {
            if !produced.contains(&n) {
                produced.push(n);
            }
        }
    }
    produced.sort_unstable();
    assert_eq!(
        produced, wanted,
        "every 373 code the corpus asks for must actually be produced"
    );
}

/// The value of one field, by tag.
fn field(wire: &[u8], tag: u32) -> Option<&[u8]> {
    let needle = format!("\u{1}{tag}=");
    let at = wire
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())?
        + needle.len();
    let end = wire[at..].iter().position(|&b| b == 1)? + at;
    Some(&wire[at..end])
}

/// The step-1 six are still in the fourteen.
/// The step-1 six are still in the twenty-seven.
///
/// Not implied by the count: each step adds files and could lose one to a rule
/// that now fires earlier. Step 2 did exactly that to two files, and only a
/// named list caught it.
#[test]
fn the_step_one_six_are_still_there() {
    let report = run(|_| acceptor()).unwrap_or_else(|e| panic!("{e}"));
    for f in [
        "1c_InvalidSenderCompID.def",
        "1c_InvalidTargetCompID.def",
        "1d_InvalidLogonBadSendingTime.def",
        "1d_InvalidLogonLengthInvalid.def",
        "1d_InvalidLogonWrongBeginString.def",
        "1e_NotLogonMessage.def",
    ] {
        assert!(
            report.passed_files.iter().any(|p| p == f),
            "{f} passed at step 1 and does not now:\n{report}"
        );
    }
}
