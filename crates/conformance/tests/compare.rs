//! The comparator, and the one rule that will cost time if it is not known.
//!
//! `Comparator.rb` compares **positionally**: field *i* of expected against
//! field *i* of received. A correct FIX message whose fields are in a different
//! order **fails**. So this comparator is also, silently, the thing that pins
//! the field ordering of every message the session layer will generate.
//!
//! Five tags are matched by shape instead of by value, and `9` is deliberately
//! not one of them — the expected `BodyLength` in each `.def` is a hard
//! assertion that the body is byte-for-byte the expected length.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::compare::{Mismatch, compare};
use fixbolt_conformance::script::{Step, scenarios};

fn w(s: &str) -> Vec<u8> {
    s.replace('|', "\x01").into_bytes()
}

const LOGON: &str =
    "8=FIX.4.4|9=62|35=A|34=1|49=ISLD|52=00000000-00:00:00.000|56=TW44|98=0|108=2|10=0|";

/// The corpus writes `10=0`. Anything playing the engine has to compute one.
fn engine_output(s: &str) -> Vec<u8> {
    fixbolt_conformance::script::with_real_checksum(&w(s))
}

#[test]
fn a_message_equals_itself() {
    assert_eq!(compare(&w(LOGON), &engine_output(LOGON)), Ok(()));
}

#[test]
fn an_expected_line_does_not_match_itself_and_that_is_the_rule_working() {
    // 238 of the corpus's 244 checksums are the literal `10=0` and 2 more are
    // `10=7` — 240 that are not three digits. Rule 4 matches the RECEIVED
    // value, and the corpus never plays the engine, so an E line compared with
    // itself must fail on tag 10.
    // A comparator where this passes has stopped checking tag 10.
    assert!(matches!(
        compare(&w(LOGON), &w(LOGON)),
        Err(Mismatch::Shape { tag: 10, .. })
    ));
}

#[test]
fn swapping_two_fields_fails() {
    // The mandatory reversal. A comparator that always passes would make the
    // whole runner report 0/59 for the wrong reason and nothing would say so.
    let swapped =
        "8=FIX.4.4|9=62|35=A|34=1|52=00000000-00:00:00.000|49=ISLD|56=TW44|98=0|108=2|10=0|";
    let r = compare(&w(LOGON), &engine_output(swapped));
    assert!(
        matches!(
            r,
            Err(Mismatch::Tag {
                at: 4,
                expected: 49,
                actual: 52
            })
        ),
        "expected a positional tag mismatch at field 4, got {r:?}"
    );
}

#[test]
fn an_extra_field_fails_even_if_every_other_field_matches() {
    let extra =
        "8=FIX.4.4|9=62|35=A|34=1|49=ISLD|52=00000000-00:00:00.000|56=TW44|98=0|108=2|141=Y|10=0|";
    assert!(matches!(
        compare(&w(LOGON), &engine_output(extra)),
        Err(Mismatch::FieldCount {
            expected: 10,
            actual: 11
        })
    ));
}

#[test]
fn the_five_loose_tags_are_matched_by_shape_not_by_value() {
    // A different but well-shaped SendingTime passes; the expected value is
    // ignored, exactly as fields.fmt says.
    let other =
        "8=FIX.4.4|9=62|35=A|34=1|49=ISLD|52=20260828-14:30:59.123|56=TW44|98=0|108=2|10=0|";
    assert_eq!(compare(&w(LOGON), &engine_output(other)), Ok(()));

    // Without milliseconds, too — half the corpus's 60= values have none.
    let no_ms = "8=FIX.4.4|9=62|35=A|34=1|49=ISLD|52=20260828-14:30:59|56=TW44|98=0|108=2|10=0|";
    assert_eq!(compare(&w(LOGON), &engine_output(no_ms)), Ok(()));

    // A checksum of the wrong width is not a checksum.
    let short_sum =
        "8=FIX.4.4|9=62|35=A|34=1|49=ISLD|52=00000000-00:00:00.000|56=TW44|98=0|108=2|10=7|";
    assert!(matches!(
        compare(&w(LOGON), &w(short_sum)),
        Err(Mismatch::Shape { tag: 10, .. })
    ));
}

#[test]
fn a_timestamp_that_is_not_a_timestamp_fails() {
    let junk = "8=FIX.4.4|9=62|35=A|34=1|49=ISLD|52=not-a-time|56=TW44|98=0|108=2|10=0|";
    assert!(matches!(
        compare(&w(LOGON), &engine_output(junk)),
        Err(Mismatch::Shape { tag: 52, .. })
    ));
}

#[test]
fn body_length_is_compared_exactly_and_that_is_deliberate() {
    // `9` is NOT in fields.fmt. Every expected BodyLength in the corpus is a
    // hard assertion that the body is byte-for-byte the right length, which is
    // what makes the ordering rule bite.
    let wrong_len =
        "8=FIX.4.4|9=63|35=A|34=1|49=ISLD|52=00000000-00:00:00.000|56=TW44|98=0|108=2|10=0|";
    assert!(matches!(
        compare(&w(LOGON), &engine_output(wrong_len)),
        Err(Mismatch::Value { tag: 9, .. })
    ));
}

// ---- against the real corpus --------------------------------------------

fn expects() -> Vec<Vec<u8>> {
    scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .flat_map(|s| s.steps)
        .filter(|s| matches!(s.kind, fixbolt_conformance::script::Kind::Expect(_)))
        .filter_map(|s| Step::message(&s).map(|m| m.wire.clone()))
        .collect()
}

#[test]
fn every_expected_line_matches_itself_once_its_checksum_is_real() {
    let all = expects();
    assert_eq!(all.len(), 250);
    let mut placeholder = 0;
    for (i, e) in all.iter().enumerate() {
        let as_sent = fixbolt_conformance::script::with_real_checksum(e);
        assert_eq!(
            compare(e, &as_sent),
            Ok(()),
            "expected line {i} does not match itself"
        );
        if compare(e, e).is_err() {
            placeholder += 1;
        }
    }
    // The same 250 lines, unmodified, mostly fail — because their checksums are
    // placeholders. Both numbers are the finding.
    // 244 E lines carry 10=: 238 are the literal `10=0` and 2 are `10=7`, so 240
    // have no three consecutive digits. The other 6 E lines get a computed
    // checksum from fixify and pass. Both numbers are the finding.
    assert_eq!(placeholder, 240, "lines whose own 10= is not three digits");
}

#[test]
fn swapping_two_fields_of_every_corpus_line_fails_every_time() {
    // The reversal, over real data rather than one hand-written message. If the
    // comparator ever stops being positional this goes red 250 times.
    let mut checked = 0;
    for e in expects() {
        let mut fields: Vec<&[u8]> = e.split(|&b| b == 0x01).filter(|f| !f.is_empty()).collect();
        // Swap two body fields, never the frame: 8, 9, 35 are fixed and a
        // reordered frame is a different failure.
        if fields.len() < 6 {
            continue;
        }
        fields.swap(3, 4);
        let mut actual = Vec::with_capacity(e.len());
        for f in fields {
            actual.extend_from_slice(f);
            actual.push(0x01);
        }
        let actual = fixbolt_conformance::script::with_real_checksum(&actual);
        assert!(
            compare(&e, &actual).is_err(),
            "a swapped message compared equal:\n{}",
            String::from_utf8_lossy(&actual).replace('\x01', "|")
        );
        checked += 1;
    }
    assert!(
        checked >= 240,
        "only {checked} lines had enough fields to swap"
    );
}

#[test]
fn the_loose_tag_list_is_read_off_fields_fmt_not_believed() {
    // vendor/quickfix/test/definitions/fields.fmt is the source of rule 4. If
    // upstream adds a tag, this goes red rather than the runner quietly
    // comparing a field that was never meant to be compared exactly.
    let path = fixbolt_conformance::script::definitions_dir()
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("fields.fmt"))
        .expect("fields.fmt path");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\n\nrun scripts/fetch-quickfix-assets.sh",
            path.display()
        )
    });

    let tags: Vec<u32> = text
        .lines()
        .filter_map(|l| l.split('=').next())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse().ok())
        .collect();
    assert_eq!(tags, fixbolt_conformance::compare::LOOSE_TAGS);

    // And the patterns are the two shapes this comparator implements.
    for l in text.lines().filter(|l| !l.is_empty()) {
        let (tag, pat) = l.split_once('=').unwrap_or_default();
        match tag {
            "10" => assert_eq!(pat, r"\d{3}"),
            "52" => assert_eq!(
                pat, r"\d{8}-\d{2}:\d{2}:\d{2}|\d{8}-\d{2}:\d{2}:\d{2}[.]\d{3}",
                "the second alternative is redundant unanchored — if it stops being an \
                 alternation, Shape::Timestamp is no longer enough"
            ),
            _ => assert_eq!(pat, r"\d{8}-\d{2}:\d{2}:\d{2}", "tag {tag}"),
        }
    }
}
