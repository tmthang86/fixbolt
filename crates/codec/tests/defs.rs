//! Every `I` and `E` line in the 59 acceptance definitions, through the parser.
//!
//! The expectations below are declared **by file name and line**, from reading
//! the definitions, and each carries the reason. They are not a success counter:
//! a counter goes red on day one and the reflex is to loosen it, which is the
//! trap `CLAUDE.md` §10 names. Rejecting in the right place is green; rejecting
//! in the wrong place, or accepting in the wrong place, is red.
#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

mod common;

use common::Direction;
use nanofix_codec::{FieldIndex, NoDict, Parsed, Validation, parse_into};

/// Lines the parser must refuse, and why.
///
/// Every one of these is silently dropped by QuickFIX except the last, whose
/// definition says the sequence number is consumed and a Reject sent — which is
/// why `BadTag` keeps the index and carries an offset.
const MUST_REFUSE: &[(&str, usize, &str, &str)] = &[
    (
        "14a_BadField.def",
        25,
        "BadTag",
        "-1=HI — a tag that is not a u32. Session still answers; see BadTag's docs",
    ),
    (
        "2d_GarbledMessage.def",
        8,
        "BadTag",
        "4garbled9=TW — tag is not a number",
    ),
    (
        "2d_GarbledMessage.def",
        13,
        "BadTag",
        "49garbled=TW — tag is not a number",
    ),
    (
        "2t_FirstThreeFieldsOutOfOrder.def",
        8,
        "BadFrameStart",
        "35=0 before 8= — the frame cannot be delimited at all",
    ),
    (
        "3c_GarbledMessage.def",
        8,
        "BadTag",
        "4garbled9=TW — tag is not a number",
    ),
    (
        "3c_GarbledMessage.def",
        13,
        "BadTag",
        "49garbled=TW — tag is not a number",
    ),
];

/// Lines whose `9=` disagrees with their own body. Three are deliberate; three
/// are QuickFIX's own expected-output lines carrying a stale length.
const BAD_BODY_LENGTH: &[(&str, usize, &str)] = &[
    (
        "14e_IncorrectEnumValue.def",
        26,
        "E line: 9=121, body is 117 — stale by 4",
    ),
    (
        "1d_InvalidLogonLengthInvalid.def",
        4,
        "deliberate: 9=40 on a shorter body",
    ),
    (
        "2m_BodyLengthValueNotCorrect.def",
        8,
        "deliberate: 9=30, too short",
    ),
    (
        "2m_BodyLengthValueNotCorrect.def",
        22,
        "deliberate: 9=111, too long",
    ),
    (
        "8_OnlyApplicationMessages.def",
        29,
        "E line: 9=93, body is 89 — stale by 4",
    ),
    (
        "RejectResentMessage.def",
        6,
        "E line: 9=63, body is 59 — stale by 4",
    ),
];

fn kind(e: &nanofix_codec::ParseError) -> String {
    let s = format!("{e:?}");
    s.split(['(', ' ', '{']).next().unwrap_or("").to_string()
}

#[test]
fn every_line_is_classified() {
    let lines = common::load_all();
    assert_eq!(lines.len(), 539, "539 I and E lines across the 59 files");
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.direction == Direction::In)
            .count(),
        289
    );
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.direction == Direction::Expect)
            .count(),
        250
    );
    assert_eq!(
        lines.iter().filter(|l| l.session.is_some()).count(),
        8,
        "8 lines carry an I1,/E1, session prefix; feeding it to the parser is a BadTag"
    );

    let mut idx: FieldIndex<64> = FieldIndex::new();
    let mut ok = 0usize;
    let mut refused: Vec<(String, usize, String)> = Vec::new();

    for l in &lines {
        // Structure only. Frame-level validation is exercised separately, because
        // the corpus's 10= values are placeholders — see the checksum test.
        match parse_into::<NoDict, 64>(&l.wire, &mut idx, Validation::NONE) {
            Ok(Parsed::Complete { consumed }) => {
                assert_eq!(
                    consumed,
                    l.wire.len(),
                    "{}:{} parsed but left {} trailing bytes",
                    l.file,
                    l.line_no,
                    l.wire.len() - consumed
                );
                ok += 1;
            }
            Ok(Parsed::Incomplete) => panic!(
                "{}:{} is a whole message and must not be Incomplete",
                l.file, l.line_no
            ),
            Err(e) => refused.push((l.file.clone(), l.line_no, kind(&e))),
        }
    }

    let expected: Vec<(String, usize, String)> = MUST_REFUSE
        .iter()
        .map(|(f, n, k, _)| ((*f).to_string(), *n, (*k).to_string()))
        .collect();
    assert_eq!(
        refused, expected,
        "\nthe parser refused a different set of lines than the definitions justify"
    );
    assert_eq!(ok, 539 - MUST_REFUSE.len());
    println!("classified {}/{}", ok + refused.len(), lines.len());
}

#[test]
fn body_length_is_checked_where_the_corpus_declares_one() {
    let lines = common::load_all();
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let v = Validation {
        body_length: true,
        check_sum: false,
    };
    let mut checked = 0usize;
    let mut wrong: Vec<(String, usize)> = Vec::new();

    for l in &lines {
        if !l.had_body_length {
            continue; // the loader computed it; that is the checksum test's job
        }
        if MUST_REFUSE
            .iter()
            .any(|(f, n, _, _)| *f == l.file && *n == l.line_no)
        {
            continue; // refused before 9= is ever compared
        }
        checked += 1;
        if parse_into::<NoDict, 64>(&l.wire, &mut idx, v)
            == Err(nanofix_codec::ParseError::BadBodyLength)
        {
            wrong.push((l.file.clone(), l.line_no));
        }
    }

    let expected: Vec<(String, usize)> = BAD_BODY_LENGTH
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), *n))
        .collect();
    assert_eq!(wrong, expected, "\nunexpected set of bad body lengths");
    println!(
        "body length: {} lines carry their own 9=, {} disagree with their body",
        checked,
        wrong.len()
    );
}

#[test]
fn the_corpus_checksums_are_placeholders_not_checksums() {
    // 244 lines carry 10=. Not one of them is the real checksum of its own bytes:
    // 238 are literally `10=0`. The QuickFIX comparator matches tag 10 by regex,
    // so the value never had to be real. A conformance runner that checksum-
    // validates expected output will fail all 244 and learn nothing.
    let lines = common::load_all();
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let v = Validation {
        body_length: false,
        check_sum: true,
    };
    let (mut carried, mut real) = (0usize, 0usize);

    for l in &lines {
        if !l.had_checksum {
            continue;
        }
        if MUST_REFUSE
            .iter()
            .any(|(f, n, _, _)| *f == l.file && *n == l.line_no)
        {
            continue;
        }
        carried += 1;
        if parse_into::<NoDict, 64>(&l.wire, &mut idx, v).is_ok() {
            real += 1;
        }
    }
    // 251 lines carry 10=. Five of the six refused lines carry one too — every
    // garbled line and the out-of-order one — so 246 reach this comparison.
    assert_eq!(
        carried, 246,
        "lines carrying their own 10=, minus the refused ones"
    );
    assert_eq!(
        real, 0,
        "if this is no longer 0 the corpus changed — re-read reference/quickfix-acceptance-def-format.md"
    );
    println!("checksum: {carried} lines carry 10=, {real} of them are real checksums");
}

#[test]
fn computed_checksums_round_trip() {
    // Where the loader supplied 10=, the parser must agree with it. Two
    // independent implementations of the same sum, over 288 real messages.
    let lines = common::load_all();
    let mut idx: FieldIndex<64> = FieldIndex::new();
    // Checksum only. Validation::ALL would also compare 9=, and
    // 1d_InvalidLogonLengthInvalid ships a deliberately wrong 9=40 on a line
    // with no 10= — so the loader computes a correct checksum over bytes whose
    // declared body length is a lie. That line belongs to the body-length test.
    let v = Validation {
        body_length: false,
        check_sum: true,
    };
    let mut n = 0usize;
    for l in &lines {
        if l.had_checksum {
            continue;
        }
        if MUST_REFUSE
            .iter()
            .any(|(f, no, _, _)| *f == l.file && *no == l.line_no)
        {
            continue;
        }
        let r = parse_into::<NoDict, 64>(&l.wire, &mut idx, v);
        assert!(
            r == Ok(Parsed::Complete {
                consumed: l.wire.len()
            }),
            "{}:{} the loader computed this 10=, so the parser must agree: {r:?}",
            l.file,
            l.line_no
        );
        n += 1;
    }
    // 539 total - 251 that carry their own 10= - 14a:25, the one refused line
    // that does not carry one.
    assert_eq!(n, 287);
    println!("computed 9= and 10= verified on {n} messages");
}
