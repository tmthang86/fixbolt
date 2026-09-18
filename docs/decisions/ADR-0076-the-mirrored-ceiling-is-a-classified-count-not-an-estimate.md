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
   asked. **ADR-0006's 45 is superseded on that point**: the ceiling is **14 of 50**.

## Measured — 2026-09-18, commit `a2aa9ce`

`[measured 2026-09-18]` `crates/conformance/src/mirror.rs`, 50 rows, commit **`a2aa9ce`** on
`plan/the-second-linux-desk-c` (CI run id: *to be filled by the manager on the closing commit*).

| Class | Files | Refused by |
|---|---|---|
| `Reachable` | **14** | — |
| `NeedsHeaderTheApiDoesNotSet` | 11 | ADR-0042 d1 |
| `NeedsMalformedOutput` | 11 | ADR-0042 d1 (ADR-0006 counted 5 of these) |
| `NeedsBodyOrderTheTableRefuses` | 6 | non-negotiable 5 / D3 |
| `NeedsUnsequencedReset` | 3 | ADR-0042 d1 |
| `NeedsGapFillAsAction` | 3 | this ADR, decision 4 |
| `NeedsAdminMessageTheApiDoesNotOriginate` | 2 | non-negotiable 2 |
| `NeedsHarnessDirective` | 0 | ADR-0006 (file dropped before the table) |
| `Unclassified` | 0 | forbidden by test |

**The ceiling is 14, not 45.** ADR-0006 reasoned from message lines and directives; the
table reads every `I` line against what the API can be asked to say, and 36 of the 50 need
bytes only a back door could produce. The 14: `13b_UnsolicitedLogoutMessage`,
`1a_ValidLogonMsgSeqNumTooHigh`, `1a_ValidLogonWithCorrectMsgSeqNum`, `2a_MsgSeqNumCorrect`,
`2k_CompIDDoesNotMatchProfile`, `2o_SendingTimeValueOutOfRange`, `2q_MsgTypeNotValid`,
`4a_NoDataSentDuringHeartBtInt`, `4b_ReceivedTestRequest`, `6_SendTestRequest`,
`8_AdminAndApplicationMessages`, `8_OnlyApplicationMessages`, `AlreadyLoggedOn`, `ReverseRoute`.

**The score today is 10 of 14.** Four `Reachable` files are red, and by this ADR's own rule
that is a claim to be settled, not a reason to move a row: `STATUS.md` item 92 holds them,
with the two hypotheses (a harness hole in `make_receivable` — a `PossDup` built without
`122=OrigSendingTime`, answered by the engine with `Reject 45=2 371=122`; and a harness that
never calls `Session::resume(cfg, 5, 1)` for `1a_ValidLogonMsgSeqNumTooHigh`) and the test that
decides each. Neither is a session defect on today's evidence.

## Consequences

**Good**

- Item 36 closes on a number that a test holds, and a file can only move between classes by
  a commit that changes the table and its test.
- The refusals are traced to the ADR that made them, so a future *"why not 50?"* has a
  one-line answer per file.

**Bad — and accepted**

- **The measured ceiling is well under 45 — 14** — and the mirrored score reads 10 / 14 where
  it read 10 / 50 against a ceiling of 45. Correct is worse to read than optimistic.
- **Classifying 34 files is judgement**, one by one, against the initiator's public API as it
  is today; a later API addition (say an operator-originated gap fill, if ever wanted) moves
  files from `Needs*` to `Reachable` and needs the table edited — by design, but by hand.
- **A `Reachable` verdict is a claim until the file passes.** The plan's step will find
  session defects the way the 2 → 10 jump did; those are wins, but they are also unplanned
  session work under non-negotiable 3.

## Sources

- `STATUS.md` item 36 `[measured 2026-09-02]`; ADR-0006; ADR-0042 decision 1.
- Search 2026-09-18 for a published mirrored/initiator-side acceptance score in QuickFIX,
  QuickFIX/J, Artio: nothing found.
