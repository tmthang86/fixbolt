//! The seven fields an application never writes, and the order it never
//! chooses.
//!
//! Every assertion here compares **the whole message, byte for byte**, and not
//! a field at a time. `9=BodyLength` and `10=CheckSum` are functions of every
//! other byte, so a message that is right in every field and wrong in its frame
//! is a message no counterparty can read — and a per-field assertion would call
//! it green.
//!
//! The expected bytes were computed once, by hand, and are literals here on
//! purpose. Building the expectation with `TemplateBuilder` would make this
//! test agree with the implementation by construction, which is the shape
//! `CLAUDE.md` §10 calls a check that proves nothing.

// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]
// A test binary, not a library crate: non-negotiable 7 is about what ships.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt::{Answer, Reply};

/// `52`, as the session hands it over: 21 bytes, milliseconds included.
const STAMP: &[u8] = b"20260902-10:00:00.123";

/// What the handler is answering. `49=ALPHA` is the counterparty, `56=US` is
/// this acceptor — so the reply must carry `49=US` and `56=ALPHA`.
fn reply_for(out: &mut [u8]) -> Reply<'_> {
    Reply::new(b"FIX.4.4", 7, STAMP, None, b"US", b"ALPHA", out)
}

fn show(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace('\x01', "|")
}

/// One `ExecutionReport`, with the body fields named in an order no dictionary
/// would choose.
///
/// The handler names ten body fields, descending and shuffled. What must come
/// out is `MsgType`, then header tags ascending, then body tags ascending, with
/// the frame around it.
#[test]
fn the_library_writes_the_header_the_handler_did_not() {
    let mut out = [0u8; 512];
    let answer = reply_for(&mut out)
        .message(b"8")
        .field(151, b"0")
        .field(150, b"0")
        .field(55, b"IBM")
        .field(54, b"1")
        .field(39, b"0")
        .field(37, b"EXEC1")
        .field(17, b"E1")
        .field(14, b"0")
        .field(11, b"ORD1")
        .field(6, b"0")
        .send();

    let Answer::Sent(range) = answer else {
        panic!("the reply was not written: {answer:?}");
    };

    const EXPECTED: &[u8] = b"8=FIX.4.4\x019=111\x0135=8\x0134=7\x0149=US\x0152=20260902-10:00:00.123\x0156=ALPHA\x016=0\x0111=ORD1\x0114=0\x0117=E1\x0137=EXEC1\x0139=0\x0154=1\x0155=IBM\x01150=0\x01151=0\x0110=137\x01";

    assert_eq!(
        show(&out[range]),
        show(EXPECTED),
        "the reply is not the message the counterparty is owed"
    );
}

/// A handler that names a field the session owns must not put a second copy of
/// it on the wire.
///
/// Not a style rule: two `34=` in one message is two sequence numbers, and a
/// counterparty reads whichever its parser reaches first.
#[test]
fn a_handler_naming_a_session_field_is_ignored() {
    let mut out = [0u8; 512];
    let answer = reply_for(&mut out)
        .message(b"8")
        .field(34, b"999")
        .field(49, b"WRONG")
        .field(52, b"19700101-00:00:00.000")
        .field(56, b"WRONG")
        .field(151, b"0")
        .field(150, b"0")
        .field(55, b"IBM")
        .field(54, b"1")
        .field(39, b"0")
        .field(37, b"EXEC1")
        .field(17, b"E1")
        .field(14, b"0")
        .field(11, b"ORD1")
        .field(6, b"0")
        .send();

    let Answer::Sent(range) = answer else {
        panic!("the reply was not written: {answer:?}");
    };

    const EXPECTED: &[u8] = b"8=FIX.4.4\x019=111\x0135=8\x0134=7\x0149=US\x0152=20260902-10:00:00.123\x0156=ALPHA\x016=0\x0111=ORD1\x0114=0\x0117=E1\x0137=EXEC1\x0139=0\x0154=1\x0155=IBM\x01150=0\x01151=0\x0110=137\x01";

    assert_eq!(
        show(&out[range]),
        show(EXPECTED),
        "a handler's copy of a session field reached the wire"
    );
}

/// `silent()` is an answer, and it writes nothing.
#[test]
fn silence_is_an_answer_and_not_a_message() {
    let mut out = [0u8; 512];
    assert_eq!(reply_for(&mut out).silent(), Answer::Silent);
    assert!(
        out.iter().all(|&b| b == 0),
        "a silent reply wrote into the output buffer"
    );
}

/// An output buffer too small is [`Answer::Failed`], never a partial message.
///
/// The engine sends what the range names. A half-written message with a range
/// would be bytes on the wire that no counterparty can frame.
#[test]
fn a_reply_that_does_not_fit_fails_rather_than_truncating() {
    let mut out = [0u8; 24];
    let answer = reply_for(&mut out)
        .message(b"8")
        .field(37, b"EXEC1")
        .field(150, b"0")
        .send();

    assert!(
        matches!(answer, Answer::Failed(_)),
        "expected a failure, got {answer:?}"
    );
    assert_eq!(answer.range(), None, "a failed reply named a range");
}

/// `35=j`, whole, and it is a **business** reject rather than a session one.
///
/// Step 6 of `plans/2026-09-04-settings-for-both-roles.md`. The expected bytes
/// are a literal for the reason the module header gives: building them with
/// `TemplateBuilder` would make this agree with the implementation by
/// construction.
#[test]
fn a_business_reject_carries_the_four_fields_in_dictionary_order() {
    let mut out = [0u8; 512];
    let mut m = reply_for(&mut out).business_reject(b"11", b"D", 2, b"unknown security");
    let answer = m.send();

    let Answer::Sent(range) = answer else {
        panic!("it should have been sent: {answer:?}");
    };
    assert_eq!(
        show(&out[range]),
        // `9=88` and `10=155` were computed away from the engine — the field
        // bytes summed by hand and by a one-line script — because a frame
        // copied out of the output under test is the one assertion in this file
        // that would agree with any implementation. The first version of this
        // literal said 93 and 016, and the engine was the one that was right.
        "8=FIX.4.4|9=88|35=j|34=7|49=US|52=20260902-10:00:00.123|56=ALPHA|\
         45=11|58=unknown security|372=D|380=2|10=155|",
        "header first, then body tags ascending — the dictionary chooses, not \
         the call site"
    );
}

/// A handler may keep writing after the four.
///
/// **This is what says `business_reject` is a convenience over `message` and
/// not a second path.** If it were its own encoder, a field added afterwards
/// would have nowhere to go.
#[test]
fn a_business_reject_is_an_ordinary_message_and_takes_more_fields() {
    let mut out = [0u8; 512];
    let mut m = reply_for(&mut out).business_reject(b"11", b"D", 2, b"no");
    let answer = m.field(379, b"CLIENT-1").send();

    let Answer::Sent(range) = answer else {
        panic!("it should have been sent: {answer:?}");
    };
    let wire = show(&out[range]);
    assert!(
        wire.contains("|379=CLIENT-1|"),
        "the extra field is there: {wire}"
    );
    assert!(wire.contains("|372=D|"), "and the four still are: {wire}");
}

/// The session's tags are still not the handler's to write.
#[test]
fn a_business_reject_cannot_restate_a_session_owned_tag() {
    let mut out = [0u8; 512];
    let mut m = reply_for(&mut out).business_reject(b"11", b"D", 2, b"no");
    let answer = m.field(34, b"999").send();

    let Answer::Sent(range) = answer else {
        panic!("it should have been sent: {answer:?}");
    };
    let wire = show(&out[range]);
    assert!(wire.contains("|34=7|"), "the session's number: {wire}");
    assert!(!wire.contains("|34=999|"), "and only that one: {wire}");
}

// ---------------------------------------------------------------------------
// `369` through the `library` seam.
//
// **This section exists because a reversal did nothing.** `[measured
// 2026-09-06]` deleting the four lines in `Reply::message` that write `369`
// left `cargo test --all` at 581 passed, 0 failed. ADR-0056 decision 2 calls
// the guarantee it gives above this seam "most of the point" of widening the
// `Application` trait at all — and it was the one claim in the whole change
// with no gate behind it. A reversal has three outcomes and *green* is the one
// that means the test you needed was never written.
// ---------------------------------------------------------------------------

#[test]
fn a_handler_that_says_nothing_about_369_still_sends_it() {
    // The guarantee: above this seam the field appears because `Reply` writes
    // it, not because a handler remembered. The handler below names four body
    // fields and nothing else — no `369`, no header at all.
    let mut out = [0u8; 512];
    let n = {
        let reply =
            Reply::<64, 1024>::new(b"FIX.4.4", 7, STAMP, Some(4), b"US", b"ALPHA", &mut out);
        let mut m = reply.message(b"8");
        m.field(37, b"X")
            .field(17, b"Y")
            .field(150, b"F")
            .field(39, b"2");
        match m.send() {
            Answer::Sent(r) => r,
            other => panic!("the reply must go out: {other:?}"),
        }
    };
    let wire = show(&out[n]);
    assert!(
        wire.contains("|369=4|"),
        "the library writes 369 the handler never mentioned: {wire}"
    );
    // And in the dictionary's position, not appended: `369` is a header field,
    // so it precedes every body tag. Non-negotiable 5.
    let at_369 = wire.find("369=").expect("369 is on the message");
    let at_37 = wire.find("|37=").expect("37 is on the message");
    assert!(at_369 < at_37, "369 is a header field: {wire}");
}

#[test]
fn a_session_that_does_not_report_369_sends_no_such_field() {
    // The neutral twin. `None` is `Config::with_last_processed` being off, and
    // without this the test above would pass for a `Reply` that always wrote
    // the field.
    let mut out = [0u8; 512];
    let n = {
        let reply = reply_for(&mut out);
        let mut m = reply.message(b"8");
        m.field(37, b"X")
            .field(17, b"Y")
            .field(150, b"F")
            .field(39, b"2");
        match m.send() {
            Answer::Sent(r) => r,
            other => panic!("the reply must go out: {other:?}"),
        }
    };
    let wire = show(&out[n]);
    assert!(!wire.contains("369="), "no knob, no field: {wire}");
}

#[test]
fn an_origination_carries_no_369() {
    // The third case, and it is not the same as the second: here the session
    // has judged nothing, so there is no *last processed* to report even with
    // the knob on. `Reply::originate` cannot be handed one.
    let mut out = [0u8; 512];
    let n = {
        let reply = Reply::<64, 1024>::originate(b"FIX.4.4", b"US", b"ALPHA", &mut out);
        let mut m = reply.message(b"B");
        m.field(148, b"hello").field(33, b"0");
        match m.send() {
            Answer::Sent(r) => r,
            other => panic!("the origination must go out: {other:?}"),
        }
    };
    let wire = show(&out[n]);
    assert!(
        !wire.contains("369="),
        "an origination reports nothing: {wire}"
    );
    assert!(
        !wire.contains("|34="),
        "and the session writes 34= later: {wire}"
    );
}
