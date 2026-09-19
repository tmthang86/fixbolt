//! The four FIXT 1.1 rules of ADR-0080 decision 3, one test each, plus the
//! FIX 4.4 twin that proves each one is scoped to the `BeginString`.
//!
//! `tests/score_fixt.rs` runs the corpora; this file holds the cases the
//! corpora cannot see. Two of the four have **no oracle at all** and ADR-0080
//! says so in its own text — a `1137` that differs from ours, and a `1128`
//! outside the FIX 5.0 family. These are the tests that keep those two
//! decisions from drifting, and each one names the ADR rather than restating
//! its reasoning.
#![cfg(feature = "fix50sp2")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source — see `tests/score.rs`'s note.
#![allow(clippy::indexing_slicing)]

use fixbolt_codec::TagValue;
use fixbolt_conformance::script::{FIXED_TIME_IN, FIXED_TIME_MILLIS};
use fixbolt_dict::{Fix44, Fixt11Fix50Sp2Tables};
use fixbolt_session::{Acceptor, Config, DropReason, Session};

/// The FIXT 1.1 / FIX 5.0 SP2 encoding, as `tests/score_fixt.rs` spells it.
type Fixt = TagValue<Fixt11Fix50Sp2Tables, 256>;
/// FIX 4.4, for the twin that proves each rule is scoped to the `BeginString`.
type Fix44Tv = TagValue<Fix44, 256>;

/// What the corpus's acceptor is: ISLD, speaking FIX 5.0 SP2 to TW50SP2.
fn fixt_acceptor() -> Session<Fixt, Acceptor> {
    Session::new(Config::acceptor_fixt(
        b"FIXT.1.1",
        b"ISLD",
        b"TW50SP2",
        b"9",
    ))
}

/// One message, with `9=` and `10=` computed, `<T>` replaced by the instant the
/// harness ticks to, and `|` for SOH.
fn msg(begin: &str, body: &str) -> Vec<u8> {
    let body = body.replace("<T>", FIXED_TIME_IN).replace('|', "\u{1}");
    let mut m = format!("8={begin}\u{1}9={}\u{1}", body.len()).into_bytes();
    m.extend_from_slice(body.as_bytes());
    let sum: u32 = m.iter().map(|c| u32::from(*c)).sum();
    m.extend_from_slice(format!("10={:03}\u{1}", sum % 256).as_bytes());
    m
}

/// Everything the session put on the wire, as one readable string.
fn readable(out: &[Vec<u8>]) -> String {
    out.iter()
        .map(|m| String::from_utf8_lossy(m).replace('\u{1}', "|"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// **A FIXT `Logon` without `1137` is dropped, and not one byte goes out.**
///
/// `1d_InvalidLogonNoDefaultApplVerID.def` is four lines and its last is
/// `eDISCONNECT` with **no `E` line in front of it**. The assertion is on the
/// output being *empty*, not on a `Logout` being absent: a `Reject`, a
/// `Logout`, or a `Logon` followed by a drop would each satisfy "no Logout"
/// and each is a different engine.
#[test]
fn a_logon_without_1137_is_dropped_and_nothing_is_sent() {
    let mut s = fixt_acceptor();
    let mut out: Vec<Vec<u8>> = Vec::new();
    s.connect(|b: &[u8]| out.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| out.push(b.to_vec()));
    let logon = msg(
        "FIXT.1.1",
        "35=A|34=1|49=TW50SP2|52=<T>|56=ISLD|98=0|108=30|",
    );
    let link = s.received(&logon, |b: &[u8]| out.push(b.to_vec()));

    assert_eq!(link, fixbolt_session::Link::Dropped);
    assert_eq!(
        s.last_drop_reason(),
        Some(DropReason::LogonWithoutDefaultApplVerId),
        "the reason must name the missing field, not the generic incomplete Logon"
    );
    assert!(
        out.is_empty(),
        "nothing at all may go out; the engine sent:\n{}",
        readable(&out)
    );
}

/// **A `1137` other than ours is accepted**, and readable afterwards.
///
/// ADR-0080 decision 3: SP2's tables are a superset of SP0's and SP1's and no
/// `.def` in the three corpora sends a mismatch, so a refusal here would be a
/// rule with no test. This is the test that stops one being added by accident,
/// and `Session::peer_default_appl_ver_id` is how an operator sees the
/// disagreement the session chose not to act on.
#[test]
fn a_default_appl_ver_id_that_differs_from_ours_still_logs_on() {
    let mut s = fixt_acceptor();
    let mut out: Vec<Vec<u8>> = Vec::new();
    s.connect(|b: &[u8]| out.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| out.push(b.to_vec()));
    // Ours is `9` (FIX 5.0 SP2); theirs says `7` (FIX 5.0).
    let logon = msg(
        "FIXT.1.1",
        "35=A|34=1|49=TW50SP2|52=<T>|56=ISLD|98=0|108=30|1137=7|",
    );
    let link = s.received(&logon, |b: &[u8]| out.push(b.to_vec()));

    assert_eq!(link, fixbolt_session::Link::Up);
    assert!(s.is_logged_on(), "a mismatch is accepted, not refused");
    assert_eq!(s.peer_default_appl_ver_id(), Some(&b"7"[..]));
    let wire = readable(&out);
    assert!(
        wire.contains("|1137=9|"),
        "the reply states what THIS end speaks, it does not echo theirs:\n{wire}"
    );
}

/// **`1128` naming a FIX 5.0 family version is parsed and ignored.**
///
/// The neutral twin of the test below. A branch that refused nothing would pass
/// that one and fail this one, which is the whole reason a rule that *accepts*
/// gets a test of its own.
#[test]
fn an_appl_ver_id_in_the_fix_50_family_is_validated_normally() {
    let (out, next_in) = after_an_order_carrying("1128=9");
    let out = readable(&out);
    assert!(
        !out.contains("|35=3|"),
        "`1128=9` is this session's own version and must not be refused:\n{out}"
    );
    // Silence alone would pass the line above even if the order had been
    // dropped on the floor, so the count is what says it was *accepted*: the
    // Logon was `34=1` and the order `34=2`, so the session now wants 3.
    assert_eq!(next_in, 3, "the order was taken, not merely not-rejected");
}

/// **`1128` naming FIX 4.2 is `Reject 373=5` with `371=1128`.**
///
/// ADR-0080 decision 3, and one of its two behaviours with no oracle: no `.def`
/// in the three corpora carries `1128` at all. `0`–`6` name FIX 2.7 through
/// FIX 4.4, which a FIXT session cannot be speaking; `text.rs` gains no new
/// text because *Value is incorrect* is already the right sentence.
#[test]
fn an_appl_ver_id_outside_the_fix_50_family_is_rejected() {
    let out = readable(&after_an_order_carrying("1128=4").0);
    assert!(out.contains("|35=3|"), "expected a Reject:\n{out}");
    assert!(out.contains("|371=1128|"), "naming the field:\n{out}");
    assert!(out.contains("|373=5|"), "with `Value is incorrect`:\n{out}");
}

/// Log on, then send the corpus's own `NewOrderSingle` with one extra header
/// field, and return everything that went out afterwards, with the `34=` the
/// session wants next.
fn after_an_order_carrying(extra: &str) -> (Vec<Vec<u8>>, u32) {
    let mut s = fixt_acceptor();
    let mut sink: Vec<Vec<u8>> = Vec::new();
    s.connect(|b: &[u8]| sink.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| sink.push(b.to_vec()));
    let logon = msg(
        "FIXT.1.1",
        "35=A|34=1|49=TW50SP2|52=<T>|56=ISLD|98=0|108=30|1137=9|",
    );
    s.received(&logon, |b: &[u8]| sink.push(b.to_vec()));
    sink.clear();

    // `8_OnlyApplicationMessages.def`'s order, which the corpus accepts, plus
    // `1128=`. It sits among the header fields because `1128` is a FIXT header
    // field and `373=14` refuses a header field after a body one.
    let order = msg(
        "FIXT.1.1",
        &format!(
            "35=D|34=2|49=TW50SP2|52=<T>|56=ISLD|{extra}|11=ID|21=3|40=1|54=1|55=INTC|60=<T>|"
        ),
    );
    s.received(&order, |b: &[u8]| sink.push(b.to_vec()));
    let next_in = s.next_in();
    (sink, next_in)
}

/// **A FIX 4.4 session emits no `1137`** — the rule is the `BeginString`'s.
///
/// The slot is declared on the `Logon` skeleton for every encoding
/// (`out::Outbound::new`, non-negotiable 5: the dictionary places it, not a
/// call site). An unset slot is not written, and this is what says so on the
/// wire rather than in the comment.
#[test]
fn a_fix_44_session_emits_no_1137() {
    let mut s: Session<Fix44Tv, Acceptor> =
        Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"));
    let mut out: Vec<Vec<u8>> = Vec::new();
    s.connect(|b: &[u8]| out.push(b.to_vec()));
    s.tick(FIXED_TIME_MILLIS, |b: &[u8]| out.push(b.to_vec()));
    let logon = msg("FIX.4.4", "35=A|34=1|49=TW44|52=<T>|56=ISLD|98=0|108=30|");
    s.received(&logon, |b: &[u8]| out.push(b.to_vec()));

    let wire = readable(&out);
    assert!(s.is_logged_on(), "the FIX 4.4 Logon still works:\n{wire}");
    assert!(
        !wire.contains("1137"),
        "a FIX 4.4 Logon neither requires nor emits `1137`:\n{wire}"
    );
    // And the rule that refuses a Logon without it is FIXT's alone: this one
    // carried no `1137` and was answered rather than dropped.
    assert_eq!(s.last_drop_reason(), None);
}
