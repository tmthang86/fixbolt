//! `serve_sharded_hft`, through its own front door.
//!
//! **Step 3 of [the-hft-front-doors-have-no-gate], and the half that cannot be
//! run from the machine it was written on.** `shard` sits behind
//! `cfg(all(feature = "affinity", target_os = "linux"))`
//! (`crates/engine/src/lib.rs:40-41`), so on darwin this file does not compile
//! rather than failing at runtime — ADR-0014 decision 2. Whether it is green is
//! something **CI says**, and saying otherwise from a laptop is the inferred
//! green `CLAUDE.md` §10 ends on.
//!
//! # Two shapes here that are not in `hft_wire.rs`, and both are known defects
//!
//! * **`serve_sharded_hft` returns `Result<Infallible, ShardError>`** — the
//!   success arm is uninhabited, so it never returns except as an error. There
//!   is nothing to join.
//! * **It cannot be stopped.** `STATUS.md` item 32 (a): it takes no
//!   `observe::Handles` and has no `_with_recovery` sibling, so there is no
//!   `Admin` to `shutdown`. This test therefore **leaks a spinning thread** for
//!   the life of this test binary. That is survivable because this is its own
//!   binary and the process exits with it — and it is stated rather than left
//!   for somebody to find, because a spinning thread is exactly the cost D8
//!   trades for.
//!
//! Neither is fixed here. The plan forbids touching `crates/*/src`, and item
//! 32 (a) has been open since 2026-09-02.
//!
//! # What this does NOT prove
//!
//! The same limitation `hft_wire.rs` records, for the same reason: this is a
//! **behaviour** gate saying the door opens and serves. It says nothing about
//! whether the shard threads sleep in the kernel, which is non-negotiable 4 and
//! belongs to `scripts/check-no-kernel-sleep.sh`. ADR-0013 decision 4: a claim
//! that does not name its mode is incomplete, and this one names `hft` and
//! claims only reachability.
//!
//! [the-hft-front-doors-have-no-gate]: ../../../docs/plans/2026-09-10-the-hft-front-doors-have-no-gate.md

#![cfg(all(feature = "affinity", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source — non-negotiable 7 is about `crates/*/src`.
//
// `[2026-09-08]` and this file is invisible to a clippy run under DEFAULT
// features, which is how `shard_wire.rs` kept a missing annotation for a while.
// It is caught by CI's own `--features affinity` step.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::time::Duration;

use fixbolt_engine::affinity::{CoreId, ShardPlan, Topology};
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::{Application, Config};

#[derive(Default)]
struct EchoApp(fixbolt_conformance::echo::Echo);

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        hdr: fixbolt_session::Header<'_>,
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        let (seq, stamp) = (hdr.seq, hdr.stamp);
        self.0.reply(msg, seq, stamp, out)
    }
}

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// One physical core, or `None` on a machine that has none to give.
///
/// The same walk `shard_wire.rs::plan_for` does, for one shard rather than two:
/// `[measured 2026-08-31]` a GitHub runner's two vCPUs are two threads of one
/// physical core, so two shards are refused there — correctly. One is not.
///
/// `allow_unisolated` is said out loud: CI has no `isolcpus` and neither does a
/// laptop.
fn one_shard() -> Option<ShardPlan> {
    let topology = Topology::read().expect("reading /sys on Linux");
    let first = topology.online().first().copied()?;
    let cores: Vec<CoreId> = vec![first];
    Some(ShardPlan::new(cores).allow_unisolated())
}

fn free_addr() -> String {
    let l = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let a = l.local_addr().expect("bound").to_string();
    drop(l);
    a
}

fn connect(addr: &str) -> TcpStream {
    for _ in 0..500 {
        if let Ok(s) = TcpStream::connect(addr) {
            s.set_nodelay(true).expect("nodelay");
            s.set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            return s;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the sharded hft loop never bound {addr}");
}

/// A `Logon` stamped now, because these loops build a `SystemClock`.
fn logon_now(seq: u32) -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis() as u64;
    let mut cache = fixbolt_codec::timestamp::TimestampCache::new();
    let full = cache.format(now, 0);
    let stamp = core::str::from_utf8(&full[..17]).expect("ascii");
    let inner = format!(
        "35=A\u{1}34={seq}\u{1}49=TW44\u{1}52={stamp}\u{1}56=ISLD\u{1}98=0\u{1}108=30\u{1}"
    );
    let framed = format!("8=FIX.4.4\u{1}9={}\u{1}{inner}10=0\u{1}", inner.len());
    fixbolt_conformance::script::with_real_checksum(framed.as_bytes())
}

/// **`serve_sharded_hft` binds and brings a session up.**
///
/// The first thing in this repository to call it.
#[test]
fn serve_sharded_hft_serves_a_session() {
    let Some(plan) = one_shard() else {
        panic!(
            "no online core to host one shard — Topology::read said so, and a skip here would be a test that skipped itself on every machine that ran it"
        );
    };

    let addr = free_addr();
    let serving = addr.clone();
    // **Not joined, because there is nothing to join** — the success arm is
    // `Infallible` and item 32 (a) leaves no way to ask it to stop. If it fails
    // to bind, the error is what the `connect` below cannot find a listener for.
    std::thread::spawn(move || {
        let err = fixbolt_engine::shard::serve_sharded_hft(
            &serving,
            Table::with_capacity(1).serving(cfg()),
            &plan,
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            |_shard| EchoApp::default(),
            None,
        );
        // Reached only on the error arm. Printed rather than swallowed: a bind
        // failure here would otherwise show up as a connect timeout and be
        // diagnosed as the wrong thing.
        eprintln!("serve_sharded_hft returned: {err:?}");
    });

    let mut client = connect(&addr);
    client.write_all(&logon_now(1)).expect("send the Logon");

    let mut buf = [0u8; 4096];
    let n = client.read(&mut buf).expect("a reply");
    let reply = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");

    assert!(
        reply.contains("|35=A|"),
        "the sharded hft acceptor answered the Logon: {reply}"
    );
    assert!(
        reply.contains("|34=1|"),
        "and a session nobody resumed starts at one: {reply}"
    );
    assert!(
        reply.contains("|49=ISLD|"),
        "and it answered as the acceptor the table names: {reply}"
    );
}
