# Getting Started with fixbolt

A concise guide to embedding `fixbolt` as a FIX 4.4 acceptor in three steps:
configuration, application handler, and engine bootstrap.

This guide follows the QuickFIX/J pattern (config → application → bootstrap)
and is grounded entirely in tested code from [`crates/library/examples/acceptor.rs`](../crates/library/examples/acceptor.rs),
[`crates/library/examples/acceptor.cfg`](../crates/library/examples/acceptor.cfg), and
[`crates/library/tests/end_to_end.rs`](../crates/library/tests/end_to_end.rs).

> Note on availability: `fixbolt` is not yet published to crates.io (`version = "0.0.0"`).
> Depend on it via path in this workspace or via git dependency in your `Cargo.toml`.
> Do not use `cargo add fixbolt`.

---

## 1. Configuration: `acceptor.cfg`

`fixbolt` loads counterparties and session parameters from an INI file formatted
like QuickFIX's configuration, with explicit sections for `[DEFAULT]` and each `[SESSION]`.

Here is [`crates/library/examples/acceptor.cfg`](../crates/library/examples/acceptor.cfg):

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

### Key Differences from QuickFIX (ADR-0040)
* **Strict keys:** An unrecognised key is an error that halts startup, never silently ignored.
* **Explicit sessions:** A file with no `[SESSION]` section fails immediately rather than silently listening for nobody.
* **Strict schedules:** Specifying a `StartTime` without an `EndTime` fails instead of defaulting to midnight.
* **UTC only:** All times are UTC; there is no timezone name or offset parsing in the settings file.

---

## 2. The Application Handler: `Handler`

Your application implements the [`Handler`](../crates/library/src/app.rs) trait.
Incoming messages are already framed, indexed, and validated.
Administrative messages (`35=0, 1, 2, 3, 4, 5, A`) are handled automatically
by the session state machine; only application messages reach your handler.

Here is the worked order handler from [`crates/library/examples/shared/order_handler.rs`](../crates/library/examples/shared/order_handler.rs):

```rust
use fixbolt::{Answer, Handler, Incoming, Reply};

#[derive(Default)]
pub struct Desk {
    fills: u32,
}

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        // Administrative messages were already answered by the session layer.
        // Ignore any non-order messages:
        if msg.msg_type() != b"D" {
            return reply.silent();
        }

        // Extract fields directly from the engine's read buffer (zero copy):
        let (Some(qty), Some(price), Some(cl_ord_id)) = (msg.get(38), msg.get(44), msg.get(11))
        else {
            return reply.silent();
        };

        self.fills += 1;
        let mut buf = [0u8; 16];
        let exec_id = format_exec_id(self.fills, &mut buf);

        // Build the outbound ExecutionReport (35=8):
        reply
            .message(b"8")
            .field(37, exec_id)                  // OrderID
            .field(17, exec_id)                  // ExecID
            .field(150, b"F")                    // ExecType (Trade)
            .field(39, b"2")                     // OrdStatus (Filled)
            .field(11, cl_ord_id)                // ClOrdID (borrowed echo)
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

### Handler Invariants
* **Header & trailer fields are automatic:** Fields `8`, `9`, `10`, `34`, `49`, `52`, and `56` are
  managed by [`Reply`](../crates/library/src/reply.rs). Outbound fields are emitted in canonical
  dictionary order regardless of the order you chain `.field(...)` calls.
* **The engine thread is inline:** `Handler::on_message` runs directly on the engine thread ([ADR-0002](../docs/decisions/ADR-0002-engine-library-split.md)).
  **Do not block**, perform synchronous database queries, or await network calls in this method.
  Blocking the handler blocks heartbeat generation and all other sessions on the core.

---

## 3. Bootstrap: `serve`

To start the engine, load the configuration table and call `fixbolt::serve`.
Here is `main` from [`crates/library/examples/acceptor.rs`](../crates/library/examples/acceptor.rs):

```rust
use fixbolt::{Limits, Settings};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = "crates/library/examples/acceptor.cfg";
    let addr = "127.0.0.1:9876";

    // 1. Load configuration table:
    let table = Settings::load(cfg)?.into_table();
    println!("serving {} counterparties on {addr}", table.len());

    // 2. Start the acceptor engine:
    let shutdown = fixbolt::serve(
        addr,
        table,
        fixbolt::app(Desk::default()),
        64,                       // Maximum concurrent connections
        Limits::new(64, 30_000)?, // Pending logon sockets (max 64, timeout 30s)
    )?;

    println!("Acceptor stopped cleanly: {shutdown:?}");
    Ok(())
}
```

### Modes: `standard` vs `hft`
* **`standard` (default):** Blocks in the OS poller (`epoll` / `kqueue`) when idle, yielding CPU back
  to the scheduler. Ideal for development, shared servers, containerized workloads, and desks
  handling multiple concurrent connections per core.
* **`hft` (opt-in):** Calls `fixbolt::serve_hft`. Pins one thread per session and busy-spins
  with zero kernel sleeps on the hot path ([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md),
  [ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md)). Requires dedicated, isolated CPU cores
  configured according to [`docs/DESIGN.md`](DESIGN.md) §9.

---

## Running the Example

Run the acceptor example included in the repository:

```bash
cargo run --example acceptor -- crates/library/examples/acceptor.cfg 127.0.0.1:9876
```

The example will bind to `127.0.0.1:9876`, load `TW44` and `BANZAI`, and await FIX connections.
This exact flow is exercised over a live TCP socket by the test suite in
[`crates/library/tests/end_to_end.rs`](../crates/library/tests/end_to_end.rs).
