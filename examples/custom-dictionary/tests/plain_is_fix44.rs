//! A type generated from an empty overlay is `Fix44`, answer for answer
//! (ADR-0207 *Consequences*, revision 2026-09-28; plan
//! `2026-09-26-docs-for-embedders` step 27).
//!
//! **Why this stands in for the 59 definitions.** The corpus is run against
//! `Fix44` (`crates/session/tests/score.rs`, 59 / 59); a session asks its
//! dictionary nothing but the fourteen functions of `Dictionary` and `Tables`.
//! So a type that answers all fourteen exactly as `Fix44` does, over every
//! input the session can hand them, scores what `Fix44` scores. The tables are
//! already proven byte-identical (`crates/dict/tests/overlay.rs`,
//! `an_empty_overlay_emits_byte_identical_fix44`); what this adds is the
//! generated `impl Dictionary` and `impl Tables` — a second, hand-written copy
//! of `Fix44`'s delegation, in which one miswired function would read fine
//! and surface only as a wrong `373=` code.
//!
//! **The domain.** Tags 0..=1024 (FIX 4.4's highest is 956) and 4999, 5000,
//! 9999, 10000, 20000, `u32::MAX`; every `msgtype` and every `enum` in
//! `crates/dict/spec/FIX44.xml`, read as strings from the file itself, plus
//! `""`, `"U1"`, `"ZZ"` and `"1 2"`; a group's counter over the tag domain.
//! The scan is asserted to find FIX 4.4's 93 message types, so it cannot pass
//! by sweeping nothing.
//!
//! `Plain` comes from this crate's `build.rs`, which generates it for this test
//! only; `src/lib.rs` does not include it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// A test binary: non-negotiable 7 is about what ships.
#![allow(clippy::indexing_slicing)]

use std::collections::BTreeSet;
use std::fmt::Debug;

use fixbolt::dict::{Dictionary, Fix44, Tables};

mod plain {
    include!(concat!(env!("OUT_DIR"), "/plain.rs"));
}
use plain::Plain;

/// The spec `Fix44` is generated from, read as text — no XML crate.
const FIX44_XML: &str = include_str!("../../../crates/dict/spec/FIX44.xml");

/// Every value of `attr='…'` in the spec, deduplicated, in file order.
fn scan(attr: &str) -> Vec<String> {
    let needle = format!("{attr}='");
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (at, _) in FIX44_XML.match_indices(&needle) {
        let rest = &FIX44_XML[at + needle.len()..];
        let value = &rest[..rest.find('\'').expect("the attribute closes")];
        if seen.insert(value.to_owned()) {
            out.push(value.to_owned());
        }
    }
    out
}

const PROBES: [&str; 4] = ["", "U1", "ZZ", "1 2"];

fn tags() -> Vec<u32> {
    (0..=1024)
        .chain([4999, 5000, 9999, 10_000, 20_000, u32::MAX])
        .collect()
}

/// Shows a message type or value the way a reader would type it: `"D"`.
fn s(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(bytes))
}

/// The first disagreement, if any: `call` names the function and arguments.
struct Diff(Option<String>);

impl Diff {
    fn check<T: PartialEq + Debug>(&mut self, call: impl FnOnce() -> String, plain: T, fix44: T) {
        if self.0.is_none() && plain != fix44 {
            self.0 = Some(format!(
                "{} differs: Plain {plain:?}, Fix44 {fix44:?}",
                call()
            ));
        }
    }
}

#[test]
fn an_empty_overlay_type_answers_every_dictionary_question_as_fix44() {
    let msg_types = scan("msgtype");
    assert_eq!(
        msg_types.len(),
        93,
        "the scan of FIX44.xml must find FIX 4.4's 93 message types, or this \
         test sweeps less than it says"
    );
    let enums = scan("enum");
    assert!(
        enums.len() > 300,
        "the scan of FIX44.xml found {} enum values",
        enums.len()
    );
    let mts: Vec<Vec<u8>> = msg_types
        .iter()
        .map(String::as_str)
        .chain(PROBES)
        .map(|m| m.as_bytes().to_vec())
        .collect();
    let values: Vec<Vec<u8>> = enums
        .iter()
        .map(String::as_str)
        .chain(PROBES)
        .map(|v| v.as_bytes().to_vec())
        .collect();
    let tags = tags();
    let mut d = Diff(None);

    // Tables: the questions with no argument or one message type.
    d.check(
        || "required_header()".to_owned(),
        <Plain as Tables>::required_header(),
        <Fix44 as Tables>::required_header(),
    );
    for mt in &mts {
        d.check(
            || format!("is_msg_type({})", s(mt)),
            <Plain as Tables>::is_msg_type(mt),
            <Fix44 as Tables>::is_msg_type(mt),
        );
        d.check(
            || format!("is_admin({})", s(mt)),
            <Plain as Tables>::is_admin(mt),
            <Fix44 as Tables>::is_admin(mt),
        );
        d.check(
            || format!("required({})", s(mt)),
            <Plain as Tables>::required(mt),
            <Fix44 as Tables>::required(mt),
        );
    }

    for &tag in &tags {
        // Dictionary, by tag.
        d.check(
            || format!("is_header({tag})"),
            <Plain as Dictionary>::is_header(tag),
            <Fix44 as Dictionary>::is_header(tag),
        );
        d.check(
            || format!("data_length_tag({tag})"),
            <Plain as Dictionary>::data_length_tag(tag),
            <Fix44 as Dictionary>::data_length_tag(tag),
        );
        // Tables, by tag.
        d.check(
            || format!("is_defined_tag({tag})"),
            <Plain as Tables>::is_defined_tag(tag),
            <Fix44 as Tables>::is_defined_tag(tag),
        );
        d.check(
            || format!("field_type({tag})"),
            <Plain as Tables>::field_type(tag),
            <Fix44 as Tables>::field_type(tag),
        );
        for v in &values {
            d.check(
                || format!("enum_allows({tag}, {})", s(v)),
                <Plain as Tables>::enum_allows(tag, v),
                <Fix44 as Tables>::enum_allows(tag, v),
            );
        }
        // By message type and tag — the tag doubling as a group's counter.
        for mt in &mts {
            d.check(
                || format!("is_defined_tag_for({}, {tag})", s(mt)),
                <Plain as Tables>::is_defined_tag_for(mt, tag),
                <Fix44 as Tables>::is_defined_tag_for(mt, tag),
            );
            d.check(
                || format!("allows({}, {tag})", s(mt)),
                <Plain as Tables>::allows(mt, tag),
                <Fix44 as Tables>::allows(mt, tag),
            );
            d.check(
                || format!("group_delimiter({}, {tag})", s(mt)),
                <Plain as Dictionary>::group_delimiter(mt, tag),
                <Fix44 as Dictionary>::group_delimiter(mt, tag),
            );
            d.check(
                || format!("group_members({}, {tag})", s(mt)),
                <Plain as Dictionary>::group_members(mt, tag),
                <Fix44 as Dictionary>::group_members(mt, tag),
            );
            d.check(
                || format!("group_order({}, {tag})", s(mt)),
                <Plain as Dictionary>::group_order(mt, tag),
                <Fix44 as Dictionary>::group_order(mt, tag),
            );
        }
    }

    if let Some(first) = d.0 {
        panic!("a type generated from an empty overlay is not Fix44: {first}");
    }
}
