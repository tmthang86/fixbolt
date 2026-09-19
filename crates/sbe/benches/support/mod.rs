//! Hand-written SBE tables shared by `benches/alloc.rs` and `benches/sbe.rs`.
//!
//! `fixbolt-sbe` has zero runtime dependencies outside `fixbolt-codec`
//! (ADR-0081 decision 1), so these benches cannot pull in `sbe-gen`'s
//! generated `Car` / `NewOrderSingle` tables the way
//! `crates/sbe-gen/tests/encoding.rs` does. `crates/sbe/src/tests.rs` already
//! hand-writes the RC4 §7 `NewOrderSingle` and a nested-group-plus-`varData`
//! message for the reader's own tests; the tables below are the same shape,
//! copied rather than shared — `tests.rs` is `#[cfg(test)]` and a
//! `harness = false` bench binary does not build with `--cfg test`.
//!
//! **Named `support/mod.rs`, not `support.rs`.** Cargo's target
//! autodiscovery only turns a bare `benches/<name>.rs` (or `benches/<name>/main.rs`)
//! into a bench target; a `mod.rs` one level down is invisible to it, the same
//! trick `crates/sbe-gen/tests/support/mod.rs` already uses for tests. Included
//! by `#[path = "support/mod.rs"]`, so `crates/sbe/Cargo.toml` needed no
//! `autobenches = false` and no extra `[[bench]]` entry for this file.

use fixbolt_sbe::{
    ByteOrder, DimensionLayout, Element, FieldLayout, GroupLayout, LengthType, MessageLayout,
    Presence, Primitive, Schema, Value, VarDataLayout,
};

// --- element tables, transcribed from crates/sbe/src/tests.rs ---------------

pub const CHAR8: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::Char,
    length: 8,
    presence: Presence::Required,
}];
pub const CHAR: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::Char,
    length: 1,
    presence: Presence::Required,
}];
pub const UINT8: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::UInt8,
    length: 1,
    presence: Presence::Required,
}];
pub const UINT16: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::UInt16,
    length: 1,
    presence: Presence::Required,
}];
pub const UINT64: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::UInt64,
    length: 1,
    presence: Presence::Required,
}];
/// `qtyEncoding`: `int32` mantissa, constant exponent 0.
pub const QTY: &[Element] = &[
    Element {
        name: "mantissa",
        offset: 0,
        primitive: Primitive::Int32,
        length: 1,
        presence: Presence::Required,
    },
    Element {
        name: "exponent",
        offset: 0,
        primitive: Primitive::Int8,
        length: 1,
        presence: Presence::Constant(Value::Int(0)),
    },
];
/// `optionalDecimalEncoding`: optional `int64` mantissa (null `i64::MIN`),
/// constant exponent -3.
pub const OPT_DECIMAL: &[Element] = &[
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

pub const fn field(
    id: u16,
    name: &'static str,
    offset: u16,
    len: u16,
    e: &'static [Element],
) -> FieldLayout {
    FieldLayout {
        id,
        name,
        offset,
        len,
        since_version: 0,
        elements: e,
    }
}

// --- NewOrderSingle: RC4 §7, flat, exactly `tests.rs`'s NEW_ORDER_SINGLE ----

pub static NOS_FIELDS: [FieldLayout; 9] = [
    field(11, "ClOrdID", 0, 8, CHAR8),
    field(1, "Account", 8, 8, CHAR8),
    field(55, "Symbol", 16, 8, CHAR8),
    field(54, "Side", 24, 1, CHAR),
    field(60, "TransactTime", 25, 8, UINT64),
    field(38, "OrderQty", 33, 4, QTY),
    field(40, "OrdType", 37, 1, CHAR),
    field(44, "Price", 38, 8, OPT_DECIMAL),
    field(99, "StopPx", 46, 8, OPT_DECIMAL),
];
pub static NEW_ORDER_SINGLE: MessageLayout = MessageLayout {
    template_id: 99,
    name: "NewOrderSingle",
    block_length: 54,
    since_version: 0,
    fields: &NOS_FIELDS,
    groups: &[],
    var_data: &[],
};

// --- Nested: one root field, an outer group nesting an inner group, a ------
// --- `varData` per outer entry. Stands in for Real Logic's `Car` (its real --
// --- schema lives behind `sbe-gen`, unreachable here) — shape only, named --
// --- plainly rather than "Car" because it is not that schema. -------------

pub static ROOT_FIELDS: [FieldLayout; 1] = [field(10, "kind", 0, 1, UINT8)];
pub static INNER_FIELDS: [FieldLayout; 1] = [field(3, "inner", 0, 2, UINT16)];
pub static INNER: [GroupLayout; 1] = [GroupLayout {
    id: 2,
    name: "innerGroup",
    block_length: 2,
    since_version: 0,
    dimension: DimensionLayout {
        block_length: LengthType::U8,
        num_in_group: LengthType::U8,
    },
    fields: &INNER_FIELDS,
    groups: &[],
    var_data: &[],
}];
pub static OUTER_FIELDS: [FieldLayout; 1] = [field(4, "outer", 0, 2, UINT16)];
pub static OUTER_DATA: [VarDataLayout; 1] = [VarDataLayout {
    id: 5,
    name: "note",
    length: LengthType::U32,
    since_version: 0,
}];
pub static OUTER: [GroupLayout; 1] = [GroupLayout {
    id: 1,
    name: "outerGroup",
    block_length: 2,
    since_version: 0,
    dimension: DimensionLayout::GROUP_SIZE_ENCODING,
    fields: &OUTER_FIELDS,
    groups: &INNER,
    var_data: &OUTER_DATA,
}];
pub static NESTED: MessageLayout = MessageLayout {
    template_id: 1,
    name: "Nested",
    block_length: 1,
    since_version: 0,
    fields: &ROOT_FIELDS,
    groups: &OUTER,
    var_data: &[],
};

/// One schema serving both messages above, id and byte order matching RC4 §7
/// (`tests.rs`'s `Examples`) so `NewOrderSingle`'s bytes mean what the spec
/// says; `Nested` (template 1) is this bench's own addition to the same
/// schema id, which is harmless — nothing outside this module reads it.
pub struct BenchSchema;
impl Schema for BenchSchema {
    const ID: u16 = 100;
    const VERSION: u16 = 0;
    const BYTE_ORDER: ByteOrder = ByteOrder::Little;
    fn message(template_id: u16) -> Option<&'static MessageLayout> {
        match template_id {
            99 => Some(&NEW_ORDER_SINGLE),
            1 => Some(&NESTED),
            _ => None,
        }
    }
}
