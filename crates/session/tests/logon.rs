//! The rules the 59 definitions cannot tell apart.
//!
//! `[measured 2026-08-28]` removing the "first message must be a Logon" check
//! leaves the score at **6 / 59**. `1e_NotLogonMessage.def` sends `35=0` *and*
//! `56=DLSI`, and the TargetCompID check catches it first — so the file named
//! for that rule does not prove it. Every test here exists because a reversal
//! stayed green.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: non-negotiable 7 is about `crates/*/src`, and
// `scripts/check-indexing-debt.sh` counts nothing outside it. An index that
// panics in a test is a failing test, which is what a test is for.
#![allow(clippy::indexing_slicing)]

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

// ---------------------------------------------------------------------------
// `ResetOnLogon` / `ResetOnLogout` / `ResetOnDisconnect`
//
// Step 1 of `plans/2026-09-04-settings-for-both-roles.md`, written to be red.
//
// **Why these are a `Config` field and not `new` versus `resume`.** Choosing
// between `Session::new` and `Session::resume` says *what the journal still
// has*; a reset policy says *what this session wants to happen next time*. The
// two answer different questions and a desk sets the second one in a file, so
// collapsing them would make `ResetOnLogon=Y` unrepresentable for exactly the
// session that needs it: one that was resumed.
//
// The default is **exactly neutral**, the same promise `Schedule::always()`
// makes: the 59 acceptance definitions run under it and none of them says
// `ResetOn*`.
// ---------------------------------------------------------------------------

#[test]
fn the_default_reset_policy_leaves_a_resumed_session_counting() {
    // The neutral half, and it must be red for the right reason if `connect`
    // ever starts resetting a resumed session by itself.
    let mut session: Session<Acceptor, 256> =
        Session::resume(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"), 500, 400);
    session.connect(|_| ());

    assert_eq!(
        session.next_out(),
        500,
        "a resumed session keeps counting out"
    );
    assert_eq!(session.next_in(), 400, "and keeps counting in");
}

#[test]
fn reset_on_logon_restarts_a_resumed_sessions_numbers() {
    let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
        .with_reset(fixbolt_session::ResetPolicy::new().on_logon());
    let mut session: Session<Acceptor, 256> = Session::resume(cfg, 500, 400);
    session.connect(|_| ());

    assert_eq!(
        session.next_out(),
        1,
        "ResetOnLogon restarts the outbound count on a resumed session"
    );
    assert_eq!(session.next_in(), 1, "and the inbound one");
}

#[test]
fn reset_on_disconnect_restarts_the_numbers() {
    let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
        .with_reset(fixbolt_session::ResetPolicy::new().on_disconnect());
    let mut session: Session<Acceptor, 256> = Session::new(cfg);
    session.connect(|_| ());
    session.tick(FIXED_TIME_MILLIS, |_| ());
    session.received(&good_logon(), |_| ());
    assert_eq!(session.next_out(), 2);

    session.disconnect(|_| ());

    assert_eq!(session.next_out(), 1, "ResetOnDisconnect restarts out");
    assert_eq!(session.next_in(), 1, "and in");
}

#[test]
fn a_disconnect_without_the_policy_keeps_the_numbers() {
    // The reversal's other half: if `disconnect` reset unconditionally, the
    // test above would pass for a reason that has nothing to do with the flag.
    let mut session = acceptor();
    session.connect(|_| ());
    session.tick(FIXED_TIME_MILLIS, |_| ());
    session.received(&good_logon(), |_| ());
    session.disconnect(|_| ());

    assert_eq!(session.next_out(), 2, "no policy, no reset");
}

// ---------------------------------------------------------------------------
// `NextExpectedMsgSeqNum (789)` — a counterparty saying which number it wants
// next, so a reconnect costs no `ResendRequest` round trip.
//
// **The corpus is blind to this field.** `[verified 2026-09-06]` `grep -rl
// '789=' vendor/quickfix/test/definitions/server/fix44/` finds **0 of 59**, so
// every case below is invented — `CLAUDE.md` §7 — and the only outside opinion
// is `scripts/interop.sh` against a real `libquickfix`.
//
// The branch structure is QuickFIX C++'s, read from `Session.cpp:198–290`
// rather than guessed: `141=Y` is applied *first*, then `789` is compared
// against the outbound count, and the retransmit runs *after* the `Logon`
// reply. There are **three** branches, not four: a reset moves the number the
// comparison reads, so `141=Y` with `789=1` lands in the equal branch on its
// own. `docs/reference/who-owns-the-outbound-header.md`.
// ---------------------------------------------------------------------------

/// A resumed acceptor whose next outbound number is `next_out`.
///
/// `next_in` is 1 so the corpus Logon's own `34=1` is the number this end is
/// waiting for — otherwise the sequence check answers before `789` is reached
/// and every assertion below would be about the wrong rule.
fn resumed(next_out: u32) -> Session<Acceptor, 256> {
    Session::resume(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"), next_out, 1)
}

/// [`good_logon`] carrying `789=<n>` in its body.
fn logon_expecting(n: u32) -> Vec<u8> {
    reframe(&replace(
        &good_logon(),
        "108=30\u{1}",
        &format!("108=30\u{1}789={n}\u{1}"),
    ))
}

#[test]
fn an_equal_next_expected_changes_nothing() {
    // The neutral case, and it is first because the two below are only
    // meaningful against it: whatever they show has to be *this* plus one
    // difference.
    let mut session = resumed(500);
    let out = collect(&mut session, &logon_expecting(500));

    assert_eq!(session.next_out(), 501, "the Logon reply spent 500");
    assert_eq!(
        out.len(),
        1,
        "789 equal to the outbound count asks for nothing: {out:?}"
    );
    assert!(out[0].contains("35=A"), "and the one message is the Logon");
}

#[test]
fn a_lower_next_expected_starts_a_resend_without_being_asked() {
    // The whole point of the field. The counterparty says "I am waiting for
    // 498"; this end has spent up to 499, so 498 and 499 go back **without a
    // `ResendRequest` ever being sent**.
    let mut session = resumed(500);
    let out = collect(&mut session, &logon_expecting(498));

    assert!(
        out.len() >= 2,
        "a low 789 must produce the Logon reply and then a replay: {out:?}"
    );
    assert!(
        out[0].contains("35=A"),
        "the Logon reply goes first: {out:?}"
    );
    // Nothing was journalled here, so the owed numbers cannot be replayed and
    // are covered by one gap fill.
    //
    // **`36=501`, not `36=500`, and the difference is deliberate.** The range
    // ends at `next_out - 1` read *after* the Logon reply spent 500, so the
    // fill covers that Logon's own number too — which is what QuickFIX C++
    // does (`Session.cpp:275`). The counterparty is not confused by it: its
    // own 789-aware path queues the out-of-order Logon instead of asking for a
    // resend, and this fill carries it past. `scripts/interop.sh` is what
    // confirms that against a real `libquickfix`; this assertion only pins the
    // shape so a change to it cannot be silent.
    assert!(
        out[1].contains("35=4") && out[1].contains("36=501"),
        "and then the gap 498..=500 is filled: {out:?}"
    );
    assert!(
        out[1].contains("34=498"),
        "the fill is numbered from where the counterparty is: {out:?}"
    );
}

#[test]
fn a_higher_next_expected_is_a_logout_with_a_reason() {
    // The counterparty is waiting for a number this end has never sent, so one
    // of the two is wrong about the session and neither can find out by
    // carrying on. QuickFIX logs out and disconnects; so does this.
    let mut session = resumed(500);
    let out = collect(&mut session, &logon_expecting(501));

    assert_eq!(
        session.last_drop_reason(),
        Some(fixbolt_session::DropReason::NextExpectedTooHigh),
        "the reason is named, not a bare socket close"
    );
    assert!(
        out.iter().any(|m| m.contains("35=5")),
        "and the counterparty is told why: {out:?}"
    );
}

#[test]
fn a_reset_is_applied_before_next_expected_is_judged() {
    // **The ordering test, and it is the sharpest one here.** `141=Y` restarts
    // both counts, so a Logon carrying `141=Y` and `789=1` is asking for
    // nothing at all. Judge `789` first and this reads as "the counterparty
    // wants 1 and I am at 500" — 499 messages replayed onto a session that
    // just reset, every one of them a number the counterparty has agreed to
    // forget.
    let wire = reframe(&replace(
        &logon_expecting(1),
        "108=30\u{1}",
        "108=30\u{1}141=Y\u{1}",
    ));

    let mut session = resumed(500);
    let out = collect(&mut session, &wire);

    assert_eq!(session.next_out(), 2, "141=Y restarted the outbound count");
    assert_eq!(out.len(), 1, "and 789=1 then asks for nothing: {out:?}");
    assert!(out[0].contains("35=A"));
}

// ---------------------------------------------------------------------------
// Sending `789=`. Off by default, because every engine surveyed defaults it off
// and a counterparty that does not expect the field answers a `Reject`.
//
// **The value differs between the two roles, and it is not a role rule.** It
// depends on whether the inbound `Logon`'s own number has been counted at the
// moment the field is written. QuickFIX C++ writes `getExpectedTargetNum()`
// when it originates (`Session.cpp:691`) and `+1` when it replies (`:713`),
// with the comment *"+1 because incoming Logon did not increment the target
// SeqNum yet"*. This engine calls `advance_past` **after** the reply is sent,
// so it is in the same position and needs the same `+1`.
// ---------------------------------------------------------------------------

#[test]
fn an_acceptor_replying_counts_the_logon_it_is_answering() {
    // `next_in` is 1, the Logon arriving is `34=1`, and the reply is built
    // before `advance_past` runs — so the number this end will want next is 2,
    // not 1. Writing `next_in` unadjusted here is the off-by-one QuickFIX's own
    // comment warns about, and it would ask the counterparty to send `34=1`
    // twice.
    let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_next_expected(true);
    let mut session: Session<Acceptor, 256> = Session::new(cfg);
    let out = collect(&mut session, &good_logon());

    assert_eq!(out.len(), 1, "one Logon reply: {out:?}");
    assert!(
        out[0].contains("|789=2|"),
        "the reply asks for the number after the Logon it just took: {out:?}"
    );
}

#[test]
fn an_acceptor_with_the_knob_off_writes_no_789() {
    // The neutral twin. Without it the test above proves only that *something*
    // put a `789` on the wire, not that the knob decides it.
    let mut session = acceptor();
    let out = collect(&mut session, &good_logon());

    assert!(
        !out[0].contains("789="),
        "off by default, and nothing writes it anyway: {out:?}"
    );
}

#[test]
fn an_initiator_opening_asks_for_the_number_it_is_actually_waiting_on() {
    // The other half of the off-by-one. Nothing has arrived, so there is no
    // inbound Logon to have counted: the number this end wants next is
    // `next_in` itself, with no adjustment.
    let cfg = Config::initiator(b"FIX.4.4", b"TW44", b"ISLD")
        .with_heart_bt_int(30)
        .with_next_expected(true);
    let mut session: Session<fixbolt_session::Initiator, 256> = Session::new(cfg);
    let mut out = Vec::new();
    session.connect(|b| out.push(render(b)));
    session.tick(FIXED_TIME_MILLIS, |b| out.push(render(b)));

    assert_eq!(out.len(), 1, "the initiator opens with one Logon: {out:?}");
    assert!(
        out[0].contains("|789=1|"),
        "a fresh initiator is waiting on 1: {out:?}"
    );
}

#[test]
fn a_resumed_initiator_asks_for_where_it_left_off() {
    // And the value is the count, not a constant — a reversal that hard-coded
    // `1` would pass the test above and fail this one.
    let cfg = Config::initiator(b"FIX.4.4", b"TW44", b"ISLD")
        .with_heart_bt_int(30)
        .with_next_expected(true);
    let mut session: Session<fixbolt_session::Initiator, 256> = Session::resume(cfg, 40, 41);
    let mut out = Vec::new();
    session.connect(|b| out.push(render(b)));
    session.tick(FIXED_TIME_MILLIS, |b| out.push(render(b)));

    assert!(
        out[0].contains("|789=41|"),
        "a resumed initiator names the number it is waiting on: {out:?}"
    );
}

#[test]
fn the_position_of_789_is_the_dictionarys_and_not_this_call_sites() {
    // Non-negotiable 5. `789` goes into the template as a slot and `Fix44`
    // decides where it lands; the acceptance comparator is positional, so a
    // hand-placed field is a latent conformance failure. `98`, `108`, `141`,
    // `789` is the dictionary's order (`spec/FIX44.xml`, Logon).
    let cfg = Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44").with_next_expected(true);
    let mut session: Session<Acceptor, 256> = Session::new(cfg);
    let out = collect(&mut session, &good_logon());

    let at_98 = out[0].find("98=").expect("the reply echoes 98");
    let at_108 = out[0].find("108=").expect("the reply echoes 108");
    let at_789 = out[0].find("789=").expect("the reply carries 789");
    assert!(
        at_98 < at_108 && at_108 < at_789,
        "98, 108, then 789, as the dictionary orders them: {out:?}"
    );
}
