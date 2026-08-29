//! What the session hands the application, and the one `PossDup` exemption the
//! corpus never exercises.
//!
//! Two things here are invisible to the score. The `52=` an application writes
//! is one of the five tags `fields.fmt` matches by **shape**, so the corpus
//! checks its width and never its value — a session that handed the application
//! a stale clock, or a constant, would score the same. And every `43=Y`
//! `SequenceReset` in the corpus happens to carry `122=`, so the rule that a
//! gap fill is never asked for one is never put to the question.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use nanofix_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use nanofix_session::{Acceptor, Application, Config, Link, Session};

fn acceptor() -> Session<Acceptor, 256> {
    Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
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

/// An application that answers nothing and remembers everything it was told.
#[derive(Default)]
struct Recorder {
    calls: Vec<(u32, String)>,
}

impl Application for Recorder {
    fn on_message(
        &mut self,
        _msg: &[u8],
        seq: u32,
        stamp: &[u8],
        _out: &mut [u8],
    ) -> Option<Range<usize>> {
        self.calls
            .push((seq, String::from_utf8_lossy(stamp).into_owned()));
        None
    }
}

/// A session logged on at [`FIXED_TIME_MILLIS`] with `108=30`, its Logon reply
/// discarded. Inbound count 2, outbound count 2.
///
/// The `HeartBtInt` matters: `15_HeaderAndBodyFieldsOrderedDifferently.def`
/// asks for `108=2`, which times the session out 4.8 s later — too short to
/// tick a clock forward under.
fn logged_on() -> Session<Acceptor, 256> {
    let mut s = acceptor();
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let link = s.received(&inputs("4b_ReceivedTestRequest.def")[0], |_| {});
    assert_eq!(link, Link::Up, "the Logon should have been accepted");
    s
}

/// The application is given the clock the session was ticked to, not a
/// constant and not a stale one.
///
/// `52` is one of the five tags `test/definitions/fields.fmt` matches by shape,
/// so the corpus pins the **width** of what the application writes and never
/// its value. `[measured 2026-08-29]` shifting the stamp handed over by four
/// seconds leaves the score at 55 / 59 and turns nothing else red — only this
/// test.
#[test]
fn the_application_is_given_the_clock_the_session_was_ticked_to() {
    let mut s = logged_on();
    let mut app = Recorder::default();
    let order = inputs("15_HeaderAndBodyFieldsOrderedDifferently.def")[1].clone();

    // Two orders, fifteen seconds apart. Both gaps are inside 1.0 x 30 s, so
    // the session sends nothing of its own and spends no number between them.
    for (at, seq) in [(10_000u64, 2u32), (25_000, 3)] {
        s.tick(FIXED_TIME_MILLIS + at, |_| {});
        let wire = reframe(&set(
            &order,
            "\u{1}34=2\u{1}",
            &format!("\u{1}34={seq}\u{1}"),
        ));
        let mut out = Vec::new();
        let link = s.received_with(&wire, &mut app, |b| {
            out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
        });
        assert_eq!(link, Link::Up, "the order was accepted: {out:?}");
        assert!(out.is_empty(), "and answered by nobody: {out:?}");
    }

    // The number is the session's own, and it did not move: a silent
    // application spends nothing. The clock did move, by exactly the tick.
    assert_eq!(
        app.calls,
        [
            (2, "20260828-12:00:10.000".to_string()),
            (2, "20260828-12:00:25.000".to_string()),
        ],
        "the clock the session was last ticked to, to the millisecond"
    );
    assert_eq!(
        app.calls[0].1.len(),
        21,
        "and 21 bytes, which is what `9=101` counts"
    );
}

/// An application that says nothing spends no sequence number.
///
/// `19a_PossResendMessageThatHAsAlreadyBeenSent.def` proves it once, through a
/// Logout numbered three lines later. This says it in one assertion.
#[test]
fn an_application_that_says_nothing_spends_no_sequence_number() {
    let order = inputs("15_HeaderAndBodyFieldsOrderedDifferently.def")[1].clone();
    let mut s = logged_on();
    let mut app = Recorder::default();
    s.received_with(&order, &mut app, |_| {});

    // The next message the session sends itself must still be 2.
    let logout = inputs("15_HeaderAndBodyFieldsOrderedDifferently.def")[3].clone();
    let logout = reframe(&set(&logout, "\u{1}34=4\u{1}", "\u{1}34=3\u{1}"));
    let mut out = Vec::new();
    s.received(&logout, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    assert_eq!(out.len(), 1, "the Logout reply: {out:?}");
    assert!(
        out[0].contains("|34=2|"),
        "the silent application spent nothing: {}",
        out[0]
    );
}

/// A `SequenceReset` behind the count is not asked for an `OrigSendingTime`.
///
/// QuickFIX's `doPossDup` exempts it by message type, and the reason is plain:
/// a gap fill stands in for messages rather than repeating one, so there is no
/// original send time to carry. `[measured 2026-08-29]` every `43=Y`
/// `SequenceReset` in the corpus carries `122=` anyway, so removing the
/// exemption leaves the score at 55 / 59 and turns nothing else red.
#[test]
fn a_gap_fill_behind_the_count_is_not_asked_for_an_orig_sending_time() {
    let mut s = logged_on();

    // Two heartbeats, to put the count at 4.
    let hb = inputs("2c_MsgSeqNumTooLow.def")[1].clone();
    for seq in [2, 3] {
        let wire = reframe(&set(&hb, "\u{1}34=2\u{1}", &format!("\u{1}34={seq}\u{1}")));
        assert!(s.received(&wire, |_| {}) == Link::Up);
    }

    // A gap fill for a number already used, admitted with `43=Y` and carrying
    // no `122=` at all.
    let reset = reframe(
        b"8=FIX.4.4\x019=0\x0135=4\x0134=2\x0143=Y\x0149=TW44\x01\
          52=20260828-12:00:00\x0156=ISLD\x0136=4\x01123=Y\x0110=0\x01",
    );

    let mut out = Vec::new();
    let link = s.received(&reset, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    assert_eq!(link, Link::Up, "and the session is not ended over it");
    assert!(
        out.is_empty(),
        "a gap fill has no original send time to be missing: {out:?}"
    );
}
