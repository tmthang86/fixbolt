# What `FIXT11.xml` + `FIX50SP2.xml` actually say — six traps, each with the test that guards it

Everything here was measured on `vendor/quickfix/spec/` at the pinned SHA
`386ce46e917ae494ab6e90b1be90fd421cdbe3f9` on **2026-09-19**, with `xml.etree`, not a regular
expression ([fix44-dictionary-traps](fix44-dictionary-traps.md) trap 4 says why). Each trap
names the decision that absorbs it in
[ADR-0083](../decisions/ADR-0083-ten-field-types-one-field-spelled-two-ways-one-empty-component-and-where-the-sp2-oracle-comes-from.md)
and the test that goes red if it comes back. The tests are named here **before** they are
written; plan row B1 writes them.

`CLAUDE.md` §4: *if it cost you, write it down*, and *every recorded trap gets a regression
test*. This page is the FIXT sibling of the FIX 4.4 page; the seven traps there still apply
to `FIX50SP2.xml` (the generator is the same), and the ones below are the ones that only a
**pair** of files can produce.

## Inventory

| Thing | `FIXT11.xml` | `FIX50SP2.xml` |
|---|---|---|
| `<field>` definitions | **71** — every one also in SP2 | **6 028** |
| Type names used | **8** — all known to FIX 4.4 | **32** — FIX 4.4's 23 minus `MULTIPLEVALUESTRING`, plus ten |
| `<message>` definitions | **8**, all `msgcat='admin'`; `XMLnonFIX` self-closing | **156**, none empty, none admin |
| `<component>` definitions | **2** — `HopGrp`, `MsgTypeGrp` (**empty**) | **725**, none empty |
| `<group>` with no children | 0 | 0 |
| `type='DATA'` fields | 7 | **75**, one without a `{name}Len`/`{name}Length` partner |
| `type='XMLDATA'` fields | 0 | **8**, all with a `{name}Len`/`{name}Length` partner |
| `DATA`/`XMLDATA` fields not at `length + 1` | — | **12** (QuickFIX C++ assumes `tag - 1`) |
| Shared fields agreeing on number + name + type spelling | **70 / 71** | |
| Shared fields whose enum sets differ | **4** — `8`, `35` (SP2 only), `1128`, `1409` (SP2 superset) | |
| `.def` files carrying `384=`, `1128=`, `1129=` or `1156=` | **0 / 180** | |

## Trap 1 — ten type names the FIX 4.4 table never met

`LOCALMKTTIME` (45 fields), `XID` (34), `XIDREF` (29), `MULTIPLECHARVALUE` (8), `XMLDATA` (8),
`TZTIMEONLY` (6), `MULTIPLESTRINGVALUE` (4), `TZTIMESTAMP` (1), `LANGUAGE` (1), `TAGNUM` (1).
`crates/dict/build.rs` dies on the first unknown name — correctly: the alternative is a
`373=6` that is silently blind to 137 fields. Neither QuickFIX C++ nor QuickFIX/J maps any of
the ten (`XMLTypeToType` → `TYPE::Unknown`; `FieldType.fromName` → `UNKNOWN`), so both engines
validate none of them. That is not an oracle for "string is fine"; it is two engines that
never wrote the arm.

ADR-0083 decision 1 gives each a variant and a format rule, six of them new.

> **Guarded by** `crates/dict/tests/field_types.rs` — the `29` count assertion (a thirtieth
> name upstream is red, not `String`), one good/bad pair per new variant written from the
> specification's sentence, and `crates/dict/tests/fixt.rs::every_sp2_field_has_a_type`
> (`field_type(tag).is_some()` for all 6 028).

## Trap 2 — the same field, spelled two ways by the same project

```xml
<!-- FIXT11.xml line 173 -->  <field number='213' name='XmlData' type='DATA' />
<!-- FIX50SP2.xml -->         <field number='213' name='XmlData' type='XMLDATA' />
```

Of the 71 fields in both files, 70 agree on number, name and type; `XmlData(213)` differs on
type alone, and it is `DATA` in every other vendored XML (`FIX40`…`FIX50SP1`). A rule that
compares type *spellings* fails the build on a field the specification defines the same way
under both names (*"XMLData fields are always immediately preceded by a length field"* — the
delimiting rule of `data`). QuickFIX's own aggregate generator hides the split by last-write-wins
(`spec/Aggregator.rb` overwrites `"type"`; `FIXT11.xml` is last in `Makefile.am`), which is why
`FixFields.h` carries `DEFINE_DATA(XmlData)` next to `DEFINE_XMLDATA(SecurityXML)`. QuickFIX/J
edited its copies to say `DATA` in both. No issue in either tracker mentions it (search
2026-09-19).

ADR-0083 decision 2: the pair build compares the **variant** `from_xml` returns, not the
string. `DATA` and `XMLDATA` are both `Data`; `INT` against `STRING` still fails.

> **Guarded by** `crates/dict/tests/fixt.rs::xml_data_is_one_data_field_from_two_spellings`
> — `field_type(213) == Some(Data)`, `field_type(1185) == Some(Data)`,
> `data_length_tag(213) == Some(212)`, `data_length_tag(1185) == Some(1184)`; and its
> reversal, a fixture pair in `crates/dict/tests/fixtures/` where one file types a field `INT`
> and the other `STRING`, which must fail `generate_pair` naming the field
> (`crates/dict/tests/fixt.rs::a_real_type_disagreement_still_stops_the_build`).

## Trap 3 — a shared `<component>` is empty on one side, and the Logon references it

```xml
<!-- FIXT11.xml line 79, inside Logon -->  <component name='MsgTypeGrp' required='N' />
<!-- FIXT11.xml line 112 -->               <component name='MsgTypeGrp' />
```

`FIX50SP2.xml`'s `MsgTypeGrp` holds `NoMsgTypes(384)` with six members. A `generate_pair`
that resolves an admin message's components against the *transport* file's map walks the
empty element, finds nothing, and emits a Logon table with no `384` group — no delimiter, no
order, no `allows(b"A", 384)`. **Nothing errors**, because an empty component walks to nothing,
and **nothing in the corpus notices**, because 0 of 180 `.def` files carry `384=`. It is
`fix44-dictionary-traps.md` trap 4 (*a self-closing element does not fail to match*) one level
down, on `<component>` instead of `<message>`.

QuickFIX C++ never merges the files and so never asks — and its generated `fixt11/Logon.h` at
the pin has no group at all, while `Message.cpp` parses an admin body against the session
dictionary, so a Logon carrying `NoMsgTypes` reaches it as repeated flat tags. QuickFIX/J's
`FIXT11.xml` has the component filled in. Artio merges with `HashMap.putAll` — application
file wins, silently, for every component, full or empty.

ADR-0083 decision 4: one component map; identical definitions merge; an empty one loses to
a full one **with a `cargo:warning`**; two full ones that differ fail the build; any
reference that resolves to zero members fails the build.

> **Guarded by** `crates/dict/tests/fixt.rs::the_logon_carries_no_msg_types_from_the_application_file`
> — `group_delimiter(b"A", 384) == Some(372)`, `group_order(b"A", 384) == [372, 385, 1130,
> 1406, 1131, 1410]`, `allows(b"A", 384)`; reversal: resolve admin messages against the
> transport map only → red on the delimiter. And
> `crates/dict/tests/fixt.rs::a_component_that_resolves_to_nothing_stops_the_build` on a
> fixture pair where the name is empty in **both** files.

## Trap 4 — one DATA field's length field is abbreviated

```xml
<field number='41873' name='EncodedUnderlyingMarketDisruptionFallbackUnderlierSecDescLen' type='LENGTH' />
<field number='41874' name='EncodedUnderlyingMarketDisruptionFallbackUnderlierSecurityDesc' type='DATA' />
```

`Security` became `Sec` in the length field's name and nowhere else. The `{name}Len` /
`{name}Length` rule that pairs all 16 FIX 4.4 DATA fields, 74 of the 75 SP2 DATA fields and
all 8 XMLDATA fields finds nothing for this one and the build dies. The obvious fallback —
`tag - 1`, QuickFIX C++'s own rule — is wrong for **12** SP2 `DATA`/`XMLDATA` fields (the
three `…PaymentStreamFormula` fields sit behind `NoLegPaymentStreamFormulas`-style counters;
`EncodedWarningText(2521)` behind `WarningText`; and so on).

ADR-0083 decision 5: one named exception, consulted only after the name rule fails, and the
build fails if the exception goes unused or names a pair the XML does not carry.

> **Guarded by** `crates/dict/tests/fixt.rs::the_one_abbreviated_length_field_is_paired_by_name`
> — `data_length_tag(41874) == Some(41873)`; and `tables.rs::signature_length_is_not_the_preceding_tag`'s
> SP2 sibling, `data_length_tag(2521) == Some(2520)`-style assertions for the 12 fields not at
> `length + 1`, so a `tag - 1` fallback cannot creep in green.

## Trap 5 — the enum lists of a shared field are not the same list

Nine of the 71 shared fields carry `<value>`s in both files. `ApplVerID(1128)`: SP2 adds
`10`. `SessionStatus(1409)`: SP2 adds `9` and `10`. `BeginString(8)` and `MsgType(35)` carry
values in SP2 only (3 and 164). ADR-0080 decision 2 spoke of number, name and type and said
nothing about values, so a `generate_pair` that takes the first list it sees ships
`enum_allows(1128, b"10") == Some(false)` — `373=5` on a value the specification defines.

ADR-0083 decision 2: the table carries the superset; two sets that each hold a value the
other lacks fail the build naming both.

> **Guarded by** `crates/dict/tests/fixt.rs::a_shared_enum_carries_the_superset` —
> `enum_allows(1128, b"10") == Some(true)`, `enum_allows(1409, b"9") == Some(true)`,
> `enum_allows(8, b"FIXT.1.1") == Some(true)`; reversal on a fixture pair with disjoint
> values → red naming the field.

## Trap 6 — a multi-value enum is matched whole (FIX 4.4 too)

Found while deciding what `MULTIPLECHARVALUE` means. The generated `enum_allows` answers
`list.contains(&value)` for the whole value, so `18=2 A` — two legal `ExecInst` values, the
form the specification's own example uses — is `Some(false)` on the **FIX 4.4** table today,
and the session answers `373=5`. QuickFIX C++ (`DataDictionary.h` `isFieldValue`) and
QuickFIX/J (`DataDictionary.java` `isMultipleValueStringField`) both split on spaces first.
0 of 239 `.def` files send a multi-value field, which is why 59 / 59 never saw it.

ADR-0083 decision 1 makes the per-token rule the FIXT table's rule and routes the FIX 4.4
change to its own plan row, because it is a session-boundary change with no `.def`.

> **Guarded by** `crates/dict/tests/field_types.rs::a_multi_value_enum_is_checked_token_by_token`
> — `enum_allows(18, b"2 A") == Some(true)`, `enum_allows(18, b"2 ZZ") == Some(false)` —
> once the FIX 4.4 row lands; on the FIXT table from B1 on.
