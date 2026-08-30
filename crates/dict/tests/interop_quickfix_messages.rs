//! A second opinion on which tags belong to which message.
//!
//! `Fix44::allows(msg_type, tag)` answers `SessionRejectReason 2`, *Tag not
//! defined for this message type* — `14c_TagNotDefinedForMsgType.def` sends
//! `55=MSFT` on a Heartbeat and expects it refused. One definition, out of 59,
//! against **12 524** (message, tag) pairs. The corpus cannot gate this table;
//! QuickFIX's generator can.
//!
//! # Both directions, exhaustively
//!
//! 93 message types × 912 tags = **84 816** answers, every one compared against
//! `FIELD_SET` plus the header and trailer sections. Checking only the positive
//! direction would let `allows` return `true` for everything, which is the same
//! failure `is_defined_tag` had to be protected from.
//!
//! `[measured 2026-08-28]` the two derivations agree on all 12 524 body pairs.
//! **They read the same XML** — by different generators, twenty years apart, but
//! the same file. That is the same limit `interop_quickfix_order.rs` states, and
//! it is worth as much and no more.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use common::{quickfix_message_fields, quickfix_msg_types, xml_field_names, xml_section_tags};
use fixbolt_dict::Fix44;

#[test]
fn every_message_tag_pair_agrees_with_quickfix_in_both_directions() {
    let quickfix = quickfix_message_fields();
    let header = xml_section_tags("header");
    let trailer = xml_section_tags("trailer");
    let all_tags: BTreeSet<u32> = xml_field_names().values().copied().collect();

    assert_eq!(
        header.len(),
        30,
        "the FIX 4.4 header, NoHops and its three members included"
    );
    assert_eq!(trailer.len(), 3, "SignatureLength, Signature, CheckSum");
    assert_eq!(quickfix.len(), 93, "one generated class per message type");

    let body_pairs: usize = quickfix.values().map(BTreeSet::len).sum();
    assert_eq!(
        body_pairs, 12_524,
        "the (message, tag) pairs QuickFIX generates"
    );

    let mut answers = 0usize;
    let mut wrong = Vec::new();
    for (mt, body) in &quickfix {
        for &tag in &all_tags {
            let expected = body.contains(&tag) || header.contains(&tag) || trailer.contains(&tag);
            if Fix44::allows(mt.as_bytes(), tag) != expected {
                wrong.push(format!(
                    "{mt}/{tag}: expected {expected}, got {}",
                    !expected
                ));
            }
            answers += 1;
        }
    }
    assert!(
        wrong.is_empty(),
        "{} disagreements: {:#?}",
        wrong.len(),
        &wrong[..wrong.len().min(20)]
    );
    assert_eq!(
        answers,
        93 * 912,
        "84 816 answers, every message against every tag"
    );
}

#[test]
fn a_header_tag_is_allowed_on_every_message_and_a_body_tag_is_not() {
    // The rule `14c` turns on, stated directly. `52=` must never be "not
    // defined for this message type", or every message the session rejects
    // would be rejected for the wrong reason.
    for mt in quickfix_msg_types() {
        assert!(
            Fix44::allows(mt.as_bytes(), 52),
            "{mt} must allow SendingTime"
        );
        assert!(Fix44::allows(mt.as_bytes(), 10), "{mt} must allow CheckSum");
    }
    // `14c_TagNotDefinedForMsgType.def`, exactly.
    assert!(!Fix44::allows(b"0", 55), "Symbol is not a Heartbeat field");
    assert!(Fix44::allows(b"D", 55), "Symbol is a NewOrderSingle field");
}

#[test]
fn an_unknown_message_type_allows_nothing() {
    // Not "allows everything". A message type the dictionary does not know is
    // answered by `373=11` before any tag is looked at, but if the session ever
    // asks anyway, the safe answer is no.
    for tag in [8u32, 35, 52, 55, 10] {
        assert!(!Fix44::allows(b"*", tag), "tag {tag} on an unknown MsgType");
        assert!(!Fix44::allows(b"", tag));
    }
}
