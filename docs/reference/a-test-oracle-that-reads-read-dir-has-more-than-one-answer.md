# A test oracle that reads `read_dir` has more than one answer

`[measured 2026-09-19]` against QuickFIX at pin `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`.

## The shape

`crates/dict/tests/fixt_order.rs` reads QuickFIX's generated SP2 headers and pins four numbers,
the way the FIX 4.4 sibling pins 730. It collected them like this:

```rust
let entries = std::fs::read_dir(&dir)...          // order: unspecified, filesystem's choice
for e in entries { ...
    out.entry((mt.clone(), counter)).or_insert((delim, order));   // first writer wins
}
```

It passed on a developer box — eight runs, and three `cargo clean -p fixbolt-dict` rebuilds, all
identical. It failed in CI, also deterministically:

```
fixt_order.rs:235   left: 64460   right: 64750
```

Same pin, same 160 header files, same `Cargo.lock`, same pinned toolchain, no `NANOFIX_*`
override. **Stable on each machine, different between them.**

## Why there were four answers, not two

`src/C++/fix50sp2/` holds **160 headers** for the **156 messages** `FIX50SP2.xml` declares.
QuickFIX renamed two messages and never deleted the headers generated from the older XML:

| MsgType | Current header | Stale header still present |
|---|---|---|
| `BN` | `ExecutionAck.h`, 322 group specs | `ExecutionAcknowledgement.h`, 14 |
| `b` | `MassQuoteAck.h`, 325 group specs | `MassQuoteAcknowledgement.h`, 18 |

Only `ExecutionAck` and `MassQuoteAck` are messages in the XML this crate's table is built from.

So **32** `(msg_type, counter)` keys are declared twice, **21** of them with a different
`message_order` — and **0** with a different delimiter, which is precisely why the delimiter
assertion beside it never caught any of this.

Two stale files are **two independent coin flips**, so the old code had **four** answers at one
pin, all reproducible on one machine by forcing the pairing:

| Execution pair | MassQuote pair | nested_counter | dropped_extras | occurrences |
|---|---|---|---|---|
| current | current | 226 | 941 | **64 094** ← the true one |
| stale | current | 230 | 1 231 | 64 384 |
| current | stale | **231** | **1 307** | **64 460** ← what CI read |
| stale | stale | **231** | **1 307** | **64 750** ← what the dev box read |

## The trap inside the trap

The two machines landed on **different** faces that happen to share the first two numbers. So the
failure presented as *"three of four pinned numbers agree, only the fourth differs"* — which reads
like a narrow, specific discrepancy and invites the conclusion that the other three are solid.

They were not. `231` and `1 307` were **also** wrong, and both were also machine-dependent. A
partial match between two machines is not evidence that the matching parts are stable; here it was
an artefact of which two of four faces came up.

## The fix, and why sorting alone is not it

Sorting the entries makes the winner *stable*. It leaves it *arbitrary* — and arbitrary in a
silly way: ASCII `'.' < 'n'` is the only reason `ExecutionAck.h` would sort before
`ExecutionAcknowledgement.h`. A number that is correct because of a punctuation byte is a number
waiting to move.

So the walk now does both:

1. `paths.sort()`, so everything else about the walk is reproducible; and
2. a real merge rule — **keep the richer `order`, and assert the other is an exact subsequence of
   it**, with the delimiters asserted equal. A stale header is an earlier generation of the same
   group, so it can differ only by fields the later revision added; asserting that relation is
   what makes discarding it safe. It holds on all 21 today, and if QuickFIX ever ships two headers
   that genuinely contradict, the assertion fires and a person reads it.

Because the choice is made from the two `order` vectors and never from arrival, **the result no
longer depends on the sort at all** — which is the property worth having, the sort being belt and
braces.

## The rule to take away

**A number pinned from a directory walk is pinned to a filesystem, not to the data.** Any oracle
that enumerates files must sort them, and — where two files can describe the same thing — must
decide between them by their *content*, with the relation it relies on asserted rather than
assumed.

And: **"it is stable across runs here" is not "it is deterministic".** Determinism has to survive
a different machine. The cheapest way to find out is to run the gate somewhere else, which is what
CI did the first time it was ever allowed to run these tests
([a-feature-gated-test-is-a-test-ci-never-runs](a-feature-gated-test-is-a-test-ci-never-runs.md)).

## What guards it

`crates/dict/tests/fixt_order.rs` and `crates/dict/tests/interop_quickfix_order.rs`, both with the
sort and the merge rule, the subsequence relation asserted rather than described.

Proven by reversal, predicted before running and observed exactly: restoring `or_insert` and
sorting **descending** rebuilds the whole stale triple — `231` nested-counter extras, `1 307`
dropped fields, `64 750` occurrences — and reddens the assertion at `left: 231 / right: 226`.
Under the merge rule, reversing the sort moves **nothing**.

FIX 4.4 was checked for the same defect and has none: 730 specs produce 730 keys with **no**
duplicate at all, so `interop_quickfix_order.rs` never had a second answer. It carries the same
fix anyway, because the code shape was the same and a gate that *could* answer differently per
machine is not one to leave in place because today's data lets it off.

## Related

- [quickfix-drops-deeply-nested-fields-from-its-own-message-order](quickfix-drops-deeply-nested-fields-from-its-own-message-order.md)
  — the page whose four pinned numbers these are; two of them were wrong until this was found.
- [a-feature-gated-test-is-a-test-ci-never-runs](a-feature-gated-test-is-a-test-ci-never-runs.md)
  — why this test had never run on a second machine.
- [a-workspace-build-and-a-per-package-build-are-different-artifacts](a-workspace-build-and-a-per-package-build-are-different-artifacts.md)
  — the other "same source, different answer" trap found in the same PR.
