//! DATA fields: the one place a value may legally contain the separator, and the
//! three ways the parser can be led out of bounds.
//!
//! The `.def` corpus has **no** DATA message at all, so none of this is covered
//! by real data. These frames are built to the FIX 4.4 specification and are
//! labelled as such — see `docs/reference/fix44-dictionary-traps.md`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use nanofix_codec::{Dictionary, FieldIndex, ParseError, Parsed, Validation, parse_into};

/// Three real FIX 4.4 DATA pairings, including the one that breaks the obvious
/// `tag - 1` rule. `crates/dict` generates the full table; this keeps the codec's
/// own tests free of a dependency cycle.
struct TestDict;

impl Dictionary for TestDict {
    fn is_header(tag: u32) -> bool {
        matches!(tag, 8 | 9 | 35 | 34 | 49 | 52 | 56)
    }
    fn data_length_tag(tag: u32) -> Option<u32> {
        match tag {
            91 => Some(90), // SecureData    <- SecureDataLen
            96 => Some(95), // RawData       <- RawDataLength
            89 => Some(93), // Signature     <- SignatureLength, NOT 88
            213 => Some(212),
            _ => None,
        }
    }
}

/// Build a frame with a correct `9=`, so DATA bounds are exercised rather than
/// body length.
fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = b"8=FIX.4.4\x01".to_vec();
    out.extend_from_slice(format!("9={}\u{1}", body.len()).as_bytes());
    out.extend_from_slice(body);
    let sum = out.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    out.extend_from_slice(format!("10={sum:03}\u{1}").as_bytes());
    out
}

#[test]
fn a_data_value_may_contain_the_separator() {
    // RawDataLength=5, RawData=ab<SOH>cd. A parser that scans for 0x01 cuts this
    // in the middle and every field after it is garbage.
    let msg = frame(b"35=A\x0134=2\x0195=5\x0196=ab\x01cd\x0198=0\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    let r = parse_into::<TestDict, 64>(&msg, &mut idx, Validation::ALL).unwrap();
    assert_eq!(
        r,
        Parsed::Complete {
            consumed: msg.len()
        }
    );

    let view = idx.view(&msg);
    assert_eq!(
        view.get(96),
        Some(&b"ab\x01cd"[..]),
        "the SOH is part of the value"
    );
    assert_eq!(
        view.get(98),
        Some(&b"0"[..]),
        "the field after it is still found"
    );
    assert_eq!(view.len(), 8);
}

#[test]
fn signature_uses_its_named_length_field_not_the_preceding_tag() {
    // Signature(89) takes SignatureLength(93). If the dictionary said 88 this
    // frame would be cut at the SOH inside the signature.
    let msg = frame(b"35=A\x0134=2\x0193=4\x0189=a\x01bc\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    parse_into::<TestDict, 64>(&msg, &mut idx, Validation::ALL).unwrap();
    assert_eq!(idx.view(&msg).get(89), Some(&b"a\x01bc"[..]));
}

#[test]
fn a_data_field_without_its_length_field_is_refused() {
    let msg = frame(b"35=A\x0134=2\x0196=abc\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert_eq!(
        parse_into::<TestDict, 64>(&msg, &mut idx, Validation::ALL),
        Err(ParseError::MissingLengthField(96))
    );
}

#[test]
fn the_length_field_must_be_immediately_before_not_merely_present() {
    // 95= is there, but 34= sits between it and 96=. Accepting this would mean
    // trusting a length that belongs to a different field.
    let msg = frame(b"35=A\x0195=3\x0134=2\x0196=abc\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert_eq!(
        parse_into::<TestDict, 64>(&msg, &mut idx, Validation::ALL),
        Err(ParseError::MissingLengthField(96))
    );
}

#[test]
fn a_length_past_the_end_of_the_frame_is_refused_not_read() {
    // The single place the parser could read out of bounds. 95=999999 on a frame
    // that is nowhere near that long.
    let msg = frame(b"35=A\x0195=999999\x0196=ab\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert_eq!(
        parse_into::<TestDict, 64>(&msg, &mut idx, Validation::ALL),
        Err(ParseError::LengthOutOfBounds(96))
    );
}

#[test]
fn a_length_that_does_not_land_on_a_separator_is_refused() {
    // 95=2 but the value is 5 bytes. The declared length lands inside the value,
    // where there is no SOH, so the frame is not what it claims to be.
    let msg = frame(b"35=A\x0195=2\x0196=abcde\x0198=0\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert_eq!(
        parse_into::<TestDict, 64>(&msg, &mut idx, Validation::ALL),
        Err(ParseError::LengthOutOfBounds(96))
    );
}

#[test]
fn a_data_field_split_across_two_reads_is_incomplete_not_an_error() {
    let msg = frame(b"35=A\x0134=2\x0195=5\x0196=ab\x01cd\x0198=0\x01");
    let mut idx: FieldIndex<64> = FieldIndex::new();
    for cut in 1..msg.len() {
        let r = parse_into::<TestDict, 64>(&msg[..cut], &mut idx, Validation::NONE);
        assert_eq!(
            r,
            Ok(Parsed::Incomplete),
            "a {cut}-byte prefix must ask for more, not fail"
        );
    }
}

#[test]
fn a_value_longer_than_u16_max_is_surfaced_not_wrapped() {
    // FieldEntry stores len as u16. 65536 bytes must not become 0.
    let big = vec![b'x'; 65536];
    let mut body = b"35=A\x0158=".to_vec();
    body.extend_from_slice(&big);
    body.push(0x01);
    let msg = frame(&body);
    let mut idx: FieldIndex<64> = FieldIndex::new();
    assert_eq!(
        parse_into::<TestDict, 64>(&msg, &mut idx, Validation::ALL),
        Err(ParseError::FieldTooLong(58))
    );
}

#[test]
fn a_value_of_exactly_u16_max_still_works() {
    let big = vec![b'x'; 65535];
    let mut body = b"35=A\x0158=".to_vec();
    body.extend_from_slice(&big);
    body.push(0x01);
    let msg = frame(&body);
    let mut idx: FieldIndex<64> = FieldIndex::new();
    parse_into::<TestDict, 64>(&msg, &mut idx, Validation::ALL).unwrap();
    assert_eq!(idx.view(&msg).get(58).map(<[u8]>::len), Some(65535));
}
