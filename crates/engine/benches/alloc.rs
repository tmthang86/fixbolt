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

use nanofix_conformance::script::{FIXED_TIME_MILLIS, with_real_checksum};
use nanofix_engine::clock::ManualClock;
use nanofix_engine::dispatch::{Dispatch, InlineDispatch, RingApp, RingDispatch};
use nanofix_engine::frame::{Cut, Framer};
use nanofix_engine::ring;
use nanofix_engine::transport::{Io, Loopback, TcpTransport, Transport};
use nanofix_engine::wait::Park;
use nanofix_engine::{Application, Config, Engine};

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
        nanofix_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Park,
        256,
        4096,
        8192,
    > = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Park,
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
        nanofix_session::Acceptor,
        InlineDispatch<Silent>,
        ManualClock,
        Park,
        256,
        4096,
        8192,
    > = Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(Silent),
        ManualClock::at(FIXED_TIME_MILLIS),
        Park,
        4,
    );
    let _ = engine2.add(engine_side2);
    // **One empty turn before the first byte**, and it is not decoration.
    // `Session::received_with` takes no clock: it judges `SendingTime` against
    // the last instant a `tick` gave it, which for a session that has never
    // ticked is zero. The corpus's `52=` is then 2026 years of skew and the
    // Logon is refused. `[measured 2026-08-30]` the engine above only accepted
    // the same Logon because the `turn` case had already ticked it 10 000
    // times. A deployment turns continuously and never meets this; a bench
    // that sends on turn one does.
    engine2.turn();
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

    println!(
        "allocations: idle {idle_allocs} send {send_allocs} recv {recv_allocs} \
         frame {frame_allocs} turn {turn_allocs} busy {busy_allocs} ring {ring_allocs}"
    );
    assert_eq!(
        [
            idle_allocs,
            send_allocs,
            recv_allocs,
            frame_allocs,
            turn_allocs,
            busy_allocs,
            ring_allocs
        ],
        [0; 7],
        "non-negotiable 1: the engine allocates nothing on the byte path"
    );
}
