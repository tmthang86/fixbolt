//! One test, its own binary, because it changes a **process-global** signal
//! disposition and must not do that to the rest of the suite.
#![cfg(all(feature = "standard", unix))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(unsafe_code)]

use fixbolt_engine::waker::Waker;

#[test]
fn waking_after_the_engine_is_gone_does_not_kill_the_process() {
    // Rust's runtime sets SIGPIPE to SIG_IGN before `main`, which is exactly
    // why this bug is invisible from an ordinary test: the write would return
    // EPIPE and the ignored return value would hide it. A library cannot rely
    // on its host doing that — a cdylib in a C program, or a `main` that resets
    // the disposition, gets the default action, which is to terminate.
    // SAFETY: sets the default disposition for SIGPIPE in this test binary and
    // nothing else. It is its own process.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let (waker, handle) = Waker::new().expect("a pipe");
    // The engine shuts down; the application thread has not noticed yet.
    drop(waker);

    for _ in 0..10 {
        handle.wake();
    }
    // Reaching here at all is the assertion.
}

/// And the socket path is **not** exposed, which was worth measuring rather
/// than assuming.
///
/// The reasoning that would have let this go unchecked: *"`std` guards its own
/// socket writes, so only the raw `libc::write` added here was ever at risk."*
/// That is a claim about somebody else's implementation. `TcpTransport::send`
/// writes to a peer that may have hung up, in the same process, under the same
/// default disposition — so it is asked directly, in the one binary where the
/// answer is not masked by Rust's `SIG_IGN`.
#[test]
fn writing_to_a_hung_up_socket_does_not_kill_the_process_either() {
    use fixbolt_engine::transport::{TcpTransport, Transport};
    use std::net::{TcpListener, TcpStream};

    // SAFETY: same as above — its own binary, and this is the disposition the
    // test exists to run under.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");
    let client = TcpStream::connect(addr).expect("connect");
    let (server, _) = listener.accept().expect("accept");
    let mut server = TcpTransport::new(server).expect("non-blocking");
    drop(client);

    // Enough to get past whatever the kernel buffers before it notices the
    // hang-up: the first write after a close often succeeds and the RST
    // arrives afterwards.
    for _ in 0..100 {
        let _ = server.send(b"8=FIX.4.4\x019=5\x0135=0\x0110=000\x01");
    }
    // Reaching here at all is the assertion.
}
