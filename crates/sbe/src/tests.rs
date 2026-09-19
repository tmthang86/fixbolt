//! The reader against the spec's own examples, with tables written by hand.
//!
//! Source: SBE 1.0 RC4 §7 (`vendor/sbe-spec/v1-0-RC4/doc/07Examples.md`,
//! fetched by `scripts/fetch-sbe-assets.sh`, never committed). The three hex
//! dumps are transcribed below; the schema is transcribed as tables. These are
//! the **hand-written** tables of plan step C1 — the tables the generator emits
//! for the same schema are checked against the same bytes in step C3.
//!
//! **The dumps win over the interpretation tables.** RC4's tables disagree with
//! their own dumps in four places, and every assertion here is on what the
//! bytes say (the 1.0 STANDARD text's dumps differ again; see the notes on each
//! test):
//!
//! - order `TransactTime`: dump `00 84 68 90 fe a8 9a 13` = 1412627244432000000
//!   (2014-10-06 20:27:24.432 UTC); the table says `c021ed1b04c32b13`,
//!   2013-10-10 13:35:33.135.
//! - order `StopPx`: the table writes `0000000000000008`; the dump carries
//!   `00 .. 00 80`, which is `i64::MIN` little-endian, the int64 null.
//! - execution `TradeDate`: dump `dd 3f` = 16349 days (2014-10-06); the table
//!   says `753e`, 2013-10-11.
//! - reject header: the dump says template 0x61 = 97 (as the schema does) and
//!   schema 0x64 = 100; the table says template 100 and schema 0.

use crate::schema::{
    DimensionLayout, Element, FieldLayout, GroupLayout, LengthType, MessageLayout, Presence,
    Primitive, Schema, Value, VarDataLayout,
};
use crate::{ByteOrder, Cursor, HEADER_LEN, MessageHeader, SbeError, SbeView};

// --- the §7 schema, by hand --------------------------------------------------

/// `idString`: `char`, `length="8"`.
const CHAR8: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::Char,
    length: 8,
    presence: Presence::Required,
}];

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
/// `qtyEncoding`: `int32` mantissa, constant exponent 0.
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
/// `optionalDecimalEncoding`: optional `int64` mantissa (null `i64::MIN`),
/// constant exponent -3.
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
/// `MONTH_YEAR`, as the example schema declares it: four required members.
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

static NOS_FIELDS: [FieldLayout; 9] = [
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
static NEW_ORDER_SINGLE: MessageLayout = MessageLayout {
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
static EXECUTION_REPORT: MessageLayout = MessageLayout {
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
static BUSINESS_MESSAGE_REJECT: MessageLayout = MessageLayout {
    template_id: 97,
    name: "BusinessMessageReject",
    block_length: 9,
    since_version: 0,
    fields: &BMR_FIELDS,
    groups: &[],
    var_data: &BMR_DATA,
};

/// The RC4 §7 schema: `id="100"`, little-endian, version 0.
struct Examples;
impl Schema for Examples {
    const ID: u16 = 100;
    const VERSION: u16 = 0;
    const BYTE_ORDER: ByteOrder = ByteOrder::Little;
    fn message(template_id: u16) -> Option<&'static MessageLayout> {
        match template_id {
            99 => Some(&NEW_ORDER_SINGLE),
            98 => Some(&EXECUTION_REPORT),
            97 => Some(&BUSINESS_MESSAGE_REJECT),
            _ => None,
        }
    }
}

// --- the §7 dumps, transcribed ----------------------------------------------

/// Simple Open Framing Header in front of every §7 dump: 4-byte big-endian
/// message size, 2-byte encoding type `eb50`. Not part of the SBE message.
const SOFH_LEN: usize = 6;

/// "Wire format of an order message", 68 bytes.
const ORDER_DUMP: [u8; 68] = [
    0x00, 0x00, 0x00, 0x44, 0xeb, 0x50, 0x36, 0x00, 0x63, 0x00, 0x64, 0x00, 0x00, 0x00, 0x4f,
    0x52, //
    0x44, 0x30, 0x30, 0x30, 0x30, 0x31, 0x41, 0x43, 0x43, 0x54, 0x30, 0x31, 0x00, 0x00, 0x47,
    0x45, //
    0x4d, 0x34, 0x00, 0x00, 0x00, 0x00, 0x31, 0x00, 0x84, 0x68, 0x90, 0xfe, 0xa8, 0x9a, 0x13,
    0x07, //
    0x00, 0x00, 0x00, 0x32, 0x1a, 0x85, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, //
    0x00, 0x00, 0x00, 0x80,
];

/// "Wire format of an execution message", 84 bytes.
const EXEC_DUMP: [u8; 84] = [
    0x00, 0x00, 0x00, 0x54, 0xeb, 0x50, 0x2a, 0x00, 0x62, 0x00, 0x64, 0x00, 0x00, 0x00, 0x4f,
    0x30, //
    0x30, 0x30, 0x30, 0x30, 0x30, 0x31, 0x45, 0x58, 0x45, 0x43, 0x30, 0x30, 0x30, 0x30, 0x46,
    0x31, //
    0x47, 0x45, 0x4d, 0x34, 0x00, 0x00, 0x00, 0x00, 0xde, 0x07, 0x06, 0xff, 0xff, 0x31, 0x01,
    0x00, //
    0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xdd, 0x3f, 0x0c, 0x00, 0x02, 0x00, 0x1a, 0x85, 0x01,
    0x00, //
    0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x24, 0x85, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, //
    0x04, 0x00, 0x00, 0x00,
];

/// "Wire format of a business reject message", 64 bytes.
const REJECT_DUMP: [u8; 64] = [
    0x00, 0x00, 0x00, 0x40, 0xeb, 0x50, 0x09, 0x00, 0x61, 0x00, 0x64, 0x00, 0x00, 0x00, 0x4f,
    0x52, //
    0x44, 0x30, 0x30, 0x30, 0x30, 0x31, 0x06, 0x27, 0x00, 0x4e, 0x6f, 0x74, 0x20, 0x61, 0x75,
    0x74, //
    0x68, 0x6f, 0x72, 0x69, 0x7a, 0x65, 0x64, 0x20, 0x74, 0x6f, 0x20, 0x74, 0x72, 0x61, 0x64,
    0x65, //
    0x20, 0x74, 0x68, 0x61, 0x74, 0x20, 0x69, 0x6e, 0x73, 0x74, 0x72, 0x75, 0x6d, 0x65, 0x6e, 0x74,
];

/// The SBE message inside a dump, after checking the SOFH size field is the
/// dump's length — the transcription is checked, not trusted.
fn sbe(dump: &[u8]) -> &[u8] {
    let size = dump.get(..4).map(|b| [b[0], b[1], b[2], b[3]]);
    assert_eq!(size.map(u32::from_be_bytes), u32::try_from(dump.len()).ok());
    assert_eq!(
        dump.get(4..6),
        Some(&[0xeb, 0x50][..]),
        "SOFH: SBE 1.0 little-endian"
    );
    dump.get(SOFH_LEN..).unwrap_or_default()
}

fn f(layout: &'static MessageLayout, id: u16) -> &'static FieldLayout {
    layout.field(id).unwrap_or(&BOGUS)
}
/// Stands in for a lookup that failed, so a typo in a test is a failed
/// assertion on a value rather than a panic in a helper.
static BOGUS: FieldLayout = field(0, "bogus", u16::MAX, 1, CHAR);

// --- NewOrderSingle: flat, fixed length --------------------------------------

#[test]
fn new_order_single_decodes_field_by_field_as_the_spec_dump_says() -> Result<(), SbeError> {
    let msg = sbe(&ORDER_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    assert_eq!((v.template_id(), v.schema_id(), v.version()), (99, 100, 0));
    assert_eq!(v.block_length(ByteOrder::Little), Ok(54));
    let layout = v.layout::<Examples>()?;
    assert_eq!(layout.name, "NewOrderSingle");
    let root = v.root::<Examples>()?;
    assert_eq!(root.bytes().len(), 54);

    let nos = &NEW_ORDER_SINGLE;
    assert_eq!(root.value(f(nos, 11))?, Some(Value::Array(b"ORD00001")));
    assert_eq!(root.value(f(nos, 1))?, Some(Value::Array(b"ACCT01\0\0")));
    assert_eq!(root.value(f(nos, 55))?, Some(Value::Array(b"GEM4\0\0\0\0")));
    assert_eq!(
        root.value(f(nos, 54))?,
        Some(Value::Char(b'1')),
        "Side = Buy"
    );
    // The dump, not RC4's table (module doc): 2014-10-06 20:27:24.432 UTC.
    assert_eq!(
        root.value(f(nos, 60))?,
        Some(Value::UInt(1_412_627_244_432_000_000))
    );
    let qty = root.field(f(nos, 38))?;
    assert_eq!(
        qty.map(|q| q.member("mantissa")),
        Some(Ok(Some(Value::Int(7))))
    );
    assert_eq!(
        qty.map(|q| q.member("exponent")),
        Some(Ok(Some(Value::Int(0))))
    );
    assert_eq!(
        root.value(f(nos, 40))?,
        Some(Value::Char(b'2')),
        "OrdType = Limit"
    );
    let px = root.field(f(nos, 44))?;
    assert_eq!(
        px.map(|p| p.element(0)),
        Some(Ok(Some(Value::Int(99_610)))),
        "99.610"
    );
    assert_eq!(px.map(|p| p.element(1)), Some(Ok(Some(Value::Int(-3)))));
    // StopPx: `i64::MIN` on the wire, the optional int64's null.
    assert_eq!(root.value(f(nos, 99))?, None, "StopPx = null");
    let stop = root.field(f(nos, 99))?;
    assert_eq!(
        stop.map(|s| s.bytes()),
        Some(&[0, 0, 0, 0, 0, 0, 0, 0x80][..])
    );

    // Nothing follows a flat message: its end is the root block's end.
    assert_eq!(v.root_end(ByteOrder::Little), Ok(msg.len()));
    assert_eq!(v.tail::<Examples>()?.position(), msg.len());
    Ok(())
}

#[test]
fn every_truncation_of_the_order_is_an_error_at_decode() {
    let msg = sbe(&ORDER_DUMP);
    for cut in 0..msg.len() {
        let short = msg.get(..cut).unwrap_or_default();
        assert_eq!(
            SbeView::decode::<Examples>(short),
            Err(SbeError::Truncated),
            "cut at {cut}"
        );
    }
    assert!(SbeView::decode::<Examples>(msg).is_ok());
}

#[test]
fn the_header_encodes_back_to_the_dump_bytes() -> Result<(), SbeError> {
    let msg = sbe(&ORDER_DUMP);
    let h = MessageHeader::decode(msg, ByteOrder::Little)?;
    let mut out = [0u8; HEADER_LEN];
    h.encode(&mut out, ByteOrder::Little)?;
    assert_eq!(Some(&out[..]), msg.get(..HEADER_LEN));
    Ok(())
}

// --- ExecutionReport: a repeating group ------------------------------------

/// Everything the execution report carries, walked end to end.
/// (fills as (FillPx mantissa, FillQty), `numInGroup`, cursor position after the group).
type Walked = ([(i64, i64); 2], usize, usize);

fn walk_execution(msg: &[u8]) -> Result<Walked, SbeError> {
    let v = SbeView::decode::<Examples>(msg)?;
    let layout = v.layout::<Examples>()?;
    let mut fills = [(0, 0); 2];
    let mut g = v.tail::<Examples>()?.group(&ER_GROUPS[0])?;
    let count = g.count();
    let mut i = 0;
    while let Some(e) = g.next_entry()? {
        let px = e.block().value(f_group(0, 1364))?;
        let qty = e.block().value(f_group(0, 1365))?;
        if let (Some(Value::Int(px)), Some(Value::Int(qty)), Some(slot)) =
            (px, qty, fills.get_mut(i))
        {
            *slot = (px, qty);
        }
        i += 1;
    }
    let end = g.finish()?.position();
    assert_eq!(layout.name, "ExecutionReport");
    Ok((fills, count, end))
}

fn f_group(group: usize, id: u16) -> &'static FieldLayout {
    ER_GROUPS
        .get(group)
        .and_then(|g| g.field(id))
        .unwrap_or(&BOGUS)
}

#[test]
fn execution_report_decodes_root_and_group_as_the_spec_dump_says() -> Result<(), SbeError> {
    let msg = sbe(&EXEC_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    assert_eq!((v.template_id(), v.schema_id(), v.version()), (98, 100, 0));
    let root = v.root::<Examples>()?;
    let er = &EXECUTION_REPORT;
    assert_eq!(root.value(f(er, 37))?, Some(Value::Array(b"O0000001")));
    assert_eq!(root.value(f(er, 17))?, Some(Value::Array(b"EXEC0000")));
    assert_eq!(
        root.value(f(er, 150))?,
        Some(Value::Char(b'F')),
        "ExecType = Trade"
    );
    assert_eq!(
        root.value(f(er, 39))?,
        Some(Value::Char(b'1')),
        "OrdStatus = PartialFilled"
    );
    assert_eq!(root.value(f(er, 55))?, Some(Value::Array(b"GEM4\0\0\0\0")));
    let mmy = root.field(f(er, 200))?;
    assert_eq!(
        mmy.map(|m| m.member("year")),
        Some(Ok(Some(Value::UInt(2014))))
    );
    assert_eq!(
        mmy.map(|m| m.member("month")),
        Some(Ok(Some(Value::UInt(6))))
    );
    // Declared required in the example schema, so 0xff is a value, not null.
    assert_eq!(
        mmy.map(|m| m.member("day")),
        Some(Ok(Some(Value::UInt(255))))
    );
    assert_eq!(
        mmy.map(|m| m.member("week")),
        Some(Ok(Some(Value::UInt(255))))
    );
    assert_eq!(root.value(f(er, 54))?, Some(Value::Char(b'1')));
    assert_eq!(root.value(f(er, 151))?, Some(Value::Int(1)));
    assert_eq!(root.value(f(er, 14))?, Some(Value::Int(6)));
    // The dump, not RC4's table (module doc).
    assert_eq!(root.value(f(er, 75))?, Some(Value::UInt(16_349)));

    let (fills, count, end) = walk_execution(msg)?;
    assert_eq!(count, 2);
    assert_eq!(fills, [(99_610, 2), (99_620, 4)]);
    assert_eq!(end, msg.len(), "the group ends where the message ends");
    Ok(())
}

#[test]
fn skipping_the_group_lands_where_walking_it_does() -> Result<(), SbeError> {
    let msg = sbe(&EXEC_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    let c = v.tail::<Examples>()?.skip_group(&ER_GROUPS[0])?;
    assert_eq!(c.position(), msg.len());
    Ok(())
}

#[test]
fn every_truncation_of_the_execution_report_is_an_error_somewhere() {
    let msg = sbe(&EXEC_DUMP);
    for cut in 0..msg.len() {
        let short = msg.get(..cut).unwrap_or_default();
        assert_eq!(
            walk_execution(short),
            Err(SbeError::Truncated),
            "cut at {cut}"
        );
    }
    assert!(walk_execution(msg).is_ok());
}

// --- BusinessMessageReject: varData ----------------------------------------

#[test]
fn business_reject_decodes_root_and_var_data_as_the_spec_dump_says() -> Result<(), SbeError> {
    let msg = sbe(&REJECT_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    assert_eq!((v.template_id(), v.schema_id(), v.version()), (97, 100, 0));
    let root = v.root::<Examples>()?;
    let bmr = &BUSINESS_MESSAGE_REJECT;
    assert_eq!(root.value(f(bmr, 379))?, Some(Value::Array(b"ORD00001")));
    assert_eq!(
        root.value(f(bmr, 380))?,
        Some(Value::UInt(6)),
        "NotAuthorized"
    );
    let (text, after) = v.tail::<Examples>()?.var_data(&BMR_DATA[0])?;
    assert_eq!(text, Some(&b"Not authorized to trade that instrument"[..]));
    assert_eq!(after.position(), msg.len());
    for cut in 0..msg.len() {
        let short = msg.get(..cut).unwrap_or_default();
        let r = SbeView::decode::<Examples>(short)
            .and_then(|v| v.tail::<Examples>())
            .and_then(|c| c.var_data(&BMR_DATA[0]));
        assert_eq!(r, Err(SbeError::Truncated), "cut at {cut}");
    }
    Ok(())
}

// --- identity checks ---------------------------------------------------------

#[test]
fn an_unknown_template_is_delimited_by_block_length_and_not_walked() -> Result<(), SbeError> {
    // The order's bytes under template 42, which the schema does not have.
    let mut msg = [0u8; 62];
    msg.copy_from_slice(sbe(&ORDER_DUMP));
    msg[2] = 42;
    let v = SbeView::decode::<Examples>(&msg)?;
    assert_eq!(v.layout::<Examples>(), Err(SbeError::UnknownTemplate));
    assert_eq!(v.root_end(ByteOrder::Little), Ok(HEADER_LEN + 54));
    Ok(())
}

#[test]
fn another_schemas_message_is_refused_by_layout() -> Result<(), SbeError> {
    let mut msg = [0u8; 62];
    msg.copy_from_slice(sbe(&ORDER_DUMP));
    msg[4] = 101;
    let v = SbeView::decode::<Examples>(&msg)?;
    assert_eq!(v.layout::<Examples>(), Err(SbeError::WrongSchema));
    Ok(())
}

// --- versioning (ADR-0081 decision 4) ----------------------------------------

/// Version 1 of a small schema: a root field, a group and a `varData` added in
/// version 1, after a version-0 field, group and `varData`.
mod versioned {
    use super::*;

    const U32: &[Element] = &[Element {
        name: "",
        offset: 0,
        primitive: Primitive::UInt32,
        length: 1,
        presence: Presence::Required,
    }];
    pub(super) static FIELDS: [FieldLayout; 2] = [
        field(1, "old", 0, 4, U32),
        FieldLayout {
            since_version: 1,
            ..field(2, "new", 4, 4, U32)
        },
    ];
    static ENTRY: [FieldLayout; 1] = [field(10, "x", 0, 4, U32)];
    pub(super) static GROUPS: [GroupLayout; 2] = [
        GroupLayout {
            id: 20,
            name: "oldGroup",
            block_length: 4,
            since_version: 0,
            dimension: DimensionLayout::GROUP_SIZE_ENCODING,
            fields: &ENTRY,
            groups: &[],
            var_data: &[],
        },
        GroupLayout {
            id: 21,
            name: "newGroup",
            block_length: 4,
            since_version: 1,
            dimension: DimensionLayout::GROUP_SIZE_ENCODING,
            fields: &ENTRY,
            groups: &[],
            var_data: &[],
        },
    ];
    pub(super) static DATA: [VarDataLayout; 2] = [
        VarDataLayout {
            id: 30,
            name: "oldData",
            length: LengthType::U8,
            since_version: 0,
        },
        VarDataLayout {
            id: 31,
            name: "newData",
            length: LengthType::U8,
            since_version: 1,
        },
    ];
    pub(super) static MSG: MessageLayout = MessageLayout {
        template_id: 1,
        name: "M",
        block_length: 8,
        since_version: 0,
        fields: &FIELDS,
        groups: &GROUPS,
        var_data: &DATA,
    };
    pub(super) struct V1;
    impl Schema for V1 {
        const ID: u16 = 5;
        const VERSION: u16 = 1;
        const BYTE_ORDER: ByteOrder = ByteOrder::Little;
        fn message(t: u16) -> Option<&'static MessageLayout> {
            (t == 1).then_some(&MSG)
        }
    }
}

/// A version-0 message: blockLength 4, one entry in `oldGroup`, `oldData`
/// "ab" — and nothing of what version 1 added.
const V0_MSG: [u8; 25] = [
    4, 0, 1, 0, 5, 0, 0, 0, // header: blockLength 4, template 1, schema 5, version 0
    7, 0, 0, 0, // old = 7
    4, 0, 1, 0, // oldGroup: blockLength 4, 1 entry
    9, 0, 0, 0, // x = 9
    2, b'a', b'b', // oldData
    0xEE, 0xEE, // not part of the message
];

#[test]
fn what_a_newer_version_added_reads_as_absent_from_an_older_message() -> Result<(), SbeError> {
    use versioned::*;
    let v = SbeView::decode::<V1>(&V0_MSG)?;
    let root = v.root::<V1>()?;
    assert_eq!(root.value(&FIELDS[0])?, Some(Value::UInt(7)));
    // `new` is at offset 4, past the 4-byte block — but it is its version that
    // says absent, before the block length is consulted.
    assert_eq!(root.field(&FIELDS[1])?, None);

    let mut g = v.tail::<V1>()?.group(&GROUPS[0])?;
    let x = g.next_entry()?.map(|e| e.block().value(&ENTRY_X));
    assert_eq!(x, Some(Ok(Some(Value::UInt(9)))));
    let c = g.finish()?;
    let at = c.position();
    let absent = c.group(&GROUPS[1])?;
    assert!(!absent.is_present());
    assert_eq!(absent.count(), 0);
    let c = absent.finish()?;
    assert_eq!(c.position(), at, "an absent group consumes nothing");
    let (old, c) = c.var_data(&DATA[0])?;
    assert_eq!(old, Some(&b"ab"[..]));
    let (new, c) = c.var_data(&DATA[1])?;
    assert_eq!(new, None);
    assert_eq!(c.position(), V0_MSG.len() - 2);
    Ok(())
}

static ENTRY_X: FieldLayout = field(
    10,
    "x",
    0,
    4,
    &[Element {
        name: "",
        offset: 0,
        primitive: Primitive::UInt32,
        length: 1,
        presence: Presence::Required,
    }],
);

#[test]
fn a_newer_encoders_longer_blocks_are_walked_by_their_wire_length() -> Result<(), SbeError> {
    // The order under version 1 with 4 more root bytes than this schema knows:
    // fields still read, and the tail starts at 8 + 58, not 8 + 54.
    let mut msg = [0u8; 66];
    msg.get_mut(..62)
        .map(|d| d.copy_from_slice(sbe(&ORDER_DUMP)))
        .unwrap_or_default();
    msg[0] = 58;
    msg[6] = 1;
    let v = SbeView::decode::<Examples>(&msg)?;
    let root = v.root::<Examples>()?;
    assert_eq!(root.bytes().len(), 58);
    assert_eq!(root.value(f(&NEW_ORDER_SINGLE, 38))?, Some(Value::Int(7)));
    assert_eq!(v.tail::<Examples>()?.position(), 66);
    Ok(())
}

#[test]
fn a_block_shorter_than_a_known_field_is_field_outside_block() -> Result<(), SbeError> {
    // blockLength 40 under version 0: Price (38..46) does not fit.
    let mut msg = [0u8; 62];
    msg.copy_from_slice(sbe(&ORDER_DUMP));
    msg[0] = 40;
    let v = SbeView::decode::<Examples>(&msg)?;
    let root = v.root::<Examples>()?;
    assert_eq!(
        root.value(f(&NEW_ORDER_SINGLE, 40))?,
        Some(Value::Char(b'2'))
    );
    assert_eq!(
        root.value(f(&NEW_ORDER_SINGLE, 44)),
        Err(SbeError::FieldOutsideBlock)
    );
    Ok(())
}

// --- nested groups with varData, and big-endian --------------------------------

mod nested {
    use super::*;

    const U16: &[Element] = UINT16;
    pub(super) static INNER_FIELDS: [FieldLayout; 1] = [field(3, "inner", 0, 2, U16)];
    pub(super) static INNER: [GroupLayout; 1] = [GroupLayout {
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
    pub(super) static OUTER_FIELDS: [FieldLayout; 1] = [field(4, "outer", 0, 2, U16)];
    pub(super) static OUTER_DATA: [VarDataLayout; 1] = [VarDataLayout {
        id: 5,
        name: "note",
        length: LengthType::U32,
        since_version: 0,
    }];
    pub(super) static OUTER: [GroupLayout; 1] = [GroupLayout {
        id: 1,
        name: "outerGroup",
        block_length: 2,
        since_version: 0,
        dimension: DimensionLayout::GROUP_SIZE_ENCODING,
        fields: &OUTER_FIELDS,
        groups: &INNER,
        var_data: &OUTER_DATA,
    }];
    pub(super) static MSG: MessageLayout = MessageLayout {
        template_id: 7,
        name: "N",
        block_length: 0,
        since_version: 0,
        fields: &[],
        groups: &OUTER,
        var_data: &[],
    };
    pub(super) struct Be;
    impl Schema for Be {
        const ID: u16 = 9;
        const VERSION: u16 = 0;
        const BYTE_ORDER: ByteOrder = ByteOrder::Big;
        fn message(t: u16) -> Option<&'static MessageLayout> {
            (t == 7).then_some(&MSG)
        }
    }
}

/// Big-endian: two outer entries, each with an inner group (uint8 dimension)
/// and a uint32-prefixed note. The first outer entry has an empty inner group.
const NESTED_BE: [u8; 42] = [
    0, 0, 0, 7, 0, 9, 0, 0, // header: blockLength 0, template 7, schema 9, version 0
    0, 2, 0, 2, // outerGroup: blockLength 2, 2 entries
    0x01, 0x00, // outer = 256
    2, 0, // innerGroup: blockLength 2, 0 entries
    0, 0, 0, 1, b'x', // note "x"
    0x00, 0x02, // outer = 2
    2, 2, // innerGroup: blockLength 2, 2 entries
    0x00, 0x0A, // inner = 10
    0x00, 0x0B, // inner = 11
    0, 0, 0, 3, b'y', b'z', b'!', // note "yz!"
    0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAD, // not part of the message
];

#[test]
fn nested_groups_and_var_data_walk_depth_first_in_big_endian() -> Result<(), SbeError> {
    use nested::*;
    let v = SbeView::decode::<Be>(&NESTED_BE)?;
    assert_eq!((v.template_id(), v.schema_id()), (7, 9));
    let mut outer = v.tail::<Be>()?.group(&OUTER[0])?;
    assert_eq!(outer.count(), 2);

    let mut seen_outer = [0u64; 2];
    let mut seen_inner = [0u64; 2];
    let mut notes: [&[u8]; 2] = [b"", b""];
    let (mut i, mut k) = (0, 0);
    while let Some(e) = outer.next_entry()? {
        if let (Some(Value::UInt(o)), Some(slot)) =
            (e.block().value(&OUTER_FIELDS[0])?, seen_outer.get_mut(i))
        {
            *slot = o;
        }
        let mut inner = e.tail().group(&INNER[0])?;
        while let Some(ie) = inner.next_entry()? {
            if let (Some(Value::UInt(x)), Some(slot)) =
                (ie.block().value(&INNER_FIELDS[0])?, seen_inner.get_mut(k))
            {
                *slot = x;
            }
            k += 1;
        }
        let (note, _) = inner.finish()?.var_data(&OUTER_DATA[0])?;
        if let (Some(n), Some(slot)) = (note, notes.get_mut(i)) {
            *slot = n;
        }
        i += 1;
    }
    let end = outer.finish()?.position();
    assert_eq!(seen_outer, [256, 2]);
    assert_eq!(seen_inner, [10, 11]);
    assert_eq!(notes, [&b"x"[..], &b"yz!"[..]]);
    assert_eq!(end, NESTED_BE.len() - 6);
    Ok(())
}

#[test]
fn entries_stay_aligned_when_the_caller_ignores_their_tails() -> Result<(), SbeError> {
    use nested::*;
    let v = SbeView::decode::<Be>(&NESTED_BE)?;
    let mut outer = v.tail::<Be>()?.group(&OUTER[0])?;
    let mut seen = [0u64; 2];
    let mut i = 0;
    while let Some(e) = outer.next_entry()? {
        if let (Some(Value::UInt(o)), Some(slot)) =
            (e.block().value(&OUTER_FIELDS[0])?, seen.get_mut(i))
        {
            *slot = o;
        }
        i += 1;
    }
    assert_eq!(seen, [256, 2]);
    assert_eq!(outer.finish()?.position(), NESTED_BE.len() - 6);
    // And skipping the whole group unread lands in the same place.
    let skipped: Cursor<'_> = v.tail::<Be>()?.skip_group(&OUTER[0])?;
    assert_eq!(skipped.position(), NESTED_BE.len() - 6);
    Ok(())
}

#[test]
fn every_truncation_of_the_nested_message_is_an_error() {
    use nested::*;
    let msg = NESTED_BE.get(..NESTED_BE.len() - 6).unwrap_or_default();
    for cut in 0..msg.len() {
        let short = msg.get(..cut).unwrap_or_default();
        let r = SbeView::decode::<Be>(short)
            .and_then(|v| v.tail::<Be>())
            .and_then(|c| c.skip_group(&OUTER[0]));
        assert_eq!(
            r.map(|c| c.position()),
            Err(SbeError::Truncated),
            "cut at {cut}"
        );
    }
}

// --- element reads -----------------------------------------------------------

#[test]
fn optional_scalars_arrays_floats_and_constants_read_as_the_table_says() -> Result<(), SbeError> {
    static ELEMENTS: [Element; 5] = [
        Element {
            name: "u8",
            offset: 0,
            primitive: Primitive::UInt8,
            length: 1,
            presence: Presence::Optional {
                null: Value::UInt(255),
            },
        },
        Element {
            name: "code",
            offset: 1,
            primitive: Primitive::Char,
            length: 3,
            presence: Presence::Optional {
                null: Value::Char(0),
            },
        },
        Element {
            name: "f",
            offset: 4,
            primitive: Primitive::Float,
            length: 1,
            presence: Presence::Optional {
                null: Value::Float(f64::NAN),
            },
        },
        Element {
            name: "fuel",
            offset: 0,
            primitive: Primitive::Char,
            length: 6,
            presence: Presence::Constant(Value::Array(b"Petrol")),
        },
        Element {
            name: "i16",
            offset: 8,
            primitive: Primitive::Int16,
            length: 1,
            presence: Presence::Required,
        },
    ];
    static F: FieldLayout = field(1, "c", 0, 10, &ELEMENTS);
    let order = ByteOrder::Little;

    let mut b = [0u8; 10];
    b[0] = 255;
    b[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
    b[8..10].copy_from_slice(&(-2i16).to_le_bytes());
    let blk = crate::view::Block::new(&b, order, 0);
    let fr = blk.field(&F)?;
    assert_eq!(
        fr.map(|x| x.member("u8")),
        Some(Ok(None)),
        "255 is uint8 null"
    );
    assert_eq!(
        fr.map(|x| x.member("code")),
        Some(Ok(None)),
        "all NUL is char-array null"
    );
    assert_eq!(
        fr.map(|x| x.member("f")),
        Some(Ok(None)),
        "NaN is float null"
    );
    assert_eq!(
        fr.map(|x| x.member("fuel")),
        Some(Ok(Some(Value::Array(b"Petrol"))))
    );
    assert_eq!(fr.map(|x| x.member("i16")), Some(Ok(Some(Value::Int(-2)))));
    assert_eq!(fr.map(|x| x.element(5)), Some(Err(SbeError::NoSuchElement)));
    assert_eq!(
        fr.map(|x| x.member("nope")),
        Some(Err(SbeError::NoSuchElement))
    );

    b[0] = 3;
    b[1..4].copy_from_slice(b"AB\0");
    b[4..8].copy_from_slice(&1.5f32.to_le_bytes());
    let blk = crate::view::Block::new(&b, order, 0);
    let fr = blk.field(&F)?;
    assert_eq!(fr.map(|x| x.member("u8")), Some(Ok(Some(Value::UInt(3)))));
    assert_eq!(
        fr.map(|x| x.member("code")),
        Some(Ok(Some(Value::Array(b"AB\0"))))
    );
    assert_eq!(fr.map(|x| x.member("f")), Some(Ok(Some(Value::Float(1.5)))));
    Ok(())
}

#[test]
fn a_field_made_only_of_constants_reads_its_value_without_wire_bytes() -> Result<(), SbeError> {
    // Car's `discountedModel`: `presence="constant" valueRef="Model.C"` — no
    // bytes on the wire, and an offset that may sit at or past the block's end.
    static DISCOUNTED: [Element; 1] = [Element {
        name: "",
        offset: 0,
        primitive: Primitive::Char,
        length: 1,
        presence: Presence::Constant(Value::Char(b'C')),
    }];
    static F: FieldLayout = field(8, "discountedModel", 200, 0, &DISCOUNTED);
    let blk = crate::view::Block::new(&[1, 2, 3], ByteOrder::Little, 0);
    assert_eq!(blk.value(&F)?, Some(Value::Char(b'C')));
    Ok(())
}

// --- the writer: the three dumps written back from their decoded values -------
//
// Step C4. Built without `encoding` too, so the writer is covered by
// `--no-default-features`. The same oracle over the generated tables, with
// nested groups, is `crates/sbe-gen/tests/encoding.rs`.

/// Writes every element of every root field of `v`, as decoded, into `w`.
fn copy_root(v: &SbeView<'_>, w: &mut crate::MessageWriter<'_>) -> Result<(), SbeError> {
    let root = v.root::<Examples>()?;
    for fl in v.layout::<Examples>()?.fields {
        let Some(r) = root.field(fl)? else {
            return Err(SbeError::FieldOutsideBlock);
        };
        for i in 0..fl.elements.len() {
            w.put_element(fl, i, r.element(i)?)?;
        }
    }
    Ok(())
}

/// The first offset where `got` and `want` differ, or their common length.
fn first_difference(got: &[u8], want: &[u8]) -> Option<usize> {
    got.iter()
        .zip(want)
        .position(|(g, w)| g != w)
        .or((got.len() != want.len()).then_some(got.len().min(want.len())))
}

#[test]
fn the_writer_reencodes_the_order_dump_from_its_decoded_values() -> Result<(), SbeError> {
    let msg = sbe(&ORDER_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    let mut out = [0xAAu8; 80];
    let mut w = crate::MessageWriter::new::<Examples>(&mut out, 99)?;
    copy_root(&v, &mut w)?;
    let len = w.finish()?;
    assert_eq!(first_difference(&out[..len], msg), None);
    Ok(())
}

#[test]
fn the_writer_reencodes_the_execution_dump_group_and_all() -> Result<(), SbeError> {
    let msg = sbe(&EXEC_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    let mut out = [0xAAu8; 96];
    let mut w = crate::MessageWriter::new::<Examples>(&mut out, 98)?;
    copy_root(&v, &mut w)?;
    let mut g = v.tail::<Examples>()?.group(&ER_GROUPS[0])?;
    w.group(&ER_GROUPS[0], |gw| {
        while let Some(e) = g.next_entry()? {
            gw.entry(|ew| {
                for fl in &FILLS_FIELDS {
                    let Some(r) = e.block().field(fl)? else {
                        return Err(SbeError::FieldOutsideBlock);
                    };
                    for i in 0..fl.elements.len() {
                        ew.put_element(fl, i, r.element(i)?)?;
                    }
                }
                Ok(())
            })?;
        }
        Ok(())
    })?;
    let len = w.finish()?;
    assert_eq!(first_difference(&out[..len], msg), None);
    Ok(())
}

#[test]
fn the_writer_reencodes_the_business_reject_dump_var_data_and_all() -> Result<(), SbeError> {
    let msg = sbe(&REJECT_DUMP);
    let v = SbeView::decode::<Examples>(msg)?;
    let mut out = [0xAAu8; 80];
    let mut w = crate::MessageWriter::new::<Examples>(&mut out, 97)?;
    copy_root(&v, &mut w)?;
    let (text, _) = v.tail::<Examples>()?.var_data(&BMR_DATA[0])?;
    w.var_data(&BMR_DATA[0], text.unwrap_or_default())?;
    let len = w.finish()?;
    assert_eq!(first_difference(&out[..len], msg), None);
    Ok(())
}
