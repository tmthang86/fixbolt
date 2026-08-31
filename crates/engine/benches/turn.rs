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
    fn on_message(&mut self, _: &[u8], _: u32, _: &[u8], _: &mut [u8]) -> Option<Range<usize>> {
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
    });
}
