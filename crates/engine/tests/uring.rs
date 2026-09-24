//! The `io_uring` transport — `docs/plans/2026-09-24-p4-io-uring-transport.md`,
//! *Cách kiểm chứng* items 2 to 4, and ADR-0190.
//!
//! Written **red first**, in step 1, against a skeleton whose every
//! constructor refuses. Each test is red at its own assertion — a refused ring
//! reads *"Uring::hft refused: …"* — never at a panic of the harness.
//!
//! # What proves the `unsafe` in `src/transport/uring.rs`
//!
//! Miri cannot run `io_uring`, and no sanitizer sees the kernel write into
//! user memory. So the proof is behavioural and is this file (plan *Bất biến*
//! 8, U1–U7): byte-exact runs over real sockets, an ownership ledger checked
//! after every reap ([`Uring::buffers_accounted_for`]), a residency check on
//! the buffer memory, and a canary written into the buffer range after the
//! ring is gone.
//!
//! # Why some tests take a write lock
//!
//! Two tests read process-wide facts: `VmRSS`, and whether an address range is
//! free. Every other test in this binary runs in parallel and would move both,
//! so those two hold [`EXCLUSIVE`] for writing and the rest for reading.
#![cfg(all(feature = "io-uring", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use fixbolt_engine::transport::uring::{
    HftArm, Uring, UringArm, UringConfig, UringRefused, UringSpin,
};
use fixbolt_engine::transport::{Io, TcpTransport, Transport};
use fixbolt_engine::wait::Waiting;

/// See the module comment: shared by every test, exclusive for the two that
/// read process-wide facts.
static EXCLUSIVE: RwLock<()> = RwLock::new(());

fn shared() -> RwLockReadGuard<'static, ()> {
    EXCLUSIVE.read().unwrap_or_else(PoisonError::into_inner)
}

fn exclusive() -> RwLockWriteGuard<'static, ()> {
    EXCLUSIVE.write().unwrap_or_else(PoisonError::into_inner)
}

fn config(buffers: u16, buffer_len: u32, connections: u16) -> UringConfig {
    UringConfig::new(buffers, buffer_len, connections).expect("a valid ring size")
}

/// An `hft` ring on the default arm. A refusal is this file's expected red in
/// step 1, and it names itself.
fn hft(cfg: UringConfig) -> (Uring, UringSpin) {
    Uring::hft(cfg, HftArm::Enter).unwrap_or_else(|e| panic!("Uring::hft refused: {e}"))
}

/// A connected pair: the client end, and the engine's end as a transport.
fn pair() -> (TcpStream, TcpTransport) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");
    let client = TcpStream::connect(addr).expect("connect");
    client.set_nodelay(true).expect("nodelay");
    let (server, _) = listener.accept().expect("accept");
    (client, TcpTransport::new(server).expect("non-blocking"))
}

/// Reap and read until `want` bytes have arrived, the connection ends, or
/// `within` passes. Returns what arrived and how the reading ended.
fn reap_until(
    wait: &mut impl Waiting,
    t: &mut impl Transport,
    want: usize,
    within: Duration,
) -> (Vec<u8>, Io) {
    // `want` may be `usize::MAX` ("until it ends"), so it only caps the
    // reservation.
    let mut got = Vec::with_capacity(want.min(1 << 20));
    let mut buf = [0u8; 4096];
    let start = Instant::now();
    let mut last = Io::Idle;
    while got.len() < want && start.elapsed() < within {
        wait.idle(&[]);
        loop {
            last = t.recv(&mut buf);
            match last {
                Io::Ready(n) => got.extend_from_slice(&buf[..n]),
                Io::Idle => break,
                Io::Closed | Io::Failed(_) => return (got, last),
            }
        }
    }
    (got, last)
}

/// A pattern no two offsets or connections share cheaply, so a byte in the
/// wrong place or the wrong connection is a mismatch.
fn pattern(conn: usize, len: usize, seed: u64) -> Vec<u8> {
    (0..len)
        .map(|i| {
            let x = (i as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add((conn as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
                .wrapping_add(seed);
            (x >> 56) as u8
        })
        .collect()
}

/// xorshift64: deterministic from a printed seed, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// ---------------------------------------------------------------------------
// Item 2: the data is right.
// ---------------------------------------------------------------------------

/// Bytes written by a peer arrive through the ring — and the ring is what
/// carried them: `report().cqes > 0`, on the arm that was asked for.
#[test]
fn a_message_arrives_through_the_ring() {
    let _g = shared();
    let (uring, mut spin) = hft(config(8, 4096, 1));
    let (mut client, server) = pair();
    let mut t = uring.register(server).expect("a free slot");

    let msg = b"8=FIX.4.4\x019=5\x0135=0\x0110=161\x01";
    client.write_all(msg).expect("write");
    let (got, _) = reap_until(&mut spin, &mut t, msg.len(), Duration::from_secs(1));

    assert_eq!(got, msg, "the bytes the peer wrote, exactly");
    let r = uring.report();
    assert!(
        r.cqes > 0,
        "no completion was reaped, so the ring did not carry these bytes: {r:?}"
    );
    assert!(r.bytes >= msg.len() as u64, "{r:?}");
    assert_eq!(r.arm, UringArm::Enter, "HftArm::Enter was asked for");
}

/// Two buffers against a 1 MiB burst: the multishot `recv` runs dry, ends on
/// `-ENOBUFS`, is submitted again — and not one byte is lost or reordered.
///
/// `enobufs > 0` **and** `rearms > 0` are what make this non-vacuous: a run in
/// which the buffers never ran out proves nothing about re-arming. Reversal R7
/// (no re-arm after `ENOBUFS`) turns it red on the byte count.
#[test]
fn a_ring_that_runs_out_of_buffers_rearms_and_loses_nothing() {
    let _g = shared();
    let (uring, mut spin) = hft(config(2, 4096, 1));
    let (mut client, server) = pair();
    let mut t = uring.register(server).expect("a free slot");

    const TOTAL: usize = 1 << 20;
    let sent = pattern(0, TOTAL, 7);
    let writer = {
        let sent = sent.clone();
        std::thread::spawn(move || {
            client.write_all(&sent).expect("write the burst");
            client
        })
    };
    let (got, how) = reap_until(&mut spin, &mut t, TOTAL, Duration::from_secs(10));
    let _client = writer.join().expect("the writer finished");

    assert_eq!(
        got.len(),
        TOTAL,
        "bytes missing after the buffers ran out (ended on {how:?})"
    );
    assert!(got == sent, "the burst arrived reordered or corrupted");
    let r = uring.report();
    assert!(
        r.enobufs > 0,
        "the buffers never ran out, so re-arming was not exercised: {r:?}"
    );
    assert!(r.rearms > 0, "ENOBUFS was seen and nothing re-armed: {r:?}");
}

/// Sixty-four connections, random chunk sizes, random interleaving, sixteen
/// small buffers shared between them — byte-exact per connection, and the
/// ownership ledger holds after **every** reap (U5, U6).
#[test]
fn sixty_four_connections_interleaved_are_byte_exact() {
    let _g = shared();
    const CONNS: usize = 64;
    const EACH: usize = 16 * 1024;
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
        | 1;
    println!("seed: {seed}");

    let (uring, mut spin) = hft(config(16, 512, CONNS as u16));
    let mut clients = Vec::with_capacity(CONNS);
    let mut transports = Vec::with_capacity(CONNS);
    for _ in 0..CONNS {
        let (c, s) = pair();
        clients.push(c);
        transports.push(uring.register(s).expect("a free slot"));
    }
    let sent: Vec<Vec<u8>> = (0..CONNS).map(|i| pattern(i, EACH, seed)).collect();

    let writer = {
        let sent = sent.clone();
        std::thread::spawn(move || {
            let mut rng = Rng(seed);
            let mut at = [0usize; CONNS];
            let mut left = CONNS;
            while left > 0 {
                let i = rng.below(CONNS);
                if at[i] == EACH {
                    continue;
                }
                let n = (1 + rng.below(700)).min(EACH - at[i]);
                clients[i]
                    .write_all(&sent[i][at[i]..at[i] + n])
                    .expect("write a chunk");
                at[i] += n;
                if at[i] == EACH {
                    left -= 1;
                }
            }
            clients
        })
    };

    let mut got: Vec<Vec<u8>> = (0..CONNS).map(|_| Vec::with_capacity(EACH)).collect();
    let mut buf = [0u8; 1024];
    let mut rng = Rng(seed ^ 0xA5A5);
    let start = Instant::now();
    while got.iter().any(|g| g.len() < EACH) && start.elapsed() < Duration::from_secs(20) {
        spin.idle(&[]);
        assert!(
            uring.buffers_accounted_for(),
            "a buffer is in two places, or in none, after a reap (seed {seed})"
        );
        for (i, t) in transports.iter_mut().enumerate() {
            // A random read size, so a buffer is sometimes read in pieces.
            let want = 1 + rng.below(buf.len());
            match t.recv(&mut buf[..want]) {
                Io::Ready(n) => got[i].extend_from_slice(&buf[..n]),
                Io::Idle => {}
                other => panic!("connection {i} ended early on {other:?} (seed {seed})"),
            }
        }
    }
    let _clients = writer.join().expect("the writer finished");

    for i in 0..CONNS {
        assert_eq!(
            got[i].len(),
            EACH,
            "connection {i} is missing bytes (seed {seed})"
        );
        assert!(
            got[i] == sent[i],
            "connection {i} received another connection's bytes, or its own out of \
             order (seed {seed})"
        );
    }
    assert!(uring.report().cqes > 0);
}

/// A completion that arrives for a connection already dropped reaches nobody —
/// in particular not the connection that took its slot.
///
/// One slot, so the second connection **must** reuse it. The first
/// connection's `recv` is armed, its peer writes, and it is dropped before the
/// next reap: that completion is posted after the slot has a new tenant.
/// Reversal R5 (no generation bump) delivers the `A`s to `B`.
#[test]
fn a_late_completion_for_a_dropped_connection_reaches_nobody() {
    let _g = shared();
    let (uring, mut spin) = hft(config(8, 4096, 1));

    let (mut ca, sa) = pair();
    let a = uring.register(sa).expect("the one slot");
    spin.idle(&[]); // A's multishot recv is now in the kernel.
    ca.write_all(&[b'A'; 1000]).expect("write A");
    std::thread::sleep(Duration::from_millis(20)); // A's bytes are in its socket.
    drop(a);

    let (mut cb, sb) = pair();
    let mut b = uring
        .register(sb)
        .expect("the slot A left must be free again");
    cb.write_all(&[b'B'; 1000]).expect("write B");
    let (got, how) = reap_until(&mut spin, &mut b, 1000, Duration::from_secs(1));

    assert!(
        got.iter().all(|&x| x == b'B'),
        "B received bytes that were A's: {:?}",
        String::from_utf8_lossy(&got[..got.len().min(64)])
    );
    assert_eq!(got.len(), 1000, "B's own bytes (ended on {how:?})");
    let r = uring.report();
    assert!(
        r.stale > 0,
        "no completion was discarded as stale, so this run never had a late \
         completion to misdeliver: {r:?}"
    );
    drop(ca);
}

/// Both directions of a close: a peer that hangs up is `Io::Closed`, and a
/// connection this end drops is a FIN the peer reads within a second.
///
/// **No reap after the drop**, on purpose: the FIN must come from
/// `shutdown(2)` at drop, not from the `ASYNC_CANCEL` the next idle turn
/// submits. Reversal R6 (no `shutdown`) leaves the pending `recv` holding the
/// socket and the peer reading nothing.
#[test]
fn a_closed_connection_is_seen_as_closed_and_its_peer_sees_fin() {
    let _g = shared();
    let (uring, mut spin) = hft(config(8, 4096, 2));

    // The peer hangs up.
    let (mut c1, s1) = pair();
    let mut t1 = uring.register(s1).expect("a free slot");
    c1.write_all(b"x").expect("write");
    drop(c1);
    let (got, how) = reap_until(&mut spin, &mut t1, usize::MAX, Duration::from_secs(1));
    assert_eq!(got, b"x", "the byte before the hang-up");
    assert_eq!(how, Io::Closed, "a peer that hung up reads as Closed");

    // This end hangs up.
    let (mut c2, s2) = pair();
    let t2 = uring.register(s2).expect("a free slot");
    spin.idle(&[]); // t2's recv is pending in the kernel, holding the socket.
    drop(t2);
    c2.set_read_timeout(Some(Duration::from_secs(1)))
        .expect("timeout");
    let mut one = [0u8; 1];
    match c2.read(&mut one) {
        Ok(0) => {}
        other => panic!("the peer must read end-of-stream within 1 s, got {other:?}"),
    }
}

/// `VmRSS` of this process.
fn rss_bytes() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("/proc/self/status");
    let line = status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .expect("a VmRSS line");
    let kb: u64 = line
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .expect("VmRSS in kB");
    kb * 1024
}

/// The buffer memory is resident as soon as the ring exists — no first touch
/// on the hot path (non-negotiable 1, U1). 64 MiB, so nothing else this
/// process does in the window can be mistaken for it. Reversal R10 (no
/// pre-touch) turns it red.
#[test]
fn buffers_are_resident_before_the_first_message() {
    let _g = exclusive();
    let cfg = config(1024, 64 * 1024, 1);
    let before = rss_bytes();
    let (_uring, _spin) = hft(cfg);
    let after = rss_bytes();
    let grew = after.saturating_sub(before);
    assert!(
        grew >= cfg.buffer_bytes(),
        "VmRSS grew by {grew} bytes when the ring was made, against {} bytes of \
         buffers: they are not all resident, and the first message would fault",
        cfg.buffer_bytes()
    );
}

/// After the ring is dropped the kernel writes nothing into what was its
/// buffer memory (U2).
///
/// The range is 64 MiB — above glibc's largest mmap threshold, so it was its
/// own mapping and dropping the ring unmapped it. The test maps that exact
/// range again (`MAP_FIXED_NOREPLACE`), fills it with a canary, has the old
/// peer send more, and reads the canary back.
///
/// What it cannot see: a write in the instant between freeing the memory and
/// closing the ring inside `Drop`, before this test can re-occupy the range.
/// Field order and the explicit `Drop` are what hold that; this catches a ring
/// that outlives its buffers by more than that instant.
#[test]
fn unregistered_buffers_are_not_written_after_the_ring_is_dropped() {
    let _g = exclusive();
    let cfg = config(1024, 64 * 1024, 1);
    let (uring, mut spin) = hft(cfg);
    let (mut client, server) = pair();
    let mut t = uring.register(server).expect("a free slot");
    client.write_all(b"before").expect("write");
    let (got, _) = reap_until(&mut spin, &mut t, 6, Duration::from_secs(1));
    assert_eq!(got, b"before", "the ring worked before it was dropped");

    let (addr, len) = uring.buffer_region();
    assert!(
        len as u64 >= cfg.buffer_bytes(),
        "the region must cover every buffer"
    );
    drop(t);
    drop(spin);
    drop(uring);

    // SAFETY: an anonymous private mapping at a range this process no longer
    // uses; `MAP_FIXED_NOREPLACE` fails rather than replacing anything that is
    // mapped there, so no live memory is touched. Unmapped below.
    #[allow(unsafe_code)]
    let p = unsafe {
        libc::mmap(
            addr as *mut libc::c_void,
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED_NOREPLACE,
            -1,
            0,
        )
    };
    assert!(
        p != libc::MAP_FAILED && p as usize == addr,
        "could not re-occupy the old buffer range at {addr:#x} ({}): this run \
         proves nothing, so it fails",
        std::io::Error::last_os_error()
    );
    // SAFETY: `p` is the `len`-byte mapping made just above, readable and
    // writable, and nothing else refers to it.
    #[allow(unsafe_code)]
    let canary = unsafe { std::slice::from_raw_parts_mut(p.cast::<u8>(), len) };
    canary.fill(0xA5);

    // The old peer keeps talking. Errors are expected — this end is gone.
    for _ in 0..16 {
        let _ = client.write_all(&[b'z'; 4096]);
    }
    std::thread::sleep(Duration::from_millis(100));

    let intact = canary.iter().all(|&b| b == 0xA5);
    // SAFETY: the mapping made above, unmapped once; `canary` is not used after.
    #[allow(unsafe_code)]
    unsafe {
        libc::munmap(p, len);
    }
    assert!(
        intact,
        "the kernel wrote into the buffer range after the ring was dropped"
    );
}

// ---------------------------------------------------------------------------
// Item 3: `standard` wakes for the data, not for the timeout.
// ---------------------------------------------------------------------------

#[cfg(feature = "standard")]
use fixbolt_engine::Acceptor;
#[cfg(feature = "standard")]
use fixbolt_engine::transport::Interest;
#[cfg(feature = "standard")]
use fixbolt_engine::transport::uring::UringBlock;

/// A `standard` ring waiting at most `timeout_ms` per idle turn.
#[cfg(feature = "standard")]
fn standard(cfg: UringConfig, timeout_ms: u32) -> (Uring, UringBlock) {
    let (uring, block) =
        Uring::standard(cfg).unwrap_or_else(|e| panic!("Uring::standard refused: {e}"));
    (uring, block.with_timeout_ms(timeout_ms))
}

/// The engine, over the ring, with a session a `Logon` can reach.
#[cfg(feature = "standard")]
mod engine {
    use fixbolt_engine::clock::ManualClock;
    use fixbolt_engine::dispatch::InlineDispatch;
    use fixbolt_engine::journal::Store;
    use fixbolt_engine::transport::uring::{UringBlock, UringTransport};
    use fixbolt_engine::{Config, Engine};

    /// An application that never answers: only the session speaks.
    pub struct Silent;
    impl fixbolt_session::Application for Silent {
        fn on_message(
            &mut self,
            _m: &[u8],
            _s: fixbolt_session::Header<'_>,
            _o: &mut [u8],
        ) -> Option<core::ops::Range<usize>> {
            None
        }
    }

    pub type Blocking = Engine<
        UringTransport,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        UringBlock,
        Store,
        256,
        4096,
        8192,
    >;

    pub fn blocking(wait: UringBlock) -> Blocking {
        Engine::new(
            Config::acceptor(b"FIX.4.4", b"ISLD", b"TEST"),
            InlineDispatch::new(Silent),
            ManualClock::at(fixbolt_conformance::script::FIXED_TIME_MILLIS),
            wait,
            4,
        )
    }

    /// A `Logon` the session accepts: the corpus's fixed instant, the clock's.
    pub fn logon() -> Vec<u8> {
        let body = format!(
            "35=A\x0134=1\x0149=TEST\x0152={}\x0156=ISLD\x0198=0\x01108=30\x01",
            fixbolt_conformance::script::FIXED_TIME_IN
        );
        let head = format!("8=FIX.4.4\x019={}\x01", body.len());
        let mut out = head.into_bytes();
        out.extend_from_slice(body.as_bytes());
        let sum: u32 = out.iter().map(|b| u32::from(*b)).sum();
        out.extend_from_slice(format!("10={:03}\x01", sum % 256).as_bytes());
        out
    }
}

/// A `Logon` that arrives while the engine is asleep in the ring is answered
/// in well under a second, against a **10 s** timeout — the wait ended on the
/// data.
#[cfg(feature = "standard")]
#[test]
fn standard_is_woken_by_the_data_not_the_timeout() {
    let _g = shared();
    let (uring, block) = standard(config(8, 4096, 2), 10_000);
    let mut e = engine::blocking(block);
    let (client, server) = pair();
    let _id = e.add(uring.register(server).expect("a free slot"));

    let peer = std::thread::spawn(move || {
        let mut client = client;
        // Late enough that the engine is already waiting in the kernel.
        std::thread::sleep(Duration::from_millis(100));
        let sent = Instant::now();
        client.write_all(&engine::logon()).expect("write the Logon");
        client
            .set_read_timeout(Some(Duration::from_secs(12)))
            .expect("timeout");
        let mut reply = [0u8; 512];
        let n = client.read(&mut reply).unwrap_or(0);
        (reply[..n].to_vec(), sent.elapsed())
    });

    let start = Instant::now();
    while !peer.is_finished() && start.elapsed() < Duration::from_secs(15) {
        e.turn();
        if !peer.is_finished() {
            e.idle();
        }
    }
    let (reply, took) = peer.join().expect("the peer finished");
    assert!(
        reply.starts_with(b"8=FIX.4.4\x01"),
        "the Logon was not answered: {:?}",
        String::from_utf8_lossy(&reply)
    );
    assert!(
        took < Duration::from_secs(1),
        "answered after {took:?} against a 10 s timeout: the wait ended on the clock, \
         not on the data"
    );
}

/// A connection arriving at the listener ends the wait — **twice, each time on
/// a new listener**, so a one-shot `POLL_ADD` left armed on a closed
/// listener's descriptor cannot pass for the new one. Reversal R4 (no
/// `POLL_ADD` for the listener) turns it red.
#[cfg(feature = "standard")]
#[test]
fn standard_is_woken_by_a_connect() {
    let _g = shared();
    let (_uring, mut block) = standard(config(8, 4096, 2), 10_000);
    for round in 0..2 {
        let acceptor = Acceptor::bind("127.0.0.1:0").expect("a free port");
        let addr = acceptor.local_addr().expect("bound");
        let listener = [Interest::readable(acceptor.source().expect("a descriptor"))];
        let dialer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            TcpStream::connect(addr).expect("connect")
        });
        let before = Instant::now();
        block.idle(&listener);
        let took = before.elapsed();
        let _client = dialer.join().expect("the dialer finished");
        assert!(
            took < Duration::from_secs(1),
            "round {round}: a connect after 100 ms ended a 10 s wait only after {took:?}"
        );
        assert!(
            acceptor.accept().is_some(),
            "round {round}: the connection that woke it"
        );
    }
}

/// The dispatch waker ends the wait: what an application thread does after it
/// pushes a reply. 2 s timeout, woken after 50 ms.
#[cfg(feature = "standard")]
#[test]
fn standard_is_woken_by_the_waker() {
    let _g = shared();
    let (_uring, block) = standard(config(8, 4096, 2), 2_000);
    let (waker, handle) = fixbolt_engine::waker::Waker::new().expect("a pipe");
    let mut e = engine::blocking(block).with_waker(waker);

    let app = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        handle.wake();
    });
    let before = Instant::now();
    e.idle();
    let took = before.elapsed();
    app.join().expect("the application thread finished");
    assert!(
        took < Duration::from_millis(500),
        "a wake after 50 ms ended a 2 s wait only after {took:?}: the engine slept \
         through it and woke on the clock"
    );
}

/// Nothing to wake it: the wait ends on its own timeout, which is what
/// delivers `Input::Tick`. 50 ms, returned within [40, 500] ms.
#[cfg(feature = "standard")]
#[test]
fn standard_wakes_on_its_own_timeout_to_tick() {
    let _g = shared();
    let (_uring, mut block) = standard(config(8, 4096, 1), 50);
    let before = Instant::now();
    block.idle(&[]);
    let took = before.elapsed();
    assert!(
        took >= Duration::from_millis(40),
        "returned after {took:?}: it did not wait, so `standard` is spinning"
    );
    assert!(
        took <= Duration::from_millis(500),
        "returned after {took:?} against a 50 ms timeout: the tick is late"
    );
}

// ---------------------------------------------------------------------------
// Item 4: blocked means refused, named, never a fallback.
// ---------------------------------------------------------------------------

/// The table of ADR-0190 decision 7, without a kernel that refuses.
#[test]
fn the_refusal_is_classified_by_errno_and_sysctl() {
    use std::io::ErrorKind;
    let _g = shared();
    let cases = [
        (libc::EPERM, Some(0), UringRefused::Blocked),
        (libc::EPERM, None, UringRefused::Blocked),
        (libc::EPERM, Some(1), UringRefused::Disabled { sysctl: 1 }),
        (libc::EPERM, Some(2), UringRefused::Disabled { sysctl: 2 }),
        (libc::ENOSYS, Some(0), UringRefused::NotInKernel),
        (libc::EINVAL, Some(0), UringRefused::KernelTooOld),
        (
            libc::ENOMEM,
            Some(0),
            UringRefused::Other(ErrorKind::OutOfMemory),
        ),
    ];
    for (errno, sysctl, want) in cases {
        assert_eq!(
            UringRefused::classify(errno, sysctl),
            want,
            "errno {errno}, io_uring_disabled {sysctl:?}"
        );
    }
    let two = UringRefused::Disabled { sysctl: 2 }.to_string();
    assert!(two.contains("kernel.io_uring_disabled = 2"), "{two}");
    let one = UringRefused::Disabled { sysctl: 1 }.to_string();
    assert!(one.contains("kernel.io_uring_group"), "{one}");
    let blocked = UringRefused::Blocked.to_string();
    assert!(blocked.contains("seccomp"), "{blocked}");
}

#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH: u32 = 0xC000_003E;
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH: u32 = 0xC000_00B7;

/// Install, on the **calling thread only**, a seccomp filter that answers
/// `EPERM` to `io_uring_setup` and lets everything else through — what
/// Docker's default profile does. Needs no privilege: `no_new_privs` first.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn refuse_io_uring_setup_on_this_thread() {
    // Classic BPF opcodes, spelled as their parts. BPF_LD, BPF_W and BPF_K
    // are all zero, so only the non-zero parts are written.
    const LD_W_ABS: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
    const JEQ_K: u16 = 0x05 | 0x10; // BPF_JMP | BPF_JEQ | BPF_K
    const RET_K: u16 = 0x06; // BPF_RET | BPF_K
    const ARCH_AT: u32 = 4; // offsetof(struct seccomp_data, arch)
    const NR_AT: u32 = 0; // offsetof(struct seccomp_data, nr)
    let ins = |code, jt, jf, k| libc::sock_filter { code, jt, jf, k };
    let mut filter = [
        ins(LD_W_ABS, 0, 0, ARCH_AT),
        ins(JEQ_K, 0, 3, AUDIT_ARCH),
        ins(LD_W_ABS, 0, 0, NR_AT),
        ins(JEQ_K, 0, 1, libc::SYS_io_uring_setup as u32),
        ins(RET_K, 0, 0, libc::SECCOMP_RET_ERRNO | libc::EPERM as u32),
        ins(RET_K, 0, 0, libc::SECCOMP_RET_ALLOW),
    ];
    let prog = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };
    // SAFETY: `prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)` takes integers only.
    #[allow(unsafe_code)]
    let rc = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    assert_eq!(rc, 0, "no_new_privs: {}", std::io::Error::last_os_error());
    // SAFETY: `prog` points at `filter`, both alive for the call; the kernel
    // copies the program and keeps no pointer. No `TSYNC` flag: this thread
    // only, so the rest of the test binary is untouched.
    #[allow(unsafe_code)]
    let rc = unsafe {
        libc::syscall(
            libc::SYS_seccomp,
            libc::SECCOMP_SET_MODE_FILTER,
            0,
            &prog as *const libc::sock_fprog,
        )
    };
    assert_eq!(
        rc,
        0,
        "install the filter: {}",
        std::io::Error::last_os_error()
    );
}

/// A real seccomp filter answering `EPERM` to `io_uring_setup` is refused at
/// startup as [`UringRefused::Blocked`], and the message names seccomp.
///
/// On a thread of its own: the filter cannot be removed, and a test run with
/// `--test-threads=1` would otherwise carry it into every later test.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[test]
fn a_blocked_io_uring_refuses_to_start_and_names_seccomp() {
    let _g = shared();
    let sysctl = std::fs::read_to_string("/proc/sys/kernel/io_uring_disabled")
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
    println!("kernel.io_uring_disabled = {sysctl:?}");
    let refused = std::thread::spawn(|| {
        refuse_io_uring_setup_on_this_thread();
        Uring::hft(config(8, 4096, 1), HftArm::Enter).err()
    })
    .join()
    .expect("the filtered thread finished");

    let Some(e) = refused else {
        panic!("io_uring_setup is filtered with EPERM and a ring was made anyway")
    };
    assert_eq!(
        e,
        UringRefused::Blocked,
        "EPERM with io_uring_disabled = {sysctl:?} is a seccomp filter: {e}"
    );
    assert!(e.to_string().contains("seccomp"), "{e}");
}
