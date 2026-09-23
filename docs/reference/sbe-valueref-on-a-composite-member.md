# `valueRef` on a composite member `<type>`: the xsd allows it, the prose table leaves it out

`[measured 2026-09-23, desk tmt-B450-I-AORUS-PRO-WIFI, phase 3 row 9 step 2]`

## What happened

A scratch crate outside the repository called `fixbolt_sbe_gen::generate` at commit `2f0a0dc`
on the Binary EntryPoint schema inside `artio-binary-entrypoint-codecs-0.184.jar`
(`binary_entrypoint.xml`, SHA-256 `c31fcd62…4a71`, fetched into gitignored `vendor/fixp/`). It
failed:

```text
binary_entrypoint.xml: ERR schema error: constant '' is not an unsigned integer: cannot parse integer from empty string
```

The schema's timestamp composites end in a constant member that names its value through an
enum instead of carrying it as text:

```xml
<type name="unit" primitiveType="uint8" presence="constant" valueRef="TimeUnit.NANOSECOND" .../>
```

`sbe-gen` read `valueRef` on a `<field>` only. On a `<type>` it ignored the attribute, took
the element's empty text as the constant, and reported the empty string, not the attribute it
had not read. **The error named a symptom two steps from its cause.**

## What the SBE 1.0 Standard says

Read at FIXTradingCommunity/fix-simple-binary-encoding `418a8f6a8b93c65b308638dab2bc6a35dddcd864`
(`scripts/fetch-sbe-assets.sh`), `v1-0-STANDARD/`:

| Source | Says about `valueRef` on `<type>` |
|---|---|
| `doc/04MessageSchema.md:142-155`, the prose `<type>` attribute table | not listed (it lists `presence`, not `valueRef`) |
| `doc/04MessageSchema.md:464`, the prose **field** attribute table | listed, "valid only if presence=constant" |
| `resources/sbe.xsd:177` → `:342-375` | **allowed**: `encodedDataType` (the `<type>` element) takes `presenceAttributes`, which declares `valueRef` at `:368`, annotated "Deprecated - only for back compatibility with RC2" |
| `doc/02FieldEncoding.md:884`, `:902`, `:921`, `:936` | the Standard's own timestamp examples put it on a composite member `<type>` |

`xmllint --schema v1-0-STANDARD/resources/sbe.xsd` on this repository's fixture (below, with
the `sbe:` namespace added to `messageSchema` and `message`) prints `validates`; the same file
with an invented `bogusAttr="1"` added to that `<type>` prints `attribute 'bogusAttr': The
attribute 'bogusAttr' is not allowed` — so the validator does check `<type>` attributes, and
`valueRef` passes that check.

**So the xsd and the examples agree, and only the prose table leaves it out.** The plan
(`docs/plans/2026-09-23-p3-fixp-spike.md`, *Những gì đã biết chắc*) and ADR-0140's *Context*
row and *Consequences* said the xsd declares `valueRef` on `fieldType` only; the xsd
measurement above says otherwise. The decision does not change — it was taken to follow the
examples and `sbe-tool` over the narrower reading, and the xsd turns out to be on the same
side.

Real Logic's `sbe-tool` (`sbe-tool/src/main/java/uk/co/real_logic/sbe/xml/EncodedDataType.java`,
read on `master` of <https://github.com/aeron-io/simple-binary-encoding>, 2026-09-23) reads
`valueRef` on any `<type>` and refuses three things: a `valueRef` whose `presence` is not
`constant` ("presence must be constant when valueRef is set"), an enum whose `encodingType` is
not the `<type>`'s `primitiveType` ("valueRef does not match this type"), and a missing enum or
`validValue`.

## Rule

- **`sbe-gen` reads `valueRef` on a `<type>`** — inside a composite or standing alone — and
  resolves it to the named `<validValue>`, as it already did on a `<field>`. The constant
  contributes 0 wire bytes, like any other constant.
- **It refuses what `sbe-tool` refuses**, each with an error naming `valueRef`: `presence` not
  `constant`; the enum's encoding not the `<type>`'s `primitiveType`; no such enum; no such
  `validValue`. ADR-0140 decision 4's "one change" is the `valueRef` resolution, and refusing
  what `sbe-tool` refuses is part of doing it correctly.
- The field-level path is unchanged except for its error text, which now names
  `valueRef '<ref>'` too; it does not check the enum's encoding against the field's type.

## Tests that guard it

`crates/sbe-gen/tests/value_ref_on_composite_member.rs`, over
`crates/sbe-gen/tests/fixtures/value_ref_on_composite_member.xml` — a schema written for this
repository: an enum `TimeUnit` (`uint8`), two composites whose constant `unit` names two
different values (nanosecond = 9, millisecond = 3), and a `uint32` right behind both.

| Test | Asserts |
|---|---|
| `a_value_ref_constant_in_a_composite_takes_the_named_valid_value_and_zero_wire_bytes` | the generated `FieldLayout`s carry `Presence::Constant(Value::UInt(9))` and `(3)`, each timestamp is 8 bytes, the next field sits at offset 16, the block is 20 |
| `a_value_ref_to_an_enum_that_does_not_exist_is_refused_naming_value_ref` | the error names `valueRef` and the enum |
| `a_value_ref_to_a_valid_value_that_does_not_exist_is_refused_naming_value_ref` | the error names `valueRef` and the value |
| `a_value_ref_whose_enum_encoding_differs_from_the_member_type_is_refused` | `primitiveType="uint16"` against a `uint8` enum is refused |
| `a_value_ref_on_a_member_that_is_not_constant_is_refused` | `valueRef` without `presence="constant"` is refused |
| `a_value_ref_constant_on_a_standalone_type_takes_the_named_valid_value_and_zero_wire_bytes` | a top-level `<type name="Unit" … presence="constant" valueRef="TimeUnit.microsecond"/>`, used by a field that says nothing about presence, is `Presence::Constant(Value::UInt(6))` with `len: 0`, and the block stays 20 — the "standing alone" half of the rule above (added by the review of PR #102, which asked for that half to be tested or dropped) |

Reversals, each run and restored: resolving no `valueRef` on a `<type>` turns the first test red
on `generate failed: schema error: constant '' is not an unsigned integer: cannot parse integer
from empty string` (the three error tests go red too, on `error does not name valueRef`);
removing the encoding check, or the presence check, turns its own test red on `generate
accepted a schema it must refuse`. For the standalone test, `[measured 2026-09-24]` resolving no
`valueRef` in `resolve_presence` turned it red on `generate failed: schema error: constant '' is
not an unsigned integer: cannot parse integer from empty string`.

On the real schema, the same scratch crate after the change:

```text
binary_entrypoint.xml: OK, 121134 bytes generated
```

and its output is byte-identical to the output for the same schema with the three `valueRef`s
replaced by hand with the literal constants `9`, `3`, `9`. The tables generated from the RC4
examples, Real Logic's `Car` and the padded fixture (`$OUT_DIR/examples_rc4.rs`, `car.rs`,
`padded.rs`) hash the same before and after.

## Not covered

**Open, no test: a field's own `presence` silently overrides a constant `<type>`.** Found by the
review of PR #102 and `[measured 2026-09-24]` reproduced with a throwaway test (not kept): the
fixture above plus `<type name="Unit" primitiveType="uint8" presence="constant"
valueRef="TimeUnit.microsecond"/>` and `<field id="4" name="unit" type="Unit"
presence="required"/>` generates `FieldLayout { id: 4, name: "unit", offset: 20, len: 1, … presence:
Presence::Required }` and `block_length: 21` — the type's constant and its `valueRef` are never
looked at, because `field_presence_override` (`crates/sbe-gen/src/generator.rs`) answers from the
field's attribute alone. So a schema that declares a constant type and a required field of it gets
one wire byte nobody on the other end expects, with no error. What SBE 1.0 and `sbe-tool` do with
this combination has not been checked; nothing in the Artio 5.6 schema hits it (its constant
fields say `presence="constant"` themselves). Open item for `STATUS.md`; the fix, and its test,
belong to whichever plan next touches `sbe-gen`.

`presence` on a field whose type is a composite — the second gap B3's current schema 8.4.2
hits — is still `Error::Unsupported`; ADR-0140 decision 4 leaves it to the FIXP ADR.
