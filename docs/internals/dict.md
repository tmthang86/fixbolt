# `dict` — internals

Build-layer crate in [DESIGN.md §3](../DESIGN.md#3-crates): code generation from the FIX 4.4
XML — tag constants, message shapes, required-field tables, field ordering, group members,
and the validation tables. It implements `codec::Dictionary`. **The three QuickFIX XML files
this generates from ship inside the crate**, at `spec/`, byte-identical to a pinned commit,
under `NOTICE` — the one deliberate exception to "no QuickFIX source is copied"
([ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md) decision 5,
[ADR-0104](../decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)),
made so `fixbolt` builds with nothing but what is in the repository (the git tag) or in a
`.crate` — no `vendor/` checkout either way
([ADR-0161](../decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)).
Everything else QuickFIX's
— the `.def` acceptance corpus and its own generated C++ — stays a test oracle in gitignored
`vendor/`, never committed. Behind the `fix50sp2` feature, the build also emits the FIXT 1.1 /
FIX 5.0 SP2 pair; `fix50sp2` adds no dependency
([ADR-0080](../decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)).

The generator is a library, in `src/codegen/`, and one generator has two loadings
([ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md) decision 1, the shape `crates/sbe-gen` already has,
[ADR-0081](../decisions/ADR-0081-sbe-tables-come-from-this-repositorys-generator-and-the-oracle-is-the-specs-bytes-plus-sbe-tool-behind-a-script.md)): `build.rs` loads it by `#[path]` to write this crate's own tables, and the
off-by-default **`codegen`** feature declares it as `pub mod codegen` for a user's own `build.rs`.
`roxmltree` (pinned `=0.20.0`) is therefore two dependencies with one version: a
**build-dependency always**, and a **normal dependency only under `codegen`** — with no feature on,
it never reaches the target build (`scripts/check-no-optional-deps.sh` asks, case
`fixbolt-dict:roxmltree`).

**The overlay path** (ADR-0207 decisions 3, 4 and 8) is the third caller: a user's `build.rs`
calls `codegen::generate(source, "Venue", paths)`, where `source` is a `Source::Fix44Overlay`
(additions to the FIX 4.4 this crate ships) or a `Source::Fix44Whole` (a complete FIX 4.4 file of
their own). An overlay is not a second generator. `merge.rs` splices its additions into the
text of the shipped `spec/FIX44.xml` (reached by `include_str!`, never through
`NANOFIX_FIX44_XML`), and the merged document then goes through the same `Model::compute` that
builds `Fix44`, so an empty overlay writes `Fix44`'s own tables byte for byte. The order is:
`merge` refuses what only an overlay can get wrong (a repeated number, name or `msgtype` that
disagrees with FIX 4.4, a value list on an open field, a `<trailer>`, an unknown section).
`Model::compute` then makes every refusal about the dictionary's content. That includes three
added in review: a tag in two of header, trailer and message bodies; a DATA group member whose
length field is not the member immediately in front of it (the order `put_group` in
`crates/codec/src/template.rs` writes); and per-tag bitsets over the 64 MiB ceiling
(`MAX_BITSET_BYTES`). `parse` refuses a field numbered 0. `merge::refuse_repeats`
last refuses a field carried twice at one level. `emit` only writes. `codegen::merged_model` stops
before the writing: it returns the `Model`, which answers the questions the emitted `impl
Dictionary` and `impl Tables` will answer, and whose `table_size()` is the size report. The
emitted file opens with a compile-time check of `FORMAT_VERSION` (`src/codegen/format.rs`)
against `fixbolt_dict::codegen_format::FORMAT_VERSION`. `lib.rs` compiles that path whether
`codegen` is on or not, because the file is compiled against the user's runtime copy of this
crate, which is built without `codegen`
([the-format-check-a-generated-file-makes-must-compile-without-the-feature-that-wrote-it](../reference/the-format-check-a-generated-file-makes-must-compile-without-the-feature-that-wrote-it.md)).

## Files, and what each keeps

| File | Keeps |
|---|---|
| `build.rs` | What only a build script does, around the generator: finds `spec/FIX44.xml` (shipped in this crate; `NANOFIX_FIX44_XML` overrides it), prints every `cargo:rerun-if-*` line, parses the XML (so a not-well-formed file is named by path), calls `codegen::fix44_from_document` and writes `$OUT_DIR/fix44.rs`; behind `fix50sp2`, the same for `$OUT_DIR/fixt11_fix50sp2.rs` from the `spec/FIXT11.xml` + `spec/FIX50SP2.xml` pair, printing the generator's warnings as `cargo:warning`s. A refusal from the generator becomes a failed build through `or_die` and `die` (`build.rs:171-181`), printing `fixbolt-dict: ` and the generator's sentence. Loads `src/field_type.rs` and `src/codegen/` by `#[path]` (`build.rs:159-167`), because a build script cannot `use` the crate it builds |
| `src/codegen/mod.rs` | The generator's entry points. Public, behind `codegen`: `fix44_tables` and (with `fix50sp2`) `fixt11_fix50sp2_tables`, which return exactly what `build.rs` writes. The user's door is `generate(Source, type_name, Paths)`: `merged_model`, then a banner carrying the table size, the format check, a `tables` module, a unit struct named `type_name`, and its `impl Dictionary` and `impl Tables`. `merged_model(Source)` is the same merge without the writing. `Source` is `Fix44Overlay` or `Fix44Whole` and is `#[non_exhaustive]`. `Paths::facade()` (the default) names `::fixbolt::dict::…`; `Paths::direct()` names `::fixbolt_dict` and `::fixbolt_codec`. `SHIPPED_FIX44` is the `include_str!` of `spec/FIX44.xml` an overlay merges onto. The rustdoc holds the `compile_fail,E0080` doctest of the format check. Crate-internal: `fix44_from_document` and `pair_from_documents`, which `build.rs` calls on documents it parsed itself |
| `src/codegen/format.rs` | `FORMAT_VERSION`, the number the emitted file checks. This file is reached two ways: as `codegen::format`, re-exported as `fixbolt_dict::codegen_format`, when `codegen` is on, and loaded by `lib.rs` by `#[path]` as the same hidden `codegen_format` when it is off. Both ways read this one line |
| `src/codegen/error.rs` | `GenError`, `#[non_exhaustive]`. Its variants are `Xml`, `Dictionary(String)` and `Unsupported`. `Dictionary` is a refusal, carrying the sentence `build.rs` used to `die` with, word for word. No function returns `Unsupported` since the overlay landed. Also holds `refuse`, the old `die` as a value |
| `src/codegen/parse.rs` | Reading one document: `<field>`s into number, `FieldType` variant and enum values, `<component>`s by name, and the refusal of an empty component in a single-file build |
| `src/codegen/merge.rs` | Two merges, whose rules differ. The first, behind `fix50sp2`, merges the FIXT pair's fields and components under ADR-0083's agreement rules (`merge_fields`, `merge_components`). The second is the **FIX 4.4 overlay**: `overlay_fix44` walks the overlay's `<header>`, `<messages>`, `<components>` and `<fields>`, checks each against the shipped file, and splices the additions into its text. `overlay_values` holds the two value-list rules: a value already listed is skipped whatever its description, and a list on an open field is refused. `refuse_repeats` runs on the merged document after the table walk and refuses one field twice at one level, naming the component path |
| `src/codegen/model.rs` | `Spec`, what one table is built from, and the walks over it: required and allowed tags, the header, group members. Each walk descends through `<component>` references with a cycle guard. Also the FIX 5.0 SP2 length exception. `Model::compute` turns a `Spec` into the public `Model` and makes **every refusal about a dictionary's content**, the three `msgcat` refusals included, so a `Model` that exists can be written. `Model`'s query methods (`is_defined_tag`, `field_type`, `enum_allows`, `is_msg_type`, `is_admin`, `required`, `allows`, `is_header`, `data_length_tag`, `group_delimiter`, `group_members`) answer as the functions of the same name on `codec::Dictionary` and `Tables`. `TableSize` is the size report |
| `src/codegen/emit.rs` | Writing a `Model` out as Rust source: every table below, and the pair's transport layer. It refuses nothing about the dictionary; its only refusal is its own arithmetic. No panicking index: bitsets are set through `set_bit`, which refuses rather than indexes |
| `spec/FIX44.xml`, `spec/FIXT11.xml`, `spec/FIX50SP2.xml` | QuickFIX's dictionaries, byte-identical to the pin `scripts/fetch-quickfix-assets.sh` uses; `scripts/check-dict-spec-pin.sh` proves it. Never edited in place — a fix to a QuickFIX dictionary quirk goes through the `NANOFIX_*_XML` overrides, not this copy |
| `NOTICE` | The QuickFIX Software License, reproduced in full, plus the attribution sentence and the pinned commit; identical to the repository root's `NOTICE` (`scripts/check-dict-spec-pin.sh` proves that too). `include_str!`'d into `src/lib.rs`'s `NOTICE` constant |
| `src/lib.rs` | The generated crate root — tag constants, message shapes, `Fix44`, the header/trailer and per-message tag sets, field ordering, group delimiters and members; behind `fix50sp2`, the `fixt11_fix50sp2` module and `Fixt11Fix50Sp2Tables` (`codec::Dictionary` and `Tables` impls). Also the one hand-written item in this crate: `pub const NOTICE: &str`, re-exported as `fixbolt::NOTICE`. And the hidden `codegen_format` module, compiled in every feature set: the re-export of `codegen::format` under `codegen`, `src/codegen/format.rs` loaded by `#[path]` without it. It is not API; it exists for the format check a generated file makes |
| `src/field_type.rs` | The 23 FIX 4.4 field types and what each accepts on the wire — hand-written, because the *format* a type accepts is not stated in the XML at all |
| `src/tables.rs` | `Tables` — the dictionary questions the **session layer** asks (as opposed to `codec::Dictionary`'s parser questions); `Session` adds `where E::Dict: Tables` (ADR-0080 decision 1: the dictionary rides the encoding) |

## Read in this order

1. `build.rs` — which files are read, the overrides, and how a refusal stops the build
2. `src/codegen/mod.rs`, then `model.rs` and `emit.rs` — what is generated and from what
   (`parse.rs` and `merge.rs` when the question is about the XML itself; `merge.rs` from
   `overlay_fix44` when it is about a user's overlay)
3. `src/lib.rs` — the shape of the generated output (read a generated table, not the
   generator's emit code, to see what a caller actually gets)
4. `src/field_type.rs` — the one hand-written table, and why it is separate from generation
5. `src/tables.rs` — the session-facing trait, read last since it is asked only after
   `codec::Dictionary` already answered

## Tests that guard it

- `tests/tables.rs`, `tests/group_tables.rs` — the generated tables themselves
- `tests/field_types.rs`, `tests/enums.rs` — the hand-written type table and enum values
- `tests/parse_with_real_dictionary.rs` — `codec::Dictionary` implemented correctly
- `tests/interop_quickfix_fields.rs`, `tests/interop_quickfix_messages.rs`,
  `tests/interop_quickfix_order.rs` — generated field/message/order tables checked against
  QuickFIX's own generated C++, per [DESIGN.md D3](../DESIGN.md). These read the shipped
  `spec/FIX44.xml` (via `tests/common/mod.rs::read_spec`), which is what checks the file that
  actually ships rather than `vendor/`'s copy of it; the C++ oracle itself still comes from
  gitignored `vendor/` (`tests/common/mod.rs::read`) and a missing oracle is a failure, never a
  skip
- `tests/fixt.rs`, `tests/fixt_order.rs` — the `fix50sp2` FIXT 1.1 / FIX 5.0 SP2 tables and
  `Fixt11Fix50Sp2Tables`
- `tests/notice.rs` — `fixbolt_dict::NOTICE` carries the QuickFIX Software License's condition 3
  acknowledgment, its condition 5 naming restriction, and the pinned commit
- `scripts/check-dict-spec-pin.sh` — the three shipped files hash to the pin, the pin matches
  `scripts/fetch-quickfix-assets.sh`'s, the two `NOTICE` copies agree, and (when `vendor/` is
  present) the shipped file and `vendor/`'s copy are the same bytes, not just the same hash
- `tests/generated_is_pinned.rs` — the generated `fix44.rs` and, with `fix50sp2`,
  `fixt11_fix50sp2.rs` hash to committed SHA-256 constants, both as `build.rs` wrote them to
  `$OUT_DIR` (no feature needed, so every `cargo test` runs it) and, behind `codegen`, as the
  library returns them. This is what holds ADR-0207 decision 2 — "the generated bytes do not
  change" — and a deliberate change updates the constant, with the reason in the commit body.
  The SHA-256 is written out in the test and checked against FIPS 180-4's examples first
- `tests/gen_matches_build.rs` — behind `codegen`: `generated_fix44_equals_the_build_output`
  and, with `fix50sp2`, `generated_pair_equals_the_build_output` run the public functions over the
  shipped `spec/` files and compare with what `build.rs` wrote to `$OUT_DIR`. That proves the two
  paths into the generator agree, **not** that its output is unchanged: `build.rs` loads the same
  `src/codegen/`, so a change to what it emits moves both sides (the pin above catches that).
  `cargo test --all` never enables `codegen`, so the `gates` CI job runs both sets and
  `scripts/check-feature-gated-tests-ran.sh` proves the tests executed. Both files compare against
  the shipped `spec/`: a build under a `NANOFIX_*_XML` override is red or compares the wrong input
- `tests/overlay.rs`: behind `codegen`, and run in the same `gates` step under the same proof. It
  holds the overlay path, each question asked of `merged_model`'s `Model` rather than by searching
  the emitted text. Its test names are the behaviour list. An overlay adds a field, an enum value,
  a group (its delimiter and declared order kept per message), a message type, a required field, a
  header field and a header group. A conflict is refused naming both sides: a number, a name, a
  type, a `msgtype` or a `msgcat` that disagrees with FIX 4.4; an unknown field; one field twice at
  one level, through a component too; a `<trailer>`; a misspelt section; a value list on an open
  field; a DATA field without its length field; a type name that cannot name a struct; a tag in
  the header or trailer and a body, or in both the header and the trailer; a DATA group member
  without its length immediately in front (at body level any order stays accepted); a field
  numbered 0; a tag whose bitsets would pass 64 MiB. The FIX
  4.4 traps the base survives still hold after the merge. `an_empty_overlay_emits_byte_identical_fix44`
  holds an empty overlay to `$OUT_DIR/fix44.rs`, byte for byte. `a_whole_file_generates_as_is`
  reads a whole file as written, a retype included, which an overlay refuses. `a_high_tag_reports_the_table_size` checks the size report.
  `the_generated_file_opens_with_its_format_check` holds the emitted check to the text of
  `mod.rs`'s two doctests. The `compile_fail,E0080` one proves that a file generated at another
  format does not compile. `generate_names_the_facade_or_the_crates_by_paths` checks the paths
  in the emitted code. The fixture, `tests/fixtures/overlay-invented.xml`, is invented. What
  it does **not** prove: that a generated file compiles in a user's crate, or that an engine
  built on it answers the wire as the `Model` does (plan steps 27 and 28)
- `scripts/check-dict-refuses-a-message-without-msgcat.sh` — the only gate that observes the
  generator's three `msgcat` refusals actually stop the build: the unknown-category arm
  (`src/codegen/model.rs:534-539`) and the missing-`msgcat` arm (`src/codegen/model.rs:540-548`),
  both in the one `match` at `src/codegen/model.rs:531-549` inside `Model::compute`, and the
  no-admin-message arm (`src/codegen/model.rs:656-663`). Each returns `GenError::Dictionary` with
  the sentence, and
  `build.rs`'s `or_die` prints it and exits.
  It never touches `vendor/`: it copies `FIX44.xml` into `target/check-msgcat/`, damages three
  copies with `sed`, and points `build.rs` at each through the `NANOFIX_FIX44_XML` override it
  already reads (named at `build.rs:45-46`, resolved by `spec_path` at `build.rs:114-119`, with
  `cargo:rerun-if-env-changed` printed at `build.rs:62`). Arm 0 (untouched) must build
  clean — proof the harness can tell the difference — and arms 1–3 must fail carrying their
  sentence: no `msgcat` attribute, `msgcat="other"`, and no `<message>` left carrying
  `msgcat='admin'` at all (the third reaches a second refusal, the no-admin-message
  condition at `src/codegen/model.rs:656`, rather than the first). Runs in the `gates` CI job on
  every commit
