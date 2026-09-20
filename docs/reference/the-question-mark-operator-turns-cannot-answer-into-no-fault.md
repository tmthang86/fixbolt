# A `?` on `None` inside an `Option`-returning function can mean "no fault" by accident

`[measured 2026-09-19, closed 2026-09-20]` · Found while building
[ADR-0085](../decisions/ADR-0085-a-member-waits-for-a-counter-that-came-before-it-and-the-array-is-only-a-cache.md)'s
B4c fixture, fixed under
[ADR-0086](../decisions/ADR-0086-a-group-count-is-asked-at-every-depth-admin-is-two-questions-with-two-names-and-xmlnonfix-is-not-asked-the-appl-ver-id-rule.md)
decision 1 · **`[to testing-skills]`**

## The shape

A validation pass over a message returns `Option<(SessionText, …)>`: `Some` names a fault,
`None` means *this message is clean*. `bad_group_count` (`crates/session/src/lib.rs`) walked
every field and, for each repeating-group counter it met, asked the codec to open that group:

```rust
let group = view.group::<D>(msg_type, counter)?;
```

[`MessageView::group`](../../crates/codec/src/group.rs) is a **top-level** lookup, by its own
rustdoc: *"Group regions are stepped over while searching, so asking a TradeCaptureReport for
`NoAllocs(78)` — which exists only inside `NoSides(552)` — gives `None`."* It answers `None` for
a counter nested inside another group, because nested is not where it looked — even though the
flat `(msg_type, counter)` table `D::group_delimiter` reads from hands the outer loop a nested
counter exactly as it hands a top-level one, so the loop walks straight into that branch.

Inside `bad_group_count`, that `None` was never "I, `MessageView::group`, could not open this
one group." `?` cannot make that distinction — it can only propagate whatever `None` it is
handed as *the caller's own* `None`, and this caller's contract defines `None` as "no fault
anywhere in this message." One nested counter on the wire, and every counter behind it silently
stopped being checked. The same shape sat one line above it, on `view.field_at(i)?`.

**Measured** on `AE` (`TradeCaptureReport`): 12 of 45 sub-5000 counters are nested, so a real
message from that family got almost none of this rule (`STATUS.md`, 2026-09-19).

## Why nothing caught it for two phases

- **The correct pattern already existed two functions down, unused here.**
  `in_a_group_before` and `in_a_group`, in the same file, walk the same `view.field_at(i)` and
  write `let Some((counter, _)) = view.field_at(i) else { continue; };` — a field they cannot
  classify is a field the loop passes, not a reason to stop. Three functions share one loop
  shape; only `bad_group_count` used `?`.
- **The 59 acceptance definitions cannot see it.** None of the 59 `.def` files populates a
  nested repeating group — `14i_RepeatingGroupCountNotEqual.def` is the only one that populates
  *any* group, and it is not nested. `--test score` read `59 / 59` before the fix, after the
  fix, and — the part worth sitting with — **also with the bug deliberately put back during the
  reversal.** A green `59 / 59` was never evidence this pass worked on a nested group; it only
  ever proved the top-level case, and it kept saying so even while the bug was live again.
- **The failure has no shape on the wire.** A message that should have drawn `Reject 373=16`
  and did not is simply accepted — no exception, no log line, no observable difference from a
  message that is genuinely clean. There is nothing for a human watching the link to notice.

## The reversal, and the sentence it was checked against

Reversal: restore the `?` at the nested-group site. Predicted red, written down before running
it: `expected Reject 373=16, engine sent no reject`. Observed: the same sentence,
`16 passed 1 failed`. Restored: `17 passed`. `--test score` stayed `59 / 59` throughout — the
one number that looked like proof of correctness never moved either way.

## The rule

**A function that returns `Option` to mean "fault, or none" must never let `?` speak for it
without checking what the callee's `None` actually means.** `?` on `None` always means *this
function itself has nothing further to say — return `None` right now*. That is exactly right
when the callee's `None` really does mean "no answer exists" all the way up. It is wrong the
moment the callee's `None` means something narrower — *I personally could not evaluate this one
case* — while the caller's own `None` means something broader — *nothing at all is wrong with
the whole message*. Those are two different questions sharing one type, and `?` cannot tell
them apart; only the surrounding code can.

The check, generalised past this one function: for every `?` inside a function whose own return
type is `Option<Fault>` (or `Result<(), Fault>`) over a loop or a scan, name out loud what the
callee's `None` (or `Err`) means. Confirm it is *exactly* "propagate my own absence of a fault"
— never "I could not evaluate this one item, but the rest of the scan should continue." When it
is the latter, the shape is `let Some(x) = callee() else { continue };` (or an explicit match),
never `?`. A loop that must not exit early on one bad element is the strongest local signal:
`bad_group_count`, `in_a_group_before` and `in_a_group` all loop over every field, and the two
already written with `let … else { continue; }` are exactly the two that could not afford to
lose every later iteration to one earlier `None`.

## Regression tests

Built by steps 1–2 of
[the-group-count-pass-and-is-admin](../plans/2026-09-20-the-group-count-pass-and-is-admin.md),
landed as `c8709a1` and `8e81aae`.

| Test | What it holds |
|---|---|
| `crates/session/tests/group_member_values.rs::a_counter_after_a_nested_group_is_still_checked` | a nested group in front of a lying top-level counter no longer swallows the check behind it |
| `…::a_nested_counter_that_lies_is_rejected` | the nested counter's own lie is caught, not only counters that come after it |
| `…::a_parent_counter_is_named_before_its_child` | when both parent and child are wrong, the fault names the parent — depth-first, wire order |
