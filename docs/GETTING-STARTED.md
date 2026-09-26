# Getting Started with fixbolt

*For a Rust developer who wants a FIX 4.4 acceptor running before reading anything else. By the
end you have a binary that accepts a Logon from either of two configured counterparties, answers
their orders with fills, and stops cleanly when you press Enter.*

Run a FIX 4.4 acceptor in two steps: a configuration file, and one file of Rust that wires up a
handler and starts the engine. Both blocks below are pasted **verbatim** into a fresh crate and
run against a real socket by `scripts/stranger-check.sh` — a client sharing no code with this
repository logs on, logs out, and stops the acceptor — so this page cannot drift from working
code the way prose next to an example can.

```sh
cargo add fixbolt --git https://github.com/tmthang86/fixbolt --tag v0.1.0
```

```toml
fixbolt = { git = "https://github.com/tmthang86/fixbolt", tag = "v0.1.0" }
```

**Not on crates.io** — by decision, not by omission:
[ADR-0161](decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)
released `0.1.0` as the git tag `v0.1.0` rather than a crates.io upload, so this is the whole
install, and it stays this way unless a later ADR decides otherwise. There is no docs.rs page
either — API documentation is `cargo doc --open` in your own checkout of the dependency.

`fixbolt-dict` ships QuickFIX's FIX 4.4 dictionary inside the crate itself (`spec/FIX44.xml`,
under [`NOTICE`](../NOTICE)), so there is nothing else to fetch and no `vendor/` checkout: the
`cargo add` line above is the whole install, with nothing but GitHub and crates.io reachable (the
tree itself comes from GitHub; a handful of ordinary dependencies such as `roxmltree` and `libc`
still come from crates.io the normal way) — no `vendor/` checkout either way
([ADR-0104](decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)).

---

## Step 1: the configuration file

`fixbolt` reads counterparties and session settings from an INI-style file in the same shape
as QuickFIX's, so an existing QuickFIX configuration will look familiar.

<!-- stranger-check: acceptor.cfg -->
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

## Step 2: the handler and the entry point

Your code implements the [`Handler`](../crates/library/src/app.rs) trait. By the time
`on_message` is called, the message has been framed, indexed and validated, and every
administrative message (`35=0, 1, 2, 3, 4, 5, A`) has already been answered by the session
layer. Only application messages reach you.

This is a whole `main.rs` — the handler, and the call to `fixbolt::serve` that loads Step 1's
configuration file and runs it — because that is what `scripts/stranger-check.sh` builds and
runs: paste it into a fresh binary crate's `src/main.rs` next to Step 1's file saved as
`acceptor.cfg`, and `cargo run` gives you a working acceptor on `127.0.0.1:9876`.

<!-- stranger-check: main.rs -->
```rust
use fixbolt::{Answer, Handler, Incoming, Limits, Reply, Settings};

#[derive(Default)]
struct Desk {
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
        let exec_id = exec_id(self.fills, &mut buf);

        // Build an ExecutionReport (35=8) that fills the order.
        reply
            .message(b"8")
            .field(37, exec_id)                    // OrderID
            .field(17, exec_id)                    // ExecID
            .field(150, b"F")                      // ExecType = Trade
            .field(39, b"2")                       // OrdStatus = Filled
            .field(11, cl_ord_id)                  // ClOrdID, echoed
            .field(55, msg.get(55).unwrap_or(b"")) // Symbol
            .field(54, msg.get(54).unwrap_or(b"")) // Side
            .field(38, qty)                        // OrderQty
            .field(32, qty)                        // LastQty
            .field(31, price)                      // LastPx
            .field(14, qty)                        // CumQty
            .field(151, b"0")                      // LeavesQty
            .field(6, price)                       // AvgPx
            .send()
    }
}

/// `EXEC-<n>` into `buf`, with no allocation.
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Both arguments are yours to change: Step 1's file need not be named
    // "acceptor.cfg", and the address need not be this one.
    let mut args = std::env::args().skip(1);
    let cfg = args.next().unwrap_or_else(|| "acceptor.cfg".to_owned());
    let addr = args.next().unwrap_or_else(|| "127.0.0.1:9876".to_owned());

    // 1. Load the configuration into a table of counterparties.
    let table = Settings::load(&cfg)?.into_table()?;
    println!("serving {} counterparties on {addr}", table.len());

    // 2. The handles, made BEFORE the engine. `serve` returns nothing until it
    //    has stopped, so this is the only moment a handle can be taken.
    let handles = fixbolt::Handles::new();
    let admin = handles.admin();
    std::thread::spawn(move || {
        // Wire this to whatever your deployment uses to say "shut down" — a
        // signal handler, an admin socket, a message on a queue. Here, one
        // line on stdin, so this file needs no extra dependency to
        // demonstrate that `serve` comes back on its own.
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
        admin.shutdown(5_000);
    });

    // 3. Start the acceptor.
    let shutdown = fixbolt::serve(
        &addr,
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

Two things to know about the handler:

- **You never write the header or trailer.** Tags `8`, `9`, `10`, `34`, `49`, `52` and `56`
  are written by [`Reply`](../crates/library/src/reply.rs). Fields you name are emitted in the
  dictionary's order no matter which order you call `.field(...)` in.
- **It runs on the engine thread.** Do not block, query a database, or wait on the network
  inside `on_message`. A stalled handler stalls heartbeats and every other session on that
  thread ([GUIDE.md §2](GUIDE.md)).

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

## Run it

Save Step 1 as `acceptor.cfg` next to Step 2's `src/main.rs`, then:

```sh
cargo run
```

The acceptor binds to `127.0.0.1:9876`, loads `TW44` and `BANZAI`, and waits for a FIX client.
Send it a `Logon` naming `56=ISLD` and `49=TW44`, and it answers with one of its own; type a
line and press enter to stop it.

Two things prove this page cannot silently drift from working code:

- **`scripts/stranger-check.sh --from packaged`** and **`--from git --tag v0.1.0`** paste exactly
  the two blocks above into a fresh crate outside this repository — the first builds it against
  the same bytes a `.crate` upload would contain, the second `cargo add`s fixbolt straight from
  GitHub at the released tag, the way `README.md` and the install line above actually tell a
  stranger to depend on it — and both drive a real Logon/Logout through it with a client that
  shares no code with fixbolt (`scripts/stranger-logon.py`) — ADR-0097 exit criterion 7, read
  through ADR-0161 decision 4.
- **[`crates/library/examples/acceptor.rs`](../crates/library/examples/acceptor.rs)** and
  **[`crates/library/examples/shared/order_handler.rs`](../crates/library/examples/shared/order_handler.rs)**
  are the same handler and the same entry point, built and tested inside this workspace by
  [`crates/library/tests/end_to_end.rs`](../crates/library/tests/end_to_end.rs) on every
  `cargo test`. If you are reading this repository's own source rather than a downloaded crate,
  `cargo run --example acceptor -- crates/library/examples/acceptor.cfg 127.0.0.1:9876` runs the
  in-tree copy.

Next: [TUTORIAL.md](TUTORIAL.md) walks through the same code in more detail and shows the
bytes on the wire.
