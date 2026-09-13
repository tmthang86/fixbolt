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
//! **Two paths, and `--path` picks which.** They are different measurements and
//! the output names which one it is, because quoting one as the other is the
//! same defect ADR-0013 decision 4 forbids for modes.
//!
//! * `--path admin` — **`TestRequest` out, `Heartbeat` back.** No application
//!   is involved: the session owns `35=1` itself, so `Never::on_message` is
//!   never reached (and says so loudly if it is). This measures read, frame,
//!   session, serialise, write, and nothing else. It needs no corpus, so the
//!   number cannot be contaminated by this tool's own message building — which
//!   is why it is the default and why both gate scripts use it.
//! * `--path app` — **`NewOrderSingle` out, `ExecutionReport` back.** This is
//!   the path `DESIGN.md` §8's table is about, and the one that puts a real
//!   number under its bottom line. It adds parse-of-an-application-message,
//!   dispatch to an application, and template serialise of a 14-field `35=8`.
//!
//! `[measured 2026-09-02]` The gap between the two is what an application costs
//! on this design: see `docs/reference/measured-costs.md`.
//!
//! # Pinning, and why the flag refuses rather than shrugs
//!
//! `DESIGN.md` §9 asks for **pinned threads**, and until 2026-09-02 this binary
//! had none — so no run of it could satisfy §9 however well the box was tuned.
//! `--engine-core` and `--client-core` pin through
//! `fixbolt_engine::affinity`, which reads the mask back off the scheduler
//! rather than trusting `sched_setaffinity`'s return (ADR-0015).
//!
//! That module is behind the `affinity` feature and Linux-only, so a build
//! without it **cannot** pin. In that build the two flags are a hard error, not
//! a no-op: `[measured 2026-08-30]` this crate has already shipped a flag that
//! was accepted, printed its banner and did nothing, because a `cfg` does not
//! reach into a dependency's features (see the `[features]` comment in
//! `Cargo.toml`). A run that quietly did not pin is not a §9 run.
//!
//! **Nothing this binary prints on a general-purpose box is a latency number
//! for publication.** `DESIGN.md` §9 describes a machine with isolated cores,
//! no frequency scaling and pinned threads. The output says so itself, every
//! run, rather than leaving it to whoever pastes it somewhere, and it names
//! which cores it actually got.

//! # The allocation count travels with the figures
//!
//! `benches/alloc.rs` cannot see a binary — it is a bench target in a library
//! crate, and `Engine` and `serve` are generic, which is the same reason
//! `dtruss` and `nm -u` could not see the engine loop either. So this binary
//! counts allocations itself, over exactly the window it times, and **prints
//! the count with every figure it publishes**. A wire-to-wire number taken
//! while something on either thread was in `malloc` is a number about `malloc`.
//!
//! It is armed only for the timed loop: startup renders 22 000 messages into a
//! `Vec<Vec<u8>>` on purpose, and `affinity::Topology` reads `/proc`.
//!
//! # TLS, and why the `tls:` line is read back
//!
//! `[2026-09-13]` step 6a of `docs/plans/2026-09-04-tls.md`. `--tls off|ktls|userspace`,
//! default `off`, needs `--features tls` for anything but `off` and refuses
//! otherwise. `off` is the plain path, unchanged: a blocking `TcpStream` client.
//! `ktls` and `userspace` put a `TlsTransport` on **both** ends — the engine's
//! accepted socket and the client's dialled one — and `userspace` forces
//! `TlsTransport::with_offload(…, false)` on both.
//!
//! **`tls:` is printed from what the engine's transport reports after the first
//! logon** (`Engine::tls_mode`), never from the flag. A `--tls ktls` whose
//! handover fell back is a figure about userspace rustls, and a line echoed
//! from the argument would label it `kernel` — the `--mode standard` lesson of
//! 2026-08-30 from the TLS side. `userspace` leaves the hot-path guarantee
//! (ADR-0005 decision 3), so its `allocs` is printed and not asserted.
//!
//! **And the read-back is refused here, not only downstream.** `[measured
//! 2026-09-13]` step 6b left a fallen-back `ktls` arm to the script that reads
//! `tls:`; the run never reached it, because `assert_eq!(allocs, 0)` fires
//! first and userspace rustls allocates. The run then failed as a hot-path
//! regression rather than as a transport that never took the keys. Every arm's
//! read-back is now checked in [`measure`], before the first sample.
#![allow(unsafe_code)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Allocations since the counter was armed, on **every** thread — which is the
/// point: the engine thread and the client thread are both inside the window.
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
/// Whether [`ALLOCS`] is counting. Relaxed: an off-by-one at the boundary would
/// be one allocation from the surrounding setup, and the assertion is against
/// zero over 20 000 messages.
static ARMED: AtomicBool = AtomicBool::new(false);

struct Counting;

// SAFETY: every method forwards to `System` — a correct allocator — with the
// same pointer, layout and size it was given, and adds nothing but two relaxed
// atomic operations. Identical in shape to the four `benches/alloc.rs` files
// this repository already has, and sound for the same reasons: nothing here
// changes what is returned to the caller, this binary is `publish = false` so
// nothing ships it, and it is **proven by reversal** — see the delivery log of
// `docs/plans/2026-08-30-w2w-and-linux-numbers.md`, where a `to_vec()` put into
// the timed loop takes the count from 0 to 20 000.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static A: Counting = Counting;

use fixbolt_codec::{FieldIndex, Template, TemplateBuilder, Validation, parse_into};
use fixbolt_dict::Fix44;
use fixbolt_engine::dispatch::{ConnId, InlineDispatch};
use fixbolt_engine::transport::{Interest, TcpTransport, TlsMode, Transport};
use fixbolt_engine::wait::{Spin, Waiting, Yield};
use fixbolt_engine::{Acceptor, Engine};
use fixbolt_session::{Application, Config};

/// Whether this build can pin a thread at all.
///
/// A `const` rather than a `cfg` at each use site, so the refusal below is one
/// branch that is always compiled and always read.
const CAN_PIN: bool = cfg!(all(feature = "affinity", target_os = "linux"));

/// Whether this build can run a TLS arm at all — the same shape as [`CAN_PIN`],
/// and refused the same way. `fixbolt_engine::tls::TlsTransport` is Linux-only.
const CAN_TLS: bool = cfg!(all(feature = "tls", target_os = "linux"));

/// What the engine thread's transport reported after the first logon, for the
/// client thread to print. `0` until then; see [`tls_code`].
///
/// A cell rather than a `println!` on the engine thread, so the line has one
/// place in the output and is printed by the thread that then asserts on it.
static TLS_SEEN: AtomicU8 = AtomicU8::new(0);

/// [`TLS_SEEN`]'s encoding. `4` is "logged on, and the connection was gone by
/// the time the engine was asked", which must fail the run rather than print.
const fn tls_code(mode: Option<TlsMode>) -> u8 {
    match mode {
        Some(TlsMode::Plain) => 1,
        Some(TlsMode::Kernel) => 2,
        Some(TlsMode::Userspace) => 3,
        None => 4,
    }
}

/// Which transport carries the session — `--tls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tls {
    /// A plain `TcpTransport` and a blocking `TcpStream` client. The default,
    /// and what every figure published before this flag was measured on.
    Off,
    /// `TlsTransport` on both ends, keys handed to the kernel.
    Ktls,
    /// `TlsTransport` on both ends with the offload refused, so `rustls` stays
    /// on the data path. **Leaves the hot-path guarantee** — ADR-0005 decision 3.
    Userspace,
}

impl Tls {
    const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Ktls => "ktls",
            Self::Userspace => "userspace",
        }
    }

    /// What the **engine** must report for this arm — [`seen_name`]'s spelling,
    /// not the flag's. `--tls ktls` reads back as `tls: kernel`, which is why
    /// the two differ at all, and why `scripts/w2w-baseline.sh` carries the
    /// same mapping in its `want_tls` case.
    const fn wants(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Ktls => "kernel",
            Self::Userspace => "userspace",
        }
    }
}

/// What `tls:` says, from what the engine reported — never from [`Tls`].
const fn seen_name(code: u8) -> &'static str {
    match code {
        1 => "off",
        2 => "kernel",
        3 => "userspace",
        _ => "unknown",
    }
}

/// Which idle strategy the engine thread runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Spin. `DESIGN.md` D8's `hft` half, and this tool's default.
    Hft,
    /// Block on readiness. The engine's default, and not this tool's.
    Standard,
    /// `sched_yield`. **Neither mode**, and it is here to be seen failing both
    /// gates rather than described as failing them.
    Yield,
}

impl Mode {
    const fn name(self) -> &'static str {
        match self {
            Self::Hft => "hft",
            Self::Standard => "standard",
            Self::Yield => "yield",
        }
    }
}

/// Which round trip is timed.
///
/// `DESIGN.md` §8's table describes [`Path::App`]; both gate scripts drive
/// [`Path::Admin`], which is why that is the default — changing it would move
/// every figure those two scripts have ever printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Path {
    /// `TestRequest` -> `Heartbeat`. No application.
    Admin,
    /// `NewOrderSingle` -> `ExecutionReport`. Through an application.
    App,
}

impl Path {
    const fn name(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::App => "app",
        }
    }

    /// The `35=` this path expects to come back.
    const fn expects(self) -> &'static [u8] {
        match self {
            Self::Admin => b"0",
            Self::App => b"8",
        }
    }
}

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
        _hdr: fixbolt_session::Header<'_>,
        _out: &mut [u8],
    ) -> Option<Range<usize>> {
        eprintln!("w2w: the application was reached; this run measures something else");
        None
    }
}

/// A desk that fills whatever it is sent, and the `--path app` half of this
/// tool.
///
/// **The template is built once, here, and patched per message.** That is what
/// `DESIGN.md` §4 D9 decided and what `crates/codec/benches/serialize.rs`
/// prices at 239.1 ns on this box; a `TemplateBuilder` inside `on_message`
/// would be measuring `STATUS.md` open item 34 instead of measuring §8.
///
/// **It is deliberately not `crates/library/examples/shared/order_handler.rs`,
/// which is the only copy of the worked `Desk`.** That one is written against
/// the `fixbolt` library layer's `Handler`/`Reply` API, whose per-message
/// template is exactly open item 34; this one is written against the raw
/// `Application` trait, because §8's rows are about the design and not about
/// the library layer's overhead. The two answer different questions and the
/// numbers are labelled with which.
///
/// Allocates nothing after construction: the index is a field, the one number
/// it renders goes into a stack array, and every other value is borrowed out of
/// the engine's read buffer.
struct Desk {
    exec: Template<32, 512>,
    idx: FieldIndex<64>,
    fills: u32,
}

impl Desk {
    fn new() -> std::io::Result<Self> {
        let exec = TemplateBuilder::<32, 512>::new(b"FIX.4.4")
            .field(35, b"8")
            .field(49, b"ISLD")
            .field(56, b"W2W")
            .slot(34)
            .slot(52)
            .slot(37)
            .slot(17)
            .slot(150)
            .slot(39)
            .slot(11)
            .slot(55)
            .slot(54)
            .slot(38)
            .slot(32)
            .slot(31)
            .slot(151)
            .slot(14)
            .slot(6)
            .build::<Fix44>()
            // `?`, not `expect`: CLAUDE.md §2 rule 7, and a tool is not exempt.
            .map_err(|e| std::io::Error::other(format!("w2w: exec template: {e:?}")))?;
        Ok(Self {
            exec,
            idx: FieldIndex::new(),
            fills: 0,
        })
    }
}

impl Application for Desk {
    fn on_message(
        &mut self,
        msg: &[u8],
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        // `w2w` measures the round trip and does not set `369`: this binary is
        // the latency figure, and a field nothing here reads would only widen
        // the message it times. ADR-0056 -- below the `library` seam the field
        // is the application's to write, and this application declines.
        let (seq, stamp) = (hdr.seq, hdr.stamp);
        // `Validation::NONE`: the session has already validated this message
        // against the dictionary before delivering it, and validating twice
        // would price a check this design does once.
        parse_into::<Fix44, 64>(msg, &mut self.idx, Validation::NONE).ok()?;
        let view = self.idx.view(msg);
        if view.get(35)? != b"D" {
            return None;
        }
        let cl_ord_id = view.get(11)?;
        let qty = view.get(38)?;
        let price = view.get(44)?;

        self.fills += 1;
        let mut seq_buf = [0u8; 10];
        let seq_bytes = render(seq, &mut seq_buf);
        let mut id_buf = [0u8; 10];
        let exec_id = render(self.fills, &mut id_buf);

        self.exec
            .encode_with::<Fix44>(
                out,
                &[
                    (34, seq_bytes),
                    (52, stamp),
                    (37, exec_id),
                    (17, exec_id),
                    (150, b"F"),
                    (39, b"2"),
                    (11, cl_ord_id),
                    (55, view.get(55).unwrap_or(b"")),
                    (54, view.get(54).unwrap_or(b"")),
                    (38, qty),
                    (32, qty),
                    (31, price),
                    (151, b"0"),
                    (14, qty),
                    (6, price),
                ],
                &[],
            )
            .ok()
    }
}

/// ASCII digits of `v`, right-aligned in `buf`. No allocation.
fn render(mut v: u32, buf: &mut [u8; 10]) -> &[u8] {
    if v == 0 {
        buf[9] = b'0';
        return &buf[9..];
    }
    let mut i = 10;
    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + u8::try_from(v % 10).unwrap_or(0);
        v /= 10;
    }
    &buf[i..]
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = arg(&args, "--messages").unwrap_or(20_000);
    let warmup: usize = arg(&args, "--warmup").unwrap_or(2_000);
    let hold_ms: u64 = arg(&args, "--hold-ms").unwrap_or(0);

    let acceptor = Acceptor::bind("127.0.0.1:0")?;
    let addr = acceptor.local_addr()?.to_string();

    // ADR-0013 decision 4: every published figure names its mode. `hft` stays
    // the default here even though `standard` is the engine's, because these
    // numbers exist to describe `hft` and changing the default would silently
    // change every figure this project has published (ADR-0014 decision 8).
    let mode = match arg::<String>(&args, "--mode").as_deref() {
        None | Some("hft") => Mode::Hft,
        Some("standard") => Mode::Standard,
        Some("yield") => Mode::Yield,
        Some(other) => {
            eprintln!("w2w: unknown --mode {other}; expected hft, standard or yield");
            // `Err`, not `Ok(())`. `[2026-09-02]` this branch exited **0** on a
            // typo, printing its complaint to stderr and measuring nothing —
            // the same shape as the `--mode standard` bug in `Cargo.toml`'s
            // comment and as a `--engine-core` that pins nothing. A script that
            // reads the exit code of `w2w --mode standrad` must not see
            // success. `--path` below has always been an error and this is now
            // consistent with it.
            return Err(std::io::Error::other("unknown --mode"));
        }
    };
    let path = match arg::<String>(&args, "--path").as_deref() {
        None | Some("admin") => Path::Admin,
        Some("app") => Path::App,
        Some(other) => {
            eprintln!("w2w: unknown --path {other}; expected admin or app");
            return Err(std::io::Error::other("unknown --path"));
        }
    };
    let tls = match arg::<String>(&args, "--tls").as_deref() {
        None | Some("off") => Tls::Off,
        Some("ktls") => Tls::Ktls,
        Some("userspace") => Tls::Userspace,
        Some(other) => {
            eprintln!("w2w: unknown --tls {other}; expected off, ktls or userspace");
            return Err(std::io::Error::other("unknown --tls"));
        }
    };
    // The refusal, for the reason `--engine-core` has one: a TLS arm that
    // quietly ran plain TCP would print a plain figure under a TLS heading.
    if !CAN_TLS && tls != Tls::Off {
        eprintln!(
            "w2w: --tls {} needs `--features tls`, on Linux.",
            tls.name()
        );
        eprintln!("     This build has no TLS transport. Build with:");
        eprintln!("       cargo build --release -p fixbolt-w2w --features tls");
        return Err(std::io::Error::other("this build has no TLS transport"));
    }
    let engine_core: Option<usize> = arg(&args, "--engine-core");
    let client_core: Option<usize> = arg(&args, "--client-core");

    // The refusal, not a shrug. See the module note: this crate has already
    // shipped a flag that was accepted and did nothing.
    if !CAN_PIN && (engine_core.is_some() || client_core.is_some()) {
        eprintln!("w2w: --engine-core / --client-core need `--features affinity`, on Linux.");
        eprintln!("     This build cannot pin, and a run that quietly did not pin is not a");
        eprintln!("     DESIGN.md §9 run. Build with:");
        eprintln!("       cargo build --release -p fixbolt-w2w --features affinity");
        return Err(std::io::Error::other("this build cannot pin a thread"));
    }
    if engine_core.is_some() && engine_core == client_core {
        eprintln!("w2w: --engine-core and --client-core name the same cpu; the client would");
        eprintln!("     be competing with the engine for it, which is what §9 forbids");
        return Err(std::io::Error::other("engine and client on one core"));
    }
    // `pin_current_thread` proves the thread went where it was told. It does
    // NOT prove the scheduler will keep other work off that core — that is
    // `isolcpus`, and the two are different claims. `[measured 2026-09-02]`
    // pinning to a non-isolated cpu2 on the §9 desktop succeeded and printed
    // `engine-core: cpu2`, which reads as a §9 run and is not one.
    //
    // So this refuses a core `isolcpus` does not name, with the same explicit
    // escape `ShardPlan::allow_unisolated` has: an A/B against an ordinary core
    // is a legitimate experiment, and it is the one this row was measured with.
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    if !args.iter().any(|a| a == "--allow-unisolated") {
        use fixbolt_engine::affinity::{CoreId, Topology};
        let topo = Topology::read().map_err(std::io::Error::other)?;
        for cpu in [engine_core, client_core].into_iter().flatten() {
            if !topo.isolated().contains(&CoreId(cpu)) {
                eprintln!("w2w: cpu{cpu} is not in isolcpus, so the scheduler may put other");
                eprintln!("     work on it and a figure from it is not a DESIGN.md §9 figure.");
                eprintln!("     isolcpus here: {:?}", topo.isolated());
                eprintln!("     Pass --allow-unisolated to measure there on purpose.");
                return Err(std::io::Error::other(
                    "pinned to a core isolcpus does not name",
                ));
            }
        }
    }

    // Printed before anything else, on its own line, because
    // `scripts/check-no-kernel-sleep.sh` and
    // `scripts/check-standard-gives-the-core-back.sh` both read it back to
    // prove they ran the arm they meant to.
    println!("mode: {}", mode.name());
    println!("path: {}", path.name());

    let stop = Arc::new(AtomicBool::new(false));
    let engine_stop = Arc::clone(&stop);

    // The engine thread, in the shape a deployment runs: `Spin` +
    // `InlineDispatch` + `SystemClock`, which is what `TcpAcceptorEngine` names.
    //
    // `--mode` swaps the idle strategy and nothing else, which is what lets one
    // binary serve as both halves of two gates:
    //
    //   * `check-no-kernel-sleep.sh` runs `hft` and requires **`standard`** to
    //     trip it, on a real `poll`/`ppoll`. Non-negotiable 4 has had two
    //     machine checks before this and both were green with a `sleep`
    //     present, so a guard that cannot be shown to go red is worth nothing.
    //   * the `standard` gate runs `standard` and requires `hft` to trip it.
    //
    // `yield` is neither mode and is here to demonstrate it: it must fail both
    // gates. Until this flag existed that claim was only prose.
    // Built before the thread starts, so a template that will not build fails
    // the run rather than the first message.
    let desk = match path {
        Path::Admin => None,
        Path::App => Some(Desk::new()?),
    };
    // The certificate and both configurations are made here, before either
    // thread starts and far outside the timed window: `rcgen` and `rustls`
    // allocate freely, and that is ADR-0005's handshake carve-out.
    #[cfg(all(feature = "tls", target_os = "linux"))]
    let pki = match tls {
        Tls::Off => None,
        Tls::Ktls | Tls::Userspace => Some(tls_arm::Pki::new()?),
    };
    #[cfg(all(feature = "tls", target_os = "linux"))]
    let side = match &pki {
        None => EngineSide::Plain,
        // **The reversal of step 6a lives on this line**: forcing `false` here
        // while `--tls ktls` is asked for must print `tls: userspace`.
        Some(p) => EngineSide::Tls(std::sync::Arc::clone(&p.server), tls == Tls::Ktls),
    };
    #[cfg(not(all(feature = "tls", target_os = "linux")))]
    let side = EngineSide::Plain;
    let body = move || {
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
        match desk {
            None => serve(acceptor, &engine_stop, mode, Never, side),
            Some(d) => serve(acceptor, &engine_stop, mode, d, side),
        }
    };
    let engine = spawn_engine(body, engine_core)?;

    // The client pins itself, from inside the thread that will run, which is
    // ADR-0015's first clause. The main thread is the client.
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    if let Some(cpu) = client_core {
        use fixbolt_engine::affinity::{self, CoreId};
        affinity::pin_current_thread(CoreId(cpu)).map_err(std::io::Error::other)?;
        let on = affinity::running_on().map_err(std::io::Error::other)?;
        println!("client-core: {on}");
    } else {
        println!("client-core: not pinned");
    }
    #[cfg(not(all(feature = "affinity", target_os = "linux")))]
    println!("client-core: not pinned");

    let run = Run {
        path,
        warmup,
        n,
        hold_ms,
        tls,
    };
    let (mut samples, allocs) = match tls {
        Tls::Off => {
            // The plain arm, exactly as it was before `--tls` existed: a
            // blocking `TcpStream` with Nagle off. Changing this client changes
            // every figure this binary has published.
            let sock = TcpStream::connect(&addr)?;
            sock.set_nodelay(true)?;
            measure(sock, &run, &stop, engine)?
        }
        #[cfg(all(feature = "tls", target_os = "linux"))]
        Tls::Ktls | Tls::Userspace => {
            let Some(p) = pki.as_ref() else {
                return Err(std::io::Error::other("w2w: a TLS arm with no certificate"));
            };
            let sock = tls_arm::connect(&addr, p, tls == Tls::Ktls)?;
            measure(sock, &run, &stop, engine)?
        }
        #[cfg(not(all(feature = "tls", target_os = "linux")))]
        Tls::Ktls | Tls::Userspace => {
            // Unreachable: refused above, before any thread started.
            return Err(std::io::Error::other("this build has no TLS transport"));
        }
    };

    samples.sort_unstable();
    let pick = |q: f64| samples[((samples.len() as f64 - 1.0) * q) as usize];
    let what = match path {
        Path::Admin => "TestRequest -> Heartbeat",
        Path::App => "NewOrderSingle -> ExecutionReport",
    };
    println!("w2w: {what}, over kernel TCP on loopback");
    println!("     mode   {:>9}", mode.name());
    println!("     path   {:>9}", path.name());
    println!("     {} samples after {} warmup", samples.len(), warmup);
    println!("     min    {:>9} ns", samples[0]);
    println!("     p50    {:>9} ns", pick(0.50));
    println!("     p99    {:>9} ns", pick(0.99));
    // Phase 1 exit criterion 6 names p99.9 specifically, and it was the one
    // percentile this binary did not print. At the default 20 000 samples it is
    // the mean of nothing — it is one sample, the 19 981st — so the criterion is
    // reported with the sample count beside it and never without.
    println!("     p99.9  {:>9} ns", pick(0.999));
    println!("     max    {:>9} ns", samples[samples.len() - 1]);
    println!("     allocs {allocs:>9}   (both threads, the timed window only)");
    println!();
    // Non-negotiable 1, for this binary. Reported first so the number is
    // readable even when the assertion below ends the run.
    //
    // `off` and `ktls` hold it; `userspace` does not claim it. ADR-0005
    // decision 3 names userspace rustls as the mode that leaves the hot-path
    // guarantee, and `measure` has already refused **every** arm whose engine
    // did not report the transport that arm names — so neither this exemption
    // nor the assertion below can be reached by a run that was on another
    // transport, and `allocs != 0` here can only mean what it says.
    if tls == Tls::Userspace {
        println!("tls userspace: rustls is on the data path, which leaves the hot-path");
        println!("guarantee (ADR-0005 decision 3); `allocs` is printed and not asserted.");
    } else {
        assert_eq!(
            allocs,
            0,
            "w2w: {allocs} allocations inside the timed window over {} messages, \
             --tls {}, the engine reporting tls '{}' — this run measures malloc as \
             well as the engine, and CLAUDE.md §2 non-negotiable 1 says the \
             engine's path has none",
            samples.len(),
            tls.name(),
            seen_name(TLS_SEEN.load(Ordering::Relaxed))
        );
    }
    println!("NOT A LATENCY NUMBER FOR PUBLICATION unless this machine matches");
    println!("DESIGN.md §9 — isolated cores, no frequency scaling, pinned threads.");
    println!("Two of those three are the machine and are read by check-machine.sh;");
    println!("the third is this run, and the `engine-core:`/`client-core:` lines above");
    println!("say whether it got them. `not pinned` means this is not a §9 figure.");
    println!("CLAUDE.md §2 rule 10: a number without its machine is someone else's claim.");
    println!("ADR-0013 decision 4: and a standard figure is not an hft figure.");
    println!("And an app figure is not an admin figure — `path` above says which.");
    Ok(())
}

/// What [`measure`] needs to know about the run, as one argument.
struct Run {
    path: Path,
    warmup: usize,
    n: usize,
    hold_ms: u64,
    tls: Tls,
}

/// The client half: log on, read back which transport the engine got, warm up,
/// time `n` round trips, and stop the engine.
///
/// Generic over [`Wire`] so the plain arm and both TLS arms run **one** loop:
/// for `C = TcpStream` it is the loop this binary had before `--tls`, call for
/// call — `write_all`, then `read` until one whole message.
///
/// Returns the samples, unsorted, and the allocation count over the timed
/// window.
fn measure<C: Wire>(
    mut sock: C,
    run: &Run,
    stop: &AtomicBool,
    engine: std::thread::JoinHandle<()>,
) -> std::io::Result<(Vec<u64>, usize)> {
    let Run {
        path,
        warmup,
        n,
        hold_ms,
        tls,
    } = *run;

    // Logon first, and read the answer, so the timed loop starts on an
    // established session rather than on a handshake.
    write_and_read(&mut sock, &logon(1))?;

    // **Read back, not echoed.** The engine thread stores what its transport
    // reported once `logons()` moved; a Logon answered means that has happened
    // or is about to, on the other thread, so this waits for it — bounded, and
    // outside the timed window.
    let deadline = Instant::now() + Duration::from_secs(10);
    let seen = loop {
        let code = TLS_SEEN.load(Ordering::Relaxed);
        if code != 0 {
            break code;
        }
        if Instant::now() > deadline {
            return Err(std::io::Error::other(
                "w2w: the engine never reported which transport carried the logon",
            ));
        }
        std::thread::yield_now();
    };
    println!("tls: {}", seen_name(seen));
    // **Every arm must have run on the transport it names, `ktls` included, and
    // this is the first thing that judges the run.** It returns before a single
    // sample is taken, so it is necessarily ahead of the `assert_eq!(allocs, 0)`
    // in `main`.
    //
    // `[measured 2026-09-13]` step 6b shipped this as `(Tls::Ktls, 2 | 3)` — a
    // `ktls` arm whose handover fell back was deliberately allowed through, on
    // the reasoning that `tls:` above already says `userspace` and
    // `scripts/w2w-baseline.sh` reads that line. A senior review forced
    // `with_offload(false)` on the engine side and found the script's check
    // unreachable in exactly the case it was written for: the run reached the
    // allocation assertion first and died with `panicked … allocs 600`, exit
    // 101, naming a hot-path regression for a connection that had simply never
    // taken the keys. The script is still the second reader; it is no longer
    // the only one.
    if seen_name(seen) != tls.wants() {
        return Err(std::io::Error::other(format!(
            "w2w: --tls {} requires the engine to report tls '{}', and it \
             reports '{}' — the kernel handover fell back to userspace rustls, \
             or this is not the arm that was asked for. Nothing measured below \
             would be a figure about '{}', so nothing is measured.",
            tls.name(),
            tls.wants(),
            seen_name(seen),
            tls.name(),
        )));
    }

    // **Every message is rendered before the clock starts.** The lesson is
    // already written down: a benchmark that formats inside its own timed loop
    // measures the formatting, and one that replays a single message measures a
    // connection that was dropped at message two
    // (docs/reference/measured-costs.md).
    let total = warmup + n;
    let msgs: Vec<Vec<u8>> = (0..total)
        .map(|i| match path {
            Path::Admin => test_request(2 + i as u32, i),
            Path::App => new_order_single(2 + i as u32, i),
        })
        .collect();

    let mut buf = [0u8; 4096];
    for m in msgs.iter().take(warmup) {
        sock.put(m)?;
        read_one(&mut sock, &mut buf)?;
    }

    let mut samples: Vec<u64> = Vec::with_capacity(n);
    // Armed after `samples` has its capacity, so the one allocation this loop
    // would otherwise be blamed for is outside the window rather than excused
    // inside it.
    ARMED.store(true, Ordering::Relaxed);
    for m in msgs.iter().skip(warmup) {
        let t0 = Instant::now();
        sock.put(m)?;
        let len = read_one(&mut sock, &mut buf)?;
        let ns = t0.elapsed().as_nanos();
        // The reply must be the one this path asks for. A run that measured a
        // Reject, a Logout, or a stale byte must not report a latency for it —
        // and a `--path app` run that quietly got a `35=3` back would otherwise
        // report the session's reject path as an application round trip.
        assert!(
            field(&buf[..len], 35) == Some(path.expects()),
            "w2w: --path {} expected 35={}, got {}",
            path.name(),
            String::from_utf8_lossy(path.expects()),
            String::from_utf8_lossy(&buf[..len])
        );
        // `35=8` alone does not prove an ExecutionReport was built: a template
        // that dropped every slot would still carry it. So the app path checks
        // the two fields that can only be there if the desk parsed the order
        // and patched the template — the ClOrdID it just sent, echoed back, and
        // the ExecType. Outside the timed section, and allocation-free.
        if path == Path::App {
            let reply = &buf[..len];
            assert!(
                field(reply, 11) == field(m, 11),
                "w2w: the ExecutionReport carries ClOrdID {:?}, the order sent {:?}",
                field(reply, 11).map(String::from_utf8_lossy),
                field(m, 11).map(String::from_utf8_lossy)
            );
            assert!(
                field(reply, 150) == Some(b"F"),
                "w2w: 150= is {:?}, so the template did not patch its slots",
                field(reply, 150).map(String::from_utf8_lossy)
            );
        }
        samples.push(u64::try_from(ns).unwrap_or(u64::MAX));
    }

    ARMED.store(false, Ordering::Relaxed);
    let allocs = ALLOCS.load(Ordering::Relaxed);

    // A window in which the engine is up, connected and idle: this is what a
    // syscall trace has to look at to answer open item 15, because an idle spin
    // is exactly where a blocking call would hide.
    if hold_ms > 0 {
        std::thread::sleep(Duration::from_millis(hold_ms));
    }

    stop.store(true, Ordering::Relaxed);
    drop(sock);
    let _ = engine.join();
    Ok((samples, allocs))
}

/// Spawn the engine thread, pinned if a core was named.
///
/// Two definitions, because pinning is a feature. The one that cannot pin never
/// sees a `Some` — `main` refuses the flag before reaching here — so its
/// `_core` is genuinely unused rather than quietly ignored.
#[cfg(all(feature = "affinity", target_os = "linux"))]
fn spawn_engine<F: FnOnce() + Send + 'static>(
    body: F,
    core: Option<usize>,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    use fixbolt_engine::affinity::{CoreId, spawn_pinned};
    match core {
        Some(cpu) => {
            // `spawn_pinned` reads the mask back off the scheduler and returns
            // the core it is actually on, so the line below is observed rather
            // than echoed from the argument (ADR-0015 decision 2).
            let (handle, on) = spawn_pinned("w2w-engine", CoreId(cpu), body)?;
            println!("engine-core: {on}");
            Ok(handle)
        }
        None => {
            println!("engine-core: not pinned");
            std::thread::Builder::new()
                .name("w2w-engine".into())
                .spawn(body)
        }
    }
}

#[cfg(not(all(feature = "affinity", target_os = "linux")))]
fn spawn_engine<F: FnOnce() + Send + 'static>(
    body: F,
    _core: Option<usize>,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    println!("engine-core: not pinned");
    std::thread::Builder::new()
        .name("w2w-engine".into())
        // `?`, not `expect`: CLAUDE.md §2 rule 7 denies unwrap/expect/panic
        // workspace-wide, and a tool is not exempt from a rule the workspace
        // enforces by lint.
        .spawn(body)
}

/// What the engine thread's connections are: plain TCP, or a `TlsTransport`
/// with its server configuration and whether to offload.
enum EngineSide {
    Plain,
    #[cfg(all(feature = "tls", target_os = "linux"))]
    Tls(std::sync::Arc<rustls::ServerConfig>, bool),
}

/// Pick the transport, with the application already chosen.
///
/// `wrap` is `lib.rs`'s `pump` shape: it turns an accepted socket into whatever
/// this engine's connections are. For the plain arm it is `Some`, and the
/// compiler removes it.
fn serve<A: Application>(
    acceptor: Acceptor,
    stop: &AtomicBool,
    mode: Mode,
    app: A,
    side: EngineSide,
) {
    match side {
        EngineSide::Plain => run(acceptor, stop, mode, app, Some::<TcpTransport>),
        #[cfg(all(feature = "tls", target_os = "linux"))]
        EngineSide::Tls(cfg, offload) => run(acceptor, stop, mode, app, move |sock| {
            // The handshake runs inside the transport's `recv`/`send` on the
            // engine thread, before the first logon and so before the timed
            // window — `tls.rs`'s module note on why it is not a pre-session
            // stage. A connection rustls will not even start is dropped.
            rustls::server::UnbufferedServerConnection::new(std::sync::Arc::clone(&cfg))
                .ok()
                .map(|conn| {
                    fixbolt_engine::tls::TlsTransport::with_offload(
                        sock,
                        fixbolt_engine::tls::Handshake::new(conn),
                        offload,
                    )
                })
        }),
    }
}

/// Pick the idle strategy, with the application and the transport already
/// chosen.
///
/// Split out from [`pump`] so that `--mode` and `--path` do not multiply into
/// six copies of the loop: a reversal that also changed the loop would prove
/// nothing about the loop.
fn run<A: Application, T: Transport, F: FnMut(TcpTransport) -> Option<T>>(
    acceptor: Acceptor,
    stop: &AtomicBool,
    mode: Mode,
    app: A,
    wrap: F,
) {
    match mode {
        Mode::Hft => pump(acceptor, stop, Spin, app, wrap),
        Mode::Yield => pump(acceptor, stop, Yield, app, wrap),
        #[cfg(all(feature = "standard", unix))]
        Mode::Standard => pump(
            acceptor,
            stop,
            fixbolt_engine::block::Block::new(16),
            app,
            wrap,
        ),
        #[cfg(not(all(feature = "standard", unix)))]
        Mode::Standard => {
            let _ = wrap;
            eprintln!("w2w: this build has no standard mode");
        }
    }
}

/// The loop `DESIGN.md` D8 describes, over whichever idle strategy and
/// transport were chosen.
///
/// Generic so the two strategies are the *same* loop: a reversal that also
/// changed the loop would prove nothing about the loop.
///
/// **Two loops, and the second is the one that is timed.** The first runs
/// until a session has logged on, then asks the engine which transport carried
/// it and stores the answer in [`TLS_SEEN`] for the client to print. The second
/// is the loop this function was before `--tls`, with no per-turn check added:
/// the question is asked once, and asking it every turn would put a branch in
/// the figure for an answer that never moves.
fn pump<A: Application, W: Waiting, T: Transport, F: FnMut(TcpTransport) -> Option<T>>(
    acceptor: Acceptor,
    stop: &AtomicBool,
    wait: W,
    app: A,
    mut wrap: F,
) {
    let mut engine: Engine<
        T,
        fixbolt_session::Acceptor,
        InlineDispatch<A>,
        fixbolt_engine::clock::SystemClock,
        W,
        fixbolt_engine::journal::Store,
        256,
        4096,
        8192,
    > = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"W2W"),
        InlineDispatch::new(app),
        fixbolt_engine::clock::SystemClock,
        wait,
        8,
    );
    let listener = acceptor.source().map(Interest::readable);
    let extra: &[Interest] = listener.as_slice();
    let mut first: Option<ConnId> = None;
    while !stop.load(Ordering::Relaxed) {
        while let Some(t) = acceptor.accept() {
            if let Some(t) = wrap(t) {
                let id = engine.add(t);
                first.get_or_insert(id);
            }
        }
        if !engine.turn() {
            engine.idle_with(extra);
        }
        if engine.logons() > 0 {
            TLS_SEEN.store(
                tls_code(first.and_then(|id| engine.tls_mode(id))),
                Ordering::Relaxed,
            );
            break;
        }
    }
    while !stop.load(Ordering::Relaxed) {
        while let Some(t) = acceptor.accept() {
            if let Some(t) = wrap(t) {
                let _ = engine.add(t);
            }
        }
        if !engine.turn() {
            engine.idle_with(extra);
        }
    }
}

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1)?.parse().ok()
}

/// The client's end of the wire: the two calls the timed loop makes.
///
/// For `TcpStream` these are `Write::write_all` and `Read::read`, which is what
/// the loop called before this trait existed. The TLS client is
/// `tls_arm::Client`.
trait Wire {
    /// Write all of `msg`, blocking until it is gone.
    fn put(&mut self, msg: &[u8]) -> std::io::Result<()>;
    /// Read what has arrived, blocking until something has. `0` is end of
    /// stream.
    fn get(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
}

impl Wire for TcpStream {
    #[inline]
    fn put(&mut self, msg: &[u8]) -> std::io::Result<()> {
        self.write_all(msg)
    }
    #[inline]
    fn get(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read(buf)
    }
}

fn write_and_read<C: Wire>(sock: &mut C, msg: &[u8]) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    sock.put(msg)?;
    read_one(sock, &mut buf)?;
    Ok(())
}

/// One whole FIX message, by its own `9=` and trailer.
fn read_one<C: Wire>(sock: &mut C, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut at = 0;
    loop {
        let n = sock.get(&mut buf[at..])?;
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

/// The TLS arms' client, certificate and configurations.
///
/// Behind the feature as a whole module, so a build without it names none of
/// `rustls`, `rcgen` or `fixbolt_engine::tls` — `CLAUDE.md` §2 rule 6.
#[cfg(all(feature = "tls", target_os = "linux"))]
mod tls_arm {
    use std::io;
    use std::net::TcpStream;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use fixbolt_engine::tls::{Client as ClientSide, Handshake, TlsTransport};
    use fixbolt_engine::transport::{Io, TcpTransport, Transport};
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};

    /// A self-signed `localhost` certificate, made per run rather than
    /// committed — a committed one expires, and the engine's own TLS tests made
    /// the same choice — and both ends' configurations built from it by the
    /// engine's own `server_config` and `client_config`.
    pub struct Pki {
        pub server: Arc<rustls::ServerConfig>,
        client: Arc<rustls::ClientConfig>,
    }

    impl Pki {
        pub fn new() -> io::Result<Self> {
            let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
                .map_err(|e| io::Error::other(format!("w2w: a self-signed certificate: {e}")))?;
            let cert: CertificateDer<'static> = ck.cert.der().clone();
            let key =
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der()));
            let server = fixbolt_engine::tls::server_config(vec![cert.clone()], key)
                .map_err(|e| io::Error::other(format!("w2w: server TLS config: {e}")))?;
            let client = fixbolt_engine::tls::client_config(vec![cert], None)
                .map_err(|e| io::Error::other(format!("w2w: client TLS config: {e}")))?;
            Ok(Self { server, client })
        }
    }

    /// The client end: step 5a's `TlsTransport<Client>`, handshaken on a
    /// non-blocking socket and then switched to blocking for the timed loop.
    pub struct Client {
        t: TlsTransport<ClientSide>,
    }

    /// Dial `addr`, complete the handshake, and hand the socket back blocking.
    ///
    /// **The handshake is spun to completion before the first `Logon`**, so the
    /// session and the timed loop both start on a connection whose transport is
    /// already decided. `offload` false is `--tls userspace`: the kernel is not
    /// asked, and `TlsTransport::fell_back` is how that end knows it is done —
    /// `is_ready` means *keys in the kernel* and never becomes true there.
    ///
    /// Then `set_nonblocking(false)`, through a duplicate of the descriptor:
    /// `O_NONBLOCK` belongs to the open file description, which the duplicate
    /// shares, so the transport's own socket blocks too. From there the
    /// transport's `recv` blocks in `read(2)` — and an `EIO` from a control
    /// record still goes through `Context::handle_io_error`, surfacing as
    /// `Io::Idle`, which [`super::Wire::get`] reads again.
    pub fn connect(addr: &str, pki: &Pki, offload: bool) -> io::Result<Client> {
        let sock = TcpStream::connect(addr)?;
        sock.set_nodelay(true)?;
        let blocking = sock.try_clone()?;
        let name = ServerName::try_from("localhost")
            .map_err(|e| io::Error::other(format!("w2w: server name: {e}")))?;
        let conn = rustls::client::UnbufferedClientConnection::new(Arc::clone(&pki.client), name)
            .map_err(|e| io::Error::other(format!("w2w: client connection: {e}")))?;
        let mut t = TlsTransport::with_offload(
            TcpTransport::new(sock)?,
            Handshake::<ClientSide>::new(conn),
            offload,
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut scratch = [0u8; 64];
        while !(t.is_ready() || t.fell_back()) {
            match t.recv(&mut scratch) {
                Io::Idle => {}
                Io::Ready(_) => {
                    return Err(io::Error::other(
                        "w2w: application data arrived before this client logged on",
                    ));
                }
                Io::Closed => return Err(io::Error::other("w2w: closed during the TLS handshake")),
                Io::Failed(k) => {
                    return Err(io::Error::new(k, "w2w: the TLS handshake failed"));
                }
            }
            if Instant::now() > deadline {
                return Err(io::Error::other(
                    "w2w: the TLS handshake did not finish in 10 s",
                ));
            }
            std::hint::spin_loop();
        }
        // The client is half of every round trip timed, so a `ktls` arm whose
        // *client* fell back is not a `ktls` figure either. The engine's half is
        // read back and printed as `tls:`; this half is refused here, because
        // nothing downstream reads it.
        if offload && !t.is_ready() {
            return Err(io::Error::other(
                "w2w: --tls ktls, and the client's kernel handover fell back to userspace",
            ));
        }
        blocking.set_nonblocking(false)?;
        Ok(Client { t })
    }

    impl super::Wire for Client {
        fn put(&mut self, mut msg: &[u8]) -> io::Result<()> {
            while !msg.is_empty() {
                match self.t.send(msg) {
                    Io::Ready(n) => msg = msg.get(n..).unwrap_or_default(),
                    // `EINTR` on the blocking write, or rustls not yet able to
                    // take the record. Asked again.
                    Io::Idle => {}
                    Io::Closed => return Err(io::Error::other("peer closed")),
                    Io::Failed(k) => return Err(io::Error::from(k)),
                }
            }
            Ok(())
        }

        fn get(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            loop {
                match self.t.recv(buf) {
                    Io::Ready(n) => return Ok(n),
                    // A control record the kernel handed back as `EIO` and
                    // ktls-core consumed — a session ticket — or `EINTR`.
                    Io::Idle => {}
                    Io::Closed => return Ok(0),
                    Io::Failed(k) => return Err(io::Error::from(k)),
                }
            }
        }
    }
}

/// One field off the wire, with no allocation.
///
/// It used to build its needle with `format!`. That is a heap allocation
/// between two timed samples — not on the engine's hot path, so not
/// non-negotiable 1, but it is this tool perturbing its own next measurement,
/// and the assertions above call it three times per message now instead of one.
/// A stack buffer costs nothing and the client's timed loop now allocates
/// nothing at all.
fn field(wire: &[u8], tag: u32) -> Option<&[u8]> {
    // `\x01` + ten digits of u32::MAX + `=`.
    let mut buf = [0u8; 12];
    buf[0] = 1;
    let mut digits = [0u8; 10];
    let d = render(tag, &mut digits);
    buf[1..=d.len()].copy_from_slice(d);
    buf[d.len() + 1] = b'=';
    let needle = &buf[..d.len() + 2];

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

/// The `NewOrderSingle` of `docs/reference/measured-costs.md`, which is the one
/// `crates/codec/benches/parse.rs` prices at 122.6 ns on this box — so the
/// wire-to-wire figure and the parse row are about the same bytes.
///
/// `44` **Price** is added to it, because the desk refuses an order it cannot
/// price and a silent refusal would show up as a hung read rather than as a
/// wrong number.
fn new_order_single(seq: u32, id: usize) -> Vec<u8> {
    frame(&format!(
        "35=D\x0134={seq}\x0149=W2W\x0152={}\x0156=ISLD\x0111=W{id}\x0121=1\x01\
38=002000.00\x0140=2\x0144=20.15\x0154=1\x0155=INTC\x0160={}\x01",
        stamp(),
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
