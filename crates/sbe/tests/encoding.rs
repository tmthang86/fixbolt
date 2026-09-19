//! `Sbe<S>` under `codec::Encoding`, and the writer's refusals, over tables
//! written by hand (plan step C4). The same checks over the tables `sbe-gen`
//! generates, and the byte-for-byte re-encode of every RC4 §7 dump, live in
//! `crates/sbe-gen/tests/encoding.rs`.
//!
//! The order message is SBE 1.0 RC4 §7 "Wire format of an order message", the
//! 6-byte SOFH stripped; its table is transcribed as `crates/sbe/src/tests.rs`
//! transcribes it.
#![cfg(feature = "encoding")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fixbolt_sbe::{
    ByteOrder, Dictionary, DimensionLayout, Element, Encoding, FieldId, FieldLayout, GroupLayout,
    LengthType, MessageLayout, MessageWriter, Parsed, Presence, Primitive, Sbe, SbeError,
    SbeTables, SbeTemplate, SbeView, Schema, Validation, Value, VarDataLayout,
};

const fn scalar(primitive: Primitive, length: u16) -> Element {
    Element {
        name: "",
        offset: 0,
        primitive,
        length,
        presence: Presence::Required,
    }
}
const fn field(id: u16, offset: u16, len: u16, e: &'static [Element]) -> FieldLayout {
    FieldLayout {
        id,
        name: "",
        offset,
        len,
        since_version: 0,
        elements: e,
    }
}

const CHAR8: &[Element] = &[scalar(Primitive::Char, 8)];
const CHAR: &[Element] = &[scalar(Primitive::Char, 1)];
const UINT64: &[Element] = &[scalar(Primitive::UInt64, 1)];
const QTY: &[Element] = &[
    Element {
        name: "mantissa",
        ..scalar(Primitive::Int32, 1)
    },
    Element {
        name: "exponent",
        offset: 0,
        primitive: Primitive::Int8,
        length: 1,
        presence: Presence::Constant(Value::Int(0)),
    },
];
const OPT_DECIMAL: &[Element] = &[
    Element {
        name: "mantissa",
        offset: 0,
        primitive: Primitive::Int64,
        length: 1,
        presence: Presence::Optional {
            null: Value::Int(i64::MIN),
        },
    },
    Element {
        name: "exponent",
        offset: 0,
        primitive: Primitive::Int8,
        length: 1,
        presence: Presence::Constant(Value::Int(-3)),
    },
];

static NOS_FIELDS: [FieldLayout; 9] = [
    field(11, 0, 8, CHAR8),
    field(1, 8, 8, CHAR8),
    field(55, 16, 8, CHAR8),
    field(54, 24, 1, CHAR),
    field(60, 25, 8, UINT64),
    field(38, 33, 4, QTY),
    field(40, 37, 1, CHAR),
    field(44, 38, 8, OPT_DECIMAL),
    field(99, 46, 8, OPT_DECIMAL),
];
static NEW_ORDER_SINGLE: MessageLayout = MessageLayout {
    template_id: 99,
    name: "NewOrderSingle",
    block_length: 54,
    since_version: 0,
    fields: &NOS_FIELDS,
    groups: &[],
    var_data: &[],
};

/// A message with a group and a `uint8`-prefixed `varData`, for the tail and
/// the length refusals.
static SMALL_ENTRY: [FieldLayout; 1] = [field(2, 0, 1, CHAR)];
static SMALL_GROUPS: [GroupLayout; 1] = [GroupLayout {
    id: 10,
    name: "G",
    block_length: 1,
    since_version: 0,
    dimension: DimensionLayout {
        block_length: LengthType::U8,
        num_in_group: LengthType::U8,
    },
    fields: &SMALL_ENTRY,
    groups: &[],
    var_data: &[],
}];
static SMALL_DATA: [VarDataLayout; 1] = [VarDataLayout {
    id: 20,
    name: "D",
    length: LengthType::U8,
    since_version: 0,
}];
static SMALL_FIELDS: [FieldLayout; 1] = [field(1, 0, 8, UINT64)];
static SMALL: MessageLayout = MessageLayout {
    template_id: 5,
    name: "Small",
    block_length: 8,
    since_version: 0,
    fields: &SMALL_FIELDS,
    groups: &SMALL_GROUPS,
    var_data: &SMALL_DATA,
};

struct Examples;
impl Schema for Examples {
    const ID: u16 = 100;
    const VERSION: u16 = 0;
    const BYTE_ORDER: ByteOrder = ByteOrder::Little;
    fn message(template_id: u16) -> Option<&'static MessageLayout> {
        match template_id {
            99 => Some(&NEW_ORDER_SINGLE),
            5 => Some(&SMALL),
            _ => None,
        }
    }
}

/// The same layouts, big-endian.
struct ExamplesBe;
impl Schema for ExamplesBe {
    const ID: u16 = 100;
    const VERSION: u16 = 0;
    const BYTE_ORDER: ByteOrder = ByteOrder::Big;
    fn message(template_id: u16) -> Option<&'static MessageLayout> {
        Examples::message(template_id)
    }
}

type E = Sbe<Examples>;

/// RC4 §7 order message, SOFH stripped: 8-byte header + 54-byte block.
const ORDER: [u8; 62] = [
    0x36, 0x00, 0x63, 0x00, 0x64, 0x00, 0x00, 0x00, // header
    b'O', b'R', b'D', b'0', b'0', b'0', b'0', b'1', // ClOrdID
    b'A', b'C', b'C', b'T', b'0', b'1', 0x00, 0x00, // Account
    b'G', b'E', b'M', b'4', 0x00, 0x00, 0x00, 0x00, // Symbol
    b'1', // Side
    0x00, 0x84, 0x68, 0x90, 0xfe, 0xa8, 0x9a, 0x13, // TransactTime
    0x07, 0x00, 0x00, 0x00, // OrderQty
    b'2', // OrdType
    0x1a, 0x85, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, // Price
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, // StopPx: null
];

fn parsed(buf: &[u8]) -> (Result<Parsed, SbeError>, <E as Encoding>::Scratch) {
    let mut scratch = Default::default();
    (E::parse(buf, &mut scratch, Validation::ALL), scratch)
}

// --- the trait's read side ----------------------------------------------------

#[test]
fn field_through_the_trait_is_the_spec_dumps_bytes() {
    let (r, scratch) = parsed(&ORDER);
    assert_eq!(r, Ok(Parsed::Complete { consumed: 62 }));
    let view = E::view(&scratch, &ORDER);
    let expect: [(u16, &[u8]); 9] = [
        (11, b"ORD00001"),
        (1, b"ACCT01\0\0"),
        (55, b"GEM4\0\0\0\0"),
        (54, b"1"),
        (60, &[0x00, 0x84, 0x68, 0x90, 0xfe, 0xa8, 0x9a, 0x13]),
        (38, &[0x07, 0, 0, 0]),
        (40, b"2"),
        (44, &[0x1a, 0x85, 0x01, 0, 0, 0, 0, 0]),
        (99, &[0, 0, 0, 0, 0, 0, 0, 0x80]),
    ];
    for (id, bytes) in expect {
        assert_eq!(E::field(view, FieldId(id)), Some(bytes), "field {id}");
    }
    assert_eq!(
        E::field(view, FieldId(12345)),
        None,
        "not a field of the message"
    );
}

#[test]
fn sbe_carries_no_session_fields() {
    let (_, scratch) = parsed(&ORDER);
    assert_eq!(E::session_fields(E::view(&scratch, &ORDER)), None);
}

#[test]
fn the_dictionary_answers_no_tag_value_question() {
    assert!(!SbeTables::<Examples>::is_header(35));
    assert_eq!(SbeTables::<Examples>::data_length_tag(58), None);
}

#[test]
fn parse_measures_the_message_and_waits_for_the_rest() {
    let mut longer = [0u8; 70];
    longer[..62].copy_from_slice(&ORDER);
    assert_eq!(parsed(&longer).0, Ok(Parsed::Complete { consumed: 62 }));
    for n in 0..ORDER.len() {
        assert_eq!(parsed(&ORDER[..n]).0, Ok(Parsed::Incomplete), "prefix {n}");
    }
}

#[test]
fn parse_refuses_another_schema_and_an_unknown_template() {
    let mut other = ORDER;
    other[4] = 0x5b; // schemaId 91
    let (r, scratch) = parsed(&other);
    assert_eq!(r, Err(SbeError::WrongSchema));
    assert_eq!(
        scratch.schema_id, 91,
        "the scratch holds the header it read"
    );
    assert_eq!(E::field(E::view(&scratch, &other), FieldId(11)), None);

    let mut unknown = ORDER;
    unknown[2] = 0x01; // templateId 1
    assert_eq!(parsed(&unknown).0, Err(SbeError::UnknownTemplate));
}

#[test]
fn a_view_from_a_default_scratch_reads_nothing_and_does_not_panic() {
    let scratch = Default::default();
    assert_eq!(E::field(E::view(&scratch, &[]), FieldId(11)), None);
    assert_eq!(E::field(E::view(&scratch, &ORDER), FieldId(11)), None);
}

// --- the trait's write side: a flat message through a template ------------------

fn order_slots() -> [(FieldId, [u8; 8], usize); 8] {
    let mut s = [(FieldId(0), [0u8; 8], 0usize); 8];
    let src: [(u16, &[u8]); 8] = [
        (11, b"ORD00001"),
        (1, b"ACCT01\0\0"),
        (55, b"GEM4\0\0\0\0"),
        (54, b"1"),
        (60, &1_412_627_244_432_000_000u64.to_le_bytes()),
        (38, &7i32.to_le_bytes()),
        (40, b"2"),
        (44, &99_610i64.to_le_bytes()),
    ];
    for (slot, (id, b)) in s.iter_mut().zip(src) {
        slot.0 = FieldId(id);
        slot.1[..b.len()].copy_from_slice(b);
        slot.2 = b.len();
    }
    s
}

#[test]
fn encode_through_a_template_writes_the_spec_order_message() -> Result<(), SbeError> {
    let t: <E as Encoding>::Template<0, 64> = SbeTemplate::new(99)?;
    assert_eq!(t.wire_len(), 62);
    let owned = order_slots();
    let slots: Vec<(FieldId, &[u8])> = owned.iter().map(|(f, b, n)| (*f, &b[..*n])).collect();
    let mut out = [0xAAu8; 80];
    let range = E::encode::<0, 64>(&t, &mut out, &slots)?;
    assert_eq!(range, 0..62);
    assert_eq!(&out[range], &ORDER[..]);
    assert_eq!(out[62], 0xAA, "nothing past the message is touched");
    Ok(())
}

#[test]
fn a_template_preset_is_carried_by_every_message() -> Result<(), SbeError> {
    let mut t: SbeTemplate<Examples, 64> = SbeTemplate::new(99)?;
    t.set_bytes(FieldId(1), b"ACCT01\0\0")?;
    t.set_element(FieldId(38), 0, Some(Value::Int(7)))?;
    let mut out = [0u8; 64];
    let r = t.encode(&mut out, &[(FieldId(11), b"ORD00001")])?;
    let view = SbeView::decode::<Examples>(&out[r])?;
    let root = view.root::<Examples>()?;
    assert_eq!(
        root.value(&NOS_FIELDS[1])?,
        Some(Value::Array(b"ACCT01\0\0"))
    );
    assert_eq!(root.value(&NOS_FIELDS[5])?, Some(Value::Int(7)));
    assert_eq!(root.value(&NOS_FIELDS[7])?, None, "Price never set: null");
    Ok(())
}

#[test]
fn a_bad_slot_is_refused_before_anything_is_written() -> Result<(), SbeError> {
    let t: SbeTemplate<Examples, 64> = SbeTemplate::new(99)?;
    let mut out = [0xAAu8; 64];
    let good: (FieldId, &[u8]) = (FieldId(11), b"ORD00001");
    assert_eq!(
        t.encode(&mut out, &[good, (FieldId(9999), b"x")]),
        Err(SbeError::UnknownField)
    );
    assert_eq!(
        t.encode(&mut out, &[good, (FieldId(54), b"12")]),
        Err(SbeError::WrongLength)
    );
    assert_eq!(out, [0xAA; 64], "neither call wrote a byte");
    assert_eq!(
        t.encode(&mut out[..61], &[good]),
        Err(SbeError::BufferTooSmall)
    );
    Ok(())
}

#[test]
fn a_template_that_does_not_fit_its_capacity_is_refused() {
    assert_eq!(
        SbeTemplate::<Examples, 61>::new(99).map(|t| t.wire_len()),
        Err(SbeError::BufferTooSmall)
    );
    assert_eq!(
        SbeTemplate::<Examples, 64>::new(1).map(|t| t.wire_len()),
        Err(SbeError::UnknownTemplate)
    );
}

#[test]
fn a_template_with_a_group_and_var_data_writes_them_empty() -> Result<(), SbeError> {
    let t: SbeTemplate<Examples, 32> = SbeTemplate::new(5)?;
    // header 8 + block 8 + dimension (u8, u8) 2 + varData length (u8) 1.
    assert_eq!(t.wire_len(), 19);
    assert_eq!(
        &t.as_bytes()[16..],
        &[1, 0, 0],
        "blockLength 1, count 0, length 0"
    );
    let mut out = [0u8; 32];
    let r = E::encode::<0, 32>(&t, &mut out, &[])?;
    assert_eq!(parsed(&out[r]).0, Ok(Parsed::Complete { consumed: 19 }));
    Ok(())
}

// --- the native writer's refusals ----------------------------------------------

#[test]
fn values_that_do_not_fit_their_element_are_bad_values() -> Result<(), SbeError> {
    let mut out = [0u8; 64];
    let mut w = MessageWriter::new::<Examples>(&mut out, 99)?;
    let side = &NOS_FIELDS[3];
    let qty = &NOS_FIELDS[5];
    let price = &NOS_FIELDS[7];
    assert_eq!(w.put(side, Value::UInt(1)), Err(SbeError::BadValue), "kind");
    assert_eq!(
        w.put(qty, Value::Int(i64::from(i32::MAX) + 1)),
        Err(SbeError::BadValue),
        "range"
    );
    assert_eq!(
        w.put_element(qty, 0, None),
        Err(SbeError::BadValue),
        "null into required"
    );
    assert_eq!(
        w.put_element(qty, 1, Some(Value::Int(2))),
        Err(SbeError::BadValue),
        "not the constant"
    );
    assert_eq!(
        w.put_element(qty, 1, Some(Value::Int(0))),
        Ok(()),
        "the constant"
    );
    assert_eq!(w.put_element(qty, 2, None), Err(SbeError::NoSuchElement));
    assert_eq!(
        w.put(&NOS_FIELDS[0], Value::Array(b"short")),
        Err(SbeError::BadValue),
        "array length"
    );
    assert_eq!(w.put_element(price, 0, None), Ok(()), "null into optional");
    assert_eq!(w.put_bytes(side, b"12"), Err(SbeError::WrongLength));
    Ok(())
}

#[test]
fn counts_and_lengths_past_their_width_overflow() -> Result<(), SbeError> {
    let mut out = [0u8; 1024];
    let mut w = MessageWriter::new::<Examples>(&mut out, 5)?;
    w.group(&SMALL_GROUPS[0], |g| {
        for _ in 0..255 {
            g.entry(|e| e.put(&SMALL_ENTRY[0], Value::Char(b'x')))?;
        }
        assert_eq!(g.entry(|_| Ok(())), Err(SbeError::LengthOverflow));
        assert_eq!(g.count(), 255);
        Ok(())
    })?;
    assert_eq!(
        w.var_data(&SMALL_DATA[0], &[0u8; 256]),
        Err(SbeError::LengthOverflow)
    );
    w.var_data(&SMALL_DATA[0], &[7u8; 255])?;
    let len = w.finish()?;
    assert_eq!(len, 8 + 8 + 2 + 255 + 1 + 255);
    assert_eq!(
        parsed(&out[..len]).0,
        Ok(Parsed::Complete { consumed: len })
    );
    Ok(())
}

#[test]
fn a_short_buffer_is_buffer_too_small_at_every_length() {
    for n in 0..19 {
        let mut out = vec![0u8; n];
        let r = MessageWriter::new::<Examples>(&mut out, 5).and_then(|w| w.finish());
        assert_eq!(r, Err(SbeError::BufferTooSmall), "{n} bytes");
    }
}

#[test]
fn big_endian_schemas_write_every_number_big_endian() -> Result<(), SbeError> {
    let mut out = [0u8; 32];
    let mut w = MessageWriter::new::<ExamplesBe>(&mut out, 5)?;
    w.put(&SMALL_FIELDS[0], Value::UInt(0x0102_0304_0506_0708))?;
    w.var_data(&SMALL_DATA[0], b"ab")?;
    let len = w.finish()?;
    assert_eq!(
        &out[..len],
        &[
            0, 8, 0, 5, 0, 100, 0, 0, // header, big-endian
            1, 2, 3, 4, 5, 6, 7, 8, // the u64
            1, 0, // dimension: blockLength 1, count 0
            2, b'a', b'b', // varData
        ]
    );
    Ok(())
}
