//! One hand-written SBE schema (`SimplifiedCar`) and its wire bytes, shared by
//! `examples/sbe_decode.rs` and `tests/sbe_example.rs` — the same shape
//! `tests/end_to_end.rs` uses for `order_handler.rs`: a test that decoded its
//! own copy of the schema could not catch this one being wrong.
//!
//! Not `sbe-gen`'s output (`crates/sbe/src/tests.rs` and `sbe-gen`'s own tests
//! check the generator); this is what a caller writes by hand for one message,
//! the way `crates/sbe/src/tests.rs`'s module doc describes.

use fixbolt::sbe::{
    ByteOrder, Element, FieldLayout, MessageLayout, Presence, Primitive, SbeError, SbeView, Schema,
    Value,
};

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
const CHAR: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::Char,
    length: 1,
    presence: Presence::Required,
}];
/// A 4-byte fixed vehicle code, e.g. `"CAR1"`.
const CHAR4: &[Element] = &[Element {
    name: "",
    offset: 0,
    primitive: Primitive::Char,
    length: 4,
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

/// `SerialNumber`, `ModelYear`, `Available`, `Code` — flat, no groups, no
/// `varData`. Offsets and `block_length` are the sum of what came before,
/// exactly as a generator would compute them from a schema file.
static CAR_FIELDS: [FieldLayout; 4] = [
    field(1, "SerialNumber", 0, 8, UINT64),
    field(2, "ModelYear", 8, 2, UINT16),
    field(3, "Available", 10, 1, CHAR),
    field(4, "Code", 11, 4, CHAR4),
];
static SIMPLIFIED_CAR: MessageLayout = MessageLayout {
    template_id: 1,
    name: "SimplifiedCar",
    block_length: 15,
    since_version: 0,
    fields: &CAR_FIELDS,
    groups: &[],
    var_data: &[],
};

/// One template, one schema — enough for the one message below.
pub struct CarSchema;
impl Schema for CarSchema {
    const ID: u16 = 7;
    const VERSION: u16 = 0;
    const BYTE_ORDER: ByteOrder = ByteOrder::Little;
    fn message(template_id: u16) -> Option<&'static MessageLayout> {
        match template_id {
            1 => Some(&SIMPLIFIED_CAR),
            _ => None,
        }
    }
}

/// One `SimplifiedCar` message, written the way it would arrive over any
/// transport: the 8-byte recommended header, then the 15-byte root block, in
/// `CarSchema::BYTE_ORDER`.
#[must_use]
pub fn wire_bytes() -> Vec<u8> {
    let mut out = Vec::with_capacity(23);
    // Header: blockLength=15, templateId=1, schemaId=7, version=0.
    out.extend_from_slice(&15u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&7u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    // Root block.
    out.extend_from_slice(&123_456_789u64.to_le_bytes());
    out.extend_from_slice(&2026u16.to_le_bytes());
    out.extend_from_slice(b"Y");
    out.extend_from_slice(b"CAR1");
    out
}

/// The four root fields of one decoded `SimplifiedCar`, plus what the header
/// said about it.
#[derive(Debug)]
pub struct Decoded<'a> {
    /// The schema name of the message (`"SimplifiedCar"`).
    pub name: &'static str,
    /// The header's `templateId`.
    pub template_id: u16,
    /// Field 1.
    pub serial_number: Option<Value<'a>>,
    /// Field 2.
    pub model_year: Option<Value<'a>>,
    /// Field 3.
    pub available: Option<Value<'a>>,
    /// Field 4.
    pub code: Option<Value<'a>>,
}

/// Decodes one `SimplifiedCar` out of `bytes` — no socket, no session, just
/// `fixbolt::sbe`'s reader over [`CarSchema`]'s tables.
///
/// # Errors
/// Whatever [`SbeView::decode`], [`SbeView::layout`], [`SbeView::root`] or
/// [`crate::Block::value`] return for a message that does not match the
/// schema above.
pub fn decode(bytes: &[u8]) -> Result<Decoded<'_>, SbeError> {
    let view = SbeView::decode::<CarSchema>(bytes)?;
    let layout = view.layout::<CarSchema>()?;
    let root = view.root::<CarSchema>()?;
    let get = |id: u16| -> Result<Option<Value<'_>>, SbeError> {
        match layout.field(id) {
            Some(f) => root.value(f),
            None => Ok(None),
        }
    };
    Ok(Decoded {
        name: layout.name,
        template_id: view.template_id(),
        serial_number: get(1)?,
        model_year: get(2)?,
        available: get(3)?,
        code: get(4)?,
    })
}
