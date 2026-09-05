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

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_engine::journal::Store;
use fixbolt_session::journal::Journal;
use fixbolt_session::{Acceptor, Application, Config, Link, Session};

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
    let mut journal = Store::new();
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
        let link = s.received_with(&wire, &mut app, &mut journal, |b| {
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
    let mut journal = Store::new();
    let order = inputs("15_HeaderAndBodyFieldsOrderedDifferently.def")[1].clone();
    let mut s = logged_on();
    let mut app = Recorder::default();
    s.received_with(&order, &mut app, &mut journal, |_| {});

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

// ----------------------------- the inbound count is written down (ADR-0017)

/// [ADR-0017](../../../docs/decisions/ADR-0017-the-inbound-count-is-persisted-after-delivery.md):
/// the session records which inbound sequence numbers it has consumed, so a
/// restart knows what it has already seen.
///
/// **The journal tests next door prove only that a journal can store it.** They
/// would all still pass with a session that never calls `mark_in` — this is the
/// test that says the session does, and the one below says *when*.
#[test]
fn the_session_marks_the_inbound_count_it_has_consumed() {
    let mut s = logged_on();
    let mut j: Store = Store::new();
    let mut app = Recorder::default();

    assert_eq!(
        j.highest_in(),
        None,
        "nothing consumed through this journal yet"
    );

    let msg = reframe(&inputs("4b_ReceivedTestRequest.def")[1]);
    let link = s.received_with(&msg, &mut app, &mut j, |_| {});
    assert_eq!(link, Link::Up);

    assert_eq!(
        j.highest_in(),
        Some(2),
        "the count the session reached is on record, not just in memory"
    );
    assert_eq!(s.next_in(), 3, "and it is one below what it expects next");
}

/// **The ordering is the decision, so the ordering is what is tested.**
///
/// ADR-0017 chose *after delivery* over *before*, and every assertion above
/// passes under either. This one puts the application and the journal on one
/// shared log and reads the order back: if the mark were written first, an
/// ill-timed crash would lose the message instead of repeating it, and no
/// count-based assertion anywhere would notice.
#[test]
fn the_mark_is_written_after_the_application_has_seen_the_message() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Event {
        Delivered(u32),
        Marked(u32),
    }

    struct Watcher(Rc<RefCell<Vec<Event>>>);
    impl Application for Watcher {
        fn on_message(
            &mut self,
            _msg: &[u8],
            seq: u32,
            _stamp: &[u8],
            _out: &mut [u8],
        ) -> Option<Range<usize>> {
            self.0.borrow_mut().push(Event::Delivered(seq));
            None
        }
    }

    struct Watched {
        inner: Store,
        log: Rc<RefCell<Vec<Event>>>,
    }
    impl Journal for Watched {
        fn put(&mut self, seq: u32, bytes: &[u8]) -> bool {
            self.inner.put(seq, bytes)
        }
        fn get(&self, seq: u32) -> Option<&[u8]> {
            self.inner.get(seq)
        }
        fn oldest(&self) -> Option<u32> {
            self.inner.oldest()
        }
        fn highest(&self) -> Option<u32> {
            self.inner.highest()
        }
        fn mark_in(&mut self, seq: u32) {
            self.log.borrow_mut().push(Event::Marked(seq));
            self.inner.mark_in(seq);
        }
        fn highest_in(&self) -> Option<u32> {
            self.inner.highest_in()
        }
        fn mark_out(&mut self, seq: u32) {
            self.inner.mark_out(seq);
        }
        fn highest_out(&self) -> Option<u32> {
            self.inner.highest_out()
        }
    }

    let log = Rc::new(RefCell::new(Vec::new()));
    let mut s = logged_on();
    let mut app = Watcher(Rc::clone(&log));
    let mut j = Watched {
        inner: Store::new(),
        log: Rc::clone(&log),
    };

    // An application message — `35=D` — so the application is genuinely called.
    // The order out of `15_HeaderAndBodyFieldsOrderedDifferently.def`, which is
    // the message the test above already relies on reaching the application.
    let msg = inputs("15_HeaderAndBodyFieldsOrderedDifferently.def")[1].clone();
    s.received_with(&msg, &mut app, &mut j, |_| {});

    let seen = log.borrow().clone();
    let delivered = seen.iter().position(|e| matches!(e, Event::Delivered(_)));
    let marked = seen.iter().position(|e| matches!(e, Event::Marked(_)));
    assert!(
        delivered.is_some(),
        "the message must reach the application at all, or this proves nothing: {seen:?}"
    );
    assert!(
        marked.is_some(),
        "and it must be marked, or there is no recovery: {seen:?}"
    );
    assert!(
        delivered < marked,
        "ADR-0017: the mark comes AFTER delivery. Order was {seen:?}"
    );
}

/// The mark never goes backwards, which matters because a gap closing releases
/// held messages after the count has already moved past them.
#[test]
fn the_inbound_mark_never_goes_backwards() {
    let mut j: Store = Store::new();
    j.mark_in(9);
    j.mark_in(4);
    assert_eq!(
        j.highest_in(),
        Some(9),
        "a later, lower mark must not undo an earlier, higher one"
    );
}
