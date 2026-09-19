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

Plan row B4c of
[2026-09-19-phase-2-fixt-and-sbe](../plans/2026-09-19-phase-2-fixt-and-sbe.md): a
`TradeCaptureReport` with 33 one-entry top-level groups on the SP2 table, asserting
`SeenCounters::full` after the scan and `373=5 371=447` on a stray member placed after the
33rd counter; the reversal's FAIL sentence is `expected 373=5 371=447, engine sent 373=16
371=1907`. The test names are filled in here by row B4e once B4c lands.
