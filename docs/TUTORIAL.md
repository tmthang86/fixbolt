# Tutorial: Building a FIX 4.4 Acceptor with fixbolt

This tutorial builds a complete FIX 4.4 acceptor with the `fixbolt` library and shows what
happens on the wire. It goes deeper than [GETTING-STARTED.md](GETTING-STARTED.md) but uses
the same tested files:

- [`crates/library/examples/acceptor.cfg`](../crates/library/examples/acceptor.cfg) — configuration
- [`crates/library/examples/shared/order_handler.rs`](../crates/library/examples/shared/order_handler.rs) — application logic
- [`crates/library/examples/acceptor.rs`](../crates/library/examples/acceptor.rs) — entry point
- [`crates/library/tests/end_to_end.rs`](../crates/library/tests/end_to_end.rs) — socket test

---

## How the pieces fit

```
TCP socket ──► framer ──► session layer ──► App adapter ──► Handler::on_message
                                               │                  │
TCP socket ◄── framer ◄── session layer ◄──────┴── Reply ◄────────┘
```

1. **Transport and framing** read bytes from the socket and cut out whole messages
   (`8=...` through `10=...`).
2. **The session layer** checks sequence numbers and handles Logon, heartbeats, test
   requests, resends and gap fills on its own.
3. **The application adapter** (`fixbolt::App`) hands each application message to your
   [`Handler`](../crates/library/src/app.rs) and turns its [`Reply`](../crates/library/src/reply.rs)
   into bytes.

---

## Step 1: declare the counterparties

An acceptor must know who is allowed to connect. Create `acceptor.cfg`:

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

- `[DEFAULT]` applies to every session below it. `SenderCompID=ISLD` is this engine's name.
- Each `[SESSION]` is one counterparty. A Logon naming a `TargetCompID` that is not listed is
  refused in the pre-session stage; no session object is ever created for it.
- `HeartBtInt=30` is the heartbeat interval in seconds.

---

## Step 2: write the handler

Your logic is an implementation of `fixbolt::Handler`. This one fills every order it receives:

```rust
use fixbolt::{Answer, Handler, Incoming, Reply};

#[derive(Default)]
pub struct Desk {
    fills: u32,
}

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        // 2a. Only orders get a reply.
        if msg.msg_type() != b"D" {
            return reply.silent();
        }

        // 2b. Read the fields we need.
        let (Some(qty), Some(price), Some(cl_ord_id)) = (msg.get(38), msg.get(44), msg.get(11))
        else {
            return reply.silent();
        };

        // 2c. Make an execution id without allocating.
        self.fills += 1;
        let mut buf = [0u8; 16];
        let exec_id = format_exec_id(self.fills, &mut buf);

        // 2d. Build and send the ExecutionReport (35=8).
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
```

### 2a. Filtering

Administrative messages (Heartbeat, TestRequest, ResendRequest, Reject, SequenceReset,
Logout, Logon) are answered by the engine and never reach the handler. For application
messages you do not want to answer, return `reply.silent()`. A silent reply spends no sequence
number.

### 2b. Reading fields without copying

`msg.get(tag)` returns `Option<&[u8]>` pointing into the engine's read buffer. Reading `11`,
`38` or `44` allocates nothing and copies nothing. The borrow lasts only for the duration of
`on_message`; to keep a value, copy the bytes into storage you own.

### 2c. Formatting without allocating

The handler runs on the engine thread, and nothing on that thread may allocate. The helper
formats a number into a stack buffer:

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

### 2d. Building the reply

`reply.message(b"8")` starts an ExecutionReport. Add fields in any order: `Reply` sorts them
into the FIX 4.4 dictionary order before writing. The header (`8`, `9`, `35`, `34`, `49`,
`52`, `56`) and the trailer (`10`) are added for you. Naming one of those seven tags yourself
is ignored rather than merged, because two `34=` in one message would be two sequence numbers.

---

## Step 3: start the engine

```rust
use fixbolt::{Limits, Settings};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg_path = "crates/library/examples/acceptor.cfg";
    let listen_addr = "127.0.0.1:9876";

    // 1. Parse the settings into a table of counterparties.
    let table = Settings::load(cfg_path)?.into_table();
    println!("loaded {} counterparty sessions", table.len());

    // 2. The handles, made before the engine — `serve` hands nothing back
    //    until it has stopped, so this is where an operator's grip comes from.
    let handles = fixbolt::Handles::new();
    let admin = handles.admin();
    std::thread::spawn(move || admin.shutdown(5_000));

    // 3. Start the engine in `standard` mode.
    let shutdown = fixbolt::serve(
        listen_addr,
        table,
        fixbolt::app(Desk::default()),
        64,                       // connections held at once
        Limits::new(64, 30_000)?, // up to 64 pending Logons, 30 s each
        fixbolt::NoLog,           // no message log
        handles,                  // watch it, administer it, stop it
    )?;

    println!("engine stopped: {shutdown:?}");
    Ok(())
}
```

The `Handles` is where `Observer`, `Admin` and `Sender` come from, and it exists before the
engine because `serve` runs the engine on **this** thread and returns only once it has stopped
([ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md)).
[GUIDE.md §8a](GUIDE.md) is what to read next if you want to watch the session step 4 puts on
the wire rather than only stop it.

`Limits::new(pending, logon_ms)` protects the acceptor from sockets that connect and never
log on. A socket that has not sent a valid Logon within `logon_ms` is closed and its slot
freed; when `pending` sockets are already waiting, the next one is refused immediately
([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)). Neither
value has a default, because nobody who has not seen your deployment can pick one.

---

## Step 4: what happens on the wire

[`crates/library/tests/end_to_end.rs`](../crates/library/tests/end_to_end.rs) runs this
exchange through a real socket. `|` stands for the `0x01` separator.

**1. The client logs on (`35=A`).**

```text
8=FIX.4.4|9=59|35=A|34=1|49=TW44|52=20260903-12:00:00.000|56=ISLD|98=0|108=30|10=123|
```

The engine answers with its own Logon. Your handler is not involved.

**2. The client sends an order (`35=D`).**

```text
8=FIX.4.4|9=112|35=D|34=2|49=TW44|52=20260903-12:00:01.000|56=ISLD|11=ORD-1|21=1|38=100|40=2|44=42|54=1|55=IBM|59=0|60=20260903-12:00:01.000|10=234|
```

**3. The acceptor replies with an ExecutionReport (`35=8`).**

```text
8=FIX.4.4|9=138|35=8|34=2|49=ISLD|52=20260903-12:00:01.001|56=TW44|6=42|11=ORD-1|14=100|17=EXEC-1|31=42|32=100|37=EXEC-1|38=100|39=2|54=1|55=IBM|150=F|151=0|10=045|
```

Three things to notice: `49` and `56` are swapped relative to the order, because your sender
is their target; the body fields are in ascending tag order (`6` before `11` before `14`),
whatever order the handler named them in; and `11`, `54` and `55` are copied from the order.

---

## Performance notes

- **The `fixbolt` library is a convenience, not the fastest path.** `[measured 2026-09-05]` on
  the §9 desktop a reply through `Handler` costs about **804 ns**, against **238 ns** to encode
  a template built once — about 3.4×, roughly 570 ns
  ([ADR-0051](decisions/ADR-0051-item-34-is-a-third-of-the-size-it-was-recorded-at.md)). For order entry at
  a few thousand messages a second in `standard` mode, that is not your problem.
- **For `hft`,** implement [`fixbolt::Application`](../crates/session/src/lib.rs) directly and
  build one `Template` per message type at logon. [`crates/conformance/src/echo.rs`](../crates/conformance/src/echo.rs)
  is a worked example, and [GUIDE.md §1b](GUIDE.md) compares the two ways.
- **Never block in `on_message`.** Move database writes and anything that can wait to another
  thread, or use `RingDispatch` so the engine hands messages across a ring
  ([GUIDE.md §2](GUIDE.md)).
