# ADR-0084 — A session message is checked against the layer that defines it, a group member's value waits for the count, and `fix50` is its own oracle

- **Status**: Accepted — 2026-09-19 — **decision 2 amended by [ADR-0085](ADR-0085-a-member-waits-for-a-counter-that-came-before-it-and-the-array-is-only-a-cache.md)**, which narrows what "member" means and fixes the overflow path. Substance untouched, per §5.
- **Approved by**: the PR B manager session under the owner's blanket delegation
  ("uỷ quyền toàn bộ cho bạn", 2026-09-19). The owner did not read it. Every figure in
  *Context* was re-measured by the manager independently of the architect before this line
  moved — including the drift lists, which the manager found undercounted on the first
  draft and sent back (item 13).
- **Date**: 2026-09-19
- **Deciders**: Tran Manh Thang. Proposed by the architect on 2026-09-19 from measurements the
  PR B manager and developer had already made and the architect re-made; nobody has read it
  yet. Every number below carries the date it was measured, and the manager re-measures the
  seven that the *Decision* turns on (Context items 1, 3, 5, 7, 9, 11, 12) before the status
  moves.
- **Related**: [ADR-0080](ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
  decision 2 (**amended here**: its sentence *"the conformance runner covers them by parameter,
  not by table"* is what fails) and decision 3 (its sentence *"SP2's tables are a superset of
  SP0/SP1's"* is **corrected here**: for enumerations the later dictionary is stricter);
  [ADR-0083](ADR-0083-ten-field-types-one-field-spelled-two-ways-one-empty-component-and-where-the-sp2-oracle-comes-from.md)
  (*Context* item 4 first noticed where QuickFIX validates an admin body; its *Consequences*
  left the per-token enum fix waiting for a row — decision 2 below gives it that row);
  [ADR-0001](ADR-0001-relationship-to-quickfix.md) (the `.def`s are the oracle, and the XML is
  the truth); `DESIGN.md` D2 (the index is flat, so a group member is a field the scan sees in
  wire order) and D3 (a rule lives in a table, never at a call site); `CLAUDE.md` §2 items 1,
  2, 3, 5; the plan [phase-2-fixt-and-sbe](../plans/2026-09-19-phase-2-fixt-and-sbe.md) row
  **B4** and its *Cách kiểm chứng* line for PR B; traps recorded in
  [fixt-corpus-traps](../reference/fixt-corpus-traps.md).
- **Answers**: the three questions ADR-0080 decision 2 did not ask, found when row B4 scored
  **173 / 180** with every FIXT rule of decision 3 built and green — and, plainly, what number
  `score_fixt` asserts at the end of phase 2.

## Context

Row B4 built `DropReason::LogonWithoutDefaultApplVerID`, the outbound `1137`, the stored
counterparty `1137` and the `1128` policy, and every file that turns on one of them passes.
`[measured 2026-09-19]` `cargo test -p fixbolt-session --features fix50sp2 --test score_fixt`
prints `fix50 57/60 fix50sp1 58/60 fix50sp2 58/60`, and the seven failures are:

| File | Corpora | What the engine sent | What the `.def` expects |
|---|---|---|---|
| `14a_BadField.def:17` | all three | `Reject 373=2` | `Reject 373=0` *Invalid tag number* for `999=HI` on a Heartbeat |
| `14i_RepeatingGroupCountNotEqual.def:16` | all three | `Reject 373=5` naming `336` | `Reject 373=16` naming `386` |
| `21_RepeatingGroupSpecifierWithValueOfZero.def:17` | `fix50` only | `Reject 373=5` naming `336` | the `35=d` echoed |

The brief that opened this ADR counted `14i` as two files; the test output names three, and
the masked diff below shows why it must be three — the file is byte-identical across the
corpora once CompID, `1137` and `BodyLength` are masked. Everything below was measured on
2026-09-19 against `vendor/quickfix/spec/` and `vendor/quickfix/test/definitions/server/` at
the pinned SHA `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`, with `xml.etree` for the XML and a
scratch crate outside the tree (`Cargo.toml` pointing at `crates/dict` with `fix50sp2` on) for
the generated table. Nothing was reasoned from memory.

### A — the admin body

1. `999` is absent from `FIXT11.xml` (71 fields, highest `1409`) and is `LegUnitOfMeasure`
   in `FIX50SP1.xml` and `FIX50SP2.xml`, and `LegUnitofMeasure` (lower-case *o*) in
   `FIX50.xml`. The merged table answers `is_defined_tag(999) == true`,
   `allows(b"0", 999) == false`, so `scan_fields` says `373=2`.
2. **Both QuickFIX engines validate an admin message's body against the transport dictionary
   alone, and an application message's body against the application dictionary.** C++
   `Session.cpp` lines 1225–1233: `if (m_sessionID.isFIXT() && message.isApp())` →
   `DataDictionary::validate(message, &sessionDD, &applicationDD)`, **else**
   `sessionDataDictionary.validate(message)`, which is `validate(msg, this, this)`; the body
   walk `iterate` then asks `checkValidTagNumber` against FIXT11's `m_fields`
   (`DataDictionary.cpp` 159–183, `DataDictionary.h` 372–376) and `999` is not there →
   `InvalidTagNumber`. `Message.cpp` 328–329 does the same on the parse side
   (`pApplicationDataDictionary = pSessionDataDictionary` for `isAdminMsgType`). QuickFIX/J
   `Session.java` 1067–1073: `MessageUtils.isAdminMessage(msgType) ?
   getSessionDataDictionary(beginString) : getApplicationDataDictionary(applVerID)`.
3. The `.def`'s own premise — *"a field identifier (tag number) not defined in the
   specification"* — was true in FIX 4.4 (highest tag `956`) and is **false** in all three
   FIX 5.0 files. QuickFIX passes the file for a structural reason: the transport layer does
   not know `999`. The structural reason is the specification's shape — `FIX50SP2.xml` has
   `<header />`, `<trailer />` and no admin message at all (ADR-0080 *Context*); the session
   messages exist only in the transport file, with the transport file's field set.
4. **Blast radius, scanned rather than assumed:** across the 60 SP2 files, the only `I` lines
   whose `35=` is an admin type and which carry a tag `FIXT11.xml` does not define are
   `14a`'s four (`999`, `0`, `-1`, `5000`, each expecting `373=0`) and the two garbled
   messages of `2d`/`3c` (which never reach the scan). No file sends an application field
   on an admin message and expects `373=2`. The 59 FIX 4.4 files are a single-file
   dictionary, where "the layer that defines the message" and "the dictionary" are one set.
5. `dict::Tables` has no question that separates the two layers, and one `DEFINED_TAGS`
   bitset cannot: the merged table is 6 028 + 71 fields with one bit per tag.

### B — a member's value before the count

6. `14i` declares `386=3`, sends two entries `336=PRE-OPEN` / `336=AFTER-HOURS`, expects
   `373=16`. `group_members(b"D", 386) == [336, 625]`; `enum_allows(336, b"PRE-OPEN") ==
   Some(false)` on the SP2 table. `scan_fields` walks the flat index in wire order and asks
   `enum_allows` on `336` (position 15) before `bad_group_count` runs at all (`lib.rs`
   3375–3386: `scan_fields` → `out_of_family_appl_ver_id` → `missing_required` →
   `bad_group_count`).
7. `TradingSessionID(336)` carries **0** enumerated values in `FIX44.xml` and `FIX50.xml`,
   **6** in `FIX50SP1.xml` (`1`–`6`), **7** in `FIX50SP2.xml`. FIX Orchestra says why: codes
   `1`–`6` *Day … AfterHours* are `added="FIX.5.0" addedEP="58"`, code `7` *Holiday* is
   `added="FIX.5.0SP2" addedEP="190"` — EP58 is after the 5.0 base QuickFIX's `FIX50.xml`
   snapshots and before SP1. So the 59 could never see this, and the FIXT corpora see it three
   times.
8. **The two QuickFIX engines disagree on whether, and agree on when.** C++
   `DataDictionary::iterate` walks one `FieldMap` and never recurses; group entries live in
   nested maps (`Message.cpp` `setGroup`), so a member is never asked `373=0/2/4/5/6` — and
   nothing in the tracker says that is deliberate (search below found nothing). QuickFIX/J
   `DataDictionary.java` 664–689 walks the top-level map first, asking `checkGroupCount` at
   the counter, **then** `for (groups …) iterate(group …)` — members **are** asked
   `checkHasValue`, `checkValidFormat`, `checkValue` and `checkField`, after every top-level
   question including the count. On `14i` both answer `373=16` for the same reason: the
   counter is a top-level field and `336` is not.
9. **This is latent in FIX 4.4 today.** `FIX44.xml` has 511 distinct group-member fields, 110
   of them enumerated; on `NewOrderSingle` alone: `StipulationType(233)`,
   `PartyIDSource(447)`, `PartyRole(452)`, `PartySubIDType(803)`, `EventType(865)`.
   `group_members(b"D", 453) == [448, 447, 452, 802]` and `enum_allows(447, b"ZZ") ==
   Some(false)`. A FIX 4.4 `D` with `453=3`, two entries, and `447=ZZ` in one of them is
   answered `373=5` here and `373=16` by both QuickFIX engines. No `.def` sends it; `14i` is
   the only populated group in the 59 (`PRD.md` §4) and its member has no enumeration there.
   This is the second enum question the FIX 4.4 path gets wrong with no corpus to see it — the
   first is `enum_allows(18, b"2 A") == Some(false)`, ADR-0083 *Context*, still `Some(false)`
   on `Fix44` today (probed 2026-09-19).

### C — three corpora, one table

10. ADR-0080 *Context* said the three directories *"differ only in the CompID and the value
    of `1137`"*. Masked properly — CompID, `1137`, and `BodyLength` on both `E` and `E1,`
    lines, with the SOH before `9=` matched as a byte and not as `.` — `fix50sp1` is equal
    to `fix50sp2` on **60 / 60** files and `fix50` differs on **exactly one**: `21`'s `35=d`
    carries `336=ONE_MAIN` at top level, and the echo carries it back. (An earlier mask that
    matched `FIXT.1.19=` as a regex over the SOH reported 53 differing files; it was wrong,
    and it is why the brief's counts and this table were checked twice.)
11. **QuickFIX ran each corpus against its own application dictionary.** C++ `test/setup.bat`
    lines 30–47 (and `test/setup.sh`, its Unix twin, both at the pin) write three `[SESSION]`
    blocks into `cfg/at.cfg`: `TargetCompID=TW50` with `AppDataDictionary=..\spec\FIX50.xml`,
    `TW50SP1` with `FIX50SP1.xml`, `TW50SP2` with `FIX50SP2.xml`. QuickFIX/J's acceptance
    tree has only `fix50` (no `sp1`, no `sp2`), its server sets `DefaultApplVerID=7` and lets
    `DefaultDataDictionaryProvider` map `7` → `FIX50.xml`; its copy of `fix50/21` carries
    `336=ONE_MAIN`. So the `fix50` corpus's oracle is `FIX50.xml`, where `336` is free.
12. Under `FIX50.xml`, `d` carries `336` at top level; under SP1 and SP2 it appears on `d`
    only inside `MarketSegmentGrp/…/TradingSessionRulesGrp`. The SP2 table's flat `allows`
    includes nested members, so `allows(b"d", 336) == true` and the only refusal left is
    `enum_allows(336, b"ONE_MAIN") == Some(false)` → `373=5`. Scoping the enum per corpus
    would let the file pass, and is a FIX50 table by another name.
13. **Enumerations drift in both directions between service packs.** ADR-0080 decision 3 said
    SP2 validates SP0 traffic because *"SP2's tables are a superset"*. True for fields and
    messages; **false for enumerations, and false both ways**. FIX50 → FIX50SP2:
    - *stricter* — four fields go from free to enumerated: `BeginString(8)` (3 values),
      `TradingSessionID(336)` (7), `TradingSessionSubID(625)` (13), `DealingCapacity(1048)`
      (3); and **five** lose at least one value SP0 allowed: `SecurityType(167)` `WLD`;
      `HaltReasonChar(327)` `D E I M P X` (all six, replaced by six others); `MatchType(574)`
      `60`–`65`; `DeskOrderHandlingInst(1035)` all 24; `PegPriceType(1094)` `6`. **Nine**
      tags.
    - *more permissive* — one field goes from enumerated to free: `DeskOrderHandlingInst(1035)`,
      **24 values in FIX50, 0 in SP2**. Under SP2 `enum_allows(1035, …)` is `None` for any
      value; under FIX50 a value outside the 24 is `Some(false)`. It is in the "loses" list
      above because it lost everything.

    FIX50SP1 → FIX50SP2: *stricter* on three free → enumerated (`8`, `1048`,
    `ListUpdateAction(1324)`) and three losing a value (`327`; `1035` all 24;
    `MarketUpdateAction(1395)` all 3) — **six** tags; *more permissive* on two, `1035` and
    `1395` (3 → 0).

    So SP2's enumerations are **neither a superset nor a subset** of FIX50's or SP1's. The
    strict direction refuses an SP0 counterparty on nine fields where QuickFIX running the
    counterparty's own dictionary accepts it; the permissive direction accepts a `1035` value
    that FIX50 refuses. **No `.def` in the three corpora carries `1035=` or `1395=`**
    (grepped 2026-09-19), so the corpora can see the strict direction on exactly one file
    (`21`, via `336`) and the permissive direction nowhere. A first version of this item
    counted four losing fields, not five, because the script compared sets only when the SP2
    set was non-empty — the one condition that hides a 24 → 0 field; the manager's
    re-measurement caught it.
14. **A `fixt11_fix50.rs` and a `fixt11_fix50sp1.rs` would build with the generator as it
    is**: `FIX50.xml` and `FIX50SP1.xml` use no type name outside the 29 variants; every
    FIXT11 field present in them agrees on number, name and variant (ten FIXT11 fields —
    `1156`, `1400`–`1404`, `1406`–`1409` — are absent from `FIX50.xml`, which the pair rule
    allows); no empty component; every `DATA` field pairs by name; no enum pair fails the
    superset rule. Not measured: build time and binary size of a third and fourth table.

### What the search found

GitHub and the QuickFIX/J tracker, 2026-09-19: **nothing** on either engine about group
members being (or not being) value-checked, and nothing about admin bodies being validated
against the transport dictionary — both are code, not issues. Nearest hits, all unrelated:
quickfix#732 (an `IncorrectTagValue` that never becomes a Reject), quickfix#154 (CME group
parsing), quickfix-j#247 (user-defined fields validated only on one path), QFJ-404 (a knob
for *Tag appears more than once*). One piece of prior art worth naming: QuickFIX/J's
`ValidationSettings.allowUnknownEnumValues` (`DataDictionary.java` 673, 791–792) switches
`373=5` off entirely — the knob a venue-defined `TradingSessionID` would need; **not decided
here**.

## Decision

### 1. A message is validated against the tag set of the layer that defines it — `Tables::is_defined_tag_for`

`dict::Tables` gains one question:

```rust
/// Whether the layer that defines this message type defines this tag — `373=0`.
/// A single-file dictionary has one layer and answers `is_defined_tag`. A FIXT
/// table has two: a message of the transport file (`is_transport_message`) is defined
/// there and carries only its tags; a message of the application file carries the
/// merged set.
fn is_defined_tag_for(msg_type: &[u8], tag: u32) -> bool;
```

- `build.rs` `generate_pair` emits a second bitset, `TRANSPORT_DEFINED_TAGS`, over the
  transport file's `<fields>` alone (71 fields, highest tag `1409` → 23 `u64` words, 184
  bytes), `pub const fn is_transport_tag(tag) -> bool`, **and**
  `pub fn is_transport_message(msg_type) -> bool` over the transport file's `<messages>` —
  eight types, `0 1 2 3 4 5 A n`, every one `msgcat='admin'` (fixt-dictionary-traps
  inventory). `Fixt11Fix50Sp2Tables::is_defined_tag_for` is
  `if is_transport_message(msg_type) { is_transport_tag(tag) } else { is_defined_tag(tag) }`.
  "Defined by the transport file" and "a message of the transport file" are then the same
  file by construction — the rule is generated, not written at a call site (D3).
  **Measured 2026-09-19, and a surprise for the plan**: the generated table exports no
  `is_admin` (its public functions are `allows`, `data_length_tag`, `enum_allows`,
  `field_type`, `group_members`, `is_defined_tag`, `is_header`, `is_msg_type`, `required`,
  `required_header`), `build.rs` never reads `msgcat`, and the session's `ADMIN` is a
  hand-written seven-entry const at `lib.rs:291` without `n` — so the plan's traps-table row
  *"`is_admin` cho FIXT lấy từ `msgcat` của `FIXT11.xml` … B1 `is_admin(b"n")` true"* was not
  built by B1. This decision does not branch on the session's `ADMIN`: a call-site list is
  exactly what D3 forbids, and it disagrees with the transport file on `n`.
- `Fix44` implements it **explicitly** as `is_defined_tag(tag)` with the one-line reason
  above; there is no default method. A default would hand a future third table FIX 4.4
  semantics silently, and ADR-0083 decision 2's posture is that nothing about a table is
  silent.
- `scan_fields` (`lib.rs` 4027) replaces its one `is_defined_tag(tag)` call with
  `is_defined_tag_for(msg_type, tag)`. Nothing else in the session moves: the check sits
  where `373=0` sits today, before `373=4` and `373=2`, and `DictionaryChecks`'
  `skips_user_defined_fields` and `allows_unknown_msg_fields` treat it exactly as they treat
  `is_defined_tag` — the knobs' documented split (`373=0` is not `373=2`) is unchanged.
  `is_defined_tag(tag)` stays on the trait as "defined in any layer"; it is the right question
  for a tag with no message around it.
- **Cost**: 184 bytes of static plus an eight-entry message list, one branch on
  `is_transport_message` per field of a FIXT admin message
  (admin messages are off the order path), no allocation.

  > **Correction, 2026-09-19, after B4a was built.** This bullet first read *"no allocation
  > (`benches/alloc.rs` case `validate NewOrderSingle (FIXT tables)` stays 0 — an application
  > message never takes the branch)"*. **That case does not exist.**
  > `grep -rn "FIXT\|fix50sp2" crates/*/benches/*.rs` returns nothing: no bench in any crate
  > has a FIXT case, and `session/benches/alloc.rs`'s 17 cases are all FIX 4.4 — none
  > validates through the pair table. The case is **row B6's**, not yet built (the plan's
  > *Cách làm* PR B item 6 names it). The bullet cited, in the present tense, a measurement
  > scheduled for later. Non-negotiable 1 says allocation is proven by the counting allocator
  > and *never by reading the code*, so the honest statement is: **the FIXT validation path's
  > allocation count is unproven until B6 adds that case.** What B4a does prove is the
  > correctness half — `crates/dict/tests/fixt.rs` asserts
  > `is_defined_tag_for(b"D", 999) == true`, so an application message takes the `else` arm.
  > Found by the B4a developer, who declined to add a bench outside its brief and reported it
  > instead. The decision itself is unchanged; only this evidence claim is corrected. Public API of `dict` grows by one method: `DESIGN.md` D16's `Tables` list,
  `CHANGELOG.md`, the trait's rustdoc.
- **Does not change FIX 4.4**: `crates/dict/src/tables.rs::the_trait_and_the_inherent_methods_agree`
  gains `is_defined_tag_for(b"0", 999) == is_defined_tag(999)` and the same for `35` and
  `5000`; 59 / 59 is the gate.
- **Proven by**: `crates/dict/tests/fixt.rs` — `is_transport_tag(999) == false`,
  `is_transport_tag(1137) == true`, `is_transport_tag(35) == true`, `is_transport_tag(55) ==
  false`, `is_transport_message(b"n") == true`, `is_transport_message(b"D") == false`,
  `is_defined_tag_for(b"0", 999) == false`, `is_defined_tag_for(b"D", 999) == true`;
  and `14a` green in all three corpora. Reversal, FAIL sentence written first: make the FIXT
  impl answer `is_defined_tag(tag)` → `14a_BadField.def:17 Value { at: 1, tag: 9, expected:
  "98", actual: "117" }` in `fix50` — the comparator's first mismatch is `BodyLength`, because
  *Tag not defined for this message type* is nineteen bytes longer than *Invalid tag number*;
  that is the sentence the test prints today.

What this rule says to a counterparty: an application field on a session message is
answered `373=0`, as by QuickFIX C++ and QuickFIX/J, because to the transport layer it is
not a defined tag. The 1128/1156/1129-on-session-messages item of ADR-0080 decision 3 is
untouched: those three are FIXT11 header fields, so they are transport-defined, and their
prohibition on session messages remains an open item, not built.

### 2. A group member's value and format are asked — after `373=1` and `373=16`

The engine keeps value-checking group members: the dictionary enumerates the field, D2 gives
the scan every member for free, QuickFIX/J does it, and the XML-is-the-truth posture of
ADR-0001 and ADR-0083 says a value the table can refuse is refused. **What changes is when.**
The rule, for both tables:

> `373=5` and `373=6` on a field that is a member of a repeating group present in the
> message are asked only after `missing_required` (`373=1`) and `bad_group_count` (`373=16`)
> have found nothing. `373=0`, `373=2`, `373=4`, `373=13` and `373=14` on a member stay where
> they are, in the wire-order scan.

That is the order both QuickFIX engines share — top-level questions, including the count at
the counter's position, then whatever they ask of members — with the one liberty this
engine already took and the corpus pinned: `373=1` after the scan (`14d`), not before it.

- **Shape**: `scan_fields` skips its two last arms for a tag that is a member of a group
  whose counter has already been seen in this scan. "Seen" is a fixed array of counters,
  bounded and on the stack (a message has at most a handful of top-level counters; FIX 4.4's
  maximum per message is measured by the row), so no allocation and no walk per field; a
  message with no counter never enters the branch. A fourth pass, `scan_group_members`, runs
  after `bad_group_count` and asks `enum_allows` and `field_type().accepts` on exactly those
  tags, in wire order, first fault wins. `in_a_group` (already there for `373=13`) is the
  membership question; it is not called per field.
- **This changes FIX 4.4 session-boundary behaviour** on a message no `.def` sends (item 9), so
  it is **a row of its own, not a B4 line**, and it shares that row with the per-token
  enum fix ADR-0083 left waiting: both are *the enum question asked wrong on the FIX 4.4 path
  with no corpus to see it*, both touch `scan_fields`' `enum_allows` arm, both need a
  hand-made message and a line in `SESSION-BEHAVIOUR.md`. One row, two tests, two
  reversals, one `CHANGELOG.md` entry, senior developer (non-negotiables 1, 2, 3). The row is
  in PR B, after B4 and before B8, because `14i` ×3 is in the 180.
- **Proven by**: `14i` green in all three corpora; and in `crates/session/tests/` two
  hand-made FIX 4.4 messages on `D` with `NoPartyIDs(453)`: `453=3` + two entries + `447=ZZ` →
  `373=16` naming `453` (count wins); `453=2` + two entries + `447=ZZ` → `373=5` naming `447`
  (the member is still checked — this is the test that turns red if someone "fixes" `14i` the
  QuickFIX C++ way). Reversal: restore the arms to the scan → the first test prints the
  FAIL sentence `expected 373=16 371=453, engine sent 373=5 371=447`. Per-token: `18=2 A` on
  `D` → no Reject; `18=2 Z` → `373=5`. `benches/validate.rs` `validate NewOrderSingle` and
  `benches/alloc.rs` re-run and quoted on both tables.
- **Cost**: one bounded array and one extra pass on messages that carry a group; a message
  without one pays a comparison per field. The number is the row's to measure, and the
  bench's bound is the gate.

### 3. One table in phase 2; the `fix50` corpus is scored on the SP2 table with its one divergence asserted by content; a FIX50 table is the named successor

ADR-0080 decision 2's *table* sentence stands — one generated `fixt11_fix50sp2.rs` — and its
*corpus* sentence is replaced:

> The three FIXT corpora are three oracles that QuickFIX ran against three application
> dictionaries. `fix50sp1` is byte-equal to `fix50sp2` modulo CompID, `1137` and `BodyLength`
> on 60 / 60 files, so the SP2 table is its oracle by measurement. `fix50` differs in one
> file, `21_RepeatingGroupSpecifierWithValueOfZero.def`, whose expectation depends on
> `FIX50.xml` leaving `TradingSessionID(336)` free; the SP2 table cannot express that and
> phase 2 does not pretend it can.

`score_fixt` therefore asserts, per corpus and by name:

- `fix50sp2` **60 / 60** and `fix50sp1` **60 / 60**;
- `fix50` **59 / 60**, and the one failure is **exactly**
  `21_RepeatingGroupSpecifierWithValueOfZero.def:17`, and the bytes the engine sent for that
  line are a `35=3` carrying `373=5` and `371=336`. Any other failing file, any other line,
  any other reject reason — or the file *passing*, which could only mean enum checking was
  loosened somewhere — is red.

The total is **179 / 180 with the 180th asserted as a divergence**, not skipped, not excluded,
not edited. No fixture changes; no exclusion list; the file is loaded and run like the other
59. The plan's *Cách kiểm chứng* line for PR B reads, after this decision:

> `score_fixt` in `fix50 59/60 fix50sp1 60/60 fix50sp2 60/60` **và** khẳng định bằng nội dung
> rằng file duy nhất lệch là `21_RepeatingGroupSpecifierWithValueOfZero.def:17`, engine trả
> `Reject 373=5 371=336` (ADR-0084 quyết định 3); câu FAIL đảo chiều `1d_…` giữ nguyên.

**Named successor**: `fixt11_fix50.rs` behind a feature `fix50` (and `fixt11_fix50sp1.rs`
behind `fix50sp1`, by the same rule), generated by `generate_pair` as it is (item 14), each
with its own `TagValue<…Tables, 64>` alias, and the conformance runner choosing the table by
corpus. When that lands, the divergence assertion is **deleted** and `fix50` runs on its own
table: 180 / 180 by table. Trigger: phase 3, or the first FIX 5.0-base counterparty this
engine must face, whichever comes first. It is not phase 2 because it is a third and fourth
`Session` monomorphisation, a feature per version through `dict`, `session`, `engine` and
`conformance`, and a documentation set that has not yet absorbed the second table — and
because it would not change what an SP2-configured acceptor does to an SP0 counterparty,
which is the real cost item 13 exposes.

**What bidirectional drift does to this decision.** It **strengthens the case for the
successor** and **weakens what the divergence assertion can claim** — and both must be said
here, not in a footnote. Because SP2's enumerations are neither a superset nor a subset of
FIX50's (item 13), the `fix50` corpus scored on the SP2 table is not a full oracle in either
direction: only a FIX50 table makes it one, which is why the successor is named and not
merely allowed. And the assertion above pins **the one strict-direction failure the corpus
produces**: a permissive-direction divergence — the engine accepting a `1035` value FIX50
refuses — would leave `fix50` at 60 / 60 and the assertion as written would not notice,
because a file that passes is not a file it examines. **That is accepted for phase 2, for a
reason and not by assumption**: no `.def` carries `1035=` or `1395=`, so there is no file on
which the permissive direction could score at all — the assertion cannot catch what the
corpus cannot see, and scoring against a FIX50 table would score the same 60 on that
direction for the same reason. What guards the permissive direction until the successor
lands is a `crates/dict/tests/fixt.rs` case pinning the drift itself on the SP2 table —
`enum_allows(1035, b"ZZZ") == None` and `enum_allows(1395, b"Z") == None` beside
`enum_allows(336, b"ONE_MAIN") == Some(false)` — with a comment naming the FIX50 counts (24,
3, 0), so a reader who assumes "superset" in either direction meets a test, and the day the
FIX50 table exists the same test gains its other half (`Some(false)` on that table).
Bidirectionality does **not** reopen "generate the FIX50 table now": it raises the value of
the table without lowering its cost, and the corpus still cannot exercise the direction the
table would add.

**ADR-0080 decision 3 is corrected, not superseded** (it is *Proposed*, so its text may be
revised in place with the revision recorded): the sentence *"SP2's tables are a superset of
SP0/SP1's"* becomes *"SP2's messages and fields are a superset; its enumerations are
neither a superset nor a subset — stricter on nine tags against FIX 5.0 (`8`, `167`, `327`,
`336`, `574`, `625`, `1035`, `1048`, `1094`) and six against SP1 (`8`, `327`, `1035`, `1048`,
`1324`, `1395`), more permissive on `1035` against both and `1395` against SP1 (ADR-0084
item 13)"*, and `SESSION-BEHAVIOUR.md`'s "accepted, validated with SP2" line carries the
same two lists with their direction. A knob in the shape of QuickFIX/J's
`allowUnknownEnumValues` is prior art for the day a venue-defined `336` arrives; not decided
here, listed as an open item in `STATUS.md`.

### 4. The number, and row B4

`score_fixt` asserts **`(179, 180)` plus the decision-3 divergence tuple** at the end of phase
2. **Row B4's "180 / 180" does not survive**, and it is restated rather than lowered: B4's
own gate is `tests/fixt.rs` (five unit tests) + 59 / 59 + the `1d` reversal + `benches/alloc.rs`
0, with `score_fixt`'s assertion landing in the last of three follow-on rows — decision 1
(`dict` + `session`, developer-level with the trait method briefed exactly), decision 2 +
per-token (senior developer), decision 3 (the test's assertion and the plan line). The
`wire_fixt` 60 / 60 on `fix50sp2` (row B5) is unchanged by any of this: SP2 is 60 / 60 once
decisions 1 and 2 land. What the manager commits per step is the manager's; what each step
must print is above.

## Consequences

**Good**

- The seven are explained by three rules, each generated from the XML or ordered by the
  corpus, none a special case for a file. `14a` ×3 and `14i` ×3 go green by rule; `21` goes
  red for a reason the test states in its own assertion.
- `373=0` on a FIXT session means what it means on QuickFIX C++ and QuickFIX/J: the transport
  layer does not define the tag. An operator reading a counterparty's Reject log sees the
  same code from three engines.
- Two latent FIX 4.4 defects — a member's value beating the count, and the whole-value enum
  match on a multi-value field — get one row, hand-made tests, and lines in
  `SESSION-BEHAVIOUR.md` before a counterparty finds them. Both are visible only because the
  FIXT corpora carry enumerations the 59 never had; the phase paid for that visibility and
  this ADR spends it.
- The `fix50` divergence is **asserted**, so the day enum checking is loosened by accident
  the test goes red on that file, and the day the FIX50 table lands the assertion's deletion
  is a visible diff. An exclusion list would have done neither.
- ADR-0080's false sentence about supersets is corrected with the nine and six tags named
  and their direction stated, and the `.def`'s false premise (item 3) is written down where
  the next reader of `14a` will look.
- The trait grows by one question that names its `373=` code, which is the trait's own rule
  for admitting a question.

**Bad — and accepted**

- **179 is not 180.** The phase-2 exit criterion in the plan and `PRD.md` §2 says 180 / 180;
  this ADR says the honest number is 179 plus one asserted divergence, and every document
  that carries the number (`CONFORMANCE.md`, `DESIGN.md` §6, `CLAUDE.md` §2 item 3's planned
  wording, `STATUS.md`) must say it that way, with the file named. A reader who sees "179 /
  180" without the sentence beside it will read a bug; the sentence is the cost.
- **An SP0 counterparty is refused on nine fields, and passed on one it should not be**, by
  an SP2-configured acceptor, and this ADR does not fix either — it names them. QuickFIX in
  the same seat, configured with SP2, does the same on all ten; QuickFIX configured with the
  counterparty's dictionary does neither, and this engine cannot be configured that way until
  the successor lands.
- **The divergence assertion sees one direction.** It pins the strict-direction failure on
  `21` and is blind to the permissive direction by construction (a passing file is not
  examined). Accepted because the corpora carry no `1035=` or `1395=`, so there is nothing
  on that side to catch — a reason recorded in decision 3, and the `dict` test named there
  is the only guard on that side until the FIX50 table exists.
- **Decision 2 adds a pass and a bounded array to the validation path** of every message that
  carries a group. The cost is unmeasured until the row measures it; if `validate
  NewOrderSingle` leaves its band the row stops, as ADR-0031 says.
- **Decision 2 makes this engine stricter than QuickFIX C++** — a bad member value in a
  well-counted group is `373=5` here and silently accepted there. That is the posture chosen
  in ADR-0083 and it is repeated here deliberately; a counterparty tested only against
  QuickFIX C++ may send group values nobody ever checked.
- **Two tag bitsets, one behind `is_transport_message`**: a reader of the generated FIXT table now finds
  two bitsets and must know which question to ask. The rustdoc on `is_defined_tag_for`
  carries the reason and the rustdoc on `is_defined_tag` says "any layer".
- **`Fix44::is_defined_tag_for` is a one-liner someone will call redundant.** It is not a
  default for the reason given; the temptation to make it one is recorded here so the reviewer
  who feels it can read why.
- **The plan's B4 row is restated by an ADR the plan did not anticipate**, which is the
  *Plan turns out wrong mid-build → stop, fix the plan, get it re-approved* case of
  `CLAUDE.md` §1. The manager, not the architect, edits the plan; this ADR gives the exact
  line.
- **The successor is a fourth `Session` monomorphisation nobody has costed.** Item 14 says the
  generator is ready; nothing says the build is cheap. When the trigger fires, the first
  measurement is `dict`'s build time with all three features on.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| **A:** accept `373=2` on `14a`, record it as a known miss | Three files pinned as "known" for a behaviour both QuickFIX engines get right and the transport-layer structure of FIXT explains; and it is not one file — any application tag on any session message answers a different code from every other engine |
| **A:** generate a separate transport table and let the session pick a table per message by admin type | Two `Tables` per session type and a branch per message on the order path, for a question one extra bitset answers off the order path; the per-message table switch is what ADR-0080 decision 2 refused |
| **A:** `is_transport_tag(tag)` on the trait, with the admin-type branch written in `scan_fields` against the session's `ADMIN` const | The rule "an admin message is checked against the transport set" would then live at a call site, and `Fix44` would need a stub answering nonsense. `is_defined_tag_for(msg_type, tag)` keeps the rule in the table and the session ignorant of layers (D3) |
| **A:** a default method on the trait | Silent inheritance of one-layer semantics by a future table; ADR-0083 decision 2 refused every silent default in the pair build and this is the same shape |
| **B:** do not value-check group members (QuickFIX C++) | Blind to 110 enumerated members in FIX 4.4 and more in SP2; weaker than QuickFIX/J; the opposite of ADR-0083's posture on the ten types |
| **B:** check the count inside the scan, at the counter's position | Simplest code and answers `14i`, but it puts `373=16` before `373=1`, where both QuickFIX engines put `373=1` first; the corpus cannot tell the two apart today, so the order is chosen from prior art, not from a file |
| **B:** run `bad_group_count` before `scan_fields` | `373=16` would beat `373=0/2/4/13` on every field of the message, including fields before the counter; further from both engines than today |
| **B:** fold the ordering fix into B4 as "the fix that makes 14i pass" | It changes FIX 4.4 session-boundary behaviour with no `.def`; `CLAUDE.md` §1 and §4 make that a row with its own test and doc line |
| **B:** two rows, one per enum defect | Same arm, same kind of test, same doc section, same reviewer; two rows are two briefs and two reviews for one change of posture |
| **C:** generate `FIX50` and `FIX50SP1` tables in phase 2 | Feasible (item 14) and the named successor; not phase 2 because it costs a feature per version through four crates and does not change the engine's behaviour toward an SP0 counterparty, which is the real problem item 13 names |
| **C:** scope `enum_allows` per corpus (per `1137`) at run time | A per-message enum-table switch — three enum tables in one binary and a branch per field; a FIX50 table by another name with none of its honesty |
| **C:** exclude `21` from `fix50` with the reason in a comment | A list of files not run is the failure mode `a-test-that-skipped-itself-on-every-machine-that-ran-it` records; asserting the divergence by content is one line more and cannot rot silently |
| **C:** edit `fix50/21` to drop `336=ONE_MAIN` | `CLAUDE.md` §10: *a fixture edited so new work can pass is the failure mode to watch for* |
| **C:** assert 173 / 180 at B4 "for now" | A number pinned to what was achieved while the row says 180 — the manager refused it and this ADR agrees; the number pinned is the number decided, with its reason in the assertion |
| **C:** lower the plan line to "≥ 179" | A bound with no name is a bound nobody reads; the line names the file and the reject |

## Sources

- `vendor/quickfix/spec/FIXT11.xml`, `FIX50.xml`, `FIX50SP1.xml`, `FIX50SP2.xml`, `FIX44.xml`
  — measured 2026-09-19 with `xml.etree` at pin `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`:
  field counts (71 / 1 090 / 1 373 / 6 028 / 912), `999` per file, `336` enum counts (0 / 0 /
  6 / 7 / 0), `d` and `D` placement of `336`, group-member enumeration counts in FIX44 (511 /
  110), pair-build feasibility for FIXT11+FIX50 and FIXT11+FIX50SP1, enumeration drift in
  both directions between FIX50 / SP1 / SP2 (free → enumerated, loses ≥ 1 value, enumerated
  → free; the last row compared against an empty SP2 set, which the first script did not),
  and `grep -c '1035=\|1395='` over the 180 `.def`s (0 files). `vendor/quickfix/test/definitions/server/{fix50,fix50sp1,fix50sp2}` —
  masked diff (CompID, `1137`, `BodyLength` on `E` and `E1,` lines, SOH as a byte) and the
  admin-message tag scan across the 60 SP2 files.
- The generated table, probed 2026-09-19 from a scratch crate depending on `crates/dict`
  with `fix50sp2`: `is_defined_tag(999) == true`, `allows(b"0", 999) == false`,
  `allows(b"d", 336) == true`, `enum_allows(336, b"ONE_MAIN") == Some(false)`,
  `enum_allows(336, b"PRE-OPEN") == Some(false)`, `enum_allows(336, b"1") == Some(true)`,
  `group_members(b"D", 386) == [336, 625]`; on `Fix44`: `is_defined_tag(999) == false`,
  `enum_allows(336, b"ONE_MAIN") == None`, `enum_allows(18, b"2 A") == Some(false)`,
  `group_members(b"D", 453) == [448, 447, 452, 802]`, `enum_allows(447, b"ZZ") == Some(false)`.
- `cargo test -p fixbolt-session --features fix50sp2 --test score_fixt`, 2026-09-19:
  `fix50 57/60 fix50sp1 58/60 fix50sp2 58/60` and the seven failure lines quoted in *Context*.
- QuickFIX C++ at the pin, read 2026-09-19: `src/C++/Session.cpp` 1170–1183 (`next(const
  std::string&)`: FIXT parses with `sessionDD` + `applicationDD` from
  `m_senderDefaultApplVerID`), 1211–1233 (`next(const Message&)`: `setTargetDefaultApplVerID`
  from the Logon's `1137`; `isFIXT() && message.isApp()` → two-dictionary validate, else
  `sessionDataDictionary.validate(message)`), 513–535 (`newMessage`: admin from `sessionDD`);
  `src/C++/DataDictionary.cpp` 123–157 (`validate`: `checkHasRequired` before `iterate`),
  159–183 (`iterate`: one `FieldMap`, no recursion, `checkGroupCount` per field);
  `src/C++/DataDictionary.h` 361–369 (`shouldCheckTag`), 372–376 (`checkValidTagNumber`
  against `m_fields`), 493–530 (`checkValue`, `checkHasValue`, `checkIsInMessage`,
  `checkGroupCount`); `src/C++/Message.cpp` 296–360 (`setString`: `isAdminMsgType` →
  `pApplicationDataDictionary = pSessionDataDictionary`); `src/C++/DataDictionaryProvider.cpp`
  (transport by `BeginString`, application by `ApplVerID`); `test/setup.bat` 30–47 and
  `test/setup.sh` (three FIXT `[SESSION]` blocks, one `AppDataDictionary` each);
  `test/runat.sh` 18–21 (the three corpora run against that server) —
  <https://github.com/quickfix/quickfix/tree/386ce46e917ae494ab6e90b1be90fd421cdbe3f9>.
- QuickFIX/J `master`, read 2026-09-19: `quickfixj-core/src/main/java/quickfix/Session.java`
  1049–1078 (`targetDefaultApplVerID` from the Logon; `isAdminMessage(msgType) ?
  getSessionDataDictionary : getApplicationDataDictionary(applVerID)`);
  `quickfixj-base/src/main/java/quickfix/DataDictionary.java` 624–661 (`validate`), 664–689
  (`iterate`: top-level walk with `checkGroupCount`, then recursion into groups), 705–727
  (`checkField`, `checkFieldFailure`), 791–799 (`checkValue`, `allowUnknownEnumValues`);
  `quickfixj-core/src/main/java/quickfix/DefaultDataDictionaryProvider.java` (`ApplVerID` →
  `FIX50.xml` by name); `quickfixj-core/src/test/java/quickfix/test/acceptance/ATServer.java`
  (`DefaultApplVerID=7`, `fix50` and `fixLatest` both accepted as `FIXT.1.1`);
  `quickfixj-core/src/test/resources/quickfix/test/acceptance/definitions/server/` (directories
  `fix40`–`fix44`, `fix50`, `fixLatest`, `future/` — no `fix50sp1`, no `fix50sp2`) and its
  `fix50/21_RepeatingGroupSpecifierWithValueOfZero.def` (`336=ONE_MAIN` present) —
  <https://github.com/quickfix-j/quickfixj>.
- FIX Orchestra, *FIX Latest*, `<fixr:codeSet id="336" name="TradingSessionIDCodeSet">`:
  codes `1`–`6` `added="FIX.5.0" addedEP="58"`, code `7` `added="FIX.5.0SP2" addedEP="190"`;
  field `336` `added="FIX.4.2" updated="FIX.5.0SP2" updatedEP="190"` — the same file ADR-0083
  read
  (<https://github.com/FIXTradingCommunity/orchestrations/blob/master/FIX%20Standard/OrchestraFIXLatest.xml>,
  read 2026-09-19).
- Issue-tracker and web search, 2026-09-19 (GitHub `quickfix/quickfix`, `quickfix-j/quickfixj`,
  the QuickFIX/J Jira): nothing on group-member validation, nothing on transport-only admin
  validation. Nearest, all unrelated: <https://github.com/quickfix/quickfix/issues/732>,
  <https://github.com/quickfix/quickfix/issues/154>,
  <https://github.com/quickfix-j/quickfixj/issues/247>,
  <https://quickfixj.org/jira/si/jira.issueviews:issue-html/QFJ-404/QFJ-404.html>. Dictionary
  sites that list `336`'s values per version — b2bits *fixopaedia* (FIX 5.0 SP1: six values;
  FIX 4.4: none) and OnixS — were reachable in search results only; the Orchestra file is the
  source quoted.
