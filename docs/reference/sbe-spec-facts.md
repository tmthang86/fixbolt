# SBE 1.0 spec facts `crates/sbe` and `crates/sbe-gen` rely on

`[2026-09-19, step C8 of the phase 2 plan]` What the code assumes about SBE 1.0, cited to the
vendored spec text rather than restated from memory. The vendor is
FIXTradingCommunity/fix-simple-binary-encoding at `418a8f6a8b93c65b308638dab2bc6a35dddcd864`,
fetched into `vendor/sbe-spec/` by `scripts/fetch-sbe-assets.sh` (ADR-0081 decision 3); every
path below is under `vendor/sbe-spec/v1-0-RC4/doc/`. Two traps already recorded stand beside
this page rather than being repeated on it:
[the-sbe-rc4-example-dumps-disagree-with-their-own-tables](the-sbe-rc4-example-dumps-disagree-with-their-own-tables.md)
(the RC4 hex dumps are the oracle, never the prose table under them) and
[an-encoding-that-ignores-a-const-parameter-makes-every-caller-name-it](an-encoding-that-ignores-a-const-parameter-makes-every-caller-name-it.md)
(`Encoding::Template<P, S>`'s unused `P` on the SBE side).

| Fact the code relies on | Vendor path, section | Guarded by |
|---|---|---|
| The message header is 8 bytes: `blockLength`, `templateId`, `schemaId`, `version`, each `uint16`, in that order, in the schema's byte order | `03MessageStructure.md`, "Message header schema" | `crates/sbe/src/header.rs::MessageHeader::{decode,encode}`; `crates/sbe/src/tests.rs::the_header_encodes_back_to_the_dump_bytes` |
| A message's body is root block, then its repeating groups (if any), then its `varData` (if any) — never interleaved, and nested groups walk depth first | `03MessageStructure.md`, "Sequence of message body elements" | `crates/sbe/src/group.rs` (`Cursor::group`); `crates/sbe/src/tests.rs::execution_report_decodes_root_and_group_as_the_spec_dump_says`, `::nested_groups_and_var_data_walk_depth_first_in_big_endian` |
| A group's dimension (`groupSizeEncoding`, or a schema's own restricted `dimensionType`) is `blockLength` then `numInGroup`, each `uint16` by default, contiguous, in that order | `03MessageStructure.md`, "Group dimension encoding" / "Encoding of repeating group dimensions" | `crates/sbe/src/schema.rs::DimensionLayout` (default `U16`/`U16`); `crates/sbe/src/group.rs::Cursor::group` |
| `varData` is a length prefix (width the schema states) then that many bytes; SBE makes no distinction between an empty value and a null one | `02FieldEncoding.md`, "Variable-length string encoding" (RC4 also carries "Variable-length data encoding" for non-character `data`) | `crates/sbe/src/vardata.rs::Cursor::var_data`; `crates/sbe/src/tests.rs::business_reject_decodes_root_and_var_data_as_the_spec_dump_says` |
| Null values are per-primitive, not a single sentinel: `int64` null is `-2^63` (`i64::MIN`); `uint8` null is `255`; `char` null is `0` (NUL); `float`/`double` null is NaN | `02FieldEncoding.md`, "Range attributes for integer fields" (integers); "Range attributes for char fields" (char); "Null values" under the float/double section | `crates/sbe/src/tests.rs::optional_scalars_arrays_floats_and_constants_read_as_the_table_says`; the `StopPx` case in `::new_order_single_decodes_field_by_field_as_the_spec_dump_says` |
| An element (field, enum value, group or message) whose `sinceVersion` exceeds the header's `version` was not yet part of the schema when the message was encoded, and reads as absent, not as zero or an error | `05SchemaExtensionMechanism.md`, "Since version" (RC4 chapter 5 — see the correction below) | `crates/sbe/src/tests.rs::what_a_newer_version_added_reads_as_absent_from_an_older_message`; `crates/sbe-gen/tests/versioning.rs` part (a), which also records the stated gap: neither fixture schema (RC4's `Examples.xml`, Real Logic's `Car`) declares `sinceVersion`, so this property is not exercised through a *generated* table in this step |
| An unknown `templateId` can be skipped by the wire's own `blockLength` alone only when the message is flat (no groups, no `varData`); past the root block nothing promises where the message ends except the transport's own framing (SOFH or otherwise) | Found against the dumps, not stated this plainly in the prose — see the trap page below | `crates/sbe/src/tests.rs::an_unknown_template_is_delimited_by_block_length_and_not_walked`; `crates/sbe-gen/tests/versioning.rs` part (b) |

**Correction to ADR-0081 decision 4**: it cites the schema extension mechanism as "§3.6". The
extension mechanism is RC4 chapter 5, `05SchemaExtensionMechanism.md` ("Schema Extension
Mechanism"); §3.6 does not exist in the RC4 table of contents. Recorded first in
[the-sbe-rc4-example-dumps-disagree-with-their-own-tables](the-sbe-rc4-example-dumps-disagree-with-their-own-tables.md);
the ADR's own text is not edited for it (`CLAUDE.md` §5: an accepted ADR's substance is never
edited in place).

## Schema rules the generator refuses rather than repairs

`[2026-09-19, senior review of PR C]` Each of these was once accepted silently; each is now an
`Err` from `fixbolt_sbe_gen::generate`, guarded by a unit test in
`crates/sbe-gen/src/generator.rs`:

| Rule | Spec | Error |
|---|---|---|
| A composite with a member `offset` (padding) is as long as the composite, not the sum of its members — the next field starts after the padding | `04MessageSchema.md`, `offset` attribute, §4.4.4.3 | was a silent overlap; now laid out right (`a_padded_composite_field_is_as_long_as_the_composite`) |
| No group after `<data>` at the same level; no field after a group or `<data>` | `03MessageStructure.md`, "Repeating group after variable-length field" / "Fixed-length field after …" | `Schema` / `Unsupported` |
| An explicit `blockLength` is at least the extent of the fields; field and composite-member offsets do not overlap | `04MessageSchema.md`, `blockLength` row; "Incompatible offset and blockLength" | `Schema` |
| `byteOrder` is absent (little), `littleEndian` or `bigEndian` — nothing else | `04MessageSchema.md`, `messageSchema` attributes | `Schema` |
| `presence="optional"` on a non-char array, and `sinceVersion` on a composite member | outside ADR-0081 decision 5 | `Unsupported` |

**On the runtime side**: a group entry whose tail is entirely absent at the header's version
consumes no bytes, so a `uint32` `numInGroup` could make one tiny message cost a walk of four
billion entries. `crates/sbe/src/group.rs` skips such entries in one step
(`zero_byte_entries_are_walked_in_constant_time`, `crates/sbe/tests/encoding.rs`).
