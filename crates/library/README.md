# fixbolt

A FIX 4.4 engine you embed, from the application's side.

`DESIGN.md` §3 L4 and §7 step 8. This crate adds **no capability**: every
byte still goes through [`fixbolt_engine`](https://docs.rs/fixbolt-engine) and [`fixbolt_session`](https://docs.rs/fixbolt-session). What it
adds is a place to stand.

## The two things it does

**One crate to depend on.** `serve` lives in `fixbolt_engine`, `Config` in
`fixbolt_session`, `Table` and `Limits` in `fixbolt_engine::presession`,
`Settings` in `fixbolt_engine::settings`, `Observer` and `Admin` in
`fixbolt_engine::observe`. Five paths and two manifest entries for one job.
They are all re-exported here, and **only** the ones an application needs:
`Engine`, `Dispatch`, `Transport`, `wait`, `shard`, `affinity`, `frame` and
`ring` are deliberately absent. Reaching for one of those means naming
`fixbolt-engine` in your own manifest, and that extra line is the pause it
is there to cause.

**A handler that does not have to know the session's job.** `Handler`
receives a message already parsed and answers through `Reply`, which
writes `8`, `9`, `10`, `34`, `49`, `52` and `56` itself and sorts every
field the handler names from the generated tables.

```no_run
use fixbolt::{Answer, Handler, Incoming, Limits, Reply, Settings};

struct Desk;

impl Handler for Desk {
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        if msg.msg_type() != b"D" {
            return reply.silent();
        }
        reply
            .message(b"8")
            .field(37, b"EXEC-1")
            .field(150, b"0")
            .field(39, b"0")
            .send()
    }
}

# // `serve` is `standard` only, so the example that calls it is too — the
# // same `#[cfg]` the re-export carries, applied to the doctest.
# #[cfg(all(feature = "standard", unix))]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let table = Settings::load("acceptor.cfg")?.into_table();

// Everything an operator can do to a running engine comes off this, and it is
// made **before** the engine: `serve` returns only once it has stopped.
let handles = fixbolt::Handles::new();
let admin = handles.admin();
std::thread::spawn(move || {
    // Wire this to whatever your deployment uses to say "shut down".
    admin.shutdown(5_000);
});

let shutdown = fixbolt::serve(
    "0.0.0.0:9876",
    table,
    fixbolt::app(Desk),
    64,
    Limits::new(64, 30_000)?,
    // Every message in and out, one line each, in a file an operator can
    // `grep` during a dispute. `fixbolt::NoLog` if you want none.
    fixbolt::FileLog::open(std::path::Path::new("messages.log"))?,
    handles,
)?;
println!("stopped: {shutdown:?}");
# Ok(())
# }
# #[cfg(not(all(feature = "standard", unix)))]
# fn main() {}
```

## What it costs, and the door that stays open

`[measured 2026-09-05, AMD Ryzen 7 3700X, the DESIGN.md §9 desktop, pass 12
fail 0 unknown 1]` one twelve-field reply through `App::on_message` costs
**1 029 ns**, of which the second parse is **160 ns** and the reply — a
`Template` built, sorted and encoded per message — is **804 ns**. Encoding a
template that was built **once** — `DESIGN.md` D9's shape — costs **238 ns** on
the same box. So this layer is about **3.4× the fast path**, roughly 570 ns
more per reply, and that is a fact about the convenience rather than about
the engine ([ADR-0051](../../docs/decisions/ADR-0051-item-34-is-a-third-of-the-size-it-was-recorded-at.md);
this paragraph said *50×* against a *40 ns* that had no committed benchmark).

For a great many FIX applications a microsecond is nothing. For an `hft`
deployment it is more than the rest of the message costs put together. If
you are the second, implement `fixbolt_session::Application` yourself and
hand *that* to `serve` — the raw seam is not taken away, and
`crates/conformance/src/echo.rs` is a worked example of using it.

[ADR-0041](https://github.com/tmthang86/fixbolt/blob/main/docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md)
is the decision, the measurement and the follow-up it names.

## The rule this crate cannot enforce

`Handler::on_message` runs **on the engine thread**, inline
([ADR-0002](https://github.com/tmthang86/fixbolt/blob/main/docs/decisions/ADR-0002-engine-library-split.md)). A
handler that blocks stops heartbeats, sequence numbers and every other
session on that thread. [docs/GUIDE.md](https://github.com/tmthang86/fixbolt/blob/main/docs/GUIDE.md) §2 is the long version.
