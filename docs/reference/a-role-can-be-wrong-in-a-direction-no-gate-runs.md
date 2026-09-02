# A whole role was wrong, and every gate was green

> `[measured 2026-09-02]` — found on the **first run** of
> [scripts/interop.sh](../../scripts/interop.sh), step 2 of
> [plans/2026-09-02-the-initiator-and-its-second-opinion.md](../plans/2026-09-02-the-initiator-and-its-second-opinion.md).
> **`[to testing-skills]`**

## The shape

A protocol with two roles. One line of code is shared by both and is correct for one of them.

```rust
// inside the inbound-Logon handler, reached by acceptor and initiator alike
self.send(Which::Logon, &extra[..n], emit)?;
```

For an **acceptor**, answering an inbound `Logon` with a `Logon` is the handshake. For an
**initiator**, which sent the first one, answering starts a *second* handshake on a session
that already has one. The line had no role condition. It had never had one.

## What every gate said

| Gate | Reading | Why it could not see this |
|---|---|---|
| `cargo test -p fixbolt-session --test score` | **59 / 59** | It is the acceptor corpus. For an acceptor the line is **correct** |
| `cargo test -p fixbolt-session --test mirror` | **0 / 50**, as asserted | Mirrored, every file needs a message the state machine cannot originate, so every file failed *before* the second Logon could be reached. A gate pinned at zero cannot report a regression, and cannot report a bug either |
| `cargo test --all` | 430 passed, 0 failed | No test drove an initiator past its own Logon |
| `cargo clippy -D warnings`, `fmt` | clean | It is well-formed code |
| `benches/alloc.rs` | `logon_out 0` | It measures allocation, and the wrong message allocated nothing |

**Six green gates, 430 passing tests, and one of the two roles could not complete a handshake
with any real counterparty.**

## What found it, and how loudly

A ten-line scenario against `libquickfix` — somebody else's twenty-year-old C++, driven over a
kernel socket by [tools/interop](../../tools/interop). Its first run, before it had ever been
green:

```
interop: logon        ok    |8=FIX.4.4|9=67|35=A|34=1|49=QFACC|...
interop: news         ok    2 application messages delivered
interop: heartbeat    FAIL  unprompted 35=0, session still answering
interop: testrequest  FAIL  35=0 back with 112=INTEROP-1
interop: resend       FAIL  nothing carrying 43=Y came back
interop: gapfill      FAIL  35=2 in: false, session survived: false
interop: logout       FAIL  35=5 out, 35=5 back
interop: FAIL 2/7
```

**Five steps failed at once and not one of them was the broken one.** The counterparty took the
second `Logon`, dropped the connection **without a word** — no `Reject`, no `Logout`, nothing in
its own log — and everything after that failed because the socket was gone. The instrument that
identified the culprit was not the failure list; it was one debug line printed per inbound
message showing `next_out=3` where 2 was expected, on the message *before* the first failure.

## The lesson, stated without FIX

**A test suite covers a direction, not a system.** Where one implementation serves two roles and
shares code between them, a suite that exercises only one role gives full marks to code that is
half wrong — and the score is honest, which is what makes it dangerous. `59 / 59` was *true*.

Three properties made this survive:

1. **The shared line was correct in the direction that was tested.** Not dead code, not
   half-written code — correct code, under-conditioned.
2. **The gate for the other direction was pinned at a constant** (`assert_eq!(passed, 0)`).
   A gate asserting a known-bad score is a gate that cannot fall, and therefore cannot report.
   It looks like coverage in a list of tests. It is a placeholder.
3. **The real counterparty failed silently and non-locally.** The symptom appeared five steps
   downstream of the cause, with an empty log at the other end. A suite of independent
   assertions would have reported five unrelated bugs.

## What to do about it

- **A second implementation is worth more than a second test.** Every check here was written by
  the same hands that wrote the code, against the same reading of the spec. The one thing that
  disagreed was an implementation that had never heard of this project. Where a real
  counterparty exists — a reference implementation, another vendor's client, the production
  system being replaced — driving it is not an integration nicety; it is the only check with an
  independent opinion.
- **A gate asserting a failing score needs a note saying what it cannot detect.** `passed == 0`
  proves the harness runs. It proves nothing about the code under it, and the file should say so
  where the number is.
- **When several steps fail at once, suspect one cause upstream of all of them, and instrument
  the step before the first failure.** The failure list points at the symptom; the state at the
  last *successful* step points at the cause.
- **A protocol role is a coverage axis.** Ask, per shared code path, *which roles reach this,
  and which of them does a test drive through it?*

## The fix

```rust
// **Only the side that did not speak first answers.**
if !R::SPEAKS_FIRST {
    self.send(Which::Logon, &extra[..n], emit)?;
}
```

Regression: `an_initiator_does_not_answer_a_logon_with_a_logon` in
`crates/session/tests/initiator.rs`, which asserts on the **count of messages sent** and on the
sequence number not being spent — both observable without a socket, which is what makes it a
unit test rather than a second interop run.
