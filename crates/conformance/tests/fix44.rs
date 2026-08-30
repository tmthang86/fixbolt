//! The runner, against all 59 definitions.
//!
//! **`0 / 59` is not evidence.** A comparator that always passes and a runner
//! that runs nothing both report `0 / 59` while no session exists, and neither
//! would say so. So this file drives two fakes:
//!
//! * [`NullSession`] answers nothing — the real score today, `0 / 59`;
//! * [`Replay`] answers with exactly what the file expects — `59 / 59`.
//!
//! Only the second one proves the runner works. It is the whole reason step 4
//! of the plan is not "it printed zero".
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::runner::{NullSession, Replay, run};

#[test]
fn a_session_that_answers_nothing_scores_zero() {
    let report = run(|_| NullSession).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.scenarios, 59);
    assert_eq!(report.passed, 0, "no session exists yet:\n{report}");
    // Every file expects at least one thing, so every file has a reason.
    assert_eq!(report.failed_scenarios(), 59);
    assert!(
        report.failures.len() >= 59,
        "each failure names a file and a line"
    );
}

#[test]
fn a_session_that_answers_correctly_scores_fifty_nine() {
    // The gate on the runner itself. If this is not 59/59 the runner is broken,
    // and the 0/59 above means nothing.
    let report = run(Replay::new).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.passed, 59,
        "the runner cannot recognise a correct answer:\n{report}"
    );
    assert!(report.failures.is_empty());
}

#[test]
fn a_failure_names_the_file_and_the_line() {
    let report = run(|_| NullSession).unwrap_or_else(|e| panic!("{e}"));
    let f = report.failures.first().expect("at least one failure");
    assert!(f.file.ends_with(".def"), "{f:?}");
    assert!(f.line_no > 0, "{f:?}");
    // And the rendered report is readable rather than a byte dump.
    let text = report.to_string();
    assert!(text.contains(" / 59"), "{text}");
    assert!(text.contains(".def:"), "{text}");
}

/// The six definitions that expect no message at all — connect, send something
/// the acceptor must refuse, expect the link to drop. Nothing in them can be
/// corrupted because nothing in them is compared.
const NO_EXPECTED_MESSAGE: &[&str] = &[
    "1c_InvalidSenderCompID.def",
    "1c_InvalidTargetCompID.def",
    "1d_InvalidLogonBadSendingTime.def",
    "1d_InvalidLogonLengthInvalid.def",
    "1d_InvalidLogonWrongBeginString.def",
    "1e_NotLogonMessage.def",
];

#[test]
fn one_wrong_byte_is_enough_to_fail() {
    // Replay, but with one byte of one compared value changed in each file.
    // Every file that expects a message must notice.
    let report = run(|s| Replay::new(s).corrupt_first_expect()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.passed_files, NO_EXPECTED_MESSAGE,
        "only the files with nothing to compare may survive a corrupted answer"
    );
    assert_eq!(report.passed, 6);
}

#[test]
fn a_session_that_says_one_extra_thing_has_not_passed() {
    // Answering every line correctly is not enough. A session that also emits
    // a message no line asked for would otherwise score full marks, and a
    // spurious Heartbeat on the wire is a real failure mode.
    let report = run(|s| Replay::new(s).with_extra_output()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.passed, 0,
        "leftover output must fail every file:\n{report}"
    );
    assert!(
        report
            .failures
            .iter()
            .all(|f| matches!(f.reason, fixbolt_conformance::runner::Reason::Unexpected(_))),
        "and for that reason, not another"
    );
}

#[test]
fn six_definitions_expect_no_message_at_all() {
    // The other half of the claim above, read off the corpus rather than
    // assumed: these six are pass/fail on the disconnect alone.
    let empty: Vec<String> = fixbolt_conformance::script::scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .filter(|s| {
            !s.steps
                .iter()
                .any(|x| matches!(x.kind, fixbolt_conformance::script::Kind::Expect(_)))
        })
        .map(|s| s.file)
        .collect();
    assert_eq!(empty, NO_EXPECTED_MESSAGE);
}

#[test]
fn the_two_multi_session_files_run_as_one_engine() {
    // 1b_DuplicateIdentity rejects the second Logon BECAUSE the first
    // connection exists. A runner that hands each connection its own session
    // object can never let that test pass, so the trait is per engine and the
    // connection is a parameter.
    let report = run(Replay::new).unwrap_or_else(|e| panic!("{e}"));
    for f in ["1b_DuplicateIdentity.def", "AlreadyLoggedOn.def"] {
        assert!(
            report.passed_files.iter().any(|p| p == f),
            "{f} did not pass:\n{report}"
        );
    }
}
