# ADR-0076 — The mirrored corpus's ceiling is a classified count, not an estimate

- **Status**: Proposed — 2026-09-18. **Supersedes** [ADR-0006](ADR-0006-mirrored-corpus-is-fifty.md)'s
  ceiling of 45 only; ADR-0006's exclusion of `1b_DuplicateIdentity.def` and its rule about
  directives stand.
- **Date**: 2026-09-18
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0004](ADR-0004-bidirectional-engine.md) decision 6,
  [ADR-0042](ADR-0042-a-second-implementation-is-the-only-independent-opinion.md) decision 1
  (no byte back door), `crates/session/tests/goodbye.rs`, `crates/conformance/src/script.rs`,
  `STATUS.md` open item 36

## Context

The mirrored corpus reads **10 / 50** `[measured 2026-09-02]`, and ADR-0006 gave it a ceiling of
45 by reasoning. Item 36 measured that the ceiling is wrong in the other direction: at least
six of the remaining files need this end to *originate* a `SequenceReset` in shapes no operator
API should expose — three want `34=0` (QuickFIX's *not sequenced*, which would let the harness
dictate a sequence number, the byte back door ADR-0042 refuses) and three want `123=Y`, which
is not an operator action but the session's own answer to a `ResendRequest`. The other 34 are
unclassified, and the item's own conclusion is that the classification is worth more than the
score.

An accepted ADR is never edited, so the wrong number needs a new one. What the search found
(2026-09-18): no other engine publishes a mirrored-corpus score — QuickFIX's `.def` files are
acceptor-side by construction, QuickFIX/J runs the same set, Artio's acceptance suite is its
own — so there is no external ceiling to borrow. The ceiling has to be this repository's
classification.

## Decision

1. **The ceiling of 45 is withdrawn.** The mirrored ceiling is **the number of files whose
   every `I` line, mirrored, is something the initiator's public API can be asked to do** — a
   count produced by a committed classification, not an estimate.
2. **The classification is a table in `crates/conformance/src/mirror.rs`**, one row per
   mirrored file, each naming the `I` line that decided it. `[revised 2026-09-18, on building
   it]` Five classes were not enough; the enum has eight, and **each refusing class names the
   decision that refuses it**:
   - `Reachable` — every `I` line is one of the six calls the API takes (`connect`,
     `send_heartbeat`, `send_test_request`, `send_resend_request`, `send_sequence_reset`,
     `begin_logout`, `send_application`) or a message the session emits on its own;
   - `NeedsUnsequencedReset` (`34=0`) — refused by ADR-0042 decision 1;
   - `NeedsGapFillAsAction` (`123=Y` as an operator order) — refused by decision 4 below;
   - `NeedsHeaderTheApiDoesNotSet` (`43=Y`/`122=`, `97=Y`, or a `34=` the file chooses) — the
     header is the session's `Template`; a call site that set these is ADR-0042's back door;
   - `NeedsAdminMessageTheApiDoesNotOriginate` (an unsolicited `Reject`, a second `Logon`) —
     the session owns both, `CLAUDE.md` §2 non-negotiable 2;
   - `NeedsBodyOrderTheTableRefuses` — every field sayable, but the file's order is not the
     generated table's and the comparator is positional: non-negotiable 5 / D3;
   - `NeedsMalformedOutput` — bytes no correct engine emits (wrong `9=`/`10=`, a missing
     required field, a repeated tag, a lying group count, a foreign `CompID`, a stale `52=`):
     ADR-0042 decision 1 again. This is ADR-0006's *"deliberately wrong Logon"* set, which it
     counted as five; the table finds **eleven**;
   - `NeedsHarnessDirective` — ADR-0006's case; **empty** among the 50, because `mirrors`
     drops `1b_DuplicateIdentity.def` before the table sees it; kept so a corpus change has
     somewhere honest to land;
   - `NeedsAScriptThatAnswers` `[added 2026-09-18, item 92]` — every `I` line is sayable and
     this end says it, but the file's *script* side then falls silent where a real peer would
     answer, or expects a message a correct initiator does not send: `6_SendTestRequest` (we
     answer the `TestRequest`; no `I` line claims the answer) and `1a_ValidLogonMsgSeqNumTooHigh`
     (the script's `ResendRequest` is never answered by the script, we gap-fill, the file
     expects our `Logout`). Mirrored, the `E` side is the acceptor harness's own script, which
     never had to answer because the acceptor under test never asked — ADR-0006's reasoning
     about `i1,DISCONNECT`, one step further. Refused by non-negotiable 3 read the right way
     round: the 59 judge the session, and a session that goes quiet to match a script is a
     worse session. `NeedsHarnessDirective` does not fit — it is about a directive line the
     harness cannot express, not about the script's behaviour — so this is a ninth variant;
     `crates/conformance/src/mirror.rs` gains it in the commit that closes 3.2.
   - `Unclassified` — forbidden by test.
   Tests (`crates/conformance/tests/mirror_classification.rs`): the row set **is** the set
   `mirrors` keeps; `Unclassified` is empty; the `34=0` and `123=Y` rows contain those bytes;
   the `Reachable` count is the ceiling the score test reads.
3. **The score gate reads its ceiling from that table.** *Passing* is `score == Reachable`;
   a `Reachable` file that fails is a session defect, as today. A file in any `Needs*` class is
   reported by name and never counted against the session.
4. **The reasons are decisions, not gaps**: `34=0` is refused by ADR-0042 decision 1;
   `123=Y` as an operator action is refused because the session already sends it when it
   should; a directive is refused by ADR-0006. Each class names its ADR in the enum's rustdoc.
5. **The number is in *Measured* below**, with the commit and the CI run, as this decision
   asked. **ADR-0006's 45 is superseded on that point**: the ceiling is **14 of 50**, and the
   score meets it.

## Measured — 2026-09-18, commits `a2aa9ce` and `09effd1`

`[measured 2026-09-18]` `crates/conformance/src/mirror.rs`, 50 rows, first at **`a2aa9ce`**,
corrected at **`09effd1`** (both on `plan/the-second-linux-desk-c`; CI run id: *to be filled by
the manager on the closing commit*).

| Class | Files | Refused by |
|---|---|---|
| `Reachable` | **14** | — |
| `NeedsMalformedOutput` | 11 | ADR-0042 d1 (ADR-0006 counted 5 of these) |
| `NeedsHeaderTheApiDoesNotSet` | 9 | ADR-0042 d1 |
| `NeedsBodyOrderTheTableRefuses` | 6 | non-negotiable 5 / D3 |
| `NeedsUnsequencedReset` | 3 | ADR-0042 d1 |
| `NeedsGapFillAsAction` | 3 | this ADR, decision 4 |
| `NeedsAdminMessageTheApiDoesNotOriginate` | 2 | non-negotiable 2 |
| `NeedsAScriptThatAnswers` | 2 | non-negotiable 3, ADR-0006's reasoning |
| `NeedsHarnessDirective` | 0 | ADR-0006 (file dropped before the table) |
| `Unclassified` | 0 | forbidden by test |

**The ceiling is 14, not 45, and the score is 14 / 14.** At `a2aa9ce` the table read 14 and the
score 10 / 14; `STATUS.md` item 92 held the four red `Reachable` rows. `09effd1` closed it as a
**harness hole, not a session defect**: `make_receivable` substituted the corpus's placeholder
timestamp (`00000000-00:00:00`) only in `52=`, while the session parses `122=` and `60=` as
`UTCTimestamp` too (FIX 4.4) and rejected them with `371=122/373=1` and `371=60/373=6`; three
tags are substituted now, three unit tests hold it, and the harness's `Adapter::at` resumes at
the first `I` line's `34=` (it moves no number). **Nothing in `crates/session/src` changed; the
59 read 59 / 59 both ways.** The composition of the 14 moved by two in each direction:

- `8_AdminAndApplicationMessages` and `8_OnlyApplicationMessages` went green — they were
  `Reachable` and red for the harness's reason;
- `19a_PossResendMessageThatHAsAlreadyBeenSent` and `19b_PossResendMessageThatHasNotBeenSent`
  **pass**, so they are `Reachable`, and their `NeedsHeaderTheApiDoesNotSet` rows (*"97=Y,
  which no send_application call sets"*) were wrong — the `97=Y` line is on the path the file
  drives and the session produces it; the rows move to `Reachable` and the `why` is rewritten
  to say which call produces it;
- `6_SendTestRequest` and `1a_ValidLogonMsgSeqNumTooHigh` stay red **because the corpus's
  script side falls silent where a correct initiator answers** — they are not `Reachable`; they
  are the two rows of the new class `NeedsAScriptThatAnswers` (decision 2).

So 14 − 2 + 2 = **14 `Reachable`, 14 green**. The 14: `13b_UnsolicitedLogoutMessage`,
`19a_PossResendMessageThatHAsAlreadyBeenSent`, `19b_PossResendMessageThatHasNotBeenSent`,
`1a_ValidLogonWithCorrectMsgSeqNum`, `2a_MsgSeqNumCorrect`, `2k_CompIDDoesNotMatchProfile`,
`2o_SendingTimeValueOutOfRange`, `2q_MsgTypeNotValid`, `4a_NoDataSentDuringHeartBtInt`,
`4b_ReceivedTestRequest`, `8_AdminAndApplicationMessages`, `8_OnlyApplicationMessages`,
`AlreadyLoggedOn`, `ReverseRoute`. The trap is written up in
[reference/a-corpus-placeholder-timestamp-lives-in-three-tags.md](../reference/a-corpus-placeholder-timestamp-lives-in-three-tags.md).
The score test asserts `score == Reachable` with no `KNOWN_RED` array — the array the plan's
3.2 row proposed was never needed.

## Consequences

**Good**

- Item 36 closes on a number that a test holds, and a file can only move between classes by
  a commit that changes the table and its test.
- The refusals are traced to the ADR that made them, so a future *"why not 50?"* has a
  one-line answer per file.

**Bad — and accepted**

- **The measured ceiling is well under 45 — 14** — and the mirrored score reads 14 / 14 where
  it read 10 / 50 against a ceiling of 45. Correct is worse to read than optimistic, and a full
  score against a small ceiling invites the question *"why so few?"*, whose answer is the table.
- **Classifying 34 files is judgement**, one by one, against the initiator's public API as it
  is today; a later API addition (say an operator-originated gap fill, if ever wanted) moves
  files from `Needs*` to `Reachable` and needs the table edited — by design, but by hand.
- **A `Reachable` verdict is a claim until the file passes**, and a passing file is a claim
  about the row's *reason* too: two rows were green under a wrong `why` (19a/19b) and two were
  `Reachable` for a wrong reason (6, 1a). The tests check bytes, not reasons.

## Sources

- `STATUS.md` item 36 `[measured 2026-09-02]`; ADR-0006; ADR-0042 decision 1.
- Search 2026-09-18 for a published mirrored/initiator-side acceptance score in QuickFIX,
  QuickFIX/J, Artio: nothing found.
