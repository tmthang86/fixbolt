# A test's name is spelled differently in a listing and in a run

> `[measured 2026-09-12]` — a gate comparing a build's own test *listing* against a real *run*'s
> log read 325 of 326 on a log where all 326 had actually run. Found while fixing an unrelated
> defect in the same gate.
>
> **`[to testing-skills]`**

## What happened

A test framework (here, Rust's built-in libtest, but the shape is not specific to it) offers two
ways to ask what happened: **list** the tests a binary contains, and **run** them and report the
outcome of each. A gate that wants to prove "everything the build says exists actually ran" has
to take a name from the first and find it in the second. That only works if the same test comes
out spelled the same way in both places.

It does not, for every test. A test marked to run-and-expect-a-panic prints one line when listed
and a different line when run:

```
listing:  NAME: test
running:  test NAME - should panic ... ok
```

The listing gives the bare name. The run report inserts `- should panic` between the name and the
outcome, because that is useful information for a human reading a live test run. A gate that
extracts "the test name" from a run line with a pattern built against the *common* case —
`test NAME ... ok`, with nothing between name and outcome — simply does not match that line. The
test ran, passed, and the gate's parser silently failed to find it in the log, because the two
tools spell the same event differently and the gate's model of "the log format" was built from
the ordinary case.

The same thing happens with `#[ignore = "reason"]`: a listing says `NAME: test`, a run says
`test NAME ... ignored, reason` — an extra clause again, this time carrying free text that can
itself contain punctuation the parser was not expecting.

## How it was found

Not by design — by accident, while fixing a different bug in the same gate (a false ordering
assumption between two merged output streams, written up separately). Re-running the fixed gate
against a **real** log, rather than a synthetic one built by hand from a listing, produced:

```
325 accounted for, 1 not
```

on a log where the sixth commit's CI run had genuinely run all 326. The one missing name was
`a_socket_that_is_held_open_and_silent_is_not_read_as_a_refusal`, which happens to carry
`#[should_panic]` — and predates the branch that finally noticed, by some distance. **A red that
said nothing about the thing the gate exists to prove.**

## Why it is worth a file

The failure mode is not "the parser has a bug" in the ordinary sense — the parser was correct for
every test it had been tried against. It was wrong about the **format**, because the format is
not one format: it is a family of related-but-different lines, and the family only becomes
visible once a run happens to contain a member the author had not written a test fixture for.
**A synthetic log built from a listing plus an assumed run-line shape will never contain the
member that breaks the assumption**, because the synthetic log was built from the same mental
model that has the gap. Only a real run — of real tests, including whichever oddly-annotated ones
already exist in the suite — surfaces it.

## The generalisation

> **A tool that reports the same underlying fact in two different commands is not guaranteed to
> spell it the same way in both.** A "list" mode and a "run" mode serve different readers — one
> is a catalogue, the other a narration of what happened — and a narration earns the right to say
> more: *why* it's running, *why* it didn't finish, what the outcome means. That extra text is
> exactly the surface a naive extractor does not expect, because the naive case was built from
> the plain, unannotated instance.
>
> **A gate that reconciles two outputs of the same tool owns the translation between their
> formats**, and that translation has to be tested against every variant the tool itself defines,
> not only the common one. For a test framework that typically means: a plain passing test, a
> failing test, an ignored test (and every reason-string shape it allows), and any
> should-panic/should-fail variant. Enumerate the tool's own documented output shapes rather than
> inferring them from whichever tests happen to be in the suite today — the suite that has none
> of the annotated kinds yet will grow one eventually, and it should not be the day the gate is
> trusted to catch it.
>
> **Test the reconciliation against a real run, not a run synthesised from the listing.** A
> synthetic log is built by the same assumptions the parser holds, so it can only ever confirm
> what the author already believed. A real log was produced by the tool being reconciled against,
> independent of any assumption about its shape.

## What to do instead

- **Enumerate the format family before writing the parser**, from the tool's own documentation
  or source, not from the test suite in front of you today.
- **Test the reconciliation against a captured real run**, kept as a fixture, that deliberately
  includes one of every annotated kind the framework supports.
- **When a count comes up short, read which name is missing before assuming the count is the
  finding.** The missing name is usually more informative than the number — it names the format
  variant nobody had written a case for.
- **Prefer machine-readable output over prose lines wherever the tool offers it.** A line meant
  for a human to read is free to grow a clause; a structured field is not.

## Guarded by

`scripts/check-feature-gated-tests-ran.sh`'s reconcile half, R1, which counts run-status
occurrences (`ok`, `FAILED`, `ignored`) for a listed name rather than pattern-matching a fixed
line shape, so an inserted clause between the name and the outcome no longer breaks the count.
`STATUS.md` item 68.

- [two-streams-through-one-pipe-have-no-guaranteed-order](two-streams-through-one-pipe-have-no-guaranteed-order.md)
  — the defect this one was found while fixing, in the same gate, in the same log.
- [reading-the-output-you-grepped-for](reading-the-output-you-grepped-for.md) — the same family:
  a piece of output was read through an assumption about its shape rather than the shape it
  actually has.
- [a-matcher-excluded-the-separator-every-real-name-uses](a-matcher-excluded-the-separator-every-real-name-uses.md),
  [a-reversal-that-removed-the-guard-s-label-not-the-guard](a-reversal-that-removed-the-guard-s-label-not-the-guard.md)
  and [a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for](a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for.md)
  — three more gates from the same week that read as green while measuring nothing, each for a
  different reason. None of them is a one-off.
