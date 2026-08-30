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

use nanofix_engine::transport::{Io, TcpTransport, Transport};

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

    let msg = b"8=FIX.4.4\x019=63\x0135=A\x0134=1\x0149=ISLD\x0156=TW44\x0110=0\x01";
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

    println!("allocations: idle {idle_allocs} send {send_allocs} recv {recv_allocs}");
    assert_eq!(
        [idle_allocs, send_allocs, recv_allocs],
        [0; 3],
        "non-negotiable 1: the engine allocates nothing on the byte path"
    );
}
