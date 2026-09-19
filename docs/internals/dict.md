# `dict` — internals

Build-layer crate in [DESIGN.md §3](../DESIGN.md#3-crates): code generation from the FIX 4.4
XML — tag constants, message shapes, required-field tables, field ordering, group members,
and the validation tables. It implements `codec::Dictionary`
([ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md) on why the XML is fetched
data, never copied source).

## Files, and what each keeps

| File | Keeps |
|---|---|
| `build.rs` | The generator itself: reads `vendor/quickfix/spec/FIX44.xml` at build time and emits every table below. Nothing under `src/` is hand-written from it |
| `src/lib.rs` | The generated crate root — tag constants, message shapes, `Fix44`, the header/trailer and per-message tag sets, field ordering, group delimiters and members |
| `src/field_type.rs` | The 23 FIX 4.4 field types and what each accepts on the wire — hand-written, because the *format* a type accepts is not stated in the XML at all |

## Read in this order

1. `build.rs` — what is generated and from what source
2. `src/lib.rs` — the shape of the generated output (read a generated table, not the
   generator's emit code, to see what a caller actually gets)
3. `src/field_type.rs` — the one hand-written table, and why it is separate from generation

## Tests that guard it

- `tests/tables.rs`, `tests/group_tables.rs` — the generated tables themselves
- `tests/field_types.rs`, `tests/enums.rs` — the hand-written type table and enum values
- `tests/parse_with_real_dictionary.rs` — `codec::Dictionary` implemented correctly
- `tests/interop_quickfix_fields.rs`, `tests/interop_quickfix_messages.rs`,
  `tests/interop_quickfix_order.rs` — generated field/message/order tables checked against
  QuickFIX's own generated C++, per [DESIGN.md D3](../DESIGN.md)
