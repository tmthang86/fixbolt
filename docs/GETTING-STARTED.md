# Getting Started with fixbolt

Run a FIX 4.4 acceptor in three steps: a configuration file, a handler, and one call to start
the engine. Every snippet here is taken from code the test suite runs:

- [`crates/library/examples/acceptor.cfg`](../crates/library/examples/acceptor.cfg) — the configuration
- [`crates/library/examples/shared/order_handler.rs`](../crates/library/examples/shared/order_handler.rs) — the handler
- [`crates/library/examples/acceptor.rs`](../crates/library/examples/acceptor.rs) — the entry point
- [`crates/library/tests/end_to_end.rs`](../crates/library/tests/end_to_end.rs) — drives the example through a real socket

> **Not on crates.io yet.** Every crate is `version = "0.0.0"` and `publish = false`. Depend
> on `fixbolt` by path inside this workspace or as a git dependency. `cargo add fixbolt` will
> not work.

Before anything else, fetch the FIX dictionary the build needs:

```sh
scripts/fetch-quickfix-assets.sh
```

---

## Step 1: the configuration file

`fixbolt` reads counterparties and session settings from an INI-style file in the same shape
as QuickFIX's, so an existing QuickFIX configuration will look familiar.

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

`[DEFAULT]` holds values shared by every session. Each `[SESSION]` names one counterparty
that is allowed to log on. A socket whose Logon names any other `TargetCompID` is closed
without a reply.

Four things differ from QuickFIX, each on purpose
([ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md)):

- **An unknown key is an error.** A mistyped `Starttime` stops startup instead of being
  ignored.
- **A file with no `[SESSION]` is an error.** An acceptor that serves nobody looks exactly like
  a firewall dropping the port.
- **A half-written schedule is an error.** `StartTime` without `EndTime` is refused, not
  completed with midnight.
- **Times are UTC.** There is no timezone name and no offset key.

Every key is described in [CONFIGURATION.md](CONFIGURATION.md).

---

## Step 2: the handler

Your code implements the [`Handler`](../crates/library/src/app.rs) trait. By the time
`on_message` is called, the message has been framed, indexed and validated, and every
administrative message (`35=0, 1, 2, 3, 4, 5, A`) has already been answered by the session
layer. Only application messages reach you.

```rust
use fixbolt::{Answer, Handler, Incoming, Reply};

#[derive(Default)]
pub struct Desk {
    fills: u32,
}

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        // Only NewOrderSingle (35=D) gets a reply; everything else is ignored.
        if msg.msg_type() != b"D" {
            return reply.silent();
        }

        // Fields are borrowed straight out of the engine's read buffer.
        let (Some(qty), Some(price), Some(cl_ord_id)) = (msg.get(38), msg.get(44), msg.get(11))
        else {
            return reply.silent();
        };

        self.fills += 1;
        let mut buf = [0u8; 16];
        let exec_id = format_exec_id(self.fills, &mut buf);

        // Build an ExecutionReport (35=8) that fills the order.
        reply
            .message(b"8")
            .field(37, exec_id)                  // OrderID
            .field(17, exec_id)                  // ExecID
            .field(150, b"F")                    // ExecType = Trade
            .field(39, b"2")                     // OrdStatus = Filled
            .field(11, cl_ord_id)                // ClOrdID, echoed
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

Two things to know about the handler:

- **You never write the header or trailer.** Tags `8`, `9`, `10`, `34`, `49`, `52` and `56`
  are written by [`Reply`](../crates/library/src/reply.rs). Fields you name are emitted in the
  dictionary's order no matter which order you call `.field(...)` in.
- **It runs on the engine thread.** Do not block, query a database, or wait on the network
  inside `on_message`. A stalled handler stalls heartbeats and every other session on that
  thread ([GUIDE.md §2](GUIDE.md)).

---

## Step 3: start the engine

Load the settings into a table and call `fixbolt::serve`:

```rust
use fixbolt::{Limits, Settings};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = "crates/library/examples/acceptor.cfg";
    let addr = "127.0.0.1:9876";

    // 1. Load the configuration into a table of counterparties.
    let table = Settings::load(cfg)?.into_table();
    println!("serving {} counterparties on {addr}", table.len());

    // 2. The handles, made BEFORE the engine. `serve` returns nothing until it
    //    has stopped, so this is the only moment a handle can be taken.
    let handles = fixbolt::Handles::new();
    let admin = handles.admin();
    std::thread::spawn(move || {
        // Wire this to whatever your deployment uses to say "shut down" — a
        // signal handler, an admin socket, a message on a queue.
        admin.shutdown(5_000);
    });

    // 3. Start the acceptor.
    let shutdown = fixbolt::serve(
        addr,
        table,
        fixbolt::app(Desk::default()),
        64,                       // connections held at once
        Limits::new(64, 30_000)?, // sockets waiting to log on, and how long each has (ms)
        fixbolt::NoLog,           // no message log; FileLog::open(path) turns one on
        handles,                  // watch it, administer it, stop it
    )?;

    println!("stopped: {shutdown:?}");
    Ok(())
}
```

The two numbers after the handler are yours to choose; there are no defaults for them:

- **Capacity** is how many logged-on connections the engine holds at once.
- **`Limits::new(pending, logon_ms)`** bounds the pre-session stage: how many sockets may wait
  for their Logon at the same time, and how long each may take. A socket that opens and says
  nothing is dropped after `logon_ms`
  ([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)).

The message log comes next. `fixbolt::NoLog` writes nothing;
`fixbolt::FileLog::open(path)` writes every message seen or sent to a text file
([GUIDE.md §6c](GUIDE.md)).

The last argument is `Handles`, and everything an operator can do to a running engine comes off
it: `handles.observer()` to watch, `handles.admin()` to change or stop, `handles.sender()` to
originate. It is passed **into** `serve` rather than handed back by it, because `serve` does not
return until the engine has stopped — so a handle taken afterwards would be a handle on nothing
([ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md)).

`serve` returns when an operator stops the engine through `Admin::shutdown`, and the
`Shutdown` it returns says whether every counterparty answered the Logout
([ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md)).
`[2026-09-05]` **that sentence used to be unreachable through this page's own API** — an `Admin`
came off an `Engine`, and nothing on this page ever holds one. `STATUS.md` item 47;
`crates/library/tests/end_to_end.rs::an_operator_stops_the_front_door_and_serve_comes_back` is
what keeps it true.

### `standard` or `hft`

- **`standard`** is what `serve` gives you. The engine blocks in the OS poller when idle, so it
  is right for development, shared servers, containers and gateways with many sessions.
- **`hft`** is `fixbolt::serve_hft`. The engine spins on the calling thread and never sleeps
  in the kernel. It needs a Linux machine set up as [DESIGN.md §9](DESIGN.md) describes and a
  core you pin the thread to yourself
  ([ADR-0012](decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md),
  [ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md)). Read
  [GUIDE.md §0](GUIDE.md) before choosing it.

---

## Run the example

```sh
cargo run --example acceptor -- crates/library/examples/acceptor.cfg 127.0.0.1:9876
```

The acceptor binds to `127.0.0.1:9876`, loads `TW44` and `BANZAI`, and waits for a FIX
client. [`crates/library/tests/end_to_end.rs`](../crates/library/tests/end_to_end.rs) runs
exactly this over a real TCP socket, so the example is tested rather than merely compiled.

Next: [TUTORIAL.md](TUTORIAL.md) walks through the same code in more detail and shows the
bytes on the wire.
