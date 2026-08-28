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

    // ---- required fields, per message, descending into components ---------
    // A `required='Y'` component contributes its own `required='Y'` fields, and
    // nothing else: Instrument is required in NewOrderSingle while every field
    // inside it, Symbol(55) included, is optional. "The message requires an
    // Instrument" and "the message requires a Symbol" are different statements.
    let components: BTreeMap<&str, roxmltree::Node<'_, '_>> = match child(root, "components") {
        Some(el) => el
            .children()
            .filter(|n| n.has_tag_name("component"))
            .filter_map(|n| n.attribute("name").map(|k| (k, n)))
            .collect(),
        None => BTreeMap::new(),
    };
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

        let mut set = BTreeSet::new();
        collect_required(m, &components, &number_of, name, &mut set, &mut Vec::new());
        let mut tags: Vec<u32> = set.into_iter().collect();
        tags.sort_unstable();
        if !tags.is_empty() {
            required.push((mt.to_string(), tags));
        }
    }

    // ---- repeating groups, per message -------------------------------------
    // Keyed by (msg_type, counter). Never by counter alone: NoMDEntries(268)
    // takes MDEntryType(269) in a snapshot and MDUpdateAction(279) in an
    // incremental refresh, and an incremental refresh is the highest-volume
    // message there is. Three more counters behave the same way.
    let mut groups: BTreeMap<(String, u32), Vec<u32>> = BTreeMap::new();
    let mut positions: usize = 0usize;
    for m in messages_el.children().filter(|n| n.has_tag_name("message")) {
        let Some(mt) = m.attribute("msgtype") else {
            die("<message> without msgtype")
        };
        collect_groups(
            m,
            &components,
            &number_of,
            mt,
            &mut groups,
            &mut positions,
            &mut Vec::new(),
        );
    }
    // The header's one group, NoHops(627), can appear in ANY message, so it is
    // keyed under the empty message type and emitted without a msg_type arm.
    collect_groups(
        header_el,
        &components,
        &number_of,
        "",
        &mut groups,
        &mut positions,
        &mut Vec::new(),
    );

    // Distinct member lists, deduplicated: many messages share a group verbatim.
    let mut lists: Vec<Vec<u32>> = Vec::new();
    let mut list_id: BTreeMap<Vec<u32>, usize> = BTreeMap::new();
    let mut by_counter: BTreeMap<u32, BTreeMap<usize, Vec<String>>> = BTreeMap::new();
    for ((mt, counter), members) in &groups {
        let id = *list_id.entry(members.clone()).or_insert_with(|| {
            lists.push(members.clone());
            lists.len() - 1
        });
        by_counter
            .entry(*counter)
            .or_default()
            .entry(id)
            .or_default()
            .push(mt.clone());
    }
    for (counter, per) in &by_counter {
        let owners: Vec<&String> = per.values().flatten().collect();
        if owners.iter().any(|m| m.is_empty()) && owners.len() > 1 {
            die(&format!(
                "counter {counter} is declared in <header> and in a message.\n\
                 A header group applies to every message, so it cannot also be\n\
                 keyed per message. Refusing to emit a table that answers one\n\
                 of the two wrongly."
            ));
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
         /// Descends into `required='Y'` components, transitively. A required\n\
         /// `<group>` contributes its counter tag, which is the field that appears\n\
         /// on the wire; what a group entry requires is a different question.\n\
         ///\n\
         /// A required component does NOT make its fields required — Instrument is\n\
         /// required in NewOrderSingle and Symbol(55) inside it is not.\n\
         ///\n\
         /// **Remaining hole:** an unknown message type is indistinguishable from\n\
         /// one with no required fields; both give `&[]`.\n\
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
    o.push_str("        _ => &[],\n    }\n}\n\n");

    // ---- group tables ------------------------------------------------------
    let _ = writeln!(
        o,
        "/// Distinct group counter tags in FIX 4.4, `NoHops(627)` from the\n\
         /// header included.\npub const GROUP_COUNTERS: usize = {};\n",
        by_counter.len()
    );
    let _ = writeln!(
        o,
        "/// Group positions once `<component>` references are expanded: the\n\
         /// number of places a group can appear across all 93 messages plus the\n\
         /// header. Larger than the 93 `<group>` declarations because a component\n\
         /// holding a group is referenced from many messages.\n\
         pub const GROUP_POSITIONS: usize = {positions};\n"
    );
    for (i, l) in lists.iter().enumerate() {
        let items = l.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
        let _ = writeln!(o, "static G{i}: [u32; {}] = [{items}];", l.len());
    }
    let _ = writeln!(
        o,
        "\n/// Every `(msg_type, counter)` pair the dictionary declares, sorted.\n\
         ///\n\
         /// Exists so a test can enumerate the groups instead of naming a few by\n\
         /// hand: \"covers every group\" is a claim, and this is what makes it\n\
         /// checkable. A validator walking a message would want the same list.\n\
         pub static GROUP_KEYS: [(&[u8], u32); {}] = [",
        groups.len()
    );
    for (mt, counter) in groups.keys() {
        let _ = writeln!(o, "    (b\"{mt}\", {counter}),");
    }
    o.push_str("];\n");
    o.push_str(
        "\n/// Every tag that may appear inside a group, in declaration order,\n\
         /// delimiter first.\n\
         ///\n\
         /// One table serves all three `Dictionary` group methods, so they cannot\n\
         /// disagree: `group_delimiter` is this list's head and `group_order` is\n\
         /// this list. A second table would be a second thing to keep in step.\n\
         ///\n\
         /// Matched on the counter first — a `u32` jump table — then on the\n\
         /// message. A pair the dictionary does not declare gives `&[]`, so\n\
         /// `268` in a NewOrderSingle is not answered with the snapshot's\n\
         /// delimiter.\n\
         #[inline]\n#[must_use]\npub fn group_members(msg_type: &[u8], counter: u32) -> &'static [u32] {\n    match counter {\n",
    );
    for (counter, per) in &by_counter {
        let header_owned = per.values().flatten().any(|m| m.is_empty());
        if header_owned {
            let id = *per.keys().next().unwrap_or(&0);
            let _ = writeln!(o, "        {counter} => &G{id}, // <header>");
            continue;
        }
        let _ = writeln!(o, "        {counter} => match msg_type {{");
        for (id, mts) in per {
            let pats = mts
                .iter()
                .map(|m| format!("b\"{m}\""))
                .collect::<Vec<_>>()
                .join(" | ");
            let _ = writeln!(o, "            {pats} => &G{id},");
        }
        o.push_str("            _ => &[],\n        },\n");
    }
    o.push_str("        _ => &[],\n    }\n}\n");

    o
}

/// Required tags of an element, descending into required components.
///
/// A `<group required='Y'>` contributes its **counter** tag: that is the field
/// that appears on the wire. What is inside the group is required per entry, not
/// per message, and belongs to a different question.
///
/// `path` guards against a component cycle. FIX 4.4 has none — measured, deepest
/// nesting is 5 — but a generator that loops on a malformed dictionary hangs the
/// build with no message.
fn collect_required<'a>(
    el: roxmltree::Node<'a, '_>,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, '_>>,
    number_of: &BTreeMap<&str, u32>,
    msg: &str,
    out: &mut BTreeSet<u32>,
    path: &mut Vec<&'a str>,
) {
    for c in el.children() {
        if c.attribute("required") != Some("Y") {
            continue;
        }
        let Some(name) = c.attribute("name") else {
            continue;
        };
        if c.has_tag_name("field") || c.has_tag_name("group") {
            match number_of.get(name) {
                Some(t) => {
                    out.insert(*t);
                }
                None => die(&format!("message {msg} names unknown field {name}")),
            }
        } else if c.has_tag_name("component") {
            let Some(def) = components.get(name) else {
                die(&format!("message {msg} names unknown component {name}"))
            };
            if path.contains(&name) {
                die(&format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_required(*def, components, number_of, msg, out, path);
            path.pop();
        }
    }
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

/// Every tag a group can hold, in declaration order, components spliced in
/// where they are referenced.
///
/// A nested `<group>` contributes its counter tag and nothing else: the counter
/// is the field that appears at this level of the wire, and what the nested
/// entries hold is the nested group's own question.
fn collect_members<'a>(
    el: roxmltree::Node<'a, '_>,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, '_>>,
    number_of: &BTreeMap<&str, u32>,
    ctx: &str,
    out: &mut Vec<u32>,
    path: &mut Vec<&'a str>,
) {
    for c in el.children() {
        let Some(name) = c.attribute("name") else {
            continue;
        };
        if c.has_tag_name("field") || c.has_tag_name("group") {
            match number_of.get(name) {
                Some(t) => out.push(*t),
                None => die(&format!("{ctx} names unknown field {name}")),
            }
        } else if c.has_tag_name("component") {
            let Some(def) = components.get(name) else {
                die(&format!("{ctx} names unknown component {name}"))
            };
            if path.contains(&name) {
                die(&format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_members(*def, components, number_of, ctx, out, path);
            path.pop();
        }
    }
}

/// Registers every group reachable from `el` under message type `mt`,
/// descending through components and through nested groups.
///
/// A group reached only through a component is the common case, not the
/// exception: `NoTradingSessions(386)` reaches NewOrderSingle solely through
/// `TrdgSesGrp`, so a walker that reads `<message>` children alone finds
/// nothing for it.
fn collect_groups<'a>(
    el: roxmltree::Node<'a, '_>,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, '_>>,
    number_of: &BTreeMap<&str, u32>,
    mt: &str,
    groups: &mut BTreeMap<(String, u32), Vec<u32>>,
    positions: &mut usize,
    path: &mut Vec<&'a str>,
) {
    for c in el.children() {
        let Some(name) = c.attribute("name") else {
            continue;
        };
        if c.has_tag_name("group") {
            let Some(&counter) = number_of.get(name) else {
                die(&format!("message {mt} names unknown group counter {name}"))
            };
            let mut members = Vec::new();
            collect_members(
                c,
                components,
                number_of,
                &format!("group {name}"),
                &mut members,
                &mut Vec::new(),
            );
            if members.is_empty() {
                die(&format!("group {name} in message {mt} has no members"));
            }
            *positions += 1;
            match groups.entry((mt.to_string(), counter)) {
                std::collections::btree_map::Entry::Vacant(v) => {
                    v.insert(members);
                }
                std::collections::btree_map::Entry::Occupied(prev) => {
                    if prev.get() != &members {
                        die(&format!(
                            "counter {counter} appears twice in message {mt} with\n\
                             different members: {:?} then {members:?}.\n\
                             A (msg_type, counter) key cannot answer both.",
                            prev.get()
                        ));
                    }
                }
            }
            collect_groups(c, components, number_of, mt, groups, positions, path);
        } else if c.has_tag_name("component") {
            let Some(def) = components.get(name) else {
                die(&format!("message {mt} names unknown component {name}"))
            };
            if path.contains(&name) {
                die(&format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_groups(*def, components, number_of, mt, groups, positions, path);
            path.pop();
        }
    }
}
