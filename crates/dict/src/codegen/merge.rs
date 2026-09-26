//! Merging two dictionary documents into one table.
//!
//! Two merges, kept apart because their rules differ:
//!
//! - the FIXT 1.1 + FIX 5.0 SP2 **pair** (ADR-0080 decision 2, as narrowed by
//!   ADR-0083) — [`merge_fields`] and [`merge_components`], behind `fix50sp2`;
//! - an **overlay** onto the shipped FIX 4.4 (ADR-0207 decision 3) —
//!   [`overlay_fix44`] and [`refuse_repeats`]. An overlay adds and never
//!   removes or retypes, and a number, a name or a msgtype it repeats must
//!   agree with FIX 4.4's, or the build fails naming both.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::error::{GenError, refuse};
#[cfg(feature = "fix50sp2")]
use super::parse::{Fields, has_members, refuse_empty_components};
use super::parse::{child, collect_components};
use crate::field_type::FieldType;

#[cfg(feature = "fix50sp2")]
/// ADR-0083 decision 2: a field defined in both files must agree on number, on
/// name, and on the **variant** `from_xml` gives its type name.
///
/// Two spellings that map to one variant are one type, which is what resolves
/// `XmlData(213)` — `DATA` in `FIXT11.xml`, `XMLDATA` in `FIX50SP2.xml` — with
/// no exception and no winner. Anything else is refused with the field named: a
/// silent "first wins" ships a `373=6` that depends on file order.
///
/// Enum lists: one side's set must be a **superset** of the other's and the
/// table carries the superset. Two sets that each hold a value the other lacks
/// fail the build naming both stray values.
pub(super) fn merge_fields<'a>(
    transport: Fields<'a>,
    app: Fields<'a>,
) -> Result<Fields<'a>, GenError> {
    let (tnum, ttype, tenum) = transport;
    let (mut number_of, mut type_of, mut enum_of) = app;

    for (name, num) in tnum {
        match number_of.get(name) {
            Some(&other) if other != num => {
                return refuse(format!(
                    "field {name} is number {num} in FIXT11.xml and {other} in FIX50SP2.xml.\n\
                 One name cannot be two tags."
                ));
            }
            Some(_) => {}
            None => {
                number_of.insert(name, num);
            }
        }
    }

    for (name, ty) in ttype {
        match type_of.get(name) {
            Some(&other) if other != ty => {
                return refuse(format!(
                    "field {name} is {ty:?} in FIXT11.xml and {other:?} in FIX50SP2.xml.\n\
                 The two spellings map to different FieldType variants, so the table\n\
                 would answer 373=6 by file order. Refusing. ADR-0083 decision 2."
                ));
            }
            Some(_) => {}
            None => {
                type_of.insert(name, ty);
            }
        }
    }

    for (name, values) in tenum {
        let take_transport = match enum_of.get(name) {
            None => true,
            Some(app_values) => {
                let mine: BTreeSet<&str> = values.iter().copied().collect();
                let theirs: BTreeSet<&str> = app_values.iter().copied().collect();
                if mine.is_subset(&theirs) {
                    false
                } else if theirs.is_subset(&mine) {
                    true
                } else {
                    let only_t: Vec<&str> = mine.difference(&theirs).copied().collect();
                    let only_a: Vec<&str> = theirs.difference(&mine).copied().collect();
                    return refuse(format!(
                        "field {name} is enumerated in both files and neither list contains\n\
                         the other: only in FIXT11.xml {only_t:?}, only in FIX50SP2.xml\n\
                         {only_a:?}. A union would hide the divergence. ADR-0083 decision 2."
                    ));
                }
            }
        };
        if take_transport {
            enum_of.insert(name, values);
        }
    }

    // The same number under two names, across the pair as well as within one
    // file. `collect_fields` already asked this per file.
    let mut named: BTreeMap<u32, &str> = BTreeMap::new();
    for (&name, &num) in &number_of {
        if let Some(prev) = named.insert(num, name)
            && prev != name
        {
            return refuse(format!(
                "fields {prev} and {name} both carry number {num} across the pair."
            ));
        }
    }

    Ok((number_of, type_of, enum_of))
}

#[cfg(feature = "fix50sp2")]
/// ADR-0083 decision 4: one component map, built from both files.
///
/// * identical children — one definition, no message (`HopGrp`);
/// * one side empty and the other not — the full one is the definition and a
///   warning, pushed onto `warnings` for `build.rs` to print as a
///   `cargo:warning`, names the component and the file that left it empty
///   (`MsgTypeGrp`);
/// * both non-empty and different — the build fails naming the component and
///   the first differing child.
pub(super) fn merge_components<'a, 'i>(
    transport: BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
    tfile: &str,
    app: BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
    afile: &str,
    warnings: &mut Vec<String>,
) -> Result<BTreeMap<&'a str, roxmltree::Node<'a, 'i>>, GenError> {
    let mut out: BTreeMap<&'a str, roxmltree::Node<'a, 'i>> = BTreeMap::new();
    let names: BTreeSet<&'a str> = transport.keys().chain(app.keys()).copied().collect();
    for name in names {
        let chosen = match (transport.get(name), app.get(name)) {
            (Some(&t), Some(&a)) => match (has_members(t), has_members(a)) {
                (true, true) => {
                    if let Some(where_) = first_difference(t, a, name) {
                        return refuse(format!(
                            "component {name} is defined in both files and they differ at\n\
                             {where_}. Two full definitions that disagree are a real\n\
                             conflict and no order rule makes it safe. ADR-0083 decision 4c."
                        ));
                    }
                    t
                }
                (true, false) => {
                    warnings.push(warn_empty(name, afile));
                    t
                }
                (false, true) => {
                    warnings.push(warn_empty(name, tfile));
                    a
                }
                (false, false) => {
                    return refuse(format!(
                        "component {name} is empty in {tfile} and in {afile}. A reference to\n\
                     it resolves to zero members. ADR-0083 decision 4."
                    ));
                }
            },
            (Some(&t), None) => t,
            (None, Some(&a)) => a,
            (None, None) => continue,
        };
        out.insert(name, chosen);
    }
    refuse_empty_components(&out, "the FIXT11.xml + FIX50SP2.xml pair")?;
    Ok(out)
}

#[cfg(feature = "fix50sp2")]
/// The override is visible in every build log, not only in the ADR: `build.rs`
/// prints this after `cargo:warning=`.
fn warn_empty(name: &str, file: &str) -> String {
    format!(
        "component {name} is declared empty in {file}; the other file's \
         full definition is used. An empty component cannot mean \"deliberately nothing\" \
         — see docs/decisions/ADR-0083 decision 4b."
    )
}

#[cfg(feature = "fix50sp2")]
/// Where two component definitions first differ: element name, `name`,
/// `required`, order or nesting. `None` when they are identical.
fn first_difference(
    x: roxmltree::Node<'_, '_>,
    y: roxmltree::Node<'_, '_>,
    path: &str,
) -> Option<String> {
    let xs: Vec<roxmltree::Node<'_, '_>> = x.children().filter(|n| n.is_element()).collect();
    let ys: Vec<roxmltree::Node<'_, '_>> = y.children().filter(|n| n.is_element()).collect();
    for i in 0..xs.len().max(ys.len()) {
        match (xs.get(i), ys.get(i)) {
            (Some(&a), Some(&b)) => {
                if describe(a) != describe(b) {
                    return Some(format!("{path}: {} against {}", describe(a), describe(b)));
                }
                if let Some(deeper) = first_difference(
                    a,
                    b,
                    &format!("{path}/{}", a.attribute("name").unwrap_or("")),
                ) {
                    return Some(deeper);
                }
            }
            (Some(&a), None) | (None, Some(&a)) => {
                return Some(format!("{path}: {} on one side only", describe(a)));
            }
            (None, None) => {}
        }
    }
    None
}

#[cfg(feature = "fix50sp2")]
fn describe(n: roxmltree::Node<'_, '_>) -> String {
    format!(
        "<{} name='{}' required='{}'>",
        n.tag_name().name(),
        n.attribute("name").unwrap_or(""),
        n.attribute("required").unwrap_or("")
    )
}

/// Text a base element gains, keyed by where the element starts. Every
/// addition to one element is one string, so one element is one edit.
type Additions<'a, 'i> = BTreeMap<usize, (roxmltree::Node<'a, 'i>, String)>;

/// FIX 4.4, indexed for the overlay's questions.
struct Base<'a, 'i> {
    header: roxmltree::Node<'a, 'i>,
    fields_el: roxmltree::Node<'a, 'i>,
    components_el: roxmltree::Node<'a, 'i>,
    messages_el: roxmltree::Node<'a, 'i>,
    field_by_name: BTreeMap<&'a str, (u32, roxmltree::Node<'a, 'i>)>,
    field_by_number: BTreeMap<u32, &'a str>,
    components: BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
    message_by_name: BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
    message_by_type: BTreeMap<&'a str, &'a str>,
}

impl<'a, 'i> Base<'a, 'i> {
    fn index(root: roxmltree::Node<'a, 'i>) -> Result<Self, GenError> {
        let (Some(header), Some(fields_el), Some(components_el), Some(messages_el)) = (
            child(root, "header"),
            child(root, "fields"),
            child(root, "components"),
            child(root, "messages"),
        ) else {
            return refuse(
                "FIX44.xml: a <header>, <fields>, <components> or <messages> section is missing",
            );
        };
        let mut field_by_name = BTreeMap::new();
        let mut field_by_number = BTreeMap::new();
        for f in fields_el.children().filter(|n| n.has_tag_name("field")) {
            let (Some(name), Some(Ok(num))) = (
                f.attribute("name"),
                f.attribute("number").map(str::parse::<u32>),
            ) else {
                return refuse("FIX44.xml: <field> without a name or a numeric number");
            };
            field_by_name.insert(name, (num, f));
            field_by_number.insert(num, name);
        }
        let mut message_by_name = BTreeMap::new();
        let mut message_by_type = BTreeMap::new();
        for m in messages_el.children().filter(|n| n.has_tag_name("message")) {
            let (Some(name), Some(mt)) = (m.attribute("name"), m.attribute("msgtype")) else {
                return refuse("FIX44.xml: <message> without name or msgtype");
            };
            message_by_name.insert(name, m);
            message_by_type.insert(mt, name);
        }
        Ok(Self {
            header,
            fields_el,
            components_el,
            messages_el,
            field_by_name,
            field_by_number,
            components: collect_components(root),
            message_by_name,
            message_by_type,
        })
    }
}

/// ADR-0207 decision 3: `overlay` merged onto `base` (the shipped FIX 4.4), as
/// XML text the single-file generator then reads like any dictionary.
///
/// Merging as text keeps one generator: the merged document goes through
/// exactly the walks and refusals `spec/FIX44.xml` does, and an overlay that
/// adds nothing leaves `base` untouched to the byte — which is what makes an
/// empty overlay's tables `Fix44`'s own (`overlay.rs`,
/// `an_empty_overlay_emits_byte_identical_fix44`).
///
/// What the overlay adds is copied verbatim into the base element it names:
/// new `<field>`s, `<component>`s and `<message>`s at the end of their
/// section; a gained `<value>` into its field; gained children into an existing
/// message, component or the header. Conflicts are refused here, naming both
/// sides; what only the merged whole can show — an unknown name, a tag twice at
/// one level — is refused after it, by the table walk and [`refuse_repeats`].
pub(super) fn overlay_fix44(base: &str, overlay: &str) -> Result<String, GenError> {
    let bdoc =
        roxmltree::Document::parse(base).map_err(|e| GenError::Xml(format!("FIX44.xml: {e}")))?;
    let odoc = roxmltree::Document::parse(overlay)
        .map_err(|e| GenError::Xml(format!("the overlay: {e}")))?;
    let oroot = odoc.root_element();
    if !oroot.has_tag_name("fix") {
        return refuse(format!(
            "the overlay's root element is <{}>, not <fix>. ADR-0207 decision 3.",
            oroot.tag_name().name()
        ));
    }
    for (attr, want) in [("type", "FIX"), ("major", "4"), ("minor", "4")] {
        if let Some(got) = oroot.attribute(attr)
            && got != want
        {
            return refuse(format!(
                "the overlay says {attr}='{got}'; it is merged onto FIX 4.4, which is {attr}='{want}'.\n\
                 Overlays onto other versions are not supported. ADR-0207 decision 8."
            ));
        }
    }
    let b = Base::index(bdoc.root_element())?;
    let mut adds: Additions<'_, '_> = BTreeMap::new();
    let mut sections: BTreeSet<&str> = BTreeSet::new();
    for section in oroot.children().filter(roxmltree::Node::is_element) {
        let name = section.tag_name().name();
        if !sections.insert(name) {
            return refuse(format!(
                "the overlay has two <{name}> sections. Put everything in one."
            ));
        }
        match name {
            "fields" => overlay_fields(&b, section, overlay, &mut adds)?,
            "components" => overlay_components(&b, section, overlay, &mut adds)?,
            "messages" => overlay_messages(&b, section, overlay, &mut adds)?,
            "header" => {
                for c in section.children().filter(roxmltree::Node::is_element) {
                    if !(c.has_tag_name("field") || c.has_tag_name("group")) {
                        return refuse(format!(
                            "the overlay's <header> holds <{}>; a header holds only <field>\n\
                             and <group>, and anything else would be dropped silently.",
                            c.tag_name().name()
                        ));
                    }
                }
                add_children(&mut adds, b.header, section, overlay)?;
            }
            "trailer" => {
                return refuse(
                    "the overlay has a <trailer>. An overlay may not change the trailer:\n\
                     SignatureLength, Signature and CheckSum are FIX 4.4's, and CheckSum is\n\
                     written last by the engine. ADR-0207 decision 3.",
                );
            }
            other => {
                return refuse(format!(
                    "the overlay has a <{other}> section, which is none of <header>,\n\
                     <messages>, <components> and <fields>; everything in it would be\n\
                     dropped silently. ADR-0207 decision 3."
                ));
            }
        }
    }
    splice(base, adds)
}

/// The overlay's `<fields>`: new fields appended, gained values added, and a
/// repeated number, name or type checked against FIX 4.4.
fn overlay_fields<'a, 'i>(
    b: &Base<'a, 'i>,
    section: roxmltree::Node<'_, '_>,
    text: &str,
    adds: &mut Additions<'a, 'i>,
) -> Result<(), GenError> {
    let mut names: BTreeSet<&str> = BTreeSet::new();
    let mut numbers: BTreeMap<u32, &str> = BTreeMap::new();
    for f in section.children().filter(roxmltree::Node::is_element) {
        if !f.has_tag_name("field") {
            return refuse(format!(
                "the overlay's <fields> holds <{}>, not <field>.",
                f.tag_name().name()
            ));
        }
        let (Some(name), Some(number), Some(ty)) = (
            f.attribute("name"),
            f.attribute("number"),
            f.attribute("type"),
        ) else {
            return refuse(
                "the overlay has a <field> without a name, a number or a type; each one\n\
                 needs all three, a field FIX 4.4 already defines included.",
            );
        };
        let Ok(num) = number.parse::<u32>() else {
            return refuse(format!(
                "the overlay's field {name} has a non-numeric number {number:?}"
            ));
        };
        if !names.insert(name) {
            return refuse(format!("field {name} appears twice in the overlay"));
        }
        if let Some(prev) = numbers.insert(num, name) {
            return refuse(format!(
                "fields {prev} and {name} both carry number {num} in the overlay"
            ));
        }
        match b.field_by_name.get(name) {
            Some(&(base_num, base_field)) => {
                if base_num != num {
                    return refuse(format!(
                        "field {name} is number {base_num} in FIX44.xml and {num} in the overlay.\n\
                         One name cannot be two tags. ADR-0207 decision 3."
                    ));
                }
                let base_ty = base_field.attribute("type").unwrap_or("");
                let same = FieldType::from_xml(ty)
                    .is_some_and(|t| FieldType::from_xml(base_ty) == Some(t));
                if !same {
                    return refuse(format!(
                        "field {name}({num}) is {base_ty} in FIX44.xml and {ty} in the overlay.\n\
                         An overlay adds and never retypes; a dictionary that changes a type\n\
                         is a whole file (Source::Fix44Whole). ADR-0207 decision 3."
                    ));
                }
                overlay_values(name, num, base_field, f, text, adds)?;
            }
            None => {
                if let Some(base_name) = b.field_by_number.get(&num) {
                    return refuse(format!(
                        "tag {num} is {base_name} in FIX44.xml and {name} in the overlay.\n\
                         One tag cannot be two fields: a venue's own field needs a number\n\
                         FIX 4.4 does not use. ADR-0207 decision 3."
                    ));
                }
                add(adds, b.fields_el, text, f)?;
            }
        }
    }
    Ok(())
}

/// The `<value>`s an overlay lists for a field FIX 4.4 defines.
///
/// A value FIX 4.4 already lists is skipped whatever its description says:
/// only `enum` reaches the wire, so a venue renaming `OrdType=1` changes
/// nothing a table holds. A field FIX 4.4 leaves open (no value list) may not
/// gain one — a list would refuse every value it does not name, which narrows
/// the field instead of adding to it.
fn overlay_values<'a, 'i>(
    name: &str,
    num: u32,
    base_field: roxmltree::Node<'a, 'i>,
    field: roxmltree::Node<'_, '_>,
    text: &str,
    adds: &mut Additions<'a, 'i>,
) -> Result<(), GenError> {
    let listed: BTreeSet<&str> = base_field
        .children()
        .filter(|n| n.has_tag_name("value"))
        .filter_map(|v| v.attribute("enum"))
        .collect();
    let mut gained: BTreeSet<&str> = BTreeSet::new();
    let mut gained_text = String::new();
    for v in field.children().filter(roxmltree::Node::is_element) {
        if !v.has_tag_name("value") {
            return refuse(format!(
                "the overlay's field {name} holds <{}>; a field holds only <value>.",
                v.tag_name().name()
            ));
        }
        let Some(e) = v.attribute("enum") else {
            return refuse(format!("field {name} has a <value> with no enum attribute"));
        };
        if listed.contains(e) || !gained.insert(e) {
            continue;
        }
        gained_text.push_str(slice(text, v.range())?);
    }
    if gained_text.is_empty() {
        return Ok(());
    }
    if listed.is_empty() {
        return refuse(format!(
            "field {name}({num}) takes any value in FIX44.xml; a value list in the overlay\n\
             would refuse every value it does not name (373=5). That narrows the field\n\
             instead of adding to it: a dictionary that does so is a whole file\n\
             (Source::Fix44Whole). ADR-0207 decision 3."
        ));
    }
    push(adds, base_field, &gained_text);
    Ok(())
}

/// The overlay's `<components>`: new ones appended, existing ones extended.
fn overlay_components<'a, 'i>(
    b: &Base<'a, 'i>,
    section: roxmltree::Node<'_, '_>,
    text: &str,
    adds: &mut Additions<'a, 'i>,
) -> Result<(), GenError> {
    let mut names: BTreeSet<&str> = BTreeSet::new();
    for c in section.children().filter(roxmltree::Node::is_element) {
        if !c.has_tag_name("component") {
            return refuse(format!(
                "the overlay's <components> holds <{}>, not <component>.",
                c.tag_name().name()
            ));
        }
        let Some(name) = c.attribute("name") else {
            return refuse("the overlay has a <component> without a name");
        };
        if !names.insert(name) {
            return refuse(format!("component {name} appears twice in the overlay"));
        }
        match b.components.get(name) {
            Some(&base_component) => add_children(adds, base_component, c, text)?,
            None => add(adds, b.components_el, text, c)?,
        }
    }
    Ok(())
}

/// The overlay's `<messages>`: new ones appended, existing ones extended under
/// the msgtype and msgcat FIX 4.4 gives them.
fn overlay_messages<'a, 'i>(
    b: &Base<'a, 'i>,
    section: roxmltree::Node<'_, '_>,
    text: &str,
    adds: &mut Additions<'a, 'i>,
) -> Result<(), GenError> {
    let mut names: BTreeSet<&str> = BTreeSet::new();
    let mut types: BTreeMap<&str, &str> = BTreeMap::new();
    for m in section.children().filter(roxmltree::Node::is_element) {
        if !m.has_tag_name("message") {
            return refuse(format!(
                "the overlay's <messages> holds <{}>, not <message>.",
                m.tag_name().name()
            ));
        }
        let Some(name) = m.attribute("name") else {
            return refuse("the overlay has a <message> without a name");
        };
        let Some(mt) = m.attribute("msgtype") else {
            return refuse(format!(
                "the overlay's message {name} has no msgtype; an overlay names the\n\
                 msgtype even of a message FIX 4.4 defines."
            ));
        };
        if !names.insert(name) {
            return refuse(format!("message {name} appears twice in the overlay"));
        }
        if let Some(prev) = types.insert(mt, name) {
            return refuse(format!(
                "messages {prev} and {name} both carry msgtype {mt} in the overlay"
            ));
        }
        match b.message_by_name.get(name) {
            Some(&base_message) => {
                let base_mt = base_message.attribute("msgtype").unwrap_or("");
                if base_mt != mt {
                    return refuse(format!(
                        "message {name} is msgtype {base_mt} in FIX44.xml and {mt} in the overlay.\n\
                         An overlay extends a message under the msgtype FIX 4.4 gives it.\n\
                         ADR-0207 decision 3."
                    ));
                }
                if let Some(cat) = m.attribute("msgcat") {
                    let base_cat = base_message.attribute("msgcat").unwrap_or("");
                    if cat != base_cat {
                        return refuse(format!(
                            "message {name}({mt}) is msgcat '{base_cat}' in FIX44.xml and '{cat}'\n\
                             in the overlay. An overlay may not move a message between the\n\
                             session and the application. ADR-0207 decision 3."
                        ));
                    }
                }
                add_children(adds, base_message, m, text)?;
            }
            None => {
                if let Some(base_name) = b.message_by_type.get(mt) {
                    return refuse(format!(
                        "msgtype {mt} is {base_name} in FIX44.xml and {name} in the overlay.\n\
                         One msgtype cannot be two messages. ADR-0207 decision 3."
                    ));
                }
                add(adds, b.messages_el, text, m)?;
            }
        }
    }
    Ok(())
}

/// Every element child of `from` (overlay text), appended to `to` (base).
fn add_children<'a, 'i>(
    adds: &mut Additions<'a, 'i>,
    to: roxmltree::Node<'a, 'i>,
    from: roxmltree::Node<'_, '_>,
    text: &str,
) -> Result<(), GenError> {
    for c in from.children().filter(roxmltree::Node::is_element) {
        add(adds, to, text, c)?;
    }
    Ok(())
}

/// The element `what` (overlay text), appended to `to` (base).
fn add<'a, 'i>(
    adds: &mut Additions<'a, 'i>,
    to: roxmltree::Node<'a, 'i>,
    text: &str,
    what: roxmltree::Node<'_, '_>,
) -> Result<(), GenError> {
    let piece = slice(text, what.range())?;
    push(adds, to, piece);
    Ok(())
}

fn push<'a, 'i>(adds: &mut Additions<'a, 'i>, to: roxmltree::Node<'a, 'i>, piece: &str) {
    let entry = adds
        .entry(to.range().start)
        .or_insert_with(|| (to, String::new()));
    entry.1.push('\n');
    entry.1.push_str(piece);
}

fn slice(text: &str, range: Range<usize>) -> Result<&str, GenError> {
    match text.get(range.clone()) {
        Some(s) => Ok(s),
        None => refuse(format!(
            "the XML parser reported bytes {range:?}, which are not in the text; this\n\
             generator's own bug"
        )),
    }
}

/// Applies the additions to `base`: each goes in front of its element's
/// closing tag, and a self-closing element is opened to take it.
fn splice(base: &str, adds: Additions<'_, '_>) -> Result<String, GenError> {
    let mut edits: Vec<(Range<usize>, String)> = Vec::with_capacity(adds.len());
    for (node, piece) in adds.into_values() {
        let r = node.range();
        let element = slice(base, r.clone())?;
        if element.ends_with("/>") {
            let open = r.end.saturating_sub(2)..r.end;
            edits.push((open, format!(">{piece}\n</{}>", node.tag_name().name())));
        } else if let Some(at) = element.rfind("</") {
            let at = r.start + at;
            edits.push((at..at, format!("{piece}\n")));
        } else {
            return refuse(format!(
                "FIX44.xml: <{}> is neither self-closing nor closed",
                node.tag_name().name()
            ));
        }
    }
    // From the end backwards, so an edit never moves the bytes the next one
    // is placed by. Nested elements (a field inside <fields>) close in front of
    // their parent, so no two edits overlap.
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = base.to_string();
    for (range, piece) in edits {
        if out.get(range.clone()).is_none() {
            return refuse(format!(
                "an overlay edit at bytes {range:?} falls outside FIX44.xml; this\n\
                 generator's own bug"
            ));
        }
        out.replace_range(range, &piece);
    }
    Ok(out)
}

/// No level of a message or of the header carries one field twice — after the
/// merge, with components spliced in and every group a level of its own.
///
/// A tag twice at one level cannot be parsed (the second reads as a repeat of
/// the first) or ordered (non-negotiable 5). `[measured 2026-09-26]` FIX 4.4 has
/// no such level, so on the merged document every one found is the overlay's.
/// Runs after the table walk, which has already refused unknown names and
/// component cycles.
pub(super) fn refuse_repeats(doc: &roxmltree::Document<'_>) -> Result<(), GenError> {
    let root = doc.root_element();
    let components = collect_components(root);
    if let Some(header) = child(root, "header") {
        level(header, "the header", &components)?;
    }
    if let Some(messages) = child(root, "messages") {
        for m in messages.children().filter(|n| n.has_tag_name("message")) {
            let ctx = format!("message {}", m.attribute("name").unwrap_or(""));
            level(m, &ctx, &components)?;
        }
    }
    Ok(())
}

fn level<'a, 'i>(
    el: roxmltree::Node<'a, 'i>,
    ctx: &str,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
) -> Result<(), GenError> {
    let mut seen: BTreeMap<&str, String> = BTreeMap::new();
    let mut groups = Vec::new();
    walk_level(el, ctx, components, &mut Vec::new(), &mut seen, &mut groups)?;
    for g in groups {
        let ctx = format!("{ctx}, group {}", g.attribute("name").unwrap_or(""));
        level(g, &ctx, components)?;
    }
    Ok(())
}

fn walk_level<'a, 'i>(
    el: roxmltree::Node<'a, 'i>,
    ctx: &str,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
    path: &mut Vec<&'a str>,
    seen: &mut BTreeMap<&'a str, String>,
    groups: &mut Vec<roxmltree::Node<'a, 'i>>,
) -> Result<(), GenError> {
    for c in el.children() {
        let Some(name) = c.attribute("name") else {
            continue;
        };
        if c.has_tag_name("field") || c.has_tag_name("group") {
            let here = if path.is_empty() {
                "directly".to_string()
            } else {
                format!("through component {}", path.join(" -> "))
            };
            if let Some(prev) = seen.insert(name, here.clone()) {
                return refuse(format!(
                    "{ctx} carries {name} twice at one level: {prev} and {here}.\n\
                     A tag twice at one level cannot be parsed or ordered — the second\n\
                     reads as a repeat of the first. ADR-0207 decision 3."
                ));
            }
            if c.has_tag_name("group") {
                groups.push(c);
            }
        } else if c.has_tag_name("component") {
            let Some(&def) = components.get(name) else {
                continue;
            };
            if path.contains(&name) {
                continue;
            }
            path.push(name);
            walk_level(def, ctx, components, path, seen, groups)?;
            path.pop();
        }
    }
    Ok(())
}
