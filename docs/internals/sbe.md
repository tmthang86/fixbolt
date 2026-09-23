# `sbe` and `sbe-gen` — internals

Layer L1 in [DESIGN.md §3](../DESIGN.md#3-crates): a second wire encoding, SBE 1.0, that
`codec::Encoding` (ADR-0082) lets the session be generic over without either crate knowing about
FIX tag=value. `sbe` is the runtime reader/writer over `&'static` layout tables; `sbe-gen` is the
build-time generator that turns an SBE XML schema into those tables — the same split `dict` has
for FIX 4.4 ([ADR-0081](../decisions/ADR-0081-sbe-tables-come-from-this-repositorys-generator-and-the-oracle-is-the-specs-bytes-plus-sbe-tool-behind-a-script.md)).
No session: [ADR-0078](../decisions/ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)
and `SbeTables<S>` implements `codec::Dictionary` only, never `dict::Tables`
([ADR-0082](../decisions/ADR-0082-the-session-is-generic-over-tag-value-encodings-and-the-boundary-to-sbe-is-the-session-not-the-trait.md)).
Spec facts and known RC4 discrepancies: [docs/reference/sbe-spec-facts.md](../reference/sbe-spec-facts.md),
[docs/reference/the-sbe-rc4-example-dumps-disagree-with-their-own-tables.md](../reference/the-sbe-rc4-example-dumps-disagree-with-their-own-tables.md).

## `crates/sbe` — files, and what each keeps

| File | Keeps |
|---|---|
| `lib.rs` | Crate root; the reading order as a doctest-shaped comment: `SbeView::decode` → `layout` → `root`/`value` → `tail` → `group`/`var_data` |
| `wire.rs` | Bounds-checked primitive reads/writes in either byte order — no `a[i..j]` indexing (CLAUDE.md §2 rule 7) |
| `error.rs` | `SbeError`, the one fieldless error for both parse and encode |
| `header.rs` | The 8-byte SBE message header (the only `messageHeader` composite this crate supports) |
| `schema.rs` | The `&'static` layout tables (`FieldLayout`, `MessageLayout`, `GroupLayout`, `VarDataLayout`, …) and the `Schema` trait `sbe-gen` emits an impl of — the contract between the two crates |
| `view.rs` | `SbeView` (24 bytes, `Copy`) and `Block`, the fixed block fields are read from |
| `group.rs` | `Cursor`, `Group` — walking groups and nested groups after a fixed block, depth first |
| `vardata.rs` | Reading `varData`: a length prefix, then that many bytes |
| `encode.rs` | `MessageWriter`/`GroupWriter`/`EntryWriter` — the native, closure-scoped writer that fills groups and `varData` table-driven, never allocating |
| `encoding.rs` | `Sbe<S>`: `codec::Encoding` impl for schema `S` (ADR-0082's associated-type table) |
| `tables.rs` | `SbeTables<S>`, the `Dict` of `Sbe<S>` — `codec::Dictionary` only, never `dict::Tables` |
| `tests.rs` | Unit tests: the RC4 §7 dumps against hand-written tables (`mod tests` in `lib.rs`) |

## Read in this order

1. `lib.rs` — the reading order the crate itself states
2. `wire.rs`, `error.rs` — the two primitives everything else builds on
3. `header.rs` — the fixed 8 bytes every message starts with
4. `schema.rs` — the table shapes, i.e. what a schema compiles to
5. `view.rs` — `SbeView`/`Block`, the shape reads happen through
6. `group.rs`, `vardata.rs` — walking past the fixed block
7. `encode.rs` — the reverse direction, table-driven
8. `encoding.rs`, `tables.rs` — the `codec::Encoding` boundary, last, since it composes 2–7

## `crates/sbe-gen` — files, and what each keeps

| File | Keeps |
|---|---|
| `generator.rs` | The XML → tables generator itself (ADR-0081 decision 2); loaded twice — once as a normal module, once by `build.rs` via `#[path]`, so there is one writer of the tables `sbe` reads. Scope (ADR-0081 decision 5) and its `Error::Unsupported(name)` refusal are stated at the top. `valueRef` resolves on a `<field>` (`field_presence_override`) and on a `<type>`, standing alone or inside a composite (`resolve_presence`), both through `find_value_ref` — the `<type>` case with `sbe-tool`'s two extra refusals ([sbe-valueref-on-a-composite-member](../reference/sbe-valueref-on-a-composite-member.md)) |
| `lib.rs` | The public entry points, `generate(xml)` and `generate_with_includes(xml, resolve)`, for a caller's own `build.rs` (this repository's own use is `crates/dict/build.rs`'s FIX 4.4 shape, mirrored for SBE) |

## Read in this order

1. `lib.rs` — the two entry points and the worked example in its doctest
2. `generator.rs` — the generator, read by its scope comment before its code

## Tests that guard it

- `crates/sbe/src/tests.rs` (unit) — RC4 §7 dumps against hand-written tables (step C1)
- `crates/sbe/tests/encoding.rs` — `Sbe<S>: codec::Encoding`, writer refusals, hand-written tables (step C4)
- `crates/sbe-gen/tests/spec_examples.rs` — the same RC4 dumps through *generated* tables (step C3(1))
- `crates/sbe-gen/tests/car_roundtrip.rs` — Real Logic's `Car` schema (two groups, one nested) through generated tables (step C3(2))
- `crates/sbe-gen/tests/encoding.rs` — byte-for-byte re-encode of all three RC4 dumps plus `Car`, over generated tables (moved from C3 to C4)
- `crates/sbe-gen/tests/versioning.rs` — schema versioning and truncation through generated tables (step C3(3))
- `crates/sbe-gen/tests/generated.rs` — the generator against two schemas it never wrote (RC4's and Real Logic's)
- `crates/sbe-gen/tests/value_ref_on_composite_member.rs` — `presence="constant" valueRef="Enum.Value"` on a composite member `<type>` resolves to the named value at 0 wire bytes, and the four refusals `sbe-tool` makes name `valueRef`; fixture `tests/fixtures/value_ref_on_composite_member.xml`, written for this repository ([the trap](../reference/sbe-valueref-on-a-composite-member.md))
- `crates/sbe/benches/alloc.rs` — counting allocator proving non-negotiable 1 for decode, one field, nested-group+`varData` walk, encode
- `crates/sbe/benches/sbe.rs` — the same four paths timed through the shared Criterion harness (no baseline recorded yet)
