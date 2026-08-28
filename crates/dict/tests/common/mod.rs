//! Reading QuickFIX's own generated C++ as an oracle.
//!
//! Four files, all under `vendor/quickfix/src/C++/`, all gitignored and read
//! rather than copied — `CLAUDE.md` §2 rule 9 and ADR-0001, the same terms the
//! `.def` corpus and `fix44/*.h` are already on.
//!
//! **A missing file is a failure, never a skip.** An oracle that quietly is not
//! there reports the same green as an oracle that agrees, and that is the exact
//! shape `scripts/check-lint-config.sh` exists to prevent elsewhere.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub fn vendor() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/quickfix")
}

/// Read one vendored file, or explain exactly how to get it.
pub fn read(rel: &str) -> String {
    let path = vendor().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\n\n\
             run scripts/fetch-quickfix-assets.sh — it fetches four src/C++ headers\n\
             as oracles. A vendor/ checkout made by an older copy of that script has\n\
             a narrower sparse-checkout and will not have them.",
            path.display()
        )
    })
}

/// `FixFieldNumbers.h`: every field name QuickFIX knows, to its tag number.
/// Every FIX version, not only 4.4 — which is what makes it a negative oracle
/// as well as a positive one.
pub fn quickfix_tag_numbers() -> BTreeMap<String, u32> {
    let text = read("src/C++/FixFieldNumbers.h");
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("const int ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once(" = ") else {
            continue;
        };
        let Ok(n) = value.trim_end_matches(';').trim().parse::<u32>() else {
            continue;
        };
        out.insert(name.to_string(), n);
    }
    assert!(
        out.len() > 6000,
        "FixFieldNumbers.h parsed to {} names; the file's shape has changed",
        out.len()
    );
    out
}

/// `FixFields.h` and `FixCommonFields.h`: field name to QuickFIX's type name,
/// read off the `DEFINE_<TYPE>(<Name>);` macros.
pub fn quickfix_field_types() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for file in ["src/C++/FixFields.h", "src/C++/FixCommonFields.h"] {
        for line in read(file).lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("DEFINE_") else {
                continue;
            };
            let Some((ty, rest)) = rest.split_once('(') else {
                continue;
            };
            let Some((name, _)) = rest.split_once(')') else {
                continue;
            };
            out.insert(name.to_string(), ty.to_string());
        }
    }
    assert!(
        out.len() > 6000,
        "FixFields.h parsed to {} names; the file's shape has changed",
        out.len()
    );
    out
}

/// `fix44/*.h`: the `MsgType("X")` of every generated FIX 4.4 message.
pub fn quickfix_msg_types() -> BTreeSet<String> {
    let dir = vendor().join("src/C++/fix44");
    let mut out = BTreeSet::new();
    for e in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = e.expect("dir entry").path();
        if path.extension().is_none_or(|x| x != "h") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read header");
        if let Some(mt) = text
            .split("MsgType(\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
        {
            out.insert(mt.to_string());
        }
    }
    assert!(
        out.len() > 90,
        "fix44/*.h yielded {} message types; the file's shape has changed",
        out.len()
    );
    out
}

/// `spec/FIX44.xml`: field name to tag number, for FIX 4.4 only.
///
/// The **same input the generator reads**, so it is a name list and nothing
/// more. Every claim about a number is settled against QuickFIX, not this.
pub fn xml_field_names() -> BTreeMap<String, u32> {
    let text = read("spec/FIX44.xml");
    let mut out = BTreeMap::new();
    for chunk in text.split("<field ").skip(1) {
        let head = chunk.split('>').next().unwrap_or_default();
        let (Some(number), Some(name)) = (attr(head, "number"), attr(head, "name")) else {
            continue;
        };
        if let Ok(n) = number.parse::<u32>() {
            out.insert(name.to_string(), n);
        }
    }
    assert_eq!(out.len(), 912, "FIX44.xml should define 912 fields");
    out
}

/// `spec/FIX44.xml`: field name to the XML's own type name.
pub fn xml_field_types() -> BTreeMap<String, String> {
    let text = read("spec/FIX44.xml");
    let mut out = BTreeMap::new();
    for chunk in text.split("<field ").skip(1) {
        let head = chunk.split('>').next().unwrap_or_default();
        if let (Some(name), Some(ty)) = (attr(head, "name"), attr(head, "type")) {
            out.insert(name.to_string(), ty.to_string());
        }
    }
    assert_eq!(out.len(), 912, "FIX44.xml should type 912 fields");
    out
}

/// `fix44/*.h`: for each message type, the tags its `FIELD_SET` lines name.
///
/// Body fields only — QuickFIX puts header and trailer on the `Message` base
/// class, so they never appear here. The caller adds them.
pub fn quickfix_message_fields() -> BTreeMap<String, BTreeSet<u32>> {
    let numbers = quickfix_tag_numbers();
    let dir = vendor().join("src/C++/fix44");
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = e.expect("dir entry").path();
        if path.extension().is_none_or(|x| x != "h") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read header");
        let Some(mt) = text
            .split("MsgType(\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
        else {
            continue;
        };
        let mut tags = BTreeSet::new();
        for chunk in text.split("FIELD_SET(*this, FIX::").skip(1) {
            let Some(name) = chunk.split(')').next() else {
                continue;
            };
            match numbers.get(name) {
                Some(&n) => {
                    tags.insert(n);
                }
                None => panic!("FIELD_SET names {name}, which FixFieldNumbers.h does not"),
            }
        }
        out.insert(mt.to_string(), tags);
    }
    out
}

/// `spec/FIX44.xml`: the tags of one top-level section, descending into groups.
pub fn xml_section_tags(section: &str) -> BTreeSet<u32> {
    let text = read("spec/FIX44.xml");
    let names = xml_field_names();
    let open = format!("<{section}>");
    let close = format!("</{section}>");
    let body = text
        .split(&open)
        .nth(1)
        .and_then(|s| s.split(&close).next())
        .unwrap_or_else(|| panic!("<{section}> not found in FIX44.xml"));
    let mut out = BTreeSet::new();
    for chunk in body.split("<field ").skip(1) {
        let head = chunk.split('>').next().unwrap_or_default();
        if let Some(name) = attr(head, "name")
            && let Some(&n) = names.get(name)
        {
            out.insert(n);
        }
    }
    // A `<group>` inside the section contributes its counter too.
    for chunk in body.split("<group ").skip(1) {
        let head = chunk.split('>').next().unwrap_or_default();
        if let Some(name) = attr(head, "name")
            && let Some(&n) = names.get(name)
        {
            out.insert(n);
        }
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
