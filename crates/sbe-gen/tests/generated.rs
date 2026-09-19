//! Proves `fixbolt-sbe-gen`'s generator against two schemas it never wrote:
//! the SBE 1.0 RC4 spec's own `07Examples.md` schema (`Examples.xml`) and
//! Real Logic's reference `Car` schema (`example-schema.xml`, pulled in
//! through its `xi:include` of `common-types.xml`).
//!
//! `build.rs` runs the generator ahead of time (it must: `build.rs` cannot
//! `use` this crate to call `generate` itself) and writes
//! `$OUT_DIR/examples_rc4.rs` and `$OUT_DIR/car.rs`. Each is `include!`d
//! into its own module below, so the two generated `Schema` impls (and their
//! identical `use fixbolt_sbe::{…}` preambles) never collide.
//!
//! If `vendor/sbe-spec`/`vendor/sbe-ref` are missing, `build.rs` writes a
//! `compile_error!` naming the gap into these files instead of tables, so
//! this whole test binary fails to compile rather than silently reporting
//! green — see the reversal in the plan step's Done list.
//!
//! Integration tests are their own crate root, same as
//! `crates/dict/tests/*.rs`, so the workspace's `unwrap_used`/`expect_used`/
//! `panic`/`indexing_slicing` denies are relaxed here the same way those
//! files relax them: a panic in a test is the test failing, which is what a
//! test is for (CLAUDE.md §2 non-negotiable 7's own reasoning).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fixbolt_sbe::{
    ByteOrder, DimensionLayout, Element, FieldLayout, GroupLayout, LengthType, MessageLayout,
    Presence, Primitive, Schema, Value, VarDataLayout,
};

mod rc4_generated {
    include!(concat!(env!("OUT_DIR"), "/examples_rc4.rs"));
}

mod car_generated {
    include!(concat!(env!("OUT_DIR"), "/car.rs"));
}

// ============================================================================
// RC4 §7 (`Examples.xml`): reproduced by hand from the same source
// `crates/sbe/src/tests.rs` reads, so this test does not trust that file —
// it trusts the same spec text, transcribed a second time independently.
// Equivalence is checked on the `MessageLayout` content only: the
// generator's `Schema::ID` reads the XML's own `id="91"` faithfully, while
// `crates/sbe/src/tests.rs` hand-picks `100` to match the wire dumps'
// `schemaId` byte, per `docs/reference/the-sbe-rc4-example-dumps-disagree-
// with-their-own-tables.md`. That disagreement is about which `u16` best
// describes reality, not about the tables under test here.
// ============================================================================

const CHAR8: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::Char,
    length: 8,
    presence: Presence::Required,
}];
const CHAR: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::Char,
    length: 1,
    presence: Presence::Required,
}];
const UINT64: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::UInt64,
    length: 1,
    presence: Presence::Required,
}];
const UINT16: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::UInt16,
    length: 1,
    presence: Presence::Required,
}];
const UINT8: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::UInt8,
    length: 1,
    presence: Presence::Required,
}];
const QTY: &[Element] = &[
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
const MONTH_YEAR: &[Element] = &[
    Element {
        name: "year",
        offset: 0,
        primitive: Primitive::UInt16,
        length: 1,
        presence: Presence::Required,
    },
    Element {
        name: "month",
        offset: 2,
        primitive: Primitive::UInt8,
        length: 1,
        presence: Presence::Required,
    },
    Element {
        name: "day",
        offset: 3,
        primitive: Primitive::UInt8,
        length: 1,
        presence: Presence::Required,
    },
    Element {
        name: "week",
        offset: 4,
        primitive: Primitive::UInt8,
        length: 1,
        presence: Presence::Required,
    },
];

const fn field(
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

static NOS_FIELDS: [FieldLayout; 9] = [
    field(11, "ClOrdId", 0, 8, CHAR8),
    field(1, "Account", 8, 8, CHAR8),
    field(55, "Symbol", 16, 8, CHAR8),
    field(54, "Side", 24, 1, CHAR),
    field(60, "TransactTime", 25, 8, UINT64),
    field(38, "OrderQty", 33, 4, QTY),
    field(40, "OrdType", 37, 1, CHAR),
    field(44, "Price", 38, 8, OPT_DECIMAL),
    field(99, "StopPx", 46, 8, OPT_DECIMAL),
];
static EXPECTED_NEW_ORDER_SINGLE: MessageLayout = MessageLayout {
    template_id: 99,
    name: "NewOrderSingle",
    block_length: 54,
    since_version: 0,
    fields: &NOS_FIELDS,
    groups: &[],
    var_data: &[],
};

static FILLS_FIELDS: [FieldLayout; 2] = [
    field(1364, "FillPx", 0, 8, OPT_DECIMAL),
    field(1365, "FillQty", 8, 4, QTY),
];
static ER_GROUPS: [GroupLayout; 1] = [GroupLayout {
    id: 2112,
    name: "FillsGrp",
    block_length: 12,
    since_version: 0,
    dimension: DimensionLayout::GROUP_SIZE_ENCODING,
    fields: &FILLS_FIELDS,
    groups: &[],
    var_data: &[],
}];
static ER_FIELDS: [FieldLayout; 10] = [
    field(37, "OrderID", 0, 8, CHAR8),
    field(17, "ExecID", 8, 8, CHAR8),
    field(150, "ExecType", 16, 1, CHAR),
    field(39, "OrdStatus", 17, 1, CHAR),
    field(55, "Symbol", 18, 8, CHAR8),
    field(200, "MaturityMonthYear", 26, 5, MONTH_YEAR),
    field(54, "Side", 31, 1, CHAR),
    field(151, "LeavesQty", 32, 4, QTY),
    field(14, "CumQty", 36, 4, QTY),
    field(75, "TradeDate", 40, 2, UINT16),
];
static EXPECTED_EXECUTION_REPORT: MessageLayout = MessageLayout {
    template_id: 98,
    name: "ExecutionReport",
    block_length: 42,
    since_version: 0,
    fields: &ER_FIELDS,
    groups: &ER_GROUPS,
    var_data: &[],
};

static BMR_FIELDS: [FieldLayout; 2] = [
    field(379, "BusinesRejectRefId", 0, 8, CHAR8),
    field(380, "BusinessRejectReason", 8, 1, UINT8),
];
static BMR_DATA: [VarDataLayout; 1] = [VarDataLayout {
    id: 58,
    name: "Text",
    length: LengthType::U16,
    since_version: 0,
}];
static EXPECTED_BUSINESS_MESSAGE_REJECT: MessageLayout = MessageLayout {
    template_id: 97,
    name: "BusinessMessageReject",
    block_length: 9,
    since_version: 0,
    fields: &BMR_FIELDS,
    groups: &[],
    var_data: &BMR_DATA,
};

#[test]
fn generated_examples_schema_matches_the_hand_written_rc4_tables() {
    use rc4_generated::Examples;

    assert_eq!(Examples::BYTE_ORDER, ByteOrder::Little);

    let nos = Examples::message(99).expect("template 99 (NewOrderSingle)");
    assert_eq!(nos, &EXPECTED_NEW_ORDER_SINGLE, "NewOrderSingle");

    let er = Examples::message(98).expect("template 98 (ExecutionReport)");
    assert_eq!(er, &EXPECTED_EXECUTION_REPORT, "ExecutionReport");

    let bmr = Examples::message(97).expect("template 97 (BusinessMessageReject)");
    assert_eq!(
        bmr, &EXPECTED_BUSINESS_MESSAGE_REJECT,
        "BusinessMessageReject"
    );

    assert!(Examples::message(1).is_none(), "no such template in RC4 §7");
}

// ============================================================================
// Real Logic's `Car` schema (`example-schema.xml` + `common-types.xml` via
// `xi:include`). Every number below is computed BY HAND from the XML, not
// copied from the generator's own output — that is what makes this a check
// rather than a tautology.
//
// Root fields (`<sbe:message name="Car" id="1">`, no `blockLength`
// attribute — computed as the sum below):
//
//   serialNumber   id1  type=uint64                       size 8   offset  0 -> next  8
//   modelYear      id2  type=ModelYear (uint16)            size 2   offset  8 -> next 10
//   available      id3  type=BooleanType (enum, uint8)     size 1   offset 10 -> next 11
//   code           id4  type=Model (enum, char)            size 1   offset 11 -> next 12
//   someNumbers    id5  type=someNumbers (uint32 length=4) size 16  offset 12 -> next 28
//   vehicleCode    id6  type=VehicleCode (char length=6)   size 6   offset 28 -> next 34
//   extras         id7  type=OptionalExtras (set, uint8)   size 1   offset 34 -> next 35
//   discountedModel id8 presence=constant                  size 0   offset 35 -> next 35
//   engine         id9  type=Engine (composite)            size 10  offset 35 -> next 45
//
// so `Car.block_length == 45`. `engine`'s own 10 bytes come from flattening
// `Engine` left to right (capacity=2, numCylinders=1, maxRpm=constant/0,
// manufacturerCode=3, fuel=constant/0, ref efficiency(Percentage:int8)=1,
// ref boosterEnabled(BooleanType:uint8)=1, ref booster(Booster composite:
// BoostType char(1) + horsePower uint8(1) = 2)): 2+1+0+3+0+1+1+2 = 10.
//
// Groups, in schema order:
//   fuelFigures id10: fields speed(uint16,2)+mpg(float,4) = block_length 6;
//     one varData usageDescription(id200, varAsciiEncoding -> length is
//     uint32 -> LengthType::U32).
//   performanceFigures id13: field octaneRating(Ron:uint8,1) = block_length
//     1; one nested group acceleration id15: fields mph(uint16,2)+
//     seconds(float,4) = block_length 6.
//
// Root varData, in schema order: manufacturer(id18), model(id19),
// activationCode(id20) — all varStringEncoding/varAsciiEncoding, both of
// which carry a uint32 `length` member, so all three are LengthType::U32.
// That is 3 varData and 2 root groups (one holding a nested one) — at least
// the "8 field offsets, 2 groups incl. nested, 3 varData" the plan asks for.
// ============================================================================

#[test]
fn generated_car_schema_matches_hand_computed_offsets_and_shape() {
    use car_generated::Baseline;

    assert_eq!(Baseline::BYTE_ORDER, ByteOrder::Little);
    assert_eq!(Baseline::ID, 1);

    let car = Baseline::message(1).expect("template 1 (Car)");
    assert_eq!(car.name, "Car");
    assert_eq!(car.block_length, 45, "sum of the nine root fields above");

    let want_fields: &[(u16, &str, u16, u16)] = &[
        (1, "serialNumber", 0, 8),
        (2, "modelYear", 8, 2),
        (3, "available", 10, 1),
        (4, "code", 11, 1),
        (5, "someNumbers", 12, 16),
        (6, "vehicleCode", 28, 6),
        (7, "extras", 34, 1),
        (8, "discountedModel", 35, 0),
        (9, "engine", 35, 10),
    ];
    assert_eq!(car.fields.len(), want_fields.len(), "9 root fields");
    for (field, &(id, name, offset, len)) in car.fields.iter().zip(want_fields) {
        assert_eq!(field.id, id, "field {name}");
        assert_eq!(field.name, name);
        assert_eq!(field.offset, offset, "field {name} offset");
        assert_eq!(field.len, len, "field {name} len");
    }
    assert_eq!(
        car.fields[7].presence_of_only_element(),
        Some(&Presence::Constant(Value::Char(b'C'))),
        "discountedModel = Model.C, valueRef resolved"
    );

    assert_eq!(car.groups.len(), 2, "fuelFigures, performanceFigures");
    let fuel_figures = &car.groups[0];
    assert_eq!(fuel_figures.id, 10);
    assert_eq!(fuel_figures.name, "fuelFigures");
    assert_eq!(fuel_figures.block_length, 6, "speed(2) + mpg(4)");
    assert_eq!(fuel_figures.dimension, DimensionLayout::GROUP_SIZE_ENCODING);
    assert_eq!(fuel_figures.groups.len(), 0);
    assert_eq!(fuel_figures.var_data.len(), 1);
    assert_eq!(fuel_figures.var_data[0].id, 200);
    assert_eq!(fuel_figures.var_data[0].length, LengthType::U32);

    let performance_figures = &car.groups[1];
    assert_eq!(performance_figures.id, 13);
    assert_eq!(performance_figures.name, "performanceFigures");
    assert_eq!(performance_figures.block_length, 1, "octaneRating(1)");
    assert_eq!(performance_figures.groups.len(), 1, "nested: acceleration");
    let acceleration = &performance_figures.groups[0];
    assert_eq!(acceleration.id, 15);
    assert_eq!(acceleration.name, "acceleration");
    assert_eq!(acceleration.block_length, 6, "mph(2) + seconds(4)");
    assert_eq!(acceleration.groups.len(), 0);
    assert_eq!(acceleration.var_data.len(), 0);

    assert_eq!(car.var_data.len(), 3, "manufacturer, model, activationCode");
    let want_var_data: &[(u16, &str)] =
        &[(18, "manufacturer"), (19, "model"), (20, "activationCode")];
    for (vd, &(id, name)) in car.var_data.iter().zip(want_var_data) {
        assert_eq!(vd.id, id, "varData {name}");
        assert_eq!(vd.name, name);
        assert_eq!(
            vd.length,
            LengthType::U32,
            "varData {name}: uint32 length prefix"
        );
    }

    assert!(
        Baseline::message(2).is_none(),
        "no such template in this schema"
    );
}

/// Small helper for the one presence assertion above: the constant's value,
/// for a field made of exactly one `Element` (`discountedModel` is not a
/// composite, so it has exactly one).
trait OnlyElementPresence {
    fn presence_of_only_element(&self) -> Option<&Presence>;
}
impl OnlyElementPresence for FieldLayout {
    fn presence_of_only_element(&self) -> Option<&Presence> {
        match self.elements {
            [only] => Some(&only.presence),
            _ => None,
        }
    }
}
