//! Setting a session's sequence numbers by hand.
//!
//! Step 2 of [three-in-the-morning]. The pure half: these three functions do
//! not know about threads, engines or sockets, and every test here drives a
//! `Session` directly.
//!
//! # Why each of the three has its own test rather than one shared one
//!
//! They differ in exactly the way that matters and in no other way: two are
//! silent and one speaks. A single test proving *"the number changed"* would
//! pass for an implementation where all three are silent, or all three speak —
//! and each of those is a different production incident.
//!
//! [three-in-the-morning]: ../../../docs/plans/2026-09-02-sequence-numbers-at-three-in-the-morning.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_session::{Acceptor, Config, Link, Session};

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// A real Logon from the corpus with its deliberately wrong field corrected.
fn good_logon() -> Vec<u8> {
    let wire = scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|s| s.file == "1c_InvalidTargetCompID.def")
        .expect("the corpus has it")
        .steps
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m) => Some(m.wire),
            _ => None,
        })
        .expect("it has an I line");
    let s = String::from_utf8(wire).expect("ascii");
    with_real_checksum(s.replace("56=DLSI", "56=ISLD").as_bytes())
}

/// A logged-on session and everything it has said so far.
fn logged_on() -> (Session<Acceptor, 256>, Vec<String>) {
    let mut s: Session<Acceptor, 256> = Session::new(cfg());
    let mut out = Vec::new();
    s.connect(|b| out.push(String::from_utf8_lossy(b).replace('\u{1}', "|")));
    s.tick(FIXED_TIME_MILLIS, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"))
    });
    assert_eq!(
        s.received(&good_logon(), |b| out
            .push(String::from_utf8_lossy(b).replace('\u{1}', "|"))),
        Link::Up,
        "the premise: a good Logon is accepted"
    );
    (s, out)
}

/// `set_next_out` moves the number and says nothing.
#[test]
fn setting_the_outbound_number_changes_it_and_sends_nothing() {
    let (mut s, out) = logged_on();
    let before = out.len();
    assert_ne!(s.next_out(), 4812, "the premise");

    assert!(s.set_next_out(4812));

    assert_eq!(s.next_out(), 4812);
    assert_eq!(
        out.len(),
        before,
        "it is local: the counterparty is told nothing, and that is what makes \
         it a lie until they are"
    );
}

/// `set_next_in` moves the number and says nothing, and **does not touch the
/// other one**. A single field written by both would pass a test that only
/// checked the one it set.
#[test]
fn setting_the_inbound_number_changes_it_and_leaves_the_other_alone() {
    let (mut s, out) = logged_on();
    let before = out.len();
    let untouched = s.next_out();

    assert!(s.set_next_in(4812));

    assert_eq!(s.next_in(), 4812);
    assert_eq!(
        s.next_out(),
        untouched,
        "the outbound number is not its business"
    );
    assert_eq!(out.len(), before, "and nothing goes on the wire");
}

/// Zero is refused by all three, and refusal **changes nothing** — an operator
/// who fat-fingers a field must not end up with a session numbered from 0,
/// which is not a sequence number FIX has.
#[test]
fn zero_is_refused_and_leaves_the_session_as_it_was() {
    let (mut s, out) = logged_on();
    let (was_out, was_in, said) = (s.next_out(), s.next_in(), out.len());

    assert!(!s.set_next_out(0));
    assert!(!s.set_next_in(0));
    let mut extra = 0;
    assert!(!s.send_sequence_reset(0, |_| extra += 1));

    assert_eq!(s.next_out(), was_out);
    assert_eq!(s.next_in(), was_in);
    assert_eq!(out.len(), said);
    assert_eq!(extra, 0, "a refused reset sends nothing");
}

/// **The honest form.** `send_sequence_reset` puts `35=4` with `123=N` and
/// `36=n` on the wire, at the *current* number, and only then becomes `n`.
#[test]
fn a_sequence_reset_tells_the_counterparty_and_then_becomes_the_number() {
    let (mut s, _) = logged_on();
    let at = s.next_out();
    let mut sent = Vec::new();

    assert!(s.send_sequence_reset(4812, |b| {
        sent.push(String::from_utf8_lossy(b).replace('\u{1}', "|"))
    }));

    assert_eq!(sent.len(), 1, "exactly one message: {sent:?}");
    let m = &sent[0];
    assert!(m.contains("|35=4|"), "a SequenceReset: {m}");
    assert!(m.contains("|123=N|"), "a reset, not a gap fill: {m}");
    assert!(m.contains("|36=4812|"), "and it names the new number: {m}");
    assert!(
        m.contains(&format!("|34={at}|")),
        "the reset itself carries the number it is replacing, {at}: {m}"
    );
    assert_eq!(
        s.next_out(),
        4812,
        "and afterwards the promise in 36= is kept"
    );
}

/// A reset **downwards** is legal, is a last resort, and nothing here stops it.
/// Recorded as a test so the permission is deliberate rather than an oversight.
#[test]
fn a_reset_may_move_the_number_down() {
    let (mut s, _) = logged_on();
    assert!(s.set_next_out(500), "get somewhere to come down from");
    let mut sent = Vec::new();
    assert!(s.send_sequence_reset(7, |b| {
        sent.push(String::from_utf8_lossy(b).replace('\u{1}', "|"))
    }));
    assert!(sent[0].contains("|36=7|"), "{sent:?}");
    assert_eq!(s.next_out(), 7);
}

/// A session that **cannot build the message** must not move the number
/// anyway — that would leave the two ends disagreeing in the one case the
/// operator was trying to fix.
///
/// `[measured 2026-09-02]` the first version of this test used a session that
/// had never connected, on the assumption that the output buffer arrives with
/// the connection. It does not: `Outbound` is built in `Session::new`, so that
/// session sent the reset happily and the test failed for a reason that had
/// nothing to do with what it was guarding. The reachable failure is a `Config`
/// whose fields do not fit the templates.
#[test]
fn a_reset_that_cannot_be_sent_does_not_move_the_number() {
    let too_long = vec![b'X'; 400];
    let mut s: Session<Acceptor, 256> =
        Session::new(Config::acceptor(b"FIX.4.4", &too_long, b"TW44"));
    let was = s.next_out();
    let mut sent = 0;
    assert!(!s.send_sequence_reset(4812, |_| sent += 1));
    assert_eq!(sent, 0);
    assert_eq!(s.next_out(), was, "a failed send leaves the number alone");
}
