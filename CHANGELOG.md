# Changelog

Notable changes to the published crates. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning will follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) from the first release.

**Scope, stated so this file does not drift into being a second `STATUS.md`:** it records
changes to the **public API and observable behaviour of released crates**, and nothing else.
Decisions live in [docs/decisions/](docs/decisions/); where the work stands lives in
[STATUS.md](STATUS.md); what is about to be built lives in [docs/plans/](docs/plans/). A thing
that has not shipped does not belong here — `CLAUDE.md` §4: one rule, one place.

## [Unreleased]

**Nothing has been released.** Two crates now exist and neither is published; the entries
below describe what a first release would contain.

### Added

- **`nanofix-codec`** — FIX 4.4 read and write, `no_std`, zero runtime dependencies.
  - `parse_into::<D, N>(buf, &mut idx, validation) -> Result<Parsed, ParseError>`. `Incomplete`
    is an `Ok`, because TCP delivers bytes and not messages.
  - `FieldIndex<N>` and `MessageView<'_, N>` — the caller owns the index and reuses it;
    the view is 24 bytes and `Copy`.
  - `Template<P, S>` and `TemplateBuilder` — a message skeleton that sorts its fields once, at
    build time, and fills holes at send time. Optional slots may be omitted.
  - `TimestampCache` — `SendingTime` with the minute prefix cached.
  - `Dictionary` trait. `is_header` and `data_length_tag` are answered by `nanofix-dict`;
    of the three repeating-group methods, all three are now implemented there — reading and
    writing groups is not, and lands with the rest of the repeating-groups plan.
- **`nanofix-dict`** — 912 tag constants, 93 message types, `is_header`, `data_length_tag`,
  `required` and the group tables, all generated from `FIX44.xml` at build time.
  - `group_members(msg_type, counter)` — one table serving `group_delimiter` (its head) and
    `group_order` (itself), so the three cannot disagree. Keyed by **`(msg_type, counter)`**:
    four counters take a different delimiter in different messages.
  - `GROUP_COUNTERS = 59`, `GROUP_POSITIONS = 731`.

### Known limitations, stated rather than discovered

- `required()` does not descend into `<component>`, so it is wrong for 21 of the 93 message
  types. Nothing calls it. Component recursion arrives with the repeating-groups plan.
- Repeating groups are not read or written. `Template` sorts by tag, which would reorder a
  group's members.
- The trailer tags — `Signature(89)`, `SignatureLength(93)`, `CheckSum(10)` — are classified as
  neither header nor body, so a written `Signature` would sort into the body. Nothing writes one.
- `[measured]` Encoding an `ExecutionReport` costs 93.8 ns against a published target of 60 ns.

Crate names change before any publish: `nanofixengine` is a placeholder.

**Before the first publish, the crate names change.** `nanofixengine` is a placeholder taken
to clear a collision with `matthart1983/nanofix`; the shortlist is in [STATUS.md](STATUS.md).
Renaming after a crates.io publish is not possible, so it happens before.
