//! Writing a [`Spec`] out as Rust source: the text `build.rs` writes to
//! `$OUT_DIR`, byte for byte what it wrote before the generator moved here
//! (ADR-0207 decision 2).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::error::{GenError, refuse};
use super::model::{Spec, collect_allowed, collect_groups, collect_header, collect_required};
use crate::field_type::FieldType;

/// Sets bit `tag` of a bitset. Every caller sizes the bitset from the highest
/// tag it will set, so a tag past the end is this generator's own bug, and is
/// refused rather than indexed.
fn set_bit(bits: &mut [u64], tag: u32) -> Result<(), GenError> {
    let words = bits.len();
    match bits.get_mut(tag as usize / 64) {
        Some(word) => {
            *word |= 1u64 << (tag % 64);
            Ok(())
        }
        None => refuse(format!(
            "tag {tag} is past the end of a {words}-word bitset; the generator sized it wrong"
        )),
    }
}

/// The number of a field this table has a type or an enumeration for. Every
/// such field was numbered by `collect_fields`, so `None` is this generator's
/// own bug.
fn number(number_of: &BTreeMap<&str, u32>, name: &str) -> Result<u32, GenError> {
    match number_of.get(name) {
        Some(&t) => Ok(t),
        None => refuse(format!("field {name} has a type but no number")),
    }
}

/// The transport layer's own tag set and message list — the pair table only.
///
/// ADR-0084 decision 1. `14a_BadField.def` sends `999=HI` on a `35=0` Heartbeat
/// and expects `373=0`, *Invalid tag number*; `999` is `LegUnitOfMeasure` in
/// `FIX50SP2.xml` and absent from `FIXT11.xml`, so the merged bitset answers
/// `true` and the engine would send `373=2`. Both QuickFIX engines validate an
/// admin body against the transport dictionary alone.
///
/// Both halves come out of the **one** transport document, so "defined by the
/// transport file" and "a message of the transport file" are the same file by
/// construction — `DESIGN.md` D3, the rule lives in a table and never at a call
/// site. A single-file table has one layer and needs neither, which is why this
/// is emitted here rather than in `emit`.
#[cfg(feature = "fix50sp2")]
pub(super) fn emit_transport_layer(tags: &[u32], msg_types: &[&str]) -> Result<String, GenError> {
    let max_tag = tags.iter().copied().max().unwrap_or(0);
    let words = (max_tag as usize / 64) + 1;
    let mut bits = vec![0u64; words];
    for &t in tags {
        set_bit(&mut bits, t)?;
    }
    if msg_types.is_empty() {
        return refuse("FIXT11.xml: <messages> is empty; there is no transport layer to emit");
    }
    let mut o = String::with_capacity(4 * 1024);
    let _ = writeln!(
        o,
        "/// Tags the **transport** file defines, as a bitset over 0..={max_tag}.\n\
         ///\n\
         /// {} fields, the highest being tag {max_tag}. Separate from\n\
         /// `DEFINED_TAGS`, which is the merged set: a session message is\n\
         /// checked against this one (ADR-0084 decision 1).\n\
         static TRANSPORT_DEFINED_TAGS: [u64; {words}] = [{}];\n\
         \n\
         /// Whether the transport file defines this tag at all.\n\
         ///\n\
         /// The `373=0` question for a message of the transport layer.\n\
         /// [`is_defined_tag`] is the same question over both files.\n\
         #[inline]\n\
         #[must_use]\n\
         // Same shape as `is_defined_tag`, and `const fn` rules out `.get()`:\n\
         // neither `slice::get` nor `Option::is_some_and` is const. The bound is\n\
         // the left half of the `&&` on the line below. STATUS.md item 55.\n\
         #[allow(clippy::indexing_slicing)]\n\
         pub const fn is_transport_tag(tag: u32) -> bool {{\n\
         \x20   let word = (tag / 64) as usize;\n\
         \x20   word < TRANSPORT_DEFINED_TAGS.len()\n\
         \x20       && (TRANSPORT_DEFINED_TAGS[word] >> (tag % 64)) & 1 == 1\n\
         }}\n",
        tags.len(),
        bits.iter()
            .map(|w| format!("0x{w:016x}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    let _ = writeln!(
        o,
        "/// Whether the transport file defines this message type.\n\
         ///\n\
         /// The {} of them, read from `FIXT11.xml`'s own `<messages>` — never\n\
         /// from a list written beside a call site, which is what `DESIGN.md` D3\n\
         /// forbids and what would disagree with this file the day either\n\
         /// changes.\n\
         #[inline]\n\
         #[must_use]\n\
         pub fn is_transport_message(msg_type: &[u8]) -> bool {{\n\
         \x20   matches!(msg_type, {})\n\
         }}\n",
        msg_types.len(),
        msg_types
            .iter()
            .map(|mt| format!("b\"{mt}\""))
            .collect::<Vec<_>>()
            .join(" | "),
    );
    Ok(o)
}

pub(super) fn emit(spec: &Spec<'_, '_>) -> Result<String, GenError> {
    let number_of = &spec.number_of;
    let type_of = &spec.type_of;
    let enum_of = &spec.enum_of;
    let components = &spec.components;
    let header_el = spec.header;
    let dialect = spec.dialect;

    // ---- header tags -------------------------------------------------------
    // Descends into <group>. The FIX 4.4 header holds one — NoHops(627) with
    // HopCompID(628), HopSendingTime(629), HopRefID(630) — and all four are
    // header fields. Taking only direct <field> children yields 26 instead of
    // 30, and the four missing ones would sort into the BODY when writing,
    // which is non-negotiable 5's exact failure mode. No acceptance definition
    // carries a hop, so nothing in the 59 would ever notice. FIXT 1.1's header
    // is the same shape: 29 direct fields and the same group.
    let mut header: BTreeSet<u32> = BTreeSet::new();
    collect_header(header_el, number_of, &mut header)?;

    // ---- DATA -> LENGTH, matched by NAME, never by tag-1 -------------------
    // `XMLDATA` is the same variant as `DATA` (ADR-0083 decision 1), so it is
    // paired by the same rule — all 8 of FIX 5.0 SP2's pair by name, measured.
    let mut data_len: BTreeMap<u32, u32> = BTreeMap::new();
    let mut exception_used = vec![false; spec.length_exceptions.len()];
    for (&name, &ty) in type_of {
        if ty != FieldType::Data {
            continue;
        }
        let tag = number(number_of, name)?;
        let candidate = [format!("{name}Len"), format!("{name}Length")]
            .into_iter()
            .find_map(|c| number_of.get(c.as_str()).copied());
        if let Some(len_tag) = candidate {
            data_len.insert(tag, len_tag);
            continue;
        }
        // The name rule found nothing. Only now is the exception table
        // consulted, and it is checked in both directions — ADR-0083
        // decision 5.
        match spec
            .length_exceptions
            .iter()
            .enumerate()
            .find(|(_, (data_tag, _, _, _))| *data_tag == tag)
        {
            Some((i, &(data_tag, length_tag, data_name, length_name))) => {
                if data_name != name {
                    return refuse(format!(
                        "length exception for tag {data_tag} names field {data_name},\n\
                         but the dictionary calls tag {tag} {name}."
                    ));
                }
                if number_of.get(length_name) != Some(&length_tag) {
                    return refuse(format!(
                        "length exception for {data_name} names {length_name} as tag\n\
                         {length_tag}, which this dictionary does not carry under that\n\
                         number. An exception the XML does not support is a wrong pairing."
                    ));
                }
                if let Some(used) = exception_used.get_mut(i) {
                    *used = true;
                }
                data_len.insert(data_tag, length_tag);
            }
            // Not a warning. A DATA field with no length field cannot be parsed
            // at all — the parser would scan for 0x01 inside binary content.
            None => {
                return refuse(format!(
                    "DATA field {name} has no matching {name}Len or {name}Length field.\n\
                 A DATA field whose length is unknown cannot be parsed: its value may\n\
                 contain 0x01. Refusing to generate a table that would parse it wrongly."
                ));
            }
        }
    }
    for ((data_tag, _, data_name, _), used) in spec.length_exceptions.iter().zip(&exception_used) {
        if !*used {
            return refuse(format!(
                "length exception {data_name}({data_tag}) went unused.\n\
                 Either the field is not in this dictionary, or the {{name}}Len /\n\
                 {{name}}Length rule found its length field without help. An exception\n\
                 nobody needs is a rule nobody checked — delete the row. ADR-0083\n\
                 decision 5."
            ));
        }
    }

    // ---- required fields, per message, descending into components ---------
    // A `required='Y'` component contributes its own `required='Y'` fields, and
    // nothing else: Instrument is required in NewOrderSingle while every field
    // inside it, Symbol(55) included, is optional. "The message requires an
    // Instrument" and "the message requires a Symbol" are different statements.
    let mut required: Vec<(String, Vec<u32>)> = Vec::new();
    let mut msg_consts: Vec<(String, String)> = Vec::new();
    let mut msg_types: BTreeSet<String> = BTreeSet::new();
    let mut admin_types: BTreeSet<String> = BTreeSet::new();
    let mut allowed: Vec<(String, BTreeSet<u32>)> = Vec::new();
    for &m in &spec.messages {
        let (Some(name), Some(mt)) = (m.attribute("name"), m.attribute("msgtype")) else {
            return refuse("<message> without name or msgtype");
        };
        msg_consts.push((screaming(name), mt.to_string()));
        if !msg_types.insert(mt.to_string()) {
            return refuse(format!("two messages share msgtype {mt}"));
        }

        // `msgcat` is the dictionary's own answer to "is this administrative".
        // A `<message>` without it stops the build, exactly as a missing `name`
        // or `msgtype` does above: a default would be this generator inventing
        // the answer, and the one place it must not be invented is the place
        // `DESIGN.md` D3 points at.
        match m.attribute("msgcat") {
            Some("admin") => {
                admin_types.insert(mt.to_string());
            }
            Some("app") => {}
            Some(other) => {
                return refuse(format!(
                    "message {name} ({mt}) has msgcat={other:?}; the only categories\n\
                 this generator knows are 'admin' and 'app'."
                ));
            }
            None => {
                return refuse(format!(
                    "message {name} ({mt}) has no msgcat attribute.\n\
                 `is_admin` is generated from it, so a message without one has no\n\
                 answer — and guessing a default here is the hand-written list\n\
                 beside a call site that DESIGN.md D3 forbids, only hidden in a\n\
                 build script."
                ));
            }
        }

        let mut set = BTreeSet::new();
        collect_required(m, components, number_of, name, &mut set, &mut Vec::new())?;
        let mut tags: Vec<u32> = set.into_iter().collect();
        tags.sort_unstable();
        if !tags.is_empty() {
            required.push((mt.to_string(), tags));
        }

        let mut body = BTreeSet::new();
        collect_allowed(m, components, number_of, name, &mut body, &mut Vec::new())?;
        allowed.push((mt.to_string(), body));
    }

    // ---- repeating groups, per message -------------------------------------
    // Keyed by (msg_type, counter). Never by counter alone: NoMDEntries(268)
    // takes MDEntryType(269) in a snapshot and MDUpdateAction(279) in an
    // incremental refresh, and an incremental refresh is the highest-volume
    // message there is. Three more counters behave the same way.
    let mut groups: BTreeMap<(String, u32), Vec<u32>> = BTreeMap::new();
    let mut positions: usize = 0usize;
    for &m in &spec.messages {
        let Some(mt) = m.attribute("msgtype") else {
            return refuse("<message> without msgtype");
        };
        collect_groups(
            m,
            components,
            number_of,
            mt,
            &mut groups,
            &mut positions,
            &mut Vec::new(),
        )?;
    }
    // The header's one group, NoHops(627), can appear in ANY message, so it is
    // keyed under the empty message type and emitted without a msg_type arm.
    collect_groups(
        header_el,
        components,
        number_of,
        "",
        &mut groups,
        &mut positions,
        &mut Vec::new(),
    )?;

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
            return refuse(format!(
                "counter {counter} is declared in <header> and in a message.\n\
                 A header group applies to every message, so it cannot also be\n\
                 keyed per message. Refusing to emit a table that answers one\n\
                 of the two wrongly."
            ));
        }
    }

    // ---- emit --------------------------------------------------------------
    let mut o = String::with_capacity(96 * 1024);
    let _ = writeln!(
        o,
        "// @generated by crates/dict/build.rs from {}.",
        spec.source
    );
    o.push_str("// Do not edit. Regenerate by touching the XML or the build script.\n\n");

    o.push_str("/// Field tag numbers, by name.\npub mod tag {\n");
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    for (name, num) in number_of {
        let c = screaming(name);
        if let Some(prev) = seen.insert(c.clone(), name) {
            return refuse(format!("fields {prev} and {name} both become tag::{c}"));
        }
        let _ = writeln!(o, "    pub const {c}: u32 = {num};");
    }
    o.push_str("}\n\n");

    o.push_str(
        "/// Message type values, by name. Multi-byte: this dialect uses values\n\
         /// of more than one character.\npub mod msg_type {\n",
    );
    let mut seen2: BTreeSet<String> = BTreeSet::new();
    for (c, mt) in &msg_consts {
        if !seen2.insert(c.clone()) {
            return refuse(format!("two messages both become msg_type::{c}"));
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

    // One bit per tag over 0..=max_tag. Shared by `ALLOWED` and `DEFINED_TAGS`
    // so the two tables cannot end up different widths.
    let max_tag = number_of.values().copied().max().unwrap_or(0);
    let words = (max_tag as usize / 64) + 1;
    // ---- required_header ---------------------------------------------------
    // `required()` answers for a message BODY, and its own doc comment says so.
    // `14b_RequiredFieldMissing.def` sends a Heartbeat with no TargetCompID and
    // expects `373=1` with `371=56` — a header field, which `required(b"0")`
    // does not and should not mention.
    let mut header_required: BTreeSet<u32> = BTreeSet::new();
    for c in header_el.children() {
        if c.attribute("required") != Some("Y") {
            continue;
        }
        if let Some(name) = c.attribute("name")
            && let Some(&t) = number_of.get(name)
        {
            header_required.insert(t);
        }
    }
    let _ = writeln!(
        o,
        "/// Header fields every message must carry, whatever its type.\n\
         ///\n\
         /// {} of them. Separate from [`required`], which answers for a message\n\
         /// body: the two are different questions and a caller asks both.\n\
         #[inline]\n\
         #[must_use]\n\
         pub const fn required_header() -> &'static [u32] {{\n\
         \x20   &[{}]\n\
         }}\n",
        header_required.len(),
        header_required
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", "),
    );

    // ---- enum_allows: the values each enumerated field will take -----------
    // `None` for a field with no enumeration, never `Some(true)`: the two mean
    // different things and confusing them makes `373=5` fire on nothing, which
    // no acceptance definition would notice.
    //
    // Value lists are deduplicated — the Y/N pair alone appears 30 times.
    let mut enum_lists: Vec<Vec<&str>> = Vec::new();
    let mut enum_index: BTreeMap<u32, usize> = BTreeMap::new();
    let mut enum_values = 0usize;
    for (&name, values) in enum_of {
        enum_values += values.len();
        let at = enum_lists
            .iter()
            .position(|v| v == values)
            .unwrap_or_else(|| {
                enum_lists.push(values.clone());
                enum_lists.len() - 1
            });
        enum_index.insert(number(number_of, name)?, at);
    }
    for (i, values) in enum_lists.iter().enumerate() {
        let _ = writeln!(
            o,
            "static V{i}: [&[u8]; {}] = [{}];",
            values.len(),
            values
                .iter()
                .map(|v| format!("b\"{v}\""))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let _ = writeln!(
        o,
        "\n/// The values an enumerated field will take. `None` means the field is\n\
         /// not enumerated, or the tag is not {dialect} at all.\n\
         ///\n\
         /// `[measured]` {} enumerated fields, {enum_values} values, {} distinct\n\
         /// lists after deduplication.\n\
         #[inline]\n\
         #[must_use]\n\
         pub fn enum_allows(tag: u32, value: &[u8]) -> Option<bool> {{\n\
         \x20   let list: &[&[u8]] = match tag {{",
        enum_of.len(),
        enum_lists.len(),
    );
    for (tag, at) in &enum_index {
        let _ = writeln!(o, "        {tag} => &V{at},");
    }
    o.push_str("        _ => return None,\n    };\n");
    // ADR-0083 decision 1, second rule: on a multi-value type the check is
    // **per token** — the value is split on single spaces and every token must
    // be in the list, which is what QuickFIX C++ (`isFieldValue`) and
    // QuickFIX/J (`DataDictionary` line 526) both do. Emitted only for a
    // table built to that rule, which since ADR-0084 decision 2's row is both
    // of them.
    let multi: Vec<u32> = if spec.per_token_enums {
        enum_index
            .keys()
            .copied()
            .filter(|tag| {
                type_of.iter().any(|(name, ty)| {
                    number_of.get(name) == Some(tag)
                        && matches!(
                            ty,
                            FieldType::MultipleValueString | FieldType::MultipleCharValue
                        )
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    if !multi.is_empty() {
        let _ = writeln!(
            o,
            "    // `18=2 A` is one legal MULTIPLECHARVALUE, not one illegal value.\n\
             \x20   if matches!(tag, {}) {{\n\
             \x20       return Some(value.split(|&b| b == b' ').all(|t| list.contains(&t)));\n\
             \x20   }}",
            multi
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    o.push_str("    Some(list.contains(&value))\n}\n\n");

    // ---- allows: one bitset per message type -------------------------------
    // A bitset, not a sorted list: 15 words against a binary search over up to
    // 300 tags, and the session asks once per field of every message it
    // validates. Header and trailer are folded in at generation time so the
    // call site asks one question instead of three.
    let mut trailer: BTreeSet<u32> = BTreeSet::new();
    collect_header(spec.trailer, number_of, &mut trailer)?;
    let mut allow_bits: Vec<(String, Vec<u64>)> = Vec::new();
    let mut body_pairs = 0usize;
    for (mt, body) in &allowed {
        body_pairs += body.len();
        let mut bits = vec![0u64; words];
        for t in body.iter().chain(header.iter()).chain(trailer.iter()) {
            set_bit(&mut bits, *t)?;
        }
        allow_bits.push((mt.clone(), bits));
    }
    let _ = writeln!(
        o,
        "/// Tags each message type may carry, as a bitset over 0..={max_tag}.\n\
         ///\n\
         /// `[measured]` {body_pairs} (message, tag) pairs from the message bodies,\n\
         /// plus the {} header and {} trailer tags folded into every one — so a\n\
         /// caller asks once, not three times. {} messages x {words} words.\n\
         static ALLOWED: [(&[u8], [u64; {words}]); {}] = [",
        header.len(),
        trailer.len(),
        allow_bits.len(),
        allow_bits.len(),
    );
    for (mt, bits) in &allow_bits {
        let _ = writeln!(
            o,
            "    (b\"{mt}\", [{}]),",
            bits.iter()
                .map(|w| format!("0x{w:016x}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    o.push_str("];\n\n");
    o.push_str(
        "/// Whether this message type may carry this tag. Answers `373=2`.\n\
         ///\n\
         /// An unknown message type allows **nothing**. It is answered by\n\
         /// `is_msg_type` and `373=11` before this is ever asked, but the safe\n\
         /// answer to a question that should not have been asked is no.\n\
         #[inline]\n\
         #[must_use]\n\
         // The bound is on the line below the index. clippy cannot see that, and\n\
         // `indexing_slicing` is denied workspace-wide since 2026-09-08 — so the\n\
         // allow is emitted here, on the function, rather than at the crate root\n\
         // where it would silence hand-written code too. STATUS.md item 55.\n\
         #[allow(clippy::indexing_slicing)]\n\
         pub fn allows(msg_type: &[u8], tag: u32) -> bool {\n\
         \x20   let word = (tag / 64) as usize;\n\
         \x20   ALLOWED\n\
         \x20       .iter()\n\
         \x20       .find(|(mt, _)| *mt == msg_type)\n\
         \x20       .is_some_and(|(_, bits)| word < bits.len() && (bits[word] >> (tag % 64)) & 1 == 1)\n\
         }\n\n",
    );

    // ---- field_type: the declared type of every tag ------------------------
    // The type *names* came from the XML and were turned into variants by
    // `collect_fields`, which is where an unknown one stops the build; what
    // each type accepts is `src/field_type.rs`, included above rather than
    // restated here.
    let mut typed: BTreeMap<u32, &'static str> = BTreeMap::new();
    for (&name, &ty) in type_of {
        typed.insert(number(number_of, name)?, ty.as_rust());
    }
    let _ = writeln!(
        o,
        "/// The declared type of a field. `None` means {dialect} has no such tag.\n\
         ///\n\
         /// {} fields across {} types.\n\
         #[inline]\n\
         #[must_use]\n\
         pub const fn field_type(tag: u32) -> Option<crate::FieldType> {{\n\
         \x20   use crate::FieldType::*;\n\
         \x20   Some(match tag {{",
        typed.len(),
        typed.values().collect::<BTreeSet<_>>().len(),
    );
    for (tag, ty) in &typed {
        let _ = writeln!(o, "        {tag} => {ty},");
    }
    o.push_str("        _ => return None,\n    })\n}\n\n");

    // ---- is_defined_tag: a bitset over the whole tag range -----------------
    // A bitset rather than a `matches!` over 912 arms: 15 words of 8 bytes
    // against a jump table, and a shift-and-mask instead of a branch. The
    // acceptance corpus asks this question once per field of every message it
    // rejects, so it sits next to the parse loop.
    let mut bits = vec![0u64; words];
    for &t in number_of.values() {
        set_bit(&mut bits, t)?;
    }
    let _ = writeln!(
        o,
        "/// Tags {dialect} defines, as a bitset over 0..={max_tag}.\n\
         ///\n\
         /// {} fields, the highest being tag {max_tag}. **There is no user-defined\n\
         /// range here.** QuickFIX\'s own `FieldNumbers.h` calls 5000..=9999\n\
         /// user-defined, but `14a_BadField.def` expects `5000=HI` refused as an\n\
         /// invalid tag, so \"defined\" means \"in FIX44.xml\" and nothing else.\n\
         static DEFINED_TAGS: [u64; {words}] = [{}];\n\
         \n\
         /// Whether {dialect} defines this tag at all. Answers `373=0`.\n\
         #[inline]\n\
         #[must_use]\n\
         // Same shape as `allows`, and `const fn` rules out `.get()`: neither\n\
         // `slice::get` nor `Option::is_some_and` is const. The bound is the\n\
         // left half of the `&&` on the line below. STATUS.md item 55.\n\
         #[allow(clippy::indexing_slicing)]\n\
         pub const fn is_defined_tag(tag: u32) -> bool {{\n\
         \x20   let word = (tag / 64) as usize;\n\
         \x20   word < DEFINED_TAGS.len() && (DEFINED_TAGS[word] >> (tag % 64)) & 1 == 1\n\
         }}\n",
        number_of.len(),
        bits.iter()
            .map(|w| format!("0x{w:016x}"))
            .collect::<Vec<_>>()
            .join(", "),
    );

    // ---- is_msg_type -------------------------------------------------------
    let _ = writeln!(
        o,
        "/// Whether this is a {dialect} message type. Answers `373=11`.\n\
         ///\n\
         /// {} of them. `required()` cannot answer this: it gives `&[]` for an\n\
         /// unknown type and for a known one with no required fields alike, which\n\
         /// is the hole its own doc comment names.\n\
         #[inline]\n\
         #[must_use]\n\
         pub fn is_msg_type(msg_type: &[u8]) -> bool {{\n\
         \x20   matches!(msg_type, {})\n\
         }}\n",
        msg_types.len(),
        msg_types
            .iter()
            .map(|mt| format!("b\"{mt}\""))
            .collect::<Vec<_>>()
            .join(" | "),
    );

    // ---- is_admin ----------------------------------------------------------
    if admin_types.is_empty() {
        return refuse(format!(
            "{dialect}: not one <message> carries msgcat='admin'.\n\
             A dictionary with no administrative message would make `is_admin`\n\
             answer `false` for Logon itself. Refusing to emit it."
        ));
    }
    let _ = writeln!(
        o,
        "/// Whether the dictionary files this table is built from call this\n\
         /// message administrative — `msgcat='admin'`, read from the XML.\n\
         ///\n\
         /// {} of the {} {dialect} message types. Never a list beside a call\n\
         /// site: `DESIGN.md` D3, and the reason this exists at all is that such\n\
         /// a list had seven entries where the XML has eight.\n\
         ///\n\
         /// This is a question about the **dictionary**, not about which\n\
         /// messages this engine's session layer answers by itself — those are\n\
         /// two sets and they differ on `35=n`.\n\
         #[inline]\n\
         #[must_use]\n\
         pub fn is_admin(msg_type: &[u8]) -> bool {{\n\
         \x20   matches!(msg_type, {})\n\
         }}\n",
        admin_types.len(),
        msg_types.len(),
        admin_types
            .iter()
            .map(|mt| format!("b\"{mt}\""))
            .collect::<Vec<_>>()
            .join(" | "),
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
        "/// Distinct group counter tags in {dialect}, `NoHops(627)` from the\n\
         /// header included.\npub const GROUP_COUNTERS: usize = {};\n",
        by_counter.len()
    );
    let _ = writeln!(
        o,
        "/// Group positions once `<component>` references are expanded: the\n\
         /// number of places a group can appear across all {} messages plus the\n\
         /// header. Larger than the `<group>` declaration count because a component\n\
         /// holding a group is referenced from many messages.\n\
         pub const GROUP_POSITIONS: usize = {positions};\n",
        spec.messages.len()
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

    Ok(o)
}

/// `ClOrdID` -> `CL_ORD_ID`, `NoMDEntries` -> `NO_MD_ENTRIES`.
///
/// A `_` goes before an uppercase letter that follows a lowercase or digit, and
/// before the last uppercase of an acronym run when a lowercase follows it.
fn screaming(name: &str) -> String {
    let ch: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 8);
    let mut prev: Option<char> = None;
    for (i, &c) in ch.iter().enumerate() {
        if let Some(p) = prev
            && c.is_ascii_uppercase()
        {
            let prev_lower = p.is_ascii_lowercase() || p.is_ascii_digit();
            let next_lower = ch.get(i + 1).is_some_and(char::is_ascii_lowercase);
            if prev_lower || (p.is_ascii_uppercase() && next_lower) {
                out.push('_');
            }
        }
        out.push(c.to_ascii_uppercase());
        prev = Some(c);
    }
    out
}
