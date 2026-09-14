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
//!
//! # Two halves, and pacing
//!
//! `[2026-09-14]` step A3a of `docs/plans/2026-09-04-the-second-linux-desk.md`.
//! With neither `--listen` nor `--connect` this binary is what it was: one
//! process, the engine on one thread and the client on another, over loopback.
//! **No new flag changes a line of that output**, so
//! `scripts/check-no-kernel-sleep.sh`, `scripts/check-standard-gives-the-core-back.sh`
//! and `scripts/w2w-baseline.sh` keep their meaning.
//!
//! * `--listen <addr>` — **the engine half, alone.** Binds `addr` and runs the
//!   same engine thread, `--mode` and `--path` as the combined run. It serves
//!   until, after the first logon, the last connection has closed, then exits.
//!   It prints `mode:`, `path:`, `listening:` (the bound address, so `:0`
//!   works), `engine-core:`, `engine-tid:`, and — after the session ends —
//!   `tls:` read back from the engine as always, and `allocs` for the window
//!   **this process can know**: from the turn after the first logon to the turn
//!   that saw the last connection close, warmup and teardown included, because
//!   the engine half cannot see where the generator's warmup ends. That count
//!   is asserted zero. **It prints no latency figure**: the far end of every
//!   round trip is in another process, and a line says so.
//! * `--connect <addr>` — **the generator half, alone.** Logs on, warms up,
//!   times `--messages` round trips, holds `--hold-ms`, closes. Its table is
//!   headed **"as the counterparty sees it"** — a round trip on this process's
//!   clock, this host's stack and the cable included — and its `allocs` is the
//!   generator thread's over the timed window, asserted zero. It prints no
//!   `mode:` and no `tls:` line: it cannot see the engine, and a line echoed
//!   from a flag is the defect this file already paid for. It needs no
//!   `affinity`, so it builds where pinning does not exist (macOS).
//!
//! **A flag that does not apply to a half is refused, not ignored** — the
//! `Cargo.toml` `[features]` lesson. `--listen` refuses `--client-core`,
//! `--messages`, `--warmup`, `--hold-ms` and `--interval` (no client thread;
//! the count, the hold and the pacing are the generator's) — `--warmup` only
//! until `--wire-timestamps` is beside it, see *Wire timestamps* below. `--connect` refuses
//! `--engine-core` and `--mode` (no engine thread). Both refuse `--tls` other
//! than `off`: the certificate is made per process, and two halves are two
//! processes. `--listen` with `--connect` is refused. A new flag with no value,
//! or one that does not parse, is refused. See [`half_of`] and its tests.
//!
//! **Two processes can disagree about `--path`, and one process could not.**
//! `--connect --path app` against `--listen --path admin` gets no reply at all,
//! so the generator bounds every read by [`REPLY_TIMEOUT`] and fails naming
//! the likely cause, and the engine half fails if [`ListenNever`] was reached. The
//! other way round is not caught by the engine half — its `Desk` is simply
//! never called — and needs no catching: the generator's `35=` check holds, and
//! each process's `path:` line says truthfully what it ran.
//!
//! **`--interval <us>`** (combined run and `--connect`): each send waits until
//! `interval` µs after the **previous send**, by spinning on the client thread
//! against `Instant` — no `sleep`, no `nanosleep`, no `clock_nanosleep`, so the
//! generator core is **burned by design**. Warmup is paced the same way. A send
//! whose previous round trip outlasted the interval goes at once, with no
//! catch-up burst, and is counted and printed as late. `0`, the default, waits
//! for nothing and prints no `interval` line. The wait is taken before `t0`, so
//! it is never inside a sample. See [`Pacer`].
//!
//! # The journal and the message log, each a type of its own
//!
//! `[2026-09-14]` step A4 of `docs/plans/2026-09-04-the-second-linux-desk.md`.
//! `--journal mem|file-async` (default `mem`) and `--log none|file` (default
//! `none`) apply wherever the engine half runs — the combined run and
//! `--listen`; `--connect` refuses both, for the reason it refuses `--mode`:
//! there is no engine thread in that process to read either choice back from.
//!
//! **With neither flag, the engine is the engine this tool timed before the
//! flags existed** — `Engine<…, Store, 256, 4096, 8192>` with the default
//! `NoLog`, type for type. `DESIGN.md` §8's round-trip rows are taken with the
//! no-flag binary, and a tool that put a runtime `match` into every journal
//! call, or an `Option` check into every log call, to offer an option would
//! change the figure it exists to produce (`CLAUDE.md` §2 rule 10). So each
//! choice is a type, not a value: [`serve_chosen`] matches the two flags
//! **once**, on the engine thread before its first turn, and calls the
//! generic [`serve`] with the journal source ([`FreshStore`] or [`OneFile`])
//! and the log (`NoLog` or `FileLog`) as type parameters. Four monomorphised
//! engines rather than one engine with a branch in it; the three flagged ones
//! are exactly as branch-free as the default, which is what boot B's row B5
//! needs of them too.
//!
//! **Opened once, outside the timed window, the same way the TLS certificate
//! and the `Desk` template already are.** `--journal file-async` opens one
//! [`FileStore`] — a [`fixbolt_engine::journal::FileJournal`] with
//! `Durability::Async`, carrying the same [`fixbolt_engine::journal::SLOTS`]
//! and [`fixbolt_engine::journal::SLOT_LEN`] as the default `Store`, so boot
//! B's row B5 changes one variable — in a file under `std::env::temp_dir()`,
//! named by this process's id so two runs never collide, and removed when
//! the run ends; `--log file` opens one [`fixbolt_engine::msglog::FileLog`]
//! the same way. A temp directory that refuses a file fails the run before
//! the engine thread starts.
//!
//! **`--journal file-async` only ever has one file to hand out.** [`OneFile`]
//! gives it to the first connection and refuses every connection after — which
//! is every connection this tool's module note above says it ever serves.
//! [`FreshStore`] builds a `Store` per connection, the `J::default()` that
//! `Engine::add` builds. The allocation count over the timed window is
//! asserted zero exactly as it always was; a `--journal` or `--log` run that
//! allocated would fail that assertion loudly rather than quietly publish a
//! number about `malloc`.
//!
//! # Wire timestamps: NIC in to NIC out, on one clock
//!
//! `[2026-09-14]` step A3b of the same plan. `--wire-timestamps --nic <ifname>
//! --observer-core <cpu>`, Linux only, wherever the engine runs (the combined
//! run and `--listen`; `--connect` refuses all three). Both stamps of a round
//! trip are taken by **this** machine's NIC, on its one PHC, so the acceptor's
//! wire-in → wire-out figure needs no clock sync with the generator.
//!
//! * **RX** — an *observer* thread, pinned to `--observer-core`, reads an
//!   `AF_PACKET` `SOCK_DGRAM` tap on `--nic` with `SO_TIMESTAMPING =
//!   RX_HARDWARE | RAW_HARDWARE`, a classic-BPF filter to the engine's port and
//!   `PACKET_IGNORE_OUTGOING`. **`recvmsg`, not a `PACKET_RX_RING` with
//!   `PACKET_TIMESTAMP`**: the ring's `tpacket_rcv` fills `tp_sec` from a
//!   software stamp or `ktime_get_real` whenever the hardware stamp is absent
//!   (`net/packet/af_packet.c`, "Always timestamp"), so a missing hardware stamp
//!   would arrive looking like a present one. `recvmsg`'s `scm_timestamping`
//!   leaves `ts[2]` zero instead, which is the only form the plan's
//!   `hw-rx-missing` count can be built on.
//! * **TX** — `SO_TIMESTAMPING = TX_HARDWARE | RAW_HARDWARE | OPT_ID |
//!   OPT_ID_TCP | OPT_TSONLY` on the accepted socket, **before `engine.add`**,
//!   and the observer reads that socket's error queue. **The engine thread makes
//!   neither call.** Its accept path hands the descriptor across an atomic and
//!   spins (no syscall) until the observer has `dup`ed it, set the option and
//!   read the peer's port ([`wire::Handoff`]). The `dup` is what lets the
//!   observer outlive the engine's own descriptor without reading a reused one.
//!   The observer **claims** the offer before it touches the descriptor, and
//!   the engine's 2 s timeout fires only on an unclaimed offer, so the engine
//!   never drops the socket while the observer is using its descriptor.
//!   Only the first connection is stamped; later ones are counted and printed.
//! * **Pairing** — `pair.rs`, pure, by the TCP byte stream: see its module note.
//!   A `ts[2]` of zero is counted into `hw-rx-missing` / `hw-tx-missing` and
//!   printed, never replaced.
//! * **The NIC** — `SIOCSHWTSTAMP` (`tx_type ON`, `rx_filter ALL`), read back
//!   through the ioctl's own result and a `SIOCGHWTSTAMP`, refused unless both
//!   say so, and the previous configuration is put back when the run ends —
//!   a run ended by SIGINT or SIGTERM included: with the flag both are caught
//!   and end the run through its normal path, `Drop` and all ([`signal`]); a
//!   second one kills at once. The read-back is not trusted as evidence (the
//!   plan's `igb` trap); only a sample's `ts[2] != 0` is.
//! * **`lo`, decided**: a loopback device (`IFF_LOOPBACK`) has no hardware
//!   clock. `SIOCSHWTSTAMP` is **not attempted** on it — it would fail with
//!   `EOPNOTSUPP`, and only after asking for `CAP_NET_ADMIN` — and a line says
//!   so. The tap still runs, so every request is still counted: the run prints
//!   `hw-rx-missing` and `hw-tx-missing` equal to the number of requests and
//!   **no wire column**. It is a run of the plumbing, not of a figure.
//! * **Mode, decided (Sửa 2, Điều 3)**: `hft` is the mode with wire figures.
//!   `--mode standard` on a NIC that is not loopback is **refused**, before
//!   anything needs a capability: a hardware TX stamp waiting in the engine
//!   socket's error queue raises `POLLERR`, which wakes a `standard` engine
//!   asleep in `poll` and keeps waking it until the observer reads the stamp —
//!   the instrument would make it spin
//!   (`docs/reference/a-transmit-timestamp-wakes-a-blocking-engine.md`).
//!   `standard` on loopback runs, so the gate scripts can use it.
//! * **Window**: the combined run's is the `--messages` timed requests.
//!   `--listen`'s is every request after the logon but the first
//!   `--warmup <n>` — which `--listen` accepts **only** beside
//!   `--wire-timestamps`, and which the generator should be given the same
//!   number as; absent is `0`, and the label says the warmup is included.
//! * **Capabilities, and what fails loudly.** The tap needs `CAP_NET_RAW`; a
//!   hardware NIC also needs `CAP_NET_ADMIN` for `SIOCSHWTSTAMP`
//!   (`net/core/dev_ioctl.c`). Refused with the `setcap` line when missing. **A
//!   process exec'd under `strace` or `gdb` by an unprivileged tracer is not
//!   given file capabilities** (`security/commoncap.c`,
//!   `LSM_UNSAFE_PTRACE`), so it is refused the same way — never run on
//!   without the tap. The gate scripts therefore run their `W2W_EXTRA` arm
//!   inside a user namespace (`unshare -Urn`), where `w2w` holds `CAP_NET_RAW`
//!   over that namespace's own `lo` with no `setcap` and no `sudo`.
//! * **Printed**: `wire p50/p99/p99.9` over the requests that had both stamps,
//!   beside the counts, the error-queue entries seen, the tap's drops, and
//!   `allocs` — which, being counted on every thread, includes the observer's,
//!   and says so: its label names every thread counted ([`counted_threads`]).
//!   Any tap drop or buffer overflow withholds the wire column: a request the
//!   tap never recorded would put its reply against its neighbour.
#![allow(unsafe_code)]
// Two kinds of `unsafe` live here, each with its own SAFETY note naming what
// proves it: the counting allocator below, and — Linux only, `mod wire` and
// `mod signal` — the FFI calls of `--wire-timestamps` (`socket`, `setsockopt`,
// `ioctl`, `bind`, `fcntl`, `getpeername`, `recvmsg` and the `CMSG_*` walk,
// and `sigaction` for SIGINT and SIGTERM), which have no safe spelling in `std`.
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

#[cfg(target_os = "linux")]
mod pair;

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
use fixbolt_engine::msglog::MessageLog;
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

/// Which resend store the engine half keeps — `--journal`. `DESIGN.md` D7's
/// table, brought to this tool for step A4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JournalKind {
    /// `fixbolt_engine::journal::Store` (a `MemJournal`), the journal this
    /// tool's engine has always had. Nothing survives a restart, and nothing a
    /// run of this tool does ever needs it to.
    Mem,
    /// [`FileStore`] — `FileJournal` with `Durability::Async`, sized with the
    /// same `SLOTS` and `SLOT_LEN` as `Mem`'s `Store` so boot B's row B5
    /// changes one variable — in a file under `std::env::temp_dir()` that
    /// this run removes when it ends.
    FileAsync,
}

impl JournalKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Mem => "mem",
            Self::FileAsync => "file-async",
        }
    }
}

/// Which message log the engine half keeps — `--log`. `msglog.rs`'s module
/// doc names the cost; this is where boot B's row B5 times it both-threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogKind {
    /// `fixbolt_engine::msglog::NoLog`, the engine's own default and this
    /// tool's: the log hook folds away at compile time.
    None,
    /// `FileLog`, in a file under `std::env::temp_dir()` that this run removes
    /// when it ends.
    File,
}

impl LogKind {
    const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::File => "file",
        }
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

/// Whether [`ListenNever`] was reached. Read only by `--listen`, which has no
/// reply check of its own: in the combined run the client's `35=` assertion
/// already fails such a run, but an engine half whose generator ran `--path
/// app` against `--path admin` would otherwise end cleanly having served
/// nothing.
static APP_REACHED: AtomicBool = AtomicBool::new(false);

/// `--listen --path admin`'s [`Never`]: the same, and it records that it was
/// reached in [`APP_REACHED`].
///
/// **A type of its own so that [`Never`] stays the application the combined
/// run was timed with.** `Never::on_message` is inlined into the session's
/// `judge` on the no-flag admin path — the path `DESIGN.md` §8's round-trip
/// rows are timed on — and the store, written there, changed that function's
/// code. `[measured 2026-09-14]` `nm -S` of `judge::<…InlineDispatch<w2w::Never>…>`
/// in a `--features affinity,tls` release build, and `objdump -d` of it
/// against 25e54dc's: 0x238e bytes there; 0x2395 with the store in `Never` (one
/// `movb` more, inside the block that runs only when the application is
/// reached); 0x2393 with the store moved into an `#[inline(never)]` function
/// (one `call` more, same place); and with `#[cold]` added to that function,
/// LLVM moved the whole reached-block out of line and flipped the branch in
/// front of it (`je` → `jne`). Only leaving `Never` as it was leaves `judge` as
/// it was; `--listen` has had its own engine instance since it existed, and
/// now has its own application type too.
struct ListenNever;

impl Application for ListenNever {
    fn on_message(
        &mut self,
        msg: &[u8],
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        APP_REACHED.store(true, Ordering::Relaxed);
        Never.on_message(msg, hdr, out)
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

/// Which half of the measurement this process runs.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Half {
    /// Neither `--listen` nor `--connect`: engine and client in one process,
    /// over loopback. What this binary was before the two flags, line for line.
    Both,
    /// `--listen <addr>`: the engine thread only.
    Listen(String),
    /// `--connect <addr>`: the client thread only.
    Connect(String),
}

/// What `--listen` refuses, and why. There is no client thread in that
/// process, and the count, the hold and the pacing all belong to the one that
/// sends.
const LISTEN_REFUSES: &[(&str, &str)] = &[
    (
        "--client-core",
        "this process has no client thread; pin the generator in the --connect process",
    ),
    (
        "--messages",
        "the engine half serves until its peer closes; the count belongs to --connect",
    ),
    (
        "--warmup",
        "the engine half serves until its peer closes; the warmup belongs to --connect \
         (with --wire-timestamps it names how many requests after the logon to leave out \
         of the wire figures)",
    ),
    (
        "--hold-ms",
        "the idle window is held by the --connect process, which owns the connection",
    ),
    (
        "--interval",
        "pacing is done by the sender; pass it to the --connect process",
    ),
];

/// What `--connect` refuses, and why. There is no engine thread in that
/// process, and it cannot see which idle strategy the other one ran.
const CONNECT_REFUSES: &[(&str, &str)] = &[
    (
        "--engine-core",
        "this process has no engine thread; pin it in the --listen process",
    ),
    (
        "--mode",
        "the engine's idle strategy is chosen in the --listen process, and this process \
         cannot read back which one ran",
    ),
    (
        "--journal",
        "the engine's resend store is chosen in the --listen process; this process has \
         no engine and nothing to journal",
    ),
    (
        "--log",
        "the engine's message log is chosen in the --listen process; this process has \
         no engine and nothing to log",
    ),
    (
        "--wire-timestamps",
        "the stamps are taken on the engine's NIC, by the process that owns the engine's \
         socket; pass it to the --listen process",
    ),
    (
        "--nic",
        "it names the engine's NIC for --wire-timestamps; pass it to the --listen process",
    ),
    (
        "--observer-core",
        "the observer thread reads the engine's socket; pass it to the --listen process",
    ),
];

/// `--mode standard --wire-timestamps` on a NIC that is not loopback: refused.
///
/// Sửa 2 of `docs/plans/2026-09-04-the-second-linux-desk.md`, Điều 3. A TX
/// timestamp waiting in the engine socket's error queue makes `tcp_poll` report
/// `POLLERR`, which `poll(2)` sets whatever `events` asked for; a `standard`
/// engine asleep in `poll` is woken, reads `EAGAIN`, and is woken again at once
/// until the observer has read the stamp — it spins, which is non-negotiable 4's
/// second half broken by the instrument. Loopback never queues a hardware TX
/// stamp, so `standard` on `lo` runs and the gate scripts can use it. `hft`
/// never waits in `poll`, and `yield` never calls it.
/// `docs/reference/a-transmit-timestamp-wakes-a-blocking-engine.md`.
// Called by the Linux `mod wire` and by the tests; off Linux `wire_of` refuses
// the flag before anything could ask.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn blocking_mode_on_hardware(mode: Mode, nic: &str, loopback: bool) -> Result<(), String> {
    if mode == Mode::Standard && !loopback {
        return Err(format!(
            "--mode standard --wire-timestamps on {nic} is refused: {nic} is not loopback, and a \
             hardware TX timestamp waiting in the engine socket's error queue raises POLLERR, \
             which poll(2) reports whatever it was asked for — a standard engine would be woken \
             and spin until the observer read it, so its figure would be of an engine that \
             spins. Wire figures on a hardware NIC are hft only; measure standard there without \
             --wire-timestamps (docs/reference/a-transmit-timestamp-wakes-a-blocking-engine.md)"
        ));
    }
    Ok(())
}

/// `--wire-timestamps`, as parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WireArgs {
    /// `--nic`: the interface both stamps are taken on.
    nic: String,
    /// `--observer-core`: where the observer thread spins.
    observer_core: usize,
}

/// `--wire-timestamps` and its two companions, with every refusal they owe.
///
/// `half_of` has already refused all three under `--connect`. A companion
/// without the flag is refused rather than ignored, and so is the flag without
/// either companion: a spinning observer with no core could land on the
/// engine's, and a tap with no interface has nothing to read.
fn wire_of(args: &[String]) -> Result<Option<WireArgs>, String> {
    let on = present(args, "--wire-timestamps");
    let nic: Option<String> = value_of(args, "--nic")?;
    let core: Option<usize> = value_of(args, "--observer-core")?;
    if !on {
        return match (nic, core) {
            (None, None) => Ok(None),
            _ => Err(
                "--nic and --observer-core belong to --wire-timestamps, which was not \
                      given"
                    .to_string(),
            ),
        };
    }
    if !cfg!(target_os = "linux") {
        return Err(
            "--wire-timestamps is Linux-only: AF_PACKET, SO_TIMESTAMPING and \
                    SIOCSHWTSTAMP do not exist here"
                .to_string(),
        );
    }
    let nic = nic.ok_or("--wire-timestamps needs --nic <ifname>")?;
    let observer_core = core.ok_or(
        "--wire-timestamps needs --observer-core <cpu>: the observer spins, and an \
         unpinned spinner may land on the engine's core",
    )?;
    Ok(Some(WireArgs { nic, observer_core }))
}

/// Which half this process is, with every refusal a half owes.
///
/// Says nothing at all when neither `--listen` nor `--connect` is present, so
/// a run with no new flag reaches `main`'s older refusals exactly as before.
fn half_of(args: &[String]) -> Result<Half, String> {
    let listen: Option<String> = value_of(args, "--listen")?;
    let connect: Option<String> = value_of(args, "--connect")?;
    let (half, name, refuses) = match (listen, connect) {
        (None, None) => return Ok(Half::Both),
        (Some(_), Some(_)) => {
            return Err(
                "--listen and --connect are the two halves of one measurement; \
                 one process runs one of them"
                    .to_string(),
            );
        }
        (Some(a), None) => (Half::Listen(a), "--listen", LISTEN_REFUSES),
        (None, Some(a)) => (Half::Connect(a), "--connect", CONNECT_REFUSES),
    };
    // `--listen --warmup <n>` is the one exception, and only with
    // `--wire-timestamps` (Sửa 2 of the plan, Q11): the engine half then leaves
    // the first `n` requests after the logon out of its wire window, so a
    // generator's cold warmup is not the p99.9. Without the flag it has
    // nothing to leave out of, and is refused as before.
    let wire_warmup = matches!(half, Half::Listen(_)) && present(args, "--wire-timestamps");
    if let Some((flag, why)) = refuses
        .iter()
        .find(|(f, _)| present(args, f) && !(wire_warmup && *f == "--warmup"))
    {
        return Err(format!("{flag} does not apply to {name}: {why}"));
    }
    // Not only the arms this build can run: a split TLS run is refused on
    // every build, so the refusal cannot depend on which features were on.
    if let Some(t) = arg::<String>(args, "--tls")
        && t != "off"
    {
        return Err(format!(
            "--tls {t} is not available to {name}: the self-signed certificate is made \
             per process, and the two halves are two processes"
        ));
    }
    Ok(half)
}

/// `--interval`, in microseconds. Absent is `0`; present without a number is
/// refused rather than read as `0`.
fn interval_of(args: &[String]) -> Result<u64, String> {
    Ok(value_of(args, "--interval")?.unwrap_or(0))
}

/// Whether `name` appears among the arguments at all.
fn present(args: &[String], name: &str) -> bool {
    args.iter().skip(1).any(|a| a == name)
}

/// A new flag's value: `None` if the flag is absent, refused if it is present
/// with no value, with another flag where the value should be, or with one
/// that does not parse. [`arg`] turns all three into "absent", which is why the
/// older flags keep it and the new ones do not.
fn value_of<T: std::str::FromStr>(args: &[String], name: &str) -> Result<Option<T>, String> {
    let Some(i) = args.iter().skip(1).position(|a| a == name) else {
        return Ok(None);
    };
    match args.get(i + 2) {
        Some(v) if !v.starts_with("--") => v
            .parse()
            .map(Some)
            .map_err(|_| format!("{name} {v}: not a valid value")),
        _ => Err(format!("{name} needs a value")),
    }
}

/// `--interval`: spin until one interval after the previous send.
///
/// **Spins, and never sleeps.** `Instant::now` is `clock_gettime(CLOCK_MONOTONIC)`
/// through the vDSO on Linux and `mach_absolute_time` on macOS — no syscall —
/// and the loop between readings is `spin_loop`. The generator's core is burned
/// for the whole run by design: a `nanosleep` would put this thread's wake-up
/// jitter, and a kernel exit, into the pacing of every message. Proven by the
/// `strace -f` gate of step A3a, not by this comment.
struct Pacer {
    every: Option<Duration>,
    next: Option<Instant>,
}

impl Pacer {
    const fn new(interval_us: u64) -> Self {
        Self {
            every: if interval_us == 0 {
                None
            } else {
                Some(Duration::from_micros(interval_us))
            },
            next: None,
        }
    }

    /// Wait for this send to be due. `true` if it was already overdue, which
    /// the caller counts as late — and then it goes at once, with no catch-up.
    #[inline]
    fn wait(&self) -> bool {
        let Some(at) = self.next else {
            return false;
        };
        let mut now = Instant::now();
        if now > at {
            return true;
        }
        while now < at {
            std::hint::spin_loop();
            now = Instant::now();
        }
        false
    }

    /// A send went at `t0`; the next is due one interval later.
    #[inline]
    fn sent(&mut self, t0: Instant) {
        if let Some(every) = self.every {
            self.next = t0.checked_add(every);
        }
    }

    /// [`Pacer::sent`] at now — reading the clock only when pacing, so an
    /// unpaced warmup reads no more clocks than it did before `--interval`.
    #[inline]
    fn sent_now(&mut self) {
        if self.every.is_some() {
            self.sent(Instant::now());
        }
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // The two halves' refusals first. With no new flag they return
    // `(Half::Both, 0)` and print nothing, so everything below runs as it did.
    let (half, interval_us, wire) = match half_of(&args)
        .and_then(|h| Ok((h, interval_of(&args)?)))
        .and_then(|(h, i)| Ok((h, i, wire_of(&args)?)))
    {
        Ok(v) => v,
        Err(why) => {
            eprintln!("w2w: {why}");
            return Err(std::io::Error::other(why));
        }
    };
    let n: usize = arg(&args, "--messages").unwrap_or(20_000);
    // `--listen` reaches here with `--warmup` only beside `--wire-timestamps`
    // (`half_of`), where it is the number of requests after the logon left out
    // of the wire window: absent is `0`, and a value that does not parse is
    // refused rather than defaulted, as for every flag A3a added.
    let warmup: usize = match &half {
        Half::Listen(_) => match value_of(&args, "--warmup") {
            Ok(v) => v.unwrap_or(0),
            Err(why) => {
                eprintln!("w2w: {why}");
                return Err(std::io::Error::other(why));
            }
        },
        _ => arg(&args, "--warmup").unwrap_or(2_000),
    };
    let hold_ms: u64 = arg(&args, "--hold-ms").unwrap_or(0);

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
    let journal = match arg::<String>(&args, "--journal").as_deref() {
        None | Some("mem") => JournalKind::Mem,
        Some("file-async") => JournalKind::FileAsync,
        Some(other) => {
            eprintln!("w2w: unknown --journal {other}; expected mem or file-async");
            return Err(std::io::Error::other("unknown --journal"));
        }
    };
    let log = match arg::<String>(&args, "--log").as_deref() {
        None | Some("none") => LogKind::None,
        Some("file") => LogKind::File,
        Some(other) => {
            eprintln!("w2w: unknown --log {other}; expected none or file");
            return Err(std::io::Error::other("unknown --log"));
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
    // The observer spins, so it is pinned or it does not run — and never where
    // the engine or the client is. It is **not** held to `isolcpus` below: it
    // times nothing, both stamps are taken by the NIC before it reads them, and
    // the plan puts it on an ordinary core. What it must not do is share one.
    if let Some(w) = &wire {
        if !CAN_PIN {
            eprintln!("w2w: --observer-core needs `--features affinity`, on Linux: an observer");
            eprintln!("     that spins unpinned may take the engine's core. Build with:");
            eprintln!("       cargo build --release -p fixbolt-w2w --features affinity");
            return Err(std::io::Error::other("this build cannot pin the observer"));
        }
        if [engine_core, client_core].contains(&Some(w.observer_core)) {
            eprintln!(
                "w2w: --observer-core {} is the engine's or the client's core; the observer",
                w.observer_core
            );
            eprintln!("     spins, and would compete with the thread it is observing");
            return Err(std::io::Error::other("observer on a measured core"));
        }
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

    let run = Run {
        path,
        warmup,
        n,
        hold_ms,
        tls,
        interval_us,
    };
    let ran = match half {
        Half::Both => both_halves(
            mode,
            run,
            engine_core,
            client_core,
            journal,
            log,
            wire.as_ref(),
        ),
        // `half_of` has refused `--client-core` here, and `--engine-core`,
        // `--mode` and `--wire-timestamps` for the generator, so neither
        // function is handed a core, a mode or a tap it would have to ignore.
        Half::Listen(addr) => {
            engine_half(&addr, mode, run, engine_core, journal, log, wire.as_ref())
        }
        // `half_of` has refused `--journal` and `--log` for `--connect`, so
        // `generator_half` never receives a choice it has no engine to act on.
        Half::Connect(addr) => generator_half(&addr, run, client_core),
    };
    // Only `--wire-timestamps` installs a handler (`signal`). By here every
    // thread that run started has been joined or has returned, and every
    // `Drop` has run — the observer's, and `HwConfig`'s, whose `restored`
    // line is above on a hardware NIC. Whatever the half returned, a caught
    // signal is what ended it, and it says so rather than dying by it.
    if let Some(sig) = signal::caught() {
        let name = signal::name(sig);
        eprintln!(
            "w2w: stopped by {name}: the engine was stopped, the observer joined, and the NIC's \
             timestamping configuration put back if this run changed it; nothing is reported"
        );
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            format!("w2w: stopped by {name}"),
        ));
    }
    ran
}

/// The combined run: engine and client in one process, over loopback. Every
/// line it prints is the line this binary printed before `--listen` existed.
fn both_halves(
    mode: Mode,
    run: Run,
    engine_core: Option<usize>,
    client_core: Option<usize>,
    journal: JournalKind,
    log: LogKind,
    wire: Option<&WireArgs>,
) -> std::io::Result<()> {
    let Run { path, tls, .. } = run;
    let acceptor = Acceptor::bind("127.0.0.1:0")?;
    let local = acceptor.local_addr()?;
    let addr = local.to_string();

    // Printed before anything else, on its own line, because
    // `scripts/check-no-kernel-sleep.sh` and
    // `scripts/check-standard-gives-the-core-back.sh` both read it back to
    // prove they ran the arm they meant to.
    println!("mode: {}", mode.name());
    println!("path: {}", path.name());
    // Only when asked for, so a run with neither flag prints what it always
    // did — see the module note "The journal and the message log".
    if journal != JournalKind::Mem {
        println!("journal: {}", journal.name());
    }
    if log != LogKind::None {
        println!("log: {}", log.name());
    }

    // Opened before the engine thread starts and outside the timed window, the
    // same carve-out the certificate and the `Desk` template below get: a temp
    // directory that refuses a file fails the run here rather than at the
    // first message. `_cleanup` removes the files when this function returns,
    // by which point `measure` below has already joined the engine thread, and
    // dropping the engine has already closed the writer threads.
    let (files, _cleanup) = open_files(journal, log)?;

    let stop = Arc::new(AtomicBool::new(false));
    let engine_stop = Arc::clone(&stop);

    // `--wire-timestamps`: the tap is reading before the client's SYN, which
    // is what `pair::requests` counts the stream from. With no flag nothing is
    // started and nothing is printed. The observer also holds the engine's
    // `stop`, for a SIGINT or SIGTERM (`signal`).
    let observer = match wire {
        Some(w) => Some(wire::Observer::start(
            w,
            mode,
            local,
            4 * (run.warmup + run.n) + 4096,
            Arc::clone(&stop),
        )?),
        None => None,
    };
    let handoff = observer.as_ref().map(wire::Observer::handoff);

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
        print_engine_tid();
        // `false`: the client on the other thread stops this engine through
        // `stop`, and times its own window.
        match desk {
            None => {
                serve_chosen::<_, false>(acceptor, &engine_stop, mode, Never, side, files, handoff)
            }
            Some(d) => {
                serve_chosen::<_, false>(acceptor, &engine_stop, mode, d, side, files, handoff)
            }
        }
    };
    let engine = spawn_engine(body, engine_core)?;

    pin_client(client_core)?;

    let peer = Peer::InProcess {
        stop: &stop,
        engine,
    };
    let measured = match tls {
        Tls::Off => {
            // The plain arm, exactly as it was before `--tls` existed: a
            // blocking `TcpStream` with Nagle off. Changing this client changes
            // every figure this binary has published.
            let sock = TcpStream::connect(&addr)?;
            sock.set_nodelay(true)?;
            measure(sock, &run, peer)
        }
        #[cfg(all(feature = "tls", target_os = "linux"))]
        Tls::Ktls | Tls::Userspace => {
            let Some(p) = pki.as_ref() else {
                return Err(std::io::Error::other("w2w: a TLS arm with no certificate"));
            };
            let sock = tls_arm::connect(&addr, p, tls == Tls::Ktls)?;
            measure(sock, &run, peer)
        }
        #[cfg(not(all(feature = "tls", target_os = "linux")))]
        Tls::Ktls | Tls::Userspace => {
            // Unreachable: refused above, before any thread started.
            let _ = peer;
            return Err(std::io::Error::other("this build has no TLS transport"));
        }
    };

    // A SIGINT or SIGTERM (`--wire-timestamps` only): the observer stopped the
    // engine, so the client's read ended — as an error, or after the hold.
    // Join the observer, which puts the NIC back as it drops, and let `main`
    // say which signal it was; nothing about this run is reported.
    if signal::caught().is_some() {
        drop(observer.map(wire::Observer::finish));
        return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
    }
    let Measured {
        mut samples,
        allocs,
        late,
    } = measured?;

    // The engine has been stopped and joined inside `measure`; the observer
    // holds its own `dup` of the socket, so it drains the last stamps now.
    let observed = observer.map(wire::Observer::finish).transpose()?;
    // And a signal that arrived while it drained.
    if signal::caught().is_some() {
        return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
    }

    let what = match path {
        Path::Admin => "TestRequest -> Heartbeat",
        Path::App => "NewOrderSingle -> ExecutionReport",
    };
    println!("w2w: {what}, over kernel TCP on loopback");
    println!("     mode   {:>9}", mode.name());
    println!("     path   {:>9}", path.name());
    print_figures(&mut samples, &run, late);
    // Which threads `allocs` counted — every one this process ran in the
    // window, the journal's and the log's writers included. With no flag it
    // reads `both threads`, as it always did.
    let counted = counted_threads(true, observed.is_some(), journal, log);
    println!("     allocs {allocs:>9}   ({counted}, the timed window only)");
    let verdict = match &observed {
        None => Ok(()),
        Some(o) => o.report(wire::Window::Combined {
            warmup: run.warmup,
            n: run.n,
            tls: tls != Tls::Off,
        }),
    };
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
    // After the allocation verdict, so a tap that did not line up with the run
    // cannot hide an allocation count — and before the footer, so it fails the
    // run.
    verdict?;
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

/// `--listen`: the engine thread alone, serving until its last connection has
/// closed. Prints no latency figure — see the module note.
fn engine_half(
    addr: &str,
    mode: Mode,
    run: Run,
    engine_core: Option<usize>,
    journal: JournalKind,
    log: LogKind,
    wire: Option<&WireArgs>,
) -> std::io::Result<()> {
    let Run { path, tls, .. } = run;
    let acceptor = Acceptor::bind(addr)?;
    let bound = acceptor.local_addr()?;

    println!("mode: {}", mode.name());
    println!("path: {}", path.name());
    // The bound address, not the argument, so `--listen 127.0.0.1:0` tells the
    // generator where to go.
    println!("listening: {bound}");
    // Only when asked for — see the module note "The journal and the message
    // log, each a type of its own".
    if journal != JournalKind::Mem {
        println!("journal: {}", journal.name());
    }
    if log != LogKind::None {
        println!("log: {}", log.name());
    }

    // Nobody stores `true` here but the observer, on SIGINT or SIGTERM
    // (`signal`): this engine ends when its peer does.
    let stop = Arc::new(AtomicBool::new(false));
    // Before the engine thread, so the tap sees the generator's SYN.
    let observer = match wire {
        Some(w) => Some(wire::Observer::start(
            w,
            mode,
            bound,
            wire::LISTEN_CAPACITY,
            Arc::clone(&stop),
        )?),
        None => None,
    };
    let handoff = observer.as_ref().map(wire::Observer::handoff);

    let desk = match path {
        Path::Admin => None,
        Path::App => Some(Desk::new()?),
    };
    // Opened before the engine thread starts, outside any timed window — see
    // `both_halves`'s copy of this comment. `_cleanup` removes the files once
    // `engine.join()` below has returned, by which point dropping the engine
    // has already closed the writer threads.
    let (files, _cleanup) = open_files(journal, log)?;
    let body = move || {
        print_engine_tid();
        // `true`: arm the allocation counter after the first logon, and return
        // once the last connection has closed.
        match desk {
            None => serve_chosen::<_, true>(
                acceptor,
                &stop,
                mode,
                ListenNever,
                EngineSide::Plain,
                files,
                handoff,
            ),
            Some(d) => {
                serve_chosen::<_, true>(acceptor, &stop, mode, d, EngineSide::Plain, files, handoff)
            }
        }
    };
    let engine = spawn_engine(body, engine_core)?;
    // The main thread blocks here and allocates nothing, so the count below is
    // the engine thread's.
    engine
        .join()
        .map_err(|_| std::io::Error::other("w2w: the engine thread panicked"))?;
    let allocs = ALLOCS.load(Ordering::Relaxed);
    let observed = observer.map(wire::Observer::finish).transpose()?;
    // As in `both_halves`: joined, drained, NIC put back — `main` reports.
    if signal::caught().is_some() {
        return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
    }

    // Read back, as in the combined run. `half_of` has refused every arm but
    // `off`, so anything else here is a transport this process did not ask for.
    let seen = TLS_SEEN.load(Ordering::Relaxed);
    println!("tls: {}", seen_name(seen));
    if seen_name(seen) != tls.wants() {
        return Err(std::io::Error::other(format!(
            "w2w: --listen requires the engine to report tls '{}', and it reports '{}'",
            tls.wants(),
            seen_name(seen),
        )));
    }

    if APP_REACHED.load(Ordering::Relaxed) {
        return Err(std::io::Error::other(
            "w2w: --listen --path admin was sent an application message; the --connect \
             process is running another --path, and nothing it timed is an admin figure",
        ));
    }

    println!("w2w: engine half, over kernel TCP, listening on {bound}");
    println!("     mode   {:>9}", mode.name());
    println!("     path   {:>9}", path.name());
    // As in `both_halves`: every thread counted, named. With no flag it reads
    // `engine thread`, as it always did.
    let counted = counted_threads(false, observed.is_some(), journal, log);
    println!(
        "     allocs {allocs:>9}   ({counted}, first logon to last close, warmup and teardown included)"
    );
    let verdict = match &observed {
        None => {
            println!(
                "     no latency figures: the far end of every round trip is in the --connect"
            );
            println!("     process, and this build takes no wire timestamps.");
            Ok(())
        }
        Some(o) => {
            println!(
                "     no round-trip figures: the far end of every round trip is in the --connect"
            );
            println!(
                "     process. The wire figures below are this acceptor's own, NIC in to NIC out."
            );
            o.report(wire::Window::Listen { skip: run.warmup })
        }
    };
    println!();
    // Non-negotiable 1, for the one thread in this process that does any work.
    // The window is wider than the combined run's — warmup and the close are in
    // it — so a zero here is at least as strong a claim.
    assert_eq!(
        allocs, 0,
        "w2w: {allocs} allocations on the engine thread between the first logon and \
         the last close — CLAUDE.md §2 non-negotiable 1 says the engine's path has none"
    );
    verdict?;
    println!("CLAUDE.md §2 rule 10: a number without its machine is someone else's claim.");
    println!("ADR-0013 decision 4: `mode` above is this engine's; the --connect table is");
    println!("about this engine only when it was run against this process.");
    Ok(())
}

/// The threads an `allocs` line counts, for its label. [`ALLOCS`] counts every
/// thread, so the label names every thread the run started: the engine, the
/// client in the combined run, the observer under `--wire-timestamps`, and the
/// writer threads `--journal file-async` and `--log file` open. With none of
/// those flags it is `both threads` (combined) or `engine thread` (`--listen`),
/// the words those lines always had.
fn counted_threads(client: bool, observer: bool, journal: JournalKind, log: LogKind) -> String {
    let mut names = vec!["engine"];
    if client {
        names.push("client");
    }
    if observer {
        names.push("observer");
    }
    if journal == JournalKind::FileAsync {
        names.push("journal writer");
    }
    if log == LogKind::File {
        names.push("log writer");
    }
    match names.as_slice() {
        ["engine", "client"] => "both threads".to_string(),
        [one] => format!("{one} thread"),
        [first @ .., last] => format!("{} and {last} threads", first.join(", ")),
        [] => String::new(),
    }
}

/// How long `--connect` waits for any one reply before calling the run failed.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// `--connect`: the client thread alone. Its figures are the counterparty's
/// view and are labelled so — see the module note.
fn generator_half(addr: &str, run: Run, client_core: Option<usize>) -> std::io::Result<()> {
    let Run { path, .. } = run;
    println!("path: {}", path.name());
    println!("connect: {addr}");
    pin_client(client_core)?;

    let sock = TcpStream::connect(addr)?;
    sock.set_nodelay(true)?;
    // A bound on every blocking read, set once, before the logon. The combined
    // run cannot be sent a message its engine will not answer; two processes
    // can — `--path app` against a `--listen --path admin` gets no reply at
    // all — and a generator that hangs forever reports nothing. `SO_RCVTIMEO`
    // adds no syscall to a read, which is what the `strace -f` gate reads.
    sock.set_read_timeout(Some(REPLY_TIMEOUT))?;
    let Measured {
        mut samples,
        allocs,
        late,
    } = measure(sock, &run, Peer::Remote).map_err(|e| match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => std::io::Error::new(
            e.kind(),
            format!(
                "w2w: no reply from {addr} within {} s — is the --listen process up, and \
                 running the same --path?",
                REPLY_TIMEOUT.as_secs()
            ),
        ),
        _ => e,
    })?;

    let what = match path {
        Path::Admin => "TestRequest -> Heartbeat",
        Path::App => "NewOrderSingle -> ExecutionReport",
    };
    println!("w2w: {what}, as the counterparty sees it, over kernel TCP to {addr}");
    println!("     path   {:>9}", path.name());
    print_figures(&mut samples, &run, late);
    println!("     allocs {allocs:>9}   (generator thread, the timed window only)");
    println!();
    // This thread's timed loop allocates nothing (see `field`), and an
    // allocation between two samples is this tool perturbing its own figure.
    assert_eq!(
        allocs,
        0,
        "w2w: {allocs} allocations on the generator thread inside the timed window \
         over {} messages",
        samples.len()
    );
    println!("AS THE COUNTERPARTY SEES IT: each figure above is a round trip on this");
    println!("process's clock — this host's stack, the wire and the acceptor together.");
    println!("It is not a wire-to-wire figure at the acceptor, and nothing is subtracted");
    println!("from it. The engine's mode, pinning and allocations are in the --listen output.");
    println!("NOT A LATENCY NUMBER FOR PUBLICATION unless both machines match DESIGN.md §9.");
    println!("CLAUDE.md §2 rule 10: a number without its machine is someone else's claim.");
    Ok(())
}

/// The engine thread's tid, so a syscall trace can be attributed to THIS
/// thread and not to the client on the main thread, which blocks on purpose.
/// `/proc/thread-self` resolves to `<pid>/task/<tid>` for the calling thread;
/// no dependency and no `gettid` binding needed.
fn print_engine_tid() {
    #[cfg(target_os = "linux")]
    if let Ok(link) = std::fs::read_link("/proc/thread-self")
        && let Some(tid) = link.to_string_lossy().rsplit('/').next()
    {
        println!("engine-tid: {tid}");
    }
}

/// The client pins itself, from inside the thread that will run, which is
/// ADR-0015's first clause. The main thread is the client.
fn pin_client(client_core: Option<usize>) -> std::io::Result<()> {
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    if let Some(cpu) = client_core {
        use fixbolt_engine::affinity::{self, CoreId};
        affinity::pin_current_thread(CoreId(cpu)).map_err(std::io::Error::other)?;
        let on = affinity::running_on().map_err(std::io::Error::other)?;
        println!("client-core: {on}");
    } else {
        println!("client-core: not pinned");
    }
    // `main` has refused a core on a build that cannot pin, so this is
    // genuinely unused rather than quietly ignored.
    #[cfg(not(all(feature = "affinity", target_os = "linux")))]
    {
        let _ = client_core;
        println!("client-core: not pinned");
    }
    Ok(())
}

/// The percentile rows, shared by the combined run and the generator half so
/// the two tables cannot drift apart in format.
fn print_figures(samples: &mut [u64], run: &Run, late: usize) {
    samples.sort_unstable();
    let pick = |q: f64| samples[((samples.len() as f64 - 1.0) * q) as usize];
    println!("     {} samples after {} warmup", samples.len(), run.warmup);
    // Only when asked for, so a run with no `--interval` prints what it did.
    if run.interval_us > 0 {
        println!(
            "     interval {} us after each send, spun on the client thread; {late} of {} sends late",
            run.interval_us,
            samples.len()
        );
    }
    println!("     min    {:>9} ns", samples[0]);
    println!("     p50    {:>9} ns", pick(0.50));
    println!("     p99    {:>9} ns", pick(0.99));
    // Phase 1 exit criterion 6 names p99.9 specifically, and it was the one
    // percentile this binary did not print. At the default 20 000 samples it is
    // the mean of nothing — it is one sample, the 19 981st — so the criterion is
    // reported with the sample count beside it and never without.
    println!("     p99.9  {:>9} ns", pick(0.999));
    println!("     max    {:>9} ns", samples[samples.len() - 1]);
}

/// What [`measure`] needs to know about the run, as one argument.
#[derive(Clone, Copy)]
struct Run {
    path: Path,
    warmup: usize,
    n: usize,
    hold_ms: u64,
    tls: Tls,
    /// `--interval`, µs; `0` is back-to-back.
    interval_us: u64,
}

/// Who is on the other end of the client's socket.
enum Peer<'a> {
    /// The engine thread of this process: its transport is read back before
    /// the first sample, and it is stopped and joined after the last.
    InProcess {
        stop: &'a AtomicBool,
        engine: std::thread::JoinHandle<()>,
    },
    /// Another process (`--connect`). Nothing about it can be read back from
    /// here, so nothing is printed about it.
    Remote,
}

/// What [`measure`] returns.
struct Measured {
    /// Unsorted.
    samples: Vec<u64>,
    /// Allocations over the timed window, every thread in this process.
    allocs: usize,
    /// Timed sends that were already overdue under `--interval`.
    late: usize,
}

/// The client half: log on, read back which transport the engine got, warm up,
/// time `n` round trips, and stop the engine.
///
/// Generic over [`Wire`] so the plain arm and both TLS arms run **one** loop:
/// for `C = TcpStream` it is the loop this binary had before `--tls`, call for
/// call — `write_all`, then `read` until one whole message.
///
/// With [`Peer::Remote`] there is no engine to read back or stop: the socket is
/// closed after `--hold-ms`, which is what ends the `--listen` process.
fn measure<C: Wire>(mut sock: C, run: &Run, peer: Peer<'_>) -> std::io::Result<Measured> {
    let Run {
        path,
        warmup,
        n,
        hold_ms,
        tls,
        interval_us,
    } = *run;

    // Logon first, and read the answer, so the timed loop starts on an
    // established session rather than on a handshake.
    write_and_read(&mut sock, &logon(1))?;

    if let Peer::InProcess { .. } = peer {
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
    // Warmup is paced like the window, so the window starts on an engine in
    // the state the interval puts it in.
    let mut pacer = Pacer::new(interval_us);
    for m in msgs.iter().take(warmup) {
        pacer.wait();
        pacer.sent_now();
        sock.put(m)?;
        read_one(&mut sock, &mut buf)?;
    }

    let mut samples: Vec<u64> = Vec::with_capacity(n);
    let mut late = 0usize;
    // Armed after `samples` has its capacity, so the one allocation this loop
    // would otherwise be blamed for is outside the window rather than excused
    // inside it.
    ARMED.store(true, Ordering::Relaxed);
    for m in msgs.iter().skip(warmup) {
        // Before `t0`: the wait is never inside a sample. With no `--interval`
        // this returns at once, having read no clock.
        if pacer.wait() {
            late += 1;
        }
        let t0 = Instant::now();
        sock.put(m)?;
        let len = read_one(&mut sock, &mut buf)?;
        let ns = t0.elapsed().as_nanos();
        pacer.sent(t0);
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
        hold(Duration::from_millis(hold_ms));
    }

    match peer {
        Peer::InProcess { stop, engine } => {
            stop.store(true, Ordering::Relaxed);
            drop(sock);
            let _ = engine.join();
        }
        // Closing is what ends the `--listen` process.
        Peer::Remote => drop(sock),
    }
    Ok(Measured {
        samples,
        allocs,
        late,
    })
}

/// `--hold-ms`. With no signal handler — every run without `--wire-timestamps`
/// — one `sleep`, as it always was. With one, slices of at most 10 ms, so a
/// SIGINT or SIGTERM during a long hold ends the run within a slice rather
/// than after the hold (`std::thread::sleep` resumes after `EINTR`).
fn hold(d: Duration) {
    if !signal::installed() {
        std::thread::sleep(d);
        return;
    }
    let end = Instant::now() + d;
    while signal::caught().is_none() {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(Duration::from_millis(10)));
    }
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

/// Where each accepted connection's resend store comes from: `--journal`,
/// resolved to a type. See the module note "The journal and the message log,
/// each a type of its own".
///
/// A trait with an associated type rather than a `FnMut` closure, so the
/// journal is a type parameter of [`pump`]'s engine — never a value matched
/// on inside it — and so each choice names one type, not one per call site.
trait Journals {
    /// The journal each connection gets.
    type J: fixbolt_session::journal::Journal;

    /// The next connection's journal, or `None` to refuse that connection.
    fn next(&mut self) -> Option<Self::J>;
}

/// `--journal mem`, the default: a fresh `Store` per connection — the
/// `J::default()` that `Engine::add` builds, so the no-flag engine's journal
/// is the type and the value it was before `--journal` existed.
struct FreshStore;

impl Journals for FreshStore {
    type J = fixbolt_engine::journal::Store;

    #[inline]
    fn next(&mut self) -> Option<Self::J> {
        Some(fixbolt_engine::journal::Store::default())
    }
}

/// The file journal `--journal file-async` opens, sized with the same
/// [`fixbolt_engine::journal::SLOTS`] and [`fixbolt_engine::journal::SLOT_LEN`]
/// as the default `Store` (`STATUS.md` item 88), so boot B's row B5 changes
/// exactly one variable between its "none" and "file-async" arms.
type FileStore = fixbolt_engine::journal::FileJournal<
    { fixbolt_engine::journal::SLOTS },
    { fixbolt_engine::journal::SLOT_LEN },
>;

/// `--journal file-async`: the one `FileJournal` this run opened, to the
/// first connection, and nothing to any after.
struct OneFile(Option<FileStore>);

impl Journals for OneFile {
    type J = FileStore;

    #[inline]
    fn next(&mut self) -> Option<Self::J> {
        self.0.take()
    }
}

/// Where `--journal file-async` and `--log file` keep their files: under the
/// system temp directory, named by this process's id so two runs on the same
/// box never collide.
fn journal_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "fixbolt-w2w-journal-{}.journal",
        std::process::id()
    ))
}

/// See [`journal_path`].
fn log_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("fixbolt-w2w-log-{}.log", std::process::id()))
}

/// Deletes its path when dropped, best effort. Used only for a file this run
/// itself opened — `--journal file-async` or `--log file` — never for a
/// deployment's own files, which this tool does not otherwise touch.
struct TempFile(std::path::PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The files `--journal file-async` and `--log file` opened, handed to the
/// engine thread. `None` is the flag's default, and [`serve_chosen`] turns
/// each `None` into the default type, not into a value of a wider one.
struct Opened {
    journal: Option<FileStore>,
    log: Option<fixbolt_engine::msglog::FileLog>,
}

/// The guards that remove whichever files [`open_files`] opened. Held by the
/// thread that joins the engine thread, never by the engine thread itself.
struct Cleanup {
    _journal: Option<TempFile>,
    _log: Option<TempFile>,
}

/// Open what `--journal` and `--log` asked for: before the engine thread
/// starts and outside the timed window, so a temp directory that refuses a
/// file fails the run rather than the first message. With neither flag this
/// opens nothing and touches no file.
fn open_files(journal: JournalKind, log: LogKind) -> std::io::Result<(Opened, Cleanup)> {
    let (journal, journal_cleanup) = match journal {
        JournalKind::Mem => (None, None),
        JournalKind::FileAsync => {
            let path = journal_path();
            let opened = fixbolt_engine::journal::FileJournal::open(
                &path,
                fixbolt_engine::journal::Durability::Async,
            )?;
            (Some(opened), Some(TempFile(path)))
        }
    };
    let (log, log_cleanup) = match log {
        LogKind::None => (None, None),
        LogKind::File => {
            let path = log_path();
            let opened = fixbolt_engine::msglog::FileLog::open(&path)?;
            (Some(opened), Some(TempFile(path)))
        }
    };
    Ok((
        Opened { journal, log },
        Cleanup {
            _journal: journal_cleanup,
            _log: log_cleanup,
        },
    ))
}

/// Pick the journal and the log, with the application and the transport side
/// already chosen: the **one** place `--journal` and `--log` are matched on,
/// before the engine's first turn.
///
/// Each arm calls [`serve`] with its own type parameters, so each is its own
/// monomorphised engine. The `(None, None)` arm is `FreshStore` + `NoLog` —
/// the engine this tool timed before either flag existed; the module note says
/// why that is a rule and not a nicety.
fn serve_chosen<A: Application, const UNTIL_CLOSED: bool>(
    acceptor: Acceptor,
    stop: &AtomicBool,
    mode: Mode,
    app: A,
    side: EngineSide,
    files: Opened,
    stamp: Option<Arc<wire::Handoff>>,
) {
    use fixbolt_engine::msglog::NoLog;
    match files {
        Opened {
            journal: None,
            log: None,
        } => serve::<_, _, _, UNTIL_CLOSED>(
            acceptor, stop, mode, app, side, FreshStore, NoLog, stamp,
        ),
        Opened {
            journal: Some(j),
            log: None,
        } => serve::<_, _, _, UNTIL_CLOSED>(
            acceptor,
            stop,
            mode,
            app,
            side,
            OneFile(Some(j)),
            NoLog,
            stamp,
        ),
        Opened {
            journal: None,
            log: Some(l),
        } => serve::<_, _, _, UNTIL_CLOSED>(acceptor, stop, mode, app, side, FreshStore, l, stamp),
        Opened {
            journal: Some(j),
            log: Some(l),
        } => serve::<_, _, _, UNTIL_CLOSED>(
            acceptor,
            stop,
            mode,
            app,
            side,
            OneFile(Some(j)),
            l,
            stamp,
        ),
    }
}

/// Pick the transport, with the application, the journal and the log already
/// chosen.
///
/// `wrap` is `lib.rs`'s `pump` shape: it turns an accepted socket into whatever
/// this engine's connections are. For the plain arm it is `Some`, and the
/// compiler removes it.
///
/// `UNTIL_CLOSED` is `--listen`: see [`pump`]. A const, so the combined run's
/// loop is compiled with no trace of it.
// Eight: A4's journal and log and A3b's stamp each arrived as a type or a value
// the engine thread must own, and bundling them would only rename the list.
#[allow(clippy::too_many_arguments)]
fn serve<A: Application, S: Journals, L: MessageLog, const UNTIL_CLOSED: bool>(
    acceptor: Acceptor,
    stop: &AtomicBool,
    mode: Mode,
    app: A,
    side: EngineSide,
    journals: S,
    log: L,
    stamp: Option<Arc<wire::Handoff>>,
) {
    match (side, stamp) {
        // No `--wire-timestamps`: the accept path this binary always had.
        (EngineSide::Plain, None) => {
            run::<_, _, _, _, _, UNTIL_CLOSED>(
                acceptor,
                stop,
                mode,
                app,
                Some::<TcpTransport>,
                journals,
                log,
            );
        }
        // `--wire-timestamps`: the observer sets `SO_TIMESTAMPING` on this
        // socket before the engine owns it, and the engine thread makes no call
        // of its own to get there — see `wire::Handoff::attach`. A socket the
        // observer could not stamp is dropped, and the run fails naming why.
        (EngineSide::Plain, Some(h)) => {
            run::<_, _, _, _, _, UNTIL_CLOSED>(
                acceptor,
                stop,
                mode,
                app,
                move |t: TcpTransport| h.attach(t.socket()).then_some(t),
                journals,
                log,
            );
        }
        #[cfg(all(feature = "tls", target_os = "linux"))]
        (EngineSide::Tls(cfg, offload), stamp) => {
            run::<_, _, _, _, _, UNTIL_CLOSED>(
                acceptor,
                stop,
                mode,
                app,
                move |sock| {
                    // Stamped on the TCP socket, before the handshake's first
                    // byte, so the key counts from the same `write_seq` as the
                    // plain arm.
                    if let Some(h) = &stamp
                        && !h.attach(sock.socket())
                    {
                        return None;
                    }
                    // The handshake runs inside the transport's `recv`/`send` on
                    // the engine thread, before the first logon and so before
                    // the timed window — `tls.rs`'s module note on why it is not
                    // a pre-session stage. A connection rustls will not even
                    // start is dropped.
                    rustls::server::UnbufferedServerConnection::new(std::sync::Arc::clone(&cfg))
                        .ok()
                        .map(|conn| {
                            fixbolt_engine::tls::TlsTransport::with_offload(
                                sock,
                                fixbolt_engine::tls::Handshake::new(conn),
                                offload,
                            )
                        })
                },
                journals,
                log,
            );
        }
    }
}

/// Pick the idle strategy, with the application and the transport already
/// chosen.
///
/// Split out from [`pump`] so that `--mode` and `--path` do not multiply into
/// six copies of the loop: a reversal that also changed the loop would prove
/// nothing about the loop.
fn run<
    A: Application,
    T: Transport,
    F: FnMut(TcpTransport) -> Option<T>,
    S: Journals,
    L: MessageLog,
    const UNTIL_CLOSED: bool,
>(
    acceptor: Acceptor,
    stop: &AtomicBool,
    mode: Mode,
    app: A,
    wrap: F,
    journals: S,
    log: L,
) {
    match mode {
        Mode::Hft => {
            pump::<_, _, _, _, _, _, UNTIL_CLOSED>(acceptor, stop, Spin, app, wrap, journals, log);
        }
        Mode::Yield => {
            pump::<_, _, _, _, _, _, UNTIL_CLOSED>(acceptor, stop, Yield, app, wrap, journals, log);
        }
        #[cfg(all(feature = "standard", unix))]
        Mode::Standard => pump::<_, _, _, _, _, _, UNTIL_CLOSED>(
            acceptor,
            stop,
            fixbolt_engine::block::Block::new(16),
            app,
            wrap,
            journals,
            log,
        ),
        #[cfg(not(all(feature = "standard", unix)))]
        Mode::Standard => {
            let _ = (wrap, journals, log);
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
///
/// **`UNTIL_CLOSED`, and only for `--listen`.** That process has no client
/// thread to time a window or set `stop`, so the engine thread does both
/// ends itself: it arms [`ARMED`] once the first logon is in, and the second
/// loop returns once the live connection count is zero, disarming on the way
/// out. The window therefore holds warmup and the turn that saw the close. As
/// a const, `false` compiles both checks away: the combined run's loops are
/// the loops above, turn for turn.
///
/// **`S` and `L` are the engine's journal and log types**, chosen once by
/// [`serve_chosen`]. With `FreshStore` and `NoLog` the engine below is the
/// `Engine<…, Store, 256, 4096, 8192>` this function built before `--journal`
/// and `--log` existed; `journals.next()` runs only when a connection is
/// accepted, never in a turn.
fn pump<
    A: Application,
    W: Waiting,
    T: Transport,
    F: FnMut(TcpTransport) -> Option<T>,
    S: Journals,
    L: MessageLog,
    const UNTIL_CLOSED: bool,
>(
    acceptor: Acceptor,
    stop: &AtomicBool,
    wait: W,
    app: A,
    mut wrap: F,
    mut journals: S,
    log: L,
) {
    let mut engine: Engine<
        T,
        fixbolt_session::Acceptor,
        InlineDispatch<A>,
        fixbolt_engine::clock::SystemClock,
        W,
        S::J,
        256,
        4096,
        8192,
        L,
    > = Engine::<_, _, _, _, _, S::J, 256, 4096, 8192>::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"W2W"),
        InlineDispatch::new(app),
        fixbolt_engine::clock::SystemClock,
        wait,
        8,
    )
    .with_log(log);
    let listener = acceptor.source().map(Interest::readable);
    let extra: &[Interest] = listener.as_slice();
    let mut first: Option<ConnId> = None;
    while !stop.load(Ordering::Relaxed) {
        while let Some(t) = acceptor.accept() {
            // `OneFile` hands out the one `--journal file-async` file to the
            // first connection and nothing to any after — see the module note.
            // `FreshStore` never runs dry.
            if let Some(t) = wrap(t)
                && let Some(j) = journals.next()
            {
                let id = engine.add_with_journal(t, j);
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
            if UNTIL_CLOSED {
                ARMED.store(true, Ordering::Relaxed);
            }
            break;
        }
    }
    while !stop.load(Ordering::Relaxed) {
        while let Some(t) = acceptor.accept() {
            if let Some(t) = wrap(t)
                && let Some(j) = journals.next()
            {
                let _ = engine.add_with_journal(t, j);
            }
        }
        if !engine.turn() {
            engine.idle_with(extra);
        }
        if UNTIL_CLOSED && engine.connections() == 0 {
            break;
        }
    }
    if UNTIL_CLOSED {
        ARMED.store(false, Ordering::Relaxed);
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

/// `--wire-timestamps`: the tap, the error queue, the NIC's configuration, and
/// the report. Linux only; see the module note's *Wire timestamps* section.
///
/// **Every `unsafe` block here is an FFI call, or a read of what one returned,
/// and none is on the engine thread.** What proves them, together: the three
/// `pair::` tests prove the arithmetic on what these calls return; the `lo`
/// run of step A3b (`requests` read back equal to logon + warmup + timed, and
/// `allocs 0`) proves the tap's `recvmsg` and header walk return the frames the
/// run sent; `scripts/check-no-kernel-sleep.sh` and
/// `scripts/check-standard-gives-the-core-back.sh` under
/// `W2W_EXTRA="--wire-timestamps --nic lo --observer-core 2"`, each run inside
/// `unshare -Urn`, prove the engine thread's syscalls and idle CPU are unchanged
/// with all of it running, and — since each reads back the `wire-timestamps:`
/// and `hw-rx-missing` lines — that the arm ran at all; the descriptor handed
/// from the engine to the observer is proven live by the handoff's state
/// machine and its four tests (`stamp`'s SAFETY note); the `standard` refusal is proven by
/// `standard_is_refused_on_a_hardware_nic_and_not_on_loopback` and by running a
/// binary with no capability against `enp9s0`. A
/// hardware stamp has not been read yet (no cable): the `SCM_TIMESTAMPING` walk
/// is proven on an absent `ts[2]` only.
#[cfg(target_os = "linux")]
mod wire {
    use std::io;
    use std::mem::{size_of, zeroed};
    use std::net::{SocketAddr, TcpStream};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU16, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::pair::{self, Inbound, Sent};
    use super::{Mode, WireArgs};

    /// Records kept by `--listen`, which cannot know the generator's count.
    /// A run that outgrows it is told so and gets no wire column.
    pub const LISTEN_CAPACITY: usize = 1 << 20;

    /// `SCM_TSTAMP_SND`, `include/uapi/linux/errqueue.h`; not in `libc`.
    const SCM_TSTAMP_SND: u32 = 0;
    /// Bytes of each tap frame the kernel copies: IPv4 and TCP headers at
    /// their largest (60 + 60). The BPF filter returns it as the snap length.
    const SNAP: usize = 128;
    /// How long the engine's accept path waits for the observer to stamp.
    const ATTACH_TIMEOUT: Duration = Duration::from_secs(2);
    /// After `finish`, how long both queues must stay empty before the
    /// observer stops: a hardware TX stamp is delivered from the driver's PTP
    /// work, after the reply has left.
    const QUIET: Duration = Duration::from_millis(100);
    /// And the most `finish` will wait for that quiet.
    const DRAIN_MAX: Duration = Duration::from_secs(2);

    // The handoff's states. The engine moves IDLE→OFFERED and, only from
    // OFFERED, OFFERED→FAILED on its timeout; the observer moves
    // OFFERED→CLAIMED before it touches the descriptor, and CLAIMED→ATTACHED
    // or CLAIMED→FAILED when it is done. Every move out of OFFERED is a CAS, so
    // exactly one of the timeout and the claim wins.
    const IDLE: u8 = 0;
    const OFFERED: u8 = 1;
    const ATTACHED: u8 = 2;
    const FAILED: u8 = 3;
    const CLAIMED: u8 = 4;

    /// The engine thread's side of the stamp: a descriptor offered across
    /// atomics, and the observer's answer.
    pub struct Handoff {
        state: AtomicU8,
        fd: AtomicI32,
        peer: AtomicU16,
        /// Connections after the first, which are served unstamped.
        extra: AtomicUsize,
    }

    /// What the engine sees when it looks at its offer once.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Answer {
        /// Not answered yet: offered and not timed out, or claimed.
        Waiting,
        /// The observer stamped the socket.
        Attached,
        /// The observer claimed it and could not stamp it, and has said why.
        Failed,
        /// Nobody claimed it before the deadline; it can no longer be claimed.
        TimedOut,
    }

    /// The observer's hold on an offered descriptor. **While it lives, the
    /// engine's [`Handoff::attach`] neither times out nor returns**, so the
    /// engine still owns the socket and [`Claim::fd`] names a live descriptor.
    /// Dropped without [`Claim::resolve`] — an unwind — it fails the offer, so
    /// the engine is never left waiting on a claim nobody will answer.
    pub struct Claim<'a> {
        h: &'a Handoff,
        fd: RawFd,
        done: bool,
    }

    impl Claim<'_> {
        pub const fn fd(&self) -> RawFd {
            self.fd
        }

        /// CLAIMED→ATTACHED or CLAIMED→FAILED: the engine may go on.
        pub fn resolve(mut self, ok: bool) {
            self.finish(if ok { ATTACHED } else { FAILED });
        }

        fn finish(&mut self, to: u8) {
            if !self.done {
                self.done = true;
                self.h.state.store(to, Ordering::Release);
            }
        }
    }

    impl Drop for Claim<'_> {
        fn drop(&mut self) {
            self.finish(FAILED);
        }
    }

    impl Handoff {
        pub const fn new() -> Self {
            Self {
                state: AtomicU8::new(IDLE),
                fd: AtomicI32::new(-1),
                peer: AtomicU16::new(0),
                extra: AtomicUsize::new(0),
            }
        }

        /// Engine: IDLE→OFFERED with `fd`. `false` once any offer has been made
        /// — only the first connection is stamped.
        fn offer(&self, fd: RawFd) -> bool {
            if self.state.load(Ordering::Acquire) != IDLE {
                return false;
            }
            // Only the engine thread moves out of IDLE, so the descriptor is in
            // place before anyone can see OFFERED.
            self.fd.store(fd, Ordering::Release);
            self.state.store(OFFERED, Ordering::Release);
            true
        }

        /// Engine: look at the offer once. `past_deadline` lets it time out,
        /// and only an offer nobody has claimed can.
        fn answer(&self, past_deadline: bool) -> Answer {
            match self.state.load(Ordering::Acquire) {
                ATTACHED => Answer::Attached,
                FAILED => Answer::Failed,
                // A CAS, not a store: the observer may be claiming right now,
                // and if it won, this offer is no longer the engine's to end.
                OFFERED
                    if past_deadline
                        && self
                            .state
                            .compare_exchange(OFFERED, FAILED, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok() =>
                {
                    Answer::TimedOut
                }
                // OFFERED and not yet due (or just claimed), or CLAIMED at any
                // time: the observer answers.
                _ => Answer::Waiting,
            }
        }

        /// Observer: OFFERED→CLAIMED, before touching the descriptor.
        pub fn claim(&self) -> Option<Claim<'_>> {
            self.state
                .compare_exchange(OFFERED, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
                .ok()
                .map(|_| Claim {
                    h: self,
                    fd: self.fd.load(Ordering::Acquire),
                    done: false,
                })
        }

        /// On the engine thread, before `engine.add`. **No syscall**: two
        /// atomic stores, then a spin on `Instant` (the vDSO) until the
        /// observer answers. `false` drops the connection; the observer has
        /// said why on stderr, or the timeout line below does. The timeout
        /// fires only while the offer is unclaimed: once the observer holds a
        /// [`Claim`], this waits for its answer, because returning would drop
        /// the socket under a descriptor the observer is using.
        pub fn attach(&self, sock: &TcpStream) -> bool {
            if !self.offer(sock.as_raw_fd()) {
                self.extra.fetch_add(1, Ordering::Relaxed);
                return true;
            }
            let deadline = Instant::now() + ATTACH_TIMEOUT;
            loop {
                match self.answer(Instant::now() > deadline) {
                    Answer::Attached => return true,
                    Answer::Failed => return false,
                    Answer::TimedOut => {
                        eprintln!(
                            "w2w: --wire-timestamps: the observer did not stamp the engine's socket \
                             within {} s",
                            ATTACH_TIMEOUT.as_secs()
                        );
                        return false;
                    }
                    Answer::Waiting => std::hint::spin_loop(),
                }
            }
        }
    }

    /// Which requests the figures are over.
    pub enum Window {
        /// The combined run: logon, `warmup`, then `n` timed. A TLS arm's
        /// handshake records are requests too, so it gets no window.
        Combined { warmup: usize, n: usize, tls: bool },
        /// `--listen`: every request after the logon but the first `skip`
        /// (`--listen --warmup <n>`).
        Listen { skip: usize },
    }

    /// What the observer recorded, for [`Observed::report`].
    pub struct Observed {
        nic: String,
        loopback: bool,
        attached: bool,
        peer: u16,
        extra: usize,
        frames: Vec<Inbound>,
        sent: Vec<Sent>,
        /// Records that did not fit — requests or stamps nobody can pair.
        overflow: usize,
        /// `PACKET_STATISTICS.tp_drops` over the run.
        tap_drops: u32,
        /// Error-queue entries that were not a `SCM_TSTAMP_SND` timestamp.
        other: usize,
        /// The first `recvmsg` failure other than "nothing there", as errno.
        errno: i32,
    }

    /// The running observer. Dropping it without [`Observer::finish`] stops
    /// the thread and restores the NIC.
    pub struct Observer {
        handoff: Arc<Handoff>,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
        out: Arc<Mutex<Option<Observed>>>,
        _nic: Option<HwConfig>,
    }

    impl Drop for Observer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
        }
    }

    fn os(what: &str) -> io::Error {
        let e = io::Error::last_os_error();
        io::Error::new(e.kind(), format!("w2w: --wire-timestamps: {what}: {e}"))
    }

    fn refused_raw(what: &str) -> io::Error {
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::EPERM) {
            eprintln!("w2w: --wire-timestamps: {what}: {e}.");
            eprintln!("     This process lacks the capability. After each build run once:");
            eprintln!("       sudo -n setcap cap_net_raw,cap_net_admin+ep target/release/w2w");
            eprintln!("     A process started under strace or gdb by an unprivileged tracer is");
            eprintln!(
                "     never given file capabilities (security/commoncap.c), so it lands here"
            );
            eprintln!("     too. Nothing runs without the tap.");
        }
        io::Error::new(e.kind(), format!("w2w: --wire-timestamps: {what}: {e}"))
    }

    fn ifreq_for(nic: &str) -> io::Result<libc::ifreq> {
        // SAFETY: `ifreq` is plain data; all-zero is a valid value of it.
        let mut ifr: libc::ifreq = unsafe { zeroed() };
        if nic.len() >= ifr.ifr_name.len() || nic.bytes().any(|b| b == 0) {
            return Err(io::Error::other(format!(
                "w2w: --nic {nic}: not an interface name"
            )));
        }
        for (d, s) in ifr.ifr_name.iter_mut().zip(nic.bytes()) {
            *d = s as libc::c_char;
        }
        Ok(ifr)
    }

    fn setsockopt<T>(fd: RawFd, level: i32, name: i32, v: &T) -> i32 {
        // SAFETY: `v` is a live `T` for the call and its exact size is passed;
        // the kernel copies it in and keeps no pointer. Proven with the module
        // note's list.
        unsafe {
            libc::setsockopt(
                fd,
                level,
                name,
                (v as *const T).cast(),
                size_of::<T>() as libc::socklen_t,
            )
        }
    }

    /// The NIC's timestamping configuration, set for the run and put back.
    struct HwConfig {
        ctl: OwnedFd,
        nic: String,
        old: libc::hwtstamp_config,
    }

    impl HwConfig {
        fn hwtstamp(
            ctl: &OwnedFd,
            nic: &str,
            req: libc::c_ulong,
            cfg: &mut libc::hwtstamp_config,
        ) -> i32 {
            let Ok(mut ifr) = ifreq_for(nic) else {
                return -1;
            };
            ifr.ifr_ifru.ifru_data = (cfg as *mut libc::hwtstamp_config).cast();
            // SAFETY: `ifr` names the interface and points at `cfg`, which
            // outlives the call; SIOC[GS]HWTSTAMP read and write exactly one
            // `hwtstamp_config` through it.
            unsafe { libc::ioctl(ctl.as_raw_fd(), req, &mut ifr) }
        }

        fn apply(nic: &str) -> io::Result<Self> {
            // SAFETY: plain `socket(2)`; the result is checked before use.
            let raw =
                unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
            if raw < 0 {
                return Err(os("socket(AF_INET) for the NIC's ioctls"));
            }
            // SAFETY: `raw` is a descriptor this function just opened and owns.
            let ctl = unsafe { OwnedFd::from_raw_fd(raw) };
            // SAFETY: all-zero is a valid `hwtstamp_config`.
            let mut old: libc::hwtstamp_config = unsafe { zeroed() };
            if Self::hwtstamp(&ctl, nic, libc::SIOCGHWTSTAMP, &mut old) < 0 {
                return Err(os(&format!("SIOCGHWTSTAMP on {nic}")));
            }
            println!(
                "hwtstamp {nic} before: tx_type {} rx_filter {}",
                old.tx_type, old.rx_filter
            );
            // SAFETY: as above.
            let mut want: libc::hwtstamp_config = unsafe { zeroed() };
            want.tx_type = libc::HWTSTAMP_TX_ON as libc::c_int;
            want.rx_filter = libc::HWTSTAMP_FILTER_ALL as libc::c_int;
            if Self::hwtstamp(&ctl, nic, libc::SIOCSHWTSTAMP, &mut want) < 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::EPERM) {
                    eprintln!("w2w: --wire-timestamps: SIOCSHWTSTAMP on {nic}: {e}.");
                    eprintln!(
                        "     It needs CAP_NET_ADMIN (net/core/dev_ioctl.c). After each build:"
                    );
                    eprintln!(
                        "       sudo -n setcap cap_net_raw,cap_net_admin+ep target/release/w2w"
                    );
                }
                return Err(io::Error::new(
                    e.kind(),
                    format!("w2w: --wire-timestamps: SIOCSHWTSTAMP on {nic}: {e}"),
                ));
            }
            let this = Self {
                ctl,
                nic: nic.to_string(),
                old,
            };
            // SAFETY: as above.
            let mut back: libc::hwtstamp_config = unsafe { zeroed() };
            let got = Self::hwtstamp(&this.ctl, nic, libc::SIOCGHWTSTAMP, &mut back);
            println!(
                "hwtstamp {nic} set: tx_type {} rx_filter {} (answer), tx_type {} rx_filter {} \
                 (read back) — a driver's read-back is not evidence; each sample's ts[2] is",
                want.tx_type, want.rx_filter, back.tx_type, back.rx_filter
            );
            let on = |c: &libc::hwtstamp_config| {
                c.tx_type == libc::HWTSTAMP_TX_ON as libc::c_int
                    && c.rx_filter == libc::HWTSTAMP_FILTER_ALL as libc::c_int
            };
            if got < 0 || !on(&want) || !on(&back) {
                // `this` drops here and puts the old configuration back.
                return Err(io::Error::other(format!(
                    "w2w: --wire-timestamps: {nic} did not take tx_type ON and rx_filter ALL"
                )));
            }
            Ok(this)
        }
    }

    impl Drop for HwConfig {
        fn drop(&mut self) {
            let mut old = self.old;
            let rc = Self::hwtstamp(&self.ctl, &self.nic, libc::SIOCSHWTSTAMP, &mut old);
            if rc < 0 {
                eprintln!(
                    "w2w: could not restore {}'s timestamping (tx_type {} rx_filter {}): {}",
                    self.nic,
                    self.old.tx_type,
                    self.old.rx_filter,
                    io::Error::last_os_error()
                );
            } else {
                println!(
                    "hwtstamp {} restored: tx_type {} rx_filter {}",
                    self.nic, old.tx_type, old.rx_filter
                );
            }
        }
    }

    /// The three timespecs of an `SCM_TIMESTAMPING` control message, in ns,
    /// or all zero when the message is absent — which is how the kernel says
    /// "no stamp" when no software reporting flag is set.
    ///
    /// # Safety
    ///
    /// `msg` must be a `msghdr` a successful `recvmsg` just filled, whose
    /// `msg_control` buffer is still live.
    unsafe fn walk(
        msg: &libc::msghdr,
        mut on_err: impl FnMut(&libc::sock_extended_err),
    ) -> [u64; 3] {
        let mut ts = [0u64; 3];
        let tsz = size_of::<libc::timespec>();
        // SAFETY: the caller's contract; `CMSG_FIRSTHDR`/`CMSG_NXTHDR` stay
        // inside `msg_controllen` and return null at its end.
        let mut c = unsafe { libc::CMSG_FIRSTHDR(msg) };
        while !c.is_null() {
            // SAFETY: non-null and inside the control buffer, per the above.
            let h = unsafe { &*c };
            // SAFETY: `CMSG_DATA` of a live header.
            let data = unsafe { libc::CMSG_DATA(c) };
            let len = h.cmsg_len as usize;
            // SAFETY: `CMSG_LEN` is arithmetic.
            let need = |n: usize| len >= unsafe { libc::CMSG_LEN(n as u32) } as usize;
            if h.cmsg_level == libc::SOL_SOCKET
                && h.cmsg_type == libc::SCM_TIMESTAMPING
                && need(3 * tsz)
            {
                for (i, slot) in ts.iter_mut().enumerate() {
                    // SAFETY: `need(3 * tsz)` holds, so three timespecs are
                    // inside the message; unaligned, so read unaligned.
                    let t: libc::timespec =
                        unsafe { std::ptr::read_unaligned(data.add(i * tsz).cast()) };
                    *slot = u64::try_from(t.tv_sec).unwrap_or(0) * 1_000_000_000
                        + u64::try_from(t.tv_nsec).unwrap_or(0);
                }
            } else if ((h.cmsg_level == libc::SOL_IP && h.cmsg_type == libc::IP_RECVERR)
                || (h.cmsg_level == libc::SOL_IPV6 && h.cmsg_type == libc::IPV6_RECVERR))
                && need(size_of::<libc::sock_extended_err>())
            {
                // SAFETY: the length was checked just above.
                let e: libc::sock_extended_err = unsafe { std::ptr::read_unaligned(data.cast()) };
                on_err(&e);
            }
            // SAFETY: `c` is a header of this message.
            c = unsafe { libc::CMSG_NXTHDR(msg, c) };
        }
        ts
    }

    impl Observer {
        /// Open the tap on `args.nic`, set the NIC up (unless it is loopback),
        /// and start the observer on `args.observer_core`. `engine` is the
        /// engine's bound address; its port is what the tap keeps.
        /// `engine_stop` is the engine thread's `stop`: the observer sets it
        /// when SIGINT or SIGTERM arrives (see `super::signal`).
        pub fn start(
            args: &WireArgs,
            mode: Mode,
            engine: SocketAddr,
            capacity: usize,
            engine_stop: Arc<AtomicBool>,
        ) -> io::Result<Self> {
            let SocketAddr::V4(v4) = engine else {
                return Err(io::Error::other(
                    "w2w: --wire-timestamps reads IPv4 frames; listen on an IPv4 address",
                ));
            };
            let port = v4.port();
            let nic = args.nic.as_str();
            let cname = std::ffi::CString::new(nic).map_err(|_| {
                io::Error::other(format!("w2w: --nic {nic}: not an interface name"))
            })?;
            // SAFETY: a NUL-terminated string that outlives the call.
            let ifindex = unsafe { libc::if_nametoindex(cname.as_ptr()) };
            if ifindex == 0 {
                return Err(os(&format!("--nic {nic}")));
            }

            // Loopback or not, asked on a plain `AF_INET` socket **before**
            // anything that needs a capability: the `standard` refusal below
            // must be reachable by a binary with none (Sửa 2, Điều 3's
            // reversal).
            // SAFETY: plain `socket(2)`; checked before use.
            let raw =
                unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
            if raw < 0 {
                return Err(os("socket(AF_INET) for SIOCGIFFLAGS"));
            }
            // SAFETY: just opened, owned here.
            let ctl = unsafe { OwnedFd::from_raw_fd(raw) };
            let mut ifr = ifreq_for(nic)?;
            // SAFETY: SIOCGIFFLAGS reads the name and writes `ifru_flags`
            // inside `ifr`, which outlives the call.
            if unsafe { libc::ioctl(ctl.as_raw_fd(), libc::SIOCGIFFLAGS, &mut ifr) } < 0 {
                return Err(os(&format!("SIOCGIFFLAGS on {nic}")));
            }
            drop(ctl);
            // SAFETY: SIOCGIFFLAGS just wrote this member of the union.
            let loopback = i32::from(unsafe { ifr.ifr_ifru.ifru_flags }) & libc::IFF_LOOPBACK != 0;
            super::blocking_mode_on_hardware(mode, nic, loopback).map_err(|why| {
                eprintln!("w2w: {why}");
                io::Error::other(format!("w2w: {why}"))
            })?;

            // Before the NIC is touched: from here to the end of the run a
            // SIGINT or SIGTERM unwinds, and `HwConfig`'s `Drop` runs.
            super::signal::install()?;

            // SAFETY: plain `socket(2)`; checked before use.
            let raw =
                unsafe { libc::socket(libc::AF_PACKET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
            if raw < 0 {
                return Err(refused_raw("socket(AF_PACKET)"));
            }
            // SAFETY: just opened, owned here.
            let tap = unsafe { OwnedFd::from_raw_fd(raw) };
            let hw = if loopback {
                println!(
                    "hwtstamp {nic}: loopback, no hardware clock — SIOCSHWTSTAMP not attempted; \
                     every stamp will be counted missing and no wire column printed"
                );
                None
            } else {
                Some(HwConfig::apply(nic)?)
            };

            let fd = tap.as_raw_fd();
            let one: libc::c_int = 1;
            if setsockopt(fd, libc::SOL_PACKET, libc::PACKET_IGNORE_OUTGOING, &one) < 0 {
                return Err(os("PACKET_IGNORE_OUTGOING"));
            }
            let rx: libc::c_uint =
                libc::SOF_TIMESTAMPING_RX_HARDWARE | libc::SOF_TIMESTAMPING_RAW_HARDWARE;
            if setsockopt(fd, libc::SOL_SOCKET, libc::SO_TIMESTAMPING, &rx) < 0 {
                return Err(os("SO_TIMESTAMPING on the tap"));
            }
            // Classic BPF over the network header (SOCK_DGRAM): IPv4 TCP, not
            // a fragment, destination port `port`; keep `SNAP` bytes. The same
            // test is made again in `pair::parse_ipv4_tcp` — this one is only
            // so the kernel does not queue what nobody reads.
            let st = |code: u32, jt: u8, jf: u8, k: u32| libc::sock_filter {
                code: code as u16,
                jt,
                jf,
                k,
            };
            let mut prog = [
                st(libc::BPF_LD | libc::BPF_B | libc::BPF_ABS, 0, 0, 9),
                st(libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K, 0, 6, 6),
                st(libc::BPF_LD | libc::BPF_H | libc::BPF_ABS, 0, 0, 6),
                st(libc::BPF_JMP | libc::BPF_JSET | libc::BPF_K, 4, 0, 0x1fff),
                st(libc::BPF_LDX | libc::BPF_B | libc::BPF_MSH, 0, 0, 0),
                st(libc::BPF_LD | libc::BPF_H | libc::BPF_IND, 0, 0, 2),
                st(
                    libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K,
                    0,
                    1,
                    u32::from(port),
                ),
                st(libc::BPF_RET | libc::BPF_K, 0, 0, SNAP as u32),
                st(libc::BPF_RET | libc::BPF_K, 0, 0, 0),
            ];
            let fprog = libc::sock_fprog {
                len: prog.len() as u16,
                filter: prog.as_mut_ptr(),
            };
            if setsockopt(fd, libc::SOL_SOCKET, libc::SO_ATTACH_FILTER, &fprog) < 0 {
                return Err(os("SO_ATTACH_FILTER on the tap"));
            }
            let rcvbuf: libc::c_int = 8 << 20;
            let _ = setsockopt(fd, libc::SOL_SOCKET, libc::SO_RCVBUF, &rcvbuf);
            // SAFETY: all-zero is a valid `sockaddr_ll`.
            let mut sll: libc::sockaddr_ll = unsafe { zeroed() };
            sll.sll_family = libc::AF_PACKET as u16;
            sll.sll_protocol = (libc::ETH_P_IP as u16).to_be();
            sll.sll_ifindex = ifindex as i32;
            // SAFETY: `sll` is a live `sockaddr_ll` and its size is passed.
            let rc = unsafe {
                libc::bind(
                    fd,
                    (&sll as *const libc::sockaddr_ll).cast(),
                    size_of::<libc::sockaddr_ll>() as libc::socklen_t,
                )
            };
            if rc < 0 {
                return Err(os(&format!("bind the tap to {nic}")));
            }
            // Zero the drop counter: PACKET_STATISTICS resets on read.
            let _ = tap_drops(fd);

            // Every record the run can make, allocated and touched now — the
            // observer's loop allocates nothing, and the counter would say so.
            let mut frames = Vec::with_capacity(capacity);
            frames.resize(capacity, Inbound::default());
            frames.clear();
            let mut sent = Vec::with_capacity(capacity);
            sent.resize(capacity, Sent::default());
            sent.clear();

            let handoff = Arc::new(Handoff::new());
            let stop = Arc::new(AtomicBool::new(false));
            let out = Arc::new(Mutex::new(None));
            let observed = Observed {
                nic: nic.to_string(),
                loopback,
                attached: false,
                peer: 0,
                extra: 0,
                frames,
                sent,
                overflow: 0,
                tap_drops: 0,
                other: 0,
                errno: 0,
            };
            let body = {
                let (h, s, o) = (Arc::clone(&handoff), Arc::clone(&stop), Arc::clone(&out));
                move || observe(tap, port, &h, &s, &engine_stop, &o, observed)
            };
            let (thread, on) = spawn_observer(args.observer_core, body)?;
            println!("wire-timestamps: {nic}, port {port}");
            println!("observer-core: {on}");
            Ok(Self {
                handoff,
                stop,
                thread: Some(thread),
                out,
                _nic: hw,
            })
        }

        /// The engine thread's end of the handoff.
        pub fn handoff(&self) -> Arc<Handoff> {
            Arc::clone(&self.handoff)
        }

        /// Drain, stop and join the observer; the NIC is restored when `self`
        /// drops at the end of this call.
        pub fn finish(mut self) -> io::Result<Observed> {
            self.stop.store(true, Ordering::Release);
            if let Some(t) = self.thread.take() {
                t.join()
                    .map_err(|_| io::Error::other("w2w: the observer thread panicked"))?;
            }
            let got = self
                .out
                .lock()
                .map_err(|_| io::Error::other("w2w: the observer's results were poisoned"))?
                .take();
            got.ok_or_else(|| io::Error::other("w2w: the observer left no results"))
        }
    }

    /// The observer thread, pinned from inside and read back as the engine
    /// is (ADR-0015). Two definitions, like `spawn_engine`: `main` refuses
    /// `--observer-core` on a build that cannot pin, so the second is never
    /// reached — and refuses rather than spin unpinned if it ever is.
    #[cfg(feature = "affinity")]
    fn spawn_observer<F: FnOnce() + Send + 'static>(
        core: usize,
        body: F,
    ) -> io::Result<(std::thread::JoinHandle<()>, String)> {
        use fixbolt_engine::affinity::{CoreId, spawn_pinned};
        let (t, on) = spawn_pinned("w2w-observer", CoreId(core), body)?;
        Ok((t, on.to_string()))
    }

    #[cfg(not(feature = "affinity"))]
    fn spawn_observer<F: FnOnce() + Send + 'static>(
        _core: usize,
        _body: F,
    ) -> io::Result<(std::thread::JoinHandle<()>, String)> {
        Err(io::Error::other(
            "w2w: --observer-core needs `--features affinity`",
        ))
    }

    fn tap_drops(fd: RawFd) -> u32 {
        // SAFETY: all-zero is a valid `tpacket_stats`.
        let mut st: libc::tpacket_stats = unsafe { zeroed() };
        let mut len = size_of::<libc::tpacket_stats>() as libc::socklen_t;
        // SAFETY: `st` and `len` are live and `len` is its size.
        let rc = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_PACKET,
                libc::PACKET_STATISTICS,
                (&mut st as *mut libc::tpacket_stats).cast(),
                &mut len,
            )
        };
        if rc < 0 { u32::MAX } else { st.tp_drops }
    }

    /// The observer's side of the handoff: `dup`, stamp, read the peer's port.
    /// Takes the [`Claim`] by reference, so it cannot outlive it.
    fn stamp(claim: &Claim<'_>) -> Result<(OwnedFd, u16), io::Error> {
        let fd = claim.fd();
        // SAFETY: `fd` is the engine's accepted socket, and the claim is what
        // keeps it live: `Handoff::claim` moved OFFERED→CLAIMED by CAS, so the
        // engine's timeout (OFFERED→FAILED, also a CAS) can no longer fire, and
        // from CLAIMED `Handoff::attach` does not return — the engine keeps its
        // `TcpTransport`, and so this descriptor, until the claim resolves after
        // this call. Only `fcntl` touches `fd`; everything after uses the `dup`.
        // Proven by `an_unclaimed_offer_times_out_and_can_no_longer_be_claimed`,
        // `a_claimed_offer_waits_past_the_deadline_for_its_answer` and
        // `an_abandoned_claim_fails_the_offer` in `mod wire`'s tests.
        let dup = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
        if dup < 0 {
            return Err(os("dup of the engine's socket"));
        }
        // SAFETY: just created, owned here.
        let dup = unsafe { OwnedFd::from_raw_fd(dup) };
        let flags: libc::c_uint = libc::SOF_TIMESTAMPING_TX_HARDWARE
            | libc::SOF_TIMESTAMPING_RAW_HARDWARE
            | libc::SOF_TIMESTAMPING_OPT_ID
            | libc::SOF_TIMESTAMPING_OPT_ID_TCP
            | libc::SOF_TIMESTAMPING_OPT_TSONLY;
        if setsockopt(
            dup.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_TIMESTAMPING,
            &flags,
        ) < 0
        {
            return Err(os("SO_TIMESTAMPING on the engine's socket"));
        }
        // SAFETY: all-zero is a valid `sockaddr_storage`.
        let mut ss: libc::sockaddr_storage = unsafe { zeroed() };
        let mut len = size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        // SAFETY: `ss` is large enough for any address and `len` says so.
        if unsafe {
            libc::getpeername(
                dup.as_raw_fd(),
                (&mut ss as *mut libc::sockaddr_storage).cast(),
                &mut len,
            )
        } < 0
        {
            return Err(os("getpeername of the engine's socket"));
        }
        let port = match i32::from(ss.ss_family) {
            libc::AF_INET => {
                // SAFETY: the family says this is a `sockaddr_in`.
                let sin: libc::sockaddr_in =
                    unsafe { std::ptr::read((&ss as *const libc::sockaddr_storage).cast()) };
                u16::from_be(sin.sin_port)
            }
            _ => {
                return Err(io::Error::other(
                    "w2w: --wire-timestamps: the peer is not IPv4",
                ));
            }
        };
        Ok((dup, port))
    }

    /// The observer thread. Spins: a waiter on the tap would be woken from the
    /// receive softirq **before** TCP delivery, which is inside the figure.
    #[allow(clippy::needless_pass_by_value)]
    fn observe(
        tap: OwnedFd,
        port: u16,
        h: &Handoff,
        stop: &AtomicBool,
        engine_stop: &AtomicBool,
        out: &Mutex<Option<Observed>>,
        mut o: Observed,
    ) {
        let mut err: Option<OwnedFd> = None;
        let mut buf = [0u8; SNAP];
        let mut ctl = [0u64; 64];
        let mut stopping: Option<Instant> = None;
        let mut quiet_since: Option<Instant> = None;
        loop {
            let mut got = false;
            // SIGINT or SIGTERM: stop the engine, which is what ends the run
            // normally too. This thread keeps going until `finish` stops it.
            super::signal::relay(engine_stop);
            // Claimed before the descriptor is touched — see `stamp`'s SAFETY.
            if let Some(claim) = h.claim() {
                match stamp(&claim) {
                    Ok((fd, peer)) => {
                        h.peer.store(peer, Ordering::Release);
                        o.peer = peer;
                        o.attached = true;
                        err = Some(fd);
                        claim.resolve(true);
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        claim.resolve(false);
                    }
                }
            }

            // The tap: every frame waiting.
            loop {
                let mut iov = libc::iovec {
                    iov_base: buf.as_mut_ptr().cast(),
                    iov_len: buf.len(),
                };
                // SAFETY: all-zero is a valid `msghdr`.
                let mut msg: libc::msghdr = unsafe { zeroed() };
                msg.msg_iov = &mut iov;
                msg.msg_iovlen = 1;
                msg.msg_control = ctl.as_mut_ptr().cast();
                msg.msg_controllen = size_of_val(&ctl);
                // SAFETY: `msg` points at `iov` → `buf` and at `ctl`, all live
                // on this stack for the call, with their true lengths.
                let n = unsafe { libc::recvmsg(tap.as_raw_fd(), &mut msg, libc::MSG_DONTWAIT) };
                if n < 0 {
                    note(&mut o);
                    break;
                }
                got = true;
                // SAFETY: `recvmsg` succeeded and `ctl` is live.
                let ts = unsafe { walk(&msg, |_| {}) };
                let len = usize::try_from(n).unwrap_or(0).min(buf.len());
                if let Some(f) = pair::parse_ipv4_tcp(&buf[..len], port, ts) {
                    if o.frames.len() < o.frames.capacity() {
                        o.frames.push(f);
                    } else {
                        o.overflow += 1;
                    }
                }
            }

            // The engine socket's error queue: every stamp waiting.
            if let Some(fd) = &err {
                loop {
                    // SAFETY: all-zero is a valid `msghdr`; no payload is
                    // asked for (`OPT_TSONLY`), only control messages.
                    let mut msg: libc::msghdr = unsafe { zeroed() };
                    msg.msg_control = ctl.as_mut_ptr().cast();
                    msg.msg_controllen = size_of_val(&ctl);
                    // SAFETY: `msg` points at `ctl`, live for the call.
                    let n = unsafe {
                        libc::recvmsg(
                            fd.as_raw_fd(),
                            &mut msg,
                            libc::MSG_ERRQUEUE | libc::MSG_DONTWAIT,
                        )
                    };
                    if n < 0 {
                        note(&mut o);
                        break;
                    }
                    got = true;
                    let mut key = None;
                    // SAFETY: `recvmsg` succeeded and `ctl` is live.
                    let ts = unsafe {
                        walk(&msg, |e| {
                            if e.ee_errno == libc::ENOMSG as u32
                                && e.ee_origin == libc::SO_EE_ORIGIN_TIMESTAMPING
                                && e.ee_info == SCM_TSTAMP_SND
                            {
                                key = Some(e.ee_data);
                            }
                        })
                    };
                    match key {
                        Some(key) if o.sent.len() < o.sent.capacity() => {
                            o.sent.push(Sent { ts, key })
                        }
                        Some(_) => o.overflow += 1,
                        None => o.other += 1,
                    }
                }
            }

            if stop.load(Ordering::Acquire) {
                let now = Instant::now();
                let began = *stopping.get_or_insert(now);
                if got {
                    quiet_since = None;
                }
                let quiet = *quiet_since.get_or_insert(now);
                if now.duration_since(quiet) >= QUIET || now.duration_since(began) >= DRAIN_MAX {
                    break;
                }
            }
            std::hint::spin_loop();
        }
        o.tap_drops = tap_drops(tap.as_raw_fd());
        o.extra = h.extra.load(Ordering::Relaxed);
        if let Ok(mut slot) = out.lock() {
            *slot = Some(o);
        }
    }

    /// A `recvmsg` that returned nothing: "nothing waiting" is the spin's
    /// normal case; anything else is kept and printed.
    fn note(o: &mut Observed) {
        let e = io::Error::last_os_error().raw_os_error().unwrap_or(0);
        if e != libc::EAGAIN && e != libc::EINTR && o.errno == 0 {
            o.errno = e;
        }
    }

    impl Observed {
        /// Print the wire section. `Err` when the tap cannot be trusted to line
        /// up with the run — which on a plain arm means the tap is broken, and
        /// must fail the run rather than print counts about something else.
        pub fn report(&self, window: Window) -> io::Result<()> {
            println!(
                "     wire   NIC in -> NIC out at the acceptor, {} hardware stamps",
                self.nic
            );
            if self.errno != 0 {
                return Err(io::Error::other(format!(
                    "w2w: --wire-timestamps: a read failed: {}",
                    io::Error::from_raw_os_error(self.errno)
                )));
            }
            if !self.attached {
                return Err(io::Error::other(
                    "w2w: --wire-timestamps: the observer never stamped the engine's socket",
                ));
            }
            let mut reqs = Vec::new();
            if pair::requests(&self.frames, self.peer, &mut reqs).is_none() {
                let hint = match (&window, self.loopback) {
                    (Window::Combined { .. }, false) => {
                        "; the combined run is over loopback, so a hardware NIC never carries \
                         it — run the engine with --listen on that NIC's address"
                    }
                    _ => "",
                };
                return Err(io::Error::other(format!(
                    "w2w: --wire-timestamps: the tap on {} never saw the TCP handshake from port {} \
                     — it is not watching the connection it was meant to{hint}",
                    self.nic, self.peer
                )));
            }
            let (range, label) = match window {
                Window::Combined {
                    warmup,
                    n,
                    tls: false,
                } => {
                    let expected = 1 + warmup + n;
                    if reqs.len() != expected {
                        return Err(io::Error::other(format!(
                            "w2w: --wire-timestamps: the tap saw {} requests from port {}, and the \
                             run sent {expected} (logon 1 + warmup {warmup} + timed {n})",
                            reqs.len(),
                            self.peer
                        )));
                    }
                    (1 + warmup..expected, "the timed window".to_string())
                }
                Window::Combined { tls: true, .. } => (
                    0..reqs.len(),
                    "every request, TLS handshake records included — no window on a TLS arm"
                        .to_string(),
                ),
                Window::Listen { skip } => {
                    // The logon, then `skip` requests the generator called warmup.
                    if 1 + skip > reqs.len() {
                        return Err(io::Error::other(format!(
                            "w2w: --wire-timestamps: the tap saw {} requests from port {}, and \
                             --warmup {skip} leaves out more than the logon and every one after it",
                            reqs.len(),
                            self.peer
                        )));
                    }
                    let label = if skip == 0 {
                        "every request after the logon; --warmup 0, so the generator's warmup \
                         is included"
                            .to_string()
                    } else {
                        format!("every request after the logon, leaving out the first {skip}")
                    };
                    (1 + skip..reqs.len(), label)
                }
            };
            let mut sent = self.sent.clone();
            sent.sort_unstable_by_key(|s| s.key);
            let mut wire = Vec::with_capacity(range.len());
            let t = pair::pair(&reqs, range, &sent, &mut wire);
            println!(
                "     requests       {:>9}   ({label}; {} seen on the tap)",
                t.requests,
                reqs.len()
            );
            println!(
                "     hw-rx-missing  {:>9}   of {}",
                t.rx_missing, t.requests
            );
            println!(
                "     hw-tx-missing  {:>9}   of {}",
                t.tx_missing, t.requests
            );
            println!(
                "     tx stamps seen {:>9}   (engine socket's error queue, every write)",
                sent.len()
            );
            if self.other > 0 {
                println!(
                    "     errqueue other {:>9}   (entries that were not a TX timestamp)",
                    self.other
                );
            }
            if t.reversed > 0 {
                println!(
                    "     reversed       {:>9}   (reply stamped before its request; not figures)",
                    t.reversed
                );
            }
            if self.extra > 0 {
                println!(
                    "     unstamped conns {:>8}   (only the first connection is stamped)",
                    self.extra
                );
            }
            println!("     tap drops      {:>9}", self.tap_drops);
            if self.overflow > 0 {
                println!(
                    "     overflow       {:>9}   (records that did not fit)",
                    self.overflow
                );
            }
            if self.tap_drops != 0 || self.overflow != 0 {
                println!(
                    "     no wire column: the tap lost frames or records, so a reply could be"
                );
                println!("     paired against a request that was not its own");
            } else if wire.is_empty() {
                println!(
                    "     no wire column: no request had both a hardware RX and a hardware TX stamp"
                );
                if self.loopback {
                    println!(
                        "     ({} is loopback and has no hardware clock: expected)",
                        self.nic
                    );
                } else {
                    println!(
                        "     ON A HARDWARE NIC. See the plan's igb trap: `ethtool -T`, link down/up,"
                    );
                    println!("     and record the kernel version before trusting any read-back.");
                }
            } else {
                wire.sort_unstable();
                let pick = |q: f64| wire[((wire.len() as f64 - 1.0) * q) as usize];
                println!(
                    "     wire over {} requests with both hardware stamps",
                    wire.len()
                );
                println!("     wire p50   {:>9} ns", pick(0.50));
                println!("     wire p99   {:>9} ns", pick(0.99));
                println!("     wire p99.9 {:>9} ns", pick(0.999));
            }
            Ok(())
        }
    }

    /// The handoff's state machine, without a socket: the engine side is
    /// `offer` + `answer`, the observer side `claim` + `Claim::resolve`, which
    /// is all `attach` and `observe` do with it. `stamp`'s SAFETY note names
    /// these three.
    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use super::{Answer, Handoff};

        #[test]
        fn an_unclaimed_offer_times_out_and_can_no_longer_be_claimed() {
            let h = Handoff::new();
            assert!(h.offer(7));
            assert_eq!(h.answer(false), Answer::Waiting);
            assert_eq!(h.answer(true), Answer::TimedOut);
            // The engine has given up and will drop the socket: the observer
            // must not get the descriptor.
            assert!(h.claim().is_none());
            assert_eq!(h.answer(true), Answer::Failed);
        }

        #[test]
        fn a_claimed_offer_waits_past_the_deadline_for_its_answer() {
            for ok in [true, false] {
                let h = Handoff::new();
                assert!(h.offer(7));
                let claim = h.claim().expect("an offered descriptor can be claimed");
                assert_eq!(claim.fd(), 7);
                // Past the deadline, and the engine still waits: the observer
                // is using the descriptor.
                assert_eq!(h.answer(true), Answer::Waiting);
                assert!(h.claim().is_none(), "one claim per offer");
                claim.resolve(ok);
                let want = if ok { Answer::Attached } else { Answer::Failed };
                assert_eq!(h.answer(true), want);
            }
        }

        #[test]
        fn an_abandoned_claim_fails_the_offer() {
            let h = Handoff::new();
            assert!(h.offer(7));
            drop(h.claim().expect("claimed"));
            assert_eq!(h.answer(false), Answer::Failed);
        }

        #[test]
        fn only_the_first_connection_is_offered() {
            let h = Handoff::new();
            assert!(h.offer(7));
            assert!(!h.offer(8));
            h.claim().expect("claimed").resolve(true);
            assert!(!h.offer(9));
        }
    }
}

/// SIGINT and SIGTERM, for `--wire-timestamps` only: the run ends through the
/// path it ends through anyway, so every `Drop` runs — above all the NIC's
/// [`wire`] `HwConfig`, which puts the timestamping configuration back.
///
/// Without it, a `--listen` waiting for a generator that never comes, stopped
/// with Ctrl-C or by a script's `kill`, died with `tx_type ON` / `rx_filter
/// ALL` left on the NIC, and the next run read that back as "before".
///
/// **The handler stores one atomic and does nothing else** — async-signal-safe
/// by construction. The observer thread, which exists exactly when the handler
/// does and spins anyway, reads it every turn and sets the engine's `stop`
/// ( `relay` ); the engine loop already returns on `stop`, `measure` and
/// `engine_half` join as they always do, and `main` reports the signal. Nothing
/// on the engine thread changes. `SA_RESTART`, so a blocking call elsewhere
/// is not handed an `EINTR` it would report as a failure of its own;
/// `SA_RESETHAND`, so a second Ctrl-C kills at once if the first is not enough.
/// With no `--wire-timestamps` nothing is installed and both signals keep their
/// default action.
#[cfg(target_os = "linux")]
mod signal {
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

    /// The last signal caught, `0` for none.
    static CAUGHT: AtomicI32 = AtomicI32::new(0);
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    extern "C" fn on_signal(sig: libc::c_int) {
        CAUGHT.store(sig, Ordering::Release);
    }

    /// Install the handler for SIGINT and SIGTERM.
    pub fn install() -> std::io::Result<()> {
        for sig in [libc::SIGINT, libc::SIGTERM] {
            // SAFETY: all-zero is a valid `sigaction`; `sigemptyset` fills the
            // mask it is given, and `sigaction` copies `sa` in. The handler it
            // installs performs one atomic store, which is async-signal-safe.
            // Proven by `a_caught_signal_is_relayed_to_the_engine_stop` (a real
            // `raise`, which would kill the test without it) and by the
            // `timeout -s INT` run of `--listen --wire-timestamps` recorded
            // with this change, which exits through `main` rather than by the
            // signal.
            let rc = unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
                sa.sa_flags = libc::SA_RESTART | libc::SA_RESETHAND;
                libc::sigemptyset(&mut sa.sa_mask);
                libc::sigaction(sig, &sa, std::ptr::null_mut())
            };
            if rc < 0 {
                let e = std::io::Error::last_os_error();
                return Err(std::io::Error::new(
                    e.kind(),
                    format!("w2w: --wire-timestamps: sigaction: {e}"),
                ));
            }
        }
        INSTALLED.store(true, Ordering::Release);
        Ok(())
    }

    /// Whether [`install`] ran — `measure`'s hold asks, so that a run with no
    /// handler sleeps exactly as it did.
    pub fn installed() -> bool {
        INSTALLED.load(Ordering::Acquire)
    }

    /// The signal that ended the run, if one did.
    pub fn caught() -> Option<i32> {
        match CAUGHT.load(Ordering::Acquire) {
            0 => None,
            sig => Some(sig),
        }
    }

    /// If a signal was caught, tell the engine to stop. `true` if one was.
    pub fn relay(engine_stop: &AtomicBool) -> bool {
        let hit = caught().is_some();
        if hit {
            engine_stop.store(true, Ordering::Relaxed);
        }
        hit
    }

    pub const fn name(sig: i32) -> &'static str {
        match sig {
            libc::SIGINT => "SIGINT",
            libc::SIGTERM => "SIGTERM",
            _ => "a signal",
        }
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use std::sync::atomic::{AtomicBool, Ordering};

        /// The one test in this binary that touches the handler: a real
        /// `raise(SIGTERM)`, which kills the test process if the handler is
        /// not installed.
        #[test]
        fn a_caught_signal_is_relayed_to_the_engine_stop() {
            let stop = AtomicBool::new(false);
            assert!(!super::relay(&stop));
            assert!(!stop.load(Ordering::Relaxed));
            super::install().unwrap();
            assert!(super::installed());
            // SAFETY: `raise` is a plain libc call; the handler it runs stores
            // one atomic.
            assert_eq!(unsafe { libc::raise(libc::SIGTERM) }, 0);
            assert_eq!(super::caught(), Some(libc::SIGTERM));
            assert!(super::relay(&stop));
            assert!(stop.load(Ordering::Relaxed));
            assert_eq!(super::name(libc::SIGTERM), "SIGTERM");
        }
    }
}

/// Off Linux there is no `--wire-timestamps`, so no handler: a run is never
/// stopped by one.
#[cfg(not(target_os = "linux"))]
mod signal {
    pub const fn installed() -> bool {
        false
    }
    pub const fn caught() -> Option<i32> {
        None
    }
    pub const fn name(_: i32) -> &'static str {
        "a signal"
    }
}

/// `--wire-timestamps` does not exist off Linux: `wire_of` refuses it before
/// anything here is reached, and these uninhabited types keep `main` free of
/// `cfg`s.
#[cfg(not(target_os = "linux"))]
mod wire {
    use std::io;
    use std::net::{SocketAddr, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use super::{Mode, WireArgs};

    pub const LISTEN_CAPACITY: usize = 0;

    pub enum Handoff {}
    impl Handoff {
        pub fn attach(&self, _: &TcpStream) -> bool {
            match *self {}
        }
    }

    // Built by `main` on every OS; read only by the Linux `report`.
    #[allow(dead_code)]
    pub enum Window {
        Combined { warmup: usize, n: usize, tls: bool },
        Listen { skip: usize },
    }

    pub enum Observed {}
    impl Observed {
        pub fn report(&self, _: Window) -> io::Result<()> {
            match *self {}
        }
    }

    pub enum Observer {}
    impl Observer {
        pub fn start(
            _: &WireArgs,
            _: Mode,
            _: SocketAddr,
            _: usize,
            _: Arc<AtomicBool>,
        ) -> io::Result<Self> {
            Err(io::Error::other("w2w: --wire-timestamps is Linux-only"))
        }
        pub fn handoff(&self) -> Arc<Handoff> {
            match *self {}
        }
        pub fn finish(self) -> io::Result<Observed> {
            match self {}
        }
    }
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

/// The two halves' refusals and `--interval`'s pacing — step A3a of
/// `docs/plans/2026-09-04-the-second-linux-desk.md`. Pure functions, so no
/// socket and no engine.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn argv(s: &str) -> Vec<String> {
        std::iter::once("w2w")
            .chain(s.split_whitespace())
            .map(String::from)
            .collect()
    }

    #[test]
    fn no_new_flag_is_the_combined_run() {
        for a in [
            "",
            "--messages 300 --warmup 50 --hold-ms 400 --mode hft",
            "--mode standard --tls ktls --engine-core 6 --client-core 7",
        ] {
            assert_eq!(half_of(&argv(a)), Ok(Half::Both), "{a}");
            assert_eq!(interval_of(&argv(a)), Ok(0), "{a}");
        }
    }

    #[test]
    fn each_half_takes_its_address() {
        assert_eq!(
            half_of(&argv(
                "--listen 0.0.0.0:9000 --mode standard --engine-core 6"
            )),
            Ok(Half::Listen("0.0.0.0:9000".into()))
        );
        assert_eq!(
            half_of(&argv(
                "--connect 10.0.0.2:9000 --messages 10 --warmup 1 --hold-ms 5 --interval 1000 --client-core 7"
            )),
            Ok(Half::Connect("10.0.0.2:9000".into()))
        );
    }

    #[test]
    fn listen_and_connect_together_are_refused() {
        let e = half_of(&argv("--listen 127.0.0.1:1 --connect 127.0.0.1:1")).unwrap_err();
        assert!(e.contains("one process runs one of them"), "{e}");
    }

    #[test]
    fn connect_refuses_what_only_an_engine_has() {
        for flag in [
            "--engine-core 6",
            "--mode hft",
            "--journal file-async",
            "--log file",
        ] {
            let e = half_of(&argv(&format!("--connect 127.0.0.1:1 {flag}"))).unwrap_err();
            let name = flag.split(' ').next().unwrap();
            assert!(
                e.starts_with(&format!("{name} does not apply to --connect")),
                "{e}"
            );
        }
    }

    #[test]
    fn listen_refuses_what_only_a_generator_has() {
        for flag in [
            "--client-core 7",
            "--messages 10",
            "--warmup 1",
            "--hold-ms 5",
            "--interval 1000",
        ] {
            let e = half_of(&argv(&format!("--listen 127.0.0.1:1 {flag}"))).unwrap_err();
            let name = flag.split(' ').next().unwrap();
            assert!(
                e.starts_with(&format!("{name} does not apply to --listen")),
                "{e}"
            );
        }
    }

    #[test]
    fn either_half_refuses_tls_but_accepts_off() {
        for half in ["--listen", "--connect"] {
            for arm in ["ktls", "userspace"] {
                let e = half_of(&argv(&format!("{half} 127.0.0.1:1 --tls {arm}"))).unwrap_err();
                assert!(
                    e.starts_with(&format!("--tls {arm} is not available to {half}")),
                    "{e}"
                );
            }
            assert!(half_of(&argv(&format!("{half} 127.0.0.1:1 --tls off"))).is_ok());
        }
    }

    #[test]
    fn a_new_flag_without_its_value_is_refused_not_defaulted() {
        assert_eq!(
            half_of(&argv("--listen")),
            Err("--listen needs a value".into())
        );
        assert_eq!(
            half_of(&argv("--connect --messages 10")),
            Err("--connect needs a value".into())
        );
        assert_eq!(
            interval_of(&argv("--interval")),
            Err("--interval needs a value".into())
        );
        assert_eq!(
            interval_of(&argv("--interval 1ms")),
            Err("--interval 1ms: not a valid value".into())
        );
        assert_eq!(
            interval_of(&argv("--interval -5")),
            Err("--interval -5: not a valid value".into())
        );
        assert_eq!(interval_of(&argv("--interval 250")), Ok(250));
    }

    #[test]
    fn wire_timestamps_are_refused_where_they_do_not_apply() {
        assert_eq!(wire_of(&argv("--mode hft")), Ok(None));
        for flag in ["--wire-timestamps", "--nic lo", "--observer-core 2"] {
            let e = half_of(&argv(&format!("--connect 127.0.0.1:1 {flag}"))).unwrap_err();
            let name = flag.split(' ').next().unwrap();
            assert!(
                e.starts_with(&format!("{name} does not apply to --connect")),
                "{e}"
            );
        }
        let e = wire_of(&argv("--nic lo --observer-core 2")).unwrap_err();
        assert!(e.contains("belong to --wire-timestamps"), "{e}");
        if cfg!(target_os = "linux") {
            let e = wire_of(&argv("--wire-timestamps --observer-core 2")).unwrap_err();
            assert!(e.contains("needs --nic"), "{e}");
            let e = wire_of(&argv("--wire-timestamps --nic lo")).unwrap_err();
            assert!(e.contains("needs --observer-core"), "{e}");
            assert_eq!(
                wire_of(&argv(
                    "--listen 127.0.0.1:0 --wire-timestamps --nic lo --observer-core 2"
                )),
                Ok(Some(WireArgs {
                    nic: "lo".into(),
                    observer_core: 2
                }))
            );
        } else {
            let e = wire_of(&argv("--wire-timestamps --nic lo --observer-core 2")).unwrap_err();
            assert!(e.contains("Linux-only"), "{e}");
        }
    }

    #[test]
    fn listen_takes_warmup_only_beside_wire_timestamps() {
        let with = "--listen 127.0.0.1:0 --warmup 50 --wire-timestamps --nic lo --observer-core 2";
        assert_eq!(half_of(&argv(with)), Ok(Half::Listen("127.0.0.1:0".into())));
        let e = half_of(&argv("--listen 127.0.0.1:0 --warmup 50")).unwrap_err();
        assert!(e.starts_with("--warmup does not apply to --listen"), "{e}");
        // The exception is `--warmup` alone: the generator's other flags stay refused.
        let e = half_of(&argv(
            "--listen 127.0.0.1:0 --messages 10 --wire-timestamps --nic lo --observer-core 2",
        ))
        .unwrap_err();
        assert!(
            e.starts_with("--messages does not apply to --listen"),
            "{e}"
        );
    }

    #[test]
    fn standard_is_refused_on_a_hardware_nic_and_not_on_loopback() {
        let e = blocking_mode_on_hardware(Mode::Standard, "enp9s0", false).unwrap_err();
        assert!(
            e.starts_with("--mode standard --wire-timestamps on enp9s0 is refused")
                && e.contains("POLLERR"),
            "{e}"
        );
        assert_eq!(
            blocking_mode_on_hardware(Mode::Standard, "lo", true),
            Ok(())
        );
        assert_eq!(
            blocking_mode_on_hardware(Mode::Hft, "enp9s0", false),
            Ok(())
        );
        assert_eq!(
            blocking_mode_on_hardware(Mode::Yield, "enp9s0", false),
            Ok(())
        );
    }

    /// What the SIGINT/SIGTERM relay leans on (`signal`): a `--listen` engine
    /// still waiting for its first connection — the wait a Ctrl-C interrupts —
    /// returns once its `stop` is set, both in the mode that spins and in the
    /// one that blocks in `poll`.
    #[test]
    fn a_stop_ends_the_listen_loop_before_any_connection() {
        let modes: &[Mode] = if cfg!(all(feature = "standard", unix)) {
            &[Mode::Hft, Mode::Standard]
        } else {
            &[Mode::Hft]
        };
        for &mode in modes {
            let acceptor = Acceptor::bind("127.0.0.1:0").unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let engine_stop = Arc::clone(&stop);
            let (files, _cleanup) = open_files(JournalKind::Mem, LogKind::None).unwrap();
            let engine = std::thread::spawn(move || {
                serve_chosen::<_, true>(
                    acceptor,
                    &engine_stop,
                    mode,
                    ListenNever,
                    EngineSide::Plain,
                    files,
                    None,
                );
            });
            std::thread::sleep(Duration::from_millis(50));
            assert!(!engine.is_finished(), "{mode:?}: returned before stop");
            stop.store(true, Ordering::Relaxed);
            let deadline = Instant::now() + Duration::from_secs(5);
            while !engine.is_finished() {
                assert!(
                    Instant::now() < deadline,
                    "{mode:?}: still serving 5 s after stop"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            engine.join().unwrap();
        }
    }

    #[test]
    fn allocs_names_every_thread_it_counts() {
        use JournalKind::{FileAsync, Mem};
        use LogKind::{File, None as NoLog};
        // No flag: the words these lines had before any flag existed.
        assert_eq!(counted_threads(true, false, Mem, NoLog), "both threads");
        assert_eq!(counted_threads(false, false, Mem, NoLog), "engine thread");
        // `--wire-timestamps` alone: the words f9abc1a printed.
        assert_eq!(
            counted_threads(true, true, Mem, NoLog),
            "engine, client and observer threads"
        );
        assert_eq!(
            counted_threads(false, true, Mem, NoLog),
            "engine and observer threads"
        );
        // The writer threads are counted, so they are named.
        assert_eq!(
            counted_threads(true, false, FileAsync, NoLog),
            "engine, client and journal writer threads"
        );
        assert_eq!(
            counted_threads(false, false, Mem, File),
            "engine and log writer threads"
        );
        assert_eq!(
            counted_threads(true, true, FileAsync, File),
            "engine, client, observer, journal writer and log writer threads"
        );
    }

    #[test]
    fn a_zero_interval_never_waits() {
        let mut p = Pacer::new(0);
        p.sent(Instant::now());
        p.sent_now();
        assert!(p.next.is_none());
        assert!(!p.wait());
    }

    #[test]
    fn the_next_send_waits_one_interval_after_the_last() {
        let mut p = Pacer::new(2_000);
        let t0 = Instant::now();
        p.sent(t0);
        assert!(!p.wait(), "a send due in 2 ms is not late");
        assert!(t0.elapsed() >= Duration::from_micros(2_000));
    }

    #[test]
    fn an_overdue_send_is_counted_late_and_goes_at_once() {
        let mut p = Pacer::new(1);
        p.sent(Instant::now());
        // The test may sleep; the pacer may not.
        std::thread::sleep(Duration::from_millis(2));
        let before = Instant::now();
        assert!(p.wait());
        assert!(before.elapsed() < Duration::from_millis(1));
    }
}
