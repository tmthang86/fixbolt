//! Reading a QuickFIX dictionary document: its fields and its components.

use std::collections::BTreeMap;

use super::error::{GenError, refuse};
use crate::field_type::FieldType;

/// The first child element of `root` called `name`.
pub(super) fn child<'a, 'i>(
    root: roxmltree::Node<'a, 'i>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'i>> {
    root.children().find(|n| n.has_tag_name(name))
}

/// Every `<field>`: number, variant and enumerated values.
///
/// The type name is turned into a [`FieldType`] **here** rather than at emit
/// time, because the pair build has to compare variants before it can emit
/// anything (ADR-0083 decision 2).
pub(super) fn collect_fields<'a>(
    root: roxmltree::Node<'a, '_>,
    file: &str,
) -> Result<Fields<'a>, GenError> {
    let Some(fields_el) = child(root, "fields") else {
        return refuse(format!("{file}: <fields> section missing"));
    };
    let mut number_of: BTreeMap<&str, u32> = BTreeMap::new();
    let mut type_of: BTreeMap<&str, FieldType> = BTreeMap::new();
    let mut enum_of: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut named: BTreeMap<u32, &str> = BTreeMap::new();
    for f in fields_el.children().filter(|n| n.has_tag_name("field")) {
        let (Some(name), Some(num)) = (f.attribute("name"), f.attribute("number")) else {
            return refuse(format!("{file}: <field> without name or number"));
        };
        let Ok(num) = num.parse::<u32>() else {
            return refuse(format!("field {name} has a non-numeric number"));
        };
        if number_of.insert(name, num).is_some() {
            return refuse(format!("field name {name} appears twice"));
        }
        if let Some(prev) = named.insert(num, name) {
            return refuse(format!(
                "{file}: fields {prev} and {name} both carry number {num}"
            ));
        }
        let ty = f.attribute("type").unwrap_or("");
        match FieldType::from_xml(ty) {
            Some(t) => {
                type_of.insert(name, t);
            }
            // A type name `src/field_type.rs` does not list must stop the
            // build. Falling back to STRING would make `373=6` silently blind
            // to a whole type, and no acceptance definition would notice.
            None => {
                return refuse(format!(
                    "field {name} has type {ty:?}, which src/field_type.rs does not know.\n\
                 Add the variant there — both `from_xml` and `as_rust` — rather than\n\
                 letting it fall through to STRING."
                ));
            }
        }

        let values: Vec<&str> = f
            .children()
            .filter(|n| n.has_tag_name("value"))
            .map(|v| match v.attribute("enum") {
                Some(e) => Ok(e),
                None => refuse(format!("field {name} has a <value> with no enum attribute")),
            })
            .collect::<Result<_, _>>()?;
        if !values.is_empty() {
            enum_of.insert(name, values);
        }
    }
    Ok((number_of, type_of, enum_of))
}

/// Every `<component>`, by name — **including empty ones**, which
/// `merge_components` needs to see before it can decide anything
/// (ADR-0083 decision 4b). A single-file build refuses them through
/// [`refuse_empty_components`].
pub(super) fn collect_components<'a, 'i>(
    root: roxmltree::Node<'a, 'i>,
) -> BTreeMap<&'a str, roxmltree::Node<'a, 'i>> {
    match child(root, "components") {
        Some(el) => el
            .children()
            .filter(|n| n.has_tag_name("component"))
            .filter_map(|n| n.attribute("name").map(|k| (k, n)))
            .collect(),
        None => BTreeMap::new(),
    }
}

/// Whether an element has at least one child element.
pub(super) fn has_members(n: roxmltree::Node<'_, '_>) -> bool {
    n.children().any(|c| c.is_element())
}

/// A `<component>` with no children cannot mean "deliberately nothing": a
/// reference to it resolves to zero members, which is the silent loss
/// `CLAUDE.md` §2 item 5 forbids. The one way an empty declaration survives is
/// as the losing half of a pair — ADR-0083 decision 4b.
pub(super) fn refuse_empty_components(
    map: &BTreeMap<&str, roxmltree::Node<'_, '_>>,
    file: &str,
) -> Result<(), GenError> {
    for (name, def) in map {
        if !has_members(*def) {
            return refuse(format!(
                "{file}: <component name='{name}'> has no children, and no other file\n\
                 defines it. A reference to it would resolve to zero members — the\n\
                 silent loss CLAUDE.md §2 item 5 forbids. ADR-0083 decision 4."
            ));
        }
    }
    Ok(())
}

/// The three maps [`collect_fields`] returns, and `merge_fields` merges.
pub(super) type Fields<'a> = (
    BTreeMap<&'a str, u32>,
    BTreeMap<&'a str, FieldType>,
    BTreeMap<&'a str, Vec<&'a str>>,
);
