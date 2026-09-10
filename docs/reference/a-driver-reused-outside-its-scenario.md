# A driver reused outside its scenario, and the FAIL that was not one

`[measured 2026-09-10]` Building `scripts/interop.sh` §4j — `ResetOnLogon` judged over a socket
against a real `libquickfix` — produced two failures of the same shape in one hour. Both were in
the **test harness**, not in the engine, and the second one nearly became a defect report against
code that was correct.

## The setup

`tools/interop/initiator.cpp` is a **fixed seven-step scenario**: logon, orders, heartbeat, test
request, resend, gap fill, logout. It was written for a session that is **new** and whose Logon
carries `141=Y`.

§4j needed a different thing: bring a session up, stop this engine, start it again on the same
journal, and log on a **second** time to see what number the resumed session uses. The obvious
move was to run the same driver twice.

## Failure 1 — six of seven steps had no meaning, and the wrapper said PASS

The second run read:

```
interop-acceptor: logon        FAIL  35=A |49=FIXBOLT| |56=QFRST| 141=MISSING
interop-acceptor: order        FAIL  35=8 for QF-ORD-1: no, for QF-ORD-2: no
interop-acceptor: heartbeat    FAIL  no unprompted 35=0 in 61 s
```

in **both** arms of the scenario — and §4j printed `interop-reset: PASS 4/4` beside them, because
its own four assertions read only the `34=` off the Logon reply and were satisfied.

That is [a-green-fraction-over-a-scenario-that-never-ran](a-green-fraction-over-a-scenario-that-never-ran.md)
arriving from a new direction. There, a fraction was green over steps that never ran. Here the
steps **ran**, reported failure, and a green fraction was printed **on top of them** by a
different scorer. The two scoreboards were not wrong individually; nothing reconciled them.

It also cost **~60 seconds per arm** — a heartbeat step waiting out a window that could not close.

**The fix was to give the driver a smaller mode**, `--logon-only`: connect, log on, print the
tape, exit. One assertion, ~1 second, and no FAIL line anywhere.

## Failure 2 — the counterparty's FAIL was about its own scenario

`141=MISSING` reads like a defect: *"this acceptor was configured `ResetOnLogon=Y` and did not put
`141=Y` on its Logon reply."* Plausible, specific, and it was one command away from becoming an
open item.

Reading the oracle's source instead settled it in the other direction. QuickFIX C++ has **two**
`generateLogon` overloads:

| Overload | Sets `141=Y` when |
|---|---|
| `generateLogon()` — the **initiator**, which speaks first (`Session.cpp:673-688`) | `shouldSendReset()`: FIX ≥ 4.1, any `ResetOn*` set, **and both counts already 1** |
| `generateLogon(const Message&)` — the **acceptor**, answering (`Session.cpp:701-710`) | `m_state.receivedReset()` — i.e. it **echoes** an incoming `141=Y` |

The acceptor never announces its own local reset. This engine does exactly the same
(`crates/session/src/lib.rs:3291`). **Two engines, same behaviour, no defect.** The FAIL line was
the C++ scenario asserting something true of the *other* scenarios, where the C++ side is
configured `ResetOnLogon=Y` and therefore does send `141=Y` for the acceptor to echo.

## The lesson, generalised

**`[to testing-skills]`** Three things, and the third is the one that had teeth.

1. **A fixed scenario driver reused for a different question reports failures that are not
   failures.** Its steps encode assumptions about the state it starts from. Reuse is cheap and the
   assumptions are invisible — they are in the step names, not in the interface. The cheap fix is a
   narrower mode on the driver, asserting only what the new question needs.

2. **A green fraction printed by one scorer over another scorer's failures is worse than either
   alone.** Two independent verdicts in one log, with nothing reconciling them, is a reader
   deciding which to believe — and the shorter, greener one wins. If an outer gate deliberately
   ignores an inner scoreboard, it has to say so where the number is printed, or stop producing
   the inner one.

3. **A counterparty's failure line is scoring the counterparty's scenario, not your system.** When
   an oracle says you are wrong, the next step is to read **why it says so in its own source** —
   not to read more of your own code, and not to reason about what a conforming implementation
   would do. Here the same behaviour the oracle was flagging is the behaviour the oracle
   implements; reasoning from the field name alone gave a confident, specific, wrong answer.
   The habit that saved it is the same one this repository already writes down for feature lists:
   *a family name on an assertion is a claim about whichever member you happened to read.*

## The tests that hold each of these

- `--logon-only` and its argument: `tools/interop/initiator.cpp::run_logon_only`, whose rustdoc
  carries the measured numbers above.
- §4j's own assertions: `scripts/interop.sh`, `interop-reset:` — the four include *the two arms
  must not agree*, which is the assertion the whole scenario exists for.
- The `141` behaviour: `docs/SESSION-BEHAVIOUR.md` §4's `ResetPolicy` entry, which now names both
  QuickFIX overloads and the line numbers.
