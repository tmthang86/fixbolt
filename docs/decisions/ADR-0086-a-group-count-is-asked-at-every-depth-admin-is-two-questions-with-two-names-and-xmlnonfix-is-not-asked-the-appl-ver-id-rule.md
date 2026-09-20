# ADR-0086 — A group's count is asked at every depth, "admin?" is two questions with two names, and XMLnonFIX is not asked the `1128` rule

- **Status**: Accepted — 2026-09-20
- **Approved by**: the owner, who approved the plan
  [the-group-count-pass-and-is-admin](../plans/2026-09-20-the-group-count-pass-and-is-admin.md)
  on 2026-09-20; that plan's section *Cần một ADR — ADR-0086* states the three decisions
  below and hands their writing to the architect as step 0. **The owner approved the plan,
  not this text**; nobody has read this ADR yet. **Nothing here is built**: every test named
  below is *to be written in steps 1–4* of the plan, and none of them exists today.
- **Addendum — 2026-09-20, later the same day; it revises no decision.** The two sentences
  immediately above were true when the architect wrote them and are false now, and §5 forbids
  editing an accepted ADR's substance — so they stay as written and this line says what is
  true instead. **All four steps are built and committed**: `c8709a1` (step 1, a nested group
  no longer ends the `373=16` pass), `8e81aae` (step 2, a nested counter that lies is rejected
  and the parent is named first), `438228a` (step 3, `Tables::is_admin` generated from
  `msgcat`), `6e84ef1` (step 4, `is_admin` at the `1128` call site and the routing list renamed
  `SESSION_OWNED`). Every test named below exists and is green. One hole the plan did not
  foresee was found by the senior review of that branch and closed in the same pass:
  the three group tests all nest exactly **two** levels, so `bad_nested_count`'s recursive
  step — decision 1's whole claim that *every* depth is asked — was never executed by any
  test, and breaking it reddened nothing. `group_member_values.rs::a_counter_three_levels_down_that_lies_is_rejected`
  (`552 -> 453 -> 802` on a `NewOrderCross`, parents honest, the bottom counter lying) is the
  test that now takes that step, proved by reversal. Where this ADR says the cost to `validate`
  is unmeasured, see `STATUS.md`'s *Not proven*: an instrument exists
  (`crates/session/benches/validate.rs`'s `validate TradeCaptureReport (33 groups)`) and a
  laptop A/B reads about +8 % of the pass, which `CLAUDE.md` §2 non-negotiable 10 does not let
  anyone publish — the band is still owed.
- **Date**: 2026-09-20
- **Deciders**: Tran Manh Thang. Written by the architect from the approved plan. The
  architect re-read the upstream sources on 2026-09-20 (*What the search found*); the
  `file:line` facts about `crates/` are the plan's, and the architect did not re-derive them.
- **Related**:
  [ADR-0080](ADR-0080-the-dictionary-rides-the-encoding-and-a-fixt-session-is-one-table-built-from-two-xml-files.md)
  decision 3 — **narrowed here, not superseded**: its `1128` rule stands for every
  application message; what changes is that `35=n` stops being counted as one for that rule
  (decision 3 below).
  [ADR-0084](ADR-0084-a-session-message-is-checked-against-the-layer-that-defines-it-a-members-value-waits-for-the-count-and-fix50-is-its-own-oracle.md)
  decision 1, which refused to branch on the session's `ADMIN` because *"a call-site list is
  exactly what D3 forbids, and it disagrees with the transport file on `n`"* — this ADR
  leans on that sentence and finishes what it left open.
  [ADR-0085](ADR-0085-a-member-waits-for-a-counter-that-came-before-it-and-the-array-is-only-a-cache.md)
  decisions 2–4: `bad_group_count` (`373=16`) runs before `scan_group_members`
  (`373=5`/`373=6`), so widening the set of counters asked `373=16` can change which reason
  code a message gets. `DESIGN.md` D2, D3; `CLAUDE.md` §2 items 1, 2, 3, 5, 7.
- **Answers**: whether `SessionRejectReason 16` is asked of a nested group; who says a
  message type is administrative, and whether routing and validation must agree on it; and
  whether ADR-0080's `1128` rule applies to `35=n`.

## Context

The `file:line` references are `crates/session/src/lib.rs` as the plan read it, unless
another file is named.

1. **One nested group kills the whole `373=16` pass.** `bad_group_count` (4309–4324) returns
   `Option` and uses `?` at 4314 and 4318. It walks every field; `D::group_delimiter` answers
   `Some` for a nested counter exactly as for a top-level one (the table is flat, keyed by
   `(msg_type, counter)`); `MessageView::group` is top-level only
   (`crates/codec/src/group.rs:165-186`) and gives `None` for a nested counter; the `?` then
   leaves the function with `None`, which here means *no fault*. Every counter after the
   first nested group is never asked. `STATUS.md` records 12 of `AE`'s 45 counters below
   5000 as nested. The two sibling functions directly below, `in_a_group_before` (4364) and
   `in_a_group` (4395), walk the same way with `let … else { continue; }` and miss nothing.
2. **The way down already exists.** `GroupEntry::group` (`group.rs:148`) opens a nested group
   inside an entry's `[start, end)`; `GroupIter` gives `declared()` and `counted()`, the
   latter computed when the group is opened (`group.rs:125`). All borrowed views.
3. **`ADMIN` is one hand-written list answering two questions.** Seven entries, no `n`, two
   call sites: 3407 (`is_application`, used once, at 3834 — the gate to `app.on_message` and
   to the journal that feeds resend) and 4296 (`out_of_family_appl_ver_id`, the exemption
   from ADR-0080 decision 3's `1128` rule). The first is a **routing** question about this
   engine; the second is a **dictionary** question.
4. **Both XML files call `n` administrative.** `FIXT11.xml`: eight `<message>`s, all
   `msgcat='admin'` ([fixt-dictionary-traps](../reference/fixt-dictionary-traps.md));
   `FIX44.xml`: `<message name='XMLnonFIX' msgtype='n' msgcat='admin' />`
   ([fix44-dictionary-traps](../reference/fix44-dictionary-traps.md)). `vendor/` was empty on
   the machine that wrote the plan and this ADR, so these are read from `docs/reference/`,
   not from the XML; step 3 fetches before it builds. `is_defined_tag_for` already treats `n`
   as a transport message (ADR-0084 decision 1), so today `n` is a transport message for
   `373=0` and an application message for `1128` — one message, two layers, by accident.
5. **ADR-0080 is still marked `Proposed`.** The plan calls its decision 3 *đã Accepted*; the
   file's status line says `Proposed — 2026-09-19`, although the decision is built and
   merged. `CLAUDE.md` §5 would therefore permit revising ADR-0080 in place. This ADR is
   written anyway, as the approved plan directs: the behaviour is shipped and cited, the
   other two decisions need a number regardless, and a narrowing recorded beside them is
   easier to find than a revision note. The discrepancy is reported to the manager, not
   resolved here.

### What the search found

Every URL below was re-fetched on 2026-09-20 (raw file for the GitHub sources) and **says
what the plan says it says**; no citation had to be changed.

- **The specification does not exempt a nested group.** `SessionRejectReason(373)` value 16
  is *"Incorrect NumInGroup count for repeating group"*
  (<https://www.onixs.biz/fix-dictionary/4.4/tagnum_373.html>, verified). B2BITS describes
  the same rule in the same words, with no exception for nesting
  (<https://kb.b2bits.com/display/B2BITS/Explanation+of+log+messages+about+validation+and+parsing+errors>,
  verified). **No text was found** that limits reason 16 to top-level groups.
- **QuickFIX asks the count at every depth.** QuickFIX/n `DataDictionary.cs`: `Iterate`
  calls `CheckGroupCount` (line 191) and `IterateGroup` per instance (214); `IterateGroup`
  (220) calls `CheckGroupCount` (242) and itself (255); `CheckGroupCount` (399–405) throws
  `RepeatingGroupCountMismatch(field.Tag)` when
  `map.GetInt(field.Tag) != map.GroupCount(field.Tag)`
  (<https://github.com/connamara/quickfixn/blob/master/QuickFIXn/DataDictionary/DataDictionary.cs>,
  verified). QuickFIX/J `DataDictionary.java` has the same pair, `iterate` (664) calling
  `checkGroupCount` (678, defined 812).
- **The three QuickFIX engines disagree on `n`.**
  - QuickFIX C++ `Message.h:299`:
    `return strchr("0A12345", msgType.getValue().c_str()[0]) != 0;` — seven, no `n`
    (<https://github.com/quickfix/quickfix/blob/master/src/C%2B%2B/Message.h>, verified).
  - QuickFIX/J `MessageUtils.java:122`:
    `return msgType.length() == 1 && "0A12345".contains(msgType);` — seven, no `n`
    (<https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-base/src/main/java/quickfix/MessageUtils.java>,
    verified).
  - QuickFIX/n `Message.cs:72`:
    `return msgType.Length == 1 && "0A12345n".Contains(msgType[0]);` — eight, with `n`
    (<https://github.com/connamara/quickfixn/blob/master/QuickFIXn/Message/Message.cs>,
    verified); its `RELEASE_NOTES.md` lists *"issue #93 - bugfix: change 'h' to 'n' in
    Message.IsAdminMsgType"*, verified.
  - All three define `isApp()` as the negation of `isAdminMsgType()`: one question, two
    names. Decision 2 departs from exactly that.
- **What was not found.** No engine that separates "the dictionary calls it admin" from "the
  session layer owns it"; no `.def` that sends `35=n`; no published cost for a recursive
  group-count check.

## Decision

### 1. `373=16` is asked at every nesting depth, depth-first, down to a constant

- `bad_group_count` replaces its two `?` with `let … else { continue; }`, the shape of
  `in_a_group_before`. A counter the view cannot open as a top-level group is a field the
  loop passes, not the end of the pass.
- After a top-level group's own count is compared, each entry is handed to
  `bad_nested_count`, which walks `D::group_members(msg_type, parent)`, opens every member
  that is itself a counter with **`GroupEntry::group`** — the existing `codec` API, nothing
  new — compares `declared()` with `Some(counted())`, and recurses per child entry with
  `depth + 1`.
- The descent stops at **`MAX_GROUP_NESTING = 8`** and answers `None` there. The constant
  bounds the stack and defends against a table whose member set contains its own counter. It
  is chosen, not measured; a test measures the dictionaries against it (below).
- **The order of report is depth-first, immediately after the parent's own count — wire
  order**, because a nested group sits between its parent counter and the next top-level
  field. When parent and child are both wrong, the counterparty reads `371=` of the
  **parent**.
- No allocation, no `format!`, no clock: recursion is stack, the views are borrowed, the
  fault stays `SessionText` + `Held<12>` (`CLAUDE.md` §2 items 1 and 2).

Held by — **all to be written in steps 1–4, none exists today**:
`crates/session/tests/group_member_values.rs::a_counter_after_a_nested_group_is_still_checked`
(step 1; expected FAIL before the fix: `expected Reject 373=16, engine sent no reject`),
`::a_nested_counter_that_lies_is_rejected` and `::a_parent_counter_is_named_before_its_child`
(step 2), `crates/dict/tests/group_tables.rs::the_generated_tables_never_nest_deeper_than_the_walk_goes`
(step 2; folds `GROUP_KEYS`, prints the real maximum depth of both tables, asserts
`<= MAX_GROUP_NESTING`), and the existing alloc case `validate TradeCaptureReport (33
groups)` in `crates/session/benches/alloc.rs`, which must still read **0**. The 59 must
still print `59 / 59`; any other number, higher included, stops the step.

### 2. "Admin?" is two questions, and each gets its own name

- **The dictionary's question** — *do the XML files call this message administrative?* — is
  `Tables::is_admin(msg_type: &[u8]) -> bool`, **generated** in `crates/dict/build.rs` from
  each `<message>`'s `msgcat`, for both `Fix44` and the FIXT pair. A `<message>` with no
  `msgcat` is a `die(…)`, never a guessed default. The trait method has **no default
  implementation**, for ADR-0084 decision 1's reason: a default hands a future third table
  FIX 4.4 semantics silently. This is the source of truth for **validation** rules.
- **The engine's question** — *does this session layer answer the message itself, or hand it
  to the application and journal it for resend?* — is `SESSION_OWNED`, the same **seven**
  entries `0 1 2 3 4 5 A`, which is `ADMIN` renamed with its rustdoc rewritten to say it is
  this engine's routing list and not the dictionary's answer, naming the three QuickFIX
  engines and the number seven. `is_application` (3407) uses it. **`35=n` still reaches
  `app.on_message` and is still journalled; routing does not change.**
- The two lists differ by exactly `n`, on purpose. `msgcat` is accepted as truth where
  ADR-0084 decision 1 demands it and deliberately **not** applied to routing: XMLnonFIX has
  no session content for a session layer to answer, and on routing this engine stands with
  QuickFIX C++ and QuickFIX/J, against QuickFIX/n.

Held by — **to be written in steps 3–4**:
`crates/dict/tests/fixt.rs::the_transport_files_admin_set_is_the_tables_admin_set`,
`crates/dict/tests/tables.rs::fix44_calls_xmlnonfix_admin` (step 3; expected FAIL:
`error[E0599]: no function or associated item named 'is_admin' found`),
`crates/session/tests/fixt.rs::xmlnonfix_still_reaches_the_application`, and the unit test
`every_admin_type_is_session_owned_except_xmlnonfix` in `crates/session/src/lib.rs` (step
4), which fails the day a dictionary grows a ninth admin type instead of letting it fall
silently to the application.

### 3. `35=n` is exempt from ADR-0080 decision 3's `1128` rule

`out_of_family_appl_ver_id` (4296) asks `D::is_admin(msg_type)` instead of `ADMIN`. **This
changes behaviour**: an XMLnonFIX message carrying a `1128` that names FIX 4.x or an unknown
value is no longer answered `Reject 373=5 371=1128`. `n` is then a transport-layer message
for `373=0` (ADR-0084 decision 1) and for `1128` alike — one message, one layer. ADR-0080
decision 3 is otherwise untouched: every application message is still asked the rule, and
its open item (*`1128` is not permitted on session messages, and is not enforced*) stays
open, now covering `n` as well.

Held by — **to be written in step 4**:
`crates/session/tests/fixt.rs::an_xmlnonfix_message_is_not_asked_the_appl_ver_id_rule`
(expected FAIL before the fix: `expected no reject, engine sent 373=5 371=1128`).

## Consequences

**Good**

- A lying counter after, or inside, a nested group is rejected. Today the first nested
  counter on the wire silently switches reason 16 off for the rest of the message — on `AE`,
  the message a venue sends most of, 12 of 45 counters are nested.
- The engine now does what the specification says and what QuickFIX/n and QuickFIX/J do on
  the group count; this is not an invention.
- No hand-written list answers a dictionary question any more (D3, `CLAUDE.md` §2 item 5),
  and the one list that remains says in its name and rustdoc whose it is.
- `n` stops being a transport message for one rule and an application message for another.
- The distance between the two lists is held by a test, not by a reader's memory.

**Bad — and accepted**

- **`validate` does more work on every message that carries a group, and NOBODY HAS
  MEASURED how much.** Not "small", not "negligible": unknown. There is no `DESIGN.md` §9
  machine in this work. The only nearby figure — `validate TradeCaptureReport (33 groups)`
  at 47 793.4 ns/op, from ADR-0085's status block — is `NO BASELINE` on an untuned Xeon,
  **unpublishable** under `CLAUDE.md` §2 item 10, predates this change, and says nothing
  about the descent. The cost is owed to the §9 desk and belongs in `STATUS.md` *Not
  proven* until paid. The top-level walk already re-opens each group from the flat index;
  the descent adds a `group_members` walk and a `group_delimiter` lookup per member per
  entry, on the same path ADR-0085 already calls quadratic.
- **Recursion in a pure session layer, bounded only by a constant a test must hold.**
  `MAX_GROUP_NESTING = 8` is a chosen number. If
  `the_generated_tables_never_nest_deeper_than_the_walk_goes` is never written, or a
  dictionary nests deeper and someone raises the assert instead of the constant, counters
  below depth 8 go unasked **silently** — the same class of hole this ADR closes, one level
  further down. At the bound the answer is `None` (no fault), not a Reject; that is a
  choice, and it favours accepting over refusing.
- **A message can now get a different reason code than it did yesterday.** `373=16` runs
  before `373=5`/`373=6` on members (ADR-0085). A message with a lying nested counter *and*
  a bad member value was answered `373=5`; it will be answered `373=16`. The verdict is
  unchanged, the code is not, and only `14i` exercises a populated group in the 59.
- **This engine now disagrees with QuickFIX/n on routing** — there `35=n` is admin and goes
  to `FromAdmin`; here it goes to the application and is resent rather than gap-filled —
  **and with QuickFIX C++ and QuickFIX/J on the `1128` exemption**, where `n` is an
  application message throughout. No reference engine matches this engine on both. An
  operator comparing logs across engines will see it, and no `.def` arbitrates: none sends
  `35=n`.
- **Two names for what every reference engine treats as one question.** A reader arriving
  from QuickFIX will look for `is_app == !is_admin` and not find it. The rustdoc on
  `SESSION_OWNED` and the unit test are the only things that stop someone "fixing" it.
- **`is_admin` has no default, so every future `Tables` implementor must write it**, and a
  dictionary whose `<message>` lacks `msgcat` breaks the build rather than degrading.
- **ADR-0080 decision 3 is narrowed, not superseded.** ADR-0080 keeps its text and gains
  only a one-line pointer in its status line, the way ADR-0084 points at ADR-0085. A reader
  of ADR-0080 alone still reads "`1128` on an application message" and must follow the
  pointer to learn `n` is not one. ADR-0080 is also still `Proposed` while built and merged
  (*Context* item 5); this ADR does not fix that.
- **The `vendor/` XML was not read for this ADR.** *Context* item 4 rests on
  `docs/reference/`; step 3 is the first time `msgcat` is read from the files, and if a
  `<message>` lacks it the build says so then, not now.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| **A:** fix the two `?` only — skip nested counters, continue, and document that reason 16 is top-level only | Closes the dead pass and keeps the hole: the specification exempts nothing, QuickFIX/n and QuickFIX/J check every depth, and on `AE` 12 of 45 counters would stay unasked — most of a venue's real traffic, not a corner. It is step 1 of the plan, as a stage, never as the end state |
| **B:** add `b"n"` to the seven-entry list | Closes the `1128` disagreement and **silently changes resend, gap-fill and application routing for `35=n`**: a message no longer journalled is a message a gap fill overwrites. No `.def` sends `35=n`, so no gate would see it — the kind of change `CLAUDE.md` §10 says must not be made quietly. It would follow QuickFIX/n (`"0A12345n"`) against QuickFIX C++ and QuickFIX/J (`"0A12345"`, both verified 2026-09-20), and it would still be a call-site list answering a dictionary question (D3) |
| **C:** one generated `is_admin` used at both call sites | Same routing change as B, arrived at from the other side, and it would let the XML decide what this session layer knows how to answer. XMLnonFIX has no session content; the session layer can do nothing with it but hand it on. Every QuickFIX engine has this coupling (`isApp = !isAdmin`), and it is why they disagree with each other on `n` |
| **D:** keep `ADMIN` by that name, add `is_admin` beside it | The old name is the cause: "admin" reads as the dictionary's answer. Two things called admin that differ on `n` is the present bug with a second copy |
| **E:** a default `is_admin` on `Tables` | ADR-0084 decision 1's argument, unchanged: a third table would inherit FIX 4.4 semantics without anyone deciding it |
| **F:** no depth bound — recurse as far as the tables go | The tables are generated from third-party XML; a member set containing its own counter is unbounded recursion in a crate that may not panic (`CLAUDE.md` §2 item 7). A bound plus a test that measures the dictionaries is ADR-0085 decision 3's shape |
| **G:** an explicit stack instead of recursion | Same bound, same stack memory, more code, and an array of `GroupIter`s to size by the same constant. Nothing measured says recursion is the problem; if the §9 desk says so, this is the named successor |
| **H:** revise ADR-0080 in place, since it is still `Proposed` | Permitted by §5, and not what the approved plan directs. The behaviour is shipped and cited by other documents; decisions 1 and 2 need a number regardless |

## Sources

- The approved plan,
  [the-group-count-pass-and-is-admin](../plans/2026-09-20-the-group-count-pass-and-is-admin.md):
  *Những gì đã biết chắc* (every `file:line` fact in *Context*), *Cách làm*, *Cần một ADR*,
  and the *Chia việc* table (every test name above).
- ADR-0080 decision 3; ADR-0084 decision 1; ADR-0085 decisions 2–4 and its status block (the
  47 793.4 ns/op figure and its `NO BASELINE` label).
- [fixt-dictionary-traps](../reference/fixt-dictionary-traps.md) and
  [fix44-dictionary-traps](../reference/fix44-dictionary-traps.md) for `msgcat`.
- Upstream, all re-fetched 2026-09-20 at `master` and found to say what the plan quotes —
  the URLs are inline under *What the search found*: OnixS FIX 4.4 tag 373; the B2BITS
  knowledge-base page; QuickFIX C++ `Message.h:299`; QuickFIX/J `MessageUtils.java:122` and
  `DataDictionary.java` 664/678/812; QuickFIX/n `Message.cs:72`,
  `DataDictionary.cs` 191–255 and 399–405, and `RELEASE_NOTES.md` (issue #93). These are
  branch heads, not pins; line numbers will drift.
