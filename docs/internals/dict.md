# `dict` — internals

Build-layer crate in [DESIGN.md §3](../DESIGN.md#3-crates): code generation from the FIX 4.4
XML — tag constants, message shapes, required-field tables, field ordering, group members,
and the validation tables. It implements `codec::Dictionary`
([ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md) on why the XML is fetched
data, never copied source). Behind the `fix50sp2` feature, `build.rs` also emits the FIXT 1.1 /
FIX 5.0 SP2 pair; `fix50sp2` adds no dependency (`roxmltree` is already unconditional)
([ADR-0080](../decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)).

## Files, and what each keeps

| File | Keeps |
|---|---|
| `build.rs` | The generator itself: reads `vendor/quickfix/spec/FIX44.xml` at build time and emits every table below; behind `fix50sp2`, also generates `$OUT_DIR/fixt11_fix50sp2.rs` from the transport/app XML pair. Nothing under `src/` is hand-written from it |
| `src/lib.rs` | The generated crate root — tag constants, message shapes, `Fix44`, the header/trailer and per-message tag sets, field ordering, group delimiters and members; behind `fix50sp2`, the `fixt11_fix50sp2` module and `Fixt11Fix50Sp2Tables` (`codec::Dictionary` and `Tables` impls) |
| `src/field_type.rs` | The 23 FIX 4.4 field types and what each accepts on the wire — hand-written, because the *format* a type accepts is not stated in the XML at all |
| `src/tables.rs` | `Tables` — the dictionary questions the **session layer** asks (as opposed to `codec::Dictionary`'s parser questions); `Session` adds `where E::Dict: Tables` (ADR-0080 decision 1: the dictionary rides the encoding) |

## Read in this order

1. `build.rs` — what is generated and from what source
2. `src/lib.rs` — the shape of the generated output (read a generated table, not the
   generator's emit code, to see what a caller actually gets)
3. `src/field_type.rs` — the one hand-written table, and why it is separate from generation
4. `src/tables.rs` — the session-facing trait, read last since it is asked only after
   `codec::Dictionary` already answered

## Tests that guard it

- `tests/tables.rs`, `tests/group_tables.rs` — the generated tables themselves
- `tests/field_types.rs`, `tests/enums.rs` — the hand-written type table and enum values
- `tests/parse_with_real_dictionary.rs` — `codec::Dictionary` implemented correctly
- `tests/interop_quickfix_fields.rs`, `tests/interop_quickfix_messages.rs`,
  `tests/interop_quickfix_order.rs` — generated field/message/order tables checked against
  QuickFIX's own generated C++, per [DESIGN.md D3](../DESIGN.md)
- `tests/fixt.rs`, `tests/fixt_order.rs` — the `fix50sp2` FIXT 1.1 / FIX 5.0 SP2 tables and
  `Fixt11Fix50Sp2Tables`
