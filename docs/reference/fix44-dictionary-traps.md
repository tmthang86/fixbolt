# What `FIX44.xml` actually says — four traps, each with the test that guards it

Everything here was measured on `vendor/quickfix/spec/FIX44.xml` on **2026-08-28**, with an
XML parser rather than a regular expression — see trap 4 for why that distinction cost an
hour. Each trap is followed by the test that fails if it comes back.

`CLAUDE.md` §4: *if it cost you, write it down*, and *every recorded trap gets a regression
test*.

## Inventory

| Thing | Count |
|---|---|
| `<field>` definitions | **912** |
| `<message>` definitions | **93** — one of which, `XMLnonFIX` (`35=n`), is empty |
| `<component>` definitions | **104** |
| Deepest component nesting | **5** |
| Header tags, groups descended | **30** (26 direct fields + `NoHops` and its 3 members) |
| Trailer tags | **3** — `SignatureLength(93)`, `Signature(89)`, `CheckSum(10)` |
| `type='DATA'` fields | **16** |
| `type='LENGTH'` fields | **18** — 16 lengths of DATA, plus `BodyLength(9)` and `MaxMessageSize(383)` |
| Messages whose required-field set changes if `<component>` is descended into | **21 / 93** |
| `<group>` declarations | **93** — **1** under `<messages>`, **91** under `<components>`, **1** under `<header>` |
| Distinct group counter tags | **59** — 58 in messages, plus `NoHops(627)` from the header |
| Group positions once components are expanded | **731** |
| Counters whose delimiter depends on the message | **4** — `268`, `124`, `295`, `420` |

## Trap 1 — a DATA field's length field is not `tag - 1`

The obvious rule holds for 15 of the 16 DATA fields, which is exactly what makes it
dangerous. It fails for one:

| DATA field | `tag - 1` says | The dictionary says |
|---|---|---|
| `Signature(89)` | `88` — `NoDlvyInst`, an unrelated field | **`SignatureLength(93)`** |

Every other DATA field agrees with the arithmetic, so a generator written to the pattern
produces a table that is 15/16 correct and silently mis-parses every signed message. The
value of a DATA field may contain `0x01`; getting its length wrong does not produce a bad
field, it produces a bad *frame*, and every field after it is garbage.

**Match by name.** For a DATA field `X`, the length field is `XLen` or `XLength`. That
resolves all 16, including `Signature` → `SignatureLength`.

`crates/dict/build.rs` refuses to generate the table at all if a DATA field cannot be matched
this way, rather than emitting `None` for it. `None` would mean "scan for `0x01`", which is
the wrong answer expressed as a default.

> **Guarded by** `crates/dict/tests/tables.rs::signature_length_is_not_the_preceding_tag`.
> Verified by reversal: replacing name matching with `tag - 1` makes it fail with
> `left: Some(88), right: Some(93)`, and no other test moves.

## Trap 2 — `required='Y'` on a `<component>` says nothing about the fields inside it

`NewOrderSingle` declares `<component name='Instrument' required='Y'/>`. Every single field
inside `Instrument`, `Symbol(55)` included, is `required='N'`.

So a required component can contribute **zero** required fields. "The message requires an
Instrument" and "the message requires a Symbol" are different statements, and only the first
one is in the dictionary.

This is what makes the FIX 4.4 required-field set smaller than intuition suggests:

```
NewOrderSingle(D) required fields = [11, 40, 54, 60]
                                     ClOrdID, OrdType, Side, TransactTime
```

`HandlInst(21)` and `Symbol(55)` are **not** among them, with or without component recursion.
Both are `required='N'`. An earlier version of the codec plan asserted they would be, on the
strength of the FIX specification rather than of this file.

> **Guarded by** `tables.rs::required_fields_of_new_order_single`, which pins the exact set.

## Trap 3 — `<header>` contains a `<group>`, not only `<field>`

The header holds 26 `<field>` children and one `<group>`: `NoHops(627)`, whose members are
`HopCompID(628)`, `HopSendingTime(629)` and `HopRefID(630)`. All four are header fields.

A generator that iterates only direct `<field>` children builds a 26-entry table. The four
missing tags then classify as **body** fields, and an outbound message carrying hops would
order them after the body's low-numbered tags instead of within the header — a positional
mismatch, which is what the acceptance comparator checks (D3, non-negotiable 5).

**Nothing in the acceptance suite would catch it.** Tags 627–630 appear **0 times** across the
59 FIX 4.4 definitions. 59/59 stays green with this wrong.

> **Guarded by** `tables.rs::header_group_and_its_members_are_header_fields`.

**Still open:** the three `<trailer>` tags are classified as neither header nor body.
`is_header(89)` and `is_header(93)` return `false`, so a written `Signature` would sort into
the body. Nothing writes one today — no `.def` carries a signature, and CheckSum is emitted
explicitly last by the template — so this is recorded rather than fixed.
`tables.rs::trailer_fields_are_not_classified_and_that_is_a_known_gap` pins the current
behaviour so the day something signs a message, a test explains why it went wrong.

## Trap 4 — a `<message>` element can be self-closing

```xml
<message name='XMLnonFIX' msgtype='n' msgcat='admin' />
```

It is the only one of the 93, and it has no body at all.

A regular expression of the shape `<message ...>(.*?)</message>` does not match it, and worse,
**it does not fail to match — it matches the wrong thing.** The pattern starts at `XMLnonFIX`
and runs to the *next* `</message>`, swallowing `RegistrationInstructions` and attributing its
body to `XMLnonFIX`. The count comes back 92 instead of 93, one message vanishes, and another
gets a body that is not its own.

This was found only because 92 disagreed with a figure already recorded elsewhere. Nothing
about the output looked wrong.

**Use an XML parser.** `crates/dict/build.rs` uses `roxmltree`; the analysis in this document
was redone with `xml.etree` after the regex was caught. Every count above changed.

> **Guarded by** the generator itself: `msg_type::XM_LNON_FIX` is emitted, and the build fails
> if two messages collide on a constant name. A regex-based generator would have dropped one.

## Trap 5 — 58 of the 59 group counters are unreachable without descending into `<component>`

**Measured 2026-08-28** against `vendor/quickfix/spec/FIX44.xml`.

Of the 93 `<group>` declarations in the file, **1** sits under `<messages>`, **91** under
`<components>`, and **1** under `<header>`. A generator that walks the children of each
`<message>` — including nested groups, but not entering component references — finds
exactly **one** counter: `NoMsgTypes(384)` in Logon.

This is not "a few edge cases missed". `NoTradingSessions(386)` reaches NewOrderSingle only
through `TrdgSesGrp`; `NoPartyIDs(453)` reaches almost everything only through `Parties`;
`NoMDEntries(268)` reaches the market-data messages only through `MDFullGrp` /
`MDIncGrp`. Descending into components is not an optimisation of the group walker — it is
the group walker.

The failure is quiet in the worst way. The tables generate, the crate compiles, every test
that does not name a group passes, and every repeating group in every real application
message is invisible. Nothing in the 59 acceptance definitions notices, because
[the suite populates one group and does it to test a wrong count](quickfix-acceptance-def-format.md).

**Guarded by** `crates/dict/tests/group_tables.rs`:
`a_group_reached_through_a_component_is_still_found` pins `(D, 386) → 336`, a pair that
exists only through a component, and `the_tables_cover_what_the_dictionary_declares` pins
the totals. Proven by reversal on 2026-08-28: removing the component descent from
`collect_groups` drops `GROUP_COUNTERS` from 59 to 2 and turns 4 of the 6 tests red.

## Trap 6 — four group counters take a different delimiter in different messages

**Measured 2026-08-28.** A group ends when a tag outside its member set appears, and the
*delimiter* — the tag that starts each entry — is the group's first declared field. Four
counters in FIX 4.4 are declared with different first fields in different messages:

| Counter | Message | Delimiter |
|---|---|---|
| `NoMDEntries(268)` | `W` MarketDataSnapshotFullRefresh | `MDEntryType(269)` |
| `NoMDEntries(268)` | `X` MarketDataIncrementalRefresh | `MDUpdateAction(279)` |
| `NoExecs(124)` | `J` AllocationInstruction | `LastQty(32)` |
| `NoExecs(124)` | `BA` CollateralReport | `ExecID(17)` |
| `NoQuoteEntries(295)` | `Z` QuoteCancel | `Symbol(55)` |
| `NoQuoteEntries(295)` | `i` MassQuote | `QuoteEntryID(299)` |
| `NoBidComponents(420)` | `k` BidRequest | `ListID(66)` |
| `NoBidComponents(420)` | `l` BidResponse | `Commission(12)` |

So a table keyed by counter alone answers half of these wrongly, and `268` is the painful
one: MarketDataIncrementalRefresh is the highest-volume message type in production FIX, and
a wrong delimiter mis-cuts every entry in it while parsing without error.

**Guarded by** `the_four_ambiguous_counters_resolve_by_message`. Proven by reversal on
2026-08-28: re-keying the generated table by counter alone makes `(X, 268)` answer `269`
instead of `279`, and makes `(D, 268)` — a message with no market data at all — answer
`Some(269)` instead of `None`.

## Trap 7 — QuickFIX's `message_order` and a group's member list are not the same list

**Measured 2026-08-28.** QuickFIX ships generated C++ for FIX 4.4, one header per message,
and every group appears in it as

```cpp
NoMDEntries() : FIX::Group(268,279,FIX::message_order(279,285,269,278,280,55,...,0)) {}
```

counter, delimiter, order. That is a second opinion on field order written by a different
generator, and comparing against it caught nothing — which is the result worth recording.

The comparison is **not** equality, and expecting equality wastes an afternoon. QuickFIX's
`message_order` lists a nested group's counter tag only sometimes; this crate's
`group_members` always does, because its writer walks the list and emits the nested group
when it reaches that counter, so the counter has to be in the list. Measured across the 730
groups QuickFIX generates:

| Claim | Result |
|---|---|
| Delimiter agrees | **730 / 730** |
| QuickFIX's order is an exact subsequence of this crate's | **730 / 730** |
| Tags QuickFIX has that this crate lacks | **0** |
| Tags this crate has that QuickFIX omits | 7 distinct, **every one a group counter** |

QuickFIX generates one file per message and `<header>` is not a message, so `NoHops(627)` has
no file: 730 + 1 = 731.

**Guarded by** `crates/dict/tests/interop_quickfix_order.rs`. Proven by reversal on
2026-08-28, and the reversal is the point: swapping two adjacent members in every generated
group leaves `crates/codec/tests/group_roundtrip.rs` **green** — that test generates its
messages from the same table the encoder reads, so a wrong order is invisible to it — and
turns the interop test **red**. A round-trip against your own table proves stability, not
correctness.

## There is no user-defined tag range, whatever `FieldNumbers.h` says

`[measured 2026-08-28]` QuickFIX's own `src/C++/FieldNumbers.h` declares:

```cpp
const int NormalMin = 1;    const int NormalMax = 4999;
const int UserMin   = 5000; const int UserMax   = 9999;
const int InternalMin = 10000;
```

So 5000–9999 reads as *user-defined and therefore acceptable*. But
`14a_BadField.def` sends `5000=HI` and expects:

```
58=Invalid tag number|371=5000|373=0
```

In the acceptance configuration, **"defined" means "in `FIX44.xml`", and nothing
else.** A dictionary that carves out a user range fails that definition and no
other definition would notice, because 5000 is the only tag in the range the
corpus ever sends.

**Guarded by** `crates/dict/tests/interop_quickfix_fields.rs`, whose negative
half refuses all 5 168 field names QuickFIX knows whose tag FIX 4.4 does not
define — 5000 among them.

## QuickFIX's own generator disagrees with the XML about 14 field types

`[measured 2026-08-28]` `src/C++/FixFields.h` is generated once for **every** FIX
version, so it carries the type each field ended up with in the latest one:

| Tag | Name | `FIX44.xml` | QuickFIX |
|---|---|---|---|
| 10 | CheckSum | STRING | CHECKSUM |
| 18 | ExecInst | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 63 | SettlType | CHAR | STRING |
| 276 | QuoteCondition | MULTIPLEVALUESTRING | MULTIPLESTRINGVALUE |
| 277 | TradeCondition | MULTIPLEVALUESTRING | MULTIPLESTRINGVALUE |
| 286 | OpenCloseSettlFlag | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 291 | FinancialStatus | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 292 | CorporateAction | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 529 | OrderRestrictions | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 532 | MassCancelRejectReason | STRING | INT |
| 546 | Scope | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 587 | LegSettlType | CHAR | STRING |
| 674 | LegAllocAcctIDSource | STRING | INT |
| 877 | UnderlyingCPProgram | STRING | INT |

The other **898 agree exactly**. ADR-0001 makes the XML the source of truth, so
this crate follows the XML — but the difference is written down and counted
rather than papered over with a loose comparison. **A fifteenth is a test
failure**, which is the whole point of writing the fourteen out.

**Guarded by** `interop_quickfix_fields.rs::every_field_type_agrees_with_quickfix_or_is_a_named_exemption`.

## Reading `FixValues.h` with the wrong regex makes a good oracle look useless

`[measured 2026-08-28]` The plan for the validation tables recorded that
QuickFIX's enum table was a **weak** oracle: 228 of 245 fields covered, 95 of
them differing in count. That measurement was wrong, and the cause was one line
of throwaway scouting code.

`FixValues.h` writes character enums and string enums differently:

```cpp
const char OrdType_MARKET = '1';                        // char
const char SecurityType_CORPORATE_BOND[] = "CORP";      // char ARRAY
```

A pattern expecting `Name = literal` finds the first and misses the second. The
array form is exactly the 17 fields that looked uncovered — `SecurityType(167)`
among them, which is the field `14e_IncorrectEnumValue.def` actually tests. So
the one field the corpus exercises was the one the bad parse hid.

Read properly, the oracle covers **245 / 245 fields and 1 708 / 1 708 values,
with zero exceptions**.

The lesson is not about regexes. **A scouting script that under-reports makes an
oracle look weak, and "the oracle is weak" is an argument for testing less.** It
went into a plan and was approved on that basis. Anything a plan claims about
how much evidence is available has to be re-derived by the test that will rely
on it, not carried over from the scout.

**Guarded by** `crates/dict/tests/enums.rs::the_array_form_is_read_and_not_skipped`,
which asserts `SecurityType` parses to more than 50 values.

## `SEQNUM` accepts zero, and the rule that said otherwise was invented

`crates/dict/src/field_type.rs` refused `34=0` as a `SEQNUM`, on the reasoning
that a sequence number counts from 1. The comment cited
`11c_NewSeqNoLess.def` as evidence. **It had misread the file.**

`11c` sends `35=4|34=0|…|36=1` and expects back
`45=0|58=Value is incorrect (out of range) for this tag|372=4|373=5`. That is
`373=5` — a value out of range — with **no `371=`** naming a tag. A rejected
`34=0` would have been `373=6` with `371=34`. The fault the file is testing is
`36=1`, a `NewSeqNo` lower than the sequence number already reached; the `34=0`
beside it is a field QuickFIX processes without comment, because a plain
`SequenceReset` has no meaningful sequence number and QuickFIX's own
`SEQNUM_CONVERTOR` is a plain integer parser.

`[measured 2026-08-29]` restoring the rule takes the acceptance score from
**37 / 59 to 34**: `11a`, `11b` and `11c` all send `34=0` and all three answer
with a Reject where the file expects a Heartbeat.

**The shape of the mistake is what to remember.** The refused case lived in
`crates/dict/tests/field_types.rs`, in the block whose own doc comment says
"these cases are written by hand, not taken from a capture". An invented case
that agrees with an invented rule is two statements of the same guess, and it
reads exactly like a test.

`Length` and `NumInGroup` refuse a negative on the same reasoning and nothing in
the corpus sends one, so they stay refused and are marked `[unproven]` in the
source. QuickFIX would accept them.

## Fifteen of the sixteen DATA pairs are `length == data - 1`, and the sixteenth is not

`[measured 2026-08-30]` FIX 4.4 declares **16 DATA fields**, each taking its length from a
separate field that must sit immediately in front of it on the wire. Fifteen of them number
the length one below the data:

```
91 <- 90    96 <- 95    213 <- 212   349 <- 348   351 <- 350   353 <- 352
355 <- 354  357 <- 356  359 <- 358   361 <- 360   363 <- 362   365 <- 364
446 <- 445  619 <- 618  622 <- 621
```

The sixteenth is **`Signature(89)`, whose length is `SignatureLength(93)`** — already recorded
above as the trap that breaks a `tag - 1` rule on the read path. On the **write** path it costs
something different and worse: an encoder that sorts body fields by ascending tag, which is
what non-negotiable 5 requires, emits `89=` *before* `93=`. Fifteen pairs come out right by
arithmetic accident and the sixteenth comes out unframable.

**Why this is easy to ship and hard to notice.** Nothing in the corpus carries a DATA message,
so the acceptance gate cannot see it; the round-trip test skipped DATA members with a comment
saying they were "a different test", and they were not tested anywhere; and the fifteen lucky
pairs make any spot-check pass. The defect is visible only in the one pairing nobody reaches
for when writing an example.

The fix is not a special case for 89. Sorting a DATA field **by its length field's tag, one
place behind it** puts every pair right whatever the numbers are, and leaves the ascending rule
intact for everything else. `crates/codec/src/template.rs::key` does that;
`crates/codec/tests/data_encode.rs` holds the `Signature` case by name.

**In repeating groups the order was already right — and that is not the same as tested.**
`[measured 2026-08-30]` **66 DATA members appear across the group tables, and all 66 have their
length declared immediately in front**, because FIX 4.4's XML declares them adjacent. What was
missing there was enforcement: nothing required the pair to be supplied together, and nothing
stopped a caller stating a wrong length. `group_roundtrip.rs` now writes **508 DATA members**,
each with a `0x01` inside its value — a DATA value without one is indistinguishable from an
ordinary field and would make the whole case vanish.

`[to testing-skills → [PR #2](https://github.com/tmthang86/testing-skills/pull/2), open]` — *fifteen out of sixteen is what a lucky fixture looks like.* A rule
derived from a set where almost every member satisfies it by coincidence will pass every
example anyone writes. The defence is to enumerate the set and check the rule against all of
it — which took one script here and would have found this on day one.
