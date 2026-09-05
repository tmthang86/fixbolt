//! Two dictionary questions a desk is allowed to stop asking.
//!
//! Step 3 of [settings-for-both-roles], written to be red.
//!
//! QuickFIX has `AllowUnknownMsgFields` and `ValidateUserDefinedFields` because
//! real counterparties send fields that are not in the specification the two
//! ends agreed on, and a session that drops the connection over one is a
//! session that cannot trade. Both are **off by default here**, meaning both
//! checks run, which is what the 59 acceptance definitions prove.
//!
//! # Why the corpus cannot see either knob
//!
//! `14a_BadField.def` is the file that looks relevant and is not.
//! `[verified 2026-09-05]` all four of its faults are `373=0`, *Invalid tag
//! number* — a tag the dictionary does not define at all — and its last case is
//! `5000=HI` with the file's own comment beside it: *"user defined is not
//! implemented yet"*. That is QuickFIX documenting, in 2004, the state these
//! two settings exist to leave.
//!
//! `AllowUnknownMsgFields` governs a different fault: `373=2`, *Tag not defined
//! for this message type* — a tag the dictionary **does** define, on a message
//! that does not carry it. **No definition in the corpus reaches it with a knob
//! that could change the answer**, so the corpus stays 59 / 59 under every
//! setting of it, which is exactly why these tests exist.
//!
//! [settings-for-both-roles]: ../../../docs/plans/2026-09-04-settings-for-both-roles.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_session::{Acceptor, Config, DictionaryChecks, Link, Session};

fn base() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

/// Every `I` line of one definition file, in order. A real corpus line rather
/// than an invented packet — `CLAUDE.md` §7.
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

/// Replace one substring and rebuild `9=` and `10=`.
///
/// The `10=0` placeholder is not decoration: a message with no trailer parses
/// as `Incomplete`, which this layer reads as *"wait for more"*, and every
/// assertion built on one passes without the message ever being judged.
fn reframe(wire: &[u8], from: &str, to: &str) -> Vec<u8> {
    let s = String::from_utf8(wire.to_vec()).expect("ascii");
    let patched = s.replace(from, to);
    assert_ne!(patched, s, "{from} is not in the message");
    let after_9 = patched.find("\u{1}35=").expect("35= follows the frame") + 1;
    let at_10 = patched.find("\u{1}10=").map_or(patched.len(), |i| i + 1);
    let head_end = patched.find('\u{1}').expect("8= is a field") + 1;
    let body = at_10 - after_9;
    let rebuilt = format!(
        "{}9={body}\u{1}{}10=0\u{1}",
        &patched[..head_end],
        &patched[after_9..at_10]
    );
    with_real_checksum(rebuilt.as_bytes())
}

fn logged_on(cfg: Config) -> Session<Acceptor, 256> {
    let mut s: Session<Acceptor, 256> = Session::new(cfg);
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let logon = &inputs("14a_BadField.def")[0];
    assert_eq!(s.received(logon, |_| {}), Link::Up, "the premise");
    assert!(s.is_logged_on(), "the premise");
    s
}

/// `14a_BadField.def`'s last heartbeat — `5000=HI` — renumbered to arrive
/// second, so the three faults before it in the file are not needed.
fn heartbeat_with_tag_5000() -> Vec<u8> {
    reframe(&inputs("14a_BadField.def")[4], "34=5", "34=2")
}

/// A `Logout` carrying `98=` and `108=`, which FIX 4.4 defines and a `Logout`
/// does not carry. `373=2`, not `373=0`.
fn logout_with_logon_fields() -> Vec<u8> {
    let logon = &inputs("14a_BadField.def")[0];
    reframe(&reframe(logon, "35=A", "35=5"), "34=1", "34=2")
}

fn answer(s: &mut Session<Acceptor, 256>, wire: &[u8]) -> (Link, Vec<String>) {
    let mut out = Vec::new();
    let link = s.received(wire, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"));
    });
    (link, out)
}

#[test]
fn by_default_a_user_defined_tag_is_still_refused() {
    // The premise, and it is the corpus's own expectation: `14a_BadField.def`
    // sends `5000=HI` and wants `371=5000`, `373=0` back.
    let mut s = logged_on(base());
    let (link, out) = answer(&mut s, &heartbeat_with_tag_5000());

    assert_eq!(link, Link::Up, "a Reject does not end the session");
    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(out[0].contains("|35=3|"), "and it is a Reject: {out:?}");
    assert!(out[0].contains("|371=5000|"), "naming the tag: {out:?}");
    assert!(out[0].contains("|373=0|"), "invalid tag number: {out:?}");
}

#[test]
fn without_user_defined_validation_tag_5000_passes_through() {
    let cfg = base().with_validation(DictionaryChecks::new().skipping_user_defined_fields());
    let mut s = logged_on(cfg);
    let (link, out) = answer(&mut s, &heartbeat_with_tag_5000());

    assert_eq!(link, Link::Up);
    assert!(
        out.is_empty(),
        "a Heartbeat carrying only a user-defined tag is answered with nothing: {out:?}"
    );
    assert_eq!(
        s.next_in(),
        3,
        "and it counted, so the message was accepted"
    );
}

#[test]
fn without_user_defined_validation_an_undefined_tag_below_the_range_is_still_refused() {
    // **The knob is a range, not an amnesty.** `999=HI` is in the specification's
    // own range and undefined; turning off user-defined validation must not
    // start accepting it, or the setting would mean *"stop checking tags"*.
    let cfg = base().with_validation(DictionaryChecks::new().skipping_user_defined_fields());
    let mut s = logged_on(cfg);
    // Already `34=2` in the file — it is the first message after the Logon.
    let (_, out) = answer(&mut s, &inputs("14a_BadField.def")[1]);

    assert_eq!(out.len(), 1, "still one Reject: {out:?}");
    assert!(out[0].contains("|371=999|"), "naming 999: {out:?}");
    assert!(out[0].contains("|373=0|"), "invalid tag number: {out:?}");
}

#[test]
fn by_default_a_defined_tag_on_the_wrong_message_is_refused() {
    // The premise for the other knob: `98=` is a real FIX 4.4 field, and a
    // `Logout` does not carry it. `373=2`, which is a different fault from
    // everything `14a_BadField.def` tests.
    let mut s = logged_on(base());
    let (link, out) = answer(&mut s, &logout_with_logon_fields());

    assert_eq!(link, Link::Up, "a Reject does not end the session");
    assert_eq!(out.len(), 1, "one Reject: {out:?}");
    assert!(out[0].contains("|35=3|"), "and it is a Reject: {out:?}");
    assert!(out[0].contains("|371=98|"), "naming the tag: {out:?}");
    assert!(
        out[0].contains("|373=2|"),
        "tag not defined for this message type: {out:?}"
    );
}

#[test]
fn allowing_unknown_message_fields_lets_the_logout_through() {
    let cfg = base().with_validation(DictionaryChecks::new().allowing_unknown_msg_fields());
    let mut s = logged_on(cfg);
    let (link, out) = answer(&mut s, &logout_with_logon_fields());

    assert_eq!(
        link,
        Link::Dropped,
        "with the field forgiven it is read as the Logout it is"
    );
    assert_eq!(out.len(), 1, "answered with one: {out:?}");
    assert!(out[0].contains("|35=5|"), "and it is a Logout: {out:?}");
}

#[test]
fn allowing_unknown_message_fields_does_not_forgive_an_undefined_tag() {
    // The other half of the same distinction. `AllowUnknownMsgFields` is about
    // `373=2`; a tag the dictionary has never heard of is `373=0` and stays
    // refused, or the two settings would be one setting.
    let cfg = base().with_validation(DictionaryChecks::new().allowing_unknown_msg_fields());
    let mut s = logged_on(cfg);
    let (_, out) = answer(&mut s, &heartbeat_with_tag_5000());

    assert_eq!(out.len(), 1, "still one Reject: {out:?}");
    assert!(out[0].contains("|373=0|"), "invalid tag number: {out:?}");
}
