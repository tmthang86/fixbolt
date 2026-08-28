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

#[test]
fn step_one_refuses_a_bad_logon_and_scores_six() {
    let report = run(|_| acceptor()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.passed, 6,
        "step 1 of the plan predicts 6 / 59:\n{report}"
    );
    assert_eq!(
        report.passed_files,
        vec![
            "1c_InvalidSenderCompID.def",
            "1c_InvalidTargetCompID.def",
            "1d_InvalidLogonBadSendingTime.def",
            "1d_InvalidLogonLengthInvalid.def",
            "1d_InvalidLogonWrongBeginString.def",
            "1e_NotLogonMessage.def",
        ],
        "and these are the six, named — a different six scoring 6 is not the same result"
    );
}
