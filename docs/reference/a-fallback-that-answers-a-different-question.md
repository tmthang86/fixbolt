# A fallback that answers a different question

`[measured 2026-09-19]` · Found by the PR B senior review, verified by the manager, decided in
[ADR-0085](../decisions/ADR-0085-a-member-waits-for-a-counter-that-came-before-it-and-the-array-is-only-a-cache.md)
· **`[to testing-skills]`**

## The shape

A hot path keeps a small fixed array as a cache — here `SeenCounters`, 32 group counters the
wire-order scan has already passed, so that "is this field a member of a group whose counter
came first?" costs a walk over at most 32 entries and no allocation. The array can fill. The
code for that case reached for a function that already existed, `in_a_group`, because it had
the right name and the right signature:

```rust
if self.full {
    return in_a_group::<D, N>(view, msg_type, tag);   // walks 0..view.len()
}
self.tags.iter().take(self.len)                        // walks what came before
    .any(|&counter| D::group_members(msg_type, counter).contains(&tag))
```

The fast path asks *has this field's counter gone past?* The fallback asks *is that counter
anywhere in the message?* Same name, same types, same `bool` — a different question. The
rustdoc above it said *"the answer never depends on the capacity, only its cost does"*, which
was true of the fast path and of nothing else.

## Why nothing caught it

- **The overflow was unreachable on the table every test used.** FIX 4.4 declares at most 23
  distinct counters on one message type; the array holds 32. Every FIX 4.4 test, bench and
  `.def` proved the fast path and could not have proved the fallback.
- **The table that can reach it had no test that tried.** FIXT / FIX 5.0 SP2 declares up to
  393 counters on one message type, but the 180 `.def`s populate one group per file at most,
  and the alloc bench's populated-group case carries one.
- **The two answers differ only when a second fault is present.** Both paths still ask every
  member's value eventually (the fourth pass asks a superset), so the *verdict* is identical
  and only the *reason code* moves — `373=16` or `373=1` where `373=5` was due. A test that
  checks accept/reject and not which fault is named cannot see it.
- **The comment was the only guard, and a comment holds nothing** (`CLAUDE.md` §4).

## How it was found, and how to find the next one

Put `panic!()` in the fallback and run everything: `cargo test --all`, then each feature set
the CI matrix builds. Zero hits means zero coverage, whatever the coverage tool says about the
lines around it. Then lower the capacity to 1 and run again: still zero means the fallback is
unreached even when trivially reachable, so no test *shape* exercises it, not merely no test
*size*. That is a five-minute probe and it is the one that turned a "tuning number" into a
finding.

## The rule

**A fallback is the same question asked the slow way.** When a bounded cache has an overflow
path, the overflow path must compute what the cache would have answered had it been larger —
never a different, cheaper, or already-written question that happens to fit the signature. The
proof is by construction (the fallback is the cache's definition, spelled out) and by a test
that fills the cache on purpose and asserts the answer did not move.

**The test that fills the cache is part of the cache.** A capacity without a test that exceeds
it is a branch waiting for a counterparty.

## Regression tests

Built by plan row B4c of
[2026-09-19-phase-2-fixt-and-sbe](../plans/2026-09-19-phase-2-fixt-and-sbe.md), landed as
`d7be83d`. Five tests, because the branch needed holding from three directions — that it exists,
that anything reaches it, and that the two paths now agree:

| Test | What it holds |
|---|---|
| `crates/session/tests/fixt.rs::a_stray_member_is_answered_in_wire_order_when_the_array_is_full` | a message that fills the array gets the positional answer. Reversal FAIL sentence, predicted then observed word for word: `expected 373=5 371=447, engine sent 373=16 371=1907` |
| `…::the_same_thirty_three_counters_without_the_two_faults_are_accepted` | the twin — the fixture is a legal message, so the red above is the branch and not the bytes. It stayed green through the reversal |
| `crates/session/src/lib.rs::tests::an_ae_with_thirty_three_group_counters_fills_the_array` | the precondition, **observed not inferred**. Proven live by a second reversal: at `SEEN = 64` it reads `33 distinct group counters must exhaust 64 slots; the scan recorded 34` — 34 counters against 32 slots, full by two rather than by luck |
| `…::tests::no_fix_44_message_type_reaches_the_seen_bound_and_the_fixt_table_passes_it` | the sentence that used to be prose: FIX 4.4's maximum is 23 against `SEEN = 32`, and the FIXT table reaches 393. Folded from `GROUP_KEYS` every run, so the day a dictionary crosses the bound the test says so |
| `tests/fixt.rs::a_member_of_a_user_defined_group_is_not_deferred_when_the_scan_ignores_its_counter` | the second gap, found while building the first fixture: under `ValidateUserDefinedFields=N` the scan skips a counter ≥ 5000 before recording it, so the walk must skip it too. The knob is the only variable — `373=5 371=492` with it on, `373=16 371=1907` with it off, same bytes |

Thirty-two group blocks make **thirty-three** counters: `40204`'s delimiter `40209` is itself a
`NumInGroup` with a group of its own, so one block records twice. That is the kind of detail a
fixture built from the XML rather than from the generated table would have got wrong.
