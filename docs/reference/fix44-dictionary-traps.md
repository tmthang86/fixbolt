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
