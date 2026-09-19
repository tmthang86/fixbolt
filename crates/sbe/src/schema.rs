//! The layout tables a schema compiles to, and the [`Schema`] trait that hands
//! them to the reader (ADR-0081 decision 2: tables, not flyweights).
//!
//! **These types are the contract `sbe-gen` emits.** Every table is `&'static`
//! data built from `const` expressions, so a generated schema is a handful of
//! `static` items and one `impl Schema`, with no code per message. One reader
//! walks every schema.
//!
//! # What the generator must guarantee
//!
//! The reader bounds-checks every byte it touches, so a wrong table can never
//! read out of bounds — but it can read the wrong bytes. These are the rules
//! the tables must hold, and the generator is where they are enforced:
//!
//! - **Offsets are resolved.** [`FieldLayout::offset`] is the byte offset from
//!   the start of the block (root block or group entry), with any `offset`
//!   attribute of the schema applied. [`Element::offset`] is relative to the
//!   field. Composites are flattened: a composite field has one [`Element`] per
//!   leaf member, nested composites included, named by their dotted path.
//! - **Lengths are wire lengths.** [`FieldLayout::len`] counts only bytes on the
//!   wire; a `constant` element contributes zero.
//! - **Order is schema order.** `groups` and `var_data` are listed in the order
//!   they appear in the schema, because that is the order they appear on the
//!   wire and the cursor walks them in that order (SBE 1.0 RC4, "Sequence of message body elements").
//! - **Only the recommended header.** The message header is the recommended
//!   four-`uint16` composite ([`crate::HEADER_LEN`]); any other `messageHeader`
//!   is rejected by the generator.
//! - **`sinceVersion` is carried on fields, groups and `varData`.** It is not
//!   carried on composite members; a composite member added in a later version
//!   is out of scope (ADR-0081 decision 5).

use crate::wire::ByteOrder;

/// An SBE primitive type (SBE 1.0 RC4 §2, "Field Encoding").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    /// `char`: one byte, US-ASCII unless the schema says otherwise.
    Char,
    /// `int8`.
    Int8,
    /// `int16`.
    Int16,
    /// `int32`.
    Int32,
    /// `int64`.
    Int64,
    /// `uint8`.
    UInt8,
    /// `uint16`.
    UInt16,
    /// `uint32`.
    UInt32,
    /// `uint64`.
    UInt64,
    /// `float`: IEEE 754 binary32.
    Float,
    /// `double`: IEEE 754 binary64.
    Double,
}

impl Primitive {
    /// Size of one value on the wire, in bytes.
    #[must_use]
    pub const fn size(self) -> usize {
        match self {
            Self::Char | Self::Int8 | Self::UInt8 => 1,
            Self::Int16 | Self::UInt16 => 2,
            Self::Int32 | Self::UInt32 | Self::Float => 4,
            Self::Int64 | Self::UInt64 | Self::Double => 8,
        }
    }
}

/// A decoded value, or a value written in a table (a constant, a null value).
///
/// Integers are widened: every signed type to `i64`, every unsigned type to
/// `u64`, `float` to `f64`. A fixed-length array (`length` > 1), of `char` or
/// of any other primitive, is its raw bytes; the caller interprets a numeric
/// array with the element's primitive and the schema's byte order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    /// A single `char`.
    Char(u8),
    /// `int8`, `int16`, `int32` or `int64`, sign-extended.
    Int(i64),
    /// `uint8`, `uint16`, `uint32` or `uint64`, zero-extended.
    UInt(u64),
    /// `float` (widened) or `double`.
    Float(f64),
    /// A fixed-length array, as its bytes on the wire (or, for a constant, as
    /// the schema wrote it).
    Array(&'a [u8]),
}

/// Whether and how an element is present on the wire (SBE 1.0 RC4 §2, `presence` and `nullValue`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Presence {
    /// Always on the wire and always a value.
    Required,
    /// On the wire; the value `null` means "not set" and reads as absent.
    ///
    /// For a scalar, `null` is the type's null value — the schema's
    /// `nullValue`, or the default of SBE 1.0 RC4 §2's null-value tables (for example
    /// `Int(i64::MIN)` for `int64`, `UInt(255)` for `uint8`, `Char(0)` for
    /// `char`, `Float(f64::NAN)` for either floating type; any NaN on the wire
    /// then reads as null). For a `char` array, `null` is `Char(c)` and the
    /// array is null when **every** byte is `c`. A numeric array is never null.
    Optional {
        /// The null value, in the widened form of [`Value`].
        null: Value<'static>,
    },
    /// Not on the wire; the schema supplies the value (`presence="constant"`,
    /// including a field's `valueRef`). Zero wire bytes.
    Constant(Value<'static>),
}

/// One leaf of a field's encoding: the field itself for a simple type, or one
/// member of a (flattened) composite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Element {
    /// Empty for a simple type; the member's dotted path inside a composite
    /// (`"mantissa"`, `"booster.boostType"`).
    pub name: &'static str,
    /// Byte offset from the start of the field. Ignored for a constant.
    pub offset: u16,
    /// The primitive type of one value.
    pub primitive: Primitive,
    /// Number of values: 1 for a scalar, `length` for a fixed-length array.
    /// Never 0 (a `length="0"` type is `varData`, see [`VarDataLayout`]).
    /// Ignored for a constant.
    pub length: u16,
    /// Required, optional with its null value, or constant with its value.
    pub presence: Presence,
}

impl Element {
    /// Bytes this element occupies on the wire: 0 for a constant.
    #[must_use]
    pub const fn wire_len(&self) -> usize {
        match self.presence {
            Presence::Constant(_) => 0,
            Presence::Required | Presence::Optional { .. } => {
                self.primitive.size() * self.length as usize
            }
        }
    }
}

/// A `<field>` of a message or group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldLayout {
    /// The schema `id` (by convention the FIX tag).
    pub id: u16,
    /// The schema `name`.
    pub name: &'static str,
    /// Byte offset from the start of the block.
    pub offset: u16,
    /// Bytes the field occupies on the wire: the end of its last non-constant
    /// element. 0 for a field made only of constants.
    pub len: u16,
    /// The schema version that added the field; a message whose header version
    /// is lower does not carry it, and it reads as absent (ADR-0081 decision 4).
    pub since_version: u16,
    /// One element for a simple type or enum or set; one per leaf member for a
    /// composite. Never empty.
    pub elements: &'static [Element],
}

/// Width of an unsigned length or count on the wire: a group dimension member
/// or a `varData` length prefix. SBE 1.0 RC4 ("Range of group entry count") requires `uint8` and
/// `uint16` and allows others; `uint32` is here because Real Logic's
/// `varStringEncoding` uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthType {
    /// `uint8`.
    U8,
    /// `uint16`.
    U16,
    /// `uint32`.
    U32,
}

impl LengthType {
    /// Size on the wire, in bytes.
    #[must_use]
    pub const fn size(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
            Self::U32 => 4,
        }
    }
}

/// The group dimension composite (`groupSizeEncoding` or the group's
/// `dimensionType`): `blockLength` then `numInGroup`, contiguous, in that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DimensionLayout {
    /// Width of `blockLength`.
    pub block_length: LengthType,
    /// Width of `numInGroup`.
    pub num_in_group: LengthType,
}

impl DimensionLayout {
    /// The recommended `groupSizeEncoding`: `uint16` + `uint16`.
    pub const GROUP_SIZE_ENCODING: Self = Self {
        block_length: LengthType::U16,
        num_in_group: LengthType::U16,
    };

    /// Bytes the dimension occupies on the wire.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.block_length.size() + self.num_in_group.size()
    }
}

/// A `<data>` element: a length prefix then that many bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarDataLayout {
    /// The schema `id`.
    pub id: u16,
    /// The schema `name`.
    pub name: &'static str,
    /// Width of the `length` member of the encoding composite.
    pub length: LengthType,
    /// The schema version that added it; absent from older messages.
    pub since_version: u16,
}

/// A `<group>`: its dimension, then per entry a block of fixed fields, then
/// the entry's nested groups, then its `varData`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupLayout {
    /// The schema `id`.
    pub id: u16,
    /// The schema `name`.
    pub name: &'static str,
    /// The entry block length the schema states or implies. The reader uses
    /// the value on the wire; this one is what an encoder writes.
    pub block_length: u16,
    /// The schema version that added the group; absent from older messages.
    pub since_version: u16,
    /// The dimension composite.
    pub dimension: DimensionLayout,
    /// Fixed fields of one entry.
    pub fields: &'static [FieldLayout],
    /// Nested groups, in schema order.
    pub groups: &'static [GroupLayout],
    /// `varData` of one entry, in schema order.
    pub var_data: &'static [VarDataLayout],
}

/// A `<message>`: the root block's fields, then groups, then `varData`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MessageLayout {
    /// The schema `id` of the message, carried in the header as `templateId`.
    pub template_id: u16,
    /// The schema `name`.
    pub name: &'static str,
    /// The root block length the schema states or implies. The reader uses the
    /// value in the header; this one is what an encoder writes.
    pub block_length: u16,
    /// The schema version that added the message.
    pub since_version: u16,
    /// Fixed fields of the root block.
    pub fields: &'static [FieldLayout],
    /// Groups at the root, in schema order.
    pub groups: &'static [GroupLayout],
    /// `varData` at the root, in schema order.
    pub var_data: &'static [VarDataLayout],
}

impl MessageLayout {
    /// The root field with schema id `id`. A linear scan: for lookup by id off
    /// the hot path; a hot path holds the `&'static FieldLayout` itself.
    #[must_use]
    pub fn field(&self, id: u16) -> Option<&'static FieldLayout> {
        self.fields.iter().find(|f| f.id == id)
    }
}

impl GroupLayout {
    /// The entry field with schema id `id`. A linear scan, as
    /// [`MessageLayout::field`].
    #[must_use]
    pub fn field(&self, id: u16) -> Option<&'static FieldLayout> {
        self.fields.iter().find(|f| f.id == id)
    }
}

impl FieldLayout {
    /// The element named `name` (empty for a simple type). A linear scan.
    #[must_use]
    pub fn element(&self, name: &str) -> Option<&'static Element> {
        self.elements.iter().find(|e| e.name == name)
    }
}

/// A compiled schema: its identity, its byte order and its messages.
///
/// Implemented by generated code, on a unit type:
///
/// ```
/// use fixbolt_sbe::{ByteOrder, MessageLayout, Schema};
///
/// struct Empty;
/// impl Schema for Empty {
///     const ID: u16 = 7;
///     const VERSION: u16 = 0;
///     const BYTE_ORDER: ByteOrder = ByteOrder::Little;
///     fn message(_template_id: u16) -> Option<&'static MessageLayout> {
///         None
///     }
/// }
/// assert!(Empty::message(1).is_none());
/// ```
pub trait Schema {
    /// `<messageSchema id=…>`, compared with the header's `schemaId`.
    const ID: u16;
    /// `<messageSchema version=…>`: the newest version this table knows.
    const VERSION: u16;
    /// `<messageSchema byteOrder=…>`: body, header and length prefixes.
    const BYTE_ORDER: ByteOrder;

    /// The layout of template `template_id`, or `None` when the schema has no
    /// such message. Generated as a `match`, so it is a jump, not a search.
    fn message(template_id: u16) -> Option<&'static MessageLayout>;
}
