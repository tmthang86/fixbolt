//! Merging two dictionary documents into one table: the FIXT 1.1 + FIX 5.0 SP2
//! pair (ADR-0080 decision 2, as narrowed by ADR-0083). Overlays onto FIX 4.4
//! (ADR-0207 decision 3) will merge by the same agreement rules.

use std::collections::{BTreeMap, BTreeSet};

use super::error::{GenError, refuse};
use super::parse::{Fields, has_members, refuse_empty_components};

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

/// The override is visible in every build log, not only in the ADR: `build.rs`
/// prints this after `cargo:warning=`.
fn warn_empty(name: &str, file: &str) -> String {
    format!(
        "component {name} is declared empty in {file}; the other file's \
         full definition is used. An empty component cannot mean \"deliberately nothing\" \
         — see docs/decisions/ADR-0083 decision 4b."
    )
}

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

fn describe(n: roxmltree::Node<'_, '_>) -> String {
    format!(
        "<{} name='{}' required='{}'>",
        n.tag_name().name(),
        n.attribute("name").unwrap_or(""),
        n.attribute("required").unwrap_or("")
    )
}
