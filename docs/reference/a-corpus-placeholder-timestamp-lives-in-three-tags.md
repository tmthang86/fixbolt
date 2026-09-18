# A corpus placeholder timestamp lives in three tags, and a real parser rejects all three

> `[measured 2026-09-18]` — found closing `STATUS.md` item 92, commit `09effd1`, step 3.2 of
> [plans/2026-09-18-closing-the-open-items.md](../plans/2026-09-18-closing-the-open-items.md).
> Sibling of [expected-output-is-not-valid-input.md](expected-output-is-not-valid-input.md).
> **`[to testing-skills]`**

## The shape

QuickFIX's acceptance `.def` files write every timestamp as a placeholder —
`52=00000000-00:00:00` — because the harness compares messages **by shape**: a field is
"present with the right type", never "a valid instant". Turning the corpus around (the mirrored
run: expected outputs become this end's inputs) means a real parser reads those bytes, so the
mirror harness's `make_receivable` substitutes a real time for the placeholder. It did so for
`52=SendingTime` only.

FIX 4.4 has more than one `UTCTimestamp` on the paths the corpus exercises: `122=OrigSendingTime`
travels with every `43=Y` PossDup, and `60=TransactTime` is required on `NewOrderSingle` and
`ExecutionReport`. The session parses both as timestamps. So three `Reachable` mirrored files —
`6_SendTestRequest`, `8_AdminAndApplicationMessages`, `8_OnlyApplicationMessages` — went red on
`Reject 45=2 371=122 373=1` and `371=60 373=6`: the engine refusing, correctly, a timestamp the
**harness** had left as zeros. The session's outbound side was probed twice (`send_application`
→ 14 fields, `34=2` then `34=3`) and was right.

**A comparator that matches by shape never had to make a placeholder real, so nothing in the
corpus says which tags carry one.** The list of tags that need substituting is a property of
the *reader* — every `UTCTimestamp` the dictionary knows on a message the file sends — not of
the file.

## The rule

- When a corpus is replayed into a real parser, enumerate the placeholder's occurrences **by
  field type from the dictionary**, not by the one tag you noticed. Anything typed
  `UTCTimestamp` (`52`, `60`, `122`, and in other messages `42`, `779`, …) carries it.
- A rejection code names the tag (`371=`); read it before suspecting the session. Here two
  codes, `373=1` (*required tag missing* — the placeholder failed to parse, so the field was
  absent to the session) and `373=6` (*incorrect data format*), pointed at the harness in one
  line each.
- A file in the *Reachable* class that is red is either a session defect or the harness
  ([ADR-0076](../decisions/ADR-0076-the-mirrored-ceiling-is-a-classified-count-not-an-estimate.md)
  *Consequences*); decide which by changing **nothing** in the session and reading whether the
  file goes green.

## What guards it

- `crates/conformance/src/script.rs`, `make_receivable`: three tags substituted (`52`, `60`,
  `122`); **three unit tests** in that file's `receivable` tests, one per tag, each asserting the
  placeholder is gone and a parseable timestamp is in its place. Reversal quoted in `09effd1`'s
  message: removing one substitution turns exactly its test red.
- The mirrored score test: `score == Reachable` (14 / 14 at `09effd1`); a placeholder that
  reaches the session again reads as a red `Reachable` file.
- `Adapter::at` resumes at the first `I` line's `34=` — the second half of item 92
  (`1a_ValidLogonMsgSeqNumTooHigh`), which turned out not to be the reason that file is red;
  it is in the `NeedsAScriptThatAnswers` class now.

## Sources

- FIX 4.4 tag 122 `OrigSendingTime` — <https://www.onixs.biz/fix-dictionary/4.4/tagnum_122.html>;
  tags 43 / 122 — <https://www.b2bits.com/fixopaedia/fixdic44/tag_122_OrigSendingTime_.html>
  (cited by the senior developer, 2026-09-18).
- `vendor/nanofix`-independent: the corpus is QuickFIX's `test/definitions/server/fix44/*.def`,
  fetched by `scripts/fetch-quickfix-assets.sh`, never committed.
