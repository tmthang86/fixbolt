//! Can `ktls-core` be driven from a plain non-blocking socket, with no async
//! runtime?
//!
//! That is `ADR-0005` open question 1 and `STATUS.md` open item 10, and it is
//! load-bearing: if the answer is no, ADR-0005's central claim collapses into
//! "userspace rustls only" and `DESIGN.md` D11 has to change.
//!
//! This program answers it by running, not by reading documentation. Every line
//! it prints beginning `PASS`, `FAIL` or `NOTE` is an observation; the plan
//! `docs/plans/2026-08-30-ktls-spike.md` requires that the evidence be bytes
//! that went through a socket, and that at least one observation come from
//! outside this process's own code — `/proc/net/tls_stat` and the raw wire read
//! in phase 2 are those.
//!
//! It is a spike. Nothing here is a transport, and none of it is merged into
//! the engine.

mod check;
mod net;
mod tls;

use std::io::{self, Read};
use std::net::TcpStream;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use ktls_core::{Context, TlsSession};
use rustls::client::{ClientConnectionData, UnbufferedClientConnection};
use rustls::server::{ServerConnectionData, UnbufferedServerConnection};
use rustls::{ClientConfig, ServerConfig};

use check::{Checks, Record, Recorder};
use tls::R;

const S2C: &[u8] = b"FIXBOLT-KTLS-SERVER-TO-CLIENT";
const C2S: &[u8] = b"FIXBOLT-KTLS-CLIENT-TO-SERVER";

/// A byte pattern that must never be visible on the wire in phase 2, and must
/// never be delivered to the peer in phase 3.
const NEEDLE: &[u8] = b"FIXBOLT-PLAINTEXT-NEEDLE-0123456789";

/// Round trips in the region a `strace` run attributes syscalls to. Big enough
/// that a per-message blocking call cannot hide in the noise of the handshake.
const ROUND_TRIPS: usize = 1000;

fn main() -> ExitCode {
    banner();

    let mut c = Checks::default();

    run("full-duplex", phase_full_duplex(&mut c), &mut c);
    run("wire-encrypted", phase_wire_is_encrypted(&mut c), &mut c);
    run("reversal", phase_reversal(&mut c), &mut c);
    run("drain-desyncs", phase_drain_desyncs(&mut c), &mut c);

    c.summary();
    if c.fail == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run(phase: &'static str, result: R<()>, c: &mut Checks) {
    if let Err(e) = result {
        c.assert(phase, false, format!("phase aborted: {e}"));
    }
}

/// The red half of `scripts/check-ktls-on-a-plain-socket.sh`.
fn wait_is_poll() -> bool {
    std::env::var("KTLS_SPIKE_WAIT").as_deref() == Ok("poll")
}

/// A blocking readiness call, on purpose.
///
/// A check that has only ever been seen passing is not known to work
/// (`CLAUDE.md` §7). This is the arm that must turn the trace red, and it is
/// `poll` rather than a sleep because `poll` is what an actual regression looks
/// like — somebody reaches for it as the obvious way to wait.
///
/// SAFETY: `libc::pollfd` is a plain `repr(C)` struct with no validity
/// invariants; `fd` is borrowed from a `TcpStream` that outlives the call, and
/// the `nfds` argument of 1 matches the single element passed. `CLAUDE.md` §2
/// rule 8 asks what proves it sound: the script's red half, which fails unless
/// this call appears in the trace attributed to this thread.
fn block_on_readiness(sock: &TcpStream) {
    use std::os::fd::AsRawFd;
    let mut fds = libc::pollfd {
        fd: sock.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe {
        libc::poll(&raw mut fds, 1, 1);
    }
}

fn banner() {
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .trim()
        .to_string();
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();
    println!("kernel: {kernel}");
    println!("cpu: {cpu}");
    println!("rustls: 0.23 / ktls-core: 0.0.5 / provider: ring");
    // Read back by scripts/check-ktls-on-a-plain-socket.sh rather than assumed
    // from the environment it set: a gate that assumes its own arm ran is a gate
    // that can be green about the wrong binary.
    println!("wait: {}", if wait_is_poll() { "poll" } else { "spin" });
}

// ---------------------------------------------------------------------------
// Phase 1 — both ends fully offloaded, and the data path stays read/write
// ---------------------------------------------------------------------------

fn phase_full_duplex(c: &mut Checks) -> R<()> {
    let pki = tls::pki()?;
    // Tickets on: phase 1 wants the kernel EIO path exercised.
    let scfg = tls::server_config(&pki, 4)?;
    let ccfg = tls::client_config(&pki)?;
    let (mut ssock, csock) = net::pair()?;

    let before = net::tls_stat();

    let (server, client) = std::thread::scope(|s| {
        let handle = s.spawn(move || {
            let mut sock = csock;
            client_half(&mut sock, ccfg)
        });
        let server = server_half(&mut ssock, scfg);
        let client = match handle.join() {
            Ok(r) => r,
            Err(_) => Err("client thread panicked".into()),
        };
        (server, client)
    });

    let after = net::tls_stat();

    c.merge(halves("full-duplex", server, client));
    c.note(
        "tls-stat",
        format!(
            "TlsTxSw {} -> {}, TlsRxSw {} -> {}, TlsDecryptError {} -> {}",
            before.get("TlsTxSw").copied().unwrap_or(0),
            after.get("TlsTxSw").copied().unwrap_or(0),
            before.get("TlsRxSw").copied().unwrap_or(0),
            after.get("TlsRxSw").copied().unwrap_or(0),
            before.get("TlsDecryptError").copied().unwrap_or(0),
            after.get("TlsDecryptError").copied().unwrap_or(0),
        ),
    );

    // Cumulative, not the `TlsCurr*` gauges: a gauge reads whatever is open at
    // the instant it is sampled, and one of the two sockets has already been
    // dropped by then. `[measured 2026-08-31]` asserting on TlsCurrTxSw read
    // 0 -> 1 for a pair that had plainly offloaded both ends.
    for (key, want) in [("TlsTxSw", 2u64), ("TlsRxSw", 2u64)] {
        let (b, a) = (
            before.get(key).copied().unwrap_or(0),
            after.get(key).copied().unwrap_or(0),
        );
        c.assert(
            "kernel-counts-the-sockets",
            !before.is_empty() && a >= b + want,
            format!("/proc/net/tls_stat {key} went {b} -> {a}, expected +{want}"),
        );
    }

    Ok(())
}

fn server_half(sock: &mut TcpStream, cfg: Arc<ServerConfig>) -> R<Vec<Record>> {
    let mut r = Recorder::default();
    let mut conn = UnbufferedServerConnection::new(cfg)?;

    let (leftover, reads) = drive_handshake!(conn, *sock)?;
    r.assert(
        "handshake-no-runtime-server",
        leftover == 0,
        format!("rustls handshake on a non-blocking socket: {reads} reads, {leftover} bytes left unprocessed"),
    );

    let (secrets, kconn) = conn.dangerous_into_kernel_connection()?;
    let version = kconn.protocol_version();
    let suite = kconn.negotiated_cipher_suite().suite();
    r.assert(
        "negotiated",
        version == rustls::ProtocolVersion::TLSv1_3,
        format!("{version:?} / {suite:?}"),
    );

    tls::hand_keys_to_kernel(&*sock, secrets, version, true, true)?;
    let mut ctx = Context::new(kconn, None);

    net::spin_write_all(sock, S2C)?;

    let mut stats = ReadStats::default();
    let mut buf = vec![0u8; 4096];
    read_exactly(sock, &mut ctx, &mut buf[..C2S.len()], &mut stats)?;
    r.assert(
        "plaintext-round-trip-server",
        &buf[..C2S.len()] == C2S,
        "wrote plaintext with write(2), read plaintext back with read(2)",
    );
    r.assert(
        "control-messages-handled",
        stats.eio_recovered == stats.eio_seen,
        format!(
            "{} EIO from the kernel (TLS control records the kernel will not decode), {} recovered by ktls_core::Context",
            stats.eio_seen, stats.eio_recovered
        ),
    );

    // Steady state. The markers delimit the region
    // scripts/check-ktls-on-a-plain-socket.sh attributes syscalls to, by the tid
    // that wrote the marker.
    let wait_poll = wait_is_poll();
    eprintln!("MARK steady-state begin");
    let mut seq = [0u8; 8];
    let mut echo = [0u8; 8];
    for i in 0..ROUND_TRIPS {
        seq.copy_from_slice(&(i as u64).to_le_bytes());
        net::spin_write_all(sock, &seq)?;
        if wait_poll {
            block_on_readiness(sock);
        }
        read_exactly(sock, &mut ctx, &mut echo, &mut stats)?;
        if echo != seq {
            r.assert("steady-state", false, format!("echo mismatch at {i}"));
            break;
        }
    }
    eprintln!("MARK steady-state end");
    r.assert(
        "steady-state",
        true,
        format!("{ROUND_TRIPS} plaintext round trips over an offloaded socket"),
    );

    // The property the engine actually depends on: an empty offloaded socket
    // reports emptiness the same way an empty TCP socket does.
    let mut scratch = [0u8; 64];
    let mut verdict = String::from("never reached WouldBlock");
    let mut ok = false;
    for _ in 0..64 {
        match sock.read(&mut scratch) {
            Ok(n) => {
                verdict = format!("unexpected {n} bytes");
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                ok = true;
                verdict = "read(2) on an empty offloaded socket returns EAGAIN".into();
                break;
            }
            Err(e) => {
                if ctx.handle_io_error(&*sock, e).is_err() {
                    verdict = "unrecoverable error".into();
                    break;
                }
            }
        }
    }
    r.assert("wouldblock-unchanged", ok, verdict);

    Ok(r.0)
}

fn client_half(sock: &mut TcpStream, cfg: Arc<ClientConfig>) -> R<Vec<Record>> {
    let mut r = Recorder::default();
    let name = rustls::pki_types::ServerName::try_from("localhost")?;
    let mut conn = UnbufferedClientConnection::new(cfg, name)?;

    let (leftover, reads) = drive_handshake!(conn, *sock)?;
    r.assert(
        "handshake-no-runtime-client",
        leftover == 0,
        format!("rustls handshake on a non-blocking socket: {reads} reads, {leftover} bytes left unprocessed"),
    );

    let (secrets, kconn) = conn.dangerous_into_kernel_connection()?;
    let version = kconn.protocol_version();

    // Deliberately no drain first. A TLS 1.3 server sends session tickets right
    // after the handshake, so this socket very likely has a handshake record
    // waiting — which is exactly the case ktls_core::Context exists for, and
    // draining it by hand would desynchronise the kernel's sequence number.
    tls::hand_keys_to_kernel(&*sock, secrets, version, true, true)?;
    let mut ctx = Context::new(kconn, None);

    let mut stats = ReadStats::default();
    let mut buf = vec![0u8; 4096];
    read_exactly(sock, &mut ctx, &mut buf[..S2C.len()], &mut stats)?;
    r.assert(
        "plaintext-round-trip-client",
        &buf[..S2C.len()] == S2C,
        "read plaintext with read(2) from a socket the kernel decrypts",
    );

    net::spin_write_all(sock, C2S)?;

    let mut seq = [0u8; 8];
    for _ in 0..ROUND_TRIPS {
        read_exactly(sock, &mut ctx, &mut seq, &mut stats)?;
        net::spin_write_all(sock, &seq)?;
    }

    r.assert(
        "session-tickets-survived",
        stats.eio_seen == stats.eio_recovered,
        format!(
            "{} kernel EIO seen on the client, {} recovered",
            stats.eio_seen, stats.eio_recovered
        ),
    );

    Ok(r.0)
}

// ---------------------------------------------------------------------------
// Phase 2 — what is actually on the wire
// ---------------------------------------------------------------------------

/// The sender offloads transmit to the kernel; the receiver enables nothing at
/// all and reads the socket raw.
///
/// This is the observation that cannot be faked by a syscall returning `Ok`:
/// the receiver is looking at the bytes the kernel put on the wire.
fn phase_wire_is_encrypted(c: &mut Checks) -> R<()> {
    let pki = tls::pki()?;
    // Tickets off: nothing but this test's own bytes may be on the wire.
    let scfg = tls::server_config(&pki, 0)?;
    let ccfg = tls::client_config(&pki)?;
    let (mut ssock, csock) = net::pair()?;

    let ready = AtomicU32::new(0);

    let (sender, receiver) = std::thread::scope(|s| {
        let ready_ref = &ready;

        let handle = s.spawn(move || -> R<Vec<Record>> {
            let mut r = Recorder::default();
            let mut sock = csock;
            let name = rustls::pki_types::ServerName::try_from("localhost")?;
            let mut conn = UnbufferedClientConnection::new(ccfg, name)?;
            drive_handshake!(conn, sock)?;
            drop(conn);

            // Reach a known-empty socket, then let the sender start.
            net::drain(&mut sock)?;
            ready_ref.store(1, Ordering::Release);

            let mut buf = vec![0u8; 4096];
            let n = net::spin_read(&mut sock, &mut buf)?;
            let seen = &buf[..n];

            // Not just "a TLS record" — a record whose length is the one the
            // needle produces: 35 bytes of plaintext, one inner content-type
            // byte, and a 16-byte AES-GCM tag. Anything else on this socket
            // (a session ticket, say) has a different length and fails here.
            let want_len = NEEDLE.len() + 1 + 16;
            let declared =
                if seen.len() >= 5 { usize::from(u16::from_be_bytes([seen[3], seen[4]])) } else { 0 };
            let header_ok = seen.len() == 5 + want_len
                && seen[0] == 0x17
                && seen[1] == 0x03
                && seen[2] == 0x03
                && declared == want_len;
            r.assert(
                "wire-is-a-tls-record",
                header_ok,
                format!(
                    "first bytes off the wire: {:02x?}; {n} bytes total, record declares {declared}, needle+type+tag is {want_len}",
                    &seen[..seen.len().min(8)]
                ),
            );

            let leaked = seen.windows(NEEDLE.len()).any(|w| w == NEEDLE);
            r.assert(
                "wire-carries-no-plaintext",
                !leaked,
                format!(
                    "{n} bytes read raw off the socket; the plaintext needle is {}",
                    if leaked { "PRESENT" } else { "absent" }
                ),
            );
            Ok(r.0)
        });

        let sender = (|| -> R<Vec<Record>> {
            let mut conn = UnbufferedServerConnection::new(scfg)?;
            drive_handshake!(conn, ssock)?;
            let (secrets, kconn) = conn.dangerous_into_kernel_connection()?;
            let version = kconn.protocol_version();

            net::spin_until(ready_ref, 1)?;
            tls::hand_keys_to_kernel(&ssock, secrets, version, true, false)?;
            net::spin_write_all(&mut ssock, NEEDLE)?;
            Ok(Vec::new())
        })();

        let receiver = match handle.join() {
            Ok(r) => r,
            Err(_) => Err("receiver thread panicked".into()),
        };
        (sender, receiver)
    });

    c.merge(halves("wire-encrypted", sender, receiver));
    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 3 — the reversal
// ---------------------------------------------------------------------------

/// Same shape as phase 2, with the key handover removed from the sender.
///
/// `CLAUDE.md` §7: a guard is proven by reversal. If plaintext written by an
/// un-offloaded sender still arrives at an offloaded receiver, then kTLS was
/// never on and phase 1 proved nothing.
fn phase_reversal(c: &mut Checks) -> R<()> {
    let pki = tls::pki()?;
    // Tickets off: nothing but this test's own bytes may be on the wire.
    let scfg = tls::server_config(&pki, 0)?;
    let ccfg = tls::client_config(&pki)?;
    let (mut ssock, csock) = net::pair()?;

    let ready = AtomicU32::new(0);

    let (sender, receiver) = std::thread::scope(|s| {
        let ready_ref = &ready;

        let handle = s.spawn(move || -> R<Vec<Record>> {
            let mut r = Recorder::default();
            let mut sock = csock;
            let name = rustls::pki_types::ServerName::try_from("localhost")?;
            let mut conn = UnbufferedClientConnection::new(ccfg, name)?;
            drive_handshake!(conn, sock)?;
            let (secrets, kconn) = conn.dangerous_into_kernel_connection()?;
            let version = kconn.protocol_version();
            tls::hand_keys_to_kernel(&sock, secrets, version, false, true)?;
            let mut ctx = Context::new(kconn, None);
            ready_ref.store(1, Ordering::Release);

            let mut buf = vec![0u8; 4096];
            let start = Instant::now();
            let mut got_needle = false;
            let mut outcome = String::from("nothing arrived before the deadline");
            while start.elapsed() < Duration::from_secs(3) {
                match sock.read(&mut buf) {
                    Ok(n) => {
                        got_needle = buf[..n].windows(NEEDLE.len()).any(|w| w == NEEDLE);
                        outcome = format!("read returned {n} bytes");
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::hint::spin_loop(),
                    Err(e) => {
                        let errno = e.raw_os_error();
                        if errno == Some(libc::EIO) && ctx.handle_io_error(&sock, e).is_ok() {
                            continue;
                        }
                        outcome = format!("read failed with errno {errno:?}");
                        break;
                    }
                }
            }
            r.assert(
                "reversal-breaks-it",
                !got_needle,
                format!("sender did not hand its keys to the kernel; receiver: {outcome}"),
            );
            Ok(r.0)
        });

        let sender = (|| -> R<Vec<Record>> {
            let mut conn = UnbufferedServerConnection::new(scfg)?;
            drive_handshake!(conn, ssock)?;
            // No setup_ulp, no keys. Plaintext straight onto the wire.
            net::spin_until(ready_ref, 1)?;
            net::spin_write_all(&mut ssock, NEEDLE)?;
            Ok(Vec::new())
        })();

        let receiver = match handle.join() {
            Ok(r) => r,
            Err(_) => Err("receiver thread panicked".into()),
        };
        (sender, receiver)
    });

    c.merge(halves("reversal", sender, receiver));
    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 4 — draining by hand before the handover breaks the session
// ---------------------------------------------------------------------------

/// The obvious way to deal with phase 1's `EIO` is to read the socket empty
/// before handing the keys over. This measures what that costs.
///
/// The kernel starts decrypting at the receive sequence number rustls hands it,
/// and that number counts the records *rustls* processed. Ciphertext pulled off
/// the socket by hand is never counted, so the kernel is left one record behind
/// the sender for the life of the connection.
fn phase_drain_desyncs(c: &mut Checks) -> R<()> {
    let pki = tls::pki()?;
    // Tickets on: there has to be a post-handshake record to drain.
    let scfg = tls::server_config(&pki, 4)?;
    let ccfg = tls::client_config(&pki)?;
    let (mut ssock, csock) = net::pair()?;

    let before = net::tls_stat();
    let ready = AtomicU32::new(0);

    let (sender, receiver) = std::thread::scope(|s| {
        let ready_ref = &ready;

        let handle = s.spawn(move || -> R<Vec<Record>> {
            let mut r = Recorder::default();
            let mut sock = csock;
            let name = rustls::pki_types::ServerName::try_from("localhost")?;
            let mut conn = UnbufferedClientConnection::new(ccfg, name)?;
            drive_handshake!(conn, sock)?;
            let (secrets, kconn) = conn.dangerous_into_kernel_connection()?;
            let version = kconn.protocol_version();

            // Drain until something was actually discarded, so the check cannot
            // pass by racing the session ticket and draining nothing at all.
            let start = Instant::now();
            let mut discarded = 0;
            while discarded == 0 {
                discarded = net::drain(&mut sock)?;
                if start.elapsed() > net::DEADLINE {
                    return Err("no post-handshake record arrived to drain".into());
                }
                std::hint::spin_loop();
            }

            tls::hand_keys_to_kernel(&sock, secrets, version, false, true)?;
            let mut ctx = Context::new(kconn, None);
            ready_ref.store(1, Ordering::Release);

            let mut buf = vec![0u8; 4096];
            let start = Instant::now();
            let mut got_needle = false;
            let mut outcome = String::from("nothing arrived before the deadline");
            while start.elapsed() < Duration::from_secs(3) {
                match sock.read(&mut buf) {
                    Ok(n) => {
                        got_needle = buf[..n].windows(NEEDLE.len()).any(|w| w == NEEDLE);
                        outcome = format!("read returned {n} bytes");
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::hint::spin_loop(),
                    Err(e) => {
                        let errno = e.raw_os_error();
                        if errno == Some(libc::EIO) && ctx.handle_io_error(&sock, e).is_ok() {
                            continue;
                        }
                        outcome = format!("read failed with errno {errno:?}");
                        break;
                    }
                }
            }
            r.assert(
                "hand-draining-desyncs-the-kernel",
                !got_needle,
                format!(
                    "{discarded} bytes of post-handshake ciphertext read by hand before the handover; the next record then: {outcome}"
                ),
            );
            Ok(r.0)
        });

        let sender = (|| -> R<Vec<Record>> {
            let mut conn = UnbufferedServerConnection::new(scfg)?;
            drive_handshake!(conn, ssock)?;
            let (secrets, kconn) = conn.dangerous_into_kernel_connection()?;
            let version = kconn.protocol_version();
            net::spin_until(ready_ref, 1)?;
            tls::hand_keys_to_kernel(&ssock, secrets, version, true, false)?;
            net::spin_write_all(&mut ssock, NEEDLE)?;
            Ok(Vec::new())
        })();

        let receiver = match handle.join() {
            Ok(r) => r,
            Err(_) => Err("receiver thread panicked".into()),
        };
        (sender, receiver)
    });

    c.merge(halves("drain-desyncs", sender, receiver));

    let after = net::tls_stat();
    let key = "TlsDecryptError";
    let (b, a) = (
        before.get(key).copied().unwrap_or(0),
        after.get(key).copied().unwrap_or(0),
    );
    c.note(
        "drain-desyncs",
        format!("/proc/net/tls_stat {key} went {b} -> {a}"),
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Reading an offloaded socket
// ---------------------------------------------------------------------------

/// Merge two thread halves' records.
///
/// Both halves are joined before either error is propagated. `[measured
/// 2026-08-31]` the first version of this file used `?` on the main half, so a
/// worker thread's error was dropped on the floor and the socket it owned was
/// closed — which the main half then reported as `ENOTCONN` from `setsockopt`.
/// The transcript named a syscall that was fine and hid the failure that
/// mattered.
fn halves(phase: &'static str, a: R<Vec<Record>>, b: R<Vec<Record>>) -> Vec<Record> {
    let mut out = Vec::new();
    for half in [a, b] {
        match half {
            Ok(mut records) => out.append(&mut records),
            Err(e) => out.push(Record {
                token: phase,
                ok: false,
                detail: format!("half aborted: {e}"),
            }),
        }
    }
    out
}

#[derive(Default)]
struct ReadStats {
    eio_seen: usize,
    eio_recovered: usize,
}

/// Fill `buf` completely, spinning, handing every error the kernel reports back
/// to `ktls_core::Context`.
///
/// This is the whole shape ADR-0005 needs to be true: call `read`, and when it
/// fails, give the error to the TLS session and try again. No runtime, no
/// readiness API, no blocking call.
fn read_exactly<C: TlsSession>(
    sock: &mut TcpStream,
    ctx: &mut Context<C>,
    buf: &mut [u8],
    stats: &mut ReadStats,
) -> R<()> {
    let start = Instant::now();
    let mut filled = 0;
    while filled < buf.len() {
        match sock.read(&mut buf[filled..]) {
            Ok(0) => return Err("peer closed".into()),
            Ok(n) => filled += n,
            Err(e) => {
                let is_eio = e.raw_os_error() == Some(libc::EIO);
                if is_eio {
                    stats.eio_seen += 1;
                }
                let recovered = ctx.handle_io_error(&*sock, e).is_ok();
                if is_eio && recovered {
                    stats.eio_recovered += 1;
                }
                if !recovered {
                    return Err("unrecoverable error on an offloaded socket".into());
                }
                if start.elapsed() > net::DEADLINE {
                    return Err("read_exactly timed out".into());
                }
                std::hint::spin_loop();
            }
        }
    }
    Ok(())
}

// Silences an unused-import warning when the kernel connection types are only
// named inside the macro expansions above.
#[allow(dead_code)]
type _ServerCtx = Context<rustls::kernel::KernelConnection<ServerConnectionData>>;
#[allow(dead_code)]
type _ClientCtx = Context<rustls::kernel::KernelConnection<ClientConnectionData>>;
