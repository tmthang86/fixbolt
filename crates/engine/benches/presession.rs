//! What the pre-session stage costs a connection.
//!
//! Step 6 of [pre-session-routing]. The stage exists because
//! `[measured 2026-08-31]` sharding took the acceptance corpus from 59 to 57,
//! and `[measured 2026-09-01]` it is back to 59 — but a fix nobody has priced
//! is a fix that gets quietly reverted the first time somebody measures the
//! connection path.
//!
//! # What it does NOT measure, and why that matters here
//!
//! **This is not on the message path.** `DESIGN.md` §8's budget is what one
//! message costs once a session is up, and nothing here happens then: the stage
//! touches a socket exactly once, before its session exists. So these numbers
//! do not belong in §8 and are not added to it.
//!
//! What they price is a **connection**, which for a FIX acceptor happens once
//! per counterparty per day rather than once per order.
//!
//! # Real sockets for the sweep, on purpose
//!
//! The sweep is a non-blocking `recv` per waiting socket, exactly like
//! `Engine::turn`'s. Over [`Loopback`](fixbolt_engine::transport::Loopback)
//! there would be no syscall in it, and the syscall is the whole cost —
//! `benches/turn.rs` says so at 420 ns of a 449 ns turn.
//!
//! [pre-session-routing]: ../../../docs/plans/2026-08-31-pre-session-routing.md
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[path = "../../codec/benches/harness.rs"]
mod harness;

use std::hint::black_box;
use std::net::{TcpListener, TcpStream};

use fixbolt_engine::presession::{
    HashRoute, Limits, One, PendingSet, Route, identity_of, is_logon,
};
use fixbolt_engine::transport::TcpTransport;
use fixbolt_session::Config;

/// The counterparty the acceptance corpus logs on as. The registry the sweep
/// runs in front of serves exactly it — ADR-0026.
fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

const PRE: usize = 4096;

/// A real `Logon` from the acceptance corpus, not an invented one.
fn a_logon() -> Vec<u8> {
    fixbolt_conformance::script::load_all()
        .expect("the corpus is fetched — scripts/fetch-quickfix-assets.sh")
        .into_iter()
        .find_map(|s| match s.kind {
            fixbolt_conformance::script::Kind::Send(m) if is_logon(&m.wire) => Some(m.wire),
            _ => None,
        })
        .expect("the corpus sends a Logon")
}

/// A set holding `n` connected, silent sockets.
///
/// The client ends are leaked deliberately: closing one would make the next
/// sweep see end-of-stream and drop it, and the bench would measure a table
/// emptying itself.
fn set_with(n: usize) -> PendingSet<TcpTransport, One, PRE> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");
    let mut set = PendingSet::new(
        Limits::new(n.max(1), u64::MAX).expect("both above zero"),
        One::new(cfg()),
    );
    for _ in 0..n {
        let client = TcpStream::connect(addr).expect("connect");
        let (server, _) = listener.accept().expect("accept");
        core::mem::forget(client);
        assert!(
            set.admit(TcpTransport::new(server).expect("non-blocking"), 0)
                .is_ok(),
            "the ceiling is n"
        );
    }
    assert_eq!(set.len(), n, "the sweep must have {n} sockets");
    set
}

fn main() {
    harness::suite(|b| {
        // 1 and 16, the same shape `benches/turn.rs` uses, so the two sweeps can
        // be compared without a second machine or a second day.
        for n in [1usize, 16] {
            let mut set = set_with(n);
            // Prove the path runs before timing it: a sweep over an empty table
            // is a different, faster lie.
            assert_eq!(set.turn(0).settled, 0, "nobody has said anything yet");
            assert_eq!(set.len(), n, "and nobody was dropped");
            b.bench(&format!("presession sweep, {n} quiet sockets"), || {
                black_box(set.turn(black_box(0)));
            });
        }

        // The whole per-connection decision, with no socket in it: read both
        // comp IDs off the bytes and pick a shard. This is what the stage adds
        // over the `recv` it would have had to do anyway.
        {
            let wire = a_logon();
            let mut route = HashRoute;
            assert!(
                identity_of(&wire).is_some(),
                "the corpus Logon names both sides"
            );
            b.bench("presession, read and route an identity", || {
                let id = identity_of(black_box(&wire)).expect("named");
                black_box(route.shard_for(id, black_box(8)));
            });
        }
    });
}
