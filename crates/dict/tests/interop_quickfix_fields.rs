//! A second opinion on tag numbers, from QuickFIX's own generated C++.
//!
//! `crates/dict/build.rs` reads `spec/FIX44.xml`. So does QuickFIX's generator —
//! but it wrote `src/C++/FixFieldNumbers.h` twenty years earlier and by a
//! different program. Agreeing with it is not the same as reading the XML twice,
//! which is the whole reason `interop_quickfix_order.rs` exists and the reason
//! this one does.
//!
//! # What the oracle can and cannot settle
//!
//! `FixFieldNumbers.h` covers **every** FIX version, so it settles two things:
//!
//! * **positive** — each of the 912 names FIX 4.4 defines has the number this
//!   crate gives it;
//! * **negative** — the 5 168 field names QuickFIX knows whose tag FIX 4.4 does
//!   not define are **not** defined here. Without this half, `is_defined_tag`
//!   returning `true` for everything would pass.
//!
//! `[measured 2026-08-28]` 5 168 *names* over 5 154 distinct *numbers* — later
//! versions give one number more than one name. The loop below walks names, so
//! it counts names; the first draft of this test asserted the number count and
//! went red by 14, which is how the difference got noticed.
//!
//! The name list itself comes from the XML, because there is nowhere else to get
//! it. That is the same limit `interop_quickfix_order.rs` states about its own
//! agreement, and it is stated here rather than glossed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{
    quickfix_field_types, quickfix_msg_types, quickfix_tag_numbers, xml_field_names,
    xml_field_types,
};
use fixbolt_dict::{FieldType, Fix44};

/// Tags QuickFIX names differently in a later version but numbers the same.
///
/// `[measured 2026-08-28]` exactly 27. They are not disagreements — the number
/// agrees, the *name* moved — but they must be exempted from the negative check,
/// because QuickFIX knows them under a name FIX 4.4 does not use.
///
/// Written out rather than computed so a 28th shows up as a failure.
const RENAMED_ACROSS_VERSIONS: usize = 27;

#[test]
fn every_fix44_tag_number_agrees_with_quickfix() {
    let xml = xml_field_names();
    let quickfix = quickfix_tag_numbers();

    let mut checked = 0usize;
    let mut absent = Vec::new();
    let mut disagreed = Vec::new();
    for (name, &number) in &xml {
        match quickfix.get(name) {
            None => absent.push(name.clone()),
            Some(&theirs) if theirs != number => disagreed.push((name.clone(), number, theirs)),
            Some(_) => checked += 1,
        }
    }

    assert!(absent.is_empty(), "QuickFIX does not name: {absent:?}");
    assert!(disagreed.is_empty(), "number disagreements: {disagreed:?}");
    assert_eq!(
        checked, 912,
        "all 912 FIX 4.4 tags, checked against QuickFIX"
    );

    for (name, &number) in &xml {
        assert!(
            Fix44::is_defined_tag(number),
            "{name} is tag {number} and this crate does not know it"
        );
    }
}

#[test]
fn a_tag_quickfix_knows_and_fix_44_does_not_is_not_defined_here() {
    let xml = xml_field_names();
    let quickfix = quickfix_tag_numbers();
    let ours: std::collections::BTreeSet<u32> = xml.values().copied().collect();

    let mut renamed = 0usize;
    let mut foreign = 0usize;
    for (name, &number) in &quickfix {
        if xml.contains_key(name) {
            continue;
        }
        if ours.contains(&number) {
            // Same number, different name: FIX renamed the field in a later
            // version. `is_defined_tag` is about numbers, so this is fine.
            renamed += 1;
            continue;
        }
        foreign += 1;
        assert!(
            !Fix44::is_defined_tag(number),
            "{name} is tag {number}, which FIX 4.4 does not define, \
             and this crate says it does"
        );
    }

    assert_eq!(
        renamed, RENAMED_ACROSS_VERSIONS,
        "the set of fields FIX renamed while keeping the number has changed"
    );
    assert_eq!(
        foreign, 5168,
        "5 168 field names whose tag FIX 4.4 does not define — this is the half \
         that stops `is_defined_tag` from being `true` for everything. Names, \
         not numbers: they cover 5 154 distinct tags"
    );
}

#[test]
fn the_message_types_agree_exactly_in_both_directions() {
    let quickfix = quickfix_msg_types();
    assert_eq!(quickfix.len(), 93, "FIX 4.4 has 93 message types");

    for mt in &quickfix {
        assert!(
            Fix44::is_msg_type(mt.as_bytes()),
            "QuickFIX generates a class for {mt} and this crate does not know it"
        );
    }

    // The other direction, over every one- and two-character candidate. Without
    // it, `is_msg_type` returning `true` for everything would pass — and `35=*`
    // in `2q_MsgTypeNotValid.def` is exactly a one-character non-type.
    let alphabet: Vec<u8> = (b'0'..=b'9')
        .chain(b'A'..=b'Z')
        .chain(b'a'..=b'z')
        .chain(*b"*?!")
        .collect();
    let mut rejected = 0usize;
    for &a in &alphabet {
        for candidate in [vec![a], vec![a, b'A'], vec![a, b'0']] {
            let s = String::from_utf8(candidate.clone()).unwrap();
            if quickfix.contains(&s) {
                continue;
            }
            assert!(
                !Fix44::is_msg_type(&candidate),
                "{s:?} is not a FIX 4.4 message type and this crate says it is"
            );
            rejected += 1;
        }
    }
    assert!(rejected > 100, "only {rejected} negative candidates");

    assert!(!Fix44::is_msg_type(b""), "the empty MsgType is not a type");
}

/// QuickFIX types that mean the same thing as an XML type under another name.
///
/// `FixFields.h` is shared across every FIX version, so it carries the refined
/// spellings later versions introduced. These are naming, not disagreement.
fn same_type(xml: &str, quickfix: &str) -> bool {
    matches!(
        (xml, quickfix),
        ("STRING", "STRING")
            | ("INT", "INT")
            | ("CHAR", "CHAR")
            | ("FLOAT", "FLOAT")
            | ("QTY", "QTY")
            | ("PRICE", "PRICE")
            | ("PRICEOFFSET", "PRICEOFFSET")
            | ("AMT", "AMT")
            | ("PERCENTAGE", "PERCENTAGE")
            | ("BOOLEAN", "BOOLEAN")
            | ("LENGTH", "LENGTH")
            | ("SEQNUM", "SEQNUM")
            | ("NUMINGROUP", "NUMINGROUP")
            | ("CURRENCY", "CURRENCY")
            | ("COUNTRY", "COUNTRY")
            | ("EXCHANGE", "EXCHANGE")
            | ("MONTHYEAR", "MONTHYEAR")
            | ("LOCALMKTDATE", "LOCALMKTDATE")
            | ("UTCDATEONLY", "UTCDATEONLY")
            | ("UTCTIMEONLY", "UTCTIMEONLY")
            | ("UTCTIMESTAMP", "UTCTIMESTAMP")
            | ("DATA", "DATA")
            | ("MULTIPLEVALUESTRING", "MULTIPLEVALUESTRING")
    )
}

/// The 14 fields where QuickFIX's generator and FIX44.xml give different type
/// names, listed by tag so a fifteenth is a failure rather than a shrug.
///
/// `[measured 2026-08-28]` all 14 are `FixFields.h` carrying a later version's
/// refinement — a `MULTIPLEVALUESTRING` split into char and string variants, a
/// `STRING` that became an `INT`. **The XML is the source of truth**
/// (ADR-0001), so this crate follows the XML and the difference is recorded
/// rather than resolved. None of the 14 is touched by the acceptance corpus.
const TYPE_EXEMPTIONS: &[(u32, &str, &str)] = &[
    (10, "STRING", "CHECKSUM"),
    (18, "MULTIPLEVALUESTRING", "MULTIPLECHARVALUE"),
    (63, "CHAR", "STRING"),
    (276, "MULTIPLEVALUESTRING", "MULTIPLESTRINGVALUE"),
    (277, "MULTIPLEVALUESTRING", "MULTIPLESTRINGVALUE"),
    (286, "MULTIPLEVALUESTRING", "MULTIPLECHARVALUE"),
    (291, "MULTIPLEVALUESTRING", "MULTIPLECHARVALUE"),
    (292, "MULTIPLEVALUESTRING", "MULTIPLECHARVALUE"),
    (529, "MULTIPLEVALUESTRING", "MULTIPLECHARVALUE"),
    (532, "STRING", "INT"),
    (546, "MULTIPLEVALUESTRING", "MULTIPLECHARVALUE"),
    (587, "CHAR", "STRING"),
    (674, "STRING", "INT"),
    (877, "STRING", "INT"),
];

#[test]
fn every_field_type_agrees_with_quickfix_or_is_a_named_exemption() {
    let xml = xml_field_names();
    let xml_types = xml_field_types();
    let quickfix = quickfix_field_types();

    let mut agreed = 0usize;
    let mut exempted = Vec::new();
    let mut surprises = Vec::new();
    for (name, &tag) in &xml {
        let ours = &xml_types[name];
        let Some(theirs) = quickfix.get(name) else {
            surprises.push(format!("{name} ({tag}): QuickFIX gives it no type"));
            continue;
        };
        if same_type(ours, theirs) {
            agreed += 1;
        } else if TYPE_EXEMPTIONS
            .iter()
            .any(|&(t, x, q)| t == tag && x == ours && q == theirs)
        {
            exempted.push(tag);
        } else {
            surprises.push(format!("{name} ({tag}): XML {ours}, QuickFIX {theirs}"));
        }
    }

    assert!(
        surprises.is_empty(),
        "unexplained type differences: {surprises:#?}"
    );
    assert_eq!(
        exempted.len(),
        TYPE_EXEMPTIONS.len(),
        "an exemption went unused"
    );
    assert_eq!(
        agreed,
        912 - TYPE_EXEMPTIONS.len(),
        "898 exact type agreements"
    );

    // And the crate's own table agrees with the XML it was generated from —
    // the half the interop above cannot see.
    for (name, &tag) in &xml {
        let ours = FieldType::from_xml(&xml_types[name])
            .unwrap_or_else(|| panic!("{name}: src/field_type.rs has no {}", xml_types[name]));
        assert_eq!(
            Fix44::field_type(tag),
            Some(ours),
            "{name} is tag {tag}, type {}",
            xml_types[name]
        );
    }
}
