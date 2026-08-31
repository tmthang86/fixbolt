# ADR-0019 — Two `unsafe` blocks, not one, and an error variant for what the enum does not model

> **Status:** **Accepted — 2026-08-31** · **Amends
> [ADR-0015](ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)
> decisions 4 and 10.** Everything else in ADR-0015 stands.
>
> §5 forbids editing an accepted ADR's substance, so these corrections arrive as
> their own decision rather than as a quiet rewrite — even though ADR-0015 was
> accepted the same day and nothing had been built on it yet. That is the point
> of the rule: the record should show that the count was wrong *before* any code
> existed, and that writing the code is what found it.
>
> **Accepted by standing delegation**, `[2026-08-30]`, like ADR-0015.

- **Date**: 2026-08-31
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0015](ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md),
  `CLAUDE.md` §2 non-negotiables 7 and 8,
  [plans/2026-08-30-threads-and-affinity.md](../plans/2026-08-30-threads-and-affinity.md) step 2

## Context

Step 2 of the plan implemented `crates/engine/src/affinity.rs`. Three of
ADR-0015's clauses did not survive contact with it, and two of them were wrong
in a way that reading the ADR alone would not reveal.

**Decision 10 contradicted decision 2.** It asked for *"exactly one `unsafe`
block, around the `sched_setaffinity` call"*. But decision 2 requires the result
to be read back, and reading it back is `sched_getaffinity` — a second FFI call,
which needs a second block. The one-block rule was unimplementable from the
moment the read-back rule was written next to it.

**`AffinityError::NotSupported` cannot be constructed.** ADR-0015 gave it the
meaning *"not Linux, or the feature is off"*. The module is declared
`#[cfg(all(feature = "affinity", target_os = "linux"))]`, so if either condition
is false the enum does not exist to hold the variant. It is a promise the API
cannot keep.

**The enum had no room for an unmodelled failure.** `sched_setaffinity` can fail
with an `errno` outside `EINVAL` and `EPERM`. With only modelled causes
available, such a failure would have to be reported as one of them — which is
exactly the shape of the defect this repository has already paid for:
`check-ktls-available.sh` printed a fixed `ENOENT` explanation for *every*
`OSError` and contradicted its own output for a day
([ktls-on-a-plain-socket.md](../reference/ktls-on-a-plain-socket.md)).

## Decision

**1. The rule is "no `unsafe` outside `affinity.rs`, one block per FFI call, each
naming what proves it sound" — not a global count.** Today that is **two**:
`sched_setaffinity` and `sched_getaffinity`. Each carries a `SAFETY` comment
naming the two tests that check it, in the style `crates/engine/src/poll.rs`
already uses.

The number is not the guarantee. **The guarantee is that a block cannot be added
without a comment saying what proves it**, which is what non-negotiable 8
actually asks for. A count is easy to satisfy by making one block bigger, which
is worse code and the same amount of `unsafe`.

**2. A third block was available and was refused.** `sched_getcpu()` would answer
"which core is this thread on". `/proc/thread-self/stat`'s `processor` field
answers the same question with no FFI at all — and it is a **better** answer,
because it is written by the scheduler rather than by this crate. `[measured
2026-08-31]` the field index was verified against `taskset -c 3` rather than
counted from documentation. Where a safe observation and an `unsafe` one are
equally good, the safe one wins; here it was not even equally good.

**3. `NotSupported` is removed.** An unconstructible variant in a public enum is
worse than no variant: it invites a `match` arm that can never run and suggests a
runtime condition that is really a compile-time one.

**4. `AffinityError` gains `Failed(i32)` and `Unreadable(&'static str)`.**
`Failed` carries the raw `errno` for a syscall failure the enum does not model.
`Unreadable` carries the path for a `/proc` or `/sys` file that could not be read
or parsed — the path, so the message names the file rather than the symptom.

Both keep the property decision 4 of ADR-0015 was really about: **the error says
what to go and look at.** Neither is a hot-path error and neither needs to be
fieldless.

**5. The enum is `#[non_exhaustive]`.** Steps 3 to 5 will add variants — the
topology rejections are not implemented yet — and a caller's `match` should not
break when they do.

## Consequences

**Good**

- The `unsafe` rule is now one a reviewer can apply to a diff: every block has a
  comment naming a test. The previous rule could only be applied by counting, and
  counting rewards merging blocks.
- An unexpected `errno` is reported as an unexpected `errno`. It will read as
  "the affinity syscall failed with errno 3" rather than as a confident and wrong
  explanation.
- One planned `unsafe` block was removed entirely by using a file the kernel
  writes, which is also the stronger observation.

**Bad, and named**

- **Two ADRs for one module before a line of it shipped.** That is process cost
  paid for a decision the size of a paragraph, and it is only worth it because
  §5's rule is what makes the older ADRs trustworthy.
- **`Failed(i32)` is a hole a lazy implementation can pour anything into.** The
  only defence is review: a new failure mode that turns out to be common belongs
  in its own variant, and `Failed` is not the place to leave it.
- **`#[non_exhaustive]` means callers cannot write a total `match`**, which is a
  real ergonomic cost, taken because the remaining steps will certainly add
  variants.

## Alternatives considered

**Keep "exactly one block" by fusing both syscalls into one `unsafe` block.**
Literally compliant and worse: the block gets bigger, the read-back would run even
when the set failed, and the `SAFETY` comment would have to argue two different
things at once.

**Keep `NotSupported` and construct it never.** Rejected in decision 3. Dead API
is API.

**Map an unknown `errno` onto `NoSuchCore`.** It is the most likely cause, and
that is exactly why it is the dangerous default: a plausible wrong answer is
harder to catch than an unhelpful right one.
