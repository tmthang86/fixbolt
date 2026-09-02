//! What an **initiator** can be told to say, and what it refuses to say.
//!
//! Step 1 of [the-initiator-and-its-second-opinion]. The corpus cannot reach
//! any of this: 46 of the 50 mirrorable files need a message that nothing on
//! the wire asks for and no clock produces, and a pure state machine cannot
//! invent one. So the operator's intent needs an API, and this file is its
//! gate.
//!
//! # Why each function has its own test rather than one shared one
//!
//! The three differ in exactly the way that matters — what the caller owns —
//! and in no other way. One test proving *"a message came out"* would pass an
//! implementation where `112=` was taken from the counterparty's last
//! TestRequest instead of from the caller, which is a real bug with a real
//! symptom: a heartbeat answering the wrong question.
//!
//! # What is deliberately **not** here
//!
//! A back door that sends caller-supplied bytes. Every function here hands the
//! session an *intent*; the session builds the message from its own `Template`
//! and keeps `8`, `9`, `34`, `49`, `52`, `56` and `10` for itself. That
//! boundary is what stops `tests/mirror.rs` from measuring the file it is
//! reading.
//!
//! [the-initiator-and-its-second-opinion]: ../../../docs/plans/2026-09-02-the-initiator-and-its-second-opinion.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{FIXED_TIME_IN, FIXED_TIME_MILLIS, with_real_checksum};
use fixbolt_session::{Config, Initiator, Link, Session};

fn cfg() -> Config {
    Config::initiator(b"FIX.4.4", b"TW44", b"ISLD").with_heart_bt_int(30)
}

/// The acceptor's answer to our Logon, stamped at the instant the session is
/// ticked to so it is inside `max_skew_ms`.
fn peer_logon(seq: u32) -> Vec<u8> {
    body(&format!(
        "35=A\u{1}34={seq}\u{1}49=ISLD\u{1}52={FIXED_TIME_IN}\u{1}56=TW44\u{1}98=0\u{1}108=30\u{1}"
    ))
}

fn body(body: &str) -> Vec<u8> {
    let head = format!("8=FIX.4.4\u{1}9={}\u{1}", body.len());
    let mut out = head.into_bytes();
    out.extend_from_slice(body.as_bytes());
    out.extend_from_slice(b"10=000\x01");
    with_real_checksum(&out)
}

fn readable(b: &[u8]) -> String {
    String::from_utf8_lossy(b).replace('\u{1}', "|")
}

/// An initiator that has logged on, and everything it has said so far.
///
/// Three inputs and no socket: `connect` records whose turn it is, `tick` is
/// what makes it speak — time enters this layer nowhere else — and the peer's
/// Logon is what completes the handshake.
fn logged_on() -> (Session<Initiator, 256>, Vec<String>) {
    let mut s: Session<Initiator, 256> = Session::new(cfg());
    let mut out: Vec<String> = Vec::new();
    assert_eq!(s.connect(|b| out.push(readable(b))), Link::Up);
    assert_eq!(
        s.tick(FIXED_TIME_MILLIS, |b| out.push(readable(b))),
        Link::Up
    );
    assert_eq!(out.len(), 1, "connect + tick is exactly one Logon: {out:?}");
    assert!(out[0].contains("|35=A|"), "and it is a Logon: {out:?}");
    assert_eq!(
        s.received(&peer_logon(1), |b| out.push(readable(b))),
        Link::Up,
        "the premise: the peer's Logon is accepted"
    );
    assert!(s.is_logged_on(), "the premise");
    (s, out)
}

/// **An initiator does not answer a Logon with a Logon.**
///
/// `[measured 2026-09-02]` it did, and no gate in this repository could see it:
/// the acceptor corpus scores 59 / 59 because for an *acceptor* answering is
/// correct, and the mirrored corpus was 0 / 50 and never read past the line
/// that carries the first Logon. What found it was `scripts/interop.sh` —
/// `libquickfix` took the second Logon, dropped the connection without a word,
/// and five of this project's seven interop steps failed at once with nothing
/// on the wire to explain why.
///
/// The reply is the **acceptor's** half of the handshake. The side that speaks
/// first has already had its say.
///
/// See `docs/reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md`.
#[test]
fn an_initiator_does_not_answer_a_logon_with_a_logon() {
    let mut s: Session<Initiator, 256> = Session::new(cfg());
    let mut out: Vec<String> = Vec::new();
    s.connect(|b| out.push(readable(b)));
    s.tick(FIXED_TIME_MILLIS, |b| out.push(readable(b)));
    assert_eq!(out.len(), 1, "the premise: our own Logon, and only it");
    let after_ours = s.next_out();

    assert_eq!(
        s.received(&peer_logon(1), |b| out.push(readable(b))),
        Link::Up
    );

    assert_eq!(
        out.len(),
        1,
        "the acceptor's Logon completes the handshake; answering it starts a \
         second one: {out:?}"
    );
    assert_eq!(
        s.next_out(),
        after_ours,
        "and no sequence number was spent on a message that should not exist"
    );
    assert!(s.is_logged_on(), "the handshake still completed");
}

/// One `send_heartbeat` is one `35=0`, at this session's next number.
#[test]
fn a_heartbeat_can_be_originated_and_is_one_message() {
    let (mut s, mut out) = logged_on();
    let before = out.len();
    let seq = s.next_out();

    assert!(s.send_heartbeat(|b| out.push(readable(b))));

    assert_eq!(out.len(), before + 1, "exactly one message: {out:?}");
    let m = &out[before];
    assert!(m.contains("|35=0|"), "{m}");
    assert!(m.contains(&format!("|34={seq}|")), "{m}");
    assert!(
        !m.contains("|112="),
        "an unprompted heartbeat answers nothing, so it carries no TestReqID: {m}"
    );
    assert_eq!(s.next_out(), seq + 1, "and the number was spent");
}

/// `send_test_request` writes **the caller's** `112=`.
///
/// The value is one that appears nowhere else in this test — not in the peer's
/// Logon, not in the session's own `OWN_TEST_REQ_ID`. An implementation that
/// reached for either would fail here and pass a test that only asked whether
/// `112=` was present.
#[test]
fn a_test_request_carries_the_id_the_caller_chose() {
    let (mut s, mut out) = logged_on();
    let before = out.len();
    let seq = s.next_out();

    assert!(s.send_test_request(b"OPERATOR-7", |b| out.push(readable(b))));

    assert_eq!(out.len(), before + 1, "exactly one message: {out:?}");
    let m = &out[before];
    assert!(m.contains("|35=1|"), "{m}");
    assert!(m.contains("|112=OPERATOR-7|"), "{m}");
    assert!(
        !m.contains("|112=TEST|"),
        "not the session's own constant: {m}"
    );
    assert!(m.contains(&format!("|34={seq}|")), "{m}");
    assert_eq!(s.next_out(), seq + 1);
}

/// `send_resend_request` writes the caller's range, and `16=0` is legal.
///
/// `16=0` means *"and everything after"*. It is the form QuickFIX itself sends
/// and the one a recovering session needs, so it must not be refused as a
/// degenerate range.
#[test]
fn a_resend_request_carries_the_range_the_caller_chose() {
    let (mut s, mut out) = logged_on();
    let before = out.len();
    let seq = s.next_out();

    assert!(s.send_resend_request(4, 9, |b| out.push(readable(b))));
    assert!(s.send_resend_request(11, 0, |b| out.push(readable(b))));

    assert_eq!(out.len(), before + 2, "two messages: {out:?}");
    let first = &out[before];
    assert!(first.contains("|35=2|"), "{first}");
    assert!(first.contains("|7=4|"), "{first}");
    assert!(first.contains("|16=9|"), "{first}");
    assert!(first.contains(&format!("|34={seq}|")), "{first}");

    let open_ended = &out[before + 1];
    assert!(open_ended.contains("|7=11|"), "{open_ended}");
    assert!(
        open_ended.contains("|16=0|"),
        "0 is 'everything after', not an empty range: {open_ended}"
    );
    assert_eq!(s.next_out(), seq + 2);
}

/// **Silence before the Logon is agreed.** All three, because a session that
/// spoke early would be spotted by whichever of the three a test happened to
/// use, and there is no reason for the three to agree by accident.
#[test]
fn nothing_can_be_originated_before_the_session_is_logged_on() {
    let mut s: Session<Initiator, 256> = Session::new(cfg());
    let mut out: Vec<String> = Vec::new();
    assert_eq!(s.connect(|b| out.push(readable(b))), Link::Up);
    assert!(!s.is_logged_on(), "the premise");
    out.clear();

    // Collected, not asserted one at a time. Three `assert!`s in a row stop at
    // the first, so a reversal that removed the guard from all three would show
    // exactly the same failure as one that removed it from one — and
    // `[measured 2026-09-02]` that is what the first run of this reversal did.
    // The repository has the shape already: a sentinel read by two decoders,
    // reversed in only one, read `5 passed; 1 failed` and looked discriminating.
    let refused = [
        s.send_heartbeat(|b| out.push(readable(b))),
        s.send_test_request(b"OPERATOR-7", |b| out.push(readable(b))),
        s.send_resend_request(4, 9, |b| out.push(readable(b))),
    ];
    assert_eq!(
        refused, [false; 3],
        "heartbeat, test request, resend request — in that order"
    );

    assert!(
        out.is_empty(),
        "refused means nothing went out, not 'went out and returned false': {out:?}"
    );
    assert_eq!(
        s.next_out(),
        1,
        "and a refused message spends no sequence number"
    );
}

/// The three do not disturb the heartbeat clock's reading of *"we spoke"*.
///
/// They are messages this end sent, so they must count as this end speaking —
/// otherwise an engine that originates steadily would still emit heartbeats on
/// top, and the counterparty would see two messages where one was due.
#[test]
fn originating_counts_as_this_end_having_spoken() {
    let (mut s, mut out) = logged_on();
    // A heartbeat is due 30 s after we last spoke. Originate at +29 s, then
    // tick to +31 s: if originating did not count, the tick would emit one.
    let at = FIXED_TIME_MILLIS + 29_000;
    assert_eq!(s.tick(at, |b| out.push(readable(b))), Link::Up);
    let before = out.len();
    assert!(s.send_test_request(b"OPERATOR-7", |b| out.push(readable(b))));
    assert_eq!(out.len(), before + 1);

    assert_eq!(
        s.tick(at + 2_000, |b| out.push(readable(b))),
        Link::Up,
        "still up"
    );
    assert_eq!(
        out.len(),
        before + 1,
        "nothing further: the TestRequest was this end speaking, 2 s ago: {out:?}"
    );
}
