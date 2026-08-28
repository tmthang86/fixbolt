//! The score. This is the gate, and it is the only one that matters.
//!
//! `CLAUDE.md` §2 non-negotiable 3: a session change that has not run the 59
//! definitions is not done. Every step of
//! `docs/plans/2026-08-28-session-layer.md` predicts a number here, and a step
//! that misses its prediction — **or beats it** — stops until the difference is
//! understood.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nanofix_conformance::runner::{Conn, Input, Link, SessionUnderTest, run};
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

/// `[measured 2026-08-28]` **14 / 59** — and the plan predicted 18.
///
/// The prediction came from a classification table that said 12 files expect
/// only `{A, 5}` back. Solving it off the corpus instead gives **9**, and two of
/// the reachable set need something this step does not have:
/// `AlreadyLoggedOn.def` and `1b_DuplicateIdentity.def` both turn on refusing a
/// second connection with the same identity, which is step 6. So the ceiling
/// for step 2 is `6 + 9 − 1 = 14`, and 14 is what it scores.
///
/// The two it lost on the way are worth naming, because both had been passing
/// **by accident** and a step that only counts upwards would have hidden it:
/// before `connect` reset the sequence numbers, the second Logon in each file
/// was refused as *too low* rather than as a duplicate identity.
#[test]
fn step_two_answers_a_logon_and_a_logout_and_scores_fourteen() {
    let report = run(|_| acceptor()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.passed, 14,
        "step 2 of the plan, revised from 18 to 14 against the corpus:\n{report}"
    );
    assert_eq!(
        report.passed_files,
        vec![
            "13b_UnsolicitedLogoutMessage.def",
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
            "7_ReceiveRejectMessage.def",
        ],
        "and these are the fourteen, named — a different fourteen is a different result"
    );
}

/// The step-1 six are still in the fourteen.
///
/// Not implied by the count: step 2 added eight and could have lost one of the
/// six to a rule that now fires earlier. This is the assertion that says it did
/// not.
#[test]
fn step_two_did_not_cost_any_of_the_step_one_six() {
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
