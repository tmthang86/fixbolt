# ADR-0083 — Ten field types, one field spelled two ways, one empty component, and where the SP2 oracle comes from

- **Status**: Proposed — 2026-09-19
- **Date**: 2026-09-19
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0080](ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
  decision 2 (the pair build — this ADR narrows its "must agree on number, name and type" and
  fills the gaps it left); [ADR-0001](ADR-0001-relationship-to-quickfix.md) (the XML is data and
  a test oracle, and *the XML is the source of truth*); [ADR-0058](ADR-0058-a-timestamp-is-read-at-every-precision-and-written-at-three.md)
  (the one time-width rule both readers share); `DESIGN.md` D3; `CLAUDE.md` §2 items 5, 6, 9;
  the plan [phase-2-fixt-and-sbe](../plans/2026-09-19-phase-2-fixt-and-sbe.md) rows **B1** and
  **B2**; traps recorded in [fixt-dictionary-traps](../reference/fixt-dictionary-traps.md) and
  [an-oracle-absent-on-disk-is-not-an-oracle-absent-upstream](../reference/an-oracle-absent-on-disk-is-not-an-oracle-absent-upstream.md).
- **Answers**: five things measured on 2026-09-19 that stop plan row B1 from building as
  written, and one that would make row B2 choose the weaker oracle for the wrong reason.

## Context

ADR-0080 decision 2 says `dict/build.rs` builds one table from `FIXT11.xml` + `FIX50SP2.xml`
and that *"a field defined in both files must agree on number, name and type, or the build
fails"*. Everything below was measured on 2026-09-19 against `vendor/quickfix/spec/` at the
pinned SHA `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`, with `xml.etree`, not a regular
expression (`fix44-dictionary-traps.md` trap 4 says why that matters).

1. **Ten type names `FieldType::from_xml` does not know.** `FIX44.xml` uses 23 type names;
   `FIX50SP2.xml` uses 32 — the 23 minus `MULTIPLEVALUESTRING`, plus ten: `LOCALMKTTIME` (45
   fields), `XID` (34), `XIDREF` (29), `MULTIPLECHARVALUE` (8), `XMLDATA` (8), `TZTIMEONLY` (6),
   `MULTIPLESTRINGVALUE` (4), `TZTIMESTAMP` (1), `LANGUAGE` (1), `TAGNUM` (1). `build.rs` line
   467–480 `die`s on the first unknown name, by design (*"a 24th type appearing upstream must
   stop the build"*), so the pair build stops at the first of 137 fields.
2. **`XmlData(213)` is `DATA` in `FIXT11.xml` and `XMLDATA` in `FIX50SP2.xml`.** All 71 FIXT11
   fields appear in FIX50SP2; 70 agree on number, name and type; this one differs on type
   alone, and it is spelled `DATA` in `FIX40`…`FIX50SP1.xml` too. QuickFIX's own aggregate
   generator resolves it by last-write-wins (`spec/Aggregator.rb` `fields()` overwrites
   `"type"`, `Makefile.am` lists `FIXT11.xml` last), which is why `src/C++/FixFields.h` says
   `DEFINE_DATA(XmlData)` beside `DEFINE_XMLDATA(SecurityXML)`. QuickFIX/J's copies say
   `DATA` in both files. Neither tracker has an issue about it (search 2026-09-19).
3. **The SP2 order oracle exists upstream and is not on disk.** `git ls-tree` at the pin shows
   160 generated headers under `src/C++/fix50sp2/` (26 413 279 bytes; `fix44/` is 95 files,
   844 426 bytes). `scripts/fetch-quickfix-assets.sh` line 50 checks out `/src/C++/fix44/`
   only, and `vendor/quickfix-src` — the path row B2 tests for — exists only after
   `scripts/interop.sh` has cloned and cmake-built libquickfix, which no `dict` test and no
   CI `test` job does. `NewOrderSingle.h` at the pin carries `MsgType("D")` and 233
   `FIX::Group(` sites in the same shape `interop_quickfix_order.rs` already reads.
4. **The shared `<component>` `MsgTypeGrp` is empty in the transport file.** `FIXT11.xml`
   holds two components: `HopGrp` (byte-identical to FIX50SP2's) and
   `<component name='MsgTypeGrp' />` — self-closing, no children (line 112). FIX50SP2's
   `MsgTypeGrp` carries `NoMsgTypes(384)` with six members (`RefMsgType(372)`,
   `MsgDirection(385)`, `RefApplVerID(1130)`, `RefApplExtID(1406)`, `RefCstmApplVerID(1131)`,
   `DefaultVerIndicator(1410)`). FIXT11's Logon references `MsgTypeGrp` (line 79). No
   component in FIX44, FIX50SP2 or anywhere else in FIXT11 is empty, no `<group>` in FIX50SP2
   is empty, and no `.def` in the three FIXT corpora carries `384=`, `1128=`, `1129=` or
   `1156=`. QuickFIX C++ never merges the two files and so never faces the question — but its
   generated `src/C++/fixt11/Logon.h` at the pin has **no** `FIX::Group(` at all, and
   `Message.cpp` line 328–329 parses an admin message's body against the *session*
   dictionary, so a Logon carrying `NoMsgTypes` reaches QuickFIX C++ as repeated flat tags.
   QuickFIX/J's `FIXT11.xml` has the component populated (line 111).
5. **One SP2 `DATA` field has a length field the name rule cannot find.** `build.rs` pairs a
   `DATA` field with `{name}Len` or `{name}Length` (line 141–158) and dies otherwise. Of the
   75 `DATA` fields in FIX50SP2, 74 pair; `EncodedUnderlyingMarketDisruptionFallbackUnderlierSecurityDesc(41874)`
   does not — its length is `EncodedUnderlyingMarketDisruptionFallbackUnderlierSecDescLen(41873)`,
   with `Security` abbreviated to `Sec`. All 8 `XMLDATA` fields pair by name. Twelve
   `DATA`/`XMLDATA` fields do not sit at `length + 1`, so QuickFIX C++'s own rule
   (`Message.cpp` line 587–592: *"Assume length field is 1 less"*, `Signature` special-cased)
   mis-pairs eleven of them.
6. **Enum lists on the 71 shared fields are not identical either.** Nine carry values on both
   sides; two of those differ — `ApplVerID(1128)`: SP2 adds `10`; `SessionStatus(1409)`: SP2
   adds `9`, `10` — and two carry values on the SP2 side only, `BeginString(8)` (3 values) and
   `MsgType(35)` (164). In every case SP2's set is a superset of FIXT11's. ADR-0080's rule says
   nothing about enums.

And one thing found on the way that is **not** decided here because it lives on the FIX 4.4
path: the generated `enum_allows` matches the whole value against the list, so a legal
two-value `18=2 A` is `Some(false)` and the session answers `373=5`. QuickFIX C++
(`DataDictionary.h` `isFieldValue`, `isMultipleValueField`) and QuickFIX/J
(`DataDictionary.java` line 526–527) both split on spaces first. No `.def` sends a multi-value
field. Decision 1 below must not let the FIXT table inherit it; the FIX 4.4 fix is a plan row
of its own (see Consequences).

What the specification says about the ten types comes from FIX Orchestra's *FIX Latest*
repository, the only spec text reachable from this desk on 2026-09-19 (`fiximate.fixtrading.org`,
`fixtrading.org` and `onixs.biz` are blocked by the egress proxy); the ten datatypes are
unchanged in wording since FIX 5.0 SP2 introduced them, and the file's text is quoted per
type under decision 1. How the two sibling engines validate them, read the same day:
QuickFIX C++ at the pin maps **none** of the ten in `XMLTypeToType` (`DataDictionary.cpp` line
589–680 — the `TYPE::Xid`, `TYPE::XmlData`, `TYPE::TzTimeOnly`… enum arms exist but nothing
produces them from XML), so every one of the 137 fields is `TYPE::Unknown` and
`checkValidFormat` skips it; the generated field classes are `String` for all but `TagNum`
(`Field.h` line 585–606, `FieldTypes.h` line 754–775). QuickFIX/J's `FieldType.fromName`
returns `UNKNOWN` for all ten (`FieldType.java` — no such constants) and `checkValidFormat`
has no `UNKNOWN` arm, so it validates none of them either. Artio's `Field.Type` knows
`MULTIPLECHARVALUE`, `MULTIPLESTRINGVALUE`, `LANGUAGE`, `XMLDATA`, `TZTIMEONLY`, `TZTIMESTAMP`
and not `XID`, `XIDREF`, `LOCALMKTTIME`, `TAGNUM` (`Type.lookup` → `valueOf` throws), and its
merge of transport + application files is `HashMap.putAll` — the application file overwrites
the transport file for fields **and** components, silently (`DictionaryParser.java` line
102–114).

## Decision

### 1. The ten type names map as follows, and every mapping is a format rule, not a lookup

`FieldType` gains six variants; four names map onto variants that already exist because the
specification defines them as the same wire format. What `accepts` does for each decides when
`373=6` fires, so each row says whether the check is the specification's format exactly or
deliberately looser, and what looser costs.

| XML type | `FieldType` variant | `accepts` | Exact or permissive |
|---|---|---|---|
| `XID` (34) | `String` (existing) | anything but SOH | **Exact for format.** Spec: *base type String; "the values of all the fields that have an XID datatype in a FIX message must be unique"*. Uniqueness is a cross-field rule, not a format, and no engine surveyed checks it; not checked here, and named as such |
| `XIDREF` (29) | `String` (existing) | anything but SOH | **Exact for format.** Spec: *"a reference to an identifier defined by the XID datatype"*. That the reference resolves is a cross-field rule; not checked |
| `LANGUAGE` (1: `LanguageCode(1474)`) | **`Language`** (new) | exactly two ASCII letters | **Exact.** Spec: *"Identifier for a national language — uses ISO 639-1 standard"*. Case is not checked, as `Country` does not check it |
| `TAGNUM` (1: `MatchAttribTagID(1626)`) | **`TagNum`** (new) | digits, first digit not `0` | **Exact.** Spec: *"int field representing a tag number. Value must be positive and may not contain leading zeros."* Stricter than QuickFIX's `IntConvertor`, which takes `-1` and `007` |
| `MULTIPLECHARVALUE` (8: `ExecInst(18)`, `Scope(546)`, …) | **`MultipleCharValue`** (new) | one or more single bytes, separated by single spaces, none empty | **Exact.** Spec: *"one or more space delimited single character values (e.g. \|18=2 A F\|)"*. QuickFIX/J checks exactly this (`CharArrayConverter`); QuickFIX C++ takes any string |
| `MULTIPLESTRINGVALUE` (4: `QuoteCondition(276)`, `TradeCondition(277)`, …) | `MultipleValueString` (existing) | anything but SOH | **Exact, under FIX 4.4's name for the same thing.** Spec: *"one or more space delimited multiple character values"* — the definition `MULTIPLEVALUESTRING` had before SP2 split it in two. Two spellings, one variant; `as_rust` keeps `MultipleValueString`. Not checked: empty tokens (`"AV  AN"`), same as today |
| `LOCALMKTTIME` (45) | **`LocalMktTime`** (new) | `HH:MM:SS`, 8 bytes, hour ≤ 23, minute ≤ 59, second ≤ 60 | **Exact.** Spec: *"Format is HH:MM:SS"* — no fraction is offered. Reuses the `time` reader with the width pinned to 8 |
| `TZTIMEONLY` (6: `MaturityTime(1079)`, …) | **`TzTimeOnly`** (new) | `HH:MM[:SS]` then optionally `Z`, or `+`/`-` `hh`, or `+`/`-` `hh:mm`; `hh` 01–12, `mm` 00–59 | **Exact.** Spec: *"HH:MM[:SS][Z \| [ + \| - hh[:mm]]]"*. No fraction, per the text |
| `TZTIMESTAMP` (1: `TZTransactTime(1132)`) | **`TzTimestamp`** (new) | `YYYYMMDD-HH:MM:SS[.s…]` under the ADR-0058 width rule, then the same optional zone suffix as `TzTimeOnly` | **Exact.** Spec (tag=value form): *"YYYYMMDD-HH:MM:SS.sss\*[Z \| [ + \| - hh[:mm]]] … the fractions of seconds may be empty"*. The date-time part is the `UtcTimestamp` reader **unchanged**, so ADR-0058's one rule for widths stays one rule; only the suffix is new |
| `XMLDATA` (8: `XmlData(213)`, `SecurityXML(1185)`, …) | `Data` (existing) | anything, including SOH and nothing | **Exact.** Spec: *"Contains an XML document raw data with no format or content restrictions. XMLData fields are always immediately preceded by a length field."* — the delimiting rule of `data`, so the same variant. `build.rs` pairs a length field for `XMLDATA` exactly as for `DATA` (all 8 pair by name, measured). Mapping it to `String` would split `SecurityXML` on an embedded `0x01` — which is what QuickFIX C++ and QuickFIX/J do today, since neither registers `XMLDATA` as a data field |

Two rules follow from the table and hold for every row:

- **A type name this table does not list still stops the build.** The ten are added to
  `from_xml` and `as_rust`; the `_ => return None` arm stays, and `build.rs` keeps dying on it.
  `FieldType` documents itself as *"exactly the type names the two dictionaries use"*, and
  `crates/dict/tests/field_types.rs` asserts the count — 29 variants, not 23 — so a thirtieth
  arriving upstream is a red test, not a silent `String`.
- **The enum check on a multi-value type is per token.** `enum_allows` on a field whose type is
  `MultipleValueString` or `MultipleCharValue` splits the value on single spaces and answers
  `Some(true)` only if every token is in the list — what QuickFIX C++ and QuickFIX/J both do.
  This is the rule the FIXT table is built to; applying it to the FIX 4.4 table fixes the
  `18=2 A` defect in *Context* and is a session-behaviour change with its own row and its own
  test (see Consequences), not a side effect of B1.

Cost of the six new variants: six `accepts` arms, no allocation, no new table shape;
`field_types.rs` grows six good/bad pairs; the FIX 4.4 table does not change (none of the six
names appears in `FIX44.xml`). Cost of the four reused variants: `field_type(1185)` reports
`Data` and `field_type(277)` reports `MultipleValueString` where the XML says `XMLDATA` and
`MULTIPLESTRINGVALUE` — the *variant* is the truth the session acts on, and the two XML
spellings that name it are recorded in `from_xml` beside each other, the way
`interop_quickfix_fields.rs::same_type` already records QuickFIX's spellings.

### 2. The pair build compares variants, not spellings — and refuses everything else

ADR-0080 decision 2's rule is narrowed to: **a field defined in both files must agree on
number, on name, and on the `FieldType` variant `from_xml` gives its type name.** Two
spellings that `from_xml` maps to one variant are one type. That resolves `XmlData(213)`
with no exception and no winner: `DATA` and `XMLDATA` are both `Data`, the table says `Data`,
and `data_length_tag(213) == Some(212)` from either file.

What the build still refuses, with the field named in the message:

- the same number under two names, or the same name under two numbers (existing checks);
- two spellings that map to **different** variants (`INT` against `STRING`, `LENGTH` against
  `INT`, `CHAR` against `STRING`). A silent "first wins" here ships a `373=6` that depends on
  file order, which is the danger ADR-0080 named and it still holds for every field but this
  one — today zero fields hit it, and the check is what keeps that zero honest;
- a type name `from_xml` does not know, on either side (decision 1).

**Enum lists** (fact 6): a field enumerated in both files must have one set a **superset** of
the other; the table carries the superset. A field enumerated on one side only carries that
side's list. Two sets that each hold a value the other lacks fail the build naming the field
and both stray values. Measured: SP2 ⊇ FIXT11 on all four differing fields, so nothing fails
today and `enum_allows(1128, b"10")` and `enum_allows(1409, b"9")` are `Some(true)` from the
SP2 list. `BeginString(8)` becomes enumerated (`FIX.4.2`, `FIX.4.4`, `FIXT.1.1`) and
`MsgType(35)` gets 164 values; the session checks both earlier and by other rules
(`WrongBeginString`, `is_msg_type` → `373=11`), so the enum arm is never the first to speak
on either. Descriptions are not compared — they differ on `EncryptMethod(98)` and
`SessionRejectReason(373)` with identical value sets and the table does not carry them.

### 3. Row B2's oracle is fetched: one path added to the sparse-checkout list

`scripts/fetch-quickfix-assets.sh` line 49–52 gains one pattern, `'/src/C++/fix50sp2/'`,
after `'/src/C++/fix44/'`. Nothing else in the script changes: `WANT_DEFS`,
`WANT_MSG_LINES` and `WANT_CHECKSUM_LINES` count `test/definitions/server/fix44/` and are
untouched; the `fix44` header count echo stays; a second echo line for `fix50sp2` is
allowed and a check that the directory is non-empty is required (the script already fails
on a missing `src/C++/*.h`, so the pattern exists). Row B2's condition is rewritten from
"if `vendor/quickfix-src` has generated `src/C++/fix50sp2/`" to "`vendor/quickfix/src/C++/fix50sp2/`,
fetched by the script", and the fallback branch ("compare header order against
`FIXT11.xml` declaration order") is deleted — the stronger oracle is on disk on every
machine that ran the fetch, so a weaker one has no reason to exist. The new test reads
`../../vendor/quickfix/src/C++/fix50sp2` beside line 47's `fix44`, and **panics with the
fetch instruction when the directory is absent**, as the existing test does — a test that
skips itself is a recorded trap
([a-test-that-skipped-itself-on-every-machine-that-ran-it](../reference/a-test-that-skipped-itself-on-every-machine-that-ran-it.md)).

`src/C++/fixt11/` (10 files, 14 118 bytes) is **not** added: its `Logon.h` has no group at
all (fact 4), so it would be an oracle that says the wrong thing about the one group the FIXT
header/admin side has. Admin-message order on the FIXT table is covered by the 180 `.def`s.

Cost: 26.4 MB of blobs per fresh fetch, on top of the 6.9 MB the checkout holds today —
measured from `ls-tree -l`, not from a clock; the first CI run after the change records the
fetch step's duration before and after in the plan's delivery log.

### 4. Components merge by name; an empty declaration loses to a full one; a reference that resolves to nothing is a build error

`generate_pair` builds **one** component map from both files, and every message — admin from
`FIXT11.xml`, application from `FIX50SP2.xml` — resolves its `<component>` references against
that one map. ADR-0080 decision 2's sentence is read as "the eight admin messages come *from*
FIXT11.xml", not "are resolved *against* FIXT11.xml alone". For a name that appears in both
files:

- (a) **identical** children (same elements, same names, same `required`, same order, same
  nesting) — one definition, no message. `HopGrp` is this case (byte-identical, measured).
- (b) **one side empty** (a self-closing or childless `<component>`) and the other not — the
  non-empty one is the definition, and `build.rs` prints a `cargo:warning` naming the
  component and the file that left it empty, so the override is visible in every build log
  and not only in this ADR. `MsgTypeGrp` is this case. An empty component cannot mean
  "deliberately nothing": a reference to it resolves to zero members, which is the exact
  silent loss `CLAUDE.md` §2 item 5 forbids.
- (c) **both non-empty and different** — the build fails naming the component and the first
  differing child. Two full definitions that disagree are a real conflict and no order rule
  makes it safe.

So `NoMsgTypes(384)` **does** reach the Logon table in phase 2: `allows(b"A", 384)` is true,
`group_delimiter(b"A", 384) == Some(372)`, and `group_order(b"A", 384)` is
`[372, 385, 1130, 1406, 1131, 1410]`. That is what FIXT 1.1 says a Logon may carry and what
QuickFIX/J's dictionary says; the corpus does not exercise it, and the unit test in
`crates/dict/tests/fixt.rs` is the guard.

What the build refuses, so this class of loss cannot recur in either file:

- **a `<component>` with no children in a file, unless the other file defines it** — case
  (b) is the only way an empty component survives, and it survives as an override, never as
  a definition;
- **a `<component>` reference that resolves to zero members**, wherever it appears (message,
  group, or another component) — this extends the existing `group {name} in message {mt} has
  no members` check in `collect_groups` (line 869) one level up rather than duplicating it:
  the walk that reaches a component with nothing under it dies naming the path, so a
  self-closing `<component>` that slips past (b) still cannot produce a table. Measured
  today: zero components in FIX44 or FIX50SP2 are empty and zero groups in FIX50SP2 are
  empty, so the FIX 4.4 build is unaffected and the guard costs nothing until the day it
  fires.

### 5. One named length-field exception, and the build fails if it goes unused

The `{name}Len` / `{name}Length` rule stays the rule. `build.rs` gains a table of named
exceptions, `(data_tag, length_tag, data_name, length_name)`, holding exactly one row —
`(41874, 41873, "EncodedUnderlyingMarketDisruptionFallbackUnderlierSecurityDesc",
"EncodedUnderlyingMarketDisruptionFallbackUnderlierSecDescLen")` — consulted only when the
name rule finds nothing, and checked in both directions: an exception whose names or numbers
the XML does not carry, or that the name rule would have found anyway, fails the build. That
is the shape `interop_quickfix_fields.rs::TYPE_EXEMPTIONS` already uses (*"an exemption went
unused"*). A `tag - 1` fallback is refused: twelve SP2 `DATA`/`XMLDATA` fields do not sit at
`length + 1`, and a fallback rule is a rule that pairs the wrong field silently the next time
upstream abbreviates a name.

## Consequences

**Good**

- The pair build builds. Every one of facts 1–6 is either absorbed by a rule (1, 2, 6) or
  refused with the field, component or path named (2's mismatches, 4's empties, 5's unused
  exception), and each rule is proven by reversal in `crates/dict/tests/fixt.rs` and the
  trap page names the test.
- `373=6` on a FIXT session means the same thing it means on FIX 4.4: the dictionary's format
  rule, from the specification's text, for every typed field. This engine is stricter than
  both QuickFIX engines on all ten types — they validate none of them — and that is the
  posture ADR-0001 chose: the XML (and the spec behind it) is the truth, not QuickFIX's
  reading of it.
- `XMLDATA` fields parse correctly with an embedded `0x01`, which neither QuickFIX C++ nor
  QuickFIX/J manages at the pin. `crates/dict/tests/parse_with_real_dictionary.rs` gets a
  sibling case on `SecurityXML(1185)`.
- The Logon's `NoMsgTypes(384)` group is in the table with its delimiter and order, so a
  counterparty that sends one (the FIXT 1.1 way to negotiate per-message-type versions) is
  parsed rather than answered `373=13` for repeated `372`.
- B2 gets the real oracle, in the shape the existing test reads, on every machine that ran
  the fetch — 730 / 730 has an SP2 sibling number rather than a "header order agrees with
  the XML it came from" tautology.
- Enum superset rule costs nothing today and turns the day upstream diverges the two files
  into a red build with both values named.

**Bad — and accepted**

- **Six new variants are six more format rules nobody in the corpus exercises.** No `.def`
  sends a `TZTIMESTAMP`, `TZTIMEONLY`, `LOCALMKTTIME`, `TAGNUM`, `LANGUAGE` or
  `MULTIPLECHARVALUE` value, so their `accepts` are proven only by the good/bad pairs in
  `field_types.rs` written from the specification's text. The risk is the one
  [a-valid-field-refused-for-its-width](../reference/a-valid-field-refused-for-its-width.md)
  records: a rule written from the spec cannot see the part of the world the spec left out.
  The mitigation is that each new arm is the *loosest* reading of its sentence (case not
  checked on `Language`; fraction widths on `TzTimestamp` under the ADR-0058 rule, not a
  narrower one), and `scripts/interop.sh`'s FIXT arm (row B7) is the first real
  counterparty to disagree.
- **The per-token enum rule changes FIX 4.4 behaviour** once applied to the `Fix44` table:
  `18=2 A` stops earning `373=5`. It is spec-correct and matches both QuickFIX engines, but
  it is a session-boundary change with no `.def`, so it gets its own row in the plan
  (a test in `crates/session/tests/`, a line in `SESSION-BEHAVIOUR.md`, `CHANGELOG.md`) and
  is not folded into B1. Until that row lands, the FIXT table is built with the per-token rule
  and the FIX 4.4 table keeps the defect, and `STATUS.md` says so.
- **`field_type()` reports a variant, not the XML spelling**, for four of the ten names. A
  reader expecting `field_type(1185)` to say "XMLDATA" reads `Data`. The variant is what the
  session acts on and the rustdoc on `from_xml` lists both spellings; the alternative — ten
  variants for ten names, with `XmlData` and `Data` having identical `accepts` and the pair
  build carrying an exception for 213 — was rejected below.
- **The `cargo:warning` for `MsgTypeGrp` fires on every `fix50sp2` build**, forever, until
  upstream fills the component. That is the point — an override that is silent is the failure
  mode — but it is noise in every log, and a future reader may be tempted to suppress it.
  The warning text names this ADR.
- **26.4 MB more per fresh fetch**, for every job that runs the fetch script, for one test.
  Measured bytes, unmeasured seconds; the delivery log records the seconds when the first CI
  run lands. If it proves expensive the remedy is a second sparse pattern behind an
  environment variable read by the fetch script *and* checked by the test, not a test that
  skips.
- **The exception table in decision 5 is a list that will grow** the next time upstream
  abbreviates a name — each entry is a manual pairing checked in both directions, so the
  growth is visible, but it is still a list a human has to extend.
- **`XID` uniqueness and `XIDREF` resolution are not checked.** Both are cross-field rules a
  per-field `accepts` cannot express; no engine surveyed checks them; listed in
  `SESSION-BEHAVIOUR.md` as *not validated*, so nobody reads `373=6` coverage as covering
  them.
- **`fixt11/` headers are not fetched**, so the FIXT admin messages have no QuickFIX C++
  order oracle. Their order is asserted by the 180 `.def`s, which carry every admin message
  in the corpus but no `NoMsgTypes` group — that group's order is checked against
  `FIX50SP2.xml`'s declaration alone, the very tautology B2 avoids for application messages.
  Recorded as *not proven* in `STATUS.md` rather than hidden.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| Map all ten new names to `String` (QuickFIX's effective behaviour) | Makes `373=6` blind to 137 fields by design, and `XMLDATA` → `String` splits `SecurityXML` on an embedded SOH; the `build.rs` comment forbids exactly this fallback |
| One new variant per XML name (ten variants, `XmlData` distinct from `Data`) | `XmlData.accepts` would equal `Data.accepts` and `MultipleStringValue.accepts` would equal `MultipleValueString.accepts`; the pair build would then need a named exception for 213 whose whole content is "these two are the same". Comparing variants makes the exception unnecessary |
| Keep ADR-0080's "agree on type" rule with `XmlData(213)` as a named exception | Encodes a fact about spelling as a fact about the field; the next field spelled two ways gets a second exception. Comparing variants is the rule that generates the exception |
| "Transport file wins" or "application file wins" on any type disagreement (Artio's `putAll`, QuickFIX's `Aggregator.rb`) | Ships a table whose `373=6` depends on file order for every future disagreement, silently. The danger ADR-0080 named; refusing is the only rule that cannot be wrong |
| Keep B2's fallback (header order against `FIXT11.xml` declaration order) | Compares a table to the file it was generated from — the "round trip against your own table" D3 warns proves stability, not correctness. The real oracle costs 26 MB and one line |
| Fetch `src/C++/fixt11/` too, as the admin-message oracle | Its `Logon.h` has no `NoMsgTypes` group, so it would assert the loss decision 4 prevents |
| Resolve admin messages against the transport component map only (the literal reading of ADR-0080 decision 2) | Drops `NoMsgTypes(384)` from Logon silently — fact 4 — and nothing in 180 files notices |
| Application component map always overrides (Artio) | Correct for `MsgTypeGrp`, silent for the day two full definitions differ; case (c) must fail |
| A `tag - 1` fallback for the DATA length rule (QuickFIX C++) | Wrong for 12 SP2 fields; a fallback that pairs the wrong field parses binary content as tags |
| Skip the SP2 order test when the headers are absent | A test that skips itself on every machine that runs it — a recorded trap |
| Union of enum lists, no check | Hides the day upstream gives the two files disjoint values for one field; superset-or-fail costs the same today and says something on that day |

## Sources

- `vendor/quickfix/spec/FIXT11.xml` (Logon lines 71–96, `MsgTypeGrp` reference line 79,
  `<components>` lines 104–113, `XmlData` line 173), `vendor/quickfix/spec/FIX50SP2.xml`
  (`MsgTypeGrp` component, `XmlData` `type='XMLDATA'`, fields 41870–41879 at lines 23340–23349),
  `FIX44.xml`, `FIX50.xml`, `FIX50SP1.xml` — measured 2026-09-19 with `xml.etree` at pin
  `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`. `vendor/quickfix/spec/Aggregator.rb` (`fields()`
  overwrites `"type"`), `spec/Makefile.am` (file order), `src/C++/FixFields.h`
  (`DEFINE_DATA(XmlData)` line 54, `DEFINE_XMLDATA(SecurityXML)` line 1231, macro counts).
  `git -C vendor/quickfix ls-tree -l -r HEAD src/C++/{fix44,fix50sp2,fixt11}` for file counts and
  bytes. `grep` over `vendor/quickfix/test/definitions/server/{fix50,fix50sp1,fix50sp2}` for
  `384=`, `1128=`, `1129=`, `1156=` (0 files).
- QuickFIX C++ at the pin, read 2026-09-19:
  `src/C++/DataDictionary.cpp` `XMLTypeToType` (lines 589–680; no arm for any of the ten),
  `addFieldType(num, XMLTypeToType(type))` line 263;
  `src/C++/DataDictionary.h` `checkValidFormat` (`TYPE::Unknown: break`), `isFieldValue`
  (line 255, splits multi-value on `' '`), `isMultipleValueField` (line 322), `isDataField`
  (line 317); `src/C++/FieldConvertors.h` lines 759–794 (`StringConvertor` for `XID`,
  `XIDREF`, `LOCALMKTTIME`, `TZTIMEONLY`, `TZTIMESTAMP`, `XMLDATA`, `LANGUAGE`; `IntConvertor`
  for `TAGNUM`); `src/C++/Field.h` lines 571–606; `src/C++/FieldTypes.h` lines 740–775;
  `src/C++/Message.cpp` `setString` lines 328–329 (admin body parsed against the session
  dictionary) and `extractField` lines 587–592 (*"Assume length field is 1 less"*);
  `src/C++/fixt11/Logon.h` (no `FIX::Group(`); `src/C++/fix50sp2/NewOrderSingle.h`
  (`MsgType("D")`, 233 `FIX::Group(`) —
  <https://github.com/quickfix/quickfix/tree/386ce46e917ae494ab6e90b1be90fd421cdbe3f9/src/C++>.
- QuickFIX/J `master`, read 2026-09-19: `quickfixj-base/src/main/java/quickfix/FieldType.java`
  (no `XID`, `XIDREF`, `LOCALMKTTIME`, `TZTIMEONLY`, `TZTIMESTAMP`, `XMLDATA`, `LANGUAGE`,
  `TAGNUM`; `fromName` → `UNKNOWN`); `quickfixj-base/src/main/java/quickfix/DataDictionary.java`
  `checkValidFormat` (no `UNKNOWN` arm; `MULTIPLECHARVALUE` → `CharArrayConverter`),
  `isMultipleValueStringField` lines 526–527; `quickfixj-messages/quickfixj-messages-fixt11/src/main/resources/FIXT11.xml`
  (`MsgTypeGrp` populated at line 111, `XmlData` `DATA` at line 298);
  `quickfixj-messages/quickfixj-messages-fix50sp2/src/main/resources/FIX50SP2.xml` (`XmlData`
  `DATA` line 6213, `SecurityXML` `XMLDATA` line 9362) —
  <https://github.com/quickfix-j/quickfixj>.
- Artio `master`, read 2026-09-19:
  `artio-codecs/src/main/java/uk/co/real_logic/artio/dictionary/DictionaryParser.java` lines
  102–114 (`allFields.putAll(fields)`, `allComponents.putAll(components)`);
  `artio-codecs/src/main/java/uk/co/real_logic/artio/dictionary/ir/Field.java` (`Type` enum,
  `lookup`) — <https://github.com/artiofix/artio>.
- FIX Orchestra, *FIX Latest* EP312, `<fixr:datatypes>` — the definitions quoted under
  decision 1 for `XID`, `XIDREF`, `LocalMktTime`, `TZTimeOnly`, `TZTimestamp`, `XMLData`,
  `Language`, `TagNum`, `MultipleCharValue`, `MultipleStringValue`, `data`, `String`
  (<https://github.com/FIXTradingCommunity/orchestrations/blob/master/FIX%20Standard/OrchestraFIXLatest.xml>,
  read 2026-09-19). `fiximate.fixtrading.org`, `www.fixtrading.org`, `www.onixs.biz` and
  `btobits.com` were blocked by the egress proxy on 2026-09-19; the Orchestra file is the FIX
  Trading Community's machine-readable publication of the same text.
- Issue-tracker search, 2026-09-19, GitHub semantic search over `quickfix/quickfix` and
  `quickfix-j/quickfixj` for the `DATA`/`XMLDATA` split of `XmlData`, the empty `MsgTypeGrp`,
  and unknown SP2 type names: **nothing on any of the three**. The nearest hits —
  quickfix#302 *FIX 5.0 sp2 Missing Message Types* (admin messages absent from FIX50SP2.xml,
  which is by design of FIXT), quickfix#389 *FIX50SP2 EP269* (a regeneration request),
  quickfix#633 (Python string retrieval) — are unrelated
  (<https://github.com/quickfix/quickfix/issues/302>, <https://github.com/quickfix/quickfix/issues/389>,
  <https://github.com/quickfix/quickfix/issues/633>, read 2026-09-19).
