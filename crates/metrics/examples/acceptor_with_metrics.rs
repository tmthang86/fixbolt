//! A FIX 4.4 acceptor with a Prometheus exporter beside it.
//!
//! ```text
//! cargo run -p fixbolt-metrics --example acceptor_with_metrics
//! curl -s 127.0.0.1:9464/metrics
//! curl -s -o /dev/null -w '%{http_code}\n' 127.0.0.1:9464/healthz
//! ```
//!
//! The acceptor listens on `127.0.0.1:9878` for `SenderCompID=TW44`,
//! `TargetCompID=ISLD`, and answers nothing but the session layer. A line on
//! stdin stops it.
//!
//! Two things in the order below are the point of the example:
//!
//! 1. **The handles are made before the engine.** `serve` builds its engine
//!    inside and returns only when it has finished, so the observer the exporter
//!    holds has to exist first — `observe::Handles`, ADR-0054.
//! 2. **The exporter is spawned before `serve`.** Its thread inherits the CPU
//!    affinity of the thread that spawns it; spawned from an engine thread that
//!    has pinned itself, it would share the engine's core.
//!
//! `standard` mode: an example that pins a core at 100% is one nobody can
//! leave running.

// `serve` exists only on a unix target; non-negotiable 6, the `#[cfg]` on the
// item. `standard` comes from this crate's dev-dependency on `fixbolt-engine`.
#[cfg(unix)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::ops::Range;

    use fixbolt_engine::observe::Handles;
    use fixbolt_engine::presession::{Limits, Table};
    use fixbolt_engine::{Application, Config};
    use fixbolt_metrics::Exporter;

    struct Silent;
    impl Application for Silent {
        fn on_message(
            &mut self,
            _: &[u8],
            _: fixbolt_session::Header<'_>,
            _: &mut [u8],
        ) -> Option<Range<usize>> {
            None
        }
    }

    let handles = Handles::new();
    let exporter = Exporter::builder("127.0.0.1:9464".parse()?)
        .engine("acceptor", handles.observer())
        .spawn()?;
    println!("metrics on http://{}/metrics", exporter.local_addr());

    let admin = handles.admin();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
        admin.shutdown(5_000);
    });

    println!("FIX acceptor on 127.0.0.1:9878 — a line on stdin stops it");
    let shutdown = fixbolt_engine::serve(
        "127.0.0.1:9878",
        Table::with_capacity(1).serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
        Silent,
        16,
        Limits::new(64, 30_000)?,
        fixbolt_engine::msglog::NoLog,
        handles,
    )?;
    println!("stopped: {} session(s) ended", shutdown.sessions());
    exporter.stop();
    Ok(())
}

#[cfg(not(unix))]
fn main() {
    eprintln!("`serve` needs a unix target");
}
