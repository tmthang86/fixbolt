//! What one generated table is built from, and the walks over it: required and
//! allowed tags, the header, and repeating groups — each descending through
//! `<component>` references.

use std::collections::{BTreeMap, BTreeSet};

use super::error::{GenError, refuse};
use super::parse::has_members;
use crate::field_type::FieldType;

/// A DATA or XMLDATA field whose length field the `{name}Len` / `{name}Length`
/// rule cannot find: `(data_tag, length_tag, data_name, length_name)`.
///
/// ADR-0083 decision 5. Consulted **only** after the name rule has failed, and
/// checked in both directions — an entry whose names or numbers the XML does
/// not carry, or that the name rule would have found anyway, fails the build.
/// The shape is `interop_quickfix_fields.rs::TYPE_EXEMPTIONS`'s.
pub(super) type LengthException = (u32, u32, &'static str, &'static str);

/// The one row measured on 2026-09-19 at pin `386ce46e`: 74 of FIX 5.0 SP2's 75
/// DATA fields and all 8 of its XMLDATA fields pair by name; this one
/// abbreviates `Security` to `Sec` in its length field's name.
///
/// A `tag - 1` fallback is refused rather than added: twelve SP2 DATA/XMLDATA
/// fields do not sit at `length + 1`, and a fallback pairs the wrong field
/// silently the next time upstream abbreviates a name.
#[cfg(feature = "fix50sp2")]
pub(super) const SP2_LENGTH_EXCEPTIONS: &[LengthException] = &[(
    41874,
    41873,
    "EncodedUnderlyingMarketDisruptionFallbackUnderlierSecurityDesc",
    "EncodedUnderlyingMarketDisruptionFallbackUnderlierSecDescLen",
)];

/// Everything one generated table is built from, however many XML files it came
/// from.
///
/// `fix44_from_document` fills it from one document; `pair_from_documents` merges two into
/// one under ADR-0083's rules and fills the same struct, so the emitter below
/// is one emitter and the two tables cannot drift apart in shape.
pub(super) struct Spec<'a, 'i> {
    /// Names this dialect in the generated doc comments.
    pub(super) dialect: &'static str,
    pub(super) number_of: BTreeMap<&'a str, u32>,
    /// The **variant**, not the XML spelling: `DATA` and `XMLDATA` are one
    /// type and the pair build depends on that (ADR-0083 decision 2).
    pub(super) type_of: BTreeMap<&'a str, FieldType>,
    pub(super) enum_of: BTreeMap<&'a str, Vec<&'a str>>,
    pub(super) components: BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
    pub(super) messages: Vec<roxmltree::Node<'a, 'i>>,
    pub(super) header: roxmltree::Node<'a, 'i>,
    pub(super) trailer: roxmltree::Node<'a, 'i>,
    /// ADR-0083 decision 5. Empty for FIX 4.4, one row for the pair.
    pub(super) length_exceptions: &'static [LengthException],
    /// Whether `enum_allows` splits a multi-value field on spaces before it
    /// checks the list. ADR-0083 decision 1's second rule. Both tables are now
    /// built with it: the pair from B1, FIX 4.4 from the row ADR-0084
    /// decision 2 gave it. The field stays because a table built to the
    /// whole-value reading is a thing this generator must still be able to
    /// say, and because the two spellings of the rule are then one line apart.
    pub(super) per_token_enums: bool,
}

/// A `<component>` reference that resolves to zero members, wherever it
/// appears. This is `collect_groups`'s `group {name} ... has no members` check
/// one level up — ADR-0083 decision 4's second refusal.
pub(super) fn refuse_empty_reference(
    def: roxmltree::Node<'_, '_>,
    name: &str,
    ctx: &str,
) -> Result<(), GenError> {
    if !has_members(def) {
        return refuse(format!(
            "{ctx} references component {name}, which resolves to zero members.\n\
             A component that splices in nothing loses every field under it in\n\
             silence — CLAUDE.md §2 item 5. ADR-0083 decision 4."
        ));
    }
    Ok(())
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
pub(super) fn collect_required<'a>(
    el: roxmltree::Node<'a, '_>,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, '_>>,
    number_of: &BTreeMap<&str, u32>,
    msg: &str,
    out: &mut BTreeSet<u32>,
    path: &mut Vec<&'a str>,
) -> Result<(), GenError> {
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
                None => return refuse(format!("message {msg} names unknown field {name}")),
            }
        } else if c.has_tag_name("component") {
            let Some(def) = components.get(name) else {
                return refuse(format!("message {msg} names unknown component {name}"));
            };
            refuse_empty_reference(*def, name, &format!("message {msg}"))?;
            if path.contains(&name) {
                return refuse(format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_required(*def, components, number_of, msg, out, path)?;
            path.pop();
        }
    }
    Ok(())
}

/// Every tag a message may carry: fields, group counters, and everything a
/// component splices in — all of it transitively.
///
/// Unlike [`collect_required`] this ignores `required`. "May carry" and "must
/// carry" are different questions and the corpus asks both, with different
/// `373` codes: 2 and 1.
pub(super) fn collect_allowed<'a>(
    el: roxmltree::Node<'a, '_>,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, '_>>,
    number_of: &BTreeMap<&str, u32>,
    msg: &str,
    out: &mut BTreeSet<u32>,
    path: &mut Vec<&'a str>,
) -> Result<(), GenError> {
    for c in el.children() {
        let Some(name) = c.attribute("name") else {
            continue;
        };
        if c.has_tag_name("field") || c.has_tag_name("group") {
            match number_of.get(name) {
                Some(t) => {
                    out.insert(*t);
                }
                None => return refuse(format!("message {msg} names unknown field {name}")),
            }
            if c.has_tag_name("group") {
                collect_allowed(c, components, number_of, msg, out, path)?;
            }
        } else if c.has_tag_name("component") {
            let Some(def) = components.get(name) else {
                return refuse(format!("message {msg} names unknown component {name}"));
            };
            refuse_empty_reference(*def, name, &format!("message {msg}"))?;
            if path.contains(&name) {
                return refuse(format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_allowed(*def, components, number_of, msg, out, path)?;
            path.pop();
        }
    }
    Ok(())
}

/// Every tag under `<header>`, descending into groups. The group's own counter
/// tag counts too: it is the field that appears on the wire.
pub(super) fn collect_header(
    el: roxmltree::Node<'_, '_>,
    number_of: &BTreeMap<&str, u32>,
    out: &mut BTreeSet<u32>,
) -> Result<(), GenError> {
    for c in el.children() {
        if !(c.has_tag_name("field") || c.has_tag_name("group")) {
            continue;
        }
        let Some(name) = c.attribute("name") else {
            return refuse("header entry without a name");
        };
        match number_of.get(name) {
            Some(t) => {
                out.insert(*t);
            }
            None => return refuse(format!("header names unknown field {name}")),
        }
        if c.has_tag_name("group") {
            collect_header(c, number_of, out)?;
        }
    }
    Ok(())
}

/// Every tag a group can hold, in declaration order, components spliced in
/// where they are referenced.
///
/// A nested `<group>` contributes its counter tag and nothing else: the counter
/// is the field that appears at this level of the wire, and what the nested
/// entries hold is the nested group's own question.
pub(super) fn collect_members<'a>(
    el: roxmltree::Node<'a, '_>,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, '_>>,
    number_of: &BTreeMap<&str, u32>,
    ctx: &str,
    out: &mut Vec<u32>,
    path: &mut Vec<&'a str>,
) -> Result<(), GenError> {
    for c in el.children() {
        let Some(name) = c.attribute("name") else {
            continue;
        };
        if c.has_tag_name("field") || c.has_tag_name("group") {
            match number_of.get(name) {
                Some(t) => out.push(*t),
                None => return refuse(format!("{ctx} names unknown field {name}")),
            }
        } else if c.has_tag_name("component") {
            let Some(def) = components.get(name) else {
                return refuse(format!("{ctx} names unknown component {name}"));
            };
            refuse_empty_reference(*def, name, ctx)?;
            if path.contains(&name) {
                return refuse(format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_members(*def, components, number_of, ctx, out, path)?;
            path.pop();
        }
    }
    Ok(())
}

/// Registers every group reachable from `el` under message type `mt`,
/// descending through components and through nested groups.
///
/// A group reached only through a component is the common case, not the
/// exception: `NoTradingSessions(386)` reaches NewOrderSingle solely through
/// `TrdgSesGrp`, so a walker that reads `<message>` children alone finds
/// nothing for it.
pub(super) fn collect_groups<'a>(
    el: roxmltree::Node<'a, '_>,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, '_>>,
    number_of: &BTreeMap<&str, u32>,
    mt: &str,
    groups: &mut BTreeMap<(String, u32), Vec<u32>>,
    positions: &mut usize,
    path: &mut Vec<&'a str>,
) -> Result<(), GenError> {
    for c in el.children() {
        let Some(name) = c.attribute("name") else {
            continue;
        };
        if c.has_tag_name("group") {
            let Some(&counter) = number_of.get(name) else {
                return refuse(format!("message {mt} names unknown group counter {name}"));
            };
            let mut members = Vec::new();
            collect_members(
                c,
                components,
                number_of,
                &format!("group {name}"),
                &mut members,
                &mut Vec::new(),
            )?;
            if members.is_empty() {
                return refuse(format!("group {name} in message {mt} has no members"));
            }
            *positions += 1;
            match groups.entry((mt.to_string(), counter)) {
                std::collections::btree_map::Entry::Vacant(v) => {
                    v.insert(members);
                }
                std::collections::btree_map::Entry::Occupied(prev) => {
                    if prev.get() != &members {
                        return refuse(format!(
                            "counter {counter} appears twice in message {mt} with\n\
                             different members: {:?} then {members:?}.\n\
                             A (msg_type, counter) key cannot answer both.",
                            prev.get()
                        ));
                    }
                }
            }
            collect_groups(c, components, number_of, mt, groups, positions, path)?;
        } else if c.has_tag_name("component") {
            let Some(def) = components.get(name) else {
                return refuse(format!("message {mt} names unknown component {name}"));
            };
            refuse_empty_reference(*def, name, &format!("message {mt}"))?;
            if path.contains(&name) {
                return refuse(format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_groups(*def, components, number_of, mt, groups, positions, path)?;
            path.pop();
        }
    }
    Ok(())
}

/// The largest the per-tag bitsets may be, `ALLOWED` and `DEFINED_TAGS`
/// together: 64 MiB. FIX 4.4's are about 11 KB; a tag at 20 000 makes them
/// about 238 KB.
pub(super) const MAX_BITSET_BYTES: usize = 64 * 1024 * 1024;

/// One `<message>`, as the tables see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Message {
    pub(super) name: String,
    pub(super) msg_type: String,
    pub(super) admin: bool,
    /// Ascending. Empty when the message requires nothing.
    pub(super) required: Vec<u32>,
    /// The body: header and trailer are folded in when asked, not stored.
    pub(super) allowed: BTreeSet<u32>,
}

/// A merged dictionary: FIX 4.4 with an overlay applied, or a whole file
/// (ADR-0207 decision 3).
///
/// Built by [`merged_model`](super::merged_model). Every table the generated
/// file holds is computed once, here, and written out from here — so what this
/// answers and what the emitted `impl Dictionary` and `impl Tables` answer are
/// one computation, not two. Each query is named after, and answers as, the
/// associated function of the same name on `fixbolt_codec::Dictionary` or
/// [`crate::Tables`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub(super) number_of: BTreeMap<String, u32>,
    pub(super) name_of: BTreeMap<u32, String>,
    pub(super) type_of: BTreeMap<String, FieldType>,
    pub(super) enum_of: BTreeMap<String, Vec<String>>,
    pub(super) header: BTreeSet<u32>,
    pub(super) header_required: BTreeSet<u32>,
    pub(super) trailer: BTreeSet<u32>,
    pub(super) data_len: BTreeMap<u32, u32>,
    /// In declaration order: the emitted `ALLOWED` and `required` follow it.
    pub(super) messages: Vec<Message>,
    /// Keyed by `(msg_type, counter)`; a header group under `""`.
    pub(super) groups: BTreeMap<(String, u32), Vec<u32>>,
    pub(super) positions: usize,
    pub(super) per_token_enums: bool,
}

/// How large the per-tag bitsets of a generated dictionary are.
///
/// `ALLOWED` holds one bitset per message type and `DEFINED_TAGS` one more,
/// each `words` 64-bit words over `0..=max_tag`. One custom tag at 20 000
/// makes every one of them 313 words where FIX 4.4's are 15 — static data,
/// allocation-free, paid in binary size and cache (ADR-0207 *Consequences*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct TableSize {
    /// The highest tag the dictionary defines.
    pub max_tag: u32,
    /// Words per bitset: `max_tag / 64 + 1`.
    pub words: usize,
    /// Message types, one `ALLOWED` bitset each.
    pub message_types: usize,
    /// `(message_types + 1) * words * 8`: `ALLOWED` and `DEFINED_TAGS`.
    pub bitset_bytes: usize,
}

impl std::fmt::Display for TableSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the highest tag, {}, makes each per-tag bitset {} words: {} bytes of static \
             tables for {} message types",
            self.max_tag, self.words, self.bitset_bytes, self.message_types
        )
    }
}

/// Distinct group member lists, and for each counter which lists which message
/// types use. Shared by the header-conflict check and the emitter, so the two
/// read one grouping.
pub(super) type GroupLists = (Vec<Vec<u32>>, BTreeMap<u32, BTreeMap<usize, Vec<String>>>);

pub(super) fn group_lists(groups: &BTreeMap<(String, u32), Vec<u32>>) -> GroupLists {
    // Deduplicated: many messages share a group verbatim.
    let mut lists: Vec<Vec<u32>> = Vec::new();
    let mut list_id: BTreeMap<Vec<u32>, usize> = BTreeMap::new();
    let mut by_counter: BTreeMap<u32, BTreeMap<usize, Vec<String>>> = BTreeMap::new();
    for ((mt, counter), members) in groups {
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
    (lists, by_counter)
}

/// The number of a field this table has a type or an enumeration for. Every
/// such field was numbered by `collect_fields`, so `None` is this generator's
/// own bug.
pub(super) fn number<K: std::borrow::Borrow<str> + Ord>(
    number_of: &BTreeMap<K, u32>,
    name: &str,
) -> Result<u32, GenError> {
    match number_of.get(name) {
        Some(&t) => Ok(t),
        None => refuse(format!("field {name} has a type but no number")),
    }
}

impl Model {
    /// Everything the emitter writes, computed from `spec`. Every refusal the
    /// generator makes about a dictionary's content is made here, so a `Model`
    /// that exists can be written.
    #[allow(clippy::too_many_lines)]
    pub(super) fn compute(spec: &Spec<'_, '_>) -> Result<Self, GenError> {
        let number_of = &spec.number_of;
        let type_of = &spec.type_of;
        let components = &spec.components;
        let header_el = spec.header;

        // ---- header tags ---------------------------------------------------
        // Descends into <group>. The FIX 4.4 header holds one — NoHops(627)
        // with HopCompID(628), HopSendingTime(629), HopRefID(630) — and all
        // four are header fields. Taking only direct <field> children yields 26
        // instead of 30, and the four missing ones would sort into the BODY
        // when writing, which is non-negotiable 5's exact failure mode. No
        // acceptance definition carries a hop, so nothing in the 59 would ever
        // notice. FIXT 1.1's header is the same shape: 29 direct fields and the
        // same group.
        let mut header: BTreeSet<u32> = BTreeSet::new();
        collect_header(header_el, number_of, &mut header)?;

        // ---- DATA -> LENGTH, matched by NAME, never by tag-1 ---------------
        // `XMLDATA` is the same variant as `DATA` (ADR-0083 decision 1), so it
        // is paired by the same rule — all 8 of FIX 5.0 SP2's pair by name,
        // measured.
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
                // Not a warning. A DATA field with no length field cannot be
                // parsed at all — the parser would scan for 0x01 inside binary
                // content.
                None => {
                    return refuse(format!(
                        "DATA field {name} has no matching {name}Len or {name}Length field.\n\
                 A DATA field whose length is unknown cannot be parsed: its value may\n\
                 contain 0x01. Refusing to generate a table that would parse it wrongly."
                    ));
                }
            }
        }
        for ((data_tag, _, data_name, _), used) in
            spec.length_exceptions.iter().zip(&exception_used)
        {
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

        // ---- required fields, per message, descending into components -----
        // A `required='Y'` component contributes its own `required='Y'` fields,
        // and nothing else: Instrument is required in NewOrderSingle while
        // every field inside it, Symbol(55) included, is optional. "The message
        // requires an Instrument" and "the message requires a Symbol" are
        // different statements.
        let mut messages: Vec<Message> = Vec::new();
        let mut msg_types: BTreeSet<&str> = BTreeSet::new();
        for &m in &spec.messages {
            let (Some(name), Some(mt)) = (m.attribute("name"), m.attribute("msgtype")) else {
                return refuse("<message> without name or msgtype");
            };
            if !msg_types.insert(mt) {
                return refuse(format!("two messages share msgtype {mt}"));
            }

            // `msgcat` is the dictionary's own answer to "is this
            // administrative". A `<message>` without it stops the build,
            // exactly as a missing `name` or `msgtype` does above: a default
            // would be this generator inventing the answer, and the one place
            // it must not be invented is the place `DESIGN.md` D3 points at.
            let admin = match m.attribute("msgcat") {
                Some("admin") => true,
                Some("app") => false,
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
            };

            let mut set = BTreeSet::new();
            collect_required(m, components, number_of, name, &mut set, &mut Vec::new())?;
            let required: Vec<u32> = set.into_iter().collect();

            let mut allowed = BTreeSet::new();
            collect_allowed(
                m,
                components,
                number_of,
                name,
                &mut allowed,
                &mut Vec::new(),
            )?;
            messages.push(Message {
                name: name.to_string(),
                msg_type: mt.to_string(),
                admin,
                required,
                allowed,
            });
        }

        // ---- repeating groups, per message ---------------------------------
        // Keyed by (msg_type, counter). Never by counter alone:
        // NoMDEntries(268) takes MDEntryType(269) in a snapshot and
        // MDUpdateAction(279) in an incremental refresh, and an incremental
        // refresh is the highest-volume message there is. Three more counters
        // behave the same way.
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
        // The header's one group, NoHops(627), can appear in ANY message, so
        // it is keyed under the empty message type and emitted without a
        // msg_type arm.
        collect_groups(
            header_el,
            components,
            number_of,
            "",
            &mut groups,
            &mut positions,
            &mut Vec::new(),
        )?;
        let (_, by_counter) = group_lists(&groups);
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

        // ---- names the emitted constants take -------------------------------
        let mut seen: BTreeMap<String, &str> = BTreeMap::new();
        for name in number_of.keys() {
            let c = screaming(name);
            if let Some(prev) = seen.insert(c.clone(), name) {
                return refuse(format!("fields {prev} and {name} both become tag::{c}"));
            }
        }
        let mut seen2: BTreeSet<String> = BTreeSet::new();
        for m in &messages {
            let c = screaming(&m.name);
            if !seen2.insert(c.clone()) {
                return refuse(format!("two messages both become msg_type::{c}"));
            }
        }

        // ---- required_header -----------------------------------------------
        // `required()` answers for a message BODY. `14b_RequiredFieldMissing.def`
        // sends a Heartbeat with no TargetCompID and expects `373=1` with
        // `371=56` — a header field, which `required(b"0")` does not and should
        // not mention.
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

        let mut trailer: BTreeSet<u32> = BTreeSet::new();
        collect_header(spec.trailer, number_of, &mut trailer)?;

        let name_of: BTreeMap<u32, &str> = number_of.iter().map(|(k, v)| (*v, *k)).collect();
        let named = |tag: u32| format!("{}({tag})", name_of.get(&tag).copied().unwrap_or("?"));

        // ---- one place per tag: header, trailer, or a body ------------------
        // `is_header` and `allows` are separate tables, and the session and
        // the writer read a tag as header or body by the first. A tag in both
        // makes a valid message answer 373=14 (a header field after a body
        // field) and makes the writer move a body field into the header.
        // `[measured 2026-09-26]` FIX 4.4 keeps the three apart, so every tag
        // found here was put there by an overlay or a whole file.
        // docs/reference/a-tag-in-the-header-and-a-body-makes-a-valid-message-a-373-14.md
        if let Some(&tag) = header.intersection(&trailer).next() {
            return refuse(format!(
                "tag {} is in the header and in the trailer. A tag has one place:\n\
                 the header, the trailer, or a message body. ADR-0207 decision 3.",
                named(tag)
            ));
        }
        for m in &messages {
            for (place, set) in [("header", &header), ("trailer", &trailer)] {
                if let Some(&tag) = set.intersection(&m.allowed).next() {
                    return refuse(format!(
                        "tag {} is in the {place} and in the body of message {}({}).\n\
                         A tag has one place: the header, the trailer, or a message body —\n\
                         in two, a valid message is rejected with 373=14 and the writer\n\
                         moves the field. ADR-0207 decision 3.",
                        named(tag),
                        m.name,
                        m.msg_type
                    ));
                }
            }
        }

        // ---- a DATA member of a group follows its length member -------------
        // At body level the encoder sorts a DATA field by its length field's
        // tag (crates/codec/src/template.rs `key`), so declaration order does
        // not matter there. A group entry is written in declaration order
        // (`put_group`) and read the same way, so there the length must be the
        // member immediately in front — the invariant `put_group`'s comment
        // records FIX 4.4 meeting for all 66 of its DATA members.
        for ((mt, counter), members) in &groups {
            for (i, tag) in members.iter().enumerate() {
                let Some(&len) = data_len.get(tag) else {
                    continue;
                };
                let in_front = i.checked_sub(1).and_then(|j| members.get(j)).copied();
                if in_front != Some(len) {
                    let owner = messages.iter().find(|m| m.msg_type == *mt).map_or_else(
                        || "the header".to_string(),
                        |m| format!("message {}({})", m.name, m.msg_type),
                    );
                    return refuse(format!(
                        "group {} in {owner} declares the DATA field {} without its length\n\
                         field {} immediately in front of it. A group entry is written and\n\
                         read in declaration order, so the length must come just before the\n\
                         data. ADR-0207 decision 3.",
                        named(*counter),
                        named(*tag),
                        named(len)
                    ));
                }
            }
        }

        // ---- a ceiling on the per-tag bitsets --------------------------------
        // Each is `max_tag / 64 + 1` words, one per message type plus one. A
        // tag near u32::MAX would size each at 512 MiB; refused before any is
        // sized.
        let max_tag = number_of.values().copied().max().unwrap_or(0);
        let words = (max_tag as usize / 64).saturating_add(1);
        let bytes = (messages.len().saturating_add(1))
            .saturating_mul(words)
            .saturating_mul(8);
        if bytes > MAX_BITSET_BYTES {
            return refuse(format!(
                "the highest tag, {}, would make the per-tag bitsets {bytes} bytes, over the\n\
                 64 MiB ceiling ({MAX_BITSET_BYTES} bytes). Use a lower tag number.\n\
                 ADR-0207 Consequences.",
                named(max_tag)
            ));
        }

        if !messages.iter().any(|m| m.admin) {
            let dialect = spec.dialect;
            return refuse(format!(
                "{dialect}: not one <message> carries msgcat='admin'.\n\
                 A dictionary with no administrative message would make `is_admin`\n\
                 answer `false` for Logon itself. Refusing to emit it."
            ));
        }

        Ok(Self {
            number_of: number_of
                .iter()
                .map(|(k, v)| ((*k).to_string(), *v))
                .collect(),
            name_of: number_of
                .iter()
                .map(|(k, v)| (*v, (*k).to_string()))
                .collect(),
            type_of: type_of
                .iter()
                .map(|(k, v)| ((*k).to_string(), *v))
                .collect(),
            enum_of: spec
                .enum_of
                .iter()
                .map(|(k, v)| {
                    (
                        (*k).to_string(),
                        v.iter().map(|s| (*s).to_string()).collect(),
                    )
                })
                .collect(),
            header,
            header_required,
            trailer,
            data_len,
            messages,
            groups,
            positions,
            per_token_enums: spec.per_token_enums,
        })
    }

    /// The highest tag, and the words of one bitset over `0..=max_tag`.
    pub(super) fn width(&self) -> (u32, usize) {
        let max_tag = self.number_of.values().copied().max().unwrap_or(0);
        (max_tag, (max_tag as usize / 64) + 1)
    }

    fn message(&self, msg_type: &[u8]) -> Option<&Message> {
        self.messages
            .iter()
            .find(|m| m.msg_type.as_bytes() == msg_type)
    }

    /// Whether the dictionary defines `tag` at all.
    #[must_use]
    pub fn is_defined_tag(&self, tag: u32) -> bool {
        self.name_of.contains_key(&tag)
    }

    /// The declared type of `tag`, `None` if it is not defined.
    #[must_use]
    pub fn field_type(&self, tag: u32) -> Option<FieldType> {
        self.name_of
            .get(&tag)
            .and_then(|name| self.type_of.get(name))
            .copied()
    }

    /// `Some(allowed)` for an enumerated field, `None` for one with no value
    /// list. A multi-value field is checked per space-separated token.
    #[must_use]
    pub fn enum_allows(&self, tag: u32, value: &[u8]) -> Option<bool> {
        let name = self.name_of.get(&tag)?;
        let list = self.enum_of.get(name)?;
        let listed = |v: &[u8]| list.iter().any(|x| x.as_bytes() == v);
        if self.per_token_enums
            && matches!(
                self.type_of.get(name),
                Some(FieldType::MultipleValueString | FieldType::MultipleCharValue)
            )
        {
            return Some(value.split(|&b| b == b' ').all(listed));
        }
        Some(listed(value))
    }

    /// Whether `msg_type` is a message type of this dictionary.
    #[must_use]
    pub fn is_msg_type(&self, msg_type: &[u8]) -> bool {
        self.message(msg_type).is_some()
    }

    /// Whether `msg_type` is declared `msgcat='admin'`.
    #[must_use]
    pub fn is_admin(&self, msg_type: &[u8]) -> bool {
        self.message(msg_type).is_some_and(|m| m.admin)
    }

    /// The tags `msg_type` must carry in its body, ascending.
    #[must_use]
    pub fn required(&self, msg_type: &[u8]) -> &[u32] {
        self.message(msg_type)
            .map_or(&[], |m| m.required.as_slice())
    }

    /// Whether `msg_type` may carry `tag`: its body, the header and the
    /// trailer. An unknown message type allows nothing.
    #[must_use]
    pub fn allows(&self, msg_type: &[u8], tag: u32) -> bool {
        self.message(msg_type).is_some_and(|m| {
            m.allowed.contains(&tag) || self.header.contains(&tag) || self.trailer.contains(&tag)
        })
    }

    /// Whether `tag` belongs to the standard header.
    #[must_use]
    pub fn is_header(&self, tag: u32) -> bool {
        self.header.contains(&tag)
    }

    /// The length field in front of a DATA field.
    #[must_use]
    pub fn data_length_tag(&self, tag: u32) -> Option<u32> {
        self.data_len.get(&tag).copied()
    }

    /// The first declared member of the group `counter` in `msg_type`.
    #[must_use]
    pub fn group_delimiter(&self, msg_type: &[u8], counter: u32) -> Option<u32> {
        self.group_members(msg_type, counter).first().copied()
    }

    /// The members of the group `counter` in `msg_type`, in declaration order;
    /// empty if `msg_type` has no such group. A header group answers for every
    /// message type.
    #[must_use]
    pub fn group_members(&self, msg_type: &[u8], counter: u32) -> &[u32] {
        self.groups
            .iter()
            .find(|((mt, c), _)| *c == counter && (mt.is_empty() || mt.as_bytes() == msg_type))
            .map_or(&[], |(_, members)| members.as_slice())
    }

    /// The size of the per-tag bitsets the emitted file holds, which the
    /// highest tag decides. A user's `build.rs` prints it with
    /// `println!("cargo:warning={}", model.table_size())` when it is larger
    /// than they expect.
    #[must_use]
    pub fn table_size(&self) -> TableSize {
        let (max_tag, words) = self.width();
        let message_types = self.messages.len();
        TableSize {
            max_tag,
            words,
            message_types,
            bitset_bytes: (message_types + 1) * words * 8,
        }
    }
}

/// `ClOrdID` -> `CL_ORD_ID`, `NoMDEntries` -> `NO_MD_ENTRIES`.
///
/// A `_` goes before an uppercase letter that follows a lowercase or digit, and
/// before the last uppercase of an acronym run when a lowercase follows it.
pub(super) fn screaming(name: &str) -> String {
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
