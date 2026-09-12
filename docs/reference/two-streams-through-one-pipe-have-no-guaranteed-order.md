# Two streams through one pipe have no guaranteed order

> `[measured 2026-09-12]` — a gate merged a tool's stdout and stderr with `2>&1` and read their
> relative order as meaning. It held on the author's desk, every time, and did not hold on CI.
>
> **`[to testing-skills]`**

## What happened

A gate needed to know which test binary a test belongs to. The build tool it drove prints two
kinds of line about that, from the same command, to two different file descriptors: the binary
being launched goes to **stderr**, and the names of the tests inside it, once it is running, go
to **stdout**. A first version of the gate ran the command with `2>&1`, merging both into one
stream, and then read the merged log top to bottom: whichever "which binary is running" line
came last was taken to own every test-name line that followed it, until the next one.

That is an assumption about **order between two file descriptors multiplexed onto one pipe**,
and nothing guarantees it. A process is free to flush stderr and stdout independently, and the
kernel is free to interleave whatever arrives at the read end in whatever order it arrives. The
gate's own author ran it dozens of times, on one desk, and the order was always the same —
because on that one machine, under that one load, it always happened to be. **Green, every time,
for a reason that had nothing to do with correctness.**

It broke on a different machine — a CI runner, with different scheduling, different buffering
behaviour, more binaries running in the window that mattered. The gate went red with a message
that named the actual defect exactly: a test's result line appeared in the log **before** any
line naming the binary it belonged to. The order the desk had always seen was not a property of
the tool. It was a property of that one desk, that one day.

## Why it is worth a file

**The desk's green was green by luck**, and there was no way to know that from the desk. Nothing
about running the gate one more time, or ten more times, on the same machine would have shown
it — the interleaving that broke the assumption needed a different scheduler, a different load,
a different number of cores under contention, none of which the desk that wrote the gate could
produce on demand. A check that depends on the *order* two independent streams arrive in is,
silently, a check that depends on the machine running it.

## The generalisation

> **Two output streams multiplexed through one pipe carry no ordering guarantee between them,**
> even when one of them "always" seems to arrive first in practice. `2>&1` is a statement about
> where bytes end up, not about when they arrive relative to a different stream's bytes. A parser
> that infers meaning from that relative order is really parsing the scheduler, not the tool.
>
> **The fix is not a better parser of the merged log — it is not merging in the first place.**
> Where the tool being driven can be asked directly, in a form that answers the actual question
> (here: "which binary owns this test name"), ask it that way and let attribution be exact by
> construction, rather than inferred from a coincidence of timing. Where only a merged log is
> available — because it is somebody else's CI job, not a tool you can re-invoke — read it for
> only what does *not* depend on order: whether a name appears at all, how many times, not which
> section it fell under. A merged log can still prove a count; it can never prove attribution.
>
> **The class is bigger than test runners.** Any tool that logs progress on one stream and
> results on another — a build system, a package manager, a supervisor multiplexing several
> workers' output — hands you the same trap the moment something downstream reads the combined
> log as if position meant ownership.

## What to do instead

- **Ask the tool for a machine-readable, unambiguous answer if one exists.** Most build tools
  can enumerate their own artifacts in a structured format (here, JSON) that names each one
  exactly, with no merge and no order to depend on. Query each artifact directly rather than
  inferring from a shared log.
- **Where only a merged log is available, assert only what survives interleaving**: presence,
  count, membership of a set — never which section of the log a line fell under.
- **A check that has only ever run on one machine has not been tested against the assumption
  that broke it.** The number of times it passed there is not evidence about a different
  scheduler.
- **When a gate is rewritten to stop depending on order, say so in its own header** — the next
  reader needs to know the guarantee changed, not just that a bug was fixed.

## Guarded by

`scripts/check-feature-gated-tests-ran.sh`, whose listing half now asks the build for its own
binaries as JSON (`--no-run --message-format=json`) rather than parsing a `Running` line, and
whose reconcile half reads a real run's log for three assertions that do not depend on the order
its two streams interleaved in. `STATUS.md` items 68 and 69. CI run `34696713544` is where this
was found — job "TLS, with the kernel it needs", on commit `7323299`.

## The same log, a third way, and the desk was green every time

`[measured 2026-09-12]` the fix above was pushed, and CI went red again on the same gate —
now reading **0 of 38** `Running` lines out of a log in which every binary had run. The cause
was neither order nor spelling: **the tool colours its output on the runner and not on a piped
desk terminal.** GitHub's runner receives

```
ESC[1mESC[92m     RunningESC[0m tests/foo.rs (target/debug/deps/foo-ff33a9423628d6e3)
```

and a matcher anchored at `^[[:space:]]*Running` cannot match a line that begins with an escape
sequence. Reproduced by colourising a real local log and running the pre-fix checker over it:
`0 Running lines`, the exact CI failure, against `38` after the fix.

**Three failures of one gate, three different causes, and one thing in common: the developer's
machine was green for all three.** A merged stream, a decorated name and a colour escape are
each a way that *the same command's output is not the same text* on two machines. The rule that
covers all three is narrower than "test on CI": **when a check parses another tool's output,
that output is an interface the tool never promised to keep — so parse the most machine-readable
form it offers, and normalise what you must parse.** In this case: take the binaries from the
tool's JSON rather than from its prose, and strip the decorations before matching anything.

And put the normalisation in the thing that *reads* the log, not in the thing that writes it.
A checker handed a log it did not produce cannot assume how it was produced — setting a
`no colour` variable where the log is generated fixes today's caller and leaves the next one
to rediscover this.

- [a-test-s-name-is-spelled-differently-in-a-listing-and-in-a-run](a-test-s-name-is-spelled-differently-in-a-listing-and-in-a-run.md)
  — found while fixing this one, in the same gate: a second, independent way the same log could
  be misread, this time about spelling rather than order.
- [reading-the-output-you-grepped-for](reading-the-output-you-grepped-for.md) — the same family
  one step removed: there, a filter over a stream's output became the result read instead of the
  thing the stream actually said. Here, the *order* of two streams became the result instead of
  what either stream actually said.
- [a-matcher-excluded-the-separator-every-real-name-uses](a-matcher-excluded-the-separator-every-real-name-uses.md),
  [a-reversal-that-removed-the-guard-s-label-not-the-guard](a-reversal-that-removed-the-guard-s-label-not-the-guard.md)
  and [a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for](a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for.md)
  — three more gates from the same week that read as green while measuring nothing, each for a
  different reason. None of them is a one-off.
