//! A FIX 4.4 acceptor, end to end, in a page.
//!
//! `DESIGN.md` §7 step 8's *"first end-to-end example"*. Everything it names
//! comes from `fixbolt` and nothing from `fixbolt_engine` or `fixbolt_session`
//! — if that ever stops being true, the facade is missing something.
//!
//! ```text
//! cargo run --example acceptor -- crates/library/examples/acceptor.cfg 127.0.0.1:9876
//! ```
//!
//! The handler it runs is `examples/shared/order_handler.rs`, and
//! `tests/end_to_end.rs` drives **that same file** through a real socket.
//!
//! **`standard` mode**, which blocks when idle and gives the core back. This is
//! an example: an example that pins a core at 100% is one nobody can leave
//! running. `serve_hft` is a one-word change and `DESIGN.md` §9 says what the
//! machine has to look like first.

// `serve` exists only under `standard` on a unix target, so this example does
// too. Non-negotiable 6: the `#[cfg]` is on the item.
#[cfg(all(feature = "standard", unix))]
#[path = "shared/order_handler.rs"]
mod order_handler;

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

#[cfg(not(all(feature = "standard", unix)))]
fn main() {
    eprintln!(
        "this example runs the `standard` mode acceptor, which needs the \
         `standard` feature on a unix target"
    );
}
