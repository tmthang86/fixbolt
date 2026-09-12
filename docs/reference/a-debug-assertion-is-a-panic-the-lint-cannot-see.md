# A debug assertion is a panic the lint cannot see

> `[measured 2026-09-12]` — `clippy::panic = "deny"` stops `panic!` and says nothing
> about `debug_assert!`, which expands to one. The lint matches the **spelling**,
> not the **meaning**, so the rule it enforces has a hole exactly the width of the
> other ways to write the same thing.
>
> **`[to testing-skills]`**

## What happened

`STATUS.md` item 64 asked for an obligation to be held: `Session::now_ms` starts at `0`, only
`tick` writes it, and `received` takes no time argument — so a message judged before the first
tick is judged against year zero. The item suggested the obvious guard, *"a debug assertion
that `now_ms != 0`"*.

`crates/session` is a library crate, and non-negotiable 7 says no `panic!`, `unwrap()` or
`expect()` in one. That rule is not discipline; it is `[workspace.lints.clippy]` with
`panic = "deny"`, and `scripts/check-lint-config.sh` proves by reversal that the deny is live.
So the question was whether the suggested guard was allowed. It is — and that is the finding.

## The measurement

A throwaway crate, its `rust-toolchain.toml` copied in so the pinned clippy is the one that
runs — `rustup` does not carry the pin into a directory outside the tree, which is the class
`scripts/check-scratch-fixtures.sh` exists to hold — with the same three denies the workspace
sets:

```toml
[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
```

**With a control `panic!` beside the assertion**, so that a silent run cannot be mistaken for a
lint that was never switched on:

```rust
pub fn judge(now_ms: u64) -> bool {
    debug_assert!(now_ms != 0, "a session judged before its first tick");
    panic!("control");
}
```

```
error: `panic` should not be present in production code
error: could not compile `dbgassert` (lib) due to 1 previous error
exit 101
```

**The control removed, the assertion kept:**

```rust
pub fn judge(now_ms: u64) -> bool {
    debug_assert!(now_ms != 0, "a session judged before its first tick");
    now_ms != 0
}
```

```
exit 0
```

Silent. `cargo 1.98.0 (797e8a9bc 2026-08-05)`, the workspace's pinned toolchain.

**The control is the whole reason this measurement is worth anything.** Without it, `exit 0`
has two readings — *the lint permits `debug_assert!`* and *the lint was never on* — and they
are indistinguishable. One `panic!` separates them.

## What it means

`debug_assert!` expands to `assert!` under `cfg(debug_assertions)`, and `assert!` expands to
`panic!`. The lint that guards non-negotiable 7 nevertheless reads it as nothing at all,
because the lint is written against the macro **as the author spelled it**. Three consequences,
and all three were reasons to reject the suggestion here:

1. **It is a `panic!` in a library crate**, which is the thing the rule forbids — smuggled past
   the only machine check the rule has.
2. **It vanishes in release.** The guard would hold in the test suite and be absent in the
   engine anybody actually runs, which is the opposite of where an obligation about live
   ordering needs to be held.
3. **Its failure mode is worse than the bug.** The fault is a policy error — somebody read
   before they ticked — and the assertion answers it by killing the engine thread.

**Written down because the rule looked airtight and was not.** `panic = "deny"` reads as *"this
crate cannot panic"*. What it says is *"this crate cannot contain the token `panic!`"*, and the
distance between those two sentences is every macro that expands to one: `assert!`,
`assert_eq!`, `debug_assert!`, `unreachable!`, `todo!`, indexing (`a[i]`), and integer
division. This repository has already paid for one of those — `indexing_slicing` was added to
the deny list on 2026-09-08 after a `copy_from_slice` panic that `unwrap`, `expect` and `panic`
were all blind to
([an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md](an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md)).
This is the same shape, found from the other end: not a panic that slipped through, but a
panic we were about to add *on purpose*, believing the gate would have stopped us if it were
wrong.

## What was done instead

A fieldless enum variant — `Refusal::NeverTicked`, surfaced as `DropReason::NeverTicked` — and
an early return in `judge` before anything reads the clock. A value rather than an assertion:
it survives release, it is observable by the caller through `Session::last_drop_reason`, and
its failure mode is a refused connection that names its own cause instead of a dead thread.
`docs/plans/2026-09-12-an-obligation-nothing-checks.md` §D.

## The general shape

**A lint that denies a spelling does not deny the meaning.** Before treating a deny-list as
proof that a class of fault cannot occur, enumerate the other ways to write the same thing and
measure whether the lint sees them — with a control instance in the fixture, because a silent
run and a switched-off gate look identical. The answer is usually that it sees one spelling of
several, and the rule you thought you had is the one spelling wide.
