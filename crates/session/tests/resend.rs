//! The gap rules the 59 definitions cannot see.
//!
//! Every file in the corpus that opens a gap ends before opening a second one,
//! and the deepest any of them holds is two messages. So three things the
//! session does are invisible to the score: that a filled gap is *closed*, that
//! held messages are replayed in sequence order, and what happens when more
//! arrive than there is room for.
//!
//! A session that never closes a gap scores 42 / 59 and then strands its next
//! one in silence — it has already asked, so it never asks again. That is the
//! kind of failure a score cannot show.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_session::{Acceptor, Config, Link, Session};

fn acceptor() -> Session<Acceptor, 256> {
    Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
}

/// Every `I` line of a definition file, in order. Real corpus lines rather than
/// invented packets — `CLAUDE.md` §7.
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

/// Recompute `9=` and `10=` after the body has changed length. The `10=0`
/// placeholder is load-bearing: a message with no trailer parses as
/// `Incomplete`, which this layer reads as "wait for more".
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

/// `4b_ReceivedTestRequest.def`'s own TestRequest, renumbered and renamed.
///
/// A TestRequest because its answer carries the `112=` it was sent, which is
/// how a reply can be traced back to the message that caused it — the only way
/// to see what order held messages were replayed in.
fn test_request(seq: u32, id: &str) -> Vec<u8> {
    let line = inputs("4b_ReceivedTestRequest.def")[1].clone();
    let renumbered = set(&line, "\u{1}34=2\u{1}", &format!("\u{1}34={seq}\u{1}"));
    reframe(&set(
        &renumbered,
        "112=HELLO\u{1}",
        &format!("112={id}\u{1}"),
    ))
}

/// Like [`replace`], but the new value may equal the old — `test_request(2, …)`
/// renumbers 34=2 to 34=2, and that is not a mistake.
fn set(wire: &[u8], from: &str, to: &str) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    assert!(s.contains(from), "{from:?} is not in the message");
    s.replace(from, to).into_bytes()
}

/// A session logged on at [`FIXED_TIME_MILLIS`] with `108=30`, its Logon reply
/// discarded. Inbound count 2, outbound count 2.
fn logged_on() -> Session<Acceptor, 256> {
    let mut s = acceptor();
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let link = s.received(&inputs("4b_ReceivedTestRequest.def")[0], |_| {});
    assert_eq!(link, Link::Up, "the Logon should have been accepted");
    s
}

/// Feed one message and report every reply, rendered.
fn feed(s: &mut Session<Acceptor, 256>, wire: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    s.received(wire, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    out
}

/// The `112=` of each reply, in the order they came out.
fn ids(replies: &[String]) -> Vec<String> {
    replies
        .iter()
        .filter_map(|r| {
            let at = r.find("|112=")? + 5;
            let end = r[at..].find('|')? + at;
            Some(r[at..end].to_string())
        })
        .collect()
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

/// A gap that has been filled is closed, and the next one is asked for.
///
/// The corpus never opens a second gap, so a session that asks once and never
/// again scores exactly the same — and then sits silent the first time a real
/// counterparty drops a message twice in one session.
#[test]
fn a_filled_gap_is_closed_and_the_next_one_is_asked_for() {
    let mut s = logged_on();

    // 34=4 with 2 expected: a gap of 2 and 3.
    let out = feed(&mut s, &test_request(4, "AHEAD"));
    assert_eq!(msg_types(&out), ["2"], "a resend request: {out:?}");
    assert!(
        out[0].contains("|7=2|"),
        "from the expected number: {}",
        out[0]
    );

    // Half-fill it. The gap runs to 3, so after 2 arrives it is still open —
    // and a message running ahead now must **not** produce a second request.
    assert_eq!(ids(&feed(&mut s, &test_request(2, "TWO"))), ["TWO"]);
    let out = feed(&mut s, &test_request(9, "AHEAD3"));
    assert!(
        out.is_empty(),
        "the gap is still open, so this is already asked for: {out:?}"
    );

    // 3 is the last number the gap was waiting on, so it closes here — and the
    // two held messages follow, in order.
    let out = feed(&mut s, &test_request(3, "THREE"));
    assert_eq!(
        ids(&out),
        ["THREE", "AHEAD"],
        "the gap closed and the held message came with it: {out:?}"
    );

    // A second gap, on a session that has already asked once. `AHEAD3` is still
    // held at 9, and the count is at 5.
    let out = feed(&mut s, &test_request(7, "AHEAD2"));
    assert_eq!(
        msg_types(&out),
        ["2"],
        "the first gap is filled, so this one must be asked for too: {out:?}"
    );
    assert!(
        out[0].contains("|7=5|"),
        "and from 5, not from 2: {}",
        out[0]
    );
}

/// A message too long to hold is dropped, not truncated — and not a panic.
///
/// The queue copies into a fixed slot. Without the length check that copy is a
/// `copy_from_slice` with mismatched lengths, which is a panic on a path a
/// hostile counterparty controls completely. Nothing in the corpus comes near
/// the bound: `[measured]` its longest message is 101 bytes.
#[test]
fn a_message_too_long_to_hold_is_dropped_rather_than_held() {
    // Just inside the slot, and just outside it. Both are well-framed messages
    // the session would answer if they arrived in order.
    let fits = test_request(3, &"X".repeat(400));
    let does_not = test_request(3, &"X".repeat(500));
    assert!(
        fits.len() < 512 && does_not.len() > 512,
        "{} {}",
        fits.len(),
        does_not.len()
    );

    for (wire, held) in [(&fits, true), (&does_not, false)] {
        let mut s = logged_on();
        assert_eq!(msg_types(&feed(&mut s, wire)), ["2"], "it runs ahead");
        // Closing the gap replays whatever was held.
        feed(&mut s, &test_request(2, "TWO"));
        // If 3 was held it has been judged and the count is at 4; if it was
        // dropped the count is still at 3 and 4 runs ahead all over again.
        let out = feed(&mut s, &test_request(4, "FOUR"));
        if held {
            assert_eq!(ids(&out), ["FOUR"], "400 bytes fits and was replayed");
        } else {
            assert_eq!(
                msg_types(&out),
                ["2"],
                "500 bytes did not fit, so the count never passed 3: {out:?}"
            );
        }
    }
}

/// Held messages are replayed in sequence order, not arrival order.
///
/// `RejectResentMessage.def` holds exactly one message, so the corpus proves
/// only that a held message comes before a fresh one. Which of several held
/// messages goes first is not in it.
#[test]
fn held_messages_are_replayed_in_sequence_order() {
    let mut s = logged_on();

    // Arriving 5, 4, 3 — backwards. Only the first asks for the gap.
    let out = feed(&mut s, &test_request(5, "FIVE"));
    assert_eq!(msg_types(&out), ["2"], "{out:?}");
    for (seq, id) in [(4, "FOUR"), (3, "THREE")] {
        let out = feed(&mut s, &test_request(seq, id));
        assert!(
            out.is_empty(),
            "held, not answered, and not asked again: {out:?}"
        );
    }

    let out = feed(&mut s, &test_request(2, "TWO"));
    assert_eq!(
        ids(&out),
        ["TWO", "THREE", "FOUR", "FIVE"],
        "the count decides the order, not the wire: {out:?}"
    );
}

/// More messages running ahead than there is room for: the extra is dropped,
/// and the session recovers by asking again rather than by stalling.
///
/// `[measured 2026-08-29]` the corpus never holds more than two, so the bound
/// itself is invisible to the score. What must not happen is a session that
/// loses its place: the dropped message was never acknowledged and the count
/// never moved, so the counterparty's next message running ahead brings it back.
#[test]
fn a_message_with_no_room_to_be_held_is_dropped_and_asked_for_again() {
    let mut s = logged_on();

    // Four slots, five messages ahead of the count.
    let out = feed(&mut s, &test_request(3, "THREE"));
    assert_eq!(msg_types(&out), ["2"], "{out:?}");
    for (seq, id) in [(4, "FOUR"), (5, "FIVE"), (6, "SIX"), (7, "SEVEN")] {
        assert!(feed(&mut s, &test_request(seq, id)).is_empty());
    }

    // Closing the gap replays the four that fit. `SEVEN` is not among them.
    let out = feed(&mut s, &test_request(2, "TWO"));
    assert_eq!(
        ids(&out),
        ["TWO", "THREE", "FOUR", "FIVE", "SIX"],
        "four held plus the one that closed the gap: {out:?}"
    );

    // The count now expects 7, and the session has not lost its place.
    let out = feed(&mut s, &test_request(7, "SEVEN"));
    assert_eq!(
        ids(&out),
        ["SEVEN"],
        "the dropped message is ordinary again once the count reaches it: {out:?}"
    );
}

/// The same number arriving twice takes one slot, not two.
///
/// A counterparty retransmitting while a resend is outstanding is ordinary.
/// Without this, four copies of one message fill the queue and the *next*
/// number has nowhere to go — so the session replays one message and then
/// stops, having lost a message it was told about.
#[test]
fn a_repeated_number_running_ahead_takes_one_slot() {
    let mut s = logged_on();

    let out = feed(&mut s, &test_request(4, "FOUR"));
    assert_eq!(msg_types(&out), ["2"], "{out:?}");
    for _ in 0..4 {
        assert!(feed(&mut s, &test_request(4, "FOUR")).is_empty());
    }
    assert!(feed(&mut s, &test_request(5, "FIVE")).is_empty());

    let out = feed(&mut s, &test_request(2, "TWO"));
    assert_eq!(ids(&out), ["TWO"], "3 is still missing: {out:?}");
    let out = feed(&mut s, &test_request(3, "THREE"));
    assert_eq!(
        ids(&out),
        ["THREE", "FOUR", "FIVE"],
        "five copies of 4 must not have crowded 5 out: {out:?}"
    );
}
