//! Step 1: bytes in and out, and the one answer a non-blocking socket gives
//! that a blocking one does not.
//!
//! **"Nothing has arrived yet" is not an error and it is not end-of-stream.**
//! Conflating the three is the bug this file exists to prevent: a session
//! dropped because the counterparty was merely quiet, or a loop that spins on a
//! closed socket forever because `WouldBlock` and `EOF` both came back as
//! `Ok(0)`. The codec settled the same question for `Parsed::Incomplete`
//! (DESIGN D2); this is that answer, at the other end of the stack.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::{TcpListener, TcpStream};

use fixbolt_engine::transport::{Io, Loopback, TcpTransport, Transport};
use fixbolt_engine::wait::{Park, Spin, Waiting};

#[test]
fn a_quiet_socket_is_idle_a_closed_one_is_closed_and_neither_is_an_error() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");
    let client = TcpStream::connect(addr).expect("connect");
    let (accepted, _) = listener.accept().expect("accept");

    let mut server = TcpTransport::new(accepted).expect("non-blocking");
    let mut client = TcpTransport::new(client).expect("non-blocking");

    // Nothing sent yet.
    let mut buf = [0u8; 64];
    assert_eq!(server.recv(&mut buf), Io::Idle, "quiet is not an error");

    assert_eq!(client.send(b"8=FIX.4.4\x01"), Io::Ready(10));
    let got = spin_until_ready(&mut server, &mut buf);
    assert_eq!(&buf[..got], b"8=FIX.4.4\x01");

    // And quiet again straight afterwards.
    assert_eq!(server.recv(&mut buf), Io::Idle, "still not an error");

    drop(client);
    let mut seen = Io::Idle;
    for _ in 0..10_000 {
        seen = server.recv(&mut buf);
        if seen != Io::Idle {
            break;
        }
    }
    assert_eq!(seen, Io::Closed, "a hang-up is not idle and not an error");
}

fn spin_until_ready<T: Transport>(t: &mut T, buf: &mut [u8]) -> usize {
    for _ in 0..10_000 {
        if let Io::Ready(n) = t.recv(buf) {
            return n;
        }
    }
    panic!("nothing arrived");
}

/// The in-memory transport the corpus runs over.
///
/// A test that binds a real port fails on a busy CI machine for a reason that
/// has nothing to do with FIX. `Loopback` has the same three answers and no
/// kernel.
#[test]
fn the_in_memory_transport_answers_the_same_three_ways() {
    let (mut a, mut b) = Loopback::pair();
    let mut buf = [0u8; 64];

    assert_eq!(b.recv(&mut buf), Io::Idle);
    assert_eq!(a.send(b"35=A\x01"), Io::Ready(5));
    assert_eq!(b.recv(&mut buf), Io::Ready(5));
    assert_eq!(&buf[..5], b"35=A\x01");
    assert_eq!(b.recv(&mut buf), Io::Idle);

    a.close();
    assert_eq!(b.recv(&mut buf), Io::Closed);
}

/// A short read is a short read, not a lost message.
///
/// TCP delivers bytes. A 64-byte message arriving into a 16-byte buffer must
/// leave the other 48 where they are, and the framer above must see them next
/// time round.
#[test]
fn a_buffer_smaller_than_the_message_loses_nothing() {
    let (mut a, mut b) = Loopback::pair();
    assert_eq!(a.send(&[b'x'; 64]), Io::Ready(64));

    let mut buf = [0u8; 16];
    let mut total = 0;
    while let Io::Ready(n) = b.recv(&mut buf) {
        total += n;
    }
    assert_eq!(total, 64, "every byte came out, sixteen at a time");
}

/// `Waiting` is a trait so a test loop does not have to burn a core.
///
/// `Spin` is the default and the reason D8 exists; `Park` is what every test in
/// this repository uses, because a CI machine running four spinning loops is a
/// CI machine that times out.
#[test]
fn both_waiting_strategies_return_and_neither_is_the_other() {
    let mut spin = Spin;
    let mut park = Park;
    spin.idle();
    park.idle();
    // A `const` block, because clippy is right that these are known at compile
    // time — which is the point: a caller can branch on them without paying
    // anything, and a test can tell the two strategies apart without timing.
    const { assert!(!Spin::SLEEPS, "the default never leaves user space") };
    const { assert!(Park::SLEEPS, "and the test strategy says it does") };
}

/// A hang-up does not swallow bytes already sent.
///
/// TCP does not, and a fake that does would hide a whole class of bug: a
/// counterparty that sends a Logout and closes immediately is ordinary, and an
/// engine that loses the Logout because the socket went away has lost a
/// protocol message to a race.
#[test]
fn closing_leaves_what_was_already_sent_where_it_is() {
    let (mut a, mut b) = Loopback::pair();
    assert_eq!(a.send(b"35=5\x01"), Io::Ready(5));
    a.close();

    let mut buf = [0u8; 64];
    assert_eq!(b.recv(&mut buf), Io::Ready(5), "the Logout is still there");
    assert_eq!(&buf[..5], b"35=5\x01");
    assert_eq!(b.recv(&mut buf), Io::Closed, "and only then the hang-up");
}
