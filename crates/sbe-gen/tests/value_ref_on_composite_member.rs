//! `presence="constant" valueRef="Enum.Value"` on a `<type>` **inside a
//! composite** (phase 3 row 9, plan step 2; ADR-0140 decision 4).
//!
//! The SBE 1.0 Standard's prose `<type>` attribute table
//! (`04MessageSchema.md:142-155`) omits `valueRef`, which its field table
//! lists (`:464`); but `sbe.xsd` allows it on `<type>` (`encodedDataType` takes
//! `presenceAttributes`, `sbe.xsd:177,368`), the Standard's own timestamp
//! examples put it on a composite member (`02FieldEncoding.md:884`), and Real
//! Logic's `sbe-tool` reads it there (`EncodedDataType.java`,
//! `lookupValueRef`). The generator follows the xsd, the examples and
//! `sbe-tool`. Before this, such a member was read as a constant with empty
//! text and failed with the misleading `constant '' is not an unsigned
//! integer`. The trap: `docs/reference/sbe-valueref-on-a-composite-member.md`.
//!
//! The schema is `tests/fixtures/value_ref_on_composite_member.xml`, written
//! for this repository. The error cases are derived from it by one textual
//! substitution each, so the fixture stays the single source.
//!
//! Integration tests are their own crate root; the workspace's panic-family
//! denies are relaxed here as in `tests/generated.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

const FIXTURE: &str = include_str!("fixtures/value_ref_on_composite_member.xml");

fn generated(xml: &str) -> String {
    match fixbolt_sbe_gen::generate(xml) {
        Ok(source) => source,
        Err(e) => panic!("generate failed: {e}"),
    }
}

fn error_of(xml: &str) -> String {
    match fixbolt_sbe_gen::generate(xml) {
        Ok(_) => panic!("generate accepted a schema it must refuse"),
        Err(e) => e.to_string(),
    }
}

/// `xml` with `from` replaced, after checking `from` occurs exactly once —
/// a substitution that silently matched nothing would test the fixture.
fn with(from: &str, to: &str) -> String {
    assert_eq!(
        FIXTURE.matches(from).count(),
        1,
        "substitution '{from}' must match the fixture exactly once"
    );
    FIXTURE.replacen(from, to, 1)
}

#[test]
fn a_value_ref_constant_in_a_composite_takes_the_named_valid_value_and_zero_wire_bytes() {
    let source = generated(FIXTURE);

    // `sent`: 8 wire bytes (the uint64 only), `unit` is TimeUnit.nanosecond = 9.
    let sent = "FieldLayout { id: 1, name: \"sent\", offset: 0, len: 8, since_version: 0, elements: &[\
        Element { name: \"time\", offset: 0, primitive: Primitive::UInt64, length: 1, presence: Presence::Required }, \
        Element { name: \"unit\", offset: 0, primitive: Primitive::UInt8, length: 1, presence: Presence::Constant(Value::UInt(9)) }] }";
    // `received`: same shape, a different validValue — TimeUnit.millisecond = 3.
    let received = "FieldLayout { id: 2, name: \"received\", offset: 8, len: 8, since_version: 0, elements: &[\
        Element { name: \"time\", offset: 0, primitive: Primitive::UInt64, length: 1, presence: Presence::Required }, \
        Element { name: \"unit\", offset: 0, primitive: Primitive::UInt8, length: 1, presence: Presence::Constant(Value::UInt(3)) }] }";
    // `after` sits right behind two 8-byte timestamps: the constants took no bytes.
    let after = "FieldLayout { id: 3, name: \"after\", offset: 16, len: 4, since_version: 0, elements: &[\
        Element { name: \"\", offset: 0, primitive: Primitive::UInt32, length: 1, presence: Presence::Required }] }";

    assert!(
        source.contains(sent),
        "no `sent` layout as expected in:\n{source}"
    );
    assert!(
        source.contains(received),
        "no `received` layout as expected in:\n{source}"
    );
    assert!(
        source.contains(after),
        "no `after` layout as expected in:\n{source}"
    );
    assert!(
        source.contains("MessageLayout { template_id: 1, name: \"Stamped\", block_length: 20,"),
        "block length is not 8 + 8 + 4 in:\n{source}"
    );
}

#[test]
fn a_value_ref_to_an_enum_that_does_not_exist_is_refused_naming_value_ref() {
    let e = error_of(&with(
        "valueRef=\"TimeUnit.nanosecond\"",
        "valueRef=\"NoSuchUnit.nanosecond\"",
    ));
    assert!(e.contains("valueRef"), "error does not name valueRef: {e}");
    assert!(
        e.contains("NoSuchUnit"),
        "error does not name the enum: {e}"
    );
}

#[test]
fn a_value_ref_to_a_valid_value_that_does_not_exist_is_refused_naming_value_ref() {
    let e = error_of(&with(
        "valueRef=\"TimeUnit.nanosecond\"",
        "valueRef=\"TimeUnit.picosecond\"",
    ));
    assert!(e.contains("valueRef"), "error does not name valueRef: {e}");
    assert!(
        e.contains("picosecond"),
        "error does not name the value: {e}"
    );
}

/// `sbe-tool` refuses an enum whose encoding differs from the member's
/// `primitiveType` ("valueRef does not match this type"); reading `9` as a
/// `uint16` would be a layout nobody on the other end agrees with.
#[test]
fn a_value_ref_whose_enum_encoding_differs_from_the_member_type_is_refused() {
    let e = error_of(&with(
        "<type name=\"unit\" primitiveType=\"uint8\" presence=\"constant\" valueRef=\"TimeUnit.nanosecond\"/>",
        "<type name=\"unit\" primitiveType=\"uint16\" presence=\"constant\" valueRef=\"TimeUnit.nanosecond\"/>",
    ));
    assert!(e.contains("valueRef"), "error does not name valueRef: {e}");
}

/// SBE: valueRef is "valid only if presence=constant"; `sbe-tool` refuses it
/// otherwise. Ignoring it would silently put a byte on the wire the schema
/// author meant to be a constant.
#[test]
fn a_value_ref_on_a_member_that_is_not_constant_is_refused() {
    let e = error_of(&with(
        "presence=\"constant\" valueRef=\"TimeUnit.nanosecond\"",
        "valueRef=\"TimeUnit.nanosecond\"",
    ));
    assert!(e.contains("valueRef"), "error does not name valueRef: {e}");
}
