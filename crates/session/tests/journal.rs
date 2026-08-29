//! What the outbound store does that the corpus cannot see.
//!
//! Three files ask this end to replay what it sent, and between them they pin
//! the shape of a replay almost completely. Three things they do not pin:
//!
//! * **the `52=` on a replay is a fresh one.** `52` is one of the five tags
//!   `fields.fmt` matches by shape, and the original and the fresh one are the
//!   same width, so a replay that kept the original scores 59 / 59.
//! * **a reply too long to keep is filled over**, not truncated. The longest
//!   reply in the corpus is 177 body bytes against a 512-byte slot.
//! * **the store is a ring**, and the corpus never puts more than three
//!   messages in it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use nanofix_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use nanofix_session::{Acceptor, Application, Config, Link, Session};

/// The acceptance server's own application: echo every order back.
struct EchoApp;

impl Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        nanofix_conformance::echo::echo(msg, out, seq, stamp).ok()
    }
}

fn inputs(file: &str) -> Vec<Vec<u8>> {
    scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|s| s.file == file)
        .unwrap_or_else(|| panic!("{file} is not in the corpus"))
        .steps
        .into_iter()
        .filter_map(|s| match s.kind {
            Kind::Send(m) => Some(m.wire),
            _ => None,
        })
        .collect()
}

/// Recompute `9=` and `10=` after the body has changed length.
fn reframe(wire: &[u8]) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    let after_9 = s.find("\u{1}35=").expect("35= follows the frame") + 1;
    let at_10 = s.find("\u{1}10=").map_or(s.len(), |i| i + 1);
    let body = at_10 - after_9;
    let head_end = s.find('\u{1}').expect("8= is a field") + 1;
    with_real_checksum(
        format!(
            "{}9={body}\u{1}{}10=0\u{1}",
            &s[..head_end],
            &s[after_9..at_10]
        )
        .as_bytes(),
    )
}

fn set(wire: &[u8], from: &str, to: &str) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    assert!(s.contains(from), "{from:?} is not in the message");
    s.replace(from, to).into_bytes()
}

/// `8_OnlyApplicationMessages.def`'s own order, renumbered.
fn order(seq: u32) -> Vec<u8> {
    let line = inputs("8_OnlyApplicationMessages.def")[1].clone();
    reframe(&set(
        &line,
        "\u{1}34=2\u{1}",
        &format!("\u{1}34={seq}\u{1}"),
    ))
}

/// That file's own `ResendRequest`, renumbered and re-ranged.
fn resend_request(seq: u32, from: u32, to: u32) -> Vec<u8> {
    let line = inputs("8_OnlyApplicationMessages.def")[4].clone();
    let line = set(&line, "\u{1}34=5\u{1}", &format!("\u{1}34={seq}\u{1}"));
    let line = set(&line, "\u{1}7=2\u{1}", &format!("\u{1}7={from}\u{1}"));
    reframe(&set(&line, "\u{1}16=4\u{1}", &format!("\u{1}16={to}\u{1}")))
}

/// A session logged on at [`FIXED_TIME_MILLIS`] with `108=30`. Inbound count 2,
/// outbound count 2.
fn logged_on() -> Session<Acceptor, 256> {
    let mut s = Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"));
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let link = s.received(&inputs("4b_ReceivedTestRequest.def")[0], |_| {});
    assert_eq!(link, Link::Up, "the Logon should have been accepted");
    s
}

fn feed(s: &mut Session<Acceptor, 256>, wire: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    s.received_with(wire, &mut EchoApp, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    out
}

fn msg_types(replies: &[String]) -> Vec<String> {
    replies
        .iter()
        .filter_map(|r| {
            let at = r.find("|35=")? + 4;
            let end = r[at..].find('|')? + at;
            Some(r[at..end].to_string())
        })
        .collect()
}

fn seq_nums(replies: &[String]) -> Vec<String> {
    replies
        .iter()
        .filter_map(|r| {
            let at = r.find("|34=")? + 4;
            let end = r[at..].find('|')? + at;
            Some(r[at..end].to_string())
        })
        .collect()
}

/// A replay carries a fresh `52=` and the original one as `122=`.
///
/// `[measured 2026-08-29]` writing the original `52=` back instead leaves the
/// score at 59 / 59: the two are the same width, and `52` is compared by shape.
/// What is wrong with it is not the corpus's business — a `SendingTime` is when
/// the bytes went out, and these bytes are going out now.
#[test]
fn a_replay_says_when_it_is_being_sent_and_when_it_first_was() {
    let mut s = logged_on();
    assert_eq!(msg_types(&feed(&mut s, &order(2))), ["D"], "the echo");

    // Twenty-five seconds later, which is inside `108=30` so the session says
    // nothing of its own and spends no number.
    s.tick(FIXED_TIME_MILLIS + 25_000, |_| {});
    let out = feed(&mut s, &resend_request(3, 2, 0));

    assert_eq!(
        msg_types(&out),
        ["D"],
        "one replay, not a gap fill: {out:?}"
    );
    assert_eq!(seq_nums(&out), ["2"], "at the number it was sent with");
    let r = &out[0];
    assert!(r.contains("|43=Y|"), "admitted as a repeat: {r}");
    assert!(
        r.contains("|52=20260828-12:00:25.000|"),
        "sent now, not then: {r}"
    );
    assert!(
        r.contains("|122=20260828-12:00:00.000|"),
        "and first sent then: {r}"
    );

    // A replay spends no number, so the next message this end originates is
    // still 3.
    let out = feed(&mut s, &order(4));
    assert_eq!(seq_nums(&out), ["3"], "the replay spent nothing: {out:?}");
}

/// A reply too long for a slot is not kept, and a resend fills over it.
///
/// Without the length check the copy into the slot is a `copy_from_slice` with
/// mismatched lengths — a panic on a path the counterparty controls, since the
/// reply's size follows the order's. `[measured]` the longest reply in the
/// corpus is 177 body bytes against a 512-byte slot, so nothing in it comes
/// near the bound.
#[test]
fn a_reply_too_long_to_keep_is_filled_over_rather_than_replayed() {
    // The order's `11=ClOrdID` carries the length: the echo copies it back, so
    // a 500-byte ID makes a reply no slot can hold.
    let long = reframe(&set(
        &order(2),
        "11=ID\u{1}",
        &format!("11={}\u{1}", "X".repeat(500)),
    ));
    let mut s = logged_on();
    let out = feed(&mut s, &long);
    assert_eq!(msg_types(&out), ["D"], "the echo still goes out: {out:?}");
    assert!(out[0].len() > 512, "and it is too big to keep");

    let out = feed(&mut s, &resend_request(3, 2, 0));
    assert_eq!(
        msg_types(&out),
        ["4"],
        "a gap fill, because it was not kept: {out:?}"
    );
    assert!(out[0].contains("|36=3|"), "covering just it: {}", out[0]);
}

/// The store is a ring: the ninth message kept pushes the first out.
///
/// `[measured 2026-08-29]` the corpus never keeps more than three, so its size
/// is invisible to the score. What must not happen is a session that answers a
/// resend with silence for the numbers it no longer holds — the counterparty is
/// waiting on those numbers and will wait forever.
#[test]
fn what_no_longer_fits_in_the_ring_is_filled_over_not_skipped() {
    let mut s = logged_on();
    for seq in 2..=10 {
        assert_eq!(msg_types(&feed(&mut s, &order(seq))), ["D"], "echo {seq}");
    }

    // Nine kept in eight slots: `34=2` is the one that went.
    let out = feed(&mut s, &resend_request(11, 2, 0));
    assert_eq!(
        msg_types(&out),
        ["4", "D", "D", "D", "D", "D", "D", "D", "D"],
        "a fill for the one that went, then the eight still here: {out:?}"
    );
    assert!(out[0].contains("|34=2|") && out[0].contains("|36=3|"));
    assert_eq!(
        seq_nums(&out)[1..],
        ["3", "4", "5", "6", "7", "8", "9", "10"],
        "every number the counterparty asked for is answered: {out:?}"
    );
}
