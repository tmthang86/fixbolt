//! The journal is told how far the outbound count has got, including the
//! numbers it holds no bytes for.
//!
//! Step 2 of [a-journal-that-knows-the-numbering]. The pure half: no socket, no
//! engine, no disk — a session, a journal, and three messages.
//!
//! # Why the acceptance corpus cannot see any of this
//!
//! The 59 definitions judge **what goes on the wire**. Every message here is
//! correct on the wire whether or not the journal was told anything, because
//! the session's own `next_out` is right the whole time — what is wrong is what
//! survives the process. A corpus that compares output byte for byte is blind
//! to it by construction, and `--test score` reads 59 / 59 with the telling
//! removed. That is recorded in ADR-0053 as a fact, not offered as a gate.
//!
//! [a-journal-that-knows-the-numbering]: ../../../docs/plans/2026-09-05-a-journal-that-knows-the-numbering.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_engine::journal::Store;
use fixbolt_session::journal::Journal;
use fixbolt_session::{Acceptor, Application, Config, Link, Session};

/// `108=30` on the Logon below, so a tick this far on produces a Heartbeat.
const PAST_THE_HEARTBEAT: u64 = FIXED_TIME_MILLIS + 31_000;

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// An application that is asked nothing and answers nothing. Every message in
/// this file is administrative on purpose — that is the whole subject.
struct Quiet;
impl Application for Quiet {
    fn on_message(
        &mut self,
        _msg: &[u8],
        _seq: u32,
        _stamp: &[u8],
        _out: &mut [u8],
    ) -> Option<Range<usize>> {
        None
    }
}

/// A real `Logon` out of the corpus, addressed to us. `CLAUDE.md` §7: a real
/// corpus line rather than an invented packet.
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

/// Rebuild `9=` and `10=` after a substitution.
fn reframe(wire: &[u8], from: &str, to: &str) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    let patched = s.replace(from, to);
    assert_ne!(patched, s, "{from} is not in the message");
    let after_9 = patched.find("\u{1}35=").expect("35= follows the frame") + 1;
    let at_10 = patched.find("\u{1}10=").map_or(patched.len(), |i| i + 1);
    let head_end = patched.find('\u{1}').expect("8= is a field") + 1;
    let body = at_10 - after_9;
    with_real_checksum(
        format!(
            "{}9={body}\u{1}{}10=0\u{1}",
            &patched[..head_end],
            &patched[after_9..at_10]
        )
        .as_bytes(),
    )
}

/// The counterparty's `Logout`, correctly framed. `98=` and `108=` are not
/// defined for a `Logout` and leaving them on gets a Reject rather than a
/// goodbye.
fn their_logout(seq: u32) -> Vec<u8> {
    reframe(
        &reframe(
            &reframe(&good_logon(), "35=A", "35=5"),
            "98=0\u{1}108=30\u{1}",
            "",
        ),
        "34=1",
        &format!("34={seq}"),
    )
}

/// Logged on, with the journal that saw it happen. The `Logon` answer is `34=1`.
fn logged_on() -> (Session<Acceptor, 256>, Store) {
    let mut s: Session<Acceptor, 256> = Session::new(cfg());
    let mut j = Store::new();
    let mut app = Quiet;
    s.connect(|_| {});
    s.tick_with(FIXED_TIME_MILLIS, &mut j, |_| {});
    assert_eq!(
        s.received_with(&good_logon(), &mut app, &mut j, |_| {}),
        Link::Up,
        "the premise: this session is logged on"
    );
    (s, j)
}

/// **The whole item, in one session.** A `Logon`, a `Heartbeat` and a `Logout`
/// spend `34=1`, `34=2` and `34=3`; not one of them is a message a journal
/// keeps, so `highest()` knows of none of them — and `highest_out()` must know
/// of all three, because that is the number a restart continues from.
///
/// `[measured 2026-09-05]` Before the session told the journal, this read
/// `None` and a real `libquickfix` refused the resumed session with
/// *"MsgSeqNum too low, expecting 4 but received 3"*. `STATUS.md` item 48.
#[test]
fn three_administrative_messages_and_the_journal_knows_the_count() {
    let (mut s, mut j) = logged_on();
    let mut app = Quiet;

    s.tick_with(PAST_THE_HEARTBEAT, &mut j, |_| {});
    assert_eq!(
        s.received_with(&their_logout(2), &mut app, &mut j, |_| {}),
        Link::Dropped,
        "a Logout answered is a link that ends"
    );

    assert_eq!(
        s.next_out(),
        4,
        "the premise: three numbers were spent — if this is not 4 the scenario \
         changed and the assertion below is measuring something else"
    );
    assert_eq!(
        j.highest(),
        None,
        "not one of the three is a message a journal keeps, and this is what \
         every worked example in the repository used to read as the count"
    );
    assert_eq!(
        j.highest_out(),
        Some(3),
        "the journal must know the count even though it holds none of the bytes"
    );
}

/// **The inbound count has the same hole, on the same line.**
///
/// `received_with` returns as soon as `judge` answers `Link::Dropped`, and the
/// counterparty's own `Logout` is judged exactly that way — so the number it
/// arrived under was consumed and never marked. A resumed session then expects
/// it again, the counterparty's next message is one too high, and this end
/// opens a gap and sends a `ResendRequest` for a message it already had.
///
/// `[measured 2026-09-05]` **found by the interop gate, not by this suite**:
/// with the outbound half fixed, the clean-logout scenario got far enough to
/// reach the inbound one and read `35=2: 1` where it wanted none. ADR-0053.
#[test]
fn the_logout_that_ends_the_session_is_still_a_message_that_was_consumed() {
    let (mut s, mut j) = logged_on();
    let mut app = Quiet;

    assert_eq!(
        s.received_with(&their_logout(2), &mut app, &mut j, |_| {}),
        Link::Dropped,
        "the premise: this is the path that returns early"
    );

    assert_eq!(
        s.next_in(),
        3,
        "the premise: the session consumed 34=1 and 34=2"
    );
    assert_eq!(
        j.highest_in(),
        Some(2),
        "and the journal must know it, or the resumed session asks for the \
         Logout again and the counterparty's next message opens a gap"
    );
}

/// The `Logon` alone is enough, and it is the smallest case that breaks a
/// restart: a process that logs on and dies has spent `34=1` and journalled
/// nothing.
#[test]
fn a_logon_on_its_own_is_already_a_number_the_journal_must_know() {
    let (_s, j) = logged_on();
    assert_eq!(j.highest(), None);
    assert_eq!(j.highest_out(), Some(1), "the Logon answer was 34=1");
}

/// **The mark is a high-water mark, so being told an old number does nothing.**
/// The session tells the journal the same number on every turn — without this
/// property a quiet session under `Durability::Fsync` would sync a disk per
/// turn for a count that has not moved.
#[test]
fn the_outbound_mark_never_goes_backwards_and_repeats_are_free() {
    let mut j: Store = Store::new();
    j.mark_out(9);
    j.mark_out(4);
    j.mark_out(9);
    assert_eq!(
        j.highest_out(),
        Some(9),
        "a later, lower mark must not undo an earlier, higher one"
    );
}

/// **A kept message raises the count too**, which is what makes telling the
/// journal after a successful `put` write nothing.
#[test]
fn a_kept_message_is_itself_a_spent_number() {
    let mut j: Store = Store::new();
    assert!(j.put(7, b"8=FIX.4.4|"), "the premise");
    assert_eq!(j.highest_out(), Some(7));
    j.mark_out(7);
    assert_eq!(j.highest_out(), Some(7), "telling it again changes nothing");
}
