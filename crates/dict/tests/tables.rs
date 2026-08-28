//! The five checks the plan names for step 1, plus the trap the XML sprang.
#![allow(clippy::unwrap_used, clippy::panic)]

use nanofix_codec::Dictionary;
use nanofix_dict::{Fix44, tag};

#[test]
fn header_tags_are_header() {
    assert!(Fix44::is_header(34), "MsgSeqNum(34) is a header field");
    assert!(Fix44::is_header(8), "BeginString(8)");
    assert!(Fix44::is_header(49), "SenderCompID(49)");
}

#[test]
fn body_tags_are_not_header() {
    assert!(!Fix44::is_header(11), "ClOrdID(11) is a body field");
    assert!(!Fix44::is_header(55), "Symbol(55)");
}

#[test]
fn data_fields_name_their_length_field() {
    assert_eq!(
        Fix44::data_length_tag(96),
        Some(95),
        "RawData -> RawDataLength"
    );
    assert_eq!(
        Fix44::data_length_tag(213),
        Some(212),
        "XmlData -> XmlDataLen"
    );
}

#[test]
fn signature_length_is_not_the_preceding_tag() {
    // The rule is NOT `tag - 1`. Signature(89) takes SignatureLength(93).
    // 15 of the 16 DATA fields in FIX 4.4 follow tag-1 and this one does not;
    // a generator written to that pattern mis-parses every signed message.
    // See docs/reference/fix44-dictionary-traps.md.
    assert_eq!(Fix44::data_length_tag(89), Some(93));
}

#[test]
fn non_data_fields_have_no_length_field() {
    assert_eq!(Fix44::data_length_tag(35), None, "MsgType is not DATA");
    assert_eq!(
        Fix44::data_length_tag(95),
        None,
        "the length field itself is not DATA"
    );
}

#[test]
fn required_fields_of_new_order_single() {
    // The plan first asked for 11, 21, 55, 54, 60, 40. Two of those are wrong
    // about the dictionary, not about the code: HandlInst(21) is required='N'
    // directly in <message>, and Symbol(55) is required='N' inside the
    // Instrument component. Component recursion would not add either. Plan
    // revised 2026-08-28 and re-approved.
    assert_eq!(Fix44::required(b"D"), &[11, 40, 54, 60]);
}

#[test]
fn required_is_knowingly_incomplete_without_component_recursion() {
    // Pinned so the limitation cannot be used by accident and cannot be closed
    // by accident either. News(B) requires MsgSeqNum-adjacent LinesOfText(33)
    // through a required component; without recursion only Headline(148) shows.
    // The repeating-groups plan lands recursion and this test flips to
    // [33, 148] there. STATUS.md open item 8.
    assert_eq!(
        Fix44::required(b"B"),
        &[148],
        "if this now contains 33, component recursion landed \
         — update this test, the plan and STATUS open item 8"
    );
}

#[test]
fn unknown_message_type_is_indistinguishable_from_no_requirements() {
    // Documented hole, pinned. b"ZZZ" is not a FIX 4.4 message type; Heartbeat
    // is, and requires nothing directly. Both answer the same. A caller that
    // needs to tell them apart cannot, which is one reason nothing calls this.
    assert_eq!(Fix44::required(b"ZZZ"), &[] as &[u32]);
    assert_eq!(Fix44::required(b"0"), &[] as &[u32]);
}

#[test]
fn tag_constants_exist() {
    assert_eq!(tag::MSG_SEQ_NUM, 34);
    assert_eq!(tag::CL_ORD_ID, 11);
}

#[test]
fn header_group_and_its_members_are_header_fields() {
    // NoHops(627) is a <group> inside <header>, not a <field>. A generator that
    // reads only direct <field> children misses it and its three members, and
    // they would then sort into the body when writing — non-negotiable 5.
    // No acceptance definition carries a hop: tags 627-630 appear 0 times in
    // the 59 .def files, so 59/59 would stay green with this wrong.
    for t in [627u32, 628, 629, 630] {
        assert!(
            Fix44::is_header(t),
            "NoHops group tag {t} belongs to the header"
        );
    }
}

#[test]
fn trailer_fields_are_not_classified_and_that_is_a_known_gap() {
    // <trailer> holds SignatureLength(93), Signature(89), CheckSum(10). None is
    // a header field, so is_header says false — which makes them sort into the
    // BODY, ascending, if anything ever writes them. Nothing does in step 1:
    // CheckSum is written explicitly last by the template, and no .def carries
    // a signature. Pinned so the day something signs a message, this test is
    // the thing that says why it went wrong. STATUS.md open item.
    for t in [10u32, 89, 93] {
        assert!(!Fix44::is_header(t), "tag {t} is trailer, not header");
    }
}
