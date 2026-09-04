# Introduction to FIX and fixbolt

This page explains the FIX 4.4 protocol in plain terms: the vocabulary, why the acceptor role
is the hard one, what already exists in Rust, and where `fixbolt` fits. Read it before
[GETTING-STARTED.md](GETTING-STARTED.md) if FIX is new to you.

---

## 1. What FIX 4.4 is

The Financial Information eXchange (FIX) protocol is the messaging standard for electronic
trading. FIX 4.4 uses a **tag=value** text format. Each field is a number, an equals sign, and
a value; fields are separated by the byte `0x01`, written `<SOH>` or `|` in documentation.

A FIX message has three parts:

| Part | Fields | Example |
|---|---|---|
| Header | version `8=`, body length `9=`, message type `35=`, sequence number `34=`, sender `49=`, target `56=`, sending time `52=` | `8=FIX.4.4\|9=112\|35=D\|34=2\|49=TW44\|56=ISLD\|52=20260903-12:00:01.000` |
| Body | fields specific to the message type | `55=IBM\|44=42.00\|38=100` |
| Trailer | checksum `10=`, three digits, sum of all bytes modulo 256 | `10=234` |

FIX separates the **session protocol** (keeping the link alive, numbering messages,
recovering lost ones) from the **application protocol** (orders, executions, market data).
An engine such as `fixbolt` implements the session protocol and hands application messages to
your code.

---

## 2. Vocabulary

### Connection and session

A **connection** is one TCP socket. Sockets drop, time out and reset all the time.

A **session** is the logical stream of messages between two named counterparties, identified
by `(BeginString, SenderCompID, TargetCompID)`. **A session outlives any one connection.** When
a socket drops, the session continues on the next socket, and sequence numbers are how the two
sides work out what was lost in between.

### Sequence numbers (`MsgSeqNum`, tag 34)

Every message in a session carries a strictly increasing integer, starting at 1. Each side
keeps two numbers:

- the number it expects on the **next inbound** message;
- the number it will stamp on the **next outbound** message.

If an inbound number is higher than expected, messages were lost and recovery starts. If it is
lower than expected, something is seriously wrong and the connection is usually dropped.

### The administrative messages

These keep the session alive. The engine handles all of them; your code never sees them.

| Message | `35=` | Purpose |
|---|---|---|
| Logon | `A` | Opens a session, names the counterparties, agrees the heartbeat interval. `141=Y` asks both sides to restart at sequence number 1 |
| Heartbeat | `0` | Sent when nothing else has been sent for `HeartBtInt` seconds |
| TestRequest | `1` | "Are you there?" The reply is a Heartbeat echoing the request's `112=` id. No reply means the connection is dropped |
| ResendRequest | `2` | Asks the peer to send messages `7=` through `16=` again |
| Reject | `3` | A message was malformed at the session level; `373=` says why |
| SequenceReset | `4` | Either a gap fill (`123=Y`: "nothing to resend in this range") or a hard reset of the counter |
| Logout | `5` | Orderly end of the session |

Two flags matter for recovery. `43=Y` (PossDupFlag) marks a message that is being sent again
and may already have been processed. `97=Y` (PossResend) marks a message the sender is not sure
it sent before.

### Application messages

Everything else: `35=D` NewOrderSingle, `35=8` ExecutionReport, and so on. These reach your
handler.

---

## 3. Why the acceptor role is hard

An **initiator** dials out and starts the session; typically a trading firm connecting to a
venue. An **acceptor** listens and serves whoever connects; typically the venue, a broker, or
an execution gateway.

The acceptor is harder to build and to operate for four reasons.

1. **It does not know who is connecting until the Logon arrives.** Between `accept` and the
   first message the socket belongs to nobody. An acceptor has to hold such sockets without
   allocating memory for them, drop them if they stay silent, and refuse to let a flood of
   them starve real counterparties.
2. **It serves many sessions at once.** If they share a thread, a stall in one counterparty's
   handler delays heartbeats for all the others and causes false disconnects.
3. **It must replay history under load.** When a counterparty reconnects with a gap, the
   acceptor has to fetch old application messages from its journal and resend them in order
   with `43=Y`, while new traffic keeps arriving.
4. **Counterparties are strict.** Exact field ordering, correct checksums, the right `373=`
   reject reason, and timestamps within a tolerance are all checked. One wrong field can make
   the other engine drop the session.

---

## 4. Prior art in Rust

- **`quickfix` (C++ binding).** Around 345 000 downloads, roughly 3.4× all pure-Rust FIX
  engines combined. It won because it is the only readily available crate that runs a
  complete acceptor, despite needing a C++ compiler and CMake.
- **`ferrumfix` (`fefix`).** A pure-Rust engine with high visibility, unmaintained since 2021
  and marked unstable by its own authors.
- **`hotfix`.** A well-structured modern implementation, initiator only.
- **`forgefix`.** Notable for precise protocol terminology and documentation.

The full survey, with licences and trade-offs, is in [reference/prior-art.md](reference/prior-art.md).

---

## 5. How fixbolt is positioned

`fixbolt` is an **acceptor-first**, pure-Rust FIX 4.4 engine built for the lowest latency
achievable on ordinary kernel TCP.

- **The session layer is pure.** The state machine does no heap allocation, no system calls,
  no locking and no socket operations. Time enters as a tick. This is what lets the 59
  QuickFIX acceptance definitions run as unit tests.
- **A pre-session stage guards the acceptor.** Sockets that have not yet logged on are held
  under an explicit count and deadline, apart from live sessions.
- **Two modes.** `standard` (the default) blocks on the OS poller when idle, so it is fine on a
  shared host or in a container. `hft` (opt-in, Linux only) pins one thread per session to an
  isolated core and never sleeps in the kernel on the hot path.

Next: [GETTING-STARTED.md](GETTING-STARTED.md) runs an acceptor in three steps.
