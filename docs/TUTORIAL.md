# Tutorial: Building a FIX 4.4 Acceptor with fixbolt

*For a Rust developer who wants to understand each part of a fixbolt acceptor, not only run one.
By the end you have this repository's own example acceptor running on your machine, you know what
every line of it does, and you have read the Logon, the order and the ExecutionReport it exchanges
on the wire.*

This tutorial builds a complete FIX 4.4 acceptor with the `fixbolt` library and shows what
happens on the wire. It goes deeper than [GETTING-STARTED.md](GETTING-STARTED.md). The
configuration file and every Rust block below are byte-for-byte copies of the example's own
files, the Rust ones compiled by CI, and `scripts/check-doc-samples.sh` fails CI if a copy
drifts. The files are:

- [`crates/library/examples/acceptor.cfg`](../crates/library/examples/acceptor.cfg) — configuration
- [`crates/library/examples/shared/order_handler.rs`](../crates/library/examples/shared/order_handler.rs) — application logic
- [`crates/library/examples/acceptor.rs`](../crates/library/examples/acceptor.rs) — entry point
- [`crates/library/tests/end_to_end.rs`](../crates/library/tests/end_to_end.rs) — socket test

The `// region:` and `// endregion:` comments in the Rust files mark the parts this page shows.

---

## How the pieces fit

```text
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

An acceptor must know who is allowed to connect. This is the example's configuration file,
whole:

<!-- sample: crates/library/examples/acceptor.cfg -->
```ini
# The acceptor `examples/acceptor.rs` serves, in QuickFIX's shape — ADR-0040.
#
# Three things behave differently from QuickFIX, each deliberately:
#   * an unrecognised key is an error, not something ignored;
#   * a file naming no [SESSION] is an error;
#   * a StartTime with no EndTime is an error, not one completed with midnight.
#
# Times are UTC. There is no timezone name and no UtcOffsetMillis key — see
# docs/GUIDE.md §5a for why a fixed offset in a settings file is a hazard
# wearing the clothes of a setting.

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

- Lines starting with `#` are comments. The header names the three places this file format
  refuses what QuickFIX would quietly accept
  ([ADR-0040](decisions/ADR-0040-a-configuration-file-refuses-what-it-does-not-understand.md)).
- `[DEFAULT]` applies to every session below it. `SenderCompID=ISLD` is this engine's name.
- Each `[SESSION]` is one counterparty. A Logon naming a `TargetCompID` that is not listed is
  refused in the pre-session stage; no session object is ever created for it.
- `HeartBtInt=30` is the heartbeat interval in seconds.

Every key the file accepts is in [CONFIGURATION.md](CONFIGURATION.md).

---

## Step 2: write the handler

Your logic is an implementation of `fixbolt::Handler`. The example keeps it in a file of its own,
`examples/shared/order_handler.rs`, so that the example and the socket test in Step 4 compile the
same code. This one fills every order it receives:

<!-- sample: crates/library/examples/shared/order_handler.rs#handler -->
```rust
use fixbolt::{Answer, Handler, Incoming, Reply};

/// A desk that fills whatever it is sent.
///
/// The counter is deliberately not readable from outside: `serve` takes the
/// application by value and never gives it back, so an accessor here would be
/// a method neither of this file's two consumers could call. What the count
/// does instead is reach the wire, as `EXEC-1`, `EXEC-2` — which is where
/// `tests/end_to_end.rs` asserts that it advances.
#[derive(Default)]
pub struct Desk {
    fills: u32,
}

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        // Everything that is not an order is somebody else's business. The
        // session has already answered the seven administrative types itself.
        if msg.msg_type() != b"D" {
            return reply.silent();
        }

        // An order with no quantity or no price is not one this desk can fill.
        // Refusing here rather than filling at zero: `silent()` says the
        // decision was made, where a `35=8` with an empty `31=` would be a
        // message the counterparty has to interpret.
        let (Some(qty), Some(price), Some(cl_ord_id)) = (msg.get(38), msg.get(44), msg.get(11))
        else {
            return reply.silent();
        };

        self.fills += 1;
        let mut buf = [0u8; 16];
        let exec_id = exec_id(self.fills, &mut buf);

        reply
            .message(b"8")
            .field(37, exec_id) // OrderID
            .field(17, exec_id) // ExecID
            .field(150, b"F") // ExecType — Trade
            .field(39, b"2") // OrdStatus — Filled
            .field(11, cl_ord_id) // echoed, borrowed from the read buffer
            .field(55, msg.get(55).unwrap_or(b"")) // Symbol
            .field(54, msg.get(54).unwrap_or(b"")) // Side
            .field(38, qty) // OrderQty
            .field(32, qty) // LastQty
            .field(31, price) // LastPx
            .field(14, qty) // CumQty
            .field(151, b"0") // LeavesQty — filled, so none left
            .field(6, price) // AvgPx
            .send()
    }
}
```

### Filtering

The first `if` lets through only NewOrderSingle (`35=D`). Administrative messages (Heartbeat,
TestRequest, ResendRequest, Reject, SequenceReset, Logout, Logon) are answered by the engine and
never reach the handler. For application messages you do not want to answer, return
`reply.silent()`. A silent reply spends no sequence number.

### Reading fields without copying

`msg.get(tag)` returns `Option<&[u8]>` pointing into the engine's read buffer. Reading `11`,
`38` or `44` allocates nothing and copies nothing. The borrow lasts only for the duration of
`on_message`; to keep a value, copy the bytes into storage you own. The `let ... else` turns an
order missing any of the three into a silent reply rather than a fill at zero.

### Formatting without allocating

The handler runs on the engine thread, and nothing on that thread may allocate. So the
execution id, `EXEC-1`, `EXEC-2` and so on, comes from a helper in the same file that formats a
number into a stack buffer the handler owns:

<!-- sample: crates/library/examples/shared/order_handler.rs#exec_id -->
```rust
/// `EXEC-<n>` into `buf`, with no allocation.
///
/// Ten digits is `u32::MAX` and the prefix is five bytes, so sixteen is always
/// enough and the slice below is always in range.
fn exec_id(n: u32, buf: &mut [u8; 16]) -> &[u8] {
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

### Building the reply

`reply.message(b"8")` starts an ExecutionReport. Add fields in any order: `Reply` sorts them
into the FIX 4.4 dictionary order before writing. The header (`8`, `9`, `35`, `34`, `49`,
`52`, `56`) and the trailer (`10`) are added for you. Naming one of those seven tags yourself
is ignored rather than merged, because two `34=` in one message would be two sequence numbers.

---

## Step 3: start the engine

This is the example's `main`, whole:

<!-- sample: crates/library/examples/acceptor.rs#main -->
```rust
#[cfg(all(feature = "standard", unix))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use fixbolt::{Limits, Settings};

    let mut args = std::env::args().skip(1);
    let cfg = args
        .next()
        .unwrap_or_else(|| "crates/library/examples/acceptor.cfg".to_owned());
    let addr = args.next().unwrap_or_else(|| "127.0.0.1:9876".to_owned());

    // A mistyped path, a mistyped key or a file naming no counterparty all stop
    // here, with the line number and what was written. An acceptor that starts
    // cleanly and serves nobody looks exactly like a firewall dropping the port
    // — ADR-0040.
    let table = Settings::load(&cfg)?.into_table()?;
    println!("serving {} counterparties on {addr}", table.len());

    // **Made before the engine, because `serve` never hands anything back
    // until it has finished.** Everything an operator can do to a running
    // engine — watch it, move a sequence number, stop it, send something it was
    // not asked for — comes off this one object. ADR-0054.
    let handles = fixbolt::Handles::new();

    // The stop. Wire it to whatever your deployment uses to say "shut down";
    // here it is a line on stdin, so the example needs no dependency to
    // demonstrate the thing that matters — `serve` comes back on its own.
    let admin = handles.admin();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
        // Up to five seconds for counterparties to answer the goodbye.
        admin.shutdown(5_000);
    });

    let shutdown = fixbolt::serve(
        &addr,
        table,
        fixbolt::app(order_handler::Desk::default()),
        64,                       // connections held at once
        Limits::new(64, 30_000)?, // sockets waiting to log on, and how long each has
        // No message log. `fixbolt::FileLog::open(path)` here writes every
        // message this acceptor sees or sends, both directions, one line each.
        fixbolt::NoLog,
        handles,
    )?;

    // `serve` returns when an operator asks it to stop, and says what it could
    // not finish. Printing it is the difference between a planned close and a
    // process that vanished — ADR-0038.
    println!("stopped: {shutdown:?}");
    Ok(())
}
```

Taking it from the top:

- **`#[cfg(all(feature = "standard", unix))]`.** `serve` exists only with the `standard`
  feature, which is on by default, on a Unix target. On any other target the example compiles a
  second `main` that only says why. `order_handler::Desk` is Step 2's handler: the example pulls
  the file in as a module with `#[path = "shared/order_handler.rs"]`; in your own crate it is an
  ordinary module.
- **Two arguments**, the configuration file and the address to listen on, each with a default.
- **`Settings::load(...)?.into_table()?`** parses the file into a table of counterparties. A
  mistyped path, an unknown key or a file naming no counterparty stops the program here, with
  the line number, instead of starting an acceptor that serves nobody.
- **`Handles::new()`** comes before the engine, because `serve` runs the engine on **this**
  thread and returns only once it has stopped
  ([ADR-0054](decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md)).
  The `Handles` is where `Observer`, `Admin` and `Sender` come from. [GUIDE.md §8a](GUIDE.md) is
  what to read next if you want to watch the session Step 4 puts on the wire rather than only
  stop it.
- **The stop** is a thread that waits for one line on standard input and then calls
  `admin.shutdown(5_000)`: log every counterparty out and give each up to five seconds to answer.
  In a deployment, wire the same call to whatever says "shut down" there, such as a signal
  handler or an admin socket.
- **`fixbolt::serve`** runs the acceptor in `standard` mode: `64` connections held at once, the
  `Limits`, no message log (`fixbolt::FileLog::open(path)` in its place writes every message in
  both directions), and the handles.
- **The return value** is a `Shutdown` that names what the engine could not finish, such as a
  counterparty that never answered the Logout. Printing it is what tells a planned close from a
  process that vanished
  ([ADR-0038](decisions/ADR-0038-an-ordered-shutdown-is-a-state-not-a-flag.md)).

`Limits::new(pending, logon_ms)` protects the acceptor from sockets that connect and never
log on. A socket that has not sent a valid Logon within `logon_ms` is closed and its slot
freed; when `pending` sockets are already waiting, the next one is refused immediately
([ADR-0020](decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)). Neither
value has a default, because nobody who has not seen your deployment can pick one.

### Run it

From a checkout of the repository:

```sh
cargo run --example acceptor -- crates/library/examples/acceptor.cfg 127.0.0.1:9876
```

It prints `serving 2 counterparties on 127.0.0.1:9876` and waits for connections. Press Enter
to stop it; it prints the `stopped:` line and exits.

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
