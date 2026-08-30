//! The rules the 59 definitions cannot tell apart.
//!
//! `[measured 2026-08-28]` removing the "first message must be a Logon" check
//! leaves the score at **6 / 59**. `1e_NotLogonMessage.def` sends `35=0` *and*
//! `56=DLSI`, and the TargetCompID check catches it first — so the file named
//! for that rule does not prove it. Every test here exists because a reversal
//! stayed green.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{
    FIXED_TIME_IN, FIXED_TIME_MILLIS, FIXED_TIME_OUT, Kind, scenarios, with_real_checksum,
};
use fixbolt_session::{Acceptor, Config, Link, Session};

fn acceptor() -> Session<Acceptor, 256> {
    Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
}

/// The first `I` line of a definition file, as the loader produces it.
///
/// A real corpus line rather than an invented packet — `CLAUDE.md` §7. The
/// tests below change exactly one field of it and say which.
fn first_input(file: &str) -> Vec<u8> {
    scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|s| s.file == file)
        .unwrap_or_else(|| panic!("{file} is not in the corpus"))
        .steps
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Send(m) => Some(m.wire),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{file} has no I line"))
}

/// Replace one field's value, keeping the length so `9=` stays right. The
/// checksum is recomputed.
fn swap(wire: &[u8], from: &str, to: &str) -> Vec<u8> {
    assert_eq!(from.len(), to.len(), "9= would move — use `reframe`");
    with_real_checksum(&replace(wire, from, to))
}

/// Replace one substring, length free. `9=` and `10=` are then both wrong;
/// [`reframe`] fixes them.
fn replace(wire: &[u8], from: &str, to: &str) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    let patched = s.replace(from, to);
    assert_ne!(patched, s, "{from} is not in the message");
    patched.into_bytes()
}

/// Recompute `9=` and `10=` for a message whose body has changed length.
fn reframe(wire: &[u8]) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    let after_9 = s.find("\u{1}35=").expect("35= follows the frame") + 1;
    let at_10 = s.find("\u{1}10=").map_or(s.len(), |i| i + 1);
    let body = at_10 - after_9;
    let head_end = s.find("\u{1}").expect("8= is a field") + 1;
    // The `10=0` placeholder matters: `with_real_checksum` finds the trailer
    // and replaces it, and a message without one parses as `Incomplete` — which
    // this layer treats as "wait for more", so every assertion built on it
    // passes for the wrong reason. That is how the first version of this
    // function looked green.
    let rebuilt = format!(
        "{}9={body}\u{1}{}10=0\u{1}",
        &s[..head_end],
        &s[after_9..at_10]
    );
    with_real_checksum(rebuilt.as_bytes())
}

/// Like [`feed`], but keeps what came back.
fn collect(session: &mut Session<Acceptor, 256>, wire: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    session.connect(|b| out.push(render(b)));
    session.tick(FIXED_TIME_MILLIS, |b| out.push(render(b)));
    session.received(wire, |b| out.push(render(b)));
    out
}

fn render(b: &[u8]) -> String {
    String::from_utf8_lossy(b).replace('\u{1}', "|")
}

fn feed(session: &mut Session<Acceptor, 256>, wire: &[u8]) -> (Link, usize) {
    let mut sent = 0usize;
    session.connect(|_| sent += 1);
    session.tick(fixbolt_conformance::script::FIXED_TIME_MILLIS, |_| {
        sent += 1
    });
    let link = session.received(wire, |_| sent += 1);
    (link, sent)
}

/// A real Logon: `1c_InvalidTargetCompID.def`'s line with its one wrong field
/// corrected. `98=` and `108=` are both present, as FIX 4.4 requires.
fn good_logon() -> Vec<u8> {
    swap(
        &first_input("1c_InvalidTargetCompID.def"),
        "56=DLSI",
        "56=ISLD",
    )
}

#[test]
fn a_first_message_that_is_not_a_logon_is_refused_on_its_own_merits() {
    // One field's difference from the message below, and the field is `35`.
    // The corpus cannot make this comparison: `1e_NotLogonMessage.def` sends
    // `35=0` to `56=DLSI`, so the identity check answers first and the rule the
    // file is named for is never reached.
    let wire = swap(&good_logon(), "35=A", "35=0");

    let mut session = acceptor();
    let (link, sent) = feed(&mut session, &wire);

    assert_eq!(link, Link::Dropped, "a Heartbeat is not a Logon");
    assert_eq!(sent, 0, "and the corpus expects no message before the drop");
}

#[test]
fn the_same_message_as_a_logon_is_not_refused() {
    // The other half. Without it the test above proves only that *some* rule
    // fired on this wire, not that it was the one about `35`.
    let mut session = acceptor();
    let (link, sent) = feed(&mut session, &good_logon());

    assert_eq!(link, Link::Up, "a well-formed Logon is not refused");
    assert_eq!(sent, 1, "and it is answered with exactly one Logon");
}

#[test]
fn a_logon_missing_a_required_field_is_refused() {
    // `98=` and `108=` are required in FIX 4.4 and the acceptor has to echo
    // both, so it cannot answer a Logon without them. Every Logon in the corpus
    // carries both, which is why nothing there covers this.
    for gone in ["98=0\u{1}", "108=30\u{1}"] {
        let wire = reframe(&replace(&good_logon(), gone, ""));
        let mut session = acceptor();
        let (link, sent) = feed(&mut session, &wire);
        assert_eq!(link, Link::Dropped, "a Logon without {gone} is not one");
        assert_eq!(sent, 0);
    }
}

#[test]
fn a_sending_time_the_engine_cannot_read_is_refused() {
    // `1d_InvalidLogonBadSendingTime` is 2001 years out, which the skew check
    // catches. A field that is not a timestamp at all takes a different branch,
    // and nothing in the corpus exercises it.
    let good = good_logon();

    let mut session = acceptor();
    assert_eq!(feed(&mut session, &good).0, Link::Up, "baseline");

    for bad in [
        "20260828-12:00:0X",
        "0000000A-12:00:00",
        "20260230-12:00:00",
    ] {
        let wire = swap(&good, FIXED_TIME_IN, bad);
        let mut session = acceptor();
        assert_eq!(
            feed(&mut session, &wire).0,
            Link::Dropped,
            "{bad} is not a timestamp"
        );
    }
}

#[test]
fn a_comp_id_too_long_to_hold_does_not_match_its_own_truncation() {
    // `Name<32>` fails closed. The attack it closes: configure a 33-byte
    // TargetCompID, and a counterparty that sends the **first 32 bytes of it**
    // is accepted by an engine that truncates. Nothing in the corpus has a
    // CompID longer than 4 bytes, so only this test holds the rule.
    let truncated = "X".repeat(32);
    let configured = format!("{truncated}Y"); // 33 bytes: one too many

    let wire = reframe(&replace(
        &first_input("1c_InvalidTargetCompID.def"),
        "56=DLSI",
        &format!("56={truncated}"),
    ));

    let mut fits: Session<Acceptor, 256> =
        Session::new(Config::acceptor(b"FIX.4.4", truncated.as_bytes(), b"TW44"));
    assert_eq!(
        feed(&mut fits, &wire).0,
        Link::Up,
        "a 32-byte CompID fits and must be accepted — otherwise the case below \
         proves nothing"
    );

    let mut overflows: Session<Acceptor, 256> =
        Session::new(Config::acceptor(b"FIX.4.4", configured.as_bytes(), b"TW44"));
    assert_eq!(
        feed(&mut overflows, &wire).0,
        Link::Dropped,
        "a configuration that does not fit must not match its own truncation"
    );
}

#[test]
fn the_reply_carries_the_clock_the_session_was_ticked_to() {
    // `[measured 2026-08-28]` stamping `52=` from a constant instead of from
    // the clock leaves the score at 14 / 59 and every test green: tag 52 is one
    // of the five in `fields.fmt`, so the acceptance comparator matches it by
    // **shape** and never by value. The corpus cannot see this field. Only this
    // test can.
    let mut session = acceptor();
    let out = collect(&mut session, &good_logon());

    assert_eq!(out.len(), 1, "one Logon back");
    assert!(
        out[0].contains(&format!("|52={FIXED_TIME_OUT}|")),
        "52= must be the instant of the last tick, not a constant: {}",
        out[0]
    );
    assert!(
        out[0].contains("|34=1|"),
        "and the first message is 34=1: {}",
        out[0]
    );
    assert!(out[0].contains("|49=ISLD|"), "sender is us: {}", out[0]);
    assert!(out[0].contains("|56=TW44|"), "target is them: {}", out[0]);
}

#[test]
fn the_clock_moves_and_the_next_message_says_so() {
    // The other half: if `52=` were stamped once and cached forever, the test
    // above would still pass.
    let minute = 60_000;
    let mut session = acceptor();
    session.connect(|_| ());
    session.tick(FIXED_TIME_MILLIS, |_| ());
    let mut first = Vec::new();
    session.received(&good_logon(), |b| first.push(render(b)));

    session.tick(FIXED_TIME_MILLIS + minute, |_| ());
    let logout = swap(&good_logon(), "35=A", "35=5");
    let mut second = Vec::new();
    session.received(&logout, |b| second.push(render(b)));

    assert_eq!(second.len(), 1, "a Logout is answered with one: {second:?}");
    assert!(
        first[0].contains("|52=20260828-12:00:00.000|"),
        "{}",
        first[0]
    );
    assert!(
        second[0].contains("|52=20260828-12:01:00.000|"),
        "the second message is a minute later: {}",
        second[0]
    );
}
