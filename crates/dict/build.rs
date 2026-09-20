//! Generates `$OUT_DIR/fix44.rs` from the QuickFIX FIX 4.4 XML dictionary, and
//! — behind the `fix50sp2` feature — `$OUT_DIR/fixt11_fix50sp2.rs` from the
//! **pair** `FIXT11.xml` + `FIX50SP2.xml` (ADR-0080 decision 2, as narrowed by
//! ADR-0083).
//!
//! The dictionaries are not in this repository — ADR-0001 keeps them in
//! gitignored `vendor/`. When one is absent the build fails loudly and names the
//! script that fetches it. It never falls back to a stub: a dictionary that
//! silently becomes empty is a parser that silently stops validating.
//!
//! Traps this generator is written against are recorded in
//! `docs/reference/fix44-dictionary-traps.md` and
//! `docs/reference/fixt-dictionary-traps.md`. Four matter here:
//!   * a DATA field's length field is NOT `tag - 1` — Signature(89) takes
//!     SignatureLength(93). Matching is by name, with one named exception
//!     (ADR-0083 decision 5).
//!   * `<message>` may be self-closing (`XMLnonFIX`), so "has children" is not
//!     the same as "exists".
//!   * one field is spelled `DATA` in one file and `XMLDATA` in the other, so
//!     the pair build compares the `FieldType` **variant**, never the XML
//!     spelling (ADR-0083 decision 2).
//!   * `<component name='MsgTypeGrp' />` is empty in `FIXT11.xml` and full in
//!     `FIX50SP2.xml`, so components merge across the pair and an empty
//!     declaration loses to a full one (ADR-0083 decision 4).

// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// `NANOFIX_FIX44_XML` overrides the location, for packagers and for CI runs
/// that place the asset elsewhere.
const OVERRIDE: &str = "NANOFIX_FIX44_XML";
const DEFAULT: &str = "../../vendor/quickfix/spec/FIX44.xml";

/// The transport half of a FIXT 1.1 session: header, trailer and the eight
/// admin messages. Same override pattern as `NANOFIX_FIX44_XML`.
const FIXT_OVERRIDE: &str = "NANOFIX_FIXT11_XML";
const FIXT_DEFAULT: &str = "../../vendor/quickfix/spec/FIXT11.xml";

/// The application half: fields, components, groups and 156 messages.
const SP2_OVERRIDE: &str = "NANOFIX_FIX50SP2_XML";
const SP2_DEFAULT: &str = "../../vendor/quickfix/spec/FIX50SP2.xml";

/// A DATA or XMLDATA field whose length field the `{name}Len` / `{name}Length`
/// rule cannot find: `(data_tag, length_tag, data_name, length_name)`.
///
/// ADR-0083 decision 5. Consulted **only** after the name rule has failed, and
/// checked in both directions — an entry whose names or numbers the XML does
/// not carry, or that the name rule would have found anyway, fails the build.
/// The shape is `interop_quickfix_fields.rs::TYPE_EXEMPTIONS`'s.
type LengthException = (u32, u32, &'static str, &'static str);

/// The one row measured on 2026-09-19 at pin `386ce46e`: 74 of FIX 5.0 SP2's 75
/// DATA fields and all 8 of its XMLDATA fields pair by name; this one
/// abbreviates `Security` to `Sec` in its length field's name.
///
/// A `tag - 1` fallback is refused rather than added: twelve SP2 DATA/XMLDATA
/// fields do not sit at `length + 1`, and a fallback pairs the wrong field
/// silently the next time upstream abbreviates a name.
const SP2_LENGTH_EXCEPTIONS: &[LengthException] = &[(
    41874,
    41873,
    "EncodedUnderlyingMarketDisruptionFallbackUnderlierSecurityDesc",
    "EncodedUnderlyingMarketDisruptionFallbackUnderlierSecDescLen",
)];

fn main() {
    println!("cargo:rerun-if-env-changed={OVERRIDE}");
    let path = spec_path(OVERRIDE, DEFAULT);
    println!("cargo:rerun-if-changed={}", path.display());
    // The type table is emitted from `FieldType::from_xml`, so editing that
    // file must regenerate. Without this line a new variant compiles into the
    // crate and never reaches the generated table.
    println!("cargo:rerun-if-changed=src/field_type.rs");

    let text = read_spec(&path, "FIX 4.4 dictionary", OVERRIDE);
    let doc = parse_spec(&path, &text);
    write_generated("fix44.rs", &generate(&doc));

    // ---- the FIXT 1.1 / FIX 5.0 SP2 pair, behind `fix50sp2` ---------------
    // **Read only when the feature is on.** `CLAUDE.md` §2 item 6: with the
    // feature off nothing below runs, so a machine that never fetched
    // `FIXT11.xml` is not asked whether it exists.
    if std::env::var_os("CARGO_FEATURE_FIX50SP2").is_none() {
        return;
    }
    println!("cargo:rerun-if-env-changed={FIXT_OVERRIDE}");
    println!("cargo:rerun-if-env-changed={SP2_OVERRIDE}");
    let transport_path = spec_path(FIXT_OVERRIDE, FIXT_DEFAULT);
    let app_path = spec_path(SP2_OVERRIDE, SP2_DEFAULT);
    println!("cargo:rerun-if-changed={}", transport_path.display());
    println!("cargo:rerun-if-changed={}", app_path.display());

    let transport_text = read_spec(&transport_path, "FIXT 1.1 dictionary", FIXT_OVERRIDE);
    let app_text = read_spec(&app_path, "FIX 5.0 SP2 dictionary", SP2_OVERRIDE);
    let transport = parse_spec(&transport_path, &transport_text);
    let app = parse_spec(&app_path, &app_text);
    write_generated("fixt11_fix50sp2.rs", &generate_pair(&transport, &app));
}

/// Where a dictionary lives: the override if it is set, the vendored copy
/// otherwise.
fn spec_path(var: &str, default: &str) -> PathBuf {
    match std::env::var(var) {
        Ok(p) => PathBuf::from(p),
        Err(_) => PathBuf::from(default),
    }
}

/// Reads a dictionary, or dies naming the script that fetches it.
fn read_spec(path: &Path, what: &str, var: &str) -> String {
    if !path.exists() {
        die(&format!(
            "{what} not found at {}\n\n  run scripts/fetch-quickfix-assets.sh\n\n\
             It is not committed on purpose: the QuickFIX licence's attribution clause\n\
             would come with it. See docs/decisions/ADR-0001-relationship-to-quickfix.md.\n\
             Set {var} to use a copy from somewhere else.",
            path.display()
        ));
    }
    match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => die(&format!("cannot read {}: {e}", path.display())),
    }
}

fn parse_spec<'i>(path: &Path, text: &'i str) -> roxmltree::Document<'i> {
    match roxmltree::Document::parse(text) {
        Ok(d) => d,
        Err(e) => die(&format!("{} is not well-formed XML: {e}", path.display())),
    }
}

fn write_generated(name: &str, body: &str) {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));
    if let Err(e) = std::fs::write(out.join(name), body) {
        die(&format!("cannot write {name}: {e}"));
    }
}

// The same file `src/field_type.rs` compiles into the crate. Included by path
// rather than copied, because "which XML type name is which variant" is one
// rule and `CLAUDE.md` §4 gives it one place. `build.rs` cannot `use` the crate
// it builds, so this is how the two stay in step.
#[path = "src/field_type.rs"]
#[allow(dead_code)]
mod field_type;

use field_type::FieldType;

fn die(msg: &str) -> ! {
    eprintln!("\nfixbolt-dict: {msg}\n");
    std::process::exit(1)
}

fn child<'a, 'i>(root: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    root.children().find(|n| n.has_tag_name(name))
}

/// Everything one generated table is built from, however many XML files it came
/// from.
///
/// [`generate`] fills it from one document; [`generate_pair`] merges two into
/// one under ADR-0083's rules and fills the same struct, so the emitter below
/// is one emitter and the two tables cannot drift apart in shape.
struct Spec<'a, 'i> {
    /// Names this dialect in the generated doc comments.
    dialect: &'static str,
    /// The `@generated by` banner's source clause.
    source: &'static str,
    number_of: BTreeMap<&'a str, u32>,
    /// The **variant**, not the XML spelling: `DATA` and `XMLDATA` are one
    /// type and the pair build depends on that (ADR-0083 decision 2).
    type_of: BTreeMap<&'a str, FieldType>,
    enum_of: BTreeMap<&'a str, Vec<&'a str>>,
    components: BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
    messages: Vec<roxmltree::Node<'a, 'i>>,
    header: roxmltree::Node<'a, 'i>,
    trailer: roxmltree::Node<'a, 'i>,
    /// ADR-0083 decision 5. Empty for FIX 4.4, one row for the pair.
    length_exceptions: &'static [LengthException],
    /// Whether `enum_allows` splits a multi-value field on spaces before it
    /// checks the list. ADR-0083 decision 1's second rule. Both tables are now
    /// built with it: the pair from B1, FIX 4.4 from the row ADR-0084
    /// decision 2 gave it. The field stays because a table built to the
    /// whole-value reading is a thing this generator must still be able to
    /// say, and because the two spellings of the rule are then one line apart.
    per_token_enums: bool,
}

/// The FIX 4.4 table, from one file.
fn generate(doc: &roxmltree::Document<'_>) -> String {
    let root = doc.root_element();
    let (number_of, type_of, enum_of) = collect_fields(root, "FIX44.xml");
    let (Some(header), Some(trailer)) = (child(root, "header"), child(root, "trailer")) else {
        die("<header> or <trailer> section missing")
    };
    let Some(messages_el) = child(root, "messages") else {
        die("<messages> section missing")
    };
    emit(&Spec {
        dialect: "FIX 4.4",
        source: "the QuickFIX FIX 4.4 XML",
        number_of,
        type_of,
        enum_of,
        components: {
            let map = collect_components(root);
            refuse_empty_components(&map, "FIX44.xml");
            map
        },
        messages: messages_el
            .children()
            .filter(|n| n.has_tag_name("message"))
            .collect(),
        header,
        trailer,
        length_exceptions: &[],
        // ADR-0083 decision 1's second rule, applied to FIX 4.4 by ADR-0084
        // decision 2's row: `18=2 A` is one legal two-value `ExecInst`, not one
        // illegal value. It was `false` here only until that row landed.
        per_token_enums: true,
    })
}

/// The FIXT 1.1 / FIX 5.0 SP2 table, from the pair.
///
/// Header, trailer and the eight admin messages come from the transport file;
/// fields, components, groups and the 156 application messages from the
/// application file. Every message — admin included — resolves its
/// `<component>` references against the **one** merged map, which is ADR-0083
/// decision 4 and the reason `NoMsgTypes(384)` reaches the Logon table at all.
fn generate_pair(transport: &roxmltree::Document<'_>, app: &roxmltree::Document<'_>) -> String {
    let (troot, aroot) = (transport.root_element(), app.root_element());
    let (tnum, ttype, tenum) = collect_fields(troot, "FIXT11.xml");
    let (anum, atype, aenum) = collect_fields(aroot, "FIX50SP2.xml");
    // Taken before the merge, because after it there is no transport half left
    // to ask. ADR-0084 decision 1: what the transport file defines is a table
    // of its own, not a filter over the merged one.
    let transport_tags: Vec<u32> = tnum.values().copied().collect();
    let (number_of, type_of, enum_of) = merge_fields((tnum, ttype, tenum), (anum, atype, aenum));

    let components = merge_components(
        collect_components(troot),
        "FIXT11.xml",
        collect_components(aroot),
        "FIX50SP2.xml",
    );

    let (Some(header), Some(trailer)) = (child(troot, "header"), child(troot, "trailer")) else {
        die("FIXT11.xml: <header> or <trailer> section missing")
    };
    let (Some(admin), Some(application)) = (child(troot, "messages"), child(aroot, "messages"))
    else {
        die("<messages> section missing in FIXT11.xml or FIX50SP2.xml")
    };
    let messages: Vec<roxmltree::Node<'_, '_>> = admin
        .children()
        .filter(|n| n.has_tag_name("message"))
        .chain(application.children().filter(|n| n.has_tag_name("message")))
        .collect();

    // The transport file's own `<messages>`, by msgtype — the same node the
    // merged list is built from, so the two cannot name different sets.
    let transport_msg_types: Vec<&str> = admin
        .children()
        .filter(|n| n.has_tag_name("message"))
        .map(|m| match m.attribute("msgtype") {
            Some(mt) => mt,
            None => die("FIXT11.xml: <message> without msgtype"),
        })
        .collect();

    let mut out = emit(&Spec {
        dialect: "FIXT 1.1 / FIX 5.0 SP2",
        source: "the QuickFIX FIXT11.xml and FIX50SP2.xml pair",
        number_of,
        type_of,
        enum_of,
        components,
        messages,
        header,
        trailer,
        length_exceptions: SP2_LENGTH_EXCEPTIONS,
        per_token_enums: true,
    });
    out.push_str(&emit_transport_layer(&transport_tags, &transport_msg_types));
    out
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
/// is emitted here rather than in [`emit`].
fn emit_transport_layer(tags: &[u32], msg_types: &[&str]) -> String {
    let max_tag = tags.iter().copied().max().unwrap_or(0);
    let words = (max_tag as usize / 64) + 1;
    let mut bits = vec![0u64; words];
    for &t in tags {
        bits[t as usize / 64] |= 1u64 << (t % 64);
    }
    if msg_types.is_empty() {
        die("FIXT11.xml: <messages> is empty; there is no transport layer to emit");
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
    o
}

/// Every `<field>`: number, variant and enumerated values.
///
/// The type name is turned into a [`FieldType`] **here** rather than at emit
/// time, because the pair build has to compare variants before it can emit
/// anything (ADR-0083 decision 2).
#[allow(clippy::type_complexity)]
fn collect_fields<'a>(
    root: roxmltree::Node<'a, '_>,
    file: &str,
) -> (
    BTreeMap<&'a str, u32>,
    BTreeMap<&'a str, FieldType>,
    BTreeMap<&'a str, Vec<&'a str>>,
) {
    let Some(fields_el) = child(root, "fields") else {
        die(&format!("{file}: <fields> section missing"))
    };
    let mut number_of: BTreeMap<&str, u32> = BTreeMap::new();
    let mut type_of: BTreeMap<&str, FieldType> = BTreeMap::new();
    let mut enum_of: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut named: BTreeMap<u32, &str> = BTreeMap::new();
    for f in fields_el.children().filter(|n| n.has_tag_name("field")) {
        let (Some(name), Some(num)) = (f.attribute("name"), f.attribute("number")) else {
            die(&format!("{file}: <field> without name or number"))
        };
        let Ok(num) = num.parse::<u32>() else {
            die(&format!("field {name} has a non-numeric number"))
        };
        if number_of.insert(name, num).is_some() {
            die(&format!("field name {name} appears twice"));
        }
        if let Some(prev) = named.insert(num, name) {
            die(&format!(
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
            None => die(&format!(
                "field {name} has type {ty:?}, which src/field_type.rs does not know.\n\
                 Add the variant there — both `from_xml` and `as_rust` — rather than\n\
                 letting it fall through to STRING."
            )),
        }

        let values: Vec<&str> = f
            .children()
            .filter(|n| n.has_tag_name("value"))
            .map(|v| match v.attribute("enum") {
                Some(e) => e,
                None => die(&format!(
                    "field {name} has a <value> with no enum attribute"
                )),
            })
            .collect();
        if !values.is_empty() {
            enum_of.insert(name, values);
        }
    }
    (number_of, type_of, enum_of)
}

/// Every `<component>`, by name — **including empty ones**, which
/// [`merge_components`] needs to see before it can decide anything
/// (ADR-0083 decision 4b). A single-file build refuses them through
/// [`refuse_empty_components`].
fn collect_components<'a, 'i>(
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
fn has_members(n: roxmltree::Node<'_, '_>) -> bool {
    n.children().any(|c| c.is_element())
}

/// A `<component>` with no children cannot mean "deliberately nothing": a
/// reference to it resolves to zero members, which is the silent loss
/// `CLAUDE.md` §2 item 5 forbids. The one way an empty declaration survives is
/// as the losing half of a pair — ADR-0083 decision 4b.
fn refuse_empty_components(map: &BTreeMap<&str, roxmltree::Node<'_, '_>>, file: &str) {
    for (name, def) in map {
        if !has_members(*def) {
            die(&format!(
                "{file}: <component name='{name}'> has no children, and no other file\n\
                 defines it. A reference to it would resolve to zero members — the\n\
                 silent loss CLAUDE.md §2 item 5 forbids. ADR-0083 decision 4."
            ));
        }
    }
}

/// The three maps [`collect_fields`] returns, and [`merge_fields`] merges.
type Fields<'a> = (
    BTreeMap<&'a str, u32>,
    BTreeMap<&'a str, FieldType>,
    BTreeMap<&'a str, Vec<&'a str>>,
);

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
fn merge_fields<'a>(transport: Fields<'a>, app: Fields<'a>) -> Fields<'a> {
    let (tnum, ttype, tenum) = transport;
    let (mut number_of, mut type_of, mut enum_of) = app;

    for (name, num) in tnum {
        match number_of.get(name) {
            Some(&other) if other != num => die(&format!(
                "field {name} is number {num} in FIXT11.xml and {other} in FIX50SP2.xml.\n\
                 One name cannot be two tags."
            )),
            Some(_) => {}
            None => {
                number_of.insert(name, num);
            }
        }
    }

    for (name, ty) in ttype {
        match type_of.get(name) {
            Some(&other) if other != ty => die(&format!(
                "field {name} is {ty:?} in FIXT11.xml and {other:?} in FIX50SP2.xml.\n\
                 The two spellings map to different FieldType variants, so the table\n\
                 would answer 373=6 by file order. Refusing. ADR-0083 decision 2."
            )),
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
                    die(&format!(
                        "field {name} is enumerated in both files and neither list contains\n\
                         the other: only in FIXT11.xml {only_t:?}, only in FIX50SP2.xml\n\
                         {only_a:?}. A union would hide the divergence. ADR-0083 decision 2."
                    ))
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
            die(&format!(
                "fields {prev} and {name} both carry number {num} across the pair."
            ));
        }
    }

    (number_of, type_of, enum_of)
}

/// ADR-0083 decision 4: one component map, built from both files.
///
/// * identical children — one definition, no message (`HopGrp`);
/// * one side empty and the other not — the full one is the definition and a
///   `cargo:warning` names the component and the file that left it empty
///   (`MsgTypeGrp`);
/// * both non-empty and different — the build fails naming the component and
///   the first differing child.
fn merge_components<'a, 'i>(
    transport: BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
    tfile: &str,
    app: BTreeMap<&'a str, roxmltree::Node<'a, 'i>>,
    afile: &str,
) -> BTreeMap<&'a str, roxmltree::Node<'a, 'i>> {
    let mut out: BTreeMap<&'a str, roxmltree::Node<'a, 'i>> = BTreeMap::new();
    let names: BTreeSet<&'a str> = transport.keys().chain(app.keys()).copied().collect();
    for name in names {
        let chosen = match (transport.get(name), app.get(name)) {
            (Some(&t), Some(&a)) => match (has_members(t), has_members(a)) {
                (true, true) => {
                    if let Some(where_) = first_difference(t, a, name) {
                        die(&format!(
                            "component {name} is defined in both files and they differ at\n\
                             {where_}. Two full definitions that disagree are a real\n\
                             conflict and no order rule makes it safe. ADR-0083 decision 4c."
                        ));
                    }
                    t
                }
                (true, false) => {
                    warn_empty(name, afile);
                    t
                }
                (false, true) => {
                    warn_empty(name, tfile);
                    a
                }
                (false, false) => die(&format!(
                    "component {name} is empty in {tfile} and in {afile}. A reference to\n\
                     it resolves to zero members. ADR-0083 decision 4."
                )),
            },
            (Some(&t), None) => t,
            (None, Some(&a)) => a,
            (None, None) => continue,
        };
        out.insert(name, chosen);
    }
    refuse_empty_components(&out, "the FIXT11.xml + FIX50SP2.xml pair");
    out
}

/// The override is visible in every build log, not only in the ADR.
fn warn_empty(name: &str, file: &str) {
    println!(
        "cargo:warning=component {name} is declared empty in {file}; the other file's \
         full definition is used. An empty component cannot mean \"deliberately nothing\" \
         — see docs/decisions/ADR-0083 decision 4b."
    );
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

/// A `<component>` reference that resolves to zero members, wherever it
/// appears. This is `collect_groups`'s `group {name} ... has no members` check
/// one level up — ADR-0083 decision 4's second refusal.
fn refuse_empty_reference(def: roxmltree::Node<'_, '_>, name: &str, ctx: &str) {
    if !has_members(def) {
        die(&format!(
            "{ctx} references component {name}, which resolves to zero members.\n\
             A component that splices in nothing loses every field under it in\n\
             silence — CLAUDE.md §2 item 5. ADR-0083 decision 4."
        ));
    }
}

fn emit(spec: &Spec<'_, '_>) -> String {
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
    collect_header(header_el, number_of, &mut header);

    // ---- DATA -> LENGTH, matched by NAME, never by tag-1 -------------------
    // `XMLDATA` is the same variant as `DATA` (ADR-0083 decision 1), so it is
    // paired by the same rule — all 8 of FIX 5.0 SP2's pair by name, measured.
    let mut data_len: BTreeMap<u32, u32> = BTreeMap::new();
    let mut exception_used = vec![false; spec.length_exceptions.len()];
    for (&name, &ty) in type_of {
        if ty != FieldType::Data {
            continue;
        }
        let tag = number_of[name];
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
            .position(|(data_tag, _, _, _)| *data_tag == tag)
        {
            Some(i) => {
                let (data_tag, length_tag, data_name, length_name) = spec.length_exceptions[i];
                if data_name != name {
                    die(&format!(
                        "length exception for tag {data_tag} names field {data_name},\n\
                         but the dictionary calls tag {tag} {name}."
                    ));
                }
                if number_of.get(length_name) != Some(&length_tag) {
                    die(&format!(
                        "length exception for {data_name} names {length_name} as tag\n\
                         {length_tag}, which this dictionary does not carry under that\n\
                         number. An exception the XML does not support is a wrong pairing."
                    ));
                }
                exception_used[i] = true;
                data_len.insert(data_tag, length_tag);
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
    for (i, (data_tag, _, data_name, _)) in spec.length_exceptions.iter().enumerate() {
        if !exception_used[i] {
            die(&format!(
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
            die("<message> without name or msgtype")
        };
        msg_consts.push((screaming(name), mt.to_string()));
        if !msg_types.insert(mt.to_string()) {
            die(&format!("two messages share msgtype {mt}"));
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
            Some(other) => die(&format!(
                "message {name} ({mt}) has msgcat={other:?}; the only categories\n\
                 this generator knows are 'admin' and 'app'."
            )),
            None => die(&format!(
                "message {name} ({mt}) has no msgcat attribute.\n\
                 `is_admin` is generated from it, so a message without one has no\n\
                 answer — and guessing a default here is the hand-written list\n\
                 beside a call site that DESIGN.md D3 forbids, only hidden in a\n\
                 build script."
            )),
        }

        let mut set = BTreeSet::new();
        collect_required(m, components, number_of, name, &mut set, &mut Vec::new());
        let mut tags: Vec<u32> = set.into_iter().collect();
        tags.sort_unstable();
        if !tags.is_empty() {
            required.push((mt.to_string(), tags));
        }

        let mut body = BTreeSet::new();
        collect_allowed(m, components, number_of, name, &mut body, &mut Vec::new());
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
            die("<message> without msgtype")
        };
        collect_groups(
            m,
            components,
            number_of,
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
        components,
        number_of,
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
            die(&format!("fields {prev} and {name} both become tag::{c}"));
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
        enum_index.insert(number_of[name], at);
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
    collect_header(spec.trailer, number_of, &mut trailer);
    let mut allow_bits: Vec<(String, Vec<u64>)> = Vec::new();
    let mut body_pairs = 0usize;
    for (mt, body) in &allowed {
        body_pairs += body.len();
        let mut bits = vec![0u64; words];
        for t in body.iter().chain(header.iter()).chain(trailer.iter()) {
            bits[*t as usize / 64] |= 1u64 << (t % 64);
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
        typed.insert(number_of[name], ty.as_rust());
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
        bits[t as usize / 64] |= 1u64 << (t % 64);
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
        die(&format!(
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
            refuse_empty_reference(*def, name, &format!("message {msg}"));
            if path.contains(&name) {
                die(&format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_required(*def, components, number_of, msg, out, path);
            path.pop();
        }
    }
}

/// Every tag a message may carry: fields, group counters, and everything a
/// component splices in — all of it transitively.
///
/// Unlike [`collect_required`] this ignores `required`. "May carry" and "must
/// carry" are different questions and the corpus asks both, with different
/// `373` codes: 2 and 1.
fn collect_allowed<'a>(
    el: roxmltree::Node<'a, '_>,
    components: &BTreeMap<&'a str, roxmltree::Node<'a, '_>>,
    number_of: &BTreeMap<&str, u32>,
    msg: &str,
    out: &mut BTreeSet<u32>,
    path: &mut Vec<&'a str>,
) {
    for c in el.children() {
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
            if c.has_tag_name("group") {
                collect_allowed(c, components, number_of, msg, out, path);
            }
        } else if c.has_tag_name("component") {
            let Some(def) = components.get(name) else {
                die(&format!("message {msg} names unknown component {name}"))
            };
            refuse_empty_reference(*def, name, &format!("message {msg}"));
            if path.contains(&name) {
                die(&format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_allowed(*def, components, number_of, msg, out, path);
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
            refuse_empty_reference(*def, name, ctx);
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
            refuse_empty_reference(*def, name, &format!("message {mt}"));
            if path.contains(&name) {
                die(&format!("component cycle: {} -> {name}", path.join(" -> ")));
            }
            path.push(name);
            collect_groups(*def, components, number_of, mt, groups, positions, path);
            path.pop();
        }
    }
}
