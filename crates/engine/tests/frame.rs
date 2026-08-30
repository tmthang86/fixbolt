//! Cutting a byte stream into messages, and the one rule that cost an hour.
//!
//! `2m_BodyLengthValueNotCorrect.def` states it in its own two comments: a `9=`
//! that promises **too few** bytes loses its own message, and one that promises
//! **too many** swallows the message after it. One rule covers both — take `9=`
//! at its word, and if the trailer is not where it says, the whole receive
//! buffer is rubbish.
//!
//! This lived in `crates/session/tests/score.rs` while `engine` did not exist.
//! Framing is L3 (`DESIGN.md` §2), and a rule that lives in a test is a rule no
//! deployment gets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nanofix_engine::frame::{Cut, Framer};

/// A well-formed message with a correct `9=` and a three-digit `10=`.
fn msg(seq: u32) -> Vec<u8> {
    let body = format!("35=0\u{1}34={seq}\u{1}49=TW44\u{1}56=ISLD\u{1}");
    let head = format!("8=FIX.4.4\u{1}9={}\u{1}", body.len());
    let mut wire = format!("{head}{body}").into_bytes();
    let sum = wire.iter().fold(0u8, |a, b| a.wrapping_add(*b));
    wire.extend_from_slice(format!("10={sum:03}\u{1}").as_bytes());
    wire
}

/// Feed `bytes` and take every message the framer can cut, rendered.
fn feed<const N: usize>(f: &mut Framer<N>, bytes: &[u8]) -> Vec<String> {
    let spare = f.spare();
    assert!(
        spare.len() >= bytes.len(),
        "the test's own buffer is too small"
    );
    spare[..bytes.len()].copy_from_slice(bytes);
    f.filled(bytes.len());

    let mut out = Vec::new();
    loop {
        match f.cut() {
            Cut::Need => break,
            Cut::Message(n) => {
                out.push(render(f.bytes(n)));
                f.take(n);
            }
            Cut::Garbage(n) => {
                out.push(format!("GARBAGE({})", render(f.bytes(n))));
                f.take(n);
            }
        }
    }
    out
}

/// Replace the real `9=` with `n`, leaving everything else alone.
///
/// Computed, not hard-coded: the body length of [`msg`] is whatever it is, and
/// a test that writes it down goes quietly wrong the day the message changes.
fn claim_body_length(wire: &[u8], n: usize) -> Vec<u8> {
    let text = String::from_utf8(wire.to_vec()).expect("ascii");
    let at = text.find("\u{1}9=").expect("a length field") + 1;
    let end = text[at..].find('\u{1}').expect("terminated") + at;
    format!("{}9={n}{}", &text[..at], &text[end..]).into_bytes()
}

fn render(b: &[u8]) -> String {
    String::from_utf8_lossy(b).replace('\u{1}', "|")
}

#[test]
fn one_message_comes_out_whole_and_then_the_buffer_is_empty() {
    let mut f: Framer<512> = Framer::new();
    let out = feed(&mut f, &msg(1));
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0].starts_with("8=FIX.4.4|9=") && out[0].ends_with('|'),
        "{out:?}"
    );
    assert_eq!(feed(&mut f, b""), Vec::<String>::new(), "nothing left over");
}

/// TCP delivers bytes, not messages: two in one read is ordinary.
#[test]
fn two_messages_in_one_read_come_out_as_two() {
    let mut f: Framer<512> = Framer::new();
    let mut both = msg(1);
    both.extend_from_slice(&msg(2));
    let out = feed(&mut f, &both);
    assert_eq!(out.len(), 2, "{out:?}");
    assert!(
        out[0].contains("|34=1|") && out[1].contains("|34=2|"),
        "{out:?}"
    );
}

/// And half a message is not a message.
#[test]
fn a_message_split_across_two_reads_waits_for_the_rest() {
    let m = msg(1);
    let (head, tail) = m.split_at(m.len() / 2);
    let mut f: Framer<512> = Framer::new();
    assert!(feed(&mut f, head).is_empty(), "half is not a message");
    let out = feed(&mut f, tail);
    assert_eq!(out.len(), 1, "and the other half completes it: {out:?}");
}

/// `9=` too short: the message loses itself, and the next read is clean.
///
/// `2m_BodyLengthValueNotCorrect.def`: *"Invalid message was ignored, and valid
/// one was processed."*
#[test]
fn a_body_length_that_is_too_short_loses_its_own_message() {
    // Four bytes short of the real body: the trailer is not where it says.
    let wrong = claim_body_length(&msg(1), 22);

    let mut f: Framer<512> = Framer::new();
    let out = feed(&mut f, &wrong);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].starts_with("GARBAGE("), "{out:?}");

    // The next read is a clean message and is unaffected.
    let out = feed(&mut f, &msg(2));
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("|34=2|"), "{out:?}");
}

/// `9=` too long: it swallows the message after it, and both are lost.
///
/// `2m`: *"it will combine with the next message and be ignored."*
#[test]
fn a_body_length_that_is_too_long_swallows_the_next_message() {
    // 60 against a real body of 26. One message is 48 bytes, so the promised
    // end lands inside the *next* one rather than past the pair — which is the
    // arithmetic `2m` relies on. A number bigger than both messages together
    // would simply wait, and waiting is right: the bytes might still come.
    let wrong = claim_body_length(&msg(1), 60);

    let mut f: Framer<512> = Framer::new();
    assert!(
        feed(&mut f, &wrong).is_empty(),
        "60 bytes have not arrived yet, so it waits"
    );

    // The next message arrives, and 99 now lands inside it.
    let out = feed(&mut f, &msg(2));
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].starts_with("GARBAGE("), "{out:?}");
    assert!(
        out[0].contains("|34=1|") && out[0].contains("|34=2|"),
        "both went together: {out:?}"
    );
}

/// 238 of the corpus's `I` lines carry the literal `10=0`. One digit.
#[test]
fn a_one_digit_checksum_is_still_a_trailer() {
    let text = String::from_utf8(msg(1)).expect("ascii");
    let at = text.find("10=").expect("a trailer");
    let short = format!("{}10=0\u{1}", &text[..at]).into_bytes();

    let mut f: Framer<512> = Framer::new();
    let out = feed(&mut f, &short);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(!out[0].starts_with("GARBAGE("), "{out:?}");
}

/// A message bigger than the buffer is dropped, and the framer recovers.
///
/// Without this the buffer fills, `spare()` is empty, and the connection is
/// wedged forever on bytes a counterparty chose — which is a denial of service
/// with one oversized `9=`.
#[test]
fn a_message_too_big_for_the_buffer_is_dropped_and_the_next_one_is_not() {
    let huge = claim_body_length(&msg(1), 999_999);

    let mut f: Framer<64> = Framer::new();
    let out = feed(&mut f, &huge);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].starts_with("GARBAGE("), "{out:?}");

    assert!(!f.spare().is_empty(), "and there is room again");
    let out = feed(&mut f, &msg(2));
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("|34=2|"), "{out:?}");
}
