# Introduction to FIX and fixbolt

An introduction to the FIX 4.4 protocol, its terminology, why building an
acceptor is difficult, and where `fixbolt` fits within the ecosystem.

This document focuses on core protocol concepts and vocabulary before diving into
code or APIs.

---

## 1. What is FIX 4.4?

The Financial Information eXchange (FIX) protocol is the messaging standard
for electronic trading in global financial markets. FIX 4.4 uses a tag-value
wire format where fields are separated by ASCII byte `0x01` (`<SOH>`).

A standard FIX message consists of:
- **Header:** Protocol version (`8=FIX.4.4`), message length (`9=...`), message type (`35=...`),
  sequence numbers (`34=...`), sender/target IDs (`49=...`, `56=...`), and timestamp (`52=...`).
- **Body:** Message-specific fields (e.g., Symbol `55=IBM`, Price `44=42.00`).
- **Trailer:** 3-digit modulo 256 checksum (`10=...`).

FIX deliberately separates **Session Protocol** (link integrity, sequencing, recovery)
from **Application Protocol** (orders, executions, market data).

---

## 2. Vocabulary First: Core Concepts

### Connection vs Session
* **Connection:** A physical TCP socket between two hosts. Connections drop, timeout,
  or reset frequently due to network blips or hardware restarts.
* **Session:** A continuous sequence of bi-directional messages between two defined
  counterparties, identified by `(BeginString, SenderCompID, TargetCompID)`.
  **A FIX session outlives any single TCP connection.** When a socket drops, the
  session re-establishes on a new socket and uses sequence numbers to synchronize.

### Sequence Numbers (`MsgSeqNum`, Tag 34)
Every message transmitted within a session carries a strictly ascending integer
sequence number starting at `1`. Each side maintains two numbers:
* **Expected Inbound Sequence Number:** What sequence number the next incoming message must have.
* **Next Outbound Sequence Number:** What sequence number to stamp on the next outbound message.

If an incoming message arrives with a sequence number higher than expected, a gap
has occurred (messages were lost in transit or on a dropped connection), triggering recovery.

### Session Lifecycle and Messages
* **Logon (`35=A`):** Initiates session synchronization, authenticates IDs, and agrees
  on heartbeat intervals. May set `ResetSeqNumFlag (141=Y)` to reset sequence numbers to 1.
* **Heartbeat (`35=0`) & TestRequest (`35=1`):** Keep-alive mechanism. If no traffic is
  heard within the negotiated `HeartBtInt`, a `TestRequest` is dispatched. If unanswered,
  the connection is dropped.
* **ResendRequest (`35=2`):** Requests the peer to re-transmit lost messages spanning `[BeginSeqNo, EndSeqNo]`.
* **SequenceReset / GapFill (`35=4`):** Sent in response to a `ResendRequest` when past messages
  are either resent with updated sequence numbers or skipped because administrative messages
  are not resent (`GapFillFlag 123=Y`).
* **PossDup (`43=Y`) & PossResend (`97=Y`):** Flags indicating a message is being resent
  and might duplicate an earlier message the application already processed.
* **Logout (`35=5`):** Orderly session termination.

### Administrative vs Application Messages
* **Administrative Messages (`35=0, 1, 2, 3, 4, 5, A`):** Messages required to maintain the
  session state machine. Handled entirely by the engine without involving your trading logic.
* **Application Messages (`35=D, 8, ...`):** Messages that represent business domain actions
  (orders, trades, cancels). Delivered directly to your application handler.

---

## 3. Why the Acceptor Role is Hard

In FIX terminology:
- An **Initiator** establishes the connection (typically buy-side or broker routing to an exchange).
- An **Acceptor** listens for incoming connections and manages sessions (typically exchanges, market makers, or execution venues).

The acceptor is significantly harder to build and operate for several reasons:

1. **The Pre-Session State Dilemma:**
   Before a newly accepted socket sends a `Logon (35=A)`, the engine does not know who
   the peer is, which session they belong to, or what credentials they possess.
   An acceptor must guard itself against slow-loris attacks and socket exhaustion
   without allocating memory or stalling valid counterparties.
2. **Asynchronous Multi-Session Management:**
   An acceptor handles multiple concurrent sessions. In shared-thread models,
   a blocking call or lock contention in one counterparty's session can delay heartbeats
   for all other counterparties, triggering false disconnects.
3. **Resend and Sequence Recovery on the Hot Path:**
   When a peer disconnects and reconnects with a sequence gap, the acceptor must retrieve
   past application messages from its journal and replay them in order with `43=PossDupFlag`,
   all while processing real-time market traffic.
4. **Strict Conformance Standards:**
   FIX counterparties rely on strict conformance rules: exact field ordering,
   checksum calculations, reject codes (`Tag 373`), and precision timestamps.
   A single malformed field or out-of-order tag can cause a counterparty's engine
   to immediately reject a session.

---

## 4. Prior Art in the Rust Ecosystem

Rust's memory safety and zero-cost abstractions make it ideal for high-frequency trading,
yet historical adoption in FIX has faced a vacuum:

* **The C++ `quickfix` FFI Crate:** Has garnered over ~345,000 downloads—roughly 3.4× the
  downloads of all pure-Rust FIX engines combined. It succeeded because it was the only
  readily available crate that could run a complete acceptor, despite requiring a C++ compiler,
  CMake, and foreign function interface overhead.
* **`ferrumfix` (`fefix`):** A pioneering pure-Rust FIX engine with high visibility, but
  unmaintained since 2021 and marked by its authors as unstable.
* **`hotfix`:** A well-structured modern Rust FIX implementation, but focused entirely on the
  initiator role.
* **`forgefix`:** Notable for its precise protocol terminology and documentation.

For an in-depth survey of open-source FIX implementations, licensing, and trade-offs,
consult [`docs/reference/prior-art.md`](reference/prior-art.md).

---

## 5. How fixbolt is Positioned

`fixbolt` is built as an **acceptor-first**, pure-Rust FIX 4.4 engine engineered to deliver
the lowest latency achievable on standard kernel TCP:

* **Pure Session Layer:** The session state machine performs zero heap allocations, executes
  no system calls, holds no locks, and contains no socket operations. Time is injected as discrete ticks.
* **Pre-Session Boundary:** Dedicated pre-session stage isolates unauthenticated sockets
  with explicit connection and timeout limits.
* **Two Operational Modes:**
  * `standard`: Blocks on kernel poll (`epoll` / `kqueue`) when idle, freeing CPU for shared
    hosts and containerized deployments.
  * `hft`: Dedicated CPU core per session, busy-polling with zero kernel sleeps on the hot path.
