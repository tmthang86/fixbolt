# A green fraction over a scenario that never ran

> `[measured 2026-09-04]` — two false greens in the same binary, both found in step 1 of
> [plans/2026-09-03-acceptor-interop.md](../plans/2026-09-03-acceptor-interop.md),
> the moment a second counterparty was pointed at a gate that had only ever had one.
> **`[to testing-skills]`**

## Shape 1 — the scoreboard counted the steps that ran, not the steps that exist

`tools/interop` scores seven steps and prints `interop: PASS n/n`. Step 2 read the
counterparty's unprompted messages, and it read them with `?`:

```rust
w.read_until(6, |m| m.contains("|35=B|"))?;   // returns from `run` on None
```

A counterparty that sent no News ended `run` there. The five steps after it never executed, and
the scoreboard — which counts what was *pushed onto it* — printed:

```
interop: logon        ok    |8=FIX.4.4|9=67|35=A|34=1|49=FIXACC|…|
interop: the counterparty stopped answering
interop: PASS 1/1
```

**`PASS 1/1`.** One of one. A perfect score over a scenario in which one step of seven ran.

`scripts/interop.sh` was immune, because it greps for each of the seven step names *and* the
literal `PASS 7/7` — the defence written after an earlier false green in the same file. A human
reading the binary's output was not immune, and neither would a script that trusted the
fraction have been. **A denominator computed from what happened cannot report what did not.**

The fix is one character: drop the `?`, let the step record `FAIL`, let the rest run. A step
that cannot get its input is a failed step, not a reason to stop counting.

## Shape 2 — the expectation was named in one place and read from another

The same binary took `--target` on the command line, built its session from it, and then judged
the reply against a literal:

```rust
let target = arg(args, "--target").unwrap_or_else(|| "QFACC".to_owned());
…
reply.contains("|49=QFACC|")     // not `target`
```

For as long as there was exactly one counterparty, `--target` was always `QFACC` and the two
agreed. The first run against a second counterparty produced this:

```
interop: logon FAIL |8=FIX.4.4|9=67|35=A|34=1|49=FIXACC|52=…|56=FIXINI|98=0|108=30|10=114|
```

A **correct** Logon, from the right counterparty, reported as a protocol failure. The failure
message even prints the value that disagrees, which is the only reason it took a minute rather
than an hour.

Note which direction this one fails in. A hard-coded expectation that happens to match is a
**false green** for as long as it matches; it turns into a false red only when someone changes
the input. This one had been green for two weeks and was never right — it was *coincident*.

## The generalisable rule

**A parameterised test must derive every expectation from the parameter.** If the test takes
`x` and asserts against a literal that happens to equal `x`, it is not parameterised; it is a
fixed test with a decorative argument, and it will report the parameter's first real change as
a defect in the system under test.

Cheap way to find these before they cost anything: **run the test with a different value of the
parameter, once.** Not a new fixture, not a matrix — one alternate value. Every assertion that
does not move with it is a literal in disguise.
