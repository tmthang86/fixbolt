//! Which values each of the 245 enumerated fields will take.
//!
//! # The oracle here is one-directional, and that is measured, not assumed
//!
//! `FixValues.h` is QuickFIX's generated enum table for **every** FIX version at
//! once, so it cannot say what FIX 4.4 alone permits — it lists more. What it
//! can say is that FIX 4.4 permits nothing QuickFIX has never heard of, and it
//! says that about **every** field: `[measured 2026-08-28]` all **245**
//! enumerated fields are covered and all **1 708** values appear in QuickFIX's
//! lists, with zero exceptions.
//!
//! **The plan for this work called that oracle weak, on a bad measurement.** A
//! scouting script matched only `const char Name_X = 'v';` and missed
//! `const char Name_X[] = "vv";` — the array form every string-valued enum uses
//! — so it reported 228 of 245 fields covered and 95 differing. The array form
//! is 17 fields, `SecurityType(167)` among them, which is the field
//! `14e_IncorrectEnumValue.def` actually tests. Reading the file properly makes
//! the oracle cover everything.
//!
//! It still cannot gate the table on its own: it says nothing about values FIX
//! 4.4 forbids and QuickFIX allows. The gate is the XML plus the corpus; this is
//! the second opinion.
//!
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{read, xml_field_names};
use fixbolt_dict::Fix44;

/// `name -> the values FIX44.xml allows`.
fn xml_enums() -> BTreeMap<String, BTreeSet<String>> {
    let text = read("spec/FIX44.xml");
    let mut out = BTreeMap::new();
    for chunk in text.split("<field ").skip(1) {
        let head = chunk.split('>').next().unwrap_or_default();
        let Some(name) = attr(head, "name") else {
            continue;
        };
        let body = chunk.split("</field>").next().unwrap_or_default();
        let values: BTreeSet<String> = body
            .split("<value ")
            .skip(1)
            .filter_map(|v| attr(v.split('>').next().unwrap_or_default(), "enum"))
            .map(str::to_string)
            .collect();
        if !values.is_empty() {
            out.insert(name.to_string(), values);
        }
    }
    out
}

/// `FieldName -> every value QuickFIX names for it, across all FIX versions`.
fn quickfix_enums() -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in read("src/C++/FixValues.h").lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("const ") else {
            continue;
        };
        // "char* FieldName_SOME_NAME = "X";" or "char FieldName_X = 'Y';"
        let Some((_, decl)) = rest.split_once(' ') else {
            continue;
        };
        let decl = decl.trim_start_matches('*').trim();
        let Some((ident, literal)) = decl.split_once(" = ") else {
            continue;
        };
        let Some((field, _)) = ident.split_once('_') else {
            continue;
        };
        let lit = literal.trim_end_matches(';').trim();
        let lit = lit
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .or_else(|| lit.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            .unwrap_or(lit);
        out.entry(field.to_string())
            .or_default()
            .insert(lit.to_string());
    }
    out
}

fn attr<'a>(head: &'a str, key: &str) -> Option<&'a str> {
    for quote in ['\'', '"'] {
        let needle = format!("{key}={quote}");
        if let Some(rest) = head.split(&needle).nth(1)
            && let Some(v) = rest.split(quote).next()
        {
            return Some(v);
        }
    }
    None
}

#[test]
fn the_table_matches_the_xml_field_for_field_and_value_for_value() {
    let names = xml_field_names();
    let enums = xml_enums();
    assert_eq!(enums.len(), 245, "FIX 4.4 enumerates 245 fields");
    let total: usize = enums.values().map(BTreeSet::len).sum();
    assert_eq!(total, 1708, "1 708 enum values in all");

    for (name, values) in &enums {
        let tag = names[name];
        for v in values {
            assert_eq!(
                Fix44::enum_allows(tag, v.as_bytes()),
                Some(true),
                "{name}({tag}) should allow {v:?}"
            );
        }
        // A value that is not in the set, built so it cannot collide with one
        // that is.
        let absent = format!("{}~", values.iter().next().unwrap());
        assert_eq!(
            Fix44::enum_allows(tag, absent.as_bytes()),
            Some(false),
            "{name}({tag}) should refuse {absent:?}"
        );
    }
}

#[test]
fn a_field_without_an_enum_answers_none_rather_than_yes() {
    // The trap this test exists for: `Some(true)` for a field that has no enum
    // would make `373=5` fire on nothing at all, and every enum test above
    // would still pass.
    let enums = xml_enums();
    let names = xml_field_names();
    let mut checked = 0usize;
    for (name, &tag) in &names {
        if enums.contains_key(name) {
            continue;
        }
        assert_eq!(
            Fix44::enum_allows(tag, b"anything"),
            None,
            "{name}({tag}) has no enum and must answer None"
        );
        checked += 1;
    }
    assert_eq!(checked, 912 - 245, "667 fields with no enumeration");

    // And a tag FIX 4.4 does not define at all.
    for tag in [0u32, 999, 5000, u32::MAX] {
        assert_eq!(Fix44::enum_allows(tag, b"1"), None, "tag {tag}");
    }
}

#[test]
fn the_cases_the_corpus_supplies() {
    // 14e_IncorrectEnumValue.def, both messages.
    assert_eq!(
        Fix44::enum_allows(21, b"4"),
        Some(false),
        "HandlInst is 1..3"
    );
    assert_eq!(Fix44::enum_allows(21, b"1"), Some(true));
    assert_eq!(Fix44::enum_allows(167, b"BOO"), Some(false), "SecurityType");
    assert_eq!(Fix44::enum_allows(167, b"CORP"), Some(true));
    // ReverseRoute.def sends 40=w six times.
    assert_eq!(Fix44::enum_allows(40, b"w"), Some(false), "OrdType");
    assert_eq!(Fix44::enum_allows(40, b"1"), Some(true));
    // 11c_NewSeqNoLess.def carries 123=N, which is legal.
    assert_eq!(Fix44::enum_allows(123, b"N"), Some(true), "GapFillFlag");
    assert_eq!(Fix44::enum_allows(123, b"Q"), Some(false));
}

#[test]
fn every_fix44_enum_value_is_one_quickfix_also_knows() {
    // One-directional and stated as such. QuickFIX lists every version's values,
    // so it cannot say what 4.4 forbids — but if 4.4 permitted a value QuickFIX
    // had never heard of, one of the two generators would be wrong.
    let names = xml_field_names();
    let enums = xml_enums();
    let quickfix = quickfix_enums();

    let mut covered = 0usize;
    let mut values = 0usize;
    let mut missing = Vec::new();
    let mut uncovered = Vec::new();
    for (name, ours) in &enums {
        let tag = names[name];
        match quickfix.get(name) {
            None => uncovered.push(format!("{name}({tag})")),
            Some(theirs) => {
                covered += 1;
                values += ours.len();
                for v in ours {
                    if !theirs.contains(v) {
                        missing.push(format!("{name}({tag}) = {v:?}"));
                    }
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "values QuickFIX does not know: {missing:#?}"
    );
    assert!(
        uncovered.is_empty(),
        "FixValues.h covers every enumerated field; these are new: {uncovered:#?}"
    );
    assert_eq!(
        covered, 245,
        "all 245 enumerated fields have a QuickFIX opinion"
    );
    assert_eq!(values, 1708, "all 1 708 values checked against QuickFIX");
}

#[test]
fn the_array_form_is_read_and_not_skipped() {
    // The specific parse bug that made the oracle look weak. `SecurityType` is
    // written as `const char SecurityType_CORPORATE_BOND[] = "CORP";`, and a
    // reader that expects `Name = 'v'` finds nothing for it. If this test ever
    // fails, the interop above is silently covering less than it claims.
    let quickfix = quickfix_enums();
    let security_type = quickfix
        .get("SecurityType")
        .expect("FixValues.h carries SecurityType in the `Name[] = \"vv\"` form");
    assert!(security_type.contains("CORP"), "{security_type:?}");
    assert!(!security_type.contains("BOO"));
    assert!(
        security_type.len() > 50,
        "only {} SecurityType values parsed",
        security_type.len()
    );
}
