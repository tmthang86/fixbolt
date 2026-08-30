//! The mirrored corpus: the same 59 files read from the other side.
//!
//! `ADR-0004` decision 6 gives a mechanical criterion rather than a list, and
//! `ADR-0006` adds the clause it was missing: 50 of the 59 mirror. This asserts
//! the criterion **produces** the list — a hand-copied list of nine names is
//! how a corpus change goes by unnoticed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nanofix_conformance::runner::{NullSession, Replay, run_mirrored};
use nanofix_conformance::script::{Kind, mirrors, scenarios, scenarios_mirrored};

/// The nine names the ADRs give, and the reason each is there.
const CANNOT_MIRROR: [&str; 9] = [
    // A tag that is not a number. No initiator sends one.
    "14a_BadField.def",
    "2d_GarbledMessage.def",
    "3c_GarbledMessage.def",
    // A field with no value.
    "14d_TagSpecifiedWithoutValue.def",
    "ReverseRouteWithEmptyRoutingTags.def",
    // A `BeginString` this engine does not speak.
    "1d_InvalidLogonWrongBeginString.def",
    "2i_BeginStringValueUnexpected.def",
    // `35=` before `8=`.
    "2t_FirstThreeFieldsOutOfOrder.def",
    // Mirrored, it asks this engine to hang up a connection nothing told it to
    // hang up. `ADR-0006` — the clause `ADR-0004` decision 6 was missing.
    "1b_DuplicateIdentity.def",
];

#[test]
fn the_nine_that_cannot_mirror_are_the_nine_the_adrs_name() {
    let all = scenarios_mirrored().unwrap_or_else(|e| panic!("{e}"));
    let mut refused: Vec<&str> = all
        .iter()
        .filter(|s| !mirrors(s))
        .map(|s| s.file.as_str())
        .collect();
    refused.sort_unstable();

    let mut expected = CANNOT_MIRROR;
    expected.sort_unstable();

    assert_eq!(
        refused, expected,
        "the criterion must produce the ADR's list, not agree with it by luck"
    );
    assert_eq!(all.len() - refused.len(), 50, "50 of 59 — `ADR-0006`");
}

/// Mirroring swaps who speaks, and nothing else.
#[test]
fn every_line_keeps_its_place_and_changes_its_side() {
    let plain = scenarios().unwrap_or_else(|e| panic!("{e}"));
    let mirrored = scenarios_mirrored().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(plain.len(), mirrored.len());

    for (a, b) in plain.iter().zip(&mirrored) {
        assert_eq!(a.file, b.file);
        assert_eq!(a.steps.len(), b.steps.len(), "{}", a.file);
        for (x, y) in a.steps.iter().zip(&b.steps) {
            assert_eq!(x.line_no, y.line_no, "{}", a.file);
            let swapped = matches!(
                (&x.kind, &y.kind),
                (Kind::Send(_), Kind::Expect(_))
                    | (Kind::Expect(_), Kind::Send(_))
                    | (Kind::Disconnect, Kind::ExpectDisconnect)
                    | (Kind::ExpectDisconnect, Kind::Disconnect)
                    | (Kind::Connect, Kind::Connect)
            );
            assert!(
                swapped,
                "{}:{} {:?} -> {:?}",
                a.file, x.line_no, x.kind, y.kind
            );
        }
    }
}

/// Both sides of a mirrored file write milliseconds.
///
/// The plain corpus gives an `I` line a 17-byte `<TIME>` because the reflector
/// writes it and QuickFIX's reflector does not write milliseconds. Mirrored,
/// **this engine** writes that line, and it writes 21 — so the `9=` the line
/// is compared against has to be computed over 21 bytes or every message in
/// the suite is four bytes out for a reason that has nothing to do with the
/// session layer.
#[test]
fn a_line_this_engine_will_write_declares_a_length_that_counts_milliseconds() {
    let mirrored = scenarios_mirrored().unwrap_or_else(|e| panic!("{e}"));
    let logon = mirrored
        .iter()
        .find(|s| s.file == "1a_ValidLogonWithCorrectMsgSeqNum.def")
        .expect("in the corpus")
        .steps
        .iter()
        .find_map(|s| match &s.kind {
            // The file's first `I` line, which mirrored is this engine's Logon.
            Kind::Expect(m) => Some(m.wire.clone()),
            _ => None,
        })
        .expect("a Logon to send");

    let text = String::from_utf8(logon).expect("ascii");
    assert!(
        text.contains("52=20260828-12:00:00.000\u{1}"),
        "milliseconds, because this engine writes them: {text:?}"
    );
    assert!(
        text.contains("\u{1}9=63\u{1}"),
        "and the declared length counts them: {text:?}"
    );
}

/// `0 / 51` is not evidence, mirrored either.
///
/// The same argument as `tests/fix44.rs`: a mirrored runner that runs nothing
/// and a comparator that always passes both report zero. [`Replay`] answering
/// with each file's own `I` lines is what says the mirrored side can tell right
/// from wrong.
#[test]
fn the_mirrored_runner_can_tell_right_from_wrong() {
    let none = run_mirrored(|_| NullSession).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(none.scenarios, 50, "50 files, not 59:\n{none}");
    assert_eq!(none.passed, 0, "a silent initiator sends nothing:\n{none}");

    let all = run_mirrored(Replay::new).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        all.passed, 50,
        "a fake that replays each file's own `I` lines:\n{all}"
    );
}
