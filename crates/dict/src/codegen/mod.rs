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

use crate::field_type::FieldType;
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

/// Merges `source` into the one dictionary a generated file would be built
/// from, and returns it for inspection (ADR-0207 decision 3).
///
/// [`generate`] is this followed by emitting; a refusal here is the build
/// failure a user sees. The [`Model`] answers the questions the emitted
/// `impl Dictionary` and `impl Tables` answer, so a merge can be tested
/// without compiling what it emits.
///
/// # Errors
///
/// [`GenError::Unsupported`], for every input, until the overlay lands.
pub fn merged_model(source: Source<'_>) -> Result<Model, GenError> {
    let _ = source;
    Err(GenError::Unsupported)
}

/// A merged dictionary: FIX 4.4 with an overlay applied, or a whole file.
///
/// Each query is named after, and answers as, the associated function of the
/// same name on `fixbolt_codec::Dictionary` or [`crate::Tables`] that the
/// emitted type implements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    // Uninhabited until the overlay lands: no `Model` can be built, so every
    // query below is unreachable rather than a wrong answer.
    never: Never,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Never {}

impl Model {
    /// Whether the dictionary defines `tag` at all.
    #[must_use]
    pub fn is_defined_tag(&self, tag: u32) -> bool {
        let _ = tag;
        match self.never {}
    }

    /// The declared type of `tag`, `None` if it is not defined.
    #[must_use]
    pub fn field_type(&self, tag: u32) -> Option<FieldType> {
        let _ = tag;
        match self.never {}
    }

    /// `Some(allowed)` for an enumerated field, `None` for one with no value
    /// list.
    #[must_use]
    pub fn enum_allows(&self, tag: u32, value: &[u8]) -> Option<bool> {
        let _ = (tag, value);
        match self.never {}
    }

    /// Whether `msg_type` is a message type of this dictionary.
    #[must_use]
    pub fn is_msg_type(&self, msg_type: &[u8]) -> bool {
        let _ = msg_type;
        match self.never {}
    }

    /// Whether `msg_type` is declared `msgcat='admin'`.
    #[must_use]
    pub fn is_admin(&self, msg_type: &[u8]) -> bool {
        let _ = msg_type;
        match self.never {}
    }

    /// The tags `msg_type` must carry, ascending.
    #[must_use]
    pub fn required(&self, msg_type: &[u8]) -> &[u32] {
        let _ = msg_type;
        match self.never {}
    }

    /// Whether `msg_type` may carry `tag`.
    #[must_use]
    pub fn allows(&self, msg_type: &[u8], tag: u32) -> bool {
        let _ = (msg_type, tag);
        match self.never {}
    }

    /// Whether `tag` belongs to the standard header.
    #[must_use]
    pub fn is_header(&self, tag: u32) -> bool {
        let _ = tag;
        match self.never {}
    }

    /// The length field in front of a DATA field.
    #[must_use]
    pub fn data_length_tag(&self, tag: u32) -> Option<u32> {
        let _ = tag;
        match self.never {}
    }

    /// The first declared member of the group `counter` in `msg_type`.
    #[must_use]
    pub fn group_delimiter(&self, msg_type: &[u8], counter: u32) -> Option<u32> {
        let _ = (msg_type, counter);
        match self.never {}
    }

    /// The members of the group `counter` in `msg_type`, in declaration
    /// order; empty if `msg_type` has no such group.
    #[must_use]
    pub fn group_members(&self, msg_type: &[u8], counter: u32) -> &[u32] {
        let _ = (msg_type, counter);
        match self.never {}
    }

    /// The size of the per-tag bitsets the emitted file will hold, which the
    /// highest tag decides (ADR-0207 *Consequences*).
    #[must_use]
    pub fn table_size(&self) -> TableSize {
        match self.never {}
    }

    /// The tables alone, naming `crate::` — the text [`fix44_tables`] returns
    /// for the same dictionary. Hidden: it exists so a test can hold an empty
    /// overlay to [`crate::Fix44`]'s own bytes; a user's file comes from
    /// [`generate`].
    ///
    /// # Errors
    ///
    /// Whatever [`fix44_tables`] refuses.
    #[doc(hidden)]
    pub fn crate_tables(&self) -> Result<String, GenError> {
        match self.never {}
    }
}

/// How large the per-tag bitsets of a generated dictionary are.
///
/// `ALLOWED` holds one bitset per message type and `DEFINED_TAGS` one more,
/// each `words` 64-bit words over `0..=max_tag`. One custom tag at 20 000
/// makes every one of them 313 words where FIX 4.4's are 15.
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
