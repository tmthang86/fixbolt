//! A FIX 4.4 acceptor over a venue's own dictionary, in a page.
//!
//! ```text
//! cargo run -p fixbolt-example-custom-dictionary -- 127.0.0.1:9877
//! ```
//!
//! It serves one counterparty, `TW44`, as `ISLD`, and runs [`Desk`] — the
//! handler `tests/venue.rs` drives through a real socket. The only difference
//! from `crates/library/examples/acceptor.rs` is the door: `serve_over`, told
//! the encoding `TagValue<Venue, 256>`, where `serve` is always FIX 4.4.
//!
//! **`standard` mode**, which blocks when idle and gives the core back.
//! `serve_hft_over` is the `hft` door, with the same arguments.

#[cfg(all(feature = "standard", unix))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use custom_dictionary::Desk;
    use custom_dictionary::venue::Venue;
    use fixbolt::dict::TagValue;
    use fixbolt::{App, Config, Limits, NoLog, NoRecovery, Table};

    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9877".to_owned());
    let table = Table::new().serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"));
    println!("serving TW44 over the Venue dictionary on {addr}");

    let handles = fixbolt::Handles::new();
    let admin = handles.admin();
    // A line on stdin stops it, as in the FIX 4.4 example.
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
        admin.shutdown(5_000);
    });

    // **`Venue` twice, and the two must be the same type.** The encoding is
    // what the session validates by; the `App`'s last parameter is what the
    // handler reads and writes by. Nothing in the types ties them.
    let shutdown = fixbolt::serve_over::<
        256,
        4096,
        8192,
        1024,
        TagValue<Venue, 256>,
        _,
        fixbolt::Store,
        _,
        _,
    >(
        &addr,
        table,
        App::<Desk, 256, 64, 1024, Venue>::with_sizes(Desk::default()),
        64,
        Limits::new(64, 30_000)?,
        NoRecovery,
        NoLog,
        handles,
    )?;
    println!("stopped: {shutdown:?}");
    Ok(())
}

#[cfg(not(all(feature = "standard", unix)))]
fn main() {
    eprintln!("this example serves in `standard` mode, which this build does not have");
}
