# Reading the output you grepped for is not reading the output **`[to testing-skills]`**

`[measured 2026-09-06]` A shell error sat in the middle of a green CI job for a whole run, on a
gate this repository had just written, run four times locally by the person who wrote it. It was
found by a rule adopted for an unrelated reason.

---

## What happened

`scripts/interop.sh` gained a scenario for `789=NextExpectedMsgSeqNum`. It writes a QuickFIX
configuration with a heredoc:

```sh
cat > "${WORK}/acceptor-789.cfg" <<CFG
...
SocketAcceptPort=${PORT}
# The C++ spelling. Java's `EnableNextExpectedMsgSeqNum` would be ignored here
SendNextExpectedMsgSeqNum=Y
CFG
```

**The delimiter is unquoted** — deliberately, because the body needs `${PORT}`, `${SRC}` and
`${WORK}`. An unquoted heredoc expands its body, and expansion includes **command
substitution**, and a comment is body like any other line. So the shell tried to run the word
between the backticks:

```text
scripts/interop.sh: line 778: EnableNextExpectedMsgSeqNum: command not found
```

Then it carried on. The scenario read `PASS 9/9`, the script exited `0`, and the CI job was
green.

**`set -euo pipefail` was in force and did not see it.** A failed command substitution inside a
heredoc is not a failed command; the assignment simply gets an empty string where the
substitution was. Here that emptied part of a *comment*, so nothing downstream noticed.

## Why four local runs did not find it

The script was run four times on the machine where it was written. Every time, its output was
read like this:

```sh
./scripts/interop.sh 2>&1 | tail -4
./scripts/interop.sh 2>&1 | grep -E 'interop-next-expected|^interop: '
```

Both filters were built from **what the run was expected to say**. `tail -4` shows the summary.
The `grep` shows the lines the new scenario prints. Neither can show a line nobody predicted,
and an unexpected line is the only kind worth looking for.

`CLAUDE.md` §7 already says *"read the output, not the exit status"*, and that rule was followed
— the exit status was never trusted. **The gap is one level further in: the output was filtered
before it was read.** A filter is a prediction, and a prediction cannot surface a surprise.

## What actually found it

`CLAUDE.md` §9's last box: *a green CI run is named, by id, for the commit being closed* — and
this repository's practice of reading the interop job's log **line by line rather than off its
conclusion**, adopted after an earlier incident about something else entirely.

Reading a whole log, top to bottom, with no filter, is what surfaced it. The rule was not written
for this and caught it anyway, which is the argument for rules of that shape.

## The fix, and why the obvious one is not enough

Removing the backticks fixes this instance. It does not stop the next one — a comment is exactly
where nobody looks for shell metacharacters, and quoting the delimiter is not available because
the body needs its variables.

**So the generation is checked instead of trusted**: stderr is captured and asserted empty.

```sh
cat > "${WORK}/acceptor-789.cfg" 2> "${WORK}/acceptor-789.err" <<CFG
...
CFG
if [[ -s "${WORK}/acceptor-789.err" ]]; then
  echo "writing the config produced errors:" >&2
  cat "${WORK}/acceptor-789.err" >&2
  exit 1
fi
```

**Proven by reversal, not by reading.** Putting the two backticks back makes the run exit `1`
with the error reported as a failure instead of printed into a passing log. A second, weaker
assertion — `grep -qx 'SendNextExpectedMsgSeqNum=Y'` on the generated file — guards the case
where a substitution mangles the key itself; **its own first reversal was a no-op**, because the
mangling chosen (`` `echo Y` ``) produced the correct value. A reversal has to break the thing
the assertion is about, and choosing the break is where the thinking is.

---

## The shape, for anyone who has never seen a FIX engine

**A generator that writes a file the system under test will read is itself under test, and it
usually is not.** The scenario asserted a great deal about what came back over the wire and
nothing at all about whether its own fixture had been produced correctly. Between "the script
ran" and "the file says what I typed" there is a whole language's worth of expansion.

Three transferable rules:

1. **At least once, read the entire log with no filter.** Not the tail, not a grep — the whole
   thing. Do it when the gate is new, and again when it changes. A filter shows you your
   expectations; the failures worth finding are the ones you did not expect.
2. **A file your test generates deserves an assertion.** Cheap ones: it is non-empty, it contains
   the line that is the point of it, writing it produced no stderr.
3. **"The exit status was zero" and "the log was clean" are different claims.** A tool can be
   loudly wrong and still succeed, and the louder it is the more likely the noise is scrolled
   past.

And one about reversals, which cost nothing to learn here: **a reversal must break the thing the
assertion is about.** `` `echo Y` `` is a command substitution and it did prove the heredoc
expands — but it expands *to the right answer*, so the assertion stayed green and proved
nothing. Picking the break is the part that needs care; running it is the easy half.

---

## `[measured 2026-09-08]` The same shape again, and this time the filter did not hide the failure — it manufactured a success

Two days later, in a session whose whole job was reading `STATUS.md`, a docs-only change was
checked against `CLAUDE.md` §7's *every step, every commit* rule:

```sh
cargo test --all 2>&1 | tail -60
```

The tool harness reported **`exit code 0`**. The suite had not run at all. The last sixty lines
said so plainly:

```text
error: failed to run custom build command for `fixbolt-dict v0.0.0`
  fixbolt-dict: FIX 4.4 dictionary not found at ../../vendor/quickfix/spec/FIX44.xml
    run scripts/fetch-quickfix-assets.sh
warning: build failed, waiting for other jobs to finish...

[exited with code 0]
```

`vendor/` is gitignored by ADR-0001 and a fresh clone does not have it, so `fixbolt-dict`'s
`build.rs` refused — correctly, with a good message. **Nothing was wrong with the gate.** What
was wrong is that the status reported for it belonged to somebody else.

### Why the zero

A pipeline's exit status is **its last command's**. `tail` read its input, printed it, and
succeeded. `cargo`'s failure was never consulted. `set -o pipefail` fixes this and is in force
in every `scripts/*.sh` here — it was not in force in an ad-hoc one-liner typed at a shell,
which is exactly where nobody thinks to set it.

So this is one turn worse than the 2026-09-06 case above. There, the filter could not *show* the
surprise but the exit status was still honest and simply not trusted. Here the filter **became**
the exit status. A green that was inferred from the wrong process is not a weaker green; it is
not a green at all.

### What found it

Rule 1 of the previous section, applied for its own sake: the output was read rather than the
status. `tail -60` happened to be wide enough to contain the error — **which is luck, and worth
naming as luck.** A `tail -3`, or a `grep 'test result:'` built from what a passing run says,
would have shown nothing at all and left `exit code 0` as the only signal. The filter that
hid the failure and the filter that revealed it differed by a number chosen without thought.

### The fix

Never let a filter stand between a command and its status. Two forms, both cheap:

```sh
cargo test --all > /tmp/test.log 2>&1; echo "CARGO_EXIT=$?"   # status is cargo's
set -o pipefail                                                # or make the pipe honest
```

The first is preferred for anything slow: the full log survives on disk for the unfiltered read
rule 1 asks for, instead of being consumed by the filter that summarised it.

### A fourth transferable rule, for the list above

4. **A filter in a pipeline replaces the exit status as well as the output.** Any tool, harness
   or CI step that reports "the command succeeded" is reporting on the *last* process in the
   pipe. Redirect to a file and read the status of the command you actually care about — or
   turn on `pipefail` and know that you did. The cost of getting this wrong is not a missed
   failure; it is a **recorded success** that later work is allowed to build on.

And one observation about environments, since this repository now runs on more than one machine:
**a gate can be green on a desk and impossible in a container**, for reasons that are correct on
both. `vendor/` is absent by design; the fetch script exists for exactly this and takes one
command. The failure mode to guard is not "the container is broken" — it is concluding *the
suite passes here* from a run that never compiled.
