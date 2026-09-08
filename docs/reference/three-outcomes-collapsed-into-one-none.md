# Three endings, one word, and the sentence that named the wrong cause

`[measured 2026-09-08]` · Found while surveying another engine's harness ·
**`[to testing-skills]`**

## The shape

An end-to-end harness reads a framed message off a socket. One line decided what
"could not read" meant:

```rust
match self.sock.read(&mut chunk) {
    Ok(0) | Err(_) => return None,
    Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
}
```

`Ok(0)` is the peer closing cleanly. `Err(WouldBlock)` on a socket carrying an
`SO_RCVTIMEO` is the read timeout expiring with the link still up. `Err(_)`
otherwise is the socket itself failing. Three different things happened; the
caller got one `None` for all three, and the run printed:

```
interop: the counterparty stopped answering
```

That sentence is true of exactly one of the three. When the counterparty
**exited**, it was wrong. When the socket **broke**, it was wrong. And the third
case — the counterparty is alive and talking but never says the thing being
waited for — did not reach that line at all: it is the read loop running out of
its bound, which was *also* `None`.

## Why no test could see it

Every scenario asserted on the message it wanted. None of them asserted on the
*absence*, because the absence was the failure path, and a failure path that
prints something plausible reads as a working failure path. The harness had been
run dozens of times, most of them green; the two or three red runs each got a
sentence that sounded like a diagnosis and was a guess.

This is the trap in its cheapest form: **the failure message is part of the test,
and nothing tests the failure message.**

## The fix, and the one thing that makes it a fix

An enum with one variant per ending, each printing its own sentence at the moment
it happens — plus a fifth for "the bound ran out", which is not a socket ending at
all and deserved separating from the four that are.

The enum alone proves nothing. What makes it a fix is a **meta-test** that stands
up a local `TcpListener` and drives the three socket endings through the real read
path:

- a server that accepts and drops the connection → must be `PeerClosed`
- a server that accepts, holds, and says nothing past the read timeout → `Timeout`
- a server that writes one whole message → `Message`

Each assertion is written as *"is not the other one"*, because the defect was
never "the code crashed" — it was two outcomes wearing one name.

**The reversal:** collapse `Ok(0)` back into the timeout branch. Exactly one test
goes red, on the assertion meant to prove it (`a clean close read as Timeout`),
and the other two stay green — which is the evidence that the three cases are
actually separated rather than jointly asserted by one over-broad check.

## What it cost to find

Nothing, and that is the uncomfortable part. It was found by **reading somebody
else's harness** and noticing they had five branches where this one had one. No
run failed, no gate went red, and no amount of staring at this repository's own
green output would have surfaced it. A test suite cannot report a distinction it
was never asked to make.

## The generalisation

> A failure path that prints a plausible sentence is indistinguishable from a
> working failure path. If a harness collapses several endings into one return
> value, its diagnostics are guesses — and the guess is most confident exactly
> when the run is red and somebody is relying on it.
>
> The check: for every "could not get it" path, name the distinct causes out
> loud, then write one test per cause that asserts *it is not the others*.
> Deliberately break the collapse and confirm only the matching test fails.
