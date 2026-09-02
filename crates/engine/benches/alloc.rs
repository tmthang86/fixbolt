//! Non-negotiable 1, for the engine: **zero** allocations on the path a byte
//! actually takes.
//!
//! `crates/codec/benches/alloc.rs` proves it for parse and encode and
//! `crates/session/benches/alloc.rs` for the state machine. This proves it for
//! the layer that touches the socket — the one where a per-read `Vec`, a
//! per-connection `String` for an address, or a `format!` in an error path
//! would be easiest to reach for.
//!
//! # The `unsafe` here
//!
//! Identical to the other two benches and sound for the same three reasons:
//! every method forwards to `System` unchanged but for a relaxed counter; this
//! is a benchmark binary, so nothing ships it; and it is proven by reversal,
//! not by reading.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};

use std::ops::Range;

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::{Dispatch, InlineDispatch, RingApp, RingDispatch};
use fixbolt_engine::frame::{Cut, Framer};
use fixbolt_engine::journal::Store;
use fixbolt_engine::presession::{Limits, One, PendingSet, Registry, Table};
use fixbolt_engine::ring;
use fixbolt_engine::transport::{Interest, Io, Loopback, TcpTransport, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};

/// The counterparty the acceptance corpus logs on as. The registry the sweep
/// runs in front of serves exactly it — ADR-0026.
fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System`, which is a correct allocator, with
// the same pointer, layout and size it was given. The only addition is a
// relaxed counter increment. See the module comment.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

/// An application that never answers. The engine's own paths are what is being
/// measured here, not an application's.
struct Silent;

impl Application for Silent {
    fn on_message(&mut self, _: &[u8], _: u32, _: &[u8], _: &mut [u8]) -> Option<Range<usize>> {
        None
    }
}

/// A whole message from its body: `8=`, `9=`, the identity fields and `10=`
/// supplied around it.
///
/// The `52=` is the corpus's fixed instant, which is also what the engine's
/// clock reads — a `SendingTime` two days out is refused for skew, and the
/// bench would then measure the refusal path and call it the message path.
fn wire(body: &str) -> Vec<u8> {
    let body = format!("{body}49=TW44\x0152=20260828-12:00:00.000\x0156=ISLD\x01");
    with_real_checksum(format!("8=FIX.4.4\x019={}\x01{body}10=0\x01", body.len()).as_bytes())
}

/// Copies the message back, so the return direction of the ring is exercised.
struct Bounce;

impl Application for Bounce {
    fn on_message(&mut self, msg: &[u8], _: u32, _: &[u8], out: &mut [u8]) -> Option<Range<usize>> {
        let n = msg.len().min(out.len());
        out[..n].copy_from_slice(&msg[..n]);
        Some(0..n)
    }
}

fn count<F: FnOnce()>(f: F) -> usize {
    let before = ALLOCS.load(Ordering::Relaxed);
    f();
    ALLOCS.load(Ordering::Relaxed) - before
}

fn main() {
    // Port 0: the OS picks a free one. A bench that hard-codes a port fails on
    // a busy machine for a reason that has nothing to do with FIX.
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");
    let client = TcpStream::connect(addr).expect("connect");
    let (accepted, _) = listener.accept().expect("accept");
    let mut server = TcpTransport::new(accepted).expect("non-blocking");
    let mut client = TcpTransport::new(client).expect("non-blocking");

    // `9=26` is the real body length of what follows — a bench that framed a
    // message with a wrong `9=` would measure the rubbish path and call it the
    // message path.
    let msg = b"8=FIX.4.4\x019=26\x0135=A\x0134=1\x0149=ISLD\x0156=TW44\x0110=0\x01";
    let mut buf = [0u8; 512];

    // Warm anything lazy, and prove both paths are the paths they claim to be:
    // a zero below must mean "did not allocate", never "did not run".
    assert_eq!(
        client.send(msg),
        Io::Ready(msg.len()),
        "the send path sends"
    );
    let mut got = 0;
    for _ in 0..10_000 {
        if let Io::Ready(n) = server.recv(&mut buf) {
            got = n;
            break;
        }
    }
    assert_eq!(got, msg.len(), "the recv path receives");

    // A quiet socket is the commonest turn of the loop by far, and the one
    // where an allocation would be paid for every spin.
    let idle_allocs = count(|| {
        for _ in 0..10_000 {
            let _ = server.recv(&mut buf);
        }
    });

    let send_allocs = count(|| {
        for _ in 0..10_000 {
            let _ = client.send(msg);
        }
    });

    let recv_allocs = count(|| {
        for _ in 0..10_000 {
            let _ = server.recv(&mut buf);
        }
    });

    // Framing is the other thing every byte goes through.
    let mut framer: Framer<4096> = Framer::new();
    {
        let spare = framer.spare();
        spare[..msg.len()].copy_from_slice(msg);
        framer.filled(msg.len());
        assert!(
            matches!(framer.cut(), Cut::Message(n) if n == msg.len()),
            "the framing path must actually cut a message"
        );
        framer.take(msg.len());
    }

    let frame_allocs = count(|| {
        for _ in 0..10_000 {
            let spare = framer.spare();
            spare[..msg.len()].copy_from_slice(msg);
            framer.filled(msg.len());
            match framer.cut() {
                Cut::Message(n) | Cut::Garbage(n) => framer.take(n),
                Cut::Need => {}
            }
        }
    });

    // A whole turn of the loop, over a transport with no kernel in it: accept
    // nothing, read nothing, tick every session. This is what the engine thread
    // does on the overwhelming majority of its turns.
    let (mut peer, engine_side) = Loopback::pair();
    let mut engine: Engine<
        Loopback,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Yield,
        Store,
        256,
        4096,
        8192,
    > = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let _ = engine.add(engine_side);
    assert_eq!(
        engine.connections(),
        1,
        "the turn path must have a connection"
    );
    engine.turn();

    let turn_allocs = count(|| {
        for _ in 0..10_000 {
            engine.turn();
        }
    });

    // The shard runtime's loop, minus the thread it runs on: drain the channel
    // of newly accepted connections, then turn. `try_recv` on an empty channel
    // is the only thing `shard.rs` adds to a quiet turn, and it runs once per
    // turn per shard for as long as the process lives.
    //
    // `[measured 2026-08-31]` the syscall half of this question is already
    // answered — two million `try_recv` calls make none
    // (`reference/measured-costs.md`). This is the other half.
    //
    // Its own engine, not the one above: adding a connection to that one would
    // break its later assertion that it still holds exactly the session it
    // started with, and a bench case that quietly changes another case's
    // premise is how a suite starts lying.
    let mut sharded: Engine<
        Loopback,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Yield,
        Store,
        256,
        4096,
        8192,
    > = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let (shard_tx, shard_rx) = std::sync::mpsc::channel::<Loopback>();
    {
        // Prove the drain path is the path before counting a zero from it. The
        // same rule as everywhere else in this file: a zero must mean "did not
        // allocate", never "did not run".
        let (_peer3, engine_side3) = Loopback::pair();
        shard_tx.send(engine_side3).expect("the receiver is alive");
        while let Ok(t) = shard_rx.try_recv() {
            sharded.add(t);
        }
        assert_eq!(
            sharded.connections(),
            1,
            "the shard drain must actually hand a connection to the engine"
        );
        sharded.turn();
    }
    // `shard_tx` stays alive, so `try_recv` below reports Empty rather than
    // Disconnected — the state a running shard is actually in.
    let shard_turn_allocs = count(|| {
        for _ in 0..10_000 {
            while let Ok(t) = shard_rx.try_recv() {
                sharded.add(t);
            }
            sharded.turn();
        }
    });
    assert_eq!(
        sharded.connections(),
        1,
        "and it must still hold that connection after the count"
    );
    drop(shard_tx);

    // And turns that actually carry messages: a Logon, then a thousand
    // Heartbeats with sequence numbers that keep going up.
    //
    // **Every message is rendered before the count starts**, on purpose.
    // Rendering allocates here — it is a `format!` in a harness, not on any
    // path this bench is about — and counting it would report the harness's
    // allocations as the engine's.
    //
    // `[measured 2026-08-30]` an earlier version of this case sent the *same*
    // Logon a thousand times. The session refused the second as a sequence
    // number already used and dropped the link, so from iteration three onward
    // the bench measured an engine with no connections while `Loopback`'s
    // queue grew unboundedly behind it. It reported "1 allocation", which was
    // that queue doubling — a number that looked like a near-pass and was
    // measuring nothing at all. `[trap recorded]` in
    // `docs/reference/measured-costs.md`.
    let traffic: Vec<Vec<u8>> = core::iter::once(wire("35=A\x0134=1\x0198=0\x01108=30\x01"))
        .chain((2..=1_000).map(|n| wire(&format!("35=0\x0134={n}\x01"))))
        .collect();

    // `Loopback`'s queue is a `VecDeque` and it grows by doubling, so the first
    // iterations allocate in the *fake*. Run the whole exchange once against a
    // throwaway engine to reach its steady capacity, then count a second one.
    let mut sink = [0u8; 4096];
    for m in &traffic {
        let _ = peer.send(m);
        engine.turn();
        let _ = peer.recv(&mut sink);
    }
    assert_eq!(
        engine.connections(),
        1,
        "the busy path must still hold a live session after a thousand messages"
    );

    let (mut peer2, engine_side2) = Loopback::pair();
    let mut engine2: Engine<
        Loopback,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Yield,
        Store,
        256,
        4096,
        8192,
    > = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let _ = engine2.add(engine_side2);
    let _ = peer2.send(&traffic[0]);
    engine2.turn();
    let _ = peer2.recv(&mut sink);

    let busy_allocs = count(|| {
        for m in &traffic[1..] {
            let _ = peer2.send(m);
            engine2.turn();
            let _ = peer2.recv(&mut sink);
        }
    });
    assert_eq!(
        engine2.connections(),
        1,
        "and it must still be live after the count, not dropped at message two"
    );

    // The ring, both directions. It allocates **once**, in `ring::pair`, and
    // never again — which is the whole point of a fixed buffer shared between
    // two threads. Everything below is the steady state.
    let (to_app, from_engine) = ring::pair(1 << 16);
    let (to_engine, from_app) = ring::pair(1 << 16);
    let mut ringed: RingDispatch<1024> = RingDispatch::new(to_app, from_app);
    let mut app: RingApp<1024> = RingApp::new(from_engine, to_engine);
    let stamp = b"20260828-12:00:00.000";
    let mut reply = [0u8; 1024];
    let order = &traffic[1];

    // Prove the path is the path: a message goes across and one comes back.
    ringed.deliver(0, order, 2, stamp, &mut reply);
    assert_eq!(
        app.pump(&mut Bounce),
        1,
        "the ring path must carry a message"
    );
    let mut came_back = 0usize;
    ringed.collect(|_, b| came_back += b.len());
    assert!(came_back > 0, "and must carry a reply back");

    let ring_allocs = count(|| {
        for _ in 0..10_000 {
            ringed.deliver(0, order, 2, stamp, &mut reply);
            app.pump(&mut Bounce);
            ringed.collect(|_, b| {
                core::hint::black_box(b);
            });
        }
    });
    assert_eq!(ringed.refused(), 0, "no case above met a full ring");

    // `standard`'s idle turn rebuilds the list of sources to wait on, every
    // time — a `Source` borrows a descriptor, so one cached across a turn can
    // name a socket that has since closed and been reissued. Rebuilding is
    // therefore not optional, and it is on the idle path, which is the one the
    // engine spends nearly all its turns on.
    //
    // Real sockets, because `Loopback` has no descriptor and would contribute
    // nothing to measure. The listener is passed as the `extra` source, so the
    // measured path is exactly what `serve` calls.
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");
    let acceptor = fixbolt_engine::Acceptor::bind("127.0.0.1:0").expect("a free port");
    let extra: Vec<Interest> = acceptor
        .source()
        .map(Interest::readable)
        .into_iter()
        .collect();
    let mut tcp: Engine<
        TcpTransport,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Yield,
        Store,
        256,
        4096,
        8192,
    > = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    for _ in 0..4 {
        let client = TcpStream::connect(addr).expect("connect");
        let (server, _) = listener.accept().expect("accept");
        core::mem::forget(client);
        tcp.add(TcpTransport::new(server).expect("non-blocking"));
    }
    assert_eq!(
        tcp.connections(),
        4,
        "the interest path must have connections"
    );
    assert_eq!(
        tcp.refresh_interests_with(&extra).len(),
        4 + extra.len(),
        "and the list it builds must not be empty"
    );

    let interests_allocs = count(|| {
        for _ in 0..10_000 {
            core::hint::black_box(tcp.refresh_interests_with(&extra));
        }
    });
    assert_eq!(
        tcp.sources_missing(),
        0,
        "every connection named its socket"
    );

    // --- the pre-session stage ---------------------------------------------
    //
    // ADR-0020 decision 1 puts this on the acceptor thread, which is allowed to
    // block — but it is NOT allowed to allocate per connection or per turn.
    // Everything comes from the ceiling the caller named, once, in `new`.
    //
    // Two cases, because the empty sweep alone would pass a `PendingSet` that
    // allocated on every `admit`: the second holds live sockets and turns them.
    let mut idle_set: PendingSet<Loopback, One, 1024> = PendingSet::new(
        Limits::new(64, 30_000).expect("both above zero"),
        One::new(cfg()),
    );
    assert!(idle_set.is_empty(), "the empty sweep must really be empty");
    let pending_idle_allocs = count(|| {
        for _ in 0..100_000 {
            core::hint::black_box(idle_set.turn(FIXED_TIME_MILLIS));
        }
    });

    let mut busy_set: PendingSet<Loopback, One, 1024> = PendingSet::new(
        Limits::new(64, 30_000).expect("both above zero"),
        One::new(cfg()),
    );
    let mut peers = Vec::new();
    for _ in 0..8 {
        let (near, far) = Loopback::pair();
        assert!(
            busy_set.admit(near, FIXED_TIME_MILLIS).is_ok(),
            "the ceiling is 64"
        );
        peers.push(far);
    }
    assert_eq!(busy_set.len(), 8, "the busy sweep must have sockets");
    let pending_busy_allocs = count(|| {
        for _ in 0..10_000 {
            core::hint::black_box(busy_set.turn(FIXED_TIME_MILLIS));
        }
    });
    assert_eq!(
        busy_set.len(),
        8,
        "and must still have them — a sweep that dropped them measured nothing"
    );

    // A `Table` lookup, on the connection path. ADR-0026's own Consequences
    // name this as the easiest invariant in that design to break: "an
    // implementation that allocates puts an allocation on a path
    // `benches/alloc.rs` currently proves is zero". The three cases above use
    // `One`, which compares and returns; this is the default `Table`, with forty
    // entries, scanned linearly.
    //
    // Forty because that is the order of a broker gateway's counterparty list
    // and because a one-entry table would find its answer first every time,
    // which is the shape that cannot fail.
    let mut forty = Table::with_capacity(40);
    for i in 0..40u8 {
        let mut them = *b"CP00";
        them[2] = b'0' + i / 10;
        them[3] = b'0' + i % 10;
        forty = forty.serving(Config::acceptor(b"FIX.4.4", b"ISLD", &them));
    }
    forty = forty.serving(cfg());
    let logon = wire("35=A\x0134=1\x0198=0\x01108=30\x01");
    let looked_up = {
        let id = fixbolt_engine::presession::identity_of(&logon).expect("names both sides");
        forty.lookup(id).is_some()
    };
    assert!(looked_up, "the table serves the corpus counterparty");
    let registry_lookup_allocs = count(|| {
        for _ in 0..100_000 {
            let id =
                fixbolt_engine::presession::identity_of(core::hint::black_box(&logon)).expect("id");
            core::hint::black_box(forty.lookup(id).is_some());
        }
    });

    // The whole per-connection cycle, and it is here because the two cases
    // above could NOT fail. `[measured 2026-09-01]` replacing
    // `Vec::with_capacity(ceiling)` with `Vec::new()` — so every `admit` grows
    // the table — left both of them reading 0, because `admit` ran outside
    // `count`. A guard that cannot go red is not a guard; this one goes red on
    // exactly that change.
    //
    // The far ends are kept alive so the only allocations in the window are the
    // set's own.
    let mut cycle_set: PendingSet<Loopback, One, 1024> = PendingSet::new(
        Limits::new(64, 30_000).expect("both above zero"),
        One::new(cfg()),
    );
    let mut kept = Vec::with_capacity(64);
    for _ in 0..64 {
        let (near, far) = Loopback::pair();
        kept.push((Some(near), far));
    }
    let cycle_allocs = count(|| {
        for slot in &mut kept {
            let Some(near) = slot.0.take() else { continue };
            if cycle_set.admit(near, FIXED_TIME_MILLIS).is_err() {
                continue;
            }
            core::hint::black_box(cycle_set.turn(FIXED_TIME_MILLIS));
        }
        while let Some(i) = cycle_set.settled() {
            core::hint::black_box(cycle_set.take(i));
        }
    });
    assert_eq!(
        cycle_set.len(),
        64,
        "64 admitted, none settled, none expired — the window did the work"
    );

    // --- being observable ---------------------------------------------------
    //
    // `observe`'s central claim is that an engine nobody is watching pays one
    // relaxed load per turn, and that an engine somebody *is* watching still
    // allocates nothing. Both halves are counted, because only the second one
    // would catch a `Snapshot` that grew a `Vec` of sessions.
    //
    // Its own engine, with a live session, so `observe-asked` really walks a
    // connection rather than describing an empty list.
    let (mut watched_peer, watched_side) = Loopback::pair();
    let mut watched: Engine<
        Loopback,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Yield,
        Store,
        256,
        4096,
        8192,
    > = Engine::new(
        cfg(),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let watcher = watched.observer();
    watched.add(watched_side);
    let _ = watched_peer.send(&traffic[0]);
    watched.turn();
    let _ = watched_peer.recv(&mut sink);
    assert_eq!(
        watched.connections(),
        1,
        "the observed engine must hold a session to describe"
    );

    // Nobody asking: the flag is false and `snapshot` never runs.
    let observe_idle_allocs = count(|| {
        for _ in 0..10_000 {
            watched.turn();
        }
    });
    assert_eq!(
        watcher.published(),
        0,
        "nobody asked, so nothing should have been published"
    );

    // Somebody asking on every turn — the worst case an operator can create,
    // and the one that would expose an allocating `Snapshot`.
    let first = {
        let _ = watcher.request();
        watched.turn();
        watcher.request().expect("the engine published on request")
    };
    assert_eq!(
        first.sessions().len(),
        1,
        "and the snapshot must describe that session, not an empty list"
    );
    assert!(
        first.healthy(),
        "a logged-on session with no refusals: {first:?}"
    );
    let observe_asked_allocs = count(|| {
        for _ in 0..10_000 {
            let _ = watcher.request();
            watched.turn();
            core::hint::black_box(watcher.request());
        }
    });
    assert!(
        watcher.published() >= 10_000,
        "asked ten thousand times, published {} — the window must have done the work",
        watcher.published()
    );

    // --- events -------------------------------------------------------------
    //
    // Unlike a snapshot, an event is **pushed**: the engine records it when it
    // happens, whether or not anybody is reading. The cost lands on the turn
    // that changes a session's state, so a `Vec` or a `String` there would be an
    // allocation on the engine thread. The reader's side allocates, on its own
    // thread, on purpose; this measures the engine's.
    let (mut ev_peer, ev_side) = Loopback::pair();
    let mut evented: Engine<
        Loopback,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Yield,
        Store,
        256,
        4096,
        8192,
    > = Engine::new(
        cfg(),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let watcher2 = evented.observer();
    evented.add(ev_side);
    let _ = ev_peer.send(&traffic[0]);
    evented.turn();
    let _ = ev_peer.recv(&mut sink);
    let mut drained = Vec::new();
    assert!(
        watcher2.events(&mut drained) > 0,
        "the event path must actually record something"
    );

    // A quiet turn on an observed engine: nothing changed, nothing recorded.
    let events_idle_allocs = count(|| {
        for _ in 0..10_000 {
            evented.turn();
        }
    });

    // And turns that really do produce events: a session logs on, twice over.
    //
    // **Everything that is not the event path is built and warmed outside the
    // window.** `[measured 2026-09-02]` two earlier versions of this case read
    // 30 000 and then 2 000 — three per iteration and then one per iteration —
    // and every one of those allocations was the fixture: `Loopback::pair`
    // builds its queues, and a `VecDeque` allocates on its first push. Warming
    // each pair before the count is what makes the number the engine's.
    let rounds = 2_000usize;
    let logon = traffic[0].clone();
    let mut pairs: Vec<(Loopback, Option<Loopback>)> = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let (mut near, mut far) = Loopback::pair();
        // Warm both queues so the first push inside the window does not grow one.
        let _ = near.send(&logon);
        let _ = far.recv(&mut sink);
        let _ = far.send(&logon);
        let _ = near.recv(&mut sink);
        pairs.push((near, Some(far)));
    }
    let mut busy_engine: Engine<
        Loopback,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Yield,
        Store,
        256,
        4096,
        8192,
    > = Engine::new(
        cfg(),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        rounds + 8,
    );
    let watcher3 = busy_engine.observer();
    {
        // Prove the path is the path before counting a zero from it.
        let (mut p, e) = Loopback::pair();
        busy_engine.add(e);
        let _ = p.send(&logon);
        busy_engine.turn();
        let _ = p.recv(&mut sink);
        let mut v = Vec::new();
        assert!(
            watcher3.events(&mut v) > 0,
            "the busy event path must record something"
        );
    }
    let events_busy_allocs = count(|| {
        for (peer, engine_side) in &mut pairs {
            let Some(e) = engine_side.take() else {
                continue;
            };
            busy_engine.add(e);
            let _ = peer.send(&logon);
            busy_engine.turn();
            let _ = peer.recv(&mut sink);
        }
    });
    let mut after = Vec::new();
    let recorded = watcher3.events(&mut after);
    assert!(
        recorded > 0,
        "{rounds} sessions logged on inside the window and the stream recorded \
         none — a zero above would be measuring nothing"
    );

    // --- administration ------------------------------------------------------
    //
    // The other direction: an operator's command crosses into the engine
    // thread. The engine's side is a `try_lock` and a fixed landing array on
    // the stack, and neither may allocate. The submitting side is measured too,
    // because the bench holds both handles on one thread — a real operator
    // holds `Admin` on their own.
    let (mut ad_peer, ad_side) = Loopback::pair();
    let mut administered: Engine<
        Loopback,
        fixbolt_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Yield,
        Store,
        256,
        4096,
        8192,
    > = Engine::new(
        cfg(),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Yield,
        4,
    );
    let commander = administered.admin();
    let ad_watch = administered.observer();
    administered.add(ad_side);
    let _ = ad_peer.send(&traffic[0]);
    administered.turn();
    let _ = ad_peer.recv(&mut sink);
    let _ = ad_watch.request();
    administered.turn();
    let ad_id = ad_watch
        .request()
        .and_then(|s| s.sessions().iter().find(|x| x.logged_on()).map(|x| x.id()))
        .expect("a logged-on session to administer");
    // Warm both sides once, outside the window: the first `submit` grows
    // nothing but the first event push does, and this bench is about neither.
    assert!(commander.submit(fixbolt_engine::observe::Command::SetNextIn { id: ad_id, n: 2 }));
    administered.turn();
    let mut drained = Vec::with_capacity(64);
    assert!(
        commander.events(&mut drained) > 0,
        "the administration path must actually record something"
    );
    drained.clear();

    // Nobody is administering anything: the queue is empty and a turn must not
    // pay for the capability existing.
    let admin_idle_allocs = count(|| {
        for _ in 0..10_000 {
            administered.turn();
        }
    });

    // And turns that really do apply a command.
    let admin_rounds = 2_000usize;
    let admin_busy_allocs = count(|| {
        for k in 0..admin_rounds {
            let n = u32::try_from(k % 1000).unwrap_or(1) + 1;
            let _ = commander.submit(fixbolt_engine::observe::Command::SetNextIn { id: ad_id, n });
            administered.turn();
        }
    });
    assert!(
        commander.events(&mut drained) > 0,
        "{admin_rounds} commands were applied inside the window and the stream \
         recorded none — a zero above would be measuring nothing"
    );

    println!(
        "allocations: idle {idle_allocs} send {send_allocs} recv {recv_allocs} \
         frame {frame_allocs} turn {turn_allocs} shard-turn {shard_turn_allocs} \
         busy {busy_allocs} ring {ring_allocs} interests {interests_allocs} \
         pending-idle {pending_idle_allocs} pending-busy {pending_busy_allocs} \
         pending-cycle {cycle_allocs} registry-lookup {registry_lookup_allocs} \
         observe-idle {observe_idle_allocs} observe-asked {observe_asked_allocs} \
         events-idle {events_idle_allocs} events-busy {events_busy_allocs} \
         admin-idle {admin_idle_allocs} admin-busy {admin_busy_allocs}"
    );
    assert_eq!(
        [
            idle_allocs,
            send_allocs,
            recv_allocs,
            frame_allocs,
            turn_allocs,
            shard_turn_allocs,
            busy_allocs,
            ring_allocs,
            interests_allocs,
            pending_idle_allocs,
            pending_busy_allocs,
            cycle_allocs,
            registry_lookup_allocs,
            observe_idle_allocs,
            observe_asked_allocs,
            events_idle_allocs,
            events_busy_allocs,
            admin_idle_allocs,
            admin_busy_allocs
        ],
        [0; 19],
        "non-negotiable 1: the engine allocates nothing on the byte path"
    );
}
