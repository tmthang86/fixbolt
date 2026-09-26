//! The FIX dictionary generator, as a library (ADR-0207).
//!
//! What works today: [`fix44_tables`] and, with `fix50sp2`,
//! `fixt11_fix50sp2_tables`. [`generate`] — a dictionary of your own — returns
//! [`GenError::Unsupported`] until the overlay lands.
//!
//! One generator, three callers (ADR-0207 decision 1). It is loaded twice, the
//! way `fixbolt-sbe-gen` is (ADR-0081): by this crate's `build.rs` through
//! `#[path]`, to write the tables of [`crate::Fix44`] and, behind `fix50sp2`,
//! of the FIXT 1.1 / FIX 5.0 SP2 pair; and as this module, behind the
//! off-by-default `codegen` feature, for a user's own `build.rs`.
//!
//! - [`fix44_tables`] and `fixt11_fix50sp2_tables` (behind `fix50sp2`) are what `build.rs`
//!   writes to `$OUT_DIR/fix44.rs` and `$OUT_DIR/fixt11_fix50sp2.rs`, byte
//!   for byte (decision 2).
//! - [`generate`] is the user's door: an overlay onto the shipped FIX 4.4, or a
//!   whole FIX 4.4 file (decision 3), into one Rust file holding the user's own
//!   zero-sized type (decision 4).
//!
//! Where `build.rs` used to `die`, every function here returns a
//! [`GenError`]; nothing in this module panics, exits or indexes (CLAUDE.md §2
//! item 7). `parse` reads a document's fields and components, `merge` joins
//! two documents, `model` holds what one table is built from and walks it,
//! `emit` writes it out.

mod emit;
mod error;
#[cfg(feature = "fix50sp2")]
mod merge;
mod model;
mod parse;

pub use error::GenError;
use error::refuse;
use model::Spec;
use parse::{child, collect_components, collect_fields, refuse_empty_components};

/// What a user's dictionary is made from (ADR-0207 decision 3).
///
/// Both shapes are QuickFIX-format XML text. The first version is FIX 4.4 only
/// (decision 8); `#[non_exhaustive]` so overlays onto the FIXT 1.1 + FIX 5.0
/// SP2 pair can be added later without a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Source<'a> {
    /// A `<fix>` document merged onto the `spec/FIX44.xml` this crate ships.
    /// Its `<header>`, `<messages>`, `<components>` and `<fields>` sections
    /// are each optional; it may add, never remove or retype.
    Fix44Overlay(&'a str),
    /// A complete QuickFIX FIX 4.4 dictionary, generated as-is.
    Fix44Whole(&'a str),
}

/// How the emitted code names the items it implements and uses (ADR-0207
/// decision 4).
///
/// [`Paths::facade`] (the default) goes through `::fixbolt::dict::…`, so a user
/// depends on `fixbolt` at run time and on `fixbolt-dict` only as a
/// build-dependency. [`Paths::direct`] names `::fixbolt_dict` and
/// `::fixbolt_codec`, for this crate's own tests and for a user who drives the
/// engine without the facade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Paths {
    root: Root,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Root {
    #[default]
    Facade,
    Direct,
}

impl Paths {
    /// Paths through the `fixbolt` facade, `::fixbolt::dict::…`.
    #[must_use]
    pub const fn facade() -> Self {
        Self { root: Root::Facade }
    }

    /// Paths straight to `::fixbolt_dict` and `::fixbolt_codec`.
    #[must_use]
    pub const fn direct() -> Self {
        Self { root: Root::Direct }
    }
}

/// Generates one Rust file for the user to `include!`: the tables, a unit
/// struct named `type_name`, and its `impl Dictionary` and `impl Tables`
/// (ADR-0207 decision 4).
///
/// # Errors
///
/// [`GenError::Unsupported`], for every input: the overlay onto the shipped FIX
/// 4.4 and the whole-file dictionary are not generated yet. The tables this
/// crate ships are generated today, by [`fix44_tables`] and, with `fix50sp2`,
/// `fixt11_fix50sp2_tables`.
pub fn generate(source: Source<'_>, type_name: &str, paths: Paths) -> Result<String, GenError> {
    let _ = (source, type_name, paths.root);
    Err(GenError::Unsupported)
}

/// The FIX 4.4 tables this crate's `build.rs` writes to `$OUT_DIR/fix44.rs`,
/// from the dictionary text `xml` (`spec/FIX44.xml` unless
/// `NANOFIX_FIX44_XML` overrides it).
///
/// This crate's own tables, emitted for `crate::`-relative inclusion; a user's
/// dictionary comes from [`generate`].
///
/// # Errors
///
/// [`GenError::Xml`] if `xml` does not parse; [`GenError::Dictionary`] where
/// `build.rs` stops the build, carrying the same sentence.
pub fn fix44_tables(xml: &str) -> Result<String, GenError> {
    let doc = parse_xml(xml)?;
    fix44_from_document(&doc)
}

/// The FIXT 1.1 / FIX 5.0 SP2 tables this crate's `build.rs` writes to
/// `$OUT_DIR/fixt11_fix50sp2.rs`, from the **pair** of dictionary texts
/// (ADR-0080 decision 2, as narrowed by ADR-0083).
///
/// A component declared empty in one file and full in the other is taken
/// from the full one (ADR-0083 decision 4b); `build.rs` prints a
/// `cargo:warning` for it, and this function does not.
///
/// # Errors
///
/// [`GenError::Xml`] if either text does not parse; [`GenError::Dictionary`]
/// where `build.rs` stops the build, carrying the same sentence.
#[cfg(feature = "fix50sp2")]
pub fn fixt11_fix50sp2_tables(transport: &str, app: &str) -> Result<String, GenError> {
    let transport = parse_xml(transport)?;
    let app = parse_xml(app)?;
    pair_from_documents(&transport, &app, &mut Vec::new())
}

fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>, GenError> {
    roxmltree::Document::parse(text).map_err(|e| GenError::Xml(e.to_string()))
}

/// The FIX 4.4 table, from one parsed file. `build.rs` parses the file itself,
/// so its not-well-formed message names the path.
pub(crate) fn fix44_from_document(doc: &roxmltree::Document<'_>) -> Result<String, GenError> {
    let root = doc.root_element();
    let (number_of, type_of, enum_of) = collect_fields(root, "FIX44.xml")?;
    let (Some(header), Some(trailer)) = (child(root, "header"), child(root, "trailer")) else {
        return refuse("<header> or <trailer> section missing");
    };
    let Some(messages_el) = child(root, "messages") else {
        return refuse("<messages> section missing");
    };
    emit::emit(&Spec {
        dialect: "FIX 4.4",
        source: "the QuickFIX FIX 4.4 XML",
        number_of,
        type_of,
        enum_of,
        components: {
            let map = collect_components(root);
            refuse_empty_components(&map, "FIX44.xml")?;
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
///
/// A component one file leaves empty is reported in `warnings`, which
/// `build.rs` prints as `cargo:warning`s — before it looks at the result, so a
/// warning is shown even when a later refusal stops the build, as it was when
/// `build.rs` printed it itself.
#[cfg(feature = "fix50sp2")]
pub(crate) fn pair_from_documents(
    transport: &roxmltree::Document<'_>,
    app: &roxmltree::Document<'_>,
    warnings: &mut Vec<String>,
) -> Result<String, GenError> {
    let (troot, aroot) = (transport.root_element(), app.root_element());
    let (tnum, ttype, tenum) = collect_fields(troot, "FIXT11.xml")?;
    let (anum, atype, aenum) = collect_fields(aroot, "FIX50SP2.xml")?;
    // Taken before the merge, because after it there is no transport half left
    // to ask. ADR-0084 decision 1: what the transport file defines is a table
    // of its own, not a filter over the merged one.
    let transport_tags: Vec<u32> = tnum.values().copied().collect();
    let (number_of, type_of, enum_of) =
        merge::merge_fields((tnum, ttype, tenum), (anum, atype, aenum))?;

    let components = merge::merge_components(
        collect_components(troot),
        "FIXT11.xml",
        collect_components(aroot),
        "FIX50SP2.xml",
        warnings,
    )?;

    let (Some(header), Some(trailer)) = (child(troot, "header"), child(troot, "trailer")) else {
        return refuse("FIXT11.xml: <header> or <trailer> section missing");
    };
    let (Some(admin), Some(application)) = (child(troot, "messages"), child(aroot, "messages"))
    else {
        return refuse("<messages> section missing in FIXT11.xml or FIX50SP2.xml");
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
            Some(mt) => Ok(mt),
            None => refuse("FIXT11.xml: <message> without msgtype"),
        })
        .collect::<Result<_, _>>()?;

    let mut out = emit::emit(&Spec {
        dialect: "FIXT 1.1 / FIX 5.0 SP2",
        source: "the QuickFIX FIXT11.xml and FIX50SP2.xml pair",
        number_of,
        type_of,
        enum_of,
        components,
        messages,
        header,
        trailer,
        length_exceptions: model::SP2_LENGTH_EXCEPTIONS,
        per_token_enums: true,
    })?;
    out.push_str(&emit::emit_transport_layer(
        &transport_tags,
        &transport_msg_types,
    )?);
    Ok(out)
}
