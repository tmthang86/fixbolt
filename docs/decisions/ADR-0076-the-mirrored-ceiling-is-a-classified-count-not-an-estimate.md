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
2. **The classification is a table in `crates/conformance/src/mirror.rs`** (new), one row per
   mirrored file: `Reachable`, `NeedsUnsequencedReset` (`34=0`), `NeedsGapFillAsAction`
   (`123=Y` originated by the operator), `NeedsHarnessDirective` (ADR-0006's directive case),
   or `Unclassified`. A test asserts **`Unclassified` is empty**, a second asserts the
   `Reachable` count equals the number the score test uses as its ceiling, and a third asserts
   every `NeedsUnsequencedReset` file really contains an `I` line with `34=0` and every
   `NeedsGapFillAsAction` one contains `123=Y` — the table is checked against the files, not
   hand-copied.
3. **The score gate reads its ceiling from that table.** *Passing* is `score == Reachable`;
   a `Reachable` file that fails is a session defect, as today. A file in any `Needs*` class is
   reported by name and never counted against the session.
4. **The reasons are decisions, not gaps**: `34=0` is refused by ADR-0042 decision 1;
   `123=Y` as an operator action is refused because the session already sends it when it
   should; a directive is refused by ADR-0006. Each class names its ADR in the enum's rustdoc.
5. **The number goes into this ADR when the table exists** (the plan's step that builds
   `mirror.rs`), in a *Measured* paragraph below the decision, with the commit and the CI run.
   Until then this ADR states the rule and no ceiling.

## Consequences

**Good**

- Item 36 closes on a number that a test holds, and a file can only move between classes by
  a commit that changes the table and its test.
- The refusals are traced to the ADR that made them, so a future *"why not 50?"* has a
  one-line answer per file.

**Bad — and accepted**

- **The measured ceiling may be well under 45**, and the mirrored score will look worse
  than the estimate did. Correct is worse to read than optimistic.
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
