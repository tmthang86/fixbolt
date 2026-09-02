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

// A test binary, not a library crate: non-negotiable 7 is about what ships.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt::{Answer, Reply};

/// `52`, as the session hands it over: 21 bytes, milliseconds included.
const STAMP: &[u8] = b"20260902-10:00:00.123";

/// What the handler is answering. `49=ALPHA` is the counterparty, `56=US` is
/// this acceptor — so the reply must carry `49=US` and `56=ALPHA`.
fn reply_for(out: &mut [u8]) -> Reply<'_> {
    Reply::new(b"FIX.4.4", 7, STAMP, b"US", b"ALPHA", out)
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
