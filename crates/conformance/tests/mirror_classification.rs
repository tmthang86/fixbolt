//! What each mirrored definition would cost, and what that makes the ceiling.
//!
//! `ADR-0006` estimated 45. `ADR-0076` withdraws the estimate: the ceiling is
//! the number of files whose every `I` line, mirrored, is something the
//! initiator's public API can be asked to do, and it is
//! `fixbolt_conformance::mirror::CLASSIFICATION` that produces it. These are
//! the three tests `ADR-0076` decision 2 names, plus the one that says the
//! table is about the right 50 files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::mirror::{
    CLASSIFICATION, MirrorClass, an_output_line_has, ceiling, class_of, files_in, mirrorable,
};

/// The table is about these files and no others.
#[test]
fn the_table_is_exactly_the_fifty_that_mirror() {
    let corpus = mirrorable().unwrap_or_else(|e| panic!("{e}"));
    let mut in_corpus: Vec<&str> = corpus.iter().map(|s| s.file.as_str()).collect();
    in_corpus.sort_unstable();
    let mut in_table: Vec<&str> = CLASSIFICATION.iter().map(|r| r.file).collect();
    in_table.sort_unstable();
    assert_eq!(
        in_table, in_corpus,
        "a row per mirrorable file, and nothing invented"
    );
}

/// `ADR-0076` decision 2, first test: **`Unclassified` is empty.**
#[test]
fn no_definition_is_unclassified() {
    let left = files_in(MirrorClass::Unclassified);
    assert!(
        left.is_empty(),
        "every mirrored definition gets a class — ADR-0076 decision 2; not classified: {left:?}"
    );
}

/// `ADR-0076` decision 2, second test: the ceiling **is** the `Reachable`
/// count, and it is the number the mirrored score gate compares against.
///
/// The literal is here so the ceiling cannot move by accident: a row that
/// changes class changes this number, and changing it is a commit that says so.
/// `[measured 2026-09-18]` **14**, against `ADR-0006`'s estimate of 45.
#[test]
fn the_ceiling_is_the_reachable_count() {
    let reachable = files_in(MirrorClass::Reachable);
    assert_eq!(ceiling(), reachable.len(), "one definition of the ceiling");
    assert_eq!(
        ceiling(),
        14,
        "the mirrored ceiling, ADR-0076 — reachable: {reachable:?}"
    );
    // Every file the gate passes today must be one the table calls reachable,
    // or the table and the score disagree about the same file.
    for passing in [
        "13b_UnsolicitedLogoutMessage.def",
        "1a_ValidLogonWithCorrectMsgSeqNum.def",
        "2a_MsgSeqNumCorrect.def",
        "2k_CompIDDoesNotMatchProfile.def",
        "2o_SendingTimeValueOutOfRange.def",
        "2q_MsgTypeNotValid.def",
        "4a_NoDataSentDuringHeartBtInt.def",
        "4b_ReceivedTestRequest.def",
        "AlreadyLoggedOn.def",
        "ReverseRoute.def",
    ] {
        assert_eq!(
            class_of(passing),
            Some(MirrorClass::Reachable),
            "{passing} passes the mirrored gate, so the table may not refuse it"
        );
    }
}

/// `ADR-0076` decision 2, third test: the two classes that name bytes are
/// checked **against the files**.
///
/// Every `NeedsUnsequencedReset` file really has an `I` line with `34=0`, and
/// every `NeedsGapFillAsAction` one really has `123=Y`. A row classified from
/// the file's *name* fails here.
#[test]
fn the_byte_classes_are_read_from_the_definitions_not_the_table() {
    let corpus = mirrorable().unwrap_or_else(|e| panic!("{e}"));
    let find = |name: &str| {
        corpus
            .iter()
            .find(|s| s.file == name)
            .unwrap_or_else(|| panic!("{name} is not in the mirrored corpus"))
    };

    let unsequenced = files_in(MirrorClass::NeedsUnsequencedReset);
    assert!(!unsequenced.is_empty(), "the class is not empty");
    for f in &unsequenced {
        let s = find(f);
        assert!(
            an_output_line_has(s, "35", "4") && an_output_line_has(s, "34", "0"),
            "{f} is classified NeedsUnsequencedReset but has no SequenceReset with 34=0"
        );
    }

    let gap_fill = files_in(MirrorClass::NeedsGapFillAsAction);
    assert!(!gap_fill.is_empty(), "the class is not empty");
    for f in &gap_fill {
        let s = find(f);
        assert!(
            an_output_line_has(s, "123", "Y"),
            "{f} is classified NeedsGapFillAsAction but has no 123=Y to send"
        );
    }

    // And the other way round: a file that asks for `34=0` may not be called
    // anything else.
    for r in &CLASSIFICATION {
        let s = find(r.file);
        if an_output_line_has(s, "35", "4") && an_output_line_has(s, "34", "0") {
            assert_eq!(
                r.class,
                MirrorClass::NeedsUnsequencedReset,
                "{} asks for 34=0 and is classified {:?}",
                r.file,
                r.class
            );
        }
    }
}
