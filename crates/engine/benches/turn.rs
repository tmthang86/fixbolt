//! What a turn of the engine loop actually costs, per session.
//!
//! Step 6 of `plans/2026-08-30-threads-and-affinity.md`. `DESIGN.md` §8 has
//! carried **703 ns per session per turn** since 2026-08-30, and that number is
//! a floor measured with a C program doing a bare non-blocking `read` — not this
//! engine doing a turn. This measures the real thing.
//!
//! # Why the sessions are idle, and why that is the case that matters
//!
//! `[measured 2026-08-30]` D8 makes an idle turn one non-blocking `read` per
//! connection, and the idle turn is the overwhelming majority of turns an engine
//! ever runs. A busy turn is measured elsewhere (`benches/dispatch.rs` for the
//! dispatch hop, `benches/alloc.rs` for the whole exchange); this is the sweep.
//!
//! # Real sockets, on purpose
//!
//! [`Loopback`](fixbolt_engine::transport::Loopback) has no kernel in it and
//! would measure the sweep without the syscall that dominates it. The whole
//! point of the comparison with 703 ns is the syscall.
//!
//! # The idle loop, kernel against `io_uring` (phase 4 row 5)
//!
//! Behind the `io-uring` feature, one pair per N in 1, 16, 64, paired by
//! name — the same prefix, `, kernel` or `, uring` after it — for row 7 to
//! record (ADR-0190 decision 10). **Each iteration is one whole idle turn:
//! `turn()` and then `idle()`.** With `io_uring`, `turn()` makes no system call
//! at all — the one kernel entry is in `UringSpin::idle` — so timing `turn()`
//! alone against the kernel arm's `turn()` would credit the ring with the
//! syscall it merely moved. The kernel arm idles by `Spin`, which does not
//! enter the kernel, so both sides are "one idle turn as an `hft` engine runs
//! it". The existing cases above are untouched. No figure from here is
//! published by this row (non-negotiable 10).
//!
//! # What this does NOT measure
//!
//! **Sharding.** `N` here is sessions on **one** engine, which is what a shard
//! holds; the total for M shards is M threads each doing this, and that is a
//! wire-to-wire question for `tools/w2w` rather than a per-iteration one. The
//! arithmetic `GUIDE.md` §1a states — 8 shards of 13 sessions instead of one of
//! 104 — is exactly the arithmetic these numbers are the input to.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[path = "../../codec/benches/harness.rs"]
mod harness;

use std::hint::black_box;
use std::net::{TcpListener, TcpStream};
use std::ops::Range;

use fixbolt_engine::Engine;
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::transport::TcpTransport;
use fixbolt_engine::wait::Yield;
use fixbolt_session::{Application, Config};

/// Answers nothing. A handler that did anything would be measured instead of
/// the sweep, and on an idle turn it is never called at all.
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

type TurnEngine = Engine<
    TcpTransport,
    fixbolt_session::Acceptor,
    InlineDispatch<Silent>,
    ManualClock,
    Yield,
    Store,
    256,
    4096,
    8192,
>;

/// An engine holding `n` connected, quiet TCP sessions.
///
/// The client ends are leaked deliberately: closing them would turn every
/// subsequent `recv` into an end-of-stream and the bench would measure an engine
/// tearing itself down.
fn engine_with(n: usize) -> TurnEngine {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");
    let mut engine: TurnEngine = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(Silent),
        // Fixed, so no session decides mid-run that a heartbeat is due and the
        // sweep stops being a sweep.
        ManualClock::at(fixbolt_conformance::script::FIXED_TIME_MILLIS),
        Yield,
        n.max(1),
    );
    for _ in 0..n {
        let client = TcpStream::connect(addr).expect("connect");
        let (server, _) = listener.accept().expect("accept");
        core::mem::forget(client);
        engine.add(TcpTransport::new(server).expect("non-blocking"));
    }
    assert_eq!(engine.connections(), n, "the sweep must have {n} sessions");
    engine
}

/// The idle-loop pair, `io-uring` builds only: `n` quiet TCP sessions on an
/// engine of transport `T` idling by `W`, timed one whole idle turn at a time.
#[cfg(all(feature = "io-uring", target_os = "linux"))]
mod idle_loop {
    use super::{Silent, harness};
    use fixbolt_engine::Engine;
    use fixbolt_engine::clock::ManualClock;
    use fixbolt_engine::dispatch::InlineDispatch;
    use fixbolt_engine::journal::Store;
    use fixbolt_engine::transport::uring::{HftArm, Uring, UringConfig};
    use fixbolt_engine::transport::{TcpTransport, Transport};
    use fixbolt_engine::wait::{Spin, Waiting};
    use fixbolt_session::Config;
    use std::hint::black_box;
    use std::net::{TcpListener, TcpStream};

    type IdleEngine<T, W> = Engine<
        T,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        W,
        Store,
        256,
        4096,
        8192,
    >;

    fn engine<T: Transport, W: Waiting>(
        n: usize,
        wait: W,
        mut wrap: impl FnMut(TcpTransport) -> Option<T>,
    ) -> IdleEngine<T, W> {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let addr = listener.local_addr().expect("bound");
        let mut engine: IdleEngine<T, W> = Engine::new(
            Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
            InlineDispatch::new(Silent),
            ManualClock::at(fixbolt_conformance::script::FIXED_TIME_MILLIS),
            wait,
            n,
        );
        for _ in 0..n {
            let client = TcpStream::connect(addr).expect("connect");
            let (server, _) = listener.accept().expect("accept");
            core::mem::forget(client);
            let t = wrap(TcpTransport::new(server).expect("non-blocking"))
                .expect("a transport for every session");
            engine.add(t);
        }
        // Prove the path runs before timing it, as the cases above do.
        engine.turn();
        engine.idle();
        assert_eq!(engine.connections(), n, "the pair must have {n} sessions");
        engine
    }

    pub fn pairs(b: &mut harness::Suite) {
        for n in [1usize, 16, 64] {
            let mut kernel = engine(n, Spin, Some);
            b.bench(&format!("idle loop, {n} idle sessions, kernel"), || {
                black_box(kernel.turn());
                kernel.idle();
            });
            drop(kernel);

            let ring = UringConfig::new(64, 4096, 64).expect("a valid ring size");
            let (uring, spin) = Uring::hft(ring, HftArm::Enter)
                .unwrap_or_else(|e| panic!("the uring arm needs a ring: {e}"));
            let mut ringed = engine(n, spin, |t| uring.register(t));
            b.bench(&format!("idle loop, {n} idle sessions, uring"), || {
                black_box(ringed.turn());
                ringed.idle();
            });
            let r = uring.report();
            assert_eq!(
                r.enter_errors, 0,
                "the ring's enter failed while timed: {r:?}"
            );
            assert_eq!(
                ringed.connections(),
                n,
                "and still {n} sessions after: {r:?}"
            );
        }
    }
}

fn main() {
    harness::suite(|b| {
        // The bare syscall, in the same binary and the same run as the turns
        // below. Without it the comparison against `DESIGN.md` §8's 703 ns
        // would be across two programs and two days, which is a comparison this
        // repository has been burned by before. With it, "what does a turn add
        // over the read it is made of" is a subtraction inside one run.
        {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
            let addr = listener.local_addr().expect("bound");
            let client = TcpStream::connect(addr).expect("connect");
            let (server, _) = listener.accept().expect("accept");
            core::mem::forget(client);
            let mut socket = TcpTransport::new(server).expect("non-blocking");
            let mut buf = [0u8; 4096];
            assert!(
                matches!(
                    fixbolt_engine::transport::Transport::recv(&mut socket, &mut buf),
                    fixbolt_engine::transport::Io::Idle
                ),
                "the socket must be quiet, or this measures a different syscall"
            );
            b.bench("recv on a quiet socket", || {
                black_box(fixbolt_engine::transport::Transport::recv(
                    &mut socket,
                    black_box(&mut buf),
                ));
            });
        }

        // 1 is the shape ADR-0012 chose and the one every latency figure here is
        // quoted at. 4 and 16 are wide enough to show whether the per-session
        // cost is flat, which is the claim 703 ns was published with.
        for n in [1usize, 4, 16] {
            let mut engine = engine_with(n);
            // Prove the path runs before timing it: a turn that moved nothing
            // because there is nothing to move is the case, but a turn on an
            // engine with no connections would be a different, faster lie.
            engine.turn();
            assert_eq!(
                engine.connections(),
                n,
                "and must still have them after a turn"
            );
            b.bench(&format!("engine turn, {n} idle sessions"), || {
                black_box(engine.turn());
            });
        }

        #[cfg(all(feature = "io-uring", target_os = "linux"))]
        idle_loop::pairs(b);
    });
}
