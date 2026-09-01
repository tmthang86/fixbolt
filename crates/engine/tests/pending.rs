//! Holding a socket that has not said who it is, and letting go of it.
//!
//! Step 3 of [pre-session-routing]. [ADR-0020] decisions 1 and 4: a `PendingSet`
//! owns the socket until the first whole message, on the acceptor thread, under
//! **two hard limits that have no defaults** — a deadline to `Logon` and a
//! ceiling on how many may wait at once.
//!
//! # Why both limits get a failing case rather than a happy one
//!
//! They are the difference between an acceptor and an open port. A connection
//! that opens and says nothing costs a slot forever; a table with no ceiling
//! costs memory forever. Neither shows up in a test that only checks a `Logon`
//! is routed, so each gets its own case and each asserts the **variant** rather
//! than `is_err()`.
//!
//! Time is a [`ManualClock`]-shaped `u64` the test moves by hand. A timeout
//! test that sleeps is a timeout test that is flaky on a loaded machine.
//!
//! # Why the EOF assertions run over a real socket
//!
//! `[measured 2026-09-01]` the first draft asserted `Io::Closed` on the peer end
//! of a [`Loopback`] after the set let go of its half. It went red, and the
//! code was right: `Loopback` hangs up only through its own `close()`, and
//! **dropping one end does nothing to the other**. A real `TcpStream` closes
//! when it drops, which is the property that matters here — a refused or
//! expired connection must actually go away.
//!
//! Teaching `Loopback` to close on drop was the other option and was not taken:
//! it is the transport the 59 acceptance definitions run over, and changing what
//! a load-bearing test double does to make a new test pass is the shape
//! `CLAUDE.md` §10 warns about. So the counts are asserted over `Loopback`,
//! where time is deterministic, and **EOF is asserted once over a real socket**,
//! where it is a fact about the thing being shipped rather than about a double.
//!
//! [pre-session-routing]: ../../../docs/plans/2026-08-31-pre-session-routing.md
//! [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{Kind, load_all};
use fixbolt_engine::clock::ManualClock;
use fixbolt_engine::dispatch::InlineDispatch;
use fixbolt_engine::journal::Store;
use fixbolt_engine::presession::{LimitError, Limits, PendingSet, Refused, is_logon};
use fixbolt_engine::transport::{Io, Loopback, TcpTransport, Transport};
use fixbolt_engine::wait::Yield;
use fixbolt_engine::{Application, Config, Engine};

const PRE: usize = 1024;
const T0: u64 = 1_000_000;

/// A real `Logon` from the acceptance corpus.
fn a_logon() -> Vec<u8> {
    load_all()
        .expect("the corpus is fetched — scripts/fetch-quickfix-assets.sh")
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m) if is_logon(&m.wire) => Some(m.wire),
            _ => None,
        })
        .expect("the corpus sends a Logon")
}

/// A real complete message that is not a `Logon`.
fn not_a_logon() -> Vec<u8> {
    load_all()
        .expect("the corpus is fetched")
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m) if !is_logon(&m.wire) && m.wire.len() > 20 => Some(m.wire),
            _ => None,
        })
        .expect("the corpus sends something else")
}

fn limits(pending: usize, logon_ms: u64) -> Limits {
    Limits::new(pending, logon_ms).expect("both above zero")
}

// --- the limits themselves ---------------------------------------------------

#[test]
fn neither_limit_has_a_default_and_zero_is_refused() {
    assert_eq!(Limits::new(0, 30_000), Err(LimitError::NoPendingAllowed));
    assert_eq!(Limits::new(8, 0), Err(LimitError::NoTimeToLogOn));
    assert!(
        Limits::new(1, 1).is_ok(),
        "one of each is a choice, not a mistake"
    );
}

#[test]
fn a_full_table_refuses_the_next_connection_immediately() {
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(2, 30_000));
    let mut ends = Vec::new();
    for _ in 0..2 {
        let (near, far) = Loopback::pair();
        assert!(matches!(set.admit(near, T0), Ok(())));
        ends.push(far);
    }
    assert_eq!(set.len(), 2);

    let (near, far) = Loopback::pair();
    let Err(refused) = set.admit(near, T0) else {
        panic!("the table is full and must say so");
    };
    // The variant, not `is_err()`: a refusal for some other reason would pass
    // that and would mean something completely different.
    assert!(matches!(refused, Refused::Full(_)));
    assert_eq!(set.len(), 2, "and it did NOT queue");

    // The socket comes back so the caller closes it on purpose rather than by
    // dropping it somewhere unnamed. That it really reaches the peer as EOF is
    // asserted over a real socket, in
    // `a_refused_connection_sees_eof_on_a_real_socket` — see the note there.
    drop(refused);
    drop(far);
}

#[test]
fn a_connection_that_never_says_anything_is_dropped_at_the_deadline() {
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(4, 30_000));
    let (near, far) = Loopback::pair();
    assert!(matches!(set.admit(near, T0), Ok(())), "room");

    let p = set.turn(T0 + 29_999);
    assert_eq!(
        p.timed_out, 0,
        "one millisecond before the deadline it still waits"
    );
    assert_eq!(set.len(), 1);

    let p = set.turn(T0 + 30_000);
    assert_eq!(p.timed_out, 1, "at the deadline it goes");
    assert_eq!(set.len(), 0, "and the slot is free again");
    drop(far);
}

#[test]
fn the_deadline_belongs_to_the_connection_not_to_the_set() {
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(4, 1_000));
    let (early, _e) = Loopback::pair();
    assert!(matches!(set.admit(early, T0), Ok(())), "room");
    let (late, _l) = Loopback::pair();
    assert!(matches!(set.admit(late, T0 + 900), Ok(())), "room");

    let p = set.turn(T0 + 1_000);
    assert_eq!(p.timed_out, 1, "only the one that has run out");
    assert_eq!(set.len(), 1);
    let p = set.turn(T0 + 1_900);
    assert_eq!(p.timed_out, 1, "and then the other, at its own deadline");
    assert_eq!(set.len(), 0);
}

// --- the lifecycle -----------------------------------------------------------

#[test]
fn a_logon_settles_and_the_bytes_come_with_it() {
    let wire = a_logon();
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(4, 30_000));
    let (near, mut far) = Loopback::pair();
    assert!(matches!(set.admit(near, T0), Ok(())), "room");
    assert_eq!(far.send(&wire), Io::Ready(wire.len()));

    let p = set.turn(T0 + 1);
    assert_eq!(p.settled, 1, "a whole Logon arrived");
    assert_eq!(p.timed_out, 0);

    let i = set.settled().expect("one is settled");
    let id = set.identity_at(i).expect("and it names both sides");
    assert_eq!(id.sender, b"TW44");
    assert_eq!(id.target, b"ISLD");

    let taken = set.take(i).expect("it comes out");
    assert_eq!(
        taken.bytes(),
        wire.as_slice(),
        "the Logon must be handed on INTACT — a stage that eats the message it \
         routed on produces an acceptor that answers nothing"
    );
    assert_eq!(set.len(), 0);
}

/// Bytes pipelined behind the `Logon` must be handed on too.
///
/// **This is the test the trap needed.** Without it, a stage that handed on
/// only the message it routed by — `rx.bytes(n)` rather than everything read —
/// passes every other case in this file, because every other case sends exactly
/// one message and the two are then the same slice. A counterparty that writes
/// its `Logon` and its first application message in one `send` would lose the
/// second silently, and the symptom is a counterparty waiting for a reply to
/// something the engine never saw.
#[test]
fn whatever_arrives_behind_the_logon_is_handed_on_with_it() {
    let logon = a_logon();
    let behind = not_a_logon();
    let mut both = logon.clone();
    both.extend_from_slice(&behind);

    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(4, 30_000));
    let (near, mut far) = Loopback::pair();
    assert!(matches!(set.admit(near, T0), Ok(())), "room");
    assert_eq!(far.send(&both), Io::Ready(both.len()));

    assert_eq!(
        set.turn(T0 + 1).settled,
        1,
        "the Logon at the front settles it"
    );
    let i = set.settled().expect("settled");
    // Routing still reads the FIRST message, not the pipeline.
    let id = set.identity_at(i).expect("named");
    assert_eq!(id.sender, b"TW44");

    let taken = set.take(i).expect("out");
    assert_eq!(
        taken.bytes(),
        both.as_slice(),
        "both messages, in order — the stage routes by the first and owes the \
         session all of them"
    );
    assert!(
        taken.bytes().len() > logon.len(),
        "and this is the assertion that fails if only the Logon is handed on"
    );
}

#[test]
fn a_logon_that_arrives_in_pieces_still_settles() {
    let wire = a_logon();
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(4, 30_000));
    let (near, mut far) = Loopback::pair();
    assert!(matches!(set.admit(near, T0), Ok(())), "room");

    for (n, byte) in wire.iter().enumerate() {
        assert_eq!(far.send(&[*byte]), Io::Ready(1));
        let p = set.turn(T0 + 1);
        if n + 1 < wire.len() {
            assert_eq!(
                p.settled,
                0,
                "{} of {} bytes is not a message",
                n + 1,
                wire.len()
            );
        } else {
            assert_eq!(p.settled, 1, "the last byte completes it");
        }
    }
    let i = set.settled().expect("settled");
    assert_eq!(set.take(i).expect("out").bytes(), wire.as_slice());
}

#[test]
fn a_first_message_that_is_not_a_logon_is_dropped_in_silence() {
    let wire = not_a_logon();
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(4, 30_000));
    let (near, mut far) = Loopback::pair();
    assert!(matches!(set.admit(near, T0), Ok(())), "room");
    assert_eq!(far.send(&wire), Io::Ready(wire.len()));

    let p = set.turn(T0 + 1);
    assert_eq!(p.not_logon, 1);
    assert_eq!(p.settled, 0, "it never becomes a connection");
    assert_eq!(set.len(), 0);

    // Nothing was said back. `1b_DuplicateIdentity.def` and
    // `AlreadyLoggedOn.def` both wait for no response at all, and this stage
    // has no session with which to say one.
    let mut buf = [0u8; 64];
    assert_eq!(far.recv(&mut buf), Io::Idle, "not one byte of reply");
    drop(far);
}

#[test]
fn a_peer_that_hangs_up_before_saying_anything_is_dropped() {
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(4, 30_000));
    let (near, mut far) = Loopback::pair();
    assert!(matches!(set.admit(near, T0), Ok(())), "room");
    far.close();

    let p = set.turn(T0 + 1);
    assert_eq!(p.gone, 1);
    assert_eq!(p.timed_out, 0, "it left; it did not run out of time");
    assert_eq!(set.len(), 0);
}

#[test]
fn a_frame_that_can_never_be_a_message_is_dropped() {
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(4, 30_000));
    let (near, mut far) = Loopback::pair();
    assert!(matches!(set.admit(near, T0), Ok(())), "room");
    // A body length that is not a number: Framer calls this Garbage, and this
    // stage has no session to hand garbage to.
    assert!(matches!(
        far.send(b"8=FIX.4.4\x019=xx\x0135=A\x01"),
        Io::Ready(_)
    ));

    let p = set.turn(T0 + 1);
    assert_eq!(p.gone, 1, "unreadable, and there is nobody to tell");
    assert_eq!(set.len(), 0);
}

#[test]
fn a_slot_freed_by_a_timeout_is_usable_again() {
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(1, 1_000));
    let (a, _a) = Loopback::pair();
    assert!(matches!(set.admit(a, T0), Ok(())), "room");
    let (b, _b) = Loopback::pair();
    assert!(matches!(set.admit(b, T0), Err(Refused::Full(_))), "full");

    assert_eq!(set.turn(T0 + 1_000).timed_out, 1);
    let (c, _c) = Loopback::pair();
    assert!(
        matches!(set.admit(c, T0 + 1_000), Ok(())),
        "the ceiling is a ceiling, not a total"
    );
}

// --- and the same properties on a real socket --------------------------------

/// Letting go of a connection must actually reach the peer.
///
/// Both halves in one test because they are one property — the set drops the
/// transport, and dropping a `TcpStream` closes it — and because each needs a
/// listener of its own.
#[test]
fn a_refused_connection_sees_eof_on_a_real_socket() {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};

    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = listener.local_addr().expect("bound");

    let mut set: PendingSet<TcpTransport, PRE> = PendingSet::new(limits(1, 30_000));

    // One in, filling the table.
    let _first_peer = TcpStream::connect(addr).expect("connect");
    let (first, _) = listener.accept().expect("accept");
    let first = TcpTransport::new(first).expect("non-blocking");
    assert!(matches!(set.admit(first, T0), Ok(())), "room for one");

    // The second is refused, and must see the socket go.
    let mut second_peer = TcpStream::connect(addr).expect("connect");
    let (second, _) = listener.accept().expect("accept");
    let second = TcpTransport::new(second).expect("non-blocking");
    let Err(refused) = set.admit(second, T0) else {
        panic!("the table is full");
    };
    assert!(matches!(refused, Refused::Full(_)));
    drop(refused);

    let mut buf = [0u8; 16];
    assert_eq!(
        second_peer.read(&mut buf).expect("read"),
        0,
        "a refused peer reads end-of-stream, not silence"
    );

    // And the one that was admitted goes the same way when it runs out of time.
    let mut first_peer = _first_peer;
    assert_eq!(set.turn(T0 + 30_000).timed_out, 1);
    assert_eq!(set.len(), 0);
    assert_eq!(
        first_peer.read(&mut buf).expect("read"),
        0,
        "an expired peer reads end-of-stream too"
    );
}

// --- the handover: the session must SEE the Logon the stage read -------------

/// A handler that answers nothing, so what comes back is the session's own.
struct Silent;

impl Application for Silent {
    fn on_message(
        &mut self,
        _: &[u8],
        _: u32,
        _: &[u8],
        _: &mut [u8],
    ) -> Option<std::ops::Range<usize>> {
        None
    }
}

type Eng = Engine<
    Loopback,
    fixbolt_session::Acceptor,
    InlineDispatch<Silent>,
    ManualClock,
    Yield,
    Store,
    256,
    4096,
    8192,
>;

fn engine() -> Eng {
    Engine::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"),
        InlineDispatch::new(Silent),
        ManualClock::at(fixbolt_conformance::script::FIXED_TIME_MILLIS),
        Yield,
        4,
    )
}

/// What the far end can read back, as text.
fn heard(far: &mut Loopback) -> String {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    while let Io::Ready(n) = far.recv(&mut buf) {
        out.extend_from_slice(&buf[..n]);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The whole point of the stage, end to end: it reads the `Logon`, hands the
/// socket on, and the session answers as if nothing had happened.
///
/// A stage that ate the message it routed by produces an acceptor that accepts
/// connections and answers nothing, and the symptom is a hung counterparty
/// rather than an error anywhere.
#[test]
fn a_session_primed_with_a_logon_answers_it() {
    let wire = a_logon();
    let mut set: PendingSet<Loopback, PRE> = PendingSet::new(limits(4, 30_000));
    let (near, mut far) = Loopback::pair();
    assert!(matches!(set.admit(near, T0), Ok(())), "room");
    assert_eq!(far.send(&wire), Io::Ready(wire.len()));
    assert_eq!(set.turn(T0 + 1).settled, 1);

    let i = set.settled().expect("settled");
    let taken = set.take(i).expect("out");
    let prefix = taken.bytes().to_vec();
    let mut eng = engine();
    eng.add_with_prefix(taken.into_transport(), &prefix)
        .expect("a Logon fits in RX");

    eng.turn();
    let out = heard(&mut far);
    assert!(
        out.contains("35=A\u{1}"),
        "the session answered the Logon the STAGE read: {out}"
    );
    assert!(!out.contains("58="), "and refused nothing: {out}");
}

#[test]
fn a_prefix_bigger_than_rx_is_refused_and_not_truncated() {
    let (near, _far) = Loopback::pair();
    let mut eng = engine();
    let huge = vec![b'x'; 4097]; // RX is 4096
    let err = eng
        .add_with_prefix(near, &huge)
        .expect_err("4097 does not fit 4096");
    assert_eq!(err.got, 4097);
    assert_eq!(err.capacity, 4096);
    assert_eq!(
        eng.connections(),
        0,
        "a refused connection must not be half-added"
    );
}
