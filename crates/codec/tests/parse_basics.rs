//! The parser's syntax contract, on hand-built frames. Real `.def` data is
//! `tests/defs.rs`; these are the boundaries that corpus does not contain.
#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use nanofix_codec::{FieldIndex, NoDict, ParseError, Parsed, Validation, parse_into};

#[test]
fn parses_a_complete_heartbeat() {
    let msg = b"8=FIX.4.4\x019=29\x0135=0\x0134=2\x0149=TW44\x0156=ISLD\x0110=000\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let r = parse_into::<NoDict, 64>(msg, &mut idx, Validation::NONE).unwrap();
    assert!(matches!(r, Parsed::Complete { consumed } if consumed == msg.len()));
    assert_eq!(idx.len(), 7);
}

#[test]
fn begin_string_must_start_the_frame() {
    // 2t_FirstThreeFieldsOutOfOrder line 8: MsgType first. The frame cannot be
    // delimited at all without 8= and 9= where they belong.
    let msg = b"35=0\x018=FIX.4.4\x019=29\x0134=2\x0110=121\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert_eq!(
        parse_into::<NoDict, 64>(msg, &mut idx, Validation::NONE),
        Err(ParseError::BadFrameStart)
    );
}

#[test]
fn body_length_must_immediately_follow_begin_string() {
    let msg = b"8=FIX.4.4\x0135=0\x019=29\x0110=121\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert_eq!(
        parse_into::<NoDict, 64>(msg, &mut idx, Validation::NONE),
        Err(ParseError::BadFrameStart)
    );
}

#[test]
fn msg_type_position_is_not_the_parsers_business() {
    // 2t line 11: 34= before 35=. It frames perfectly well. QuickFIX drops it
    // without responding and without consuming a sequence number, so the
    // session needs nothing from it — but deciding that is the session's job.
    let msg = b"8=FIX.4.4\x019=25\x0134=3\x0135=0\x0149=TW44\x0110=000\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert!(matches!(
        parse_into::<NoDict, 64>(msg, &mut idx, Validation::NONE),
        Ok(Parsed::Complete { .. })
    ));
}

#[test]
fn every_prefix_of_a_frame_is_incomplete() {
    let msg = b"8=FIX.4.4\x019=29\x0135=0\x0134=2\x0149=TW44\x0156=ISLD\x0110=000\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    for cut in 1..msg.len() {
        assert_eq!(
            parse_into::<NoDict, 64>(&msg[..cut], &mut idx, Validation::NONE),
            Ok(Parsed::Incomplete),
            "prefix of {cut} bytes should be Incomplete"
        );
    }
}

#[test]
fn empty_value_is_a_field_not_an_error() {
    // 14d sends 56= with nothing after it and expects a Reject naming tag 56,
    // WITH the sequence number consumed. If the parser refused the frame the
    // session could not read 34= and 14d could never pass. D12.
    let msg = b"8=FIX.4.4\x019=20\x0135=A\x0134=2\x0156=\x0110=000\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    parse_into::<NoDict, 64>(msg, &mut idx, Validation::NONE).unwrap();
    let view = idx.view(msg);
    assert_eq!(view.get(56), Some(&b""[..]), "56= is present and empty");
    assert_eq!(view.get(34), Some(&b"2"[..]));
}

#[test]
fn non_numeric_tag_is_rejected() {
    // 2d/3c_GarbledMessage: "4garbled9=TW"
    let msg = b"8=FIX.4.4\x019=52\x0135=0\x0134=2\x014garbled9=TW\x0110=000\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert!(matches!(
        parse_into::<NoDict, 64>(msg, &mut idx, Validation::NONE),
        Err(ParseError::BadTag { .. })
    ));
}

#[test]
fn overflowing_tag_is_rejected_not_wrapped() {
    let msg = b"8=FIX.4.4\x019=20\x0199999999999=x\x0110=000\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert!(matches!(
        parse_into::<NoDict, 64>(msg, &mut idx, Validation::NONE),
        Err(ParseError::BadTag { .. })
    ));
}

#[test]
fn too_many_fields_errors_and_does_not_truncate() {
    let msg = b"8=FIX.4.4\x019=29\x0135=0\x0134=2\x0149=TW44\x0156=ISLD\x0110=000\x01";
    let mut idx: FieldIndex<4> = FieldIndex::new();
    assert_eq!(
        parse_into::<NoDict, 4>(msg, &mut idx, Validation::NONE),
        Err(ParseError::TooManyFields)
    );
}

#[test]
fn wrong_body_length_is_caught_when_validation_is_on() {
    // 1d_InvalidLogonLengthInvalid ships 9=40 on a body that is not 40 bytes.
    let msg = b"8=FIX.4.4\x019=40\x0135=0\x0134=2\x0110=000\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert_eq!(
        parse_into::<NoDict, 64>(msg, &mut idx, Validation::ALL),
        Err(ParseError::BadBodyLength)
    );
}

#[test]
fn wrong_checksum_is_caught_when_validation_is_on() {
    // 3b_InvalidChecksum ships 10=256, which is not even a byte value.
    let msg = b"8=FIX.4.4\x019=10\x0135=0\x0134=2\x0110=256\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert_eq!(
        parse_into::<NoDict, 64>(msg, &mut idx, Validation::ALL),
        Err(ParseError::BadCheckSum)
    );
}

#[test]
fn message_view_is_twenty_four_bytes() {
    assert_eq!(
        core::mem::size_of::<nanofix_codec::MessageView<'static, 64>>(),
        24
    );
    assert_eq!(core::mem::size_of::<nanofix_codec::FieldEntry>(), 12);
    assert_eq!(core::mem::align_of::<nanofix_codec::FieldEntry>(), 4);
}

#[test]
fn a_negative_tag_still_lets_the_session_answer() {
    // 14a_BadField line 25 sends -1=HI and its @expected says: "Send Reject
    // ... referencing invalid tag number. Increment inbound MsgSeqNum." So the
    // session must read 34= from a message the parser could not finish, and must
    // put the text "-1" into 371=. Both come out of this one error.
    let msg = b"8=FIX.4.4\x019=40\x0135=0\x0134=4\x0149=TW44\x01-1=HI\x0110=000\x01";
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let Err(ParseError::BadTag { at }) = parse_into::<NoDict, 64>(msg, &mut idx, Validation::NONE)
    else {
        panic!("expected BadTag")
    };
    assert_eq!(
        nanofix_codec::tag_text_at(msg, at as usize),
        Some(&b"-1"[..])
    );

    // Everything before the bad tag survived, which is the whole point.
    let view = idx.view(msg);
    assert_eq!(
        view.get(34),
        Some(&b"4"[..]),
        "the session can still read MsgSeqNum"
    );
    assert_eq!(view.get(35), Some(&b"0"[..]));
}

#[test]
fn tags_that_are_merely_wrong_are_not_the_parsers_business() {
    // The same file sends 999=, 0= and 5000=. All three are readable numbers, so
    // the parser passes them up and the session rejects them against the
    // dictionary. Only the unreadable one stops here.
    for tag in ["999", "0", "5000"] {
        let msg = format!("8=FIX.4.4\u{1}9=40\u{1}35=0\u{1}34=4\u{1}{tag}=HI\u{1}10=000\u{1}");
        let mut idx: FieldIndex<64> = FieldIndex::new();
        assert!(
            matches!(
                parse_into::<NoDict, 64>(msg.as_bytes(), &mut idx, Validation::NONE),
                Ok(Parsed::Complete { .. })
            ),
            "tag {tag} is readable; judging it is the session's job"
        );
    }
}
