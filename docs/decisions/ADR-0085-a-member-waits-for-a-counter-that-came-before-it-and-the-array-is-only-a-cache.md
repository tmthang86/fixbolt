# ADR-0085 — A member's value waits for a counter that came *before* it, the array that remembers the counters is only a cache, and a message that fills it gets the same answer

- **Status**: Accepted — 2026-09-19
- **Approved by**: the PR B manager session under the owner's blanket delegation of
  2026-09-19 ("uỷ quyền toàn bộ"), the same standing under which ADR-0084 was accepted.
  **The owner has not read it.** Before accepting, the manager reproduced independently:
  the divergence between the two oracles and that `in_a_group` scans `0..view.len()`
  unscoped; that `crates/session/src/lib.rs` carries no `#[cfg(test)]` module today; that
  `find_from` is a linear walk, so the scan is already O(n²) and a positional fallback is
  the same cost class; that ADR-0085 is unused on **every** ref (`git log --all`), this
  repository having taken one ADR number twice before; and that the rustdoc sentence this
  ADR calls stale is stale — `lib.rs:4013` still says the populated-group case is owed, and
  B6 added it. **Not reproduced by the manager**: the architect's count of 48 top-level
  counters on `AE`. A naive parse of the message element finds 0 literal `<group>` children
  because they arrive through `<component>`, and the manager did not build a recursive
  expander to settle it. The figure is the architect's, and *Context* item 5's likelihood
  is labelled judgement rather than measurement in the ADR's own words.
- **Measured after the fact** `[2026-09-19, commits d7be83d and B4d]`: the decision is built
  and its branch is reached. `SeenCounters` fills at 34 counters against 32 slots on the
  33-group `AE` fixture (proven by a reversal at `SEEN = 64`); the reversal sentence
  `expected 373=5 371=447, engine sent 373=16 371=1907` was predicted and observed word for
  word; the alloc case `validate TradeCaptureReport (33 groups)` reads **0**, with a
  corrupted-copy assertion that fires only if the full-array path ran inside the bench
  binary. A second gap closed in the same row: under `ValidateUserDefinedFields=N` the scan
  skips a counter ≥ 5000 before recording it, so the walk skips it too — 158 SP2 pairs are
  in that class, and decision 2's "agree by construction" is true as written only because
  of that fix.
- **The cost is measured on the wrong machine, and it is large.** `validate
  TradeCaptureReport (33 groups)` reads **47 793.4 ns/op** against **~1 046 ns/op** for
  `validate NewOrderSingle (FIXT tables)` — same box, same run, both `NO BASELINE` on an
  untuned Xeon, so **neither is a published figure** (non-negotiable 10) and the ratio is
  the only thing worth reading. About 46×. **It is not isolated**: the 33-group message is
  far larger as well as quadratic, so this does not say how much of it is the walk. It is
  the number that decides whether *Alternatives* D (a generated `member → counters` table)
  is needed, and it is owed to the §9 desk before anyone concludes either way.
- **Date**: 2026-09-19
- **Deciders**: Tran Manh Thang. Proposed by the architect on 2026-09-19 from a senior-review
  finding the PR B manager verified before it reached the architect; the measurements in
  *Context* items 1–4 are the manager's, dated, and the architect did not re-run the panic
  probe. The architect re-counted the FIXT figures from the XML (item 3) and read the three
  engines named under *What the search found*. Nobody has read this ADR yet.
- **Related**: [ADR-0084](ADR-0084-a-session-message-is-checked-against-the-layer-that-defines-it-a-members-value-waits-for-the-count-and-fix50-is-its-own-oracle.md)
  decision 2 — **amended here, not superseded**. Its *rule* (`373=5` and `373=6` on a member
  wait for `373=1` and `373=16`) stands unchanged. What this ADR replaces is its *Shape*
  paragraph's definition of "member" — *"a member of a group whose counter has already been
  seen in this scan"* was the fast path's definition and *"`in_a_group` … is not called per
  field"* stops being true once the array is full — and the sentence the rustdoc built on it,
  *"the answer never depends on the capacity, only its cost does"*, which is false today.
  `CLAUDE.md` §5 forbids editing an accepted ADR's substance, which is why this is a new number
  and not a patch. `DESIGN.md` D2 (the index is flat; a group is a region of it, not a node)
  and D3 (a rule lives in a table). `CLAUDE.md` §2 items 1 (the array exists because a `Vec`
  would allocate) and 2 (the session layer is pure).
- **Answers**: what the membership oracle is when the deferral array of ADR-0084 decision 2
  overflows, and whether a fixed bound belongs in `scan_fields` at all.

## Context

The line numbers below are `crates/session/src/lib.rs` at `f707539` on `feat/phase-2-b`.

1. **The mechanism.** `scan_fields` (4097–4197) walks the flat index once, in wire order.
   When it passes a `NumInGroup` field that is a counter of this message type, it records the
   tag in `SeenCounters` (4027–4089): a stack array of `SEEN = 32` `u32`s, a length, and a
   `full` flag. Before asking a field's `373=5` and `373=6` it asks `seen.defers(tag)` (4172):
   *is `tag` a member of a group whose counter this scan has already passed?* — a walk over
   at most 32 recorded counters, `group_members(msg_type, counter).contains(&tag)` each. A
   deferred field is asked instead by the fourth pass, `scan_group_members` (4217–4247), after
   `missing_required` and `bad_group_count`. When `record` meets a 33rd distinct counter with
   no slot left it sets `full`, and from then on `defers` returns `in_a_group(view, msg_type,
   tag)` (4301–4316), which walks `0..view.len()` — **the whole message** — and answers *is
   `tag` a member of a group whose counter appears anywhere in this message, before or after
   the field?*

2. **The two are not the same question, and the difference is a different Reject.** The
   manager's hand-made FIX 4.4 `35=D`, with the required `40=OrdType` omitted and a
   `PartyIDSource(447)` sitting at top level *before* the `NoPartyIDs(453)` group it is a
   member of, is answered `373=5 371=447` when the array is not full (447's counter had not
   gone past, so 447 is asked in wire order and its value is bad) and `373=1 371=40` when the
   array's answer is replaced by `in_a_group`'s (453 is somewhere in the message, so 447 is
   deferred; the scan finds nothing; `missing_required` speaks first). Same bytes, two reason
   codes, chosen by a number no document publishes. `[measured 2026-09-19]` by the manager.

3. **FIX 4.4 cannot fill the array; FIXT can, and easily.** `[measured 2026-09-19]` over the
   generated `GROUP_KEYS`: `FIX44.xml` has 731 `(msg_type, counter)` pairs and at most **23**
   distinct counters on one message type (`AllocationInstruction(J)`,
   `AllocationReport(AS)`), so no FIX 4.4 message reaches `full`. The FIXT 1.1 / FIX 5.0 SP2
   table has **25 929** pairs; `TradeCaptureReport(AE)` declares **393** counters at all
   depths, and 94 of 144 message types declare more than 32. Re-counted by the architect
   from `FIX50SP2.xml` at pin `386ce46e…` with `xml.etree`: `AE` has **48 counters at the top
   level** (137 at depth one), and 32 of the 48 have a delimiter field with no enumeration —
   so 33 one-entry top-level groups on one `AE`, no nesting needed, fill the array with every
   value legal. The table of the 48, with delimiter and type, is under *Sources*.

4. **No test reaches the fallback.** `[measured 2026-09-19]` by the manager: with a `panic!`
   in the `full` branch, `cargo test --all`, `cargo test -p fixbolt-session --features
   fix50sp2` and `cargo test -p fixbolt-engine --features fix50sp2` all exit 0 with zero
   hits; with `SEEN` lowered to 1 and the panic still in place, still zero. The branch that
   changes the answer is the branch nothing runs. The bench that ADR-0084 was accepted on,
   `validate NewOrderSingle (populated group)` in `crates/session/benches/alloc.rs`, carries
   one group — so the `SeenCounters` rustdoc's sentence *"no `alloc.rs` case sends a populated
   group"* is stale as well; B6 added that case after the sentence was written.

5. **How likely is a wire message with more than 32 distinct populated counters?** The
   architect's judgement, not a measurement: a production `TradeCaptureReport` carries on the
   order of five to twelve distinct populated groups (sides, parties per side, party sub-IDs,
   legs, underlyings, regulatory timestamps, fees, stipulations); 33 is outside anything the
   180 `.def`s, the 59, or the interop harness send, and outside what the architect has seen
   described for any venue. **That is a statement about cost, not about correctness.** The
   branch is chosen by the counterparty's bytes, so a fuzzer or a hostile peer reaches it at
   will, and a session layer whose reason code depends on a number it never published is
   not deterministic in the sense D1 means. Unlikely says the slow path may be slow; it does
   not say the slow path may answer differently.

6. **What each oracle means in FIX terms.** The array's question — *has this field's counter
   gone past?* — is the only approximation of *is this field inside a group instance* that a
   flat index affords (D2: the parser records tags in wire order and knows nothing about
   groups). A member ahead of its counter is not inside any instance; it is a stray top-level
   field, and the three engines under *What the search found* all treat it as one.
   `in_a_group`'s question — *does this message carry that group anywhere?* — is the one the
   `373=13` arm needs (4158–4163): a tag that belongs to a group the message carries repeats
   by design, wherever the repeat sits. That is why `in_a_group` exists and why it is
   position-free; it was borrowed for the deferral because it was there, not because it asks
   the deferral's question.

7. **The cost class is already on the path.** The `373=13` arm calls `view.find_from(0, tag)`
   for every field (4158; `crates/codec/src/index.rs:163–171` is a linear walk of the
   entries), so `scan_fields` is already O(n²) in the field count, bounded by `FieldIndex<N>`.
   A positional fallback that walks `0..i` per field when the array is full is the same class
   with a heavier step (`group_delimiter` and `group_members` lookups instead of a tag
   compare), on messages that carry more than 32 distinct counters and on no other.

### What the search found

The FIX Trading Community site, the OnixS reference pages and the w3.org mirror of *FIX 4.4
Volume 1* are all blocked by this session's egress proxy, so the specification is quoted from
a copy on GitHub (EPAM's *FIX Antenna* backgrounder, which reproduces the *Volume 1* field-
ordering section verbatim) and not from the primary source. The QuickFIX sources were read at
the pins named under *Sources*.

- **The specification.** *Volume 1*, general message format: *"Except where noted, fields
  within a message can be defined in any sequence … The exceptions to this rule are: … 4.
  Fields within repeating data groups must be specified in the order that the fields are
  specified in the message definition within the FIX specification document. The NoXXX field
  where XXX is the field being counted specifies the number of repeating group instances that
  must immediately precede the repeating group contents. 5. A tag number (field) should only
  appear in a message once. If it appears more than once in the message it should be
  considered an error …"* — and, in the repeating-groups section, *"The NoXXX field … occurs
  once for a repeating group and must immediately precede the repeating group contents."*
  The specification places the counter **before** its entries and never speaks of a member
  outside its group; a member ahead of its counter is, by rule 4, not part of the group's
  contents, and by rule 5 it must not then reappear inside them.
- **QuickFIX C++** (`DataDictionary.cpp` 536–589, `addXMLGroup`): a group's members are added
  to the group's own `DataDictionary` (`groupDD.addField`) and **never** to the enclosing
  message's field set; `addMsgField(msgtype, …)` (392, 515) is called for direct fields and
  for the counter only. `Message::setString` (296–369) appends a field to the top-level
  `FieldMap` unless `setGroup` (372–414) has consumed it as an entry, and `setGroup` starts
  consuming only *after* the counter. So a member ahead of its counter is a top-level field,
  and `iterate` (159–183) asks it, in this order: `checkHasValue`, `checkValidFormat`
  (`373=6`), `checkValue` (`373=5`), `checkValidTagNumber`, `checkIsInMessage` → **`373=2`**,
  `checkGroupCount` (`373=16`) — after `checkHasRequired` (`373=1`) in `validate` (123–157).
  There is no bound on groups because there is no array: `setGroup` does `new Group(…)` per
  entry.
- **QuickFIX/J** (`quickfixj-base/…/DataDictionary.java` at `master`, 1233–1275
  `addXMLGroup`, same registration: members go to `groupDD`; 343–357 `addMsgField` /
  `isMsgField`). `Message.parseBody` (684–714) does `setField(this, field)` for a field the
  dictionary does not mark as a counter, so a member ahead of its counter is a top-level
  field; `iterate` (664–689) asks every top-level field `checkHasValue`, `checkValidFormat`,
  `checkValue`, `checkField` (→ `TAG_NOT_DEFINED_FOR_THIS_MESSAGE_TYPE`, **`373=2`**, unless
  `AllowUnknownMsgFields`), `checkGroupCount`, and only then recurses into the entries.
  `parseGroup` (723–800) also does what this engine does not: a member out of the declared
  order inside an entry is `REPEATING_GROUP_FIELDS_OUT_OF_ORDER` (`373=15`, QFJ-792) and a
  member met before the delimiter is rejected (QFJ-934). No bound; recursion depth is the
  nesting depth; the price is `new Group(…)` and a `TreeMap` per entry.
- **quickfixgo** (`validation.go` at `main`, 171–276): walks the message *definition* against
  the wire; a tag the definition does not list at the current level is
  `TagNotDefinedForThisMessageType`; `validateVisitGroupField` consumes entries after the
  counter; no bound; an `iteratedTags` map is allocated per message.
- **What was not found.** No engine, tracker thread or specification text that defers a
  member's value by whether its counter is *anywhere* in the message; no engine with a fixed
  array of seen counters; no published figure for the number of distinct populated groups on a
  real `TradeCaptureReport`. Item 5 is therefore judgement, and says so.

The three engines have no bound because none of them has a flat index: each pays a heap
object per group entry, which `CLAUDE.md` §2 item 1 rules out here. **The bound is the price
of D2, not an idea borrowed from anyone**, and the way to keep it honest is to make it a cache
of an answer the slow path also gives, not a switch between two answers.

## Decision

### 1. The deferral oracle is positional

> A field's `373=5` and `373=6` wait for `373=1` and `373=16` **if and only if** it is a
> member of a repeating group of this message type whose counter appears **before it on the
> wire**. A member ahead of its counter is a stray top-level field and is answered in wire
> order like any other top-level field.

This is ADR-0084 decision 2's rule with "a group this message carries" replaced by "a group
whose counter came first", which is what the fast path always did and what the flat index can
know. It agrees with every engine read above on the one point they share — a member ahead of
its counter is *not* in the group — and it keeps the liberty ADR-0084 took and the corpus
pinned: the stray is asked in wire order (`14h`), not by tag number as QuickFIX/J's `TreeMap`
would order it, and it is asked its *value*, not refused as `373=2` (decision 4).

### 2. The fallback asks the same question the array answers

`SeenCounters::defers` on a full array calls a **positional** membership function —
`in_a_group_before(view, msg_type, tag, upto)`, walking `0..upto` where `upto` is the field's
own index — and never `in_a_group`. In `scan_group_members` the same call is made with `upto =
view.len()`: by then the array holds every counter the scan passed, position-free, so the
fallback must see every counter too, and the fourth pass asks the same superset in both modes
(decision 4). The array is then a cache of that walk's answer: the two agree by construction
on every message, and `SEEN` becomes what the rustdoc claimed it was — a number that moves
the cost and nothing else. The mechanism is the row's: it may remember the
index at which the array filled and walk only from there, if a measurement says the walk is
worth shortening; the rule fixes the answer, not the loop.

### 3. The bound stays at 32, and a test holds the sentence that says FIX 4.4 never reaches it

`SEEN = 32` stays. Widening it to the SP2 maximum (393 → 1 572 bytes zeroed on every
`validate`) is refused: it charges every message, including every FIX 4.4 message, to make
unreachable a branch that decision 2 already makes harmless. The sentence *"FIX 4.4 declares
at most 23 distinct group counters for one message type"* moves from rustdoc into a unit test
in `crates/session/src/lib.rs` — the only place `SEEN` is visible — that folds
`fixbolt_dict::GROUP_KEYS` by message type and asserts the maximum is `≤ SeenCounters::SEEN`,
so the day a dictionary crosses 32 the test says so and the fallback is known to be live for
that table (it already is for `fixt11_fix50sp2`, which the same test measures and prints).

### 4. What does not change, and is recorded as not changing

- **The `373=13` arm keeps `in_a_group`.** Its question is *does this tag repeat by design in
  this message*, which is a property of the message and not of a position; `21_…ValueOfZero`
  and `14i` pin it. Two functions with one question each is the cost, named under
  *Consequences*.
- **The fourth pass keeps asking every member.** `scan_group_members` runs with the array
  complete, so its `defers` sees every counter the scan passed — a superset of what the scan
  deferred. Asking a field the scan already passed a second time cannot change the answer
  (it passed), so the superset is harmless, and stating that here is what lets the pass stay
  one loop.
- **QuickFIX's `373=2` for a stray member and QuickFIX/J's `373=15` for a mis-ordered one are
  not adopted.** The flat `allows` table folds members into the message's tag set by design
  (D2, and `group_member_values.rs::a_member_of_a_group_the_message_does_not_carry_is_checked_in_the_scan`
  pins it), and a structural parse is a different engine. This is a divergence from both
  QuickFIX engines and it is written down as one; ADR-0084 *Consequences* already carries the
  stricter-than-C++ side of it.

### 5. The fallback is proven reached, and the false sentences are replaced

A test sends one `TradeCaptureReport` that fills the array — 33 distinct top-level one-entry
groups, with a bad-valued stray member *after* the 33rd counter and *before* its own counter,
and one group whose count disagrees — and asserts `373=5` naming the stray. A unit test on the
same bytes asserts `SeenCounters::full` after `scan_fields`, so "reached" is observed and not
inferred. The reversal (put `in_a_group` back in the fallback) has its FAIL sentence written
before it runs: `expected 373=5 371=447, engine sent 373=16 371=1907`. The rustdoc paragraph
on `SeenCounters` is rewritten to state decision 1, name the test, and drop the stale
`alloc.rs` sentence (item 4).

## Consequences

**Good**

- **The Reject a counterparty reads no longer depends on a private number.** Before this ADR,
  a message carrying more than 32 distinct counters got the same *verdict* as any other —
  every field the scan deferred is asked by the fourth pass, so nothing is accepted that would
  otherwise be rejected, and nothing rejected that would otherwise pass — but it could name a
  different fault: `373=16` or `373=1` where a smaller message would have said `373=5` on a
  stray member, or a later `373=5` where a smaller message named an earlier one. After it, the
  same bytes get the same Reject at any count. That is the whole of what a counterparty
  observes in the overflow case: **the reason code is now stable; the verdict always was.**
- The `SeenCounters` rustdoc says something true, and the truth is held by two tests instead
  of a sentence: one that fills the array, one that measures the dictionaries against `SEEN`.
- Decision 1 is the definition both the fast path and the corpus already used, so `14i` ×3,
  `21`, and every test in `group_member_values.rs` stay green unmodified; the change is
  confined to the branch nothing reached.
- The alloc case the row adds (`validate TradeCaptureReport (33 groups)`) is the first that
  drives `SeenCounters` past its capacity **and** the first that walks 33 populated groups,
  closing the "group path's allocation count is unproven" admission with a message that is
  the worst case rather than the easiest.

**Bad — and accepted**

- **A message with more than 32 distinct counters pays a quadratic walk with a table lookup
  per step**, in `scan_fields` and again in `scan_group_members`, and nothing bounds it but
  `FieldIndex<N>`. Unmeasured until the row measures it; `NO BASELINE` on the cloud box, and a
  desk number only when the desk is free. If the number is ugly, alternative D below is the
  named successor, not a wider array.
- **Two membership functions with near-identical names and one question each.** A reader
  will ask why `in_a_group` is not `in_a_group_before` with `upto = view.len()`. It could be,
  and the row may write it that way; the two *questions* stay two, and the rustdoc on each
  names its arm.
- **The FIX 4.4 path still cannot reach the fallback**, so `cargo test --all` proves nothing
  about it and the `fix50sp2` CI step (`check-feature-gated-tests-ran.sh`) is the only gate.
  A reviewer who runs the default feature set and reports green has not run this.
- **One reversal proves one message.** The equivalence of decision 2 is argued, not
  enumerated; a fuzz target that compares `validate_with` on a message against the same
  message with its groups truncated to one entry would prove more and is not in this ADR.
- **The stray-member divergence from both QuickFIX engines is now stated twice** (here and
  ADR-0084) and fixed in neither; an operator comparing Reject logs across engines sees
  `373=5` here and `373=2` there for the same stray.
- **Item 5 is judgement.** If a venue's `TradeCaptureReport` really carries 33 distinct
  populated groups, the cost above is on its hot path and this ADR did not measure it.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| **A:** keep the position-free fallback, define it as deliberate, document what a counterparty sees | Defines a reason code as a function of an unpublished capacity; the `.def` comparator is positional and would call the two answers two behaviours; nothing in the search supports "member anywhere" as a meaning of membership |
| **B:** drop the bound — a `Vec`, or a structural parse that allocates per entry as all three engines do | `CLAUDE.md` §2 item 1; D2's flat index is the design, and its cost is exactly this array |
| **C:** widen the array to the largest dictionary's maximum (393) so it never fills | 1.5 KB zeroed per message on every table including FIX 4.4, whose maximum is 23, to make unreachable a branch decision 2 makes harmless; and a third table can exceed 393 |
| **D:** a generated reverse table `member → counters` per message type plus a tag bitset of seen counters — no bound, O(1) per field | The right successor if the quadratic walk ever measures badly: a `dict` generator change (25 929 pairs on SP2), a bitset sized by the largest tag, and a row of its own. Not for a review finding on a branch that is otherwise closed |
| **E:** make the array's answer position-free too, by a pre-pass that records every counter before the scan | Same array, same overflow, one more pass on every message, and a rule the flat index cannot honour once the array is full |
| **F:** make `SeenCounters` generic over its capacity so a FIX 4.4 test can overflow it at `SEEN = 1` | A fixture bent so the branch is reached is the failure mode §10 names; the real SP2 message reaches it at 32 and is the worst case the dictionary allows |

## Sources

- `crates/session/src/lib.rs` at `f707539`: 3371–3390 (`validate`'s pass order), 3986–3997
  (`validate_with`), 3999–4089 (`SeenCounters`, its rustdoc, `record`, `defers`), 4097–4197
  (`scan_fields`; the `373=13` arm at 4158–4163, the deferral arm at 4172–4181, `record` at
  4190–4195), 4217–4247 (`scan_group_members`), 4283–4298 (`bad_group_count`), 4301–4316
  (`in_a_group`). `crates/codec/src/index.rs` 163–171 (`find_from`).
- `crates/session/benches/alloc.rs` 660–760 at `f707539`: `validate NewOrderSingle
  (populated group)`, one `453=2` group, and the comment that names it as the only populated
  group in the bench.
- The senior-review finding as verified by the PR B manager, 2026-09-19: the `35=D` of item 2
  and its two answers; the panic probe of item 4; the `GROUP_KEYS` counts of item 3.
- `vendor/quickfix/spec/FIX50SP2.xml` at pin `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`,
  re-counted 2026-09-19 with `xml.etree`, components expanded, nested groups excluded from the
  top-level count. `AE`: 393 counters at all depths, **48 at the top level**, 137 at depth one.
  The 48, as `(counter, name, delimiter, delimiter type, delimiter enumerated)` — the 32
  marked `no` are the ones a one-entry group can populate with any well-formed value:
  `1838 NoTradePriceConditions 1839 INT yes`; `1907 NoRegulatoryTradeIDs 1903 STRING no`;
  `2709 NoPriceQualifiers 2710 INT yes`; `1116 NoRootPartyIDs 1117 STRING no`;
  `454 NoSecurityAltID 455 STRING no`; `1976 NoSecondaryAssetClasses 1977 INT no`;
  `2304 NoAssetAttributes 2305 STRING no`; `864 NoEvents 865 INT yes`;
  `1018 NoInstrumentParties 1019 STRING no`; `1483 NoComplexEvents 1484 INT yes`;
  `40278 NoBusinessCenters 40471 STRING no`; `41230 NoPricingDateBusinessCenters 41231 STRING no`;
  `41092 NoMarketDisruptionEvents 41093 STRING no`; `41094 NoMarketDisruptionFallbacks 41095 STRING no`;
  `41096 NoMarketDisruptionFallbackReferencePrices 41097 INT yes`;
  `42775 NoSettlMethodElectionDateBusinessCenters 42776 STRING no`;
  `41116 NoOptionExerciseBusinessCenters 41117 STRING no`; `41137 NoOptionExerciseDates 41138 LOCALMKTDATE no`;
  `41140 NoOptionExerciseExpirationDateBusinessCenters 41141 STRING no`;
  `41152 NoOptionExerciseExpirationDates 41153 LOCALMKTDATE no`; `40049 NoStreams 40050 INT yes`;
  `40090 NoProvisions 40091 INT yes`; `40019 NoAdditionalTerms 40020 BOOLEAN no`;
  `40181 NoProtectionTerms 40182 AMT no`; `40022 NoCashSettlTerms 40023 CURRENCY no`;
  `40204 NoPhysicalSettlTerms 40209 NUMINGROUP no` (its delimiter is itself a counter — a
  one-entry group here records **two** counters); `42296 NoExtraordinaryEvents 42297 STRING no`;
  `870 NoInstrAttrib 871 INT yes`; `2734 NoIndexRollMonths 2733 STRING no`;
  `2746 NoReferenceDataDates 2747 UTCTIMESTAMP no`; `40040 NoContractualDefinitions 40041 STRING no`;
  `40046 NoFinancingTermSupplements 40047 STRING no`; `40042 NoContractualMatrices 40043 STRING no`;
  `40212 NoPayments 40213 INT yes`; `711 NoUnderlyings 311 STRING no`;
  `1647 NoRelatedInstruments 1648 INT yes`; `1703 NoCollateralAmounts 1704 AMT no`;
  `1445 NoRateSources 1446 INT yes`; `2871 NoTransactionAttributes 2872 INT yes`;
  `753 NoPosAmt 707 STRING yes`; `555 NoLegs 600 STRING no`; `768 NoTrdRegTimestamps 769 UTCTIMESTAMP no`;
  `1841 NoTradeQtys 1842 INT yes`; `552 NoSides 54 CHAR yes`; `1387 NoTrdRepIndicators 1388 INT no`;
  `2668 NoTrdRegPublications 2669 INT yes`; `41312 NoMandatoryClearingJurisdictions 41313 STRING no`;
  `2104 NoAttachments 2105 STRING no`. `AE` declares **no** required body field on this
  table, which is why the reachability test uses a `373=16` and not a `373=1` as its second
  fault. `NoPartyIDs(453)` is nested under `NoSides(552)` on `AE`, and `group_members(b"AE",
  453)` contains `447` because `GROUP_KEYS` keys nested groups by message type (the
  `every-message-type-has-a-group` and `group_tables.rs` facts).
- *FIX 4.4 Volume 1*, general message format rules 4 and 5 and the repeating-groups section,
  quoted from EPAM's copy:
  <https://github.com/epam/fix-antenna-net-core/blob/main/Docs/Backgrounder.md> (lines 55–71
  and 107–121 of the raw file, read 2026-09-19). The primary sources
  (<https://www.fixtrading.org/standards/tagvalue-online/>,
  <https://ref.onixs.biz/fix-repeating-group.html>, the w3.org mirror of
  `fix-44_VOL-1_w_Errata_20030618.doc`) were blocked by the session's egress proxy on
  2026-09-19; the quotation should be checked against one of them by whoever accepts this ADR.
- QuickFIX C++ at pin `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`, read 2026-09-19:
  `src/C++/DataDictionary.cpp` 123–157 (`validate`), 159–183 (`iterate`), 380–400
  (`readFromDocument` message fields), 490–534 (`addXMLComponentFields`), 536–589
  (`addXMLGroup`); `src/C++/DataDictionary.h` 185–199 (`addMsgField`, `isMsgField`), 512–528
  (`checkIsInMessage`, `checkGroupCount`); `src/C++/Message.cpp` 296–369 (`setString`),
  372–414 (`setGroup`, `new Group` per entry) —
  <https://github.com/quickfix/quickfix/tree/386ce46e917ae494ab6e90b1be90fd421cdbe3f9>.
- QuickFIX/J `master`, read 2026-09-19 (the organisation is `quickfix-j`, not `quickfix`):
  `quickfixj-base/src/main/java/quickfix/DataDictionary.java` 343–357, 624–689, 705–727,
  812–824, 1105–1140, 1180–1275; `quickfixj-base/src/main/java/quickfix/Message.java`
  684–800 (`parseBody`, `setField` → `TAG_APPEARS_MORE_THAN_ONCE`, `parseGroup` with QFJ-533,
  QFJ-742, QFJ-792, QFJ-934) — <https://github.com/quickfix-j/quickfixj>.
- quickfixgo `main`, read 2026-09-19: `validation.go` 108–163 (`validateFIX`,
  `validateFIXT`), 171–276 (`validateWalk`, `validateVisitField`, `validateVisitGroupField`) —
  <https://github.com/quickfixgo/quickfix>.
- The trap, generalised, is
  [a-fallback-that-answers-a-different-question](../reference/a-fallback-that-answers-a-different-question.md).
