//! A dictionary of the user's own, merged from an overlay onto the shipped FIX
//! 4.4 or read whole (ADR-0207 decisions 3 and 8).
//!
//! **What this proves:** each question is asked of the **merged model**
//! (`codegen::merged_model`), the thing the emitted `impl Dictionary` and
//! `impl Tables` are written from — never by searching the emitted text. An
//! overlay adds a field, an enum value, a group, a message type, a required
//! field and a header field, and each shows up where the parser, the validator
//! and the writer will look; the FIX 4.4 traps the base already survives
//! (`docs/reference/fix44-dictionary-traps.md`) still hold after the merge; a
//! conflicting overlay is refused naming both sides, by `merged_model` and by
//! `generate` alike; an empty overlay changes nothing, byte for byte.
//!
//! **What it does not prove:** that the file `generate` emits compiles in a
//! user's crate, or that an engine built on it answers the wire as the model
//! does — that is the example crate's job (plan steps 27 and 28). The fixture
//! is invented here (`fixtures/overlay-invented.xml`); no venue's rules of
//! engagement are tested, because none may be committed (ADR-0207
//! *Consequences*).
//!
//! `an_empty_overlay_emits_byte_identical_fix44` compares against
//! `$OUT_DIR/fix44.rs`, which `NANOFIX_FIX44_XML` can point at another file;
//! it is meant for the default build, as `gen_matches_build.rs` is.
#![cfg(feature = "codegen")]
#![allow(clippy::panic)]

use fixbolt_dict::FieldType;
use fixbolt_dict::codegen::{self, GenError, Model, Paths, Source};

const OVERLAY: &str = include_str!("fixtures/overlay-invented.xml");
const SHIPPED_FIX44: &str = include_str!("../spec/FIX44.xml");
const BUILT_FIX44: &str = include_str!(concat!(env!("OUT_DIR"), "/fix44.rs"));

/// An overlay with no sections at all, and one with every section present and
/// empty: both are "nothing to add".
const EMPTY_OVERLAYS: [&str; 2] = [
    "<fix type='FIX' major='4' minor='4' servicepack='0'></fix>",
    "<fix type='FIX' major='4' minor='4' servicepack='0'>\
     <header/><messages/><components/><fields/></fix>",
];

// Tags of the invented fixture.
const VENUE_CLIENT_ID: u32 = 5001;
const VENUE_SESSION_TAG: u32 = 5002;
const NO_VENUE_FEES: u32 = 5003;
const VENUE_FEE_TYPE: u32 = 5004;
const VENUE_FEE_AMT: u32 = 5005;
const VENUE_BLOB_LEN: u32 = 5006;
const VENUE_BLOB: u32 = 5007;
const VENUE_TRACE: u32 = 20_000;

fn model(source: Source<'_>) -> Model {
    match codegen::merged_model(source) {
        Ok(m) => m,
        Err(e) => panic!("merged_model returned Err({e:?}) — \"{e}\""),
    }
}

fn venue() -> Model {
    model(Source::Fix44Overlay(OVERLAY))
}

/// An overlay holding only a `<fields>` section.
fn fields_overlay(fields: &str) -> String {
    format!("<fix type='FIX' major='4' minor='4' servicepack='0'><fields>{fields}</fields></fix>")
}

/// The refusal `overlay` must meet: a [`GenError::Dictionary`] from
/// `merged_model`, and the same error from `generate` — the build failure a
/// user sees is the merge's own.
fn refusal(overlay: &str) -> String {
    let err = match codegen::merged_model(Source::Fix44Overlay(overlay)) {
        Ok(_) => panic!("the overlay merged; it must be refused:\n{overlay}"),
        Err(e) => e,
    };
    let msg = match &err {
        GenError::Dictionary(msg) => msg.clone(),
        other => {
            panic!("expected GenError::Dictionary naming the conflict, got {other:?} — \"{other}\"")
        }
    };
    assert_eq!(
        codegen::generate(Source::Fix44Overlay(overlay), "Venue", Paths::direct()).err(),
        Some(err),
        "generate must stop on the refusal merged_model gives"
    );
    msg
}

/// `msg` names every one of `tokens` as a whole word, ignoring case — so
/// `Symbol` is not found inside `VenueSymbol`, nor `55` inside `5510`.
fn assert_names(msg: &str, tokens: &[&str]) {
    let words: Vec<String> = msg
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(str::to_ascii_lowercase)
        .collect();
    for token in tokens {
        assert!(
            words.contains(&token.to_ascii_lowercase()),
            "the refusal must name {token:?}; it says:\n{msg}"
        );
    }
}

/// Fails naming the first line where two emitted files part.
fn assert_same_text(what: &str, got: &str, want: &str) {
    let first_difference = got
        .lines()
        .zip(want.lines())
        .enumerate()
        .find(|(_, (g, w))| g != w)
        .map(|(i, (g, w))| format!("line {}:\n  got:  {g}\n  want: {w}", i + 1));
    assert_eq!(first_difference, None, "{what}: the two texts differ");
    assert_eq!(
        got.len(),
        want.len(),
        "{what}: same lines as far as the shorter goes, different lengths"
    );
}

#[test]
fn an_empty_overlay_emits_byte_identical_fix44() {
    let shipped = model(Source::Fix44Whole(SHIPPED_FIX44));
    let shipped_file =
        codegen::generate(Source::Fix44Whole(SHIPPED_FIX44), "Fix44", Paths::direct());
    for empty in EMPTY_OVERLAYS {
        let merged = model(Source::Fix44Overlay(empty));
        let tables = match merged.crate_tables() {
            Ok(text) => text,
            Err(e) => panic!("crate_tables returned Err({e:?}) — \"{e}\""),
        };
        assert_same_text(
            &format!("{empty} against $OUT_DIR/fix44.rs"),
            &tables,
            BUILT_FIX44,
        );
        assert_eq!(
            merged, shipped,
            "{empty}: the merged model is not FIX 4.4's"
        );
        assert_eq!(
            codegen::generate(Source::Fix44Overlay(empty), "Fix44", Paths::direct()),
            shipped_file,
            "{empty}: generate differs from the shipped file generated whole"
        );
    }
}

#[test]
fn an_added_field_is_defined_and_typed() {
    let m = venue();
    for (tag, ty) in [
        (VENUE_CLIENT_ID, FieldType::String),
        (VENUE_SESSION_TAG, FieldType::String),
        (NO_VENUE_FEES, FieldType::NumInGroup),
        (VENUE_FEE_TYPE, FieldType::Char),
        (VENUE_FEE_AMT, FieldType::Amt),
        (VENUE_BLOB_LEN, FieldType::Length),
        (VENUE_BLOB, FieldType::Data),
        (VENUE_TRACE, FieldType::String),
    ] {
        assert!(m.is_defined_tag(tag), "{tag} is not defined");
        assert_eq!(m.field_type(tag), Some(ty), "type of {tag}");
    }
    // There is no user-defined tag range (fix44-dictionary-traps.md): a tag the
    // overlay does not name stays undefined, 5000 included.
    for tag in [5000, 5008, 19_999, 20_001] {
        assert!(
            !m.is_defined_tag(tag),
            "{tag} is defined but the overlay never names it"
        );
        assert_eq!(m.field_type(tag), None, "type of undefined {tag}");
    }
    // FIX 4.4's own fields are still there, and still what they were.
    assert_eq!(m.field_type(55), Some(FieldType::String), "Symbol(55)");
    assert_eq!(m.field_type(63), Some(FieldType::Char), "SettlType(63)");
    // An added DATA field pairs with its `{name}Len` field, and trap 1 —
    // Signature(89) takes SignatureLength(93), not 88 — survives the merge.
    assert_eq!(m.data_length_tag(VENUE_BLOB), Some(VENUE_BLOB_LEN));
    assert_eq!(m.data_length_tag(89), Some(93));
    assert!(m.allows(b"U1", VENUE_BLOB) && m.allows(b"U1", VENUE_TRACE));
}

#[test]
fn an_added_enum_value_is_allowed_and_the_old_ones_still_are() {
    let m = venue();
    assert_eq!(
        m.enum_allows(40, b"Z"),
        Some(true),
        "the added OrdType value"
    );
    for old in [
        "1", "2", "3", "4", "6", "7", "8", "9", "D", "E", "G", "I", "J", "K", "L", "M", "P",
    ] {
        assert_eq!(
            m.enum_allows(40, old.as_bytes()),
            Some(true),
            "FIX 4.4's OrdType {old} is no longer allowed"
        );
    }
    assert_eq!(
        m.enum_allows(40, b"5"),
        Some(false),
        "an OrdType neither side lists"
    );
    // The value lands on the field the overlay named and on no other.
    assert_eq!(m.enum_allows(54, b"Z"), Some(false), "Side(54) gained Z");
    // An added field's own list, and an added field with none.
    assert_eq!(m.enum_allows(VENUE_FEE_TYPE, b"1"), Some(true));
    assert_eq!(m.enum_allows(VENUE_FEE_TYPE, b"2"), Some(true));
    assert_eq!(m.enum_allows(VENUE_FEE_TYPE, b"3"), Some(false));
    assert_eq!(m.enum_allows(VENUE_CLIENT_ID, b"anything"), None);
}

#[test]
fn an_added_group_has_its_delimiter_members_and_declared_order() {
    let m = venue();
    // Declared as VenueFeeAmt, VenueFeeType, Currency: not ascending, so a
    // table that sorts would answer differently.
    assert_eq!(m.group_delimiter(b"U1", NO_VENUE_FEES), Some(VENUE_FEE_AMT));
    assert_eq!(
        m.group_members(b"U1", NO_VENUE_FEES),
        &[VENUE_FEE_AMT, VENUE_FEE_TYPE, 15]
    );
    assert!(m.allows(b"U1", NO_VENUE_FEES), "the counter is on the wire");
    assert!(
        m.allows(b"U1", 15),
        "a standard field inside an added group"
    );
}

#[test]
fn a_group_reused_in_two_messages_keeps_each_delimiter() {
    let m = venue();
    // One counter, two declarations: through the component VenueFeeGrp in D,
    // inline in U1. Keyed by (msg_type, counter), each keeps its own head.
    assert_eq!(m.group_delimiter(b"D", NO_VENUE_FEES), Some(VENUE_FEE_TYPE));
    assert_eq!(
        m.group_members(b"D", NO_VENUE_FEES),
        &[VENUE_FEE_TYPE, VENUE_FEE_AMT]
    );
    assert_eq!(m.group_delimiter(b"U1", NO_VENUE_FEES), Some(VENUE_FEE_AMT));
    // A message the overlay did not touch has no such group.
    assert_eq!(m.group_delimiter(b"G", NO_VENUE_FEES), None);
    assert_eq!(m.group_members(b"G", NO_VENUE_FEES), &[] as &[u32]);
    // Traps 5 and 6 survive the merge: a group reached only through a
    // component, and FIX 4.4's own counter with two delimiters.
    assert_eq!(m.group_delimiter(b"D", 386), Some(336), "(D, 386)");
    assert_eq!(m.group_delimiter(b"W", 268), Some(269), "(W, 268)");
    assert_eq!(m.group_delimiter(b"X", 268), Some(279), "(X, 268)");
}

#[test]
fn an_added_message_type_is_a_message_type_and_not_admin() {
    let m = venue();
    assert!(m.is_msg_type(b"U1"));
    assert!(!m.is_admin(b"U1"), "U1 is msgcat='app'");
    assert_eq!(
        m.required(b"U1"),
        &[11],
        "VenueFeeReport requires ClOrdID only"
    );
    assert!(!m.is_msg_type(b"U2"), "a type the overlay never declares");
    // FIX 4.4's own, including trap 4's self-closing XMLnonFIX.
    assert!(m.is_msg_type(b"D") && !m.is_admin(b"D"));
    assert!(m.is_msg_type(b"A") && m.is_admin(b"A"));
    assert!(m.is_msg_type(b"n"), "XMLnonFIX(n)");
}

#[test]
fn a_field_added_to_a_message_as_required_is_required() {
    let m = venue();
    // Trap 2's pinned set, plus the one the overlay made required.
    assert_eq!(m.required(b"D"), &[11, 40, 54, 60, VENUE_CLIENT_ID]);
    assert!(m.allows(b"D", VENUE_CLIENT_ID));
    // The component added to D is allowed, not required.
    assert!(m.allows(b"D", NO_VENUE_FEES) && m.allows(b"D", VENUE_FEE_AMT));
    // Only D gained it.
    assert!(!m.required(b"G").contains(&VENUE_CLIENT_ID));
    assert!(!m.allows(b"G", VENUE_CLIENT_ID));
}

#[test]
fn an_added_header_field_is_a_header_field() {
    let m = venue();
    assert!(m.is_header(VENUE_SESSION_TAG));
    assert!(
        !m.is_header(VENUE_CLIENT_ID),
        "a body field is not in the header"
    );
    // Trap 3 survives the merge: NoHops and its members are header fields.
    for tag in [49, 627, 628, 629, 630] {
        assert!(m.is_header(tag), "{tag} is no longer a header field");
    }
    assert!(
        !m.required(b"D").contains(&VENUE_SESSION_TAG),
        "a header field is not a body requirement"
    );
}

#[test]
fn a_number_reused_with_another_name_fails_naming_both() {
    let msg = refusal(&fields_overlay(
        "<field number='55' name='VenueInstrumentCode' type='STRING' />",
    ));
    assert_names(&msg, &["Symbol", "VenueInstrumentCode", "55"]);
}

#[test]
fn a_name_reused_with_another_number_fails_naming_both() {
    let msg = refusal(&fields_overlay(
        "<field number='5010' name='Symbol' type='STRING' />",
    ));
    assert_names(&msg, &["Symbol", "55", "5010"]);
}

#[test]
fn a_retyped_field_fails() {
    let msg = refusal(&fields_overlay(
        "<field number='55' name='Symbol' type='PRICE' />",
    ));
    assert_names(&msg, &["Symbol", "55", "STRING", "PRICE"]);
}

#[test]
fn a_data_field_added_without_its_length_field_fails() {
    // The control: the fixture's VenueBlob(5007) has VenueBlobLen(5006).
    assert_eq!(venue().data_length_tag(VENUE_BLOB), Some(VENUE_BLOB_LEN));
    let msg = refusal(&fields_overlay(
        "<field number='5020' name='VenueBlob' type='DATA' />",
    ));
    assert_names(&msg, &["VenueBlob", "VenueBlobLen"]);
}

#[test]
fn a_whole_file_generates_as_is() {
    // The user's own copy: SettlType retyped (which an overlay may not do —
    // QuickFIX's generated headers do call it STRING) and one field added.
    let whole = SHIPPED_FIX44
        .replacen(
            "<field number='63' name='SettlType' type='CHAR'>",
            "<field number='63' name='SettlType' type='STRING'>",
            1,
        )
        .replacen(
            "\n </fields>",
            "\n  <field number='5001' name='VenueClientID' type='STRING' />\n </fields>",
            1,
        );
    assert!(
        whole.contains("name='SettlType' type='STRING'") && whole.contains("name='VenueClientID'"),
        "the edits to the shipped text did not apply"
    );
    let m = model(Source::Fix44Whole(&whole));
    assert_eq!(
        m.field_type(63),
        Some(FieldType::String),
        "as the file says"
    );
    assert!(m.is_defined_tag(VENUE_CLIENT_ID));
    // Read as-is, not merged onto anything: nothing else appears.
    assert!(!m.is_defined_tag(VENUE_SESSION_TAG));
    assert!(!m.is_msg_type(b"U1"));
    assert_eq!(m.required(b"D"), &[11, 40, 54, 60]);
    // The shipped file read whole is FIX 4.4.
    assert_eq!(
        model(Source::Fix44Whole(SHIPPED_FIX44)).field_type(63),
        Some(FieldType::Char)
    );
    match codegen::generate(Source::Fix44Whole(&whole), "Venue", Paths::direct()) {
        Ok(text) => assert!(text.contains("struct Venue"), "no type named Venue"),
        Err(e) => panic!("generate returned Err({e:?}) — \"{e}\""),
    }
}

#[test]
fn a_high_tag_reports_the_table_size() {
    // FIX 4.4 alone: 956 is the highest tag, 15 words, 93 messages.
    let base = model(Source::Fix44Overlay(EMPTY_OVERLAYS[0])).table_size();
    assert_eq!(
        (
            base.max_tag,
            base.words,
            base.message_types,
            base.bitset_bytes
        ),
        (956, 15, 93, 94 * 15 * 8)
    );
    // One tag at 20 000 and one message more: 313 words each, ~21x the bytes.
    let size = venue().table_size();
    assert_eq!(
        (
            size.max_tag,
            size.words,
            size.message_types,
            size.bitset_bytes
        ),
        (VENUE_TRACE, 313, 94, 95 * 313 * 8)
    );
    let said = size.to_string();
    assert_names(&said, &["20000", "313", "237880"]);
}

/// An overlay holding the sections given, verbatim.
fn overlay(sections: &str) -> String {
    format!("<fix type='FIX' major='4' minor='4' servicepack='0'>{sections}</fix>")
}

#[test]
fn an_existing_message_repeated_with_another_msgtype_fails_naming_both() {
    let msg = refusal(&overlay(
        "<messages><message name='NewOrderSingle' msgtype='U9' msgcat='app'>\
         <field name='Text' required='N' /></message></messages>",
    ));
    assert_names(&msg, &["NewOrderSingle", "D", "U9"]);
    // The other way round: a new name on a msgtype FIX 4.4 already uses.
    let msg = refusal(&overlay(
        "<messages><message name='VenueOrder' msgtype='D' msgcat='app'>\
         <field name='Text' required='N' /></message></messages>",
    ));
    assert_names(&msg, &["NewOrderSingle", "VenueOrder", "D"]);
}

#[test]
fn an_existing_message_repeated_with_another_msgcat_fails_naming_both() {
    let msg = refusal(&overlay(
        "<messages><message name='NewOrderSingle' msgtype='D' msgcat='admin'>\
         <field name='Text' required='N' /></message></messages>",
    ));
    assert_names(&msg, &["NewOrderSingle", "app", "admin"]);
}

#[test]
fn an_unknown_field_referenced_by_the_overlay_fails_naming_it() {
    let msg = refusal(&overlay(
        "<messages><message name='NewOrderSingle' msgtype='D' msgcat='app'>\
         <field name='VenueNowhere' required='N' /></message></messages>",
    ));
    assert_names(&msg, &["NewOrderSingle", "VenueNowhere"]);
    let msg = refusal(&overlay(
        "<header><field name='VenueNowhere' required='N' /></header>",
    ));
    assert_names(&msg, &["header", "VenueNowhere"]);
}

#[test]
fn a_field_added_twice_to_one_message_fails_naming_both() {
    // Twice in the overlay itself.
    let msg = refusal(&overlay(
        "<messages><message name='NewOrderSingle' msgtype='D' msgcat='app'>\
         <field name='VenueClientID' required='N' />\
         <field name='VenueClientID' required='Y' /></message></messages>\
         <fields><field number='5001' name='VenueClientID' type='STRING' /></fields>",
    ));
    assert_names(&msg, &["NewOrderSingle", "VenueClientID"]);
    // Once in the overlay, once already in FIX 4.4's NewOrderSingle.
    let msg = refusal(&overlay(
        "<messages><message name='NewOrderSingle' msgtype='D' msgcat='app'>\
         <field name='ClOrdID' required='N' /></message></messages>",
    ));
    assert_names(&msg, &["NewOrderSingle", "ClOrdID"]);
    // Already there through a component: Symbol(55) is in Instrument.
    let msg = refusal(&overlay(
        "<messages><message name='NewOrderSingle' msgtype='D' msgcat='app'>\
         <field name='Symbol' required='Y' /></message></messages>",
    ));
    assert_names(&msg, &["NewOrderSingle", "Symbol", "Instrument"]);
}

#[test]
fn an_overlay_trailer_fails() {
    let msg = refusal(&overlay(
        "<trailer><field name='Text' required='N' /></trailer>",
    ));
    assert_names(&msg, &["trailer"]);
}

#[test]
fn an_unknown_overlay_section_fails_rather_than_being_ignored() {
    // A misspelt section would otherwise drop every addition in it silently.
    let msg = refusal(&overlay(
        "<feilds><field number='5001' name='VenueClientID' type='STRING' /></feilds>",
    ));
    assert_names(&msg, &["feilds"]);
}

#[test]
fn an_added_header_group_and_its_members_are_header_fields() {
    // The header already holds a group, NoHops(627) — trap 3 — and a header
    // group is keyed for every message type.
    let m = model(Source::Fix44Overlay(&overlay(
        "<header><group name='NoVenueRoutes' required='N'>\
         <field name='VenueRouteID' required='N' />\
         <field name='VenueRouteTime' required='N' /></group></header>\
         <fields><field number='5008' name='NoVenueRoutes' type='NUMINGROUP' />\
         <field number='5009' name='VenueRouteID' type='STRING' />\
         <field number='5010' name='VenueRouteTime' type='UTCTIMESTAMP' /></fields>",
    )));
    for tag in [5008, 5009, 5010, 627] {
        assert!(m.is_header(tag), "{tag} is not a header field");
    }
    for mt in [&b"D"[..], b"A", b"8"] {
        assert_eq!(m.group_delimiter(mt, 5008), Some(5009));
        assert_eq!(m.group_members(mt, 5008), &[5009, 5010]);
        assert!(m.allows(mt, 5008), "a header group is allowed everywhere");
    }
}

#[test]
fn a_value_already_listed_with_another_description_is_accepted_because_descriptions_are_not_on_the_wire()
 {
    let m = model(Source::Fix44Overlay(&overlay(
        "<fields><field number='40' name='OrdType' type='CHAR'>\
         <value enum='1' description='VENUE_MARKET' /></field></fields>",
    )));
    assert_eq!(m.enum_allows(40, b"1"), Some(true));
    assert_eq!(
        m,
        model(Source::Fix44Whole(SHIPPED_FIX44)),
        "a repeated value changed the model"
    );
}

#[test]
fn a_value_list_on_a_field_fix44_leaves_open_fails() {
    // Symbol(55) takes any value in FIX 4.4. A value list would turn every
    // other symbol into 373=5: that narrows the field, it does not add to it.
    let msg = refusal(&fields_overlay(
        "<field number='55' name='Symbol' type='STRING'>\
         <value enum='VENUE' description='INVENTED' /></field>",
    ));
    assert_names(&msg, &["Symbol", "55"]);
}

#[test]
fn the_trailer_gap_is_still_open_after_the_merge() {
    // fix44-dictionary-traps.md, trap 3 "Still open": the trailer's three tags
    // are classified as neither header nor body. The merge must not move them.
    let m = venue();
    for tag in [89, 93, 10] {
        assert!(!m.is_header(tag), "{tag} became a header field");
        assert!(m.allows(b"D", tag), "{tag} is folded into every message");
    }
}

#[test]
fn an_added_value_on_a_multi_value_field_is_checked_per_token() {
    // ExecInst(18) is MULTIPLEVALUESTRING: `18=2 A` is two values, each checked.
    let m = model(Source::Fix44Overlay(&overlay(
        "<fields><field number='18' name='ExecInst' type='MULTIPLEVALUESTRING'>\
         <value enum='f' description='INVENTED_VENUE_INST' /></field></fields>",
    )));
    assert_eq!(m.enum_allows(18, b"f"), Some(true));
    assert_eq!(m.enum_allows(18, b"2 f"), Some(true));
    assert_eq!(m.enum_allows(18, b"2 A"), Some(true), "FIX 4.4's own pair");
    assert_eq!(
        m.enum_allows(18, b"f g"),
        Some(false),
        "g is in neither list"
    );
}

fn venue_file(paths: Paths) -> String {
    match codegen::generate(Source::Fix44Overlay(OVERLAY), "Venue", paths) {
        Ok(text) => text,
        Err(e) => panic!("generate returned Err({e:?}) — \"{e}\""),
    }
}

#[test]
fn the_generated_file_opens_with_its_format_check() {
    // The doctests in src/codegen/mod.rs compile this line at the current
    // format and fail it (E0080) at another; this holds the emitted line to
    // their text.
    let v = codegen::FORMAT_VERSION;
    for (paths, constant) in [
        (
            Paths::direct(),
            "::fixbolt_dict::codegen_format::FORMAT_VERSION",
        ),
        (
            Paths::facade(),
            "::fixbolt::dict::codegen_format::FORMAT_VERSION",
        ),
    ] {
        let check = format!(
            "const _: () = assert!(\n    {constant} == {v},\n    \"Venue was generated by \
             fixbolt-dict's codegen at format {v}, and the fixbolt-dict it is compiled against \
             reads another format: give the build-dependency and the dependency the same \
             fixbolt version\"\n);\n"
        );
        assert!(
            venue_file(paths).contains(&check),
            "{paths:?}: no format check of the expected text"
        );
    }
    // One number, read by the generator and by the runtime crate.
    assert_eq!(v, fixbolt_dict::codegen_format::FORMAT_VERSION);
    assert_eq!(
        v, 1,
        "the format moved: move the two doctests in src/codegen/mod.rs with it"
    );
}

#[test]
fn generate_names_the_facade_or_the_crates_by_paths() {
    let facade = venue_file(Paths::facade());
    for wanted in [
        "impl ::fixbolt::dict::Dictionary for Venue",
        "impl ::fixbolt::dict::Tables for Venue",
        "Option<::fixbolt::dict::FieldType>",
    ] {
        assert!(facade.contains(wanted), "facade: no {wanted:?}");
    }
    for unwanted in ["::fixbolt_dict", "::fixbolt_codec", "crate::"] {
        assert!(!facade.contains(unwanted), "facade names {unwanted:?}");
    }
    let direct = venue_file(Paths::direct());
    for wanted in [
        "impl ::fixbolt_codec::Dictionary for Venue",
        "impl ::fixbolt_dict::Tables for Venue",
        "Option<::fixbolt_dict::FieldType>",
    ] {
        assert!(direct.contains(wanted), "direct: no {wanted:?}");
    }
    for unwanted in ["::fixbolt::", "crate::"] {
        assert!(!direct.contains(unwanted), "direct names {unwanted:?}");
    }
    assert_eq!(Paths::default(), Paths::facade());
}

#[test]
fn a_type_name_that_cannot_name_a_struct_fails() {
    for bad in ["", "9Venue", "Ven ue", "Venue::X", "tables", "_"] {
        match codegen::generate(Source::Fix44Overlay(OVERLAY), bad, Paths::direct()) {
            Err(GenError::Dictionary(msg)) => assert!(msg.contains("type name"), "{bad:?}: {msg}"),
            other => panic!(
                "{bad:?}: expected a refusal, got {:?}",
                other.map(|t| t.len())
            ),
        }
    }
}
