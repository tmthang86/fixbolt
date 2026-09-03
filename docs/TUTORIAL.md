# Tutorial: Building a FIX 4.4 Acceptor with fixbolt

This tutorial walks through building a complete, high-performance FIX 4.4 acceptor
from scratch using the `fixbolt` library.

The code in this tutorial is not pseudocode: every snippet is drawn directly from
tested files in the repository:
- [`crates/library/examples/acceptor.cfg`](../crates/library/examples/acceptor.cfg) (configuration)
- [`crates/library/examples/shared/order_handler.rs`](../crates/library/examples/shared/order_handler.rs) (application logic)
- [`crates/library/examples/acceptor.rs`](../crates/library/examples/acceptor.rs) (entry point)
- [`crates/library/tests/end_to_end.rs`](../crates/library/tests/end_to_end.rs) (socket test suite)

---

## Architecture Overview

`fixbolt` separates FIX processing into clear layers:
1. **Transport & Framing:** Receives TCP bytes, extracts full messages (`8=...10=...`).
2. **Session State Machine:** Validates message sequence numbers, handles Logons, heartbeats,
   test requests, resends, and gap fills automatically.
3. **Application Facade (`fixbolt`):** Adapts decoded incoming messages to your application
   via [`Handler`](../crates/library/src/app.rs) and formats replies with [`Reply`](../crates/library/src/reply.rs).

```
TCP Socket ──► Framer ──► Session Layer ──► App Adapter ──► Handler::on_message
                                               │                  │
TCP Socket ◄── Framer ◄── Session Layer ◄──────┴── Reply ◄────────┘
```

---

## Step 1: Defining the Session Configuration

FIX acceptors require strict identification of counterparty connections. `fixbolt`
loads these from an INI-style configuration file.

Create `acceptor.cfg` (matching [`crates/library/examples/acceptor.cfg`](../crates/library/examples/acceptor.cfg)):

```ini
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=ISLD

[SESSION]
TargetCompID=TW44
HeartBtInt=30

[SESSION]
TargetCompID=BANZAI
HeartBtInt=30
```

### Configuration Semantics
- `[DEFAULT]`: Specifies parameters shared across sessions. `SenderCompID=ISLD` identifies this engine.
- `[SESSION]`: Declares each allowed counterparty (`TargetCompID`). Sockets attempting to log on
  with an undeclared `TargetCompID` are rejected during pre-session negotiation without creating
  a session object.
- `HeartBtInt=30`: Heartbeat interval in seconds.

---

## Step 2: Implementing the Order Handler

Your application logic lives in an implementation of `fixbolt::Handler`.

Here is the complete implementation of a filling desk from [`crates/library/examples/shared/order_handler.rs`](../crates/library/examples/shared/order_handler.rs#L37-L80):

```rust
use fixbolt::{Answer, Handler, Incoming, Reply};

#[derive(Default)]
pub struct Desk {
    fills: u32,
}

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        // Step 2a: Filter message types
        if msg.msg_type() != b"D" {
            return reply.silent();
        }

        // Step 2b: Extract required fields
        let (Some(qty), Some(price), Some(cl_ord_id)) = (msg.get(38), msg.get(44), msg.get(11))
        else {
            return reply.silent();
        };

        // Step 2c: Generate execution identifier
        self.fills += 1;
        let mut buf = [0u8; 16];
        let exec_id = format_exec_id(self.fills, &mut buf);

        // Step 2d: Construct and send ExecutionReport (35=8)
        reply
            .message(b"8")
            .field(37, exec_id)                  // OrderID
            .field(17, exec_id)                  // ExecID
            .field(150, b"F")                    // ExecType — Trade
            .field(39, b"2")                     // OrdStatus — Filled
            .field(11, cl_ord_id)                // ClOrdID (echoed)
            .field(55, msg.get(55).unwrap_or(b"")) // Symbol
            .field(54, msg.get(54).unwrap_or(b"")) // Side
            .field(38, qty)                      // OrderQty
            .field(32, qty)                      // LastQty
            .field(31, price)                    // LastPx
            .field(14, qty)                      // CumQty
            .field(151, b"0")                    // LeavesQty
            .field(6, price)                     // AvgPx
            .send()
    }
}
```

### Detailed Breakdown

#### Filtering Administrative Traffic (`2a`)
Administrative messages (`Heartbeat (0)`, `TestRequest (1)`, `ResendRequest (2)`,
`Reject (3)`, `SequenceReset (4)`, `Logout (5)`, `Logon (A)`) are answered directly
by the engine. Your handler only receives application messages. Non-order application
types should return `reply.silent()`.

#### Zero-Copy Field Extraction (`2b`)
`msg.get(tag)` returns `Option<&'a [u8]>` referencing the engine's internal read buffer.
No string allocations, copies, or heap allocations occur when reading fields like
`11 (ClOrdID)`, `38 (OrderQty)`, or `44 (Price)`.

#### Stack-Based Execution IDs (`2c`)
Formatting numbers without allocation:
```rust
fn format_exec_id(n: u32, buf: &mut [u8; 16]) -> &[u8] {
    buf[..5].copy_from_slice(b"EXEC-");
    let mut digits = [0u8; 10];
    let mut v = n;
    let mut i = 10;
    if v == 0 {
        i = 9;
        digits[9] = b'0';
    }
    while v > 0 && i > 0 {
        i -= 1;
        digits[i] = b'0' + u8::try_from(v % 10).unwrap_or(0);
        v /= 10;
    }
    let len = 10 - i;
    buf[5..5 + len].copy_from_slice(&digits[i..]);
    &buf[..5 + len]
}
```

#### Outbound Message Construction (`2d`)
Calling `reply.message(b"8")` initiates a message. Fields can be added in any order;
`Reply` sorts all fields canonically according to the FIX 4.4 dictionary specification
before writing to the socket buffer. Standard header fields (`8=FIX.4.4`, `9=BodyLength`,
`35=MsgType`, `34=SeqNum`, `49=SenderCompID`, `52=SendingTime`, `56=TargetCompID`) and
the trailer (`10=CheckSum`) are automatically added.

---

## Step 3: Bootstrapping the Engine

Wiring the configuration, the application handler, and the network listener
together into an executable:

From [`crates/library/examples/acceptor.rs`](../crates/library/examples/acceptor.rs#L26-L55):

```rust
use fixbolt::{Limits, Settings};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg_path = "crates/library/examples/acceptor.cfg";
    let listen_addr = "127.0.0.1:9876";

    // 1. Parse settings into routing table:
    let table = Settings::load(cfg_path)?.into_table();
    println!("Loaded {} counterparty sessions", table.len());

    // 2. Start the engine in `standard` mode:
    let shutdown = fixbolt::serve(
        listen_addr,
        table,
        fixbolt::app(Desk::default()),
        64,                       // Max active connections
        Limits::new(64, 30_000)?, // Max 64 pending handshakes, 30s timeout
    )?;

    println!("Engine stopped: {shutdown:?}");
    Ok(())
}
```

### Pre-Session Limits: `Limits::new`
Acceptor sockets pass through a pre-session handshake before logon completes.
`Limits::new(max_pending, timeout_millis)` prevents socket exhaustion attacks:
if a counterparty connects but fails to send a valid `Logon` message within
`timeout_millis`, the engine closes the socket and frees the connection slot ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)).

---

## Step 4: Testing the Acceptor

`crates/library/tests/end_to_end.rs` verifies this flow against a real operating system
socket. Here is the expected wire exchange:

### 1. Client Sends Logon (`35=A`)
```text
8=FIX.4.4|9=59|35=A|34=1|49=TW44|52=20260903-12:00:00.000|56=ISLD|98=0|108=30|10=123|
```
The engine responds automatically with its own `35=A` logon confirmation.

### 2. Client Sends NewOrderSingle (`35=D`)
```text
8=FIX.4.4|9=112|35=D|34=2|49=TW44|52=20260903-12:00:01.000|56=ISLD|11=ORD-1|21=1|38=100|40=2|44=42|54=1|55=IBM|59=0|60=20260903-12:00:01.000|10=234|
```

### 3. Acceptor Returns ExecutionReport (`35=8`)
```text
8=FIX.4.4|9=138|35=8|34=2|49=ISLD|52=20260903-12:00:01.001|56=TW44|6=42|11=ORD-1|14=100|17=EXEC-1|31=42|32=100|37=EXEC-1|38=100|39=2|54=1|55=IBM|150=F|151=0|10=045|
```
Note how `49` and `56` are reversed, `6` precedes `37` in dictionary order, and `11`, `54`, `55`
are preserved from the order.

---

## Performance Considerations

- **Library convenience vs raw speed:** The `fixbolt` library API provides convenience
  at roughly ~2.1 µs per message ([ADR-0041](decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md)).
- **Sub-microsecond HFT:** If your application requires latency under 1 µs, implement
  [`fixbolt_session::Application`](../crates/session/src/lib.rs) directly and pre-compile
  outbound message templates. See [`crates/conformance/src/echo.rs`](../crates/conformance/src/echo.rs) for a worked example.
- **Engine thread isolation:** Always process compute-heavy tasks or database writes on a
  worker thread; never block inside `Handler::on_message`.
