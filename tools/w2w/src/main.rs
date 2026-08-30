//! Wire-to-wire: what a message costs from the moment it leaves one process to
//! the moment the answer arrives back.
//!
//! `DESIGN.md` §7 step 7, and the only thing that can produce a number for
//! `DESIGN.md` §8 — every row of which is currently taken from somebody else's
//! literature. It is also the concrete binary that open item 15 needs: the
//! non-negotiable *the engine thread never sleeps in the kernel* has never had a
//! machine check, because `dtruss` is refused by macOS SIP and reading undefined
//! symbols out of an rlib passes even with a `thread::sleep` present — `Engine`
//! and `serve` are generic and are never code-generated into the library.
//! A syscall trace of this binary is what closes that.
//!
//! # What is measured, and what is not
//!
//! **An administrative round trip: `TestRequest` out, `Heartbeat` back.** No
//! application is involved — `Echo::on_message` returns `None` and is never
//! reached, because the session owns `35=1` itself. So this measures read,
//! frame, session, serialise, write, and nothing else.
//!
//! That is deliberate for the first version: it needs no echo application and no
//! corpus, so the number cannot be contaminated by this tool's own message
//! building. An application echo, which is what `DESIGN.md` §8's rows actually
//! describe, comes with the half of the plan that needs a machine matching §9.
//!
//! **Nothing this binary prints on a general-purpose box is a latency number
//! for publication.** `DESIGN.md` §9 describes a machine with isolated cores,
//! no frequency scaling and pinned threads. The output says so itself, every
//! run, rather than leaving it to whoever pastes it somewhere.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nanofix_engine::dispatch::InlineDispatch;
use nanofix_engine::wait::{Park, Spin, Waiting};
use nanofix_engine::{Acceptor, Engine};
use nanofix_session::{Application, Config};

/// The application that is never called.
///
/// `35=1` is one of the seven administrative types the session owns, so this
/// exists only to satisfy the type. If it is ever reached the run is not
/// measuring what this file says it measures, so it says so loudly rather than
/// returning `None` quietly.
struct Never;

impl Application for Never {
    fn on_message(
        &mut self,
        _msg: &[u8],
        _seq: u32,
        _stamp: &[u8],
        _out: &mut [u8],
    ) -> Option<Range<usize>> {
        eprintln!("w2w: the application was reached; this run measures something else");
        None
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = arg(&args, "--messages").unwrap_or(20_000);
    let warmup: usize = arg(&args, "--warmup").unwrap_or(2_000);
    let hold_ms: u64 = arg(&args, "--hold-ms").unwrap_or(0);

    let acceptor = Acceptor::bind("127.0.0.1:0")?;
    let addr = acceptor.local_addr()?.to_string();

    let park = args.iter().any(|a| a == "--park");

    let stop = Arc::new(AtomicBool::new(false));
    let engine_stop = Arc::clone(&stop);

    // The engine thread, in the shape a deployment runs: `Spin` +
    // `InlineDispatch` + `SystemClock`, which is what `TcpAcceptorEngine` names.
    //
    // `--park` swaps `Spin` for `Park`, and it exists for exactly one reason:
    // **`scripts/check-no-kernel-sleep.sh` runs this binary both ways and
    // requires the second one to fail.** Non-negotiable 4 has had two machine
    // checks before this and both were green with a `sleep` present, so a guard
    // here that cannot be shown to go red is worth nothing.
    let engine = std::thread::Builder::new()
        .name("w2w-engine".into())
        .spawn(move || {
            // The tid, so a syscall trace can be attributed to THIS thread and
            // not to the client on the main thread, which blocks on purpose.
            // `/proc/thread-self` resolves to `<pid>/task/<tid>` for the calling
            // thread; no dependency and no `gettid` binding needed.
            #[cfg(target_os = "linux")]
            if let Ok(link) = std::fs::read_link("/proc/thread-self")
                && let Some(tid) = link.to_string_lossy().rsplit('/').next()
            {
                println!("engine-tid: {tid}");
            }
            if park {
                pump(acceptor, &engine_stop, Park);
            } else {
                pump(acceptor, &engine_stop, Spin);
            }
        })
        // `?`, not `expect`: CLAUDE.md §2 rule 7 denies unwrap/expect/panic
        // workspace-wide, and a tool is not exempt from a rule the workspace
        // enforces by lint.
        ?;

    let mut sock = TcpStream::connect(&addr)?;
    sock.set_nodelay(true)?;

    // Logon first, and read the answer, so the timed loop starts on an
    // established session rather than on a handshake.
    write_and_read(&mut sock, &logon(1))?;

    // **Every message is rendered before the clock starts.** The lesson is
    // already written down: a benchmark that formats inside its own timed loop
    // measures the formatting, and one that replays a single message measures a
    // connection that was dropped at message two
    // (docs/reference/measured-costs.md).
    let total = warmup + n;
    let msgs: Vec<Vec<u8>> = (0..total).map(|i| test_request(2 + i as u32, i)).collect();

    let mut buf = [0u8; 4096];
    for m in msgs.iter().take(warmup) {
        sock.write_all(m)?;
        read_one(&mut sock, &mut buf)?;
    }

    let mut samples: Vec<u64> = Vec::with_capacity(n);
    for m in msgs.iter().skip(warmup) {
        let t0 = Instant::now();
        sock.write_all(m)?;
        let len = read_one(&mut sock, &mut buf)?;
        let ns = t0.elapsed().as_nanos();
        // The reply must be a Heartbeat. A run that measured a Logout, or read a
        // stale byte, must not report a latency for it.
        assert!(
            field(&buf[..len], 35) == Some(b"0"),
            "w2w: expected a Heartbeat back, got {}",
            String::from_utf8_lossy(&buf[..len])
        );
        samples.push(u64::try_from(ns).unwrap_or(u64::MAX));
    }

    // A window in which the engine is up, connected and idle: this is what a
    // syscall trace has to look at to answer open item 15, because an idle spin
    // is exactly where a blocking call would hide.
    if hold_ms > 0 {
        std::thread::sleep(Duration::from_millis(hold_ms));
    }

    stop.store(true, Ordering::Relaxed);
    drop(sock);
    let _ = engine.join();

    samples.sort_unstable();
    let pick = |q: f64| samples[((samples.len() as f64 - 1.0) * q) as usize];
    println!("w2w: TestRequest -> Heartbeat, over kernel TCP on loopback");
    println!("     {} samples after {} warmup", samples.len(), warmup);
    println!("     min    {:>9} ns", samples[0]);
    println!("     p50    {:>9} ns", pick(0.50));
    println!("     p99    {:>9} ns", pick(0.99));
    println!("     max    {:>9} ns", samples[samples.len() - 1]);
    println!();
    println!("NOT A LATENCY NUMBER FOR PUBLICATION unless this machine matches");
    println!("DESIGN.md §9 — isolated cores, no frequency scaling, pinned threads.");
    println!("CLAUDE.md §2 rule 10: a number without its machine is someone else's claim.");
    Ok(())
}

/// The loop `DESIGN.md` D8 describes, over whichever idle strategy was chosen.
///
/// Generic so the two strategies are the *same* loop: a reversal that also
/// changed the loop would prove nothing about the loop.
fn pump<W: Waiting>(acceptor: Acceptor, stop: &AtomicBool, wait: W) {
    let mut engine: Engine<
        nanofix_engine::transport::TcpTransport,
        nanofix_session::Acceptor,
        InlineDispatch<Never>,
        nanofix_engine::clock::SystemClock,
        W,
        nanofix_engine::journal::Store,
        256,
        4096,
        8192,
    > = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"W2W"),
        InlineDispatch::new(Never),
        nanofix_engine::clock::SystemClock,
        wait,
        8,
    );
    while !stop.load(Ordering::Relaxed) {
        while let Some(t) = acceptor.accept() {
            let _ = engine.add(t);
        }
        if !engine.turn() {
            engine.idle();
        }
    }
}

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1)?.parse().ok()
}

fn write_and_read(sock: &mut TcpStream, msg: &[u8]) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    sock.write_all(msg)?;
    read_one(sock, &mut buf)?;
    Ok(())
}

/// One whole FIX message, by its own `9=` and trailer.
fn read_one(sock: &mut TcpStream, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut at = 0;
    loop {
        let n = sock.read(&mut buf[at..])?;
        if n == 0 {
            return Err(std::io::Error::other("peer closed"));
        }
        at += n;
        if let Some(end) = whole(&buf[..at]) {
            return Ok(end);
        }
    }
}

fn whole(bytes: &[u8]) -> Option<usize> {
    let at = bytes.windows(3).position(|w| w == b"\x019=")?;
    let digits = &bytes[at + 3..];
    let end = digits.iter().position(|b| *b == 1)?;
    let len: usize = core::str::from_utf8(&digits[..end]).ok()?.parse().ok()?;
    let stop = at + 3 + end + 1 + len;
    if bytes.len() < stop + 4 || bytes.get(stop..stop + 3) != Some(b"10=") {
        return None;
    }
    let k = bytes[stop + 3..].iter().position(|b| *b == 1)?;
    Some(stop + 3 + k + 1)
}

fn field(wire: &[u8], tag: u32) -> Option<&[u8]> {
    let needle = format!("\x01{tag}=").into_bytes();
    let start = if wire.starts_with(&needle[1..]) {
        needle.len() - 1
    } else {
        wire.windows(needle.len()).position(|w| w == needle)? + needle.len()
    };
    let end = wire[start..].iter().position(|&b| b == 1)? + start;
    Some(&wire[start..end])
}

fn logon(seq: u32) -> Vec<u8> {
    frame(&format!(
        "35=A\x0134={seq}\x0149=W2W\x0152={}\x0156=ISLD\x0198=0\x01108=30\x01",
        stamp()
    ))
}

fn test_request(seq: u32, id: usize) -> Vec<u8> {
    frame(&format!(
        "35=1\x0134={seq}\x0149=W2W\x0152={}\x0156=ISLD\x01112=W{id}\x01",
        stamp()
    ))
}

/// `BodyLength` and `CheckSum`, computed rather than guessed.
fn frame(body: &str) -> Vec<u8> {
    let head = format!("8=FIX.4.4\x019={}\x01", body.len());
    let mut out = head.into_bytes();
    out.extend_from_slice(body.as_bytes());
    let sum: u32 = out.iter().map(|b| u32::from(*b)).sum();
    out.extend_from_slice(format!("10={:03}\x01", sum % 256).as_bytes());
    out
}

fn stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86_400;
    let (y, m, d) = civil(days);
    let t = secs % 86_400;
    format!(
        "{y:04}{m:02}{d:02}-{:02}:{:02}:{:02}",
        t / 3600,
        (t % 3600) / 60,
        t % 60
    )
}

/// Days since the Unix epoch to a civil date. Howard Hinnant's algorithm.
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
