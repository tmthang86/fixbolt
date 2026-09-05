# A test that cannot fail, and the reversal that was the only thing that noticed

> `[measured 2026-09-05]` — found in the first hour of wave B, writing the test that a plan's
> published memory figures rested on.
> [ADR-0055](../decisions/ADR-0055-max-message-size-is-not-a-key-and-rx-is-the-answer.md),
> `crates/engine/tests/connection_size.rs`.

## The finding

Two documents were doing arithmetic on the size of one connection — *"quadrupling the receive
buffer is +0.57% of a session"* — and nothing asserted either input. So a test file was written
with two assertions:

1. Raising the receive buffer raises the struct **by exactly the buffer**, no padding.
2. The 2 MiB resend ring is **on the heap**, not inside the struct. Every figure above assumes
   it, and putting it back inline would multiply them by a hundred.

Both passed. Then each was reversed, as `CLAUDE.md` §10 requires.

**The first went red where it was aimed** — the buffer pinned to a fixed size, `left: 0, right:
12288`.

**The second did not go red. It stopped the crate compiling:**

```text
error[E0080]: evaluation panicked: assertion failed: core::mem::size_of::<Store>() <= 64
```

The crate under test already carried, forty lines above the field being reversed:

```rust
/// **Going back to an inline `[Slot<LEN>; N]` is a compile error, not a test.**
///
/// A test can be deleted, skipped, or quietly pass on a machine with a large
/// stack. This cannot: the struct is a fat pointer and two options, and an
/// inline 2 MiB array does not fit in 64 bytes.
const _: () = assert!(core::mem::size_of::<Store>() <= 64);
```

**The test could not fail while the crate it tested existed**, because no build in which it
would fail is a build. It was not a weak guard. It was no guard, and it had been green from the
moment it was written — which is exactly how it would have read to everybody afterwards.

The comment on the `const` assertion had even argued, in advance, against writing the test that
was written. It was forty lines from the code being read and nobody was reading it for this.

## Why it is worth a page

Deleting a redundant test is a small thing. What it exposes is not.

**A green test is evidence of nothing until you know it can be red**, and *unfalsifiable* does
not look any different from *passing* in a test report. This one had every mark of a good test:
a real invariant, a documented reason, a failure message that named the consequence, a
non-trivial subject. It sat in the same file as a test that genuinely worked. The only thing
separating the two was that one of them had been reversed.

And the mechanism generalises past `const` assertions. A test cannot fail whenever something
**earlier in the chain** already refuses the condition it checks:

| The guard that fires first | The test that can therefore never be red |
|---|---|
| A `const` assertion, a static assertion, a type-level bound | Any runtime check of the same property |
| The type system — a value that cannot be constructed wrong | A test asserting it is not wrong |
| A schema, a migration, or a `NOT NULL` at the database | A test asserting the field is present |
| A validator or a parser at the boundary | A test asserting the parsed value is well-formed |
| A linter or a compiler warning promoted to an error in CI | A test asserting the code shape it forbids is absent |

In every row the redundant test is *correct*, *readable*, and *worthless*, and in every row it
will be read as coverage of the property — including by the person who later weakens the real
guard and sees the suite stay green for a completely different reason.

## The rule

**`[to testing-skills]`**

**Reverse every guard, and treat "the reversal did not compile" as a distinct outcome from "the
reversal went red".** Only the second one tells you the test works. The first tells you
something *else* already enforced the property — which is usually good news about the system and
always bad news about the test.

Three things this repository now does:

- **A reversal has three outcomes, not two**: red at the assertion you aimed at (the test
  works); green (the test is blind); **does not build** (something stronger got there first, and
  the test is redundant). The third was not in the vocabulary before this.
- **When the third happens, delete the test and record why in the file that held it** — the
  finding is more useful than the test was, and the next person to have the same good idea will
  read it.
- **Prefer the guard that cannot be deleted.** A `const` assertion beats a test for a
  compile-time property, for the reason its own comment gives: a test can be skipped, deleted,
  or quietly pass on a friendlier machine. Where such a guard already exists, the test's only
  contribution is to look like a second one.

The general shape is the one this repository keeps arriving at from different directions: **a
check proves nothing until something reads it, and a check that cannot fail is never read at
all.** It is the same family as [a reversal that fails by
hanging](a-reversal-can-fail-by-hanging.md) — both are reversals that produce a *non-answer*
which is easy to file as an answer.
