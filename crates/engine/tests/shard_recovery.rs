//! `serve_sharded_hft_with_recovery`, through its own front door.
//!
//! **Step 5 of [the-desk-free-residue], `STATUS.md` item 32 (a), the recovery
//! half.** `serve_with_recovery` and `serve_hft_with_recovery` let a
//! single-engine deployment resume its sequence numbers from a journal on disk;
//! until this file there was nothing equivalent behind the sharded doors, so a
//! sharded deployment could not resume at all —
//! [ADR-0088].
//!
//! # What this proves, and on which thread
//!
//! The counterparty is named on the **acceptor** thread, which ADR-0020 allows
//! to block, and the session is built on a **shard** thread, which in `hft`
//! mode never blocks (`CLAUDE.md` §2 non-negotiable 4). So what recovery found
//! has to travel between the two, and
//! [`Start`](fixbolt_engine::recovery::Start) is what travels. This test drives
//! the whole of that from outside: a `Recovery` that answers
//! `next_out: 5, next_in: 3`, a client whose `Logon` is `34=3`, and a reply that
//! must read `34=5`. A door that asked recovery on the wrong thread, dropped the
//! answer on the channel, or started the session fresh would answer `34=1`.
//!
//! `the_start_that_crosses_the_channel_is_send` is the other half and it is a
//! **compile-time** fact rather than a comment: `Start<J>` rides an
//! `mpsc::Sender`, so a journal that is not `Send` must not compile. ADR-0088
//! decision 4.
//!
//! # What this does NOT prove
//!
//! The same limitation `shard_hft.rs` records. This is a **behaviour** gate
//! saying the door opens, resumes and serves. It says nothing about whether the
//! shard threads sleep in the kernel — that is non-negotiable 4 and belongs to
//! `scripts/check-no-kernel-sleep.sh`. And the sharded runtime still **cannot be
//! stopped**: item 32's ordered-shutdown half is left open by ADR-0088 decision
//! 5, so this test leaks a spinning thread for the life of this binary, exactly
//! as `shard_hft.rs` does and for the same reason.
//!
//! [the-desk-free-residue]: ../../../docs/plans/2026-09-20-the-desk-free-residue.md
//! [ADR-0088]: ../../../docs/decisions/ADR-0088-recovery-reaches-the-sharded-runtime-and-the-journal-crosses-the-channel-with-the-connection.md

// Three conditions, and the third is the one `shard_hft.rs` learned from CI:
// `serve_sharded_hft_with_recovery` is behind `#[cfg(feature = "standard")]` as
// well as behind `shard`'s own `affinity` + Linux gate, because it owns the
// pre-session stage and that needs a poller. A file gated on its module's
// features rather than on the function's is non-negotiable 6 one level over.
#![cfg(all(feature = "affinity", feature = "standard", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source — non-negotiable 7 is about `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::Range;
use std::time::Duration;

use fixbolt_engine::affinity::{CoreId, ShardPlan, Topology};
use fixbolt_engine::journal::{FileJournal, Store};
use fixbolt_engine::presession::{Limits, Table};
use fixbolt_engine::recovery::{FromFn, Resumed, Start};
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

/// One physical core, or `None` on a machine that has none to give. The walk
/// `shard_hft.rs::one_shard` does, for the same reason.
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

/// Everything the engine still has to say inside `window`, `SOH` rendered `|`.
///
/// A drain, not a read: the pass is the window closing empty, so a timeout is
/// the expected ending and not a failure. The caller's own 5 s timeout is put
/// back afterwards, because that one guards a reply that *is* owed.
fn drain(client: &mut TcpStream, window: Duration) -> String {
    client.set_read_timeout(Some(window)).expect("timeout");
    let mut all = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match client.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => all.extend_from_slice(&buf[..n]),
            // `WouldBlock` on Linux, `TimedOut` elsewhere, or the peer went
            // away — every one of them means the engine said no more.
            Err(_) => break,
        }
    }
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    String::from_utf8_lossy(&all).replace('\u{1}', "|")
}

/// **The sharded door resumes the numbers a `Recovery` handed it — both of them.**
///
/// `next_out: 5` is the outbound number under test: it is not one, not the
/// client's, and nothing in the session could have derived it — it can only have
/// come from the acceptor thread, across the channel, into the shard thread's
/// `Session`.
///
/// `next_in: 3` is the inbound one, and **this test is the only thing in the
/// workspace that reads it**. It is what makes the client's `34=3` in sequence;
/// resumed with any other inbound number the same `Logon` is a gap, and the
/// session answers it *and* a `ResendRequest`. So the absence of `35=2` is the
/// assertion that carries `next_in` — without it, `r.next_in` in
/// `crates/engine/src/lib.rs` could be replaced by a literal `1` and nothing
/// here, in `shard_hft.rs`, or on the `serve_with_recovery` /
/// `serve_hft_with_recovery` paths that share that code would go red.
#[test]
fn a_sharded_acceptor_resumes_the_numbers_recovery_hands_it() {
    let Some(plan) = one_shard() else {
        panic!(
            "no online core to host one shard — Topology::read said so, and a skip here would be a test that skipped itself on every machine that ran it"
        );
    };

    let addr = free_addr();
    let serving = addr.clone();
    // Not joined: the success arm is `Infallible` and ADR-0088 decision 5
    // leaves the sharded runtime with no way to be asked to stop.
    std::thread::spawn(move || {
        let recovery = FromFn::new(|_cfg: &Config| {
            Some(Resumed {
                journal: Store::default(),
                next_out: 5,
                next_in: 3,
                last_active_ms: None,
            })
        });
        let err = fixbolt_engine::shard::serve_sharded_hft_with_recovery(
            &serving,
            Table::with_capacity(1).serving(cfg()),
            &plan,
            4,
            Limits::new(8, 30_000).expect("both above zero"),
            |_shard| EchoApp::default(),
            recovery,
            None,
        );
        eprintln!("serve_sharded_hft_with_recovery returned: {err:?}");
    });

    let mut client = connect(&addr);
    // `34=3` is what `next_in: 3` expects, so the `Logon` is in sequence and the
    // reply is a `Logon` rather than a `ResendRequest` or a reject.
    client.write_all(&logon_now(3)).expect("send the Logon");

    let mut buf = [0u8; 4096];
    let n = client.read(&mut buf).expect("a reply");
    let reply = String::from_utf8_lossy(&buf[..n]).replace('\u{1}', "|");

    assert!(
        reply.contains("|35=A|"),
        "the sharded hft acceptor answered the Logon: {reply}"
    );
    assert!(
        reply.contains("|34=5|"),
        "and it answered with the number recovery handed it, not with one: {reply}"
    );
    assert!(
        reply.contains("|49=ISLD|"),
        "and it answered as the acceptor the table names: {reply}"
    );
    assert!(
        !reply.contains("|35=2|"),
        "and it asked for no resend, because next_in: 3 made the client's 34=3 in sequence: {reply}"
    );

    // Once more over whatever else the engine has to say, because a resend need
    // not share a segment with the Logon reply. Nothing is owed on a session
    // whose `108=30` and 30 s window have not elapsed, so a window that closes
    // empty is the pass.
    let rest = drain(&mut client, Duration::from_millis(500));
    assert!(
        !rest.contains("|35=2|"),
        "and no resend arrived in a later segment either: {rest}"
    );
}

/// **What crosses the channel is `Send`, said by the compiler.**
///
/// ADR-0088 decision 4. `Start<J>` rides an `mpsc::Sender` between the acceptor
/// thread and a shard thread, and a journal on disk is the case that matters —
/// `FileJournal` owns a file handle, and "it is expected to be `Send`" is a
/// sentence, not a check. This is the check.
#[test]
fn the_start_that_crosses_the_channel_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Start<Store>>();
    assert_send::<Start<FileJournal<8, 256>>>();
}
