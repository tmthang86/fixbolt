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

## Files, and what each keeps

| File | Keeps |
|---|---|
| `build.rs` | What only a build script does, around the generator: finds `spec/FIX44.xml` (shipped in this crate; `NANOFIX_FIX44_XML` overrides it), prints every `cargo:rerun-if-*` line, parses the XML (so a not-well-formed file is named by path), calls `codegen::fix44_from_document` and writes `$OUT_DIR/fix44.rs`; behind `fix50sp2`, the same for `$OUT_DIR/fixt11_fix50sp2.rs` from the `spec/FIXT11.xml` + `spec/FIX50SP2.xml` pair, printing the generator's warnings as `cargo:warning`s. A refusal from the generator becomes a failed build through `or_die` and `die` (`build.rs:171-181`), printing `fixbolt-dict: ` and the generator's sentence. Loads `src/codegen/` and `src/field_type.rs` by `#[path]` (`build.rs:160-167`), because a build script cannot `use` the crate it builds |
| `src/codegen/mod.rs` | The generator's entry points. Public, behind `codegen`: `fix44_tables` and (with `fix50sp2`) `fixt11_fix50sp2_tables`, which return exactly what `build.rs` writes; `generate`, `Source` and `Paths`, the user's door (still `GenError::Unsupported`, ADR-0207 decisions 3 and 4). Crate-internal: `fix44_from_document` and `pair_from_documents`, which `build.rs` calls on documents it parsed itself |
| `src/codegen/error.rs` | `GenError` — `Xml`, `Dictionary(String)` (a refusal, carrying the sentence `build.rs` used to `die` with, word for word) and `Unsupported`; and `refuse`, the old `die` as a value |
| `src/codegen/parse.rs` | Reading one document: `<field>`s into number, `FieldType` variant and enum values, `<component>`s by name, and the refusal of an empty component in a single-file build |
| `src/codegen/merge.rs` | Behind `fix50sp2`: merging the FIXT pair's fields and components under ADR-0083's agreement rules. The overlay merge of ADR-0207 decision 3 will live here |
| `src/codegen/model.rs` | `Spec`, what one table is built from, and the walks over it — required and allowed tags, the header, group members — each descending through `<component>` references with a cycle guard; the FIX 5.0 SP2 length exception |
| `src/codegen/emit.rs` | Writing a `Spec` out as Rust source — every table below — and the pair's transport layer. No panicking index: bitsets are set through `set_bit`, which refuses rather than indexes |
| `spec/FIX44.xml`, `spec/FIXT11.xml`, `spec/FIX50SP2.xml` | QuickFIX's dictionaries, byte-identical to the pin `scripts/fetch-quickfix-assets.sh` uses; `scripts/check-dict-spec-pin.sh` proves it. Never edited in place — a fix to a QuickFIX dictionary quirk goes through the `NANOFIX_*_XML` overrides, not this copy |
| `NOTICE` | The QuickFIX Software License, reproduced in full, plus the attribution sentence and the pinned commit; identical to the repository root's `NOTICE` (`scripts/check-dict-spec-pin.sh` proves that too). `include_str!`'d into `src/lib.rs`'s `NOTICE` constant |
| `src/lib.rs` | The generated crate root — tag constants, message shapes, `Fix44`, the header/trailer and per-message tag sets, field ordering, group delimiters and members; behind `fix50sp2`, the `fixt11_fix50sp2` module and `Fixt11Fix50Sp2Tables` (`codec::Dictionary` and `Tables` impls). Also the one hand-written item in this crate: `pub const NOTICE: &str`, re-exported as `fixbolt::NOTICE` |
| `src/field_type.rs` | The 23 FIX 4.4 field types and what each accepts on the wire — hand-written, because the *format* a type accepts is not stated in the XML at all |
| `src/tables.rs` | `Tables` — the dictionary questions the **session layer** asks (as opposed to `codec::Dictionary`'s parser questions); `Session` adds `where E::Dict: Tables` (ADR-0080 decision 1: the dictionary rides the encoding) |

## Read in this order

1. `build.rs` — which files are read, the overrides, and how a refusal stops the build
2. `src/codegen/mod.rs`, then `model.rs` and `emit.rs` — what is generated and from what
   (`parse.rs` and `merge.rs` when the question is about the XML itself)
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
- `tests/gen_matches_build.rs` — behind `codegen`: `generated_fix44_equals_the_build_output`
  and, with `fix50sp2`, `generated_pair_equals_the_build_output` run the library over the
  shipped `spec/` files and compare with what `build.rs` wrote to `$OUT_DIR`, byte for byte
  (ADR-0207 decision 2). `cargo test --all` never enables the feature, so the `gates` CI job runs
  both sets and `scripts/check-feature-gated-tests-ran.sh` proves the two tests executed. They
  compare against the shipped files: a build under a `NANOFIX_*_XML` override compares the
  wrong input
- `scripts/check-dict-refuses-a-message-without-msgcat.sh` — the only gate that observes the
  generator's three `msgcat` refusals actually stop the build: the unknown-category arm
  (`src/codegen/emit.rs:232-237`) and the missing-`msgcat` arm (`src/codegen/emit.rs:238-246`),
  both in the one `match` at `src/codegen/emit.rs:227-247`, and the `admin_types.is_empty()` arm
  (`src/codegen/emit.rs:645-651`). Each returns `GenError::Dictionary` with the sentence, and
  `build.rs`'s `or_die` prints it and exits.
  It never touches `vendor/`: it copies `FIX44.xml` into `target/check-msgcat/`, damages three
  copies with `sed`, and points `build.rs` at each through the `NANOFIX_FIX44_XML` override it
  already reads (named at `build.rs:45-46`, resolved by `spec_path` at `build.rs:114-119`, with
  `cargo:rerun-if-env-changed` printed at `build.rs:62`). Arm 0 (untouched) must build
  clean — proof the harness can tell the difference — and arms 1–3 must fail carrying their
  sentence: no `msgcat` attribute, `msgcat="other"`, and no `<message>` left carrying
  `msgcat='admin'` at all (the third reaches a second refusal, the `admin_types.is_empty()`
  condition at `src/codegen/emit.rs:645`, rather than the first). Runs in the `gates` CI job on
  every commit
