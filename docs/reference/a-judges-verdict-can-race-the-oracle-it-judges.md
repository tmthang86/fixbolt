# A judge's own send can race the oracle it is judging

`[measured 2026-09-23]` found while building `tools/interop-qfj/Judge.java`'s `gapfill` step,
[docs/plans/2026-09-23-p3-quickfixj-interop.md](../plans/2026-09-23-p3-quickfixj-interop.md)
rows 1–2, [ADR-0130](../decisions/ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md).

**A test harness that drives a real counterparty library and also sends its own traffic to that
same socket can race the library's own background thread.** The judge is not a passive
observer here — QuickFIX/J answers a `ResendRequest` from *its own* session-processing thread,
and nothing serialised that write against the judge's next send on its own thread.

## The mechanism

`gapfill` bumps fixbolt's expected inbound number by 3 (so a `ResendRequest` is certain), sends
a `TestRequest` to provoke it, waits for the `ResendRequest` (`35=2`) to arrive, then sends a
second `TestRequest` to prove the session survived the gap. QuickFIX/J answers the
`ResendRequest` with a `SequenceReset-GapFill` (`35=4 123=Y`) — since the range was never really
sent, there is nothing to replay — and it does so **inside the same call stack that handles the
inbound `ResendRequest`**, on QuickFIX/J's own session thread. The judge's second `TestRequest`
is sent independently, on the judge's own thread.

Nothing orders these two writes to the same TCP socket. Occasionally the second `TestRequest` —
a real, higher sequence number — reached the wire **before** the gap fill that was supposed to
precede it. fixbolt, correctly holding an out-of-order message rather than answering it, never
replied within the step's deadline, and `gapfill` read as a flake.

## The fix

Wait for evidence the gap fill was actually sent — an outbound `35=4 123=Y` line in the judge's
own `RawLog` — before sending the second `TestRequest`. QuickFIX/J's send happens inside the
same call stack that handles the `ResendRequest`, so the line is available within milliseconds;
the wait costs nothing on the passing path and turns the race into an ordering the judge itself
enforces, rather than one it hoped for:

```java
final boolean sawResend =
    cur.await(8_000, l -> l.startsWith("in ") && l.contains("|35=2|")) != null;
final boolean sawGapFillSent = !sawResend || cur.await(
    3_000, l -> l.startsWith("out ") && l.contains("|35=4|") && l.contains("|123=Y|")) != null;
```

(`tools/interop-qfj/Judge.java`, `stepGapfill`.)

## The generalisation

`[to testing-skills]` — **when a test harness is also a participant, not just an observer, the
thing under test and the harness's own writes share a wire, and a background thread inside the
library the harness drives is a second writer the harness did not budget for.** The fix is never
"add a sleep": it is to read the participant's own transcript for the specific effect the next
step depends on, and block on that line rather than on wall-clock time — the same discipline
`scripts/interop-qfj.sh`'s `wait_for_line` already applies at the process level, applied here
one layer down, inside a single process's own log.

## What guards it

`stepGapfill`'s wait for the raw `35=4 123=Y` line, exercised by every `gapfill` step of every
arm `scripts/interop-qfj.sh` runs — four arms, three consecutive runs on the desk
(`docs/CONFORMANCE.md` §10), none red. No dedicated reversal was written for the race itself
(reverting the wait reproduces a timing-dependent flake, not a deterministic FAIL, so a reversal
here would be exactly the kind of red the plan's own testing note on hanging reversals warns
against reading as proof) — the ordering is guarded by construction, not by a red/green pair.
