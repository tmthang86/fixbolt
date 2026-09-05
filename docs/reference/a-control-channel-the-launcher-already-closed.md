# A control channel the launcher had already closed

> `[measured 2026-09-05]` — found by the blocking `interop` CI job, within an hour of the
> comment that got it wrong. `STATUS.md` item 47,
> [ADR-0054](../decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md).

## The finding

Item 47 gave `fixbolt::serve` a `Handles`, so a caller can finally stop the engine through
`Admin::shutdown`. `tools/interop`'s acceptor role was changed to demonstrate that: instead of
being `kill`ed by `scripts/interop.sh`, it would be **asked** to stop, and the gate would assert
on the `Shutdown` that came back.

The trigger was a line on stdin. And the first version treated **end-of-input** as the signal
too, with a comment saying why:

```rust
// EOF counts: a supervisor that closes the pipe has stopped supervising,
// and an acceptor that keeps serving after that is a process nobody owns.
let _ = read_line(&mut stdin().lock(), &mut line);
admin.shutdown(2_000);
```

It passed locally. The CI job's log:

```text
interop: fixbolt acceptor on 127.0.0.1:15645, 1 counterparties
interop: stopping on ""
interop: acceptor stopped: Shutdown { sessions: 0, said_goodbye: 0, acked: 0, timed_out: 0 }
```

**The acceptor stopped before the counterparty had connected**, and the whole direction failed.
A background process on a CI runner has no terminal on stdin, so the read returned zero bytes
at once. The empty string in `stopping on ""` is the whole diagnosis.

## Why it passed locally and failed there

Not a timing difference, and not the runner being slower. **The two environments disagree about
what an unspecified stdin is.** In an interactive shell a backgrounded process inherits the
terminal and a read simply blocks; under a CI runner, `nohup`, `systemd`, `docker` without
`-i`, or anything that redirects from `/dev/null`, it is closed or empty and the read returns
immediately.

So the mechanism did not fail *sometimes*. It worked exactly once — in the environment where
the author happened to be standing.

## The fix

Only a **non-empty line** stops the engine; end-of-input leaves it serving and says so:

```text
interop: stdin ended without a stop line; still serving
```

Both halves are checked rather than one:

- `printf 'stop\n'` into the fifo → `interop: stopping on "stop"`, and the gate reads
  `interop-acceptor: shutdown ok Shutdown { … }`.
- launched with `< /dev/null` → the process is **still alive** a second later, and the log
  carries the *still serving* line.

`scripts/interop.sh` holds the write end of a fifo open (`exec 9> …`) for the same reason: a
reader whose only writer has gone sees EOF, so the fifo without the held descriptor is the same
bug wearing a different hat.

## The rule

**`[to testing-skills]`**

**A control channel that the environment can close is not a control channel — and the
environment's default is not the one you are looking at.** A harness that stops a process on
end-of-input has made a claim about every launcher that will ever start it: that closing stdin
means *"stop"* rather than *"I am not a terminal"*. That claim is false for most of them.

Two things generalise past this repository:

**Distinguish "the signal arrived" from "the channel ended".** They are different events and
only one of them is an instruction. Any harness that reads a pipe, a socket or a file for a
command has both, and the quiet failure is treating the second as the first — because it fires
**instantly and always**, so the failure it causes looks like whatever was happening at start-up
rather than like a control-channel bug.

**A comment arguing for runtime behaviour is the shape to distrust.** `CLAUDE.md` §4 says prose
does not hold a constraint, and this comment is what that rule looks like when it is violated
by a careful-sounding sentence: *"a supervisor that closes the pipe has stopped supervising"*
reads like a reason and is an assumption about somebody else's launcher. The test that would
have caught it costs three lines — start the thing with `< /dev/null` and assert it is still
running — and it now exists.
