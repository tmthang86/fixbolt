# The lint was denied, the debt was annotated, and the check protected nothing

`[measured 2026-09-08]` · Rust, `clippy` · **`[to testing-skills]`**

## The plan, which was reasonable

A lint worth having fires 207 times in code that already exists. Cleaning it all
in one change is a diff nobody can review, so the standard move is a **ratchet**:

1. set the lint to `deny` for everybody,
2. give each file that already violates it an `allow`, with its count written down,
3. add a script that counts the violations anyway — with `--force-warn`, which
   overrides an `allow` — and fails if the total goes up.

New code in a clean file is stopped by (1). New code in a dirty file is stopped by
(3). That was the design, and it is a good one.

## What actually shipped

Step 2 was done by putting this at the top of each offending file:

```rust
#![allow(clippy::indexing_slicing)]
```

`#!` is an **inner** attribute. At the top of a module file it applies to that
module. At the top of `lib.rs` it applies to **the entire crate** — every
submodule, including the clean ones.

Three of the 21 offending files were `lib.rs`. So three whole crates — the
biggest three, holding most of the code — had the lint silently turned off. The
ratchet's second half still worked, because `--force-warn` overrides an allow. The
*first* half, the deny that was supposed to stop new code, was inert across most
of the codebase.

## How it was found, and how it was nearly not

The reversal was: paste a new violation into a file with no `allow` of its own and
watch the lint reject it.

```
$ # append `fn probe(v: &[u8]) -> u8 { v[0] }` to a clean module
$ cargo clippy -p <crate> --lib ; echo $?
0
```

Exit 0. No warning. The file had no `allow`; its crate root did.

The near-miss is worth recording. The **first** attempt at this reversal was run
inside a shell line containing an unquoted backtick pair in an `echo`. The shell
tried to execute the word between them, printed `deny: command not found`, and the
grep that followed counted `0`. **The number 0 was correct and meant nothing** —
it was the count from a command that had already gone wrong. It was caught only by
reading the whole output rather than the number that was being looked for.

So the sequence was: a broken reversal produced the same answer as the real
defect, and the real defect was confirmed only on the second, correctly quoted
attempt.

## The fix

Replace the crate-root `#![allow]` with attributes scoped to the items that need
them: 13 functions in one `lib.rs`, 2 in another. For generated code pulled in by
`include!`, note that **an `#[allow]` written next to the `include!` does not reach
inside it** — rustc reports it as an unused attribute and the errors stand. The
allow has to be emitted by the generator, onto the generated functions.

After that the reversal bites in all three crates:

```
crates/engine/src/clock.rs      -> 1 error from the deny
crates/session/src/schedule.rs  -> 1 error
crates/codec/src/checksum.rs    -> 1 error
```

## The generalisation

> **An opt-out written at the top of a file has a scope, and the scope is a
> property of the language, not of the line's position.** A suppression meant for
> one file that lands on a whole compilation unit produces exactly the observable
> a working ratchet produces — clean output, a stable count — while protecting
> nothing.
>
> The check is not "did I write the suppressions", it is: **introduce a fresh
> violation into a file that was supposed to be protected, and confirm the gate
> rejects it.** Do it in each unit the suppressions touch, not once. A ratchet's
> counter can be perfectly correct while its enforcing half is switched off, and
> the counter is the half everybody looks at.
