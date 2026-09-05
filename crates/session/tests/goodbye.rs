//! Saying goodbye and waiting to be answered.
//!
//! Step 2 of [an-ordered-shutdown]. The pure half.
//!
//! # Why this is not `logout_now`
//!
//! `logout_now` is D10's path: the queue has filled, the counterparty has
//! stopped reading, and the right answer is to say why and cut. It returns
//! `Link::Dropped` immediately, and it should.
//!
//! An ordered shutdown is the opposite case — everything is healthy and we are
//! the ones leaving — so the link must stay up long enough to hear the reply.
//! **One function serving both is how both come to be wrong**, and these tests
//! are what say the two are still different.
//!
//! [an-ordered-shutdown]: ../../../docs/plans/2026-09-02-an-ordered-shutdown.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{FIXED_TIME_MILLIS, Kind, scenarios, with_real_checksum};
use fixbolt_session::{Acceptor, Config, DropReason, Link, Session};

fn cfg() -> Config {
    Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")
}

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
    let rebuilt = format!(
        "{}9={body}\u{1}{}10=0\u{1}",
        &patched[..head_end],
        &patched[after_9..at_10]
    );
    with_real_checksum(rebuilt.as_bytes())
}

fn logged_on() -> (Session<Acceptor, 256>, Vec<String>) {
    let mut s: Session<Acceptor, 256> = Session::new(cfg());
    let mut out = Vec::new();
    s.connect(|b| out.push(String::from_utf8_lossy(b).replace('\u{1}', "|")));
    s.tick(FIXED_TIME_MILLIS, |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"))
    });
    assert_eq!(
        s.received(&good_logon(), |b| out
            .push(String::from_utf8_lossy(b).replace('\u{1}', "|"))),
        Link::Up,
        "the premise"
    );
    (s, out)
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

/// **The difference that matters.** `begin_logout` sends the goodbye and leaves
/// the link **up**, so the caller can wait for an answer.
#[test]
fn begin_logout_sends_the_goodbye_and_keeps_the_link_up() {
    let (mut s, _) = logged_on();
    let mut sent = Vec::new();

    let link = s.begin_logout(b"going away", |b| {
        sent.push(String::from_utf8_lossy(b).replace('\u{1}', "|"))
    });

    assert_eq!(link, Link::Up, "there is still an answer to wait for");
    assert_eq!(sent.len(), 1, "exactly one message: {sent:?}");
    assert!(sent[0].contains("|35=5|"), "a Logout: {}", sent[0]);
    assert!(
        sent[0].contains("|58=going away|"),
        "carrying the reason: {}",
        sent[0]
    );
}

/// **The contrast, asserted rather than assumed.** `logout_now` on the same
/// state gives up the link at once. If these two ever converge, one of the two
/// callers is wrong and nothing else would notice.
#[test]
fn logout_now_still_gives_up_the_link_at_once() {
    let (mut s, _) = logged_on();
    let link = s.logout_now(b"slow consumer", |_| {});
    assert_eq!(
        link,
        Link::Dropped,
        "D10's path cuts, and must keep cutting"
    );
}

/// The answer arrives and the session ends, naming the counterparty.
#[test]
fn their_answer_ends_the_session() {
    let (mut s, _) = logged_on();
    assert_eq!(s.begin_logout(b"bye", |_| {}), Link::Up);

    assert_eq!(
        s.received(&their_logout(2), |_| {}),
        Link::Dropped,
        "they answered"
    );
    assert_eq!(s.last_drop_reason(), Some(DropReason::PeerLogout));
}

/// **The answer to our goodbye is not answered again.**
///
/// A `Logout` exchange is one message each way: we say it, they say it back,
/// the link goes down. A third one is wrong on the wire and QuickFIX does not
/// send it — `nextLogout` replies only when *it* did not start the exchange.
///
/// `[measured 2026-09-02]` **this engine sent the third one**, and nothing
/// could see it: the acceptor corpus never has the acceptor start a logout, so
/// every `35=5` in those 59 files is a reply that *should* be sent;
/// `their_answer_ends_the_session` above passes an `emit` of `|_| {}` and so
/// counts nothing; and `scripts/interop.sh` stops reading once it has seen the
/// counterparty's `35=5`, so the extra message left after it. It was found by
/// the mirrored corpus, on `10_MsgSeqNumEqual.def`, as *"unexpected output"*.
///
/// **Same family as the `Logon` echo** — an asymmetry the acceptor corpus
/// cannot show, because an acceptor is always the responder.
#[test]
fn the_answer_to_our_own_goodbye_is_not_answered_again() {
    let (mut s, _) = logged_on();
    let mut out: Vec<String> = Vec::new();

    assert_eq!(
        s.begin_logout(b"bye", |b| out
            .push(String::from_utf8_lossy(b).replace('\u{1}', "|"))),
        Link::Up
    );
    assert_eq!(out.len(), 1, "the premise: our goodbye went out");
    assert!(out[0].contains("|35=5|"), "and it is a Logout: {out:?}");

    assert_eq!(
        s.received(&their_logout(2), |b| out
            .push(String::from_utf8_lossy(b).replace('\u{1}', "|"))),
        Link::Dropped
    );

    assert_eq!(
        out.len(),
        1,
        "their answer is the end of the exchange, not a message to answer: {out:?}"
    );
    assert_eq!(s.last_drop_reason(), Some(DropReason::PeerLogout));
}

/// A `Logout` **we did not start** is still answered, which is the other half.
///
/// Without this the fix above could be "never reply to a Logout", and the 59
/// acceptance definitions would catch that — but only because they happen to
/// contain the case. Asserting it here means the pair is stated rather than
/// inherited.
#[test]
fn a_goodbye_we_did_not_start_is_still_answered() {
    let (mut s, _) = logged_on();
    let mut out: Vec<String> = Vec::new();

    assert_eq!(
        s.received(&their_logout(2), |b| out
            .push(String::from_utf8_lossy(b).replace('\u{1}', "|"))),
        Link::Dropped
    );

    assert_eq!(
        out.len(),
        1,
        "they said goodbye first, so we answer: {out:?}"
    );
    assert!(out[0].contains("|35=5|"), "{out:?}");
}

/// **A goodbye with no words carries no `58=`.**
///
/// `[measured 2026-09-02]` `begin_logout(b"")` wrote an empty `58=`, which is a
/// field on the wire that says nothing and a field count the corpus does not
/// expect. Found the same way: `10_MsgSeqNumEqual.def` line 13 wanted 8 fields
/// and got 9.
#[test]
fn a_goodbye_with_no_text_carries_no_text_field() {
    let (mut s, _) = logged_on();
    let mut out: Vec<String> = Vec::new();

    assert_eq!(
        s.begin_logout(b"", |b| out
            .push(String::from_utf8_lossy(b).replace('\u{1}', "|"))),
        Link::Up
    );

    assert_eq!(out.len(), 1);
    assert!(out[0].contains("|35=5|"), "{out:?}");
    assert!(
        !out[0].contains("|58="),
        "no words means no field, not an empty one: {out:?}"
    );
}

/// **A silent counterparty does not keep the link up for ever by itself** — the
/// heartbeat still runs while we wait, so even without a caller's deadline the
/// session is not immortal.
///
/// This is a floor, not the deadline: 2.4 heartbeat intervals is far longer
/// than a shutdown should take, which is why the caller owns a deadline of its
/// own.
#[test]
fn a_silent_counterparty_still_times_out_eventually() {
    let (mut s, _) = logged_on();
    assert_eq!(s.begin_logout(b"bye", |_| {}), Link::Up);

    // 108=30, and the session gives up after 2.4 intervals of silence.
    assert_eq!(
        s.tick(FIXED_TIME_MILLIS + 200_000, |_| {}),
        Link::Dropped,
        "nothing came back for long enough"
    );
    assert_eq!(s.last_drop_reason(), Some(DropReason::HeartbeatTimeout));
}

/// Asking twice sends one goodbye, not two. A shutdown that resends its own
/// `Logout` every turn would spend a sequence number per turn.
#[test]
fn asking_twice_sends_one_goodbye() {
    let (mut s, _) = logged_on();
    let mut sent = 0;
    assert_eq!(s.begin_logout(b"bye", |_| sent += 1), Link::Up);
    assert_eq!(
        s.begin_logout(b"bye", |_| sent += 1),
        Link::Dropped,
        "already waiting: there is nothing more to say"
    );
    assert_eq!(sent, 1);
}

/// A session that never logged on has nothing to say goodbye to. **FIX has no
/// `Logout` before a `Logon`**, so nothing goes out — and it still ends with a
/// reason, because a shutdown that closed sockets anonymously would show up on
/// the event stream as `EndedWithoutReason`.
#[test]
fn a_session_that_never_logged_on_is_ended_with_a_reason_and_told_nothing() {
    let mut s: Session<Acceptor, 256> = Session::new(cfg());
    s.connect(|_| {});
    s.tick(FIXED_TIME_MILLIS, |_| {});
    let mut sent = 0;
    assert_eq!(
        s.begin_logout(b"bye", |_| sent += 1),
        Link::Dropped,
        "there is nothing to wait for"
    );
    assert_eq!(sent, 0, "and nothing to say: {sent}");
    assert_eq!(s.last_drop_reason(), Some(DropReason::EngineShutdown));
}

/// **A goodbye that cannot be built must not leave a shutdown waiting** for an
/// answer to a message that was never sent.
///
/// `[measured 2026-09-02]` two wrong fixtures before this one. A `Config` whose
/// fields do not fit the templates can never log on, so it took the
/// *never-logged-on* branch above; and a 400-byte `58=` **still encodes**, so
/// the goodbye went out and the link stayed up. The boundary was **probed
/// rather than guessed**: 400 bytes sends, 1 000 does not.
#[test]
fn a_goodbye_that_cannot_be_built_does_not_leave_a_shutdown_waiting() {
    let (mut s, _) = logged_on();
    let far_too_long = vec![b'X'; 1_000];
    let mut sent = 0;
    assert_eq!(
        s.begin_logout(&far_too_long, |_| sent += 1),
        Link::Dropped,
        "nothing went out, so there is nothing to wait for"
    );
    assert_eq!(sent, 0);
    assert_eq!(s.last_drop_reason(), Some(DropReason::CannotSend));
}

/// `ResetOnLogout=Y` — step 1 of `plans/2026-09-04-settings-for-both-roles.md`.
///
/// **Here rather than in `tests/logon.rs`, and the reason is a false green that
/// happened.** This test was first written there, where the nearest thing to a
/// counterparty `Logout` is `swap(&good_logon(), "35=A", "35=5")`. That is not
/// a `Logout`: `98=` and `108=` stay on it and the session answers with a
/// `Reject (35=3)`. The assertion guarding *"the goodbye keeps the number it
/// was owed"* read `|34=2|` and **passed on the Reject**, because a Reject is
/// also the second message this session sends. [`their_logout`] already existed
/// here, with the trap written into its own doc comment.
///
/// So both halves are asserted now: the number **and** the `35=`. A number
/// alone cannot tell a goodbye from a refusal.
#[test]
fn reset_on_logout_restarts_the_numbers_only_after_the_goodbye_is_numbered() {
    let mut s: Session<Acceptor, 256> =
        Session::new(cfg().with_reset(fixbolt_session::ResetPolicy::new().on_logout()));
    s.connect(|_| ());
    s.tick(FIXED_TIME_MILLIS, |_| ());
    s.received(&good_logon(), |_| ());
    assert_eq!(s.next_out(), 2, "the Logon reply spent 34=1");

    let mut out = Vec::new();
    let link = s.received(&their_logout(2), |b| {
        out.push(String::from_utf8_lossy(b).replace('\u{1}', "|"))
    });

    assert_eq!(link, Link::Dropped, "a goodbye ends the session");
    assert_eq!(out.len(), 1, "answered with exactly one: {out:?}");
    assert!(out[0].contains("|35=5|"), "and it is a Logout: {out:?}");
    assert!(
        out[0].contains("|34=2|"),
        "the goodbye keeps the number it was owed — resetting before it is \
         written would spend 34=1 twice in one session: {out:?}"
    );
    assert_eq!(s.next_out(), 1, "and only then does the count restart");
    assert_eq!(s.next_in(), 1, "both directions");
}

/// The other half: without the policy nothing restarts.
///
/// If the reset in `end` were unconditional the test above would pass for a
/// reason that has nothing to do with the flag it is named for.
#[test]
fn a_logout_without_the_policy_keeps_the_numbers() {
    let (mut s, _) = logged_on();
    s.received(&their_logout(2), |_| ());

    assert_eq!(s.next_out(), 3, "no policy, no reset");
    assert_eq!(s.next_in(), 3, "in either direction");
}
