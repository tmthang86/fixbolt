//! Generates `$OUT_DIR/fix44.rs` from the QuickFIX FIX 4.4 XML dictionary.
//!
//! The dictionary is not in this repository — ADR-0001 keeps it in gitignored
//! `vendor/`. When it is absent the build fails loudly and names the script that
//! fetches it. It never falls back to a stub: a dictionary that silently becomes
//! empty is a parser that silently stops validating.
//!
//! Traps this generator is written against are recorded in
//! `docs/reference/fix44-dictionary-traps.md`. Two matter here:
//!   * a DATA field's length field is NOT `tag - 1` — Signature(89) takes
//!     SignatureLength(93). Matching is by name.
//!   * `<message>` may be self-closing (`XMLnonFIX`), so "has children" is not
//!     the same as "exists".

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

/// `NANOFIX_FIX44_XML` overrides the location, for packagers and for CI runs
/// that place the asset elsewhere.
const OVERRIDE: &str = "NANOFIX_FIX44_XML";
const DEFAULT: &str = "../../vendor/quickfix/spec/FIX44.xml";

fn main() {
    println!("cargo:rerun-if-env-changed={OVERRIDE}");

    let path = match std::env::var(OVERRIDE) {
        Ok(p) => PathBuf::from(p),
        Err(_) => PathBuf::from(DEFAULT),
    };
    println!("cargo:rerun-if-changed={}", path.display());

    if !path.exists() {
        die(&format!(
            "FIX 4.4 dictionary not found at {}\n\n  run scripts/fetch-quickfix-assets.sh\n\n\
             It is not committed on purpose: the QuickFIX licence's attribution clause\n\
             would come with it. See docs/decisions/ADR-0001-relationship-to-quickfix.md.\n\
             Set {OVERRIDE} to use a copy from somewhere else.",
            path.display()
        ));
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => die(&format!("cannot read {}: {e}", path.display())),
    };
    let doc = match roxmltree::Document::parse(&text) {
        Ok(d) => d,
        Err(e) => die(&format!("{} is not well-formed XML: {e}", path.display())),
    };

    let generated = generate(&doc);

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));
    if let Err(e) = std::fs::write(out.join("fix44.rs"), generated) {
        die(&format!("cannot write fix44.rs: {e}"));
    }
}

fn die(msg: &str) -> ! {
    eprintln!("\nnanofix-dict: {msg}\n");
    std::process::exit(1)
}

fn child<'a, 'i>(root: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    root.children().find(|n| n.has_tag_name(name))
}

fn generate(doc: &roxmltree::Document<'_>) -> String {
    let root = doc.root_element();

    // ---- every field: number, name, type -----------------------------------
    let Some(fields_el) = child(root, "fields") else {
        die("<fields> section missing")
    };
    let mut number_of: BTreeMap<&str, u32> = BTreeMap::new();
    let mut type_of: BTreeMap<&str, &str> = BTreeMap::new();
    for f in fields_el.children().filter(|n| n.has_tag_name("field")) {
        let (Some(name), Some(num)) = (f.attribute("name"), f.attribute("number")) else {
            die("<field> without name or number")
        };
        let Ok(num) = num.parse::<u32>() else {
            die(&format!("field {name} has a non-numeric number"))
        };
        if number_of.insert(name, num).is_some() {
            die(&format!("field name {name} appears twice"));
        }
        type_of.insert(name, f.attribute("type").unwrap_or(""));
    }

    // ---- header tags -------------------------------------------------------
    let Some(header_el) = child(root, "header") else {
        die("<header> section missing")
    };
    // Descends into <group>. The FIX 4.4 header holds one — NoHops(627) with
    // HopCompID(628), HopSendingTime(629), HopRefID(630) — and all four are
    // header fields. Taking only direct <field> children yields 26 instead of
    // 30, and the four missing ones would sort into the BODY when writing,
    // which is non-negotiable 5's exact failure mode. No acceptance definition
    // carries a hop, so nothing in the 59 would ever notice.
    let mut header: BTreeSet<u32> = BTreeSet::new();
    collect_header(header_el, &number_of, &mut header);

    // ---- DATA -> LENGTH, matched by NAME, never by tag-1 -------------------
    let mut data_len: BTreeMap<u32, u32> = BTreeMap::new();
    for (&name, &ty) in &type_of {
        if ty != "DATA" {
            continue;
        }
        let candidate = [format!("{name}Len"), format!("{name}Length")]
            .into_iter()
            .find_map(|c| number_of.get(c.as_str()).copied());
        match candidate {
            Some(len_tag) => {
                data_len.insert(number_of[name], len_tag);
            }
            // Not a warning. A DATA field with no length field cannot be parsed
            // at all — the parser would scan for 0x01 inside binary content.
            None => die(&format!(
                "DATA field {name} has no matching {name}Len or {name}Length field.\n\
                 A DATA field whose length is unknown cannot be parsed: its value may\n\
                 contain 0x01. Refusing to generate a table that would parse it wrongly."
            )),
        }
    }

    // ---- required fields, per message. NO component recursion --------------
    // Deliberate, and it is wrong for 21 of the 93 messages. See the plan's
    // revision of 2026-08-28 and STATUS.md open item 8.
    let Some(messages_el) = child(root, "messages") else {
        die("<messages> section missing")
    };
    let mut required: Vec<(String, Vec<u32>)> = Vec::new();
    let mut msg_consts: Vec<(String, String)> = Vec::new();
    for m in messages_el.children().filter(|n| n.has_tag_name("message")) {
        let (Some(name), Some(mt)) = (m.attribute("name"), m.attribute("msgtype")) else {
            die("<message> without name or msgtype")
        };
        msg_consts.push((screaming(name), mt.to_string()));

        let mut tags: Vec<u32> = m
            .children()
            .filter(|c| c.has_tag_name("field") || c.has_tag_name("group"))
            .filter(|c| c.attribute("required") == Some("Y"))
            .filter_map(|c| c.attribute("name"))
            .map(|n| match number_of.get(n) {
                Some(t) => *t,
                None => die(&format!("message {name} names unknown field {n}")),
            })
            .collect();
        tags.sort_unstable();
        tags.dedup();
        if !tags.is_empty() {
            required.push((mt.to_string(), tags));
        }
    }

    // ---- emit --------------------------------------------------------------
    let mut o = String::with_capacity(96 * 1024);
    o.push_str("// @generated by crates/dict/build.rs from the QuickFIX FIX 4.4 XML.\n");
    o.push_str("// Do not edit. Regenerate by touching the XML or the build script.\n\n");

    o.push_str("/// Field tag numbers, by name.\npub mod tag {\n");
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    for (name, num) in &number_of {
        let c = screaming(name);
        if let Some(prev) = seen.insert(c.clone(), name) {
            die(&format!("fields {prev} and {name} both become tag::{c}"));
        }
        let _ = writeln!(o, "    pub const {c}: u32 = {num};");
    }
    o.push_str("}\n\n");

    o.push_str(
        "/// Message type values, by name. Multi-byte: FIX 4.4 uses AA..BH.\npub mod msg_type {\n",
    );
    let mut seen2: BTreeSet<String> = BTreeSet::new();
    for (c, mt) in &msg_consts {
        if !seen2.insert(c.clone()) {
            die(&format!("two messages both become msg_type::{c}"));
        }
        let _ = writeln!(o, "    pub const {c}: &[u8] = b\"{mt}\";");
    }
    o.push_str("}\n\n");

    let _ = writeln!(
        o,
        "/// The {} header tags, from the XML `<header>` section.\n\
         #[inline]\npub const fn is_header(tag: u32) -> bool {{\n    matches!(tag, {})\n}}\n",
        header.len(),
        header
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(" | ")
    );

    o.push_str(
        "/// For a DATA field, the tag of its length field.\n\
         ///\n\
         /// Matched by NAME, not by `tag - 1`: Signature(89) takes\n\
         /// SignatureLength(93), and 15 of the 16 DATA fields would agree with the\n\
         /// arithmetic rule while that one silently would not.\n\
         #[inline]\npub const fn data_length_tag(tag: u32) -> Option<u32> {\n    match tag {\n",
    );
    for (d, l) in &data_len {
        let _ = writeln!(o, "        {d} => Some({l}),");
    }
    o.push_str("        _ => None,\n    }\n}\n\n");

    o.push_str(
        "/// Fields a message type requires.\n\
         ///\n\
         /// **Incomplete, knowingly.** Only `required='Y'` children of `<message>`\n\
         /// are counted; `<component>` is not descended into, which is wrong for 21\n\
         /// of the 93 message types. An unknown message type is indistinguishable\n\
         /// from one with no required fields — both give `&[]`. Nothing calls this\n\
         /// yet, and nothing should until the repeating-groups plan lands component\n\
         /// recursion. STATUS.md open item 8.\n\
         #[inline]\npub fn required(msg_type: &[u8]) -> &'static [u32] {\n    match msg_type {\n",
    );
    for (mt, tags) in &required {
        let list = tags
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(o, "        b\"{mt}\" => &[{list}],");
    }
    o.push_str("        _ => &[],\n    }\n}\n");

    o
}

/// Every tag under `<header>`, descending into groups. The group's own counter
/// tag counts too: it is the field that appears on the wire.
fn collect_header(
    el: roxmltree::Node<'_, '_>,
    number_of: &BTreeMap<&str, u32>,
    out: &mut BTreeSet<u32>,
) {
    for c in el.children() {
        if !(c.has_tag_name("field") || c.has_tag_name("group")) {
            continue;
        }
        let Some(name) = c.attribute("name") else {
            die("header entry without a name")
        };
        match number_of.get(name) {
            Some(t) => {
                out.insert(*t);
            }
            None => die(&format!("header names unknown field {name}")),
        }
        if c.has_tag_name("group") {
            collect_header(c, number_of, out);
        }
    }
}

/// `ClOrdID` -> `CL_ORD_ID`, `NoMDEntries` -> `NO_MD_ENTRIES`.
///
/// A `_` goes before an uppercase letter that follows a lowercase or digit, and
/// before the last uppercase of an acronym run when a lowercase follows it.
fn screaming(name: &str) -> String {
    let ch: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 8);
    for i in 0..ch.len() {
        let c = ch[i];
        if i > 0 && c.is_ascii_uppercase() {
            let prev_lower = ch[i - 1].is_ascii_lowercase() || ch[i - 1].is_ascii_digit();
            let next_lower = ch.get(i + 1).is_some_and(char::is_ascii_lowercase);
            if prev_lower || (ch[i - 1].is_ascii_uppercase() && next_lower) {
                out.push('_');
            }
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}
