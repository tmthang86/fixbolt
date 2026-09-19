# ADR-0081 — SBE tables come from this repository's generator, and the oracle is the spec's bytes plus `sbe-tool` behind a script

- **Status**: Accepted — 2026-09-19
- **Date**: 2026-09-19
- **Deciders**: Tran Manh Thang, who approved it by name ("duyệt ADR-0081", 2026-09-19) in the
  Mac session that builds PR C, and in the same message delegated every later design and merge
  decision on that PR to the session.
- **Related**: [ADR-0078](ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)
  decision 1 (a generated SBE codec, gated by round-tripping the reference schema examples);
  [ADR-0079](ADR-0079-one-view-per-encoding-and-one-trait-over-them.md) decisions 1, 4;
  [ADR-0001](ADR-0001-relationship-to-quickfix.md) (external assets are data, fetched into
  `vendor/`); [ADR-0042](ADR-0042-a-second-implementation-is-the-only-independent-opinion.md);
  `DESIGN.md` D3, D5; `CLAUDE.md` §2 items 1, 6, 9, §6 *Dependencies*; the plan
  [phase-2-fixt-and-sbe](../plans/2026-09-19-phase-2-fixt-and-sbe.md)
- **Answers**: *who generates the SBE layouts, in what crate, and what proves them right.*

## Context

ADR-0078 decides SBE arrives as *a generated SBE codec from an SBE XML schema*. Two things
could do the generating.

**The reference generator.** `sbe-tool` (Real Logic, Apache-2.0) has a Rust target
(`sbe-tool/src/main/java/uk/co/real_logic/sbe/generation/rust/`, read 2026-09-19) and the
README says its output is *"100% safe rust crates (no `unsafe` code will be generated)"* and
*"generated crates do not have any dependencies on any libraries"*. It is invoked as
`java -Dsbe.target.language=Rust -jar sbe-all-<version>.jar schema.xml`. It is a Java
program. `CLAUDE.md` §2 item 6 says `build.rs` invokes no external toolchain unless a
feature is on, and CI builds `--no-default-features` on a machine with nothing optional
installed; `codec` has zero runtime dependencies and `no_std` is its goal. A Java step inside
a Rust build is the exact shape D5 was written against. The generated flyweights also index
fields by generated method, one type per message — a shape that does not sit under one
`Encoding::field(view, id)` without a second layer.

**This repository's own generator.** `dict/build.rs` already turns a FIX XML dictionary into
tables with `roxmltree` as a build dependency, and D3 says layouts come from generated tables.
An SBE schema is smaller and more regular than a FIX dictionary: types, composites, enums,
sets, messages with fields at offsets, groups with a dimension composite, `varData` at the
end (SBE 1.0 §3.5: fixed fields, then groups, then variable-length data).

**What proves either right.** The spec's `07Examples.md` (v1.0 RC4) carries three
byte-exact hex dumps with offsets — a flat `NewOrderSingle` (header `blockLength=54,
templateId=99, schemaId=100, version=0`), an `ExecutionReport` with a repeating group
(`blockLength=42, templateId=98`) and a `BusinessMessageReject` with variable-length data
(`blockLength=9, templateId=100`). Those are an oracle with no toolchain. The reference
implementation's `sbe-samples/src/main/resources/example-schema.xml` (`Car`: composites,
enums, a set, a nested group, three `varData`, `byteOrder="littleEndian"`, `id="1"
version="0"`) is the schema every other SBE implementation is tested against, and the
`sbe-tool` output for it is a second implementation in the ADR-0042 sense. Both are fetched,
never committed (ADR-0001's rule for QuickFIX's assets, applied again).

The crates on crates.io (`sbe`, `sbe_gen` — the latter generates over `zerocopy`, a runtime
dependency) were read and not adopted: one dependency on the hot path each, none aligned
with `FieldIndex`/`MessageView`'s borrowed-view shape.

## Decision

1. **Two new crates, in DESIGN.md §7 order after `library`:**
   - **`sbe`** (L1, beside `codec`): the runtime. `SbeView<'a>` (`&[u8]` + `block_offset:
     u16` + `template_id: u16` + `schema_id: u16` + `version: u16` = **24 bytes**, `Copy`,
     pinned by a `const _: () = assert!` as `MessageView` is), the message-header decode,
     a group cursor over `groupSizeEncoding`, `varData` reads, and the `Encoding` impl
     `Sbe<S: Schema>`. **Zero runtime dependencies, `no_std`-compatible from the first
     commit** (`#![no_std]` on, because nothing in it needs `std`). No `unsafe`: every read
     is a bounds-checked slice, so a truncated message is an `Err`, never a read past the
     end.
   - **`sbe-gen`**: the generator, a library with one function `generate(xml: &str) ->
     Result<String, Error>` and a `build.rs`-shaped example, `roxmltree` as its one normal
     dependency. Not on any hot path; not a dependency of `sbe`. A user's crate lists it
     under `[build-dependencies]` and calls it from its own `build.rs`, which is how
     `dict` already works and how a user brings a venue schema this repository never sees.
2. **The generated output is tables, not flyweights.** Per message: template id, block
   length, a field table `(id, offset, len, primitive, presence)`, a group table
   `(id, dimension layout, block length, member table)`, and a `varData` table — all
   `&'static` and read by `sbe`'s generic accessors through `Schema`. One code path for
   every schema, one alloc bench, one timing bench; a typed accessor layer (`fn cl_ord_id()`)
   is a later, optional generation target, not phase 2.
3. **Two oracles, one CI job.** (a) `crates/sbe/tests/spec_examples.rs`: the three hex
   dumps from `07Examples.md`, transcribed as bytes, decoded field by field and re-encoded
   byte for byte; their schema is fetched into `vendor/sbe-spec/` by
   `scripts/fetch-sbe-assets.sh` at a pinned commit. (b) `scripts/sbe-interop.sh`: fetches
   `sbe-all-<version>.jar` from Maven Central at a pinned version, runs it on
   `example-schema.xml`, builds the generated crate in `target/`, and drives a small
   Rust binary that encodes `Car` with theirs and decodes with ours, then the reverse,
   asserting byte equality both ways. **A CI job `sbe-interop` runs it**, like `interop`
   runs `libquickfix`; Java (Temurin 17) is installed by the job, never assumed. No
   `build.rs` anywhere runs Java.
4. **Schema versioning is honoured on read**: a field whose `sinceVersion` is greater than
   the header's `version` reads as absent, and an unknown `templateId` is skipped by
   `blockLength` (SBE 1.0 §3.6). Fields marked `deprecated` are read like any other.
5. **Scope of SBE 1.0 supported in phase 2**: little- and big-endian schemas; `char`,
   integer and floating primitives; `composite`, `enum`, `set`; fixed-length arrays;
   constants; `optional` presence with null values; groups, nested groups; `varData`.
   Not in phase 2: SBE 2.0 RC features (issue #157's list), `ref` inside composites beyond
   one level, and any `offset`/`blockLength` padding scheme beyond what the schema states
   explicitly.

## Consequences

**Good**

- `sbe` keeps every rule `codec` lives under: zero dependencies, no `unsafe`, no
  allocation, borrowed views, a `Copy` view of 24 bytes, tables from generation (D3).
- The oracle that runs on every commit needs nothing installed; the second implementation
  runs in CI on the same footing as `libquickfix` does today.
- A user's venue schema is one `build.rs` away, and this repository never holds it —
  which is what *open-sourced from day one* requires of anything a venue owns.

**Bad — and accepted**

- **This is a second generator to maintain**, beside `dict/build.rs`, for a schema language
  with its own corner cases (composite `ref`, `offset` overrides, `sinceVersion` on groups).
  Every corner not in the two oracles is untested until a schema that uses it arrives.
- **Table-driven access is a lookup per field** — a linear scan of a `&'static` table, the
  same trade `MessageView` made (ADR-0003 point 4). For a 20-field root block it is cheaper
  than the FIX scan it replaces, and it is measured in `benches/sbe.rs` against the tag=value
  parse on the same machine before any number is quoted.
- **`sbe-tool` is pinned by version and fetched from Maven Central**; a Maven outage is a red
  CI job that says so, like a GitHub outage is for `interop`.
- **No venue schema is tested.** iLink 3, B3, MOEX all extend SBE with venue conventions;
  what phase 2 proves is the spec's examples and Real Logic's, nothing a venue signed.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| Commit `sbe-tool`'s generated Rust for the example schema and build on it | Generated code from an Apache-2.0 tool is fine to hold, but it makes the reference the implementation instead of the oracle, and its per-message flyweights do not sit under `Encoding::field` without a wrapper that costs what the generic tables cost anyway |
| Run `sbe-tool` from `build.rs` behind a feature | D5 rule 2 in letter, not in spirit: a feature whose on-state needs a JDK is a feature CI never turns on |
| Adopt `sbe_gen` (zerocopy) or `sbe` from crates.io | A runtime dependency on the hot path; neither shares the borrowed-view shape; neither has a public conformance oracle beyond its own tests |
| Flyweights (one struct per message) as the generated output | Fast and typed, but `Encoding` needs one `View` and one `field` per encoding; flyweights are the optional layer above, later |

## Sources

- FIX Trading Community, SBE v1.0 RC4 — *Message Structure* (§2 header composite, §3.3.1
  root block, §3.4.5 `groupSizeEncoding`, §3.5 order, §3.6 unknown template) and *Examples*
  (three hex dumps) —
  <https://github.com/FIXTradingCommunity/fix-simple-binary-encoding/blob/master/v1-0-RC4/doc/03MessageStructure.md>,
  <https://github.com/FIXTradingCommunity/fix-simple-binary-encoding/blob/master/v1-0-RC4/doc/07Examples.md>;
  SBE 2.0 RC features listed in issue #157. Read 2026-09-19.
- Real Logic / Aeron `simple-binary-encoding` README — Rust generator *"100% safe … no
  dependencies"*, Apache-2.0, `java -Dsbe.target.language=Rust -jar sbe-all-<v>.jar`;
  Maven `uk.co.real-logic:sbe-tool`; the Rust generator sources; `sbe-samples/.../
  example-schema.xml` — <https://github.com/aeron-io/simple-binary-encoding>, read 2026-09-19.
- crates.io `sbe`, `sbe_gen` (zerocopy-based generator), `sbe-schema` — read 2026-09-19,
  not adopted.
- `crates/dict/build.rs` (`roxmltree` as a build dependency; `NANOFIX_FIX44_XML` override),
  `scripts/interop.sh` (an external toolchain fetched and built by a script, run by a CI
  job), read 2026-09-19.
