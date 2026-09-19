# What the three FIXT corpora say that the table cannot — three traps, and one about the diff that found them

Everything here was measured on 2026-09-19 against `vendor/quickfix/test/definitions/server/`
and `vendor/quickfix/spec/` at the pinned SHA `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`,
after plan row B4 had built every FIXT rule of ADR-0080 decision 3 and scored **173 / 180**.
Each trap names the decision that absorbs it in
[ADR-0084](../decisions/ADR-0084-a-session-message-is-checked-against-the-layer-that-defines-it-a-members-value-waits-for-the-count-and-fix50-is-its-own-oracle.md)
and the test that goes red if it comes back. The tests are named here **before** they are
written; the rows ADR-0084 decision 4 asks for write them.

[fixt-dictionary-traps](fixt-dictionary-traps.md) is about what two XML files say when merged.
This page is about what three `.def` directories say when run against that merge — the
traps a **corpus** can produce that a dictionary cannot.

`CLAUDE.md` §4: *if it cost you, write it down*, and *every recorded trap gets a regression
test*.

## Inventory

| Thing | Measured |
|---|---|
| Files per corpus | 60 / 60 / 60 |
| `fix50sp1` vs `fix50sp2`, CompID + `1137` + `BodyLength` masked | **equal on 60 / 60** |
| `fix50` vs `fix50sp2`, same mask | **differ on 1 / 60** — `21_RepeatingGroupSpecifierWithValueOfZero.def`, `336=ONE_MAIN` on a `35=d` |
| `I` lines with an admin `35=` carrying a tag `FIXT11.xml` does not define, SP2 corpus | **4**, all in `14a_BadField.def`, all expecting `373=0` (plus two garbled lines in `2d`/`3c` that never reach the scan) |
| `TradingSessionID(336)` enumerated values — FIX44 / FIX50 / FIX50SP1 / FIX50SP2 | **0 / 0 / 6 / 7** (codes `1`–`6` added FIX.5.0 EP58, `7` added SP2 EP190, FIX Orchestra) |
| Fields free in `FIX50.xml` but enumerated in `FIX50SP2.xml` (stricter) | **4** — `8`, `336`, `625`, `1048` |
| Fields where `FIX50SP2.xml` drops a value `FIX50.xml` had (stricter) | **5** — `167` (`WLD`), `327` (`D E I M P X`), `574` (`60`–`65`), `1035` (all 24), `1094` (`6`) |
| Fields enumerated in `FIX50.xml` but free in `FIX50SP2.xml` (more permissive) | **1** — `DeskOrderHandlingInst(1035)`, 24 → 0 |
| Same three rows, `FIX50SP1.xml` → `FIX50SP2.xml` | **3** (`8`, `1048`, `1324`) / **3** (`327`, `1035` all 24, `1395` all 3) / **2** (`1035` 24 → 0, `MarketUpdateAction(1395)` 3 → 0) |
| `.def` files in the three corpora carrying `1035=` or `1395=` | **0 / 180** |
| Enumerated repeating-group members in `FIX44.xml` | **110** of 511 distinct member fields; on `D`: `233`, `447`, `452`, `803`, `865` |
| Populated repeating groups in the 59 FIX 4.4 files | **1** — `14i`'s `386`, whose member `336` has no enumeration in FIX 4.4 |

## Trap 1 — a merged table has one `is_defined_tag`, and a session message has one layer

`14a_BadField.def` sends `999=HI` on a Heartbeat and expects `373=0` *Invalid tag number*. On
FIX 4.4 that is what `999` is — the highest tag is `956`. On the merged FIXT table `999` is
`LegUnitOfMeasure` from `FIX50SP2.xml`, so `is_defined_tag(999)` is `true`,
`allows(b"0", 999)` is `false`, and the session answers `373=2`. Three files, one per corpus.

QuickFIX C++ (`Session.cpp` 1225–1233) and QuickFIX/J (`Session.java` 1067–1073) validate an
admin message's body against the **transport** dictionary alone and never see `999`. That is
not a quirk: `FIX50SP2.xml` has an empty header, an empty trailer and no admin message, so a
session message exists only in `FIXT11.xml`, with `FIXT11.xml`'s 71 fields.

The `.def`'s own comment — *"not defined in the specification"* — is **false for FIX 5.0**;
`999` is defined in all three application files. The file passes upstream for a structural
reason, and the structural reason is the rule worth having: **a message is validated against
the tag set of the layer that defines it.**

ADR-0084 decision 1: `Tables::is_defined_tag_for(msg_type, tag)`; the FIXT table answers
from a second bitset over the transport file's fields for the transport file's eight message
types (`is_transport_message`, generated — **not** the session's hand-written `ADMIN` const,
which has seven entries and no `n`; no `is_admin` is generated today, whatever the plan's
traps table says), `Fix44` answers `is_defined_tag`.

> **Guarded by** `crates/dict/tests/fixt.rs` — `is_transport_tag(999) == false`,
> `is_transport_tag(1137) == true`, `is_transport_message(b"n") == true`,
> `is_transport_message(b"D") == false`, `is_defined_tag_for(b"0", 999) == false`,
> `is_defined_tag_for(b"D", 999) == true`; `crates/dict/src/tables.rs::the_trait_and_the_inherent_methods_agree`
> for `Fix44` (`is_defined_tag_for` ≡ `is_defined_tag` on `35`, `999`, `5000`); and
> `score_fixt` on `14a` ×3. Reversal sentence, measured today: `14a_BadField.def:17 Value
> { at: 1, tag: 9, expected: "98", actual: "117" }`.

## Trap 2 — a flat scan asks a member's value before it asks the count

`14i_RepeatingGroupCountNotEqual.def` declares `386=3`, sends two `336=` entries, expects
`373=16`. Under FIX 4.4 it passes because `336` is free. Under the SP2 table
`enum_allows(336, b"PRE-OPEN")` is `Some(false)`, and `scan_fields` — which walks the flat
index in wire order (D2) — answers `373=5` on the member at position 15 before
`bad_group_count` ever runs. Three files.

QuickFIX C++ never asks a member anything (`DataDictionary::iterate` walks one `FieldMap`,
no recursion). QuickFIX/J asks members everything — **after** the top-level walk, which asks
the count at the counter (`DataDictionary.java` 664–689). They disagree on *whether* and agree
on *when*.

**This is latent in FIX 4.4 now.** 110 of FIX 4.4's group-member fields are enumerated. A
`NewOrderSingle` with `453=3`, two `NoPartyIDs` entries and `447=ZZ` in one of them is
`373=5` here and `373=16` on both QuickFIX engines. No `.def` sends it. It is the second
enum question the FIX 4.4 path gets wrong with nothing to see it — the first is
`enum_allows(18, b"2 A") == Some(false)` (ADR-0083 *Context*), and the two share one plan
row.

ADR-0084 decision 2: members are still value-checked; `373=5` and `373=6` on a member are
asked after `373=1` and `373=16`.

> **Guarded by** two hand-made FIX 4.4 messages in `crates/session/tests/`: `453=3` + two
> entries + `447=ZZ` → `373=16` naming `453`; `453=2` + two entries + `447=ZZ` → `373=5`
> naming `447` (the one that turns red if members stop being checked); `score_fixt` on `14i`
> ×3; `benches/validate.rs` and `benches/alloc.rs` re-run on both tables. Reversal sentence,
> written first: `expected 373=16 371=453, engine sent 373=5 371=447`.

## Trap 3 — "the corpora differ only in `1137`" was true for 59 files of 60, and a later dictionary is stricter

ADR-0080 *Context* said the three directories differ only in CompID and `1137`. `fix50sp1`
does. `fix50` has one file, `21`, that carries `336=ONE_MAIN` on a `35=d` — and QuickFIX ran
that corpus against `FIX50.xml` (`test/setup.bat` 30–47: three `[SESSION]` blocks, one
`AppDataDictionary` each; QuickFIX/J's acceptance server maps `DefaultApplVerID=7` to
`FIX50.xml` and has no `sp1`/`sp2` directory at all), where `336` has **no** enumeration.
Under SP2 it has seven, and the SP2 table refuses the file `373=5`.

The general shape is the trap: **enumerations drift both ways between service packs.** "SP2
is a superset of SP0" (ADR-0080 decision 3) is true for fields and messages and **false
for enumerations in both directions** — nine tags on which an SP0 counterparty is refused
by an SP2 table, and one, `DeskOrderHandlingInst(1035)`, that FIX50 enumerates with 24 values
and SP2 leaves free, so the SP2 table *accepts* a value FIX50 refuses (inventory above). The
corpus found the one strict-direction file those sixty happen to touch, and can find no
permissive-direction file, because none carries `1035=` or `1395=`. The first count of the
"loses a value" row here was four, not five: the script compared sets only when the SP2 set
was non-empty, which is precisely the condition that hides a 24 → 0 field. Compare against
an empty set too.

ADR-0084 decision 3: one table in phase 2; `score_fixt` asserts `fix50` **59 / 60 with the
60th asserted by content** — file, line, `373=5`, `371=336` — so the divergence cannot rot
into a skip and cannot vanish without a visible diff. Successor: `fixt11_fix50.rs` behind a
feature `fix50`, which the generator builds today with zero new rules (measured); when it
lands the assertion is deleted and `fix50` is 60 / 60 by table.

> **Guarded by** `crates/session/tests/score_fixt.rs` — the per-corpus numbers and the
> divergence tuple, which sees the **strict** direction only (a passing file is not examined);
> and `crates/dict/tests/fixt.rs` — `enum_allows(336, b"ONE_MAIN") == Some(false)` on the SP2
> table beside `Fix44::enum_allows(336, b"ONE_MAIN") == None`, **and** the permissive side
> pinned as `enum_allows(1035, b"ZZZ") == None` and `enum_allows(1395, b"Z") == None` with the
> FIX50/SP1 counts (24, 3) in the comment — so both directions of the drift are a test and
> not a sentence, until the FIX50 table gives the second half its `Some(false)`.

## Trap 0 — the diff that said 53 files differ

The first masked diff of the three corpora reported **53** of 60 `fix50` files differing
from `fix50sp2`. The mask was `s/^E8=FIXT.1.19=[0-9]+/…/` — a regex in which `.` was meant
to match the SOH between `FIXT.1.1` and `9=`, but `FIXT.1.19=` has no character position
for it: the pattern needed `FIXT\.1\.1.9=` (or the byte). Nothing was masked, every
`BodyLength` differed by three (`TW50` vs `TW50SP2`), and every file "differed". A second
pass also missed the `E1,` prefix some files put in front of the expected line. The right
answer — one file — took three attempts, and the brief that opened ADR-0084 had already
miscounted `14i` as two files where the test output names three.

The lesson is the one [fix44-dictionary-traps](fix44-dictionary-traps.md) trap 4 already
records for XML, applied to `.def`s: **a regex over a wire format is a measurement of the
regex.** Mask with the SOH as a byte, check the mask on one file whose answer you know, and
read the failing list the test prints rather than a count somebody summarised.

> **Guarded by** nothing mechanical — a measuring mistake, not a code path. It is here so the
> next person who diffs the corpora starts from a mask that works: `s/TW50SP2/TWX/g;
> s/1137=9/1137=X/g; s/^E(1,)?8=FIXT\.1\.1.9=[0-9]+/E\18=FIXT.1.1.9=N/`, checked against
> `21` (must differ) and `14i` (must not).

## Related

- [ADR-0084](../decisions/ADR-0084-a-session-message-is-checked-against-the-layer-that-defines-it-a-members-value-waits-for-the-count-and-fix50-is-its-own-oracle.md)
  — the three decisions, with the QuickFIX C++ and QuickFIX/J line numbers behind each.
- [ADR-0080](../decisions/ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
  decisions 2 and 3 — the sentences trap 3 corrects.
- [ADR-0083](../decisions/ADR-0083-ten-field-types-one-field-spelled-two-ways-one-empty-component-and-where-the-sp2-oracle-comes-from.md)
  — the pair build these corpora run against, and the first latent enum defect.
- [fixt-dictionary-traps](fixt-dictionary-traps.md) — the dictionary-side sibling.
- [a-test-that-skipped-itself-on-every-machine-that-ran-it](a-test-that-skipped-itself-on-every-machine-that-ran-it.md)
  — why trap 3 asserts a divergence rather than excluding a file.
