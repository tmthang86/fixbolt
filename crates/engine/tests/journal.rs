//! What the journal does that the corpus cannot see. `DESIGN.md` D7.
//!
//! **This file used to live in `crates/session/`**, because the store did.
//! It moved with the store: the session now says *keep this* and asks *do you
//! still have it*, and holds nothing.
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

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_engine::journal::{Durability, FileJournal, MemJournal, SLOT_LEN, Store};
use fixbolt_session::journal::{Journal, NoJournal};
use fixbolt_session::{Acceptor, Application, Config, Link, Session};

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
        fixbolt_conformance::echo::echo(msg, out, seq, stamp).ok()
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
/// The ring the wrapping tests below are about.
///
/// **Eight on purpose, and not `Store`.** `Store`'s `SLOTS` is a deployment
/// default and moved from 8 to 4096 on 2026-09-04 (ADR-0046 decision 2). A test
/// that asks *"what happens when the ring wraps"* through the default stops
/// asking anything the moment the default is larger than the test's own
/// message count — it goes green by never wrapping, which is the failure mode
/// `docs/reference/` keeps collecting. So the size is named here.
type Ring8 = MemJournal<8, SLOT_LEN>;

fn logged_on() -> (Session<Acceptor, 256>, Ring8) {
    let mut s = Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"));
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let link = s.received(&inputs("4b_ReceivedTestRequest.def")[0], |_| {});
    assert_eq!(link, Link::Up, "the Logon should have been accepted");
    (s, Ring8::new())
}

fn feed<J: Journal>(s: &mut Session<Acceptor, 256>, j: &mut J, wire: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    s.received_with(wire, &mut EchoApp, j, |b| {
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
    let (mut s, mut j) = logged_on();
    assert_eq!(
        msg_types(&feed(&mut s, &mut j, &order(2))),
        ["D"],
        "the echo"
    );

    // Twenty-five seconds later, which is inside `108=30` so the session says
    // nothing of its own and spends no number.
    s.tick(FIXED_TIME_MILLIS + 25_000, |_| {});
    let out = feed(&mut s, &mut j, &resend_request(3, 2, 0));

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
    let out = feed(&mut s, &mut j, &order(4));
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
    let (mut s, mut j) = logged_on();
    let out = feed(&mut s, &mut j, &long);
    assert_eq!(msg_types(&out), ["D"], "the echo still goes out: {out:?}");
    assert!(out[0].len() > 512, "and it is too big to keep");

    let out = feed(&mut s, &mut j, &resend_request(3, 2, 0));
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
    let (mut s, mut j) = logged_on();
    for seq in 2..=10 {
        assert_eq!(
            msg_types(&feed(&mut s, &mut j, &order(seq))),
            ["D"],
            "echo {seq}"
        );
    }

    // Nine kept in eight slots: `34=2` is the one that went.
    //
    // `[2026-09-04]` **and it takes two calls now.** A resend goes out in
    // batches of `Config::resend_batch` messages — 8 by default — and a fill
    // plus eight replays is nine. The assertion is unchanged in substance: the
    // whole answer, in order, nothing skipped. ADR-0046 decision 4.
    let mut out = feed(&mut s, &mut j, &resend_request(11, 2, 0));
    while out.len() < 9 {
        let before = out.len();
        s.tick_with(FIXED_TIME_MILLIS, &mut j, |b| {
            out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
        });
        assert!(
            out.len() > before,
            "the replay stalled at {} of 9",
            out.len()
        );
    }
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

// ------------------------------------------------------- the three policies

/// D7's `None`: nothing is kept, and a `ResendRequest` is answered entirely
/// with gap fills.
///
/// **This is a legal answer, not a degraded one.** A journal that does not
/// survive a restart could not have replayed after one either, so a simulator
/// loses nothing by keeping nothing.
#[test]
fn none_keeps_nothing_and_fills_over_everything() {
    let (mut s, _) = logged_on();
    let mut none = NoJournal;

    let mut out = Vec::new();
    s.received_with(&order(2), &mut EchoApp, &mut none, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    assert_eq!(msg_types(&out), ["D"], "the echo still goes out");

    let mut out = Vec::new();
    s.received_with(&resend_request(3, 2, 0), &mut EchoApp, &mut none, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    assert_eq!(
        msg_types(&out),
        ["4"],
        "one SequenceReset gap fill and no replay: {out:?}"
    );
    assert!(
        out.iter().any(|m| m.contains("|123=Y|")),
        "and it says it is a gap fill: {out:?}"
    );
}

/// The session says *keep this*, and the journal is what decides where it
/// goes. Here: a file, `fsync`ed before `put` returns.
#[test]
fn fsync_puts_the_message_on_disk_before_it_returns() {
    let path =
        std::env::temp_dir().join(format!("fixbolt-journal-fsync-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let (mut s, _) = logged_on();
    let mut j: FileJournal<8, SLOT_LEN> =
        FileJournal::open(&path, Durability::Fsync).expect("open");
    s.received_with(&order(2), &mut EchoApp, &mut j, |_| {});

    // Nothing has been closed and nothing has been flushed by hand: `Fsync`
    // means the bytes are already there.
    let on_disk = std::fs::read(&path).expect("read");
    assert!(
        find(&on_disk, b"35=D").is_some(),
        "the echoed order is on disk: {} bytes",
        on_disk.len()
    );
    assert!(
        j.get(2).is_some(),
        "and it can still be replayed without reading the file back"
    );
    drop(j);
    let _ = std::fs::remove_file(&path);
}

/// `Async` is D7's default: the engine thread hands the bytes over and returns.
/// They reach the disk when the writer thread gets there.
#[test]
fn async_reaches_the_disk_once_the_writer_has_caught_up() {
    let path =
        std::env::temp_dir().join(format!("fixbolt-journal-async-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let (mut s, _) = logged_on();
    let mut j: FileJournal<8, SLOT_LEN> =
        FileJournal::open(&path, Durability::Async).expect("open");
    for seq in 2..=4u32 {
        s.received_with(&order(seq), &mut EchoApp, &mut j, |_| {});
    }
    assert!(
        j.get(2).is_some() && j.get(4).is_some(),
        "a resend is answered from memory, not from the file"
    );

    // `close` is the only synchronisation there is, and that is the trade:
    // nothing blocks the engine thread, so nothing knows when the disk has it.
    j.close();
    let on_disk = std::fs::read(&path).expect("read");
    assert_eq!(
        count_of(&on_disk, b"35=D"),
        3,
        "all three reached the writer thread: {} bytes",
        on_disk.len()
    );
    drop(j);
    let _ = std::fs::remove_file(&path);
}

/// A message longer than a slot is refused rather than truncated, whichever
/// journal is fitted — a truncated replay is a message that does not checksum.
#[test]
fn a_message_too_long_for_a_slot_is_refused_by_every_journal() {
    let long = vec![b'x'; SLOT_LEN + 1];
    let mut mem = Store::new();
    mem.put(9, &long);
    assert!(mem.get(9).is_none(), "the ring refused it");

    let path =
        std::env::temp_dir().join(format!("fixbolt-journal-long-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut file: FileJournal<8, SLOT_LEN> =
        FileJournal::open(&path, Durability::Fsync).expect("open");
    file.put(9, &long);
    assert!(
        file.get(9).is_none(),
        "and so did the one that also writes to disk"
    );
    drop(file);
    let _ = std::fs::remove_file(&path);
}

// ------------------------------------------- what the ring will say about itself
//
// ADR-0046 decisions 1 and 3. Two questions the trait could not answer before,
// and both are load-bearing for the counters the session grows in step 3: a
// session cannot tell *"this number was never sent"* from *"this number fell
// out of the ring"* without `oldest`, and only the second is worth an event.

/// `oldest` names the lowest number still answerable, and it moves as the ring
/// wraps.
///
/// `None` on an empty journal — **not** `Some(0)`, which would be a sequence
/// number FIX never uses being handed back as if it were one.
#[test]
fn the_journal_says_what_its_oldest_kept_number_is() {
    let mut j: MemJournal<8, SLOT_LEN> = MemJournal::new();
    assert_eq!(j.oldest(), None, "an empty ring holds no oldest number");

    for seq in 1..=8 {
        assert!(j.put(seq, b"body"), "the ring took {seq}");
    }
    assert_eq!(
        j.oldest(),
        Some(1),
        "eight in eight slots: nothing has gone"
    );
    assert_eq!(j.highest(), Some(8));

    assert!(j.put(9, b"body"));
    assert_eq!(j.oldest(), Some(2), "9 overwrote 1, so 2 is the oldest");
    assert_eq!(j.highest(), Some(9));

    for seq in 10..=20 {
        assert!(j.put(seq, b"body"));
    }
    assert_eq!(j.oldest(), Some(13), "twenty put, eight kept: 13..=20");
    assert_eq!(j.get(12), None, "and 12 really is gone");
    assert!(j.get(13).is_some());
}

/// A `put` the journal refuses says so, rather than returning nothing and
/// leaving the session to find out from a counterparty.
///
/// The refusal itself is not new — a message longer than `LEN` has always been
/// dropped rather than truncated, because a truncated replay does not checksum.
/// What is new is that the caller is told.
#[test]
fn a_put_that_is_refused_says_so() {
    let mut j: MemJournal<8, SLOT_LEN> = MemJournal::new();
    assert!(j.put(1, b"short enough"), "a message that fits is kept");

    let long = vec![b'x'; SLOT_LEN + 1];
    assert!(
        !j.put(2, &long),
        "one byte too long is refused, and says so"
    );
    assert_eq!(j.get(2), None, "and it really was not kept");
    assert_eq!(j.oldest(), Some(1), "a refusal does not disturb the ring");

    // The other two implementations answer the same way. `NoJournal` keeps
    // nothing at all, so every `put` is a refusal — which is the honest answer
    // and is what makes the session's counter meaningful for it too.
    let mut none = NoJournal;
    assert!(!none.put(1, b"short enough"));
    assert_eq!(none.oldest(), None);

    let path = std::env::temp_dir().join(format!(
        "fixbolt-journal-refused-{}.log",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut file: FileJournal<8, SLOT_LEN> =
        FileJournal::open(&path, Durability::Fsync).expect("open");
    assert!(file.put(1, b"short enough"));
    assert!(
        !file.put(2, &long),
        "the one that also writes to disk agrees"
    );
    assert_eq!(file.oldest(), Some(1));
    drop(file);
    let _ = std::fs::remove_file(&path);
}

// ----------------------------------------- the ring at the size it now ships
//
// ADR-0046 decision 2. These three run against `Store` deliberately: they are
// the tests that would notice if the default went back to something a trading
// day overflows, and the point of them is the default rather than the ring.

/// Four thousand and one hundred puts, 4096 kept, and `oldest` moved with them.
#[test]
fn four_thousand_puts_keep_the_last_4096_and_oldest_moves_with_them() {
    let mut j = Store::new();
    for seq in 1..=4096 {
        assert!(j.put(seq, b"body"), "the ring took {seq}");
    }
    assert_eq!(j.oldest(), Some(1), "4096 in 4096 slots: nothing has gone");
    assert_eq!(j.highest(), Some(4096));

    for seq in 4097..=4196 {
        assert!(j.put(seq, b"body"));
    }
    assert_eq!(j.oldest(), Some(101), "a hundred more pushed a hundred out");
    assert_eq!(j.highest(), Some(4196));
    assert_eq!(j.get(100), None);
    assert!(j.get(101).is_some());
}

/// Ten thousand messages through a 4096-slot ring, and **every** number is
/// answered correctly — the ones still there and the ones gone.
///
/// The second half is the half worth having: a `get` that returned the wrong
/// slot's bytes would still be `Some`, and a resend built on it would replay a
/// message under somebody else's sequence number.
#[test]
fn get_finds_by_number_after_the_ring_wraps() {
    let mut j = Store::new();
    for seq in 1..=10_000u32 {
        let body = format!("msg-{seq}");
        assert!(j.put(seq, body.as_bytes()));
    }

    // 10 000 put, 4096 kept: 5905..=10 000.
    for seq in 5905..=10_000u32 {
        let want = format!("msg-{seq}");
        assert_eq!(
            j.get(seq).map(|b| String::from_utf8_lossy(b).into_owned()),
            Some(want),
            "{seq} is still here and is itself"
        );
    }
    for seq in [1u32, 2, 4095, 4096, 5903, 5904] {
        assert_eq!(j.get(seq), None, "{seq} fell out and does not come back");
    }
    assert_eq!(j.oldest(), Some(5905));
    assert_eq!(j.highest(), Some(10_000));
}

/// A slot indexed by `seq % N` holds one number at a time, and it is checked.
///
/// `Admin::SetNextOut` moves an outbound number **backwards** (ADR-0036), so a
/// number already in the ring can be sent again with different bytes. A `get`
/// that trusted the index without comparing the number would hand back the
/// previous message — under the right sequence number, with a valid checksum,
/// and wrong.
#[test]
fn a_number_reused_after_an_admin_reset_does_not_return_the_old_bytes() {
    let mut j: Ring8 = Ring8::new();
    assert!(j.put(9, b"the first nine"));
    assert_eq!(j.get(9), Some(&b"the first nine"[..]));

    // An operator winds the count back and the session sends 9 again.
    assert!(j.put(9, b"the second nine"));
    assert_eq!(
        j.get(9),
        Some(&b"the second nine"[..]),
        "the newer message under that number"
    );

    // And a number whose slot is occupied by somebody else is absent, not
    // somebody else's bytes: 17 % 8 == 1, and slot 1 holds the second nine.
    assert_eq!(j.get(17), None, "17 was never sent");
}

// --------------------------------------------- what a gap fill costs, counted
//
// ADR-0046 decision 1. A `ResendRequest` reaching past the ring is answered
// with a gap fill, which is legal and **silent**, and the silence is the whole
// problem. These three are about the counters that end it.
//
// They live here rather than in `crates/session/tests/` — where the counters
// themselves are — because the helpers that build a real corpus order and a
// real `ResendRequest` are here, and a second copy of them is two fixtures that
// will eventually disagree.

/// The counter names **how many messages** may have been lost, not how many
/// times it happened.
///
/// One resend that fills over thirteen numbers is thirteen messages the
/// counterparty asked for and did not get. A counter reading `1` would say the
/// ring is fine.
#[test]
fn a_resend_that_reaches_below_the_ring_counts_every_number_it_filled() {
    let (mut s, mut j) = logged_on();
    assert_eq!(s.resend_beyond_journal(), 0, "nothing has been filled yet");

    // Twenty orders echoed back: outbound 34=2..=21, of which the eight-slot
    // ring keeps 14..=21. `34=1` was the Logon and was never journalled.
    for seq in 2..=21 {
        assert_eq!(
            msg_types(&feed(&mut s, &mut j, &order(seq))),
            ["D"],
            "{seq}"
        );
    }

    // One fill and eight replays is nine messages, which is two batches.
    let mut out = feed(&mut s, &mut j, &resend_request(22, 1, 0));
    while out.len() < 9 {
        let before = out.len();
        s.tick_with(FIXED_TIME_MILLIS, &mut j, |b| {
            out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
        });
        assert!(
            out.len() > before,
            "the replay stalled at {} of 9",
            out.len()
        );
    }
    assert_eq!(
        msg_types(&out),
        ["4", "D", "D", "D", "D", "D", "D", "D", "D"],
        "one fill for everything gone, then the eight still here: {out:?}"
    );
    assert!(
        out[0].contains("|34=1|") && out[0].contains("|36=14|"),
        "the fill covers 1 through 13: {}",
        out[0]
    );
    assert_eq!(
        s.resend_beyond_journal(),
        13,
        "thirteen numbers were filled below the ring's floor, and the counter \
         says thirteen rather than one"
    );
}

/// A ring whose floor is still 1 has lost nothing, and says so.
///
/// **This is the half that stops the counter being an alarm that is always
/// on.** A session gap-fills over its own administrative messages on every
/// reconnect that asks `7=1`; none of those was ever resendable by anybody, and
/// none of them is a loss.
#[test]
fn a_fill_over_messages_the_ring_never_held_is_not_counted() {
    let (mut s, mut j) = logged_on();
    for seq in 2..=3 {
        assert_eq!(
            msg_types(&feed(&mut s, &mut j, &order(seq))),
            ["D"],
            "{seq}"
        );
    }

    let out = feed(&mut s, &mut j, &resend_request(4, 1, 0));
    assert_eq!(
        msg_types(&out),
        ["4", "D", "D"],
        "a fill for the Logon, then both orders replayed: {out:?}"
    );
    assert_eq!(
        s.resend_beyond_journal(),
        0,
        "the ring's floor is still 1, so nothing has fallen out of it"
    );
}

/// A message the journal refuses is counted, so an acceptor whose replies are
/// longer than its slots finds out from a counter rather than from a
/// counterparty.
#[test]
fn a_put_the_journal_refuses_is_counted() {
    let mut s = Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"));
    let mut j: MemJournal<8, 64> = MemJournal::new();
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    s.received(&inputs("8_OnlyApplicationMessages.def")[0], |_| {});
    assert_eq!(s.puts_refused(), 0);

    // The echo of a corpus order is well over 64 bytes, so every one of these
    // is refused by the ring — and sent anyway. Declining to keep a message is
    // not declining to send it.
    for seq in 2..=4 {
        assert_eq!(
            msg_types(&feed(&mut s, &mut j, &order(seq))),
            ["D"],
            "{seq}"
        );
    }
    assert_eq!(j.get(2), None, "and none of them was kept");
    assert_eq!(
        s.puts_refused(),
        3,
        "three refusals, counted — the ring is silent about this and the \
         session is not"
    );
}

// ------------------------------------------- a replay that takes several turns
//
// ADR-0046 decision 4. The wire half of this is
// `crates/engine/tests/backpressure.rs::a_resend_larger_than_tx_does_not_end_
// the_session`, which is the one that proves the session survives it. These are
// the session's own rules about the cursor.

/// A logged-on acceptor with a ring big enough to keep everything.
fn logged_on_deep(batch: u16) -> (Session<Acceptor, 256>, Store) {
    let mut s =
        Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_resend_batch(batch));
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let link = s.received(&inputs("4b_ReceivedTestRequest.def")[0], |_| {});
    assert_eq!(link, Link::Up, "the Logon should have been accepted");
    (s, Store::new())
}

/// The `34=` of every reply carrying `43=Y`, in the order they came out.
fn replayed(replies: &[String]) -> Vec<u32> {
    replies
        .iter()
        .filter(|r| r.contains("|43=Y|"))
        .filter_map(|r| {
            let at = r.find("|34=")? + 4;
            let end = r[at..].find('|')? + at;
            r[at..end].parse().ok()
        })
        .collect()
}

/// A hundred messages come back over several calls, in order, none missing and
/// none twice.
#[test]
fn a_long_resend_is_replayed_over_several_ticks_in_order() {
    let (mut s, mut j) = logged_on_deep(8);
    for seq in 2..=101 {
        assert_eq!(
            msg_types(&feed(&mut s, &mut j, &order(seq))),
            ["D"],
            "{seq}"
        );
    }

    let mut out = feed(&mut s, &mut j, &resend_request(102, 2, 0));
    let first = replayed(&out).len();
    assert_eq!(first, 8, "one batch on the call that asked, not a hundred");

    let mut calls = 1;
    while replayed(&out).len() < 100 {
        calls += 1;
        assert!(
            calls < 100,
            "still {} of 100 after {calls} calls",
            replayed(&out).len()
        );
        s.tick_with(FIXED_TIME_MILLIS, &mut j, |b| {
            out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
        });
    }
    assert_eq!(calls, 13, "a hundred at eight a call is thirteen calls");
    assert_eq!(
        replayed(&out),
        (2..=101).collect::<Vec<u32>>(),
        "in order, none missing, none twice"
    );

    // And it stops when it is done: a fourteenth call says nothing.
    let mut after = Vec::new();
    s.tick_with(FIXED_TIME_MILLIS, &mut j, |b| after.push(b.len()));
    assert!(after.is_empty(), "nothing is owed any more: {after:?}");
}

/// `tick`, without a journal, cannot continue a replay — and the plain form is
/// still there for every caller that has no journal at all.
///
/// This is the trap the split exists for: an engine that kept calling `tick`
/// would answer the first batch and then go quiet for ever, and every gate in
/// this repository would stay green because none of them asks for more than
/// three messages.
#[test]
fn a_replay_stalls_on_the_journal_less_tick_and_says_nothing_wrong() {
    let (mut s, mut j) = logged_on_deep(8);
    for seq in 2..=21 {
        feed(&mut s, &mut j, &order(seq));
    }
    let out = feed(&mut s, &mut j, &resend_request(22, 2, 0));
    assert_eq!(replayed(&out).len(), 8, "the first batch went out");

    let mut after = Vec::new();
    for _ in 0..5 {
        s.tick(FIXED_TIME_MILLIS, |b| {
            after.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
        });
    }
    assert!(
        replayed(&after).is_empty(),
        "`tick` has no journal to replay from: {after:?}"
    );
    // And `tick_with` picks it up where it was left.
    let mut resumed = Vec::new();
    s.tick_with(FIXED_TIME_MILLIS, &mut j, |b| {
        resumed.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    assert_eq!(replayed(&resumed), (10..=17).collect::<Vec<u32>>());
}

/// A replay does not outlive the connection it was owed on.
///
/// Without this the first tick of the **next** connection pushes the previous
/// session's `43=Y` messages onto it, numbered for a session that has ended.
#[test]
fn a_disconnect_cancels_a_resend_in_progress() {
    let (mut s, mut j) = logged_on_deep(8);
    for seq in 2..=101 {
        feed(&mut s, &mut j, &order(seq));
    }
    let out = feed(&mut s, &mut j, &resend_request(102, 2, 0));
    assert_eq!(replayed(&out).len(), 8, "a replay is in progress");

    s.disconnect(|_| {});
    s.connect(|_| {});
    let mut after = Vec::new();
    s.tick_with(FIXED_TIME_MILLIS, &mut j, |b| {
        after.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    assert!(
        replayed(&after).is_empty(),
        "the new connection owes nothing: {after:?}"
    );
}

/// A second `ResendRequest` replaces the first; it does not queue behind it.
///
/// A counterparty that asks twice wants the second answer. Two cursors would
/// interleave two replays on one wire, and the numbers would look like a
/// session that had lost count.
#[test]
fn a_new_resend_request_replaces_the_one_in_progress() {
    let (mut s, mut j) = logged_on_deep(4);
    for seq in 2..=101 {
        feed(&mut s, &mut j, &order(seq));
    }
    let first = feed(&mut s, &mut j, &resend_request(102, 2, 0));
    assert_eq!(replayed(&first), vec![2, 3, 4, 5], "four, then the cursor");

    let second = feed(&mut s, &mut j, &resend_request(103, 50, 53));
    assert_eq!(
        replayed(&second),
        vec![50, 51, 52, 53],
        "the new question, answered whole: {second:?}"
    );
    let mut after = Vec::new();
    s.tick_with(FIXED_TIME_MILLIS, &mut j, |b| {
        after.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    assert!(
        replayed(&after).is_empty(),
        "and the first question is not resumed: {after:?}"
    );
}

/// A new message sent while a replay is in flight carries the **next new**
/// number, not one from the range being replayed.
#[test]
fn a_message_sent_during_a_resend_carries_the_next_new_number() {
    let (mut s, mut j) = logged_on_deep(4);
    for seq in 2..=101 {
        feed(&mut s, &mut j, &order(seq));
    }
    let before = s.next_out();
    let out = feed(&mut s, &mut j, &resend_request(102, 2, 0));
    assert_eq!(replayed(&out).len(), 4, "a replay is in progress");
    assert_eq!(
        s.next_out(),
        before,
        "a replay spends no sequence number: that is what a resend is"
    );

    // A new order arrives mid-replay and is answered with the next new number.
    let answered = feed(&mut s, &mut j, &order(103));
    let fresh: Vec<u32> = answered
        .iter()
        .filter(|r| !r.contains("|43=Y|"))
        .filter_map(|r| {
            let at = r.find("|34=")? + 4;
            let end = r[at..].find('|')? + at;
            r[at..end].parse().ok()
        })
        .collect();
    assert_eq!(fresh, vec![before], "the new echo carries {before}");
}

/// A schedule boundary restarts both counts, so a replay owed on the old
/// numbers is owed on numbers that no longer mean anything.
#[test]
fn a_schedule_reset_cancels_a_resend_in_progress() {
    use fixbolt_session::schedule::Schedule;

    let day: u64 = 24 * 60 * 60 * 1000;
    // Open all day, closed for the last second: two instants a day apart are
    // inside the window and belong to different sessions, which is the only
    // thing this test needs from a schedule.
    let mut s = Session::new(
        Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
            .with_resend_batch(4)
            .with_schedule(
                Schedule::daily(0, 23 * 3_600 + 59 * 60 + 59).expect("00:00:00 to 23:59:59"),
            ),
    );
    let mut j = Store::new();
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    s.received(&inputs("4b_ReceivedTestRequest.def")[0], |_| {});
    for seq in 2..=101 {
        feed(&mut s, &mut j, &order(seq));
    }
    let out = feed(&mut s, &mut j, &resend_request(102, 2, 0));
    assert_eq!(replayed(&out).len(), 4, "a replay is in progress");

    // A day later: the same wall-clock window, a different session.
    let mut after = Vec::new();
    s.tick_with(FIXED_TIME_MILLIS + day, &mut j, |b| {
        after.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    assert!(
        replayed(&after).is_empty(),
        "the numbers restarted, so nothing is owed on the old ones: {after:?}"
    );
}

/// A batch of zero is read as one.
///
/// Zero would leave every resend permanently unfinished, which is worse than
/// any reading of what the caller meant, and the builders here are infallible.
#[test]
fn a_batch_of_zero_is_read_as_one() {
    let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_resend_batch(0);
    assert_eq!(cfg.resend_batch(), 1);

    let (mut s, mut j) = logged_on_deep(0);
    for seq in 2..=11 {
        feed(&mut s, &mut j, &order(seq));
    }
    let out = feed(&mut s, &mut j, &resend_request(12, 2, 0));
    assert_eq!(replayed(&out), vec![2], "one message a call, not none");
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn count_of(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}
