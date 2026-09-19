//! The SBE 1.0 XML → Rust generator core (ADR-0081 decision 2).
//!
//! Loaded twice: once as a normal module of this crate's public API
//! (`src/lib.rs`), and once by `build.rs` via `#[path]`, because `build.rs`
//! cannot `use` the crate it is building for — the same shape
//! `crates/dict/build.rs` uses for `src/field_type.rs`. Both call sites get
//! exactly the same generator, which is the point: there is one reader
//! (`fixbolt_sbe`) and this is the one writer of the tables it reads.
//!
//! # Scope (ADR-0081 decision 5)
//!
//! In scope: little- and big-endian schemas; `char`, integer and floating
//! primitives; `composite`, `enum`, `set`; fixed-length arrays; constants;
//! `optional` presence with null values; groups, nested groups; `varData`;
//! one level of `<ref>` inside a composite. Anything else is refused with
//! [`Error::Unsupported`], naming the construct — never silently generated
//! as something else.
//!
//! # Table contract
//!
//! Every table this module emits must satisfy the rules `crates/sbe/src/
//! schema.rs` documents on [`Element`], [`FieldLayout`], [`GroupLayout`],
//! [`MessageLayout`] and [`VarDataLayout`] mentioned there (names repeated
//! here only for orientation; the generator does not depend on
//! `fixbolt_sbe` — it emits source text that refers to those types by name).
//! In particular: offsets are resolved (no call-site arithmetic in the
//! generated code), a `constant` element contributes zero wire bytes, and
//! `groups`/`var_data` are listed in schema order.
//!
//! [`Element`]: https://docs.rs/fixbolt-sbe (crate not a dependency here)
//! [`FieldLayout`]: https://docs.rs/fixbolt-sbe
//! [`GroupLayout`]: https://docs.rs/fixbolt-sbe
//! [`MessageLayout`]: https://docs.rs/fixbolt-sbe
//! [`VarDataLayout`]: https://docs.rs/fixbolt-sbe

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use roxmltree::{Document, Node};

// --------------------------------------------------------------------------
// Public API
// --------------------------------------------------------------------------

/// Everything that can go wrong turning a schema into tables.
///
/// [`Error::Unsupported`] names the schema element or attribute that put the
/// schema outside ADR-0081 decision 5's scope; it is the *only* variant that
/// means "this schema asked for something the generator refuses to guess
/// about" — every other variant means the XML itself is malformed or
/// self-contradictory.
#[derive(Debug)]
pub enum Error {
    /// The XML did not parse.
    Xml(String),
    /// The XML parsed, but is not a schema this generator can read (a
    /// missing required attribute, an unresolvable reference, two types
    /// sharing one name, and so on).
    Schema(String),
    /// A specific, named construct that ADR-0081 decision 5 puts out of
    /// scope. Never generated as a guess — this is the one variant a caller
    /// should expect to see for a schema that uses a feature phase 2 does
    /// not cover.
    Unsupported(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(msg) => write!(f, "not well-formed XML: {msg}"),
            Self::Schema(msg) => write!(f, "schema error: {msg}"),
            Self::Unsupported(what) => write!(f, "unsupported SBE construct: {what}"),
        }
    }
}

impl std::error::Error for Error {}

/// Generates Rust source for `xml`, a self-contained SBE 1.0 schema with no
/// `xi:include`. Returns [`Error::Unsupported`] naming `"xi:include"` if the
/// schema has one — use [`generate_with_includes`] instead.
pub fn generate(xml: &str) -> Result<String, Error> {
    let doc = Document::parse(xml).map_err(|e| Error::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.children().any(|n| n.is_element() && is_xinclude(n)) {
        return Err(Error::Unsupported("xi:include"));
    }
    generate_impl(root, &[])
}

/// Generates Rust source for `xml`, resolving any `xi:include href="…"` at
/// the schema's top level by calling `resolve(href)`. `resolve` returns the
/// included document's text (itself a bare `<types>…</types>` root, the SBE
/// convention `common-types.xml` follows) or `None` if `href` cannot be
/// found, which is reported as [`Error::Schema`].
pub fn generate_with_includes(
    xml: &str,
    resolve: impl Fn(&str) -> Option<String>,
) -> Result<String, Error> {
    let root_doc = Document::parse(xml).map_err(|e| Error::Xml(e.to_string()))?;
    let root = root_doc.root_element();

    // Resolved include text is collected first, and OWNED here, so the
    // `Document`s parsed from it can borrow from a place that outlives them.
    let mut included_xml: Vec<String> = Vec::new();
    for child in root.children().filter(|n| n.is_element()) {
        if !is_xinclude(child) {
            continue;
        }
        let href = child
            .attribute("href")
            .ok_or_else(|| Error::Schema("xi:include with no href".to_string()))?;
        let text = resolve(href)
            .ok_or_else(|| Error::Schema(format!("xi:include href '{href}' not resolved")))?;
        included_xml.push(text);
    }
    let mut included_docs: Vec<Document<'_>> = Vec::with_capacity(included_xml.len());
    for text in &included_xml {
        included_docs.push(Document::parse(text).map_err(|e| Error::Xml(e.to_string()))?);
    }

    generate_impl(root, &included_docs)
}

// --------------------------------------------------------------------------
// Primitives, values, presence — the leaf vocabulary of a table
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Primitive {
    Char,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float,
    Double,
}

impl Primitive {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "char" => Self::Char,
            "int8" => Self::Int8,
            "int16" => Self::Int16,
            "int32" => Self::Int32,
            "int64" => Self::Int64,
            "uint8" => Self::UInt8,
            "uint16" => Self::UInt16,
            "uint32" => Self::UInt32,
            "uint64" => Self::UInt64,
            "float" => Self::Float,
            "double" => Self::Double,
            _ => return None,
        })
    }

    const fn size(self) -> u16 {
        match self {
            Self::Char | Self::Int8 | Self::UInt8 => 1,
            Self::Int16 | Self::UInt16 => 2,
            Self::Int32 | Self::UInt32 | Self::Float => 4,
            Self::Int64 | Self::UInt64 | Self::Double => 8,
        }
    }

    /// The `fixbolt_sbe::schema::Primitive` variant name this maps to.
    const fn rust_name(self) -> &'static str {
        match self {
            Self::Char => "Char",
            Self::Int8 => "Int8",
            Self::Int16 => "Int16",
            Self::Int32 => "Int32",
            Self::Int64 => "Int64",
            Self::UInt8 => "UInt8",
            Self::UInt16 => "UInt16",
            Self::UInt32 => "UInt32",
            Self::UInt64 => "UInt64",
            Self::Float => "Float",
            Self::Double => "Double",
        }
    }

    /// Whether this primitive widens to a signed (`Value::Int`) or unsigned
    /// (`Value::UInt`) integer, per `crate::schema::Value`'s widening rule.
    const fn is_signed(self) -> bool {
        matches!(self, Self::Int8 | Self::Int16 | Self::Int32 | Self::Int64)
    }

    const fn is_float(self) -> bool {
        matches!(self, Self::Float | Self::Double)
    }
}

/// SBE 1.0 RC4 §2's null-value table, used when `presence="optional"` names
/// no `nullValue` of its own.
fn default_null(primitive: Primitive) -> Value {
    match primitive {
        Primitive::Char => Value::Char(0),
        Primitive::Int8 => Value::Int(i64::from(i8::MIN)),
        Primitive::Int16 => Value::Int(i64::from(i16::MIN)),
        Primitive::Int32 => Value::Int(i64::from(i32::MIN)),
        Primitive::Int64 => Value::Int(i64::MIN),
        Primitive::UInt8 => Value::UInt(u64::from(u8::MAX)),
        Primitive::UInt16 => Value::UInt(u64::from(u16::MAX)),
        Primitive::UInt32 => Value::UInt(u64::from(u32::MAX)),
        Primitive::UInt64 => Value::UInt(u64::MAX),
        Primitive::Float | Primitive::Double => Value::Float(f64::NAN),
    }
}

#[derive(Debug, Clone)]
enum Value {
    Char(u8),
    Int(i64),
    UInt(u64),
    Float(f64),
    /// A char-array or other fixed-length-array constant, as raw bytes.
    Bytes(Vec<u8>),
}

fn constant_value(primitive: Primitive, text: &str) -> Result<Value, Error> {
    let text = text.trim();
    if primitive == Primitive::Char {
        let byte_len = text.len();
        return Ok(if byte_len <= 1 {
            Value::Char(text.as_bytes().first().copied().unwrap_or(0))
        } else {
            Value::Bytes(text.as_bytes().to_vec())
        });
    }
    if primitive.is_float() {
        return text
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|e| Error::Schema(format!("constant '{text}' is not a float: {e}")));
    }
    if primitive.is_signed() {
        return text
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|e| Error::Schema(format!("constant '{text}' is not an integer: {e}")));
    }
    text.parse::<u64>()
        .map(Value::UInt)
        .map_err(|e| Error::Schema(format!("constant '{text}' is not an unsigned integer: {e}")))
}

#[derive(Debug, Clone)]
enum Presence {
    Required,
    Optional { null: Value },
    Constant(Value),
}

#[derive(Debug, Clone)]
struct Element {
    /// Empty for a simple type; the member's dotted path inside a composite.
    path: String,
    offset: u16,
    primitive: Primitive,
    length: u16,
    presence: Presence,
}

/// Bytes on the wire: 0 for a constant, matching `Element::wire_len`.
fn wire_len(primitive: Primitive, length: u16, presence: &Presence) -> u16 {
    match presence {
        Presence::Constant(_) => 0,
        Presence::Required | Presence::Optional { .. } => primitive.size() * length,
    }
}

// --------------------------------------------------------------------------
// The merged `<types>` table
// --------------------------------------------------------------------------

/// Every `<type>`, `<composite>`, `<enum>` and `<set>` this schema declares,
/// by name — merged across the schema's own `<types>` blocks and any
/// resolved `<xi:include>`. `BTreeMap` keeps lookups (and, incidentally,
/// iteration, though nothing here iterates it) deterministic.
struct Types<'a, 'i> {
    by_name: BTreeMap<&'a str, Node<'a, 'i>>,
}

impl<'a, 'i> Types<'a, 'i> {
    fn get(&self, name: &str) -> Option<Node<'a, 'i>> {
        self.by_name.get(name).copied()
    }
}

fn collect_type_defs<'a, 'i>(types_el: Node<'a, 'i>, out: &mut Vec<Node<'a, 'i>>) {
    for child in types_el.children() {
        if child.is_element() && matches!(local_name(child), "type" | "composite" | "enum" | "set")
        {
            out.push(child);
        }
    }
}

fn build_types<'a, 'i>(nodes: Vec<Node<'a, 'i>>) -> Result<Types<'a, 'i>, Error> {
    let mut by_name = BTreeMap::new();
    for node in nodes {
        let name = node
            .attribute("name")
            .ok_or_else(|| Error::Schema(format!("<{}> with no name", local_name(node))))?;
        if by_name.insert(name, node).is_some() {
            return Err(Error::Schema(format!("type '{name}' declared twice")));
        }
    }
    Ok(Types { by_name })
}

/// A node's tag name, ignoring any namespace prefix. Both fixture schemas
/// mix prefixed (`sbe:message`) and unprefixed (`types`, `field`) elements,
/// and the two schemas bind the `sbe` prefix to two different namespace
/// URIs — matching on the local name only is what makes one generator read
/// both without caring which URI a given schema chose.
fn local_name<'i>(node: Node<'_, 'i>) -> &'i str {
    node.tag_name().name()
}

fn is_xinclude(node: Node<'_, '_>) -> bool {
    local_name(node) == "include"
}

fn is_element_iter<'a, 'i>(node: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(Node::is_element)
}

// --------------------------------------------------------------------------
// Resolving a `<field>`/`<data>` `type="…"` reference into elements
// --------------------------------------------------------------------------

/// Resolves `type_ref` to its flattened, offset-resolved elements, and the
/// type's size on the wire.
///
/// The size is what a field of this type occupies in its block, or a `<ref>`
/// to it in its composite: for a composite it is `flatten_composite`'s, which
/// includes the padding a member's `offset` introduces (RC4
/// `04MessageSchema.md` "Element offset within a composite type") — never
/// the sum of the elements' own sizes.
///
/// `prefix` is the dotted path so far (empty at the top of a field).
/// `ref_depth` counts `<ref>` hops already taken; ADR-0081 decision 5 allows
/// exactly one, so a second one is [`Error::Unsupported`].
/// `presence_override` is `Some` only when the enclosing `<field>` itself
/// carries `presence`/`valueRef` — SBE allows that only for a field whose
/// type resolves to a single scalar/enum/set element, never a composite.
fn flatten(
    type_ref: &str,
    types: &Types<'_, '_>,
    prefix: &str,
    ref_depth: u32,
    presence_override: Option<Presence>,
) -> Result<(Vec<Element>, u32), Error> {
    let single = |el: Element| {
        let size = u32::from(wire_len(el.primitive, el.length, &el.presence));
        (vec![el], size)
    };
    if let Some(primitive) = Primitive::from_name(type_ref) {
        let presence = presence_override.unwrap_or(Presence::Required);
        return Ok(single(Element {
            path: prefix.to_string(),
            offset: 0,
            primitive,
            length: 1,
            presence,
        }));
    }
    let node = types
        .get(type_ref)
        .ok_or_else(|| Error::Schema(format!("unknown type '{type_ref}'")))?;
    match local_name(node) {
        "type" => element_from_type_node(node, prefix, presence_override).map(single),
        "enum" => element_from_enum_node(node, types, prefix, presence_override).map(single),
        "set" => element_from_set_node(node, types, prefix, presence_override).map(single),
        "composite" => {
            if presence_override.is_some() {
                return Err(Error::Unsupported(
                    "presence or valueRef on a field whose type is a composite",
                ));
            }
            flatten_composite(node, types, prefix, ref_depth)
        }
        _other => Err(Error::Unsupported(
            "type reference resolves to neither type, enum, set nor composite",
        )),
    }
}

fn element_from_type_node(
    node: Node<'_, '_>,
    prefix: &str,
    presence_override: Option<Presence>,
) -> Result<Element, Error> {
    let primitive_name = node
        .attribute("primitiveType")
        .ok_or_else(|| Error::Schema(format!("<type name='{prefix}'> with no primitiveType")))?;
    let primitive = Primitive::from_name(primitive_name)
        .ok_or(Error::Unsupported("primitiveType outside SBE 1.0's eleven"))?;
    let length: u16 = match node.attribute("length") {
        Some(s) => s
            .parse()
            .map_err(|_| Error::Schema(format!("<type> length '{s}' is not a u16")))?,
        None => 1,
    };
    if length == 0 {
        // `length="0"` is the `varData` member of a DATA-shaped composite,
        // handled by `parse_vardata`, never by generic flattening.
        return Err(Error::Unsupported(
            "length=\"0\" outside a <data> composite",
        ));
    }
    let presence = match presence_override {
        Some(p) => p,
        None => resolve_presence(node, primitive)?,
    };
    if matches!(presence, Presence::Optional { .. }) && primitive != Primitive::Char && length > 1 {
        // `fixbolt_sbe` nulls an array only as a char array (every byte the
        // null char); ADR-0081 decision 5 does not cover any other.
        return Err(Error::Unsupported(
            "presence=\"optional\" on a non-char array (length > 1)",
        ));
    }
    Ok(Element {
        path: prefix.to_string(),
        offset: 0,
        primitive,
        length,
        presence,
    })
}

/// Resolves a `<type>`'s `presence` attribute (default `required`).
///
/// The required/optional/constant vocabulary is SBE 1.0 RC4 §2. A constant's
/// value is always read through [`constant_value`], whose own char-vs-array
/// decision (single byte or fewer is a scalar `Char`, more is a byte array)
/// depends on the constant *text*, not on the type's declared `length` —
/// one rule, in one place, rather than this function guessing again from a
/// `length` attribute that a constant like `Engine.fuel` (SBE's own sample
/// schema) does not even bother to declare.
fn resolve_presence(node: Node<'_, '_>, primitive: Primitive) -> Result<Presence, Error> {
    match node.attribute("presence").unwrap_or("required") {
        "required" => Ok(Presence::Required),
        "optional" => {
            let null = match node.attribute("nullValue") {
                Some(text) => constant_value(primitive, text)?,
                None => default_null(primitive),
            };
            Ok(Presence::Optional { null })
        }
        "constant" => Ok(Presence::Constant(constant_value(
            primitive,
            node.text().unwrap_or(""),
        )?)),
        other => Err(Error::Schema(format!(
            "presence '{other}' is not one of required/optional/constant"
        ))),
    }
}

fn resolve_encoding_primitive(
    encoding_type: &str,
    types: &Types<'_, '_>,
) -> Result<Primitive, Error> {
    if let Some(p) = Primitive::from_name(encoding_type) {
        return Ok(p);
    }
    let node = types
        .get(encoding_type)
        .ok_or_else(|| Error::Schema(format!("unknown encodingType '{encoding_type}'")))?;
    if local_name(node) != "type" {
        return Err(Error::Unsupported(
            "enum/set encodingType that is not a scalar type",
        ));
    }
    let primitive_name = node
        .attribute("primitiveType")
        .ok_or_else(|| Error::Schema("encodingType <type> with no primitiveType".to_string()))?;
    if node.attribute("length").is_some_and(|l| l != "1") {
        return Err(Error::Unsupported(
            "enum/set encodingType with an array length",
        ));
    }
    Primitive::from_name(primitive_name)
        .ok_or(Error::Unsupported("enum/set encodingType primitive"))
}

fn element_from_enum_node(
    node: Node<'_, '_>,
    types: &Types<'_, '_>,
    prefix: &str,
    presence_override: Option<Presence>,
) -> Result<Element, Error> {
    let encoding_type = node
        .attribute("encodingType")
        .ok_or_else(|| Error::Schema(format!("<enum name='{prefix}'> with no encodingType")))?;
    let primitive = resolve_encoding_primitive(encoding_type, types)?;
    let presence = presence_override.unwrap_or(Presence::Required);
    Ok(Element {
        path: prefix.to_string(),
        offset: 0,
        primitive,
        length: 1,
        presence,
    })
}

fn element_from_set_node(
    node: Node<'_, '_>,
    types: &Types<'_, '_>,
    prefix: &str,
    presence_override: Option<Presence>,
) -> Result<Element, Error> {
    let encoding_type = node
        .attribute("encodingType")
        .ok_or_else(|| Error::Schema(format!("<set name='{prefix}'> with no encodingType")))?;
    let primitive = resolve_encoding_primitive(encoding_type, types)?;
    let presence = presence_override.unwrap_or(Presence::Required);
    Ok(Element {
        path: prefix.to_string(),
        offset: 0,
        primitive,
        length: 1,
        presence,
    })
}

/// Resolves a `valueRef="EnumName.ValueName"` attribute to the constant it
/// names, under `primitive` (the enum's own encoding primitive).
fn resolve_value_ref(
    value_ref: &str,
    types: &Types<'_, '_>,
    primitive: Primitive,
) -> Result<Value, Error> {
    let (enum_name, value_name) = value_ref
        .split_once('.')
        .ok_or_else(|| Error::Schema(format!("valueRef '{value_ref}' is not 'Enum.Value'")))?;
    let enum_node = types
        .get(enum_name)
        .filter(|n| local_name(*n) == "enum")
        .ok_or_else(|| Error::Schema(format!("valueRef names unknown enum '{enum_name}'")))?;
    let value_node = is_element_iter(enum_node)
        .find(|n| local_name(*n) == "validValue" && n.attribute("name") == Some(value_name))
        .ok_or_else(|| {
            Error::Schema(format!(
                "enum '{enum_name}' has no validValue '{value_name}'"
            ))
        })?;
    constant_value(primitive, value_node.text().unwrap_or(""))
}

/// Flattens a `<composite>`'s members, resolving offsets left to right.
///
/// Returns the flattened elements and the composite's total wire size (the
/// cursor's final position) — the size a `<ref>` to this composite must
/// advance the enclosing composite's own cursor by.
///
/// A constant member's *position* still advances the cursor by its wire
/// size (0), but the `offset` **written into its `Element`** is 0 unless an
/// explicit `offset` attribute says otherwise — matching
/// `crates/sbe/src/tests.rs`'s hand-written tables, and consistent with
/// `Element::offset` being documented as "ignored for a constant": the
/// value cannot be observed, so there is exactly one value to standardise
/// on.
fn flatten_composite(
    node: Node<'_, '_>,
    types: &Types<'_, '_>,
    prefix: &str,
    ref_depth: u32,
) -> Result<(Vec<Element>, u32), Error> {
    let mut out = Vec::new();
    let mut cursor: u32 = 0;
    for child in is_element_iter(node) {
        let name = child
            .attribute("name")
            .ok_or_else(|| Error::Schema("composite member with no name".to_string()))?;
        let member_prefix = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}.{name}")
        };
        let explicit_offset: Option<u32> = child
            .attribute("offset")
            .map(|s| {
                s.parse()
                    .map_err(|_| Error::Schema(format!("offset '{s}' is not a u32")))
            })
            .transpose()?;
        if child.attribute("sinceVersion").is_some() {
            return Err(Error::Unsupported("sinceVersion on a composite member"));
        }
        if let Some(o) = explicit_offset
            && o < cursor
        {
            return Err(Error::Schema(format!(
                "composite member '{member_prefix}' offset {o} would overlap the members before it, \
                 which end at {cursor} (RC4 \"Element offset within a composite type\")"
            )));
        }
        let effective_pos = explicit_offset.unwrap_or(cursor);

        match local_name(child) {
            "ref" => {
                if ref_depth >= 1 {
                    return Err(Error::Unsupported("<ref> nested more than one level deep"));
                }
                let target = child
                    .attribute("type")
                    .ok_or_else(|| Error::Schema(format!("<ref name='{name}'> with no type")))?;
                let (sub, size) = flatten(target, types, &member_prefix, ref_depth + 1, None)?;
                for mut e in sub {
                    e.offset = u16::try_from(u32::from(e.offset) + effective_pos)
                        .map_err(|_| Error::Unsupported("offset exceeds u16"))?;
                    out.push(e);
                }
                cursor = effective_pos + size;
            }
            "type" | "enum" | "set" => {
                let mut el = match local_name(child) {
                    "type" => element_from_type_node(child, &member_prefix, None)?,
                    "enum" => element_from_enum_node(child, types, &member_prefix, None)?,
                    _ => element_from_set_node(child, types, &member_prefix, None)?,
                };
                let is_constant = matches!(el.presence, Presence::Constant(_));
                let size = u32::from(wire_len(el.primitive, el.length, &el.presence));
                el.offset = if is_constant {
                    u16::try_from(explicit_offset.unwrap_or(0))
                        .map_err(|_| Error::Unsupported("offset exceeds u16"))?
                } else {
                    u16::try_from(effective_pos)
                        .map_err(|_| Error::Unsupported("offset exceeds u16"))?
                };
                out.push(el);
                cursor = effective_pos + size;
            }
            _other => {
                return Err(Error::Unsupported(
                    "composite member that is not type, enum, set or ref",
                ));
            }
        }
    }
    Ok((out, cursor))
}

// --------------------------------------------------------------------------
// Fields, groups, varData, messages
// --------------------------------------------------------------------------

struct FieldDef {
    id: u16,
    name: String,
    offset: u16,
    len: u16,
    since_version: u16,
    elements: Vec<Element>,
}

struct GroupDef {
    id: u16,
    name: String,
    block_length: u16,
    since_version: u16,
    dimension: Dimension,
    fields: Vec<FieldDef>,
    groups: Vec<GroupDef>,
    var_data: Vec<VarDataDef>,
}

struct VarDataDef {
    id: u16,
    name: String,
    length: LengthType,
    since_version: u16,
}

struct MessageDef {
    template_id: u16,
    name: String,
    block_length: u16,
    since_version: u16,
    fields: Vec<FieldDef>,
    groups: Vec<GroupDef>,
    var_data: Vec<VarDataDef>,
}

#[derive(Clone, Copy)]
enum LengthType {
    U8,
    U16,
    U32,
}

impl LengthType {
    const fn rust_name(self) -> &'static str {
        match self {
            Self::U8 => "U8",
            Self::U16 => "U16",
            Self::U32 => "U32",
        }
    }
}

struct Dimension {
    block_length: LengthType,
    num_in_group: LengthType,
}

fn length_type_of(node: Node<'_, '_>) -> Result<LengthType, Error> {
    let primitive_name = node
        .attribute("primitiveType")
        .ok_or_else(|| Error::Schema("dimension member with no primitiveType".to_string()))?;
    match Primitive::from_name(primitive_name) {
        Some(Primitive::UInt8) => Ok(LengthType::U8),
        Some(Primitive::UInt16) => Ok(LengthType::U16),
        Some(Primitive::UInt32) => Ok(LengthType::U32),
        _ => Err(Error::Unsupported(
            "length/dimension member primitive other than uint8/16/32",
        )),
    }
}

fn resolve_dimension(name: &str, types: &Types<'_, '_>) -> Result<Dimension, Error> {
    let node = types
        .get(name)
        .ok_or_else(|| Error::Schema(format!("unknown dimensionType '{name}'")))?;
    if local_name(node) != "composite" {
        return Err(Error::Unsupported("dimensionType that is not a composite"));
    }
    let members: Vec<Node<'_, '_>> = is_element_iter(node).collect();
    let [block_length_node, num_in_group_node] = members.as_slice() else {
        return Err(Error::Unsupported(
            "dimensionType composite without exactly two members",
        ));
    };
    if block_length_node.attribute("name") != Some("blockLength")
        || num_in_group_node.attribute("name") != Some("numInGroup")
    {
        return Err(Error::Unsupported(
            "dimensionType composite members named other than blockLength/numInGroup",
        ));
    }
    Ok(Dimension {
        block_length: length_type_of(*block_length_node)?,
        num_in_group: length_type_of(*num_in_group_node)?,
    })
}

fn parse_vardata(node: Node<'_, '_>, types: &Types<'_, '_>) -> Result<VarDataDef, Error> {
    let id = required_u16_attr(node, "id")?;
    let name = required_attr(node, "name")?.to_string();
    let since_version = optional_u16_attr(node, "sinceVersion")?.unwrap_or(0);
    let type_ref = required_attr(node, "type")?;
    let type_node = types
        .get(type_ref)
        .ok_or_else(|| Error::Schema(format!("unknown data type '{type_ref}'")))?;
    if local_name(type_node) != "composite" {
        return Err(Error::Unsupported("<data> type that is not a composite"));
    }
    let members: Vec<Node<'_, '_>> = is_element_iter(type_node).collect();
    let [length_node, data_node] = members.as_slice() else {
        return Err(Error::Unsupported(
            "<data> composite without exactly two members",
        ));
    };
    if length_node.attribute("name") != Some("length")
        || data_node.attribute("name") != Some("varData")
    {
        return Err(Error::Unsupported(
            "<data> composite members named other than length/varData",
        ));
    }
    Ok(VarDataDef {
        id,
        name,
        length: length_type_of(*length_node)?,
        since_version,
    })
}

fn required_attr<'a>(node: Node<'a, '_>, name: &str) -> Result<&'a str, Error> {
    node.attribute(name)
        .ok_or_else(|| Error::Schema(format!("<{}> with no {name}", local_name(node))))
}

fn required_u16_attr(node: Node<'_, '_>, name: &str) -> Result<u16, Error> {
    let text = required_attr(node, name)?;
    text.parse().map_err(|_| {
        Error::Schema(format!(
            "<{}> {name}='{text}' is not a u16",
            local_name(node)
        ))
    })
}

fn optional_u16_attr(node: Node<'_, '_>, name: &str) -> Result<Option<u16>, Error> {
    node.attribute(name)
        .map(|text| {
            text.parse().map_err(|_| {
                Error::Schema(format!(
                    "<{}> {name}='{text}' is not a u16",
                    local_name(node)
                ))
            })
        })
        .transpose()
}

/// `presence`/`valueRef` on a `<field>` or `<data>` element itself, if any —
/// only ever `Some` for a field whose type resolves to a scalar/enum/set
/// (checked by `flatten`, which refuses the composite case).
fn field_presence_override(
    node: Node<'_, '_>,
    type_ref: &str,
    types: &Types<'_, '_>,
) -> Result<Option<Presence>, Error> {
    let Some(presence_attr) = node.attribute("presence") else {
        return Ok(None);
    };
    // The leaf primitive is needed before the override can be built (a
    // default null, or a constant's parsed value, both depend on it), so it
    // is resolved once here rather than threaded back out of `flatten`.
    let leaf_primitive = resolve_leaf_primitive(type_ref, types)?;
    match presence_attr {
        "required" => Ok(Some(Presence::Required)),
        "optional" => {
            let null = match node.attribute("nullValue") {
                Some(text) => constant_value(leaf_primitive, text)?,
                None => default_null(leaf_primitive),
            };
            Ok(Some(Presence::Optional { null }))
        }
        "constant" => {
            let value = if let Some(value_ref) = node.attribute("valueRef") {
                resolve_value_ref(value_ref, types, leaf_primitive)?
            } else {
                constant_value(leaf_primitive, node.text().unwrap_or(""))?
            };
            Ok(Some(Presence::Constant(value)))
        }
        other => Err(Error::Schema(format!(
            "presence '{other}' is not one of required/optional/constant"
        ))),
    }
}

/// The primitive a field's `type` resolves to, when it is a scalar, enum or
/// set — i.e. when a field-level `presence` override is even meaningful.
/// A composite target is [`Error::Unsupported`], matching `flatten`'s own
/// refusal of a presence override on a composite-typed field.
fn resolve_leaf_primitive(type_ref: &str, types: &Types<'_, '_>) -> Result<Primitive, Error> {
    if let Some(p) = Primitive::from_name(type_ref) {
        return Ok(p);
    }
    let node = types
        .get(type_ref)
        .ok_or_else(|| Error::Schema(format!("unknown type '{type_ref}'")))?;
    match local_name(node) {
        "type" => {
            let primitive_name = required_attr(node, "primitiveType")?;
            Primitive::from_name(primitive_name)
                .ok_or(Error::Unsupported("primitiveType outside SBE 1.0's eleven"))
        }
        "enum" | "set" => resolve_encoding_primitive(required_attr(node, "encodingType")?, types),
        "composite" => Err(Error::Unsupported(
            "presence or valueRef on a field whose type is a composite",
        )),
        _other => Err(Error::Unsupported(
            "type reference resolves to neither type, enum, set nor composite",
        )),
    }
}

fn parse_field(node: Node<'_, '_>, types: &Types<'_, '_>, cursor: u32) -> Result<FieldDef, Error> {
    let id = required_u16_attr(node, "id")?;
    let name = required_attr(node, "name")?.to_string();
    let type_ref = required_attr(node, "type")?;
    let since_version = optional_u16_attr(node, "sinceVersion")?.unwrap_or(0);
    let presence_override = field_presence_override(node, type_ref, types)?;
    let (elements, size) = flatten(type_ref, types, "", 0, presence_override)?;
    let len = u16::try_from(size).map_err(|_| Error::Unsupported("field length exceeds u16"))?;
    let explicit_offset = optional_u16_attr(node, "offset")?;
    let offset = match explicit_offset {
        Some(o) if u32::from(o) < cursor => {
            return Err(Error::Schema(format!(
                "field '{name}' offset {o} would overlap the fields before it, which end at \
                 {cursor} (RC4 \"Field offset specified by message schema\")"
            )));
        }
        Some(o) => o,
        None => u16::try_from(cursor).map_err(|_| Error::Unsupported("offset exceeds u16"))?,
    };
    Ok(FieldDef {
        id,
        name,
        offset,
        len,
        since_version,
        elements,
    })
}

/// A `<message>`'s or `<group>`'s children, in schema order: `<field>`s
/// first (each advancing the running block cursor), then `<group>`s, then
/// `<data>`s — SBE 1.0 RC4's "Sequence of message body elements". A
/// `<field>` after a `<group>`/`<data>`, or a `<group>` after a `<data>`, is
/// refused rather than silently reordered (`03MessageStructure.md`, "Fixed-
/// length field after repeating group or variable-length field" and
/// "Repeating group after variable-length field"). Explicit field offsets
/// that overlap an earlier field, and an explicit `blockLength` shorter than
/// the fields, are refused as [`Error::Schema`].
/// A message's or group's fixed fields, its groups, its `varData`, and the
/// fixed block's length — what `<message>` and `<group>` both parse to,
/// named once so their shared parser (`layout_block`) does not trip
/// `clippy::type_complexity`.
type Block = (Vec<FieldDef>, Vec<GroupDef>, Vec<VarDataDef>, u16);

fn layout_block(node: Node<'_, '_>, types: &Types<'_, '_>) -> Result<Block, Error> {
    let mut fields = Vec::new();
    let mut groups = Vec::new();
    let mut var_data = Vec::new();
    let mut cursor: u32 = 0;
    let mut fields_done = false;
    for child in is_element_iter(node) {
        if local_name(child) == "group" && !var_data.is_empty() {
            return Err(Error::Schema(
                "<group> declared after <data> at the same level (RC4 \"Repeating group after \
                 variable-length field\")"
                    .to_string(),
            ));
        }
        match local_name(child) {
            "field" => {
                if fields_done {
                    return Err(Error::Unsupported(
                        "<field> declared after <group> or <data>",
                    ));
                }
                let field = parse_field(child, types, cursor)?;
                cursor = cursor.max(u32::from(field.offset)) + u32::from(field.len);
                fields.push(field);
            }
            "group" => {
                fields_done = true;
                groups.push(parse_group(child, types)?);
            }
            "data" => {
                fields_done = true;
                var_data.push(parse_vardata(child, types)?);
            }
            _other => {
                return Err(Error::Unsupported(
                    "message or group child that is not field, group or data",
                ));
            }
        }
    }
    let block_length = match optional_u16_attr(node, "blockLength")? {
        Some(v) if u32::from(v) < cursor => {
            return Err(Error::Schema(format!(
                "blockLength {v} is shorter than the fields, which end at {cursor} (RC4 \
                 <message> blockLength: \"must be greater than or equal to the sum of field \
                 lengths\")"
            )));
        }
        Some(v) => v,
        None => {
            u16::try_from(cursor).map_err(|_| Error::Unsupported("block length exceeds u16"))?
        }
    };
    Ok((fields, groups, var_data, block_length))
}

fn parse_group(node: Node<'_, '_>, types: &Types<'_, '_>) -> Result<GroupDef, Error> {
    let id = required_u16_attr(node, "id")?;
    let name = required_attr(node, "name")?.to_string();
    let since_version = optional_u16_attr(node, "sinceVersion")?.unwrap_or(0);
    let dimension_type = node
        .attribute("dimensionType")
        .unwrap_or("groupSizeEncoding");
    let dimension = resolve_dimension(dimension_type, types)?;
    let (fields, groups, var_data, block_length) = layout_block(node, types)?;
    Ok(GroupDef {
        id,
        name,
        block_length,
        since_version,
        dimension,
        fields,
        groups,
        var_data,
    })
}

fn parse_message(node: Node<'_, '_>, types: &Types<'_, '_>) -> Result<MessageDef, Error> {
    let template_id = required_u16_attr(node, "id")?;
    let name = required_attr(node, "name")?.to_string();
    let since_version = optional_u16_attr(node, "sinceVersion")?.unwrap_or(0);
    let (fields, groups, var_data, block_length) = layout_block(node, types)?;
    Ok(MessageDef {
        template_id,
        name,
        block_length,
        since_version,
        fields,
        groups,
        var_data,
    })
}

/// The standard 8-byte header (`crate::HEADER_LEN` in `fixbolt_sbe`):
/// `blockLength`, `templateId`, `schemaId`, `version`, each `uint16`, in
/// that order and no others. `schema.rs`'s contract rejects any other
/// `messageHeader` — this is where that rejection happens.
fn validate_header(types: &Types<'_, '_>) -> Result<(), Error> {
    let node = types
        .get("messageHeader")
        .ok_or(Error::Unsupported("schema declares no messageHeader"))?;
    if local_name(node) != "composite" {
        return Err(Error::Unsupported("messageHeader that is not a composite"));
    }
    let members: Vec<Node<'_, '_>> = is_element_iter(node).collect();
    let names: Vec<Option<&str>> = members.iter().map(|n| n.attribute("name")).collect();
    let want = [
        Some("blockLength"),
        Some("templateId"),
        Some("schemaId"),
        Some("version"),
    ];
    if names.as_slice() != want.as_slice() {
        return Err(Error::Unsupported(
            "messageHeader other than the recommended blockLength/templateId/schemaId/version",
        ));
    }
    for member in &members {
        if member.attribute("primitiveType") != Some("uint16") {
            return Err(Error::Unsupported(
                "messageHeader member that is not uint16",
            ));
        }
    }
    Ok(())
}

// --------------------------------------------------------------------------
// Naming and emission
// --------------------------------------------------------------------------

/// The generated `Schema` unit struct's name: `PascalCase` of the schema's
/// `package` attribute, or `Schema<id>` when `package` is absent or has no
/// alphanumeric character at all. `package` is split on every run of
/// non-alphanumeric characters, and each run's first character is
/// upper-cased; nothing else about the run's letters is touched, so
/// `"myPackage"` becomes `"MyPackage"` and `"fix.sbe_v1"` becomes
/// `"FixSbeV1"`. This is the whole rule — there is no per-schema
/// configuration of it.
fn schema_struct_name(package: &str, schema_id: u16) -> String {
    let mut name = String::new();
    for part in package.split(|c: char| !c.is_ascii_alphanumeric()) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            name.extend(first.to_uppercase());
            name.push_str(chars.as_str());
        }
    }
    if name.is_empty() || name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("Schema{schema_id}")
    } else {
        name
    }
}

fn quote(s: &str) -> String {
    format!("{s:?}")
}

fn byte_string_literal(bytes: &[u8]) -> String {
    let mut out = String::from("b\"");
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(b as char),
            _ => {
                let _ = write!(out, "\\x{b:02x}");
            }
        }
    }
    out.push('"');
    out
}

fn value_literal(value: &Value) -> String {
    match value {
        Value::Char(c) => format!("Value::Char({c})"),
        Value::Int(i) => format!("Value::Int({i})"),
        Value::UInt(u) => format!("Value::UInt({u})"),
        Value::Float(f) if f.is_nan() => "Value::Float(f64::NAN)".to_string(),
        Value::Float(f) => format!("Value::Float({f:?}f64)"),
        Value::Bytes(b) => format!("Value::Array({})", byte_string_literal(b)),
    }
}

fn presence_literal(presence: &Presence) -> String {
    match presence {
        Presence::Required => "Presence::Required".to_string(),
        Presence::Optional { null } => {
            format!("Presence::Optional {{ null: {} }}", value_literal(null))
        }
        Presence::Constant(value) => format!("Presence::Constant({})", value_literal(value)),
    }
}

fn emit_element(el: &Element) -> String {
    format!(
        "Element {{ name: {}, offset: {}, primitive: Primitive::{}, length: {}, presence: {} }}",
        quote(&el.path),
        el.offset,
        el.primitive.rust_name(),
        el.length,
        presence_literal(&el.presence)
    )
}

fn emit_field(f: &FieldDef) -> String {
    let elements = f
        .elements
        .iter()
        .map(emit_element)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "FieldLayout {{ id: {}, name: {}, offset: {}, len: {}, since_version: {}, elements: &[{elements}] }}",
        f.id,
        quote(&f.name),
        f.offset,
        f.len,
        f.since_version
    )
}

fn emit_dimension(d: &Dimension) -> String {
    format!(
        "DimensionLayout {{ block_length: LengthType::{}, num_in_group: LengthType::{} }}",
        d.block_length.rust_name(),
        d.num_in_group.rust_name()
    )
}

fn emit_var_data(v: &VarDataDef) -> String {
    format!(
        "VarDataLayout {{ id: {}, name: {}, length: LengthType::{}, since_version: {} }}",
        v.id,
        quote(&v.name),
        v.length.rust_name(),
        v.since_version
    )
}

fn emit_group(g: &GroupDef) -> String {
    let fields = g
        .fields
        .iter()
        .map(emit_field)
        .collect::<Vec<_>>()
        .join(", ");
    let groups = g
        .groups
        .iter()
        .map(emit_group)
        .collect::<Vec<_>>()
        .join(", ");
    let var_data = g
        .var_data
        .iter()
        .map(emit_var_data)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "GroupLayout {{ id: {}, name: {}, block_length: {}, since_version: {}, dimension: {}, fields: &[{fields}], groups: &[{groups}], var_data: &[{var_data}] }}",
        g.id,
        quote(&g.name),
        g.block_length,
        g.since_version,
        emit_dimension(&g.dimension)
    )
}

fn emit_message(m: &MessageDef) -> String {
    let fields = m
        .fields
        .iter()
        .map(emit_field)
        .collect::<Vec<_>>()
        .join(", ");
    let groups = m
        .groups
        .iter()
        .map(emit_group)
        .collect::<Vec<_>>()
        .join(", ");
    let var_data = m
        .var_data
        .iter()
        .map(emit_var_data)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "MessageLayout {{ template_id: {}, name: {}, block_length: {}, since_version: {}, fields: &[{fields}], groups: &[{groups}], var_data: &[{var_data}] }}",
        m.template_id,
        quote(&m.name),
        m.block_length,
        m.since_version
    )
}

fn generate_impl(root: Node<'_, '_>, included: &[Document<'_>]) -> Result<String, Error> {
    let schema_id = required_u16_attr(root, "id")?;
    let version = optional_u16_attr(root, "version")?.unwrap_or(0);
    // RC4 `<messageSchema>` `byteOrder`: default littleEndian; the values
    // are exactly littleEndian and bigEndian.
    let byte_order = match root.attribute("byteOrder") {
        None | Some("littleEndian") => "Little",
        Some("bigEndian") => "Big",
        Some(other) => {
            return Err(Error::Schema(format!(
                "byteOrder '{other}' is neither littleEndian nor bigEndian"
            )));
        }
    };
    let package = root.attribute("package").unwrap_or("");
    let struct_name = schema_struct_name(package, schema_id);

    let mut type_nodes = Vec::new();
    for types_el in root
        .children()
        .filter(|n| n.is_element() && local_name(*n) == "types")
    {
        collect_type_defs(types_el, &mut type_nodes);
    }
    for doc in included {
        collect_type_defs(doc.root_element(), &mut type_nodes);
    }
    let types = build_types(type_nodes)?;
    validate_header(&types)?;

    let mut messages = Vec::new();
    for m in root
        .children()
        .filter(|n| n.is_element() && local_name(*n) == "message")
    {
        messages.push(parse_message(m, &types)?);
    }

    let mut out = String::with_capacity(4 * 1024);
    let _ = writeln!(
        out,
        "// @generated by fixbolt-sbe-gen from an SBE 1.0 XML schema."
    );
    let _ = writeln!(
        out,
        "// Do not edit. Regenerate by touching the schema or the generator."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "use fixbolt_sbe::{{ByteOrder, DimensionLayout, Element, FieldLayout, GroupLayout, LengthType, MessageLayout, Presence, Primitive, Schema, Value, VarDataLayout}};"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "/// `<messageSchema package={package:?} id={schema_id}>`."
    );
    let _ = writeln!(out, "pub struct {struct_name};");
    let _ = writeln!(out, "impl Schema for {struct_name} {{");
    let _ = writeln!(out, "    const ID: u16 = {schema_id};");
    let _ = writeln!(out, "    const VERSION: u16 = {version};");
    let _ = writeln!(
        out,
        "    const BYTE_ORDER: ByteOrder = ByteOrder::{byte_order};"
    );
    let _ = writeln!(
        out,
        "    fn message(template_id: u16) -> Option<&'static MessageLayout> {{"
    );
    let _ = writeln!(out, "        match template_id {{");
    for m in &messages {
        let _ = writeln!(
            out,
            "            {} => Some(&{}),",
            m.template_id,
            emit_message(m)
        );
    }
    let _ = writeln!(out, "            _ => None,");
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = r#"<composite name="messageHeader">
        <type name="blockLength" primitiveType="uint16"/>
        <type name="templateId" primitiveType="uint16"/>
        <type name="schemaId" primitiveType="uint16"/>
        <type name="version" primitiveType="uint16"/>
    </composite>"#;

    /// The recommended `groupSizeEncoding`, needed by any test schema that
    /// declares a `<group>` without spelling out its own `dimensionType`.
    const GROUP_SIZE: &str = r#"<composite name="groupSizeEncoding">
        <type name="blockLength" primitiveType="uint16"/>
        <type name="numInGroup" primitiveType="uint16"/>
    </composite>"#;

    /// A minimal, otherwise-valid schema: the recommended header and
    /// `groupSizeEncoding`, plus whatever `extra_types`/`messages` a test
    /// wants to add.
    fn schema_xml(extra_types: &str, messages: &str) -> String {
        format!(
            r#"<messageSchema id="1" version="0" byteOrder="littleEndian" package="p">
                <types>{HEADER}{GROUP_SIZE}{extra_types}</types>
                {messages}
            </messageSchema>"#
        )
    }

    #[test]
    fn schema_struct_name_is_pascal_case_of_package() {
        assert_eq!(schema_struct_name("Examples", 91), "Examples");
        assert_eq!(schema_struct_name("baseline", 1), "Baseline");
        assert_eq!(schema_struct_name("fix.sbe_v1", 1), "FixSbeV1");
        assert_eq!(schema_struct_name("", 7), "Schema7");
        assert_eq!(schema_struct_name("1x", 8), "Schema8");
    }

    #[test]
    fn generate_refuses_a_schema_with_xi_include() {
        let xml = format!(
            r#"<messageSchema xmlns:xi="http://www.w3.org/2001/XInclude" id="1" version="0" package="p">
                <xi:include href="common.xml"/>
                <types>{HEADER}</types>
            </messageSchema>"#
        );
        let result = generate(&xml);
        assert!(matches!(result, Err(Error::Unsupported("xi:include"))));
    }

    #[test]
    fn generate_with_includes_merges_the_included_types() -> Result<(), Error> {
        let xml = r#"<messageSchema xmlns:xi="http://www.w3.org/2001/XInclude" id="1" version="0" package="p">
            <xi:include href="common.xml"/>
            <types><composite name="unused"><type name="x" primitiveType="uint8"/></composite></types>
            <message id="1" name="M"><field id="1" name="f" type="uint32"/></message>
        </messageSchema>"#;
        let included = format!("<types>{HEADER}</types>");
        let generated =
            generate_with_includes(xml, |href| (href == "common.xml").then(|| included.clone()))?;
        assert!(generated.contains("struct P;"));
        assert!(generated.contains("FieldLayout"));
        Ok(())
    }

    #[test]
    fn a_ref_nested_two_levels_deep_is_unsupported() {
        let xml = schema_xml(
            r#"<composite name="C"><type name="v" primitiveType="uint8"/></composite>
               <composite name="B"><ref name="c" type="C"/></composite>
               <composite name="A"><ref name="b" type="B"/></composite>"#,
            r#"<message id="1" name="M"><field id="1" name="f" type="A"/></message>"#,
        );
        let result = generate(&xml);
        assert!(matches!(result, Err(Error::Unsupported(_))));
    }

    #[test]
    fn presence_on_a_composite_typed_field_is_unsupported() {
        let xml = schema_xml(
            r#"<composite name="A"><type name="v" primitiveType="uint8"/></composite>"#,
            r#"<message id="1" name="M"><field id="1" name="f" type="A" presence="constant">1</field></message>"#,
        );
        let result = generate(&xml);
        assert!(matches!(result, Err(Error::Unsupported(_))));
    }

    #[test]
    fn a_missing_message_header_is_unsupported() {
        let xml = r#"<messageSchema id="1" version="0" package="p">
            <types><type name="dummy" primitiveType="uint8"/></types>
            <message id="1" name="M"><field id="1" name="f" type="uint8"/></message>
        </messageSchema>"#;
        let result = generate(xml);
        assert!(matches!(result, Err(Error::Unsupported(_))));
    }

    #[test]
    fn a_non_recommended_message_header_is_unsupported() {
        let xml = r#"<messageSchema id="1" version="0" package="p">
            <types><composite name="messageHeader">
                <type name="blockLength" primitiveType="uint32"/>
                <type name="templateId" primitiveType="uint16"/>
                <type name="schemaId" primitiveType="uint16"/>
                <type name="version" primitiveType="uint16"/>
            </composite></types>
            <message id="1" name="M"><field id="1" name="f" type="uint8"/></message>
        </messageSchema>"#;
        let result = generate(xml);
        assert!(matches!(result, Err(Error::Unsupported(_))));
    }

    #[test]
    fn an_unknown_primitive_type_is_unsupported() {
        let xml = schema_xml(
            r#"<type name="huge" primitiveType="int128"/>"#,
            r#"<message id="1" name="M"><field id="1" name="f" type="huge"/></message>"#,
        );
        let result = generate(&xml);
        assert!(matches!(result, Err(Error::Unsupported(_))));
    }

    #[test]
    fn a_field_after_a_group_is_unsupported() {
        let xml = schema_xml(
            "",
            r#"<message id="1" name="M">
                <group name="g" id="1" dimensionType="groupSizeEncoding">
                    <field id="2" name="gf" type="uint8"/>
                </group>
                <field id="3" name="late" type="uint8"/>
            </message>"#,
        );
        let result = generate(&xml);
        assert!(matches!(result, Err(Error::Unsupported(_))));
    }

    /// A composite member's `offset` pads the composite (RC4 §4.4.4.3,
    /// `04MessageSchema.md` "Element offset within a composite type"): the
    /// field is as long as the composite, padding included, and the next
    /// field starts after it.
    #[test]
    fn a_padded_composite_field_is_as_long_as_the_composite() -> Result<(), Error> {
        let xml = schema_xml(
            r#"<composite name="Padded">
                   <type name="x" primitiveType="uint8"/>
                   <type name="y" primitiveType="uint32" offset="4"/>
               </composite>"#,
            r#"<message id="1" name="M">
                   <field id="1" name="p" type="Padded"/>
                   <field id="2" name="q" type="uint32"/>
               </message>"#,
        );
        let generated = generate(&xml)?;
        assert!(
            generated.contains(r#"name: "p", offset: 0, len: 8,"#),
            "{generated}"
        );
        assert!(
            generated.contains(r#"name: "q", offset: 8, len: 4,"#),
            "{generated}"
        );
        assert!(generated.contains("block_length: 12,"), "{generated}");
        Ok(())
    }

    /// `03MessageStructure.md`, "Repeating group after variable-length field".
    #[test]
    fn a_group_after_data_is_a_schema_error() {
        let xml = schema_xml(
            r#"<composite name="varStr">
                   <type name="length" primitiveType="uint16"/>
                   <type name="varData" primitiveType="uint8" length="0"/>
               </composite>"#,
            r#"<message id="1" name="M">
                   <data name="d" id="5" type="varStr"/>
                   <group name="g" id="10"><field name="z" id="12" type="uint8"/></group>
               </message>"#,
        );
        let result = generate(&xml);
        assert!(
            matches!(&result, Err(Error::Schema(msg)) if msg.contains("group after variable-length")),
            "{result:?}"
        );
    }

    /// `04MessageSchema.md` `<message>` `blockLength`: "must be greater than
    /// or equal to the sum of field lengths".
    #[test]
    fn a_block_length_shorter_than_its_fields_is_a_schema_error() {
        let xml = schema_xml(
            "",
            r#"<message id="1" name="M" blockLength="2"><field id="1" name="w" type="uint32"/></message>"#,
        );
        let result = generate(&xml);
        assert!(
            matches!(&result, Err(Error::Schema(msg)) if msg.contains("blockLength")),
            "{result:?}"
        );
    }

    /// `03MessageStructure.md` "Field offset specified by message schema":
    /// "an offset is invalid if it would cause elements to overlap".
    #[test]
    fn a_field_offset_overlapping_an_earlier_field_is_a_schema_error() {
        let xml = schema_xml(
            "",
            r#"<message id="1" name="M">
                   <field id="1" name="a" type="uint32"/>
                   <field id="2" name="b" type="uint8" offset="2"/>
               </message>"#,
        );
        let result = generate(&xml);
        assert!(
            matches!(&result, Err(Error::Schema(msg)) if msg.contains("overlap")),
            "{result:?}"
        );
    }

    /// `04MessageSchema.md` "Element offset within a composite type": the same
    /// rule for a composite's members.
    #[test]
    fn a_composite_member_offset_overlapping_an_earlier_member_is_a_schema_error() {
        let xml = schema_xml(
            r#"<composite name="C">
                   <type name="a" primitiveType="uint32"/>
                   <type name="b" primitiveType="uint8" offset="1"/>
               </composite>"#,
            r#"<message id="1" name="M"><field id="1" name="c" type="C"/></message>"#,
        );
        let result = generate(&xml);
        assert!(
            matches!(&result, Err(Error::Schema(msg)) if msg.contains("overlap")),
            "{result:?}"
        );
    }

    /// Outside ADR-0081 decision 5: an optional array is null-checked only
    /// as a char array.
    #[test]
    fn an_optional_non_char_array_is_unsupported() {
        let xml = schema_xml(
            r#"<type name="Arr" primitiveType="uint8" length="4" presence="optional"/>"#,
            r#"<message id="1" name="M"><field id="1" name="arr" type="Arr"/></message>"#,
        );
        let result = generate(&xml);
        assert!(
            matches!(&result, Err(Error::Unsupported(what)) if what.contains("optional")),
            "{result:?}"
        );
    }

    #[test]
    fn an_optional_char_array_is_still_generated() -> Result<(), Error> {
        let xml = schema_xml(
            r#"<type name="Str" primitiveType="char" length="4" presence="optional"/>"#,
            r#"<message id="1" name="M"><field id="1" name="s" type="Str"/></message>"#,
        );
        let generated = generate(&xml)?;
        assert!(generated.contains("Presence::Optional"), "{generated}");
        Ok(())
    }

    #[test]
    fn a_composite_member_with_since_version_is_unsupported() {
        let xml = schema_xml(
            r#"<composite name="Cv">
                   <type name="a" primitiveType="uint8"/>
                   <type name="b" primitiveType="uint8" sinceVersion="1"/>
               </composite>"#,
            r#"<message id="1" name="M"><field id="1" name="c" type="Cv"/></message>"#,
        );
        let result = generate(&xml);
        assert!(
            matches!(&result, Err(Error::Unsupported(what)) if what.contains("sinceVersion")),
            "{result:?}"
        );
    }

    /// `04MessageSchema.md` `<messageSchema>` `byteOrder`: default
    /// `littleEndian`; the two values are `littleEndian` and `bigEndian`.
    #[test]
    fn byte_order_defaults_to_little_and_accepts_only_the_two_spec_values() {
        let with = |attr: &str| {
            generate(&format!(
                r#"<messageSchema id="1" version="0" {attr} package="p">
                       <types>{HEADER}</types>
                   </messageSchema>"#
            ))
        };
        let little = "ByteOrder::Little;";
        let big = "ByteOrder::Big;";
        assert!(matches!(&with(""), Ok(s) if s.contains(little)));
        assert!(matches!(&with(r#"byteOrder="littleEndian""#), Ok(s) if s.contains(little)));
        assert!(matches!(&with(r#"byteOrder="bigEndian""#), Ok(s) if s.contains(big)));
        for bad in ["BigEndian", "big", "LittleEndian", ""] {
            let result = with(&format!(r#"byteOrder="{bad}""#));
            assert!(
                matches!(&result, Err(Error::Schema(msg)) if msg.contains("byteOrder")),
                "byteOrder={bad:?}: {result:?}"
            );
        }
    }

    #[test]
    fn a_flat_message_generates_a_field_table() -> Result<(), Error> {
        let xml = schema_xml(
            "",
            r#"<message id="7" name="M"><field id="1" name="f" type="uint32"/></message>"#,
        );
        let generated = generate(&xml)?;
        assert!(generated.contains("struct P;"));
        assert!(generated.contains("template_id: 7"));
        assert!(generated.contains(r#"name: "f""#));
        assert!(generated.contains("Primitive::UInt32"));
        Ok(())
    }
}
