# QuickFIX drops deeply nested fields from its own `message_order`

`[measured 2026-09-19]` against QuickFIX at pin `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`.

## The claim that did not transfer

`crates/dict/tests/interop_quickfix_order.rs` checks this crate's FIX 4.4 group tables against
QuickFIX's generated C++ on three claims, and has read `730 / 730` on all three since 2026-08-28:

1. every group's **delimiter** agrees exactly;
2. QuickFIX's order is an exact **subsequence** of this crate's;
3. every tag this crate lists and QuickFIX does not **is itself a group counter** — i.e. the
   only thing this crate adds is the nesting hook QuickFIX omits.

Step B2 pointed the same three claims at FIX 5.0 SP2. Claims 1 and 2 hold on **all 25 927**
SP2 groups, zero exceptions. **Claim 3 is false**, on 941 distinct tags across 64 094
occurrences — and the cause is QuickFIX's generator, not this crate's table.

`[corrected 2026-09-19]` those two numbers first read **1 307** and **64 750**, and both
were wrong: the oracle walked `read_dir` and kept whichever header arrived first, so it
had four answers at one pin. See
[a-test-oracle-that-reads-read-dir-has-more-than-one-answer](a-test-oracle-that-reads-read-dir-has-more-than-one-answer.md).

## The proof, from QuickFIX's own header

`TradeCaptureReport`'s `NoSides(552)` group. Expanding `FIX50SP2.xml` recursively — outside
`build.rs` and outside the test's own parser, with a from-scratch script — gives **162** tags,
`OrderQty(38)` among them, reached by `TrdCapRptSideGrp → TradeReportOrderDetail →
OrderQtyData`. This crate's `G402` is those same 162.

`vendor/quickfix/src/C++/fix50sp2/TradeCaptureReport.h` says both of these at once:

```
6417:      FIELD_SET(*this, FIX::OrderQty);      ← the class HAS the field
```
```
$ grep -o "Group(552,[^)]*)" TradeCaptureReport.h | tr ',' '\n' | grep -c '^38$'
0                                                ← the ORDER TABLE omits it
```

Its `message_order(552, …)` carries **144** tags against the XML's 162. QuickFIX's own
generated C++ contradicts itself: the group class sets a field that the group's order table
does not mention.

The same shape, with a different cause, on `LegSecurityXML(1872)` and 23 fields like it — a
`<component>` wrapping a `LENGTH` + `XMLDATA` + `STRING` triple. `IOI.h` sets
`FIX::LegSecurityXML` three times and never lists `1872` in any `message_order` in the file.

The common factor is `<component>` nesting **two or more levels deep inside a `<group>`**. FIX
4.4 barely has that shape, which is why 730 / 730 never saw it; SP2 is full of it.

## Why this is not a defect here

A field this crate lists and QuickFIX omits is a field the **XML** puts in the group. Dropping
it to match QuickFIX would be `CLAUDE.md` §2 item 5's failure mode — field order coming from
something other than the generated tables — and would mean refusing, or misordering, a message
a counterparty is entitled to send. `DESIGN.md` D3 makes the XML the source of truth and
QuickFIX a *second opinion*; this is the case where the second opinion is incomplete.

Note what is **not** claimed: that QuickFIX is broken at run time. Its `DataDictionary` reads
the same XML this crate does, so a QuickFIX *engine* validates these fields fine.
`message_order()` in the generated convenience classes is a narrower thing, and it is only
that narrower thing this oracle reads.

## What the test does instead

`crates/dict/tests/fixt_order.rs` keeps claims 1 and 2 as hard assertions, unweakened, and
splits claim 3 into two **pinned counts** rather than one boolean:

| Number | Meaning |
|---|---|
| `checked == 25_927` | every group in the generated SP2 headers |
| `nested_counter_extras == 226` | the FIX 4.4 kind — extras that *are* group counters |
| `dropped_field_extras == 941` | distinct plain fields QuickFIX's generator drops |
| `dropped_field_occurrences == 64_094` | how often, across all groups |

All four are asserted, not printed, so a change in this crate's table **or** in QuickFIX's
generator turns the test red and makes someone look. A count pinned is weaker than an
invariant proved, and the test's own doc says so: it is the strongest claim the oracle can
still carry.

## The trap in one line

**An oracle that agrees 730 / 730 on one dictionary has not been validated for another.** The
three claims were written for FIX 4.4's nesting depth; two survived the move to SP2 and one did
not, and nothing about the first result predicted which.

## What guards it

`crates/dict/tests/fixt_order.rs`, both tests, under `--features fix50sp2`. Proven by reversal:
swapping two adjacent members of `G402` makes claim 2 red naming `(AE, 552)` — predicted before
the run, observed exactly, restored byte-identical.

## Related

- [ADR-0083](../decisions/ADR-0083-ten-field-types-one-field-spelled-two-ways-one-empty-component-and-where-the-sp2-oracle-comes-from.md)
  decision 3 — why the SP2 headers are fetched at all. This qualifies that decision's value
  without overturning it: the oracle still proves delimiters and ordering, which is what D3
  asks of it.
- [ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md) — QuickFIX is data and a test
  oracle, never source.
- [a-documentation-cell-is-not-a-specification](a-documentation-cell-is-not-a-specification.md)
  — the other trap where a secondary artefact was read as authority.
