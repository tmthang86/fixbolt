//! The 50 mirrorable definitions, this engine playing the **initiator**.
//!
//! The acceptor's gate (`tests/score.rs`) is the primary one and it is met.
//! This is the secondary gate `ADR-0004` decision 6 defines and `ADR-0006`
//! corrects: the same files read from the other side, with `I` lines as this
//! engine's output.
//!
//! # This gate was pinned at zero, and a pinned gate reports nothing
//!
//! `[measured 2026-09-02]` it asserted `passed == 0` from 2026-08-30 to
//! 2026-09-02. While it stood there, an initiator that answered a `Logon` with
//! a `Logon` — a defect that made the role unusable against any real
//! counterparty — passed here, passed `tests/score.rs` at 59 / 59, and passed
//! 430 other tests. It was found by `scripts/interop.sh` on that script's first
//! run. `docs/reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md`.
//!
//! It now reads **2 / 50** and can fall, which is the whole difference.
//!
//! **What this gate cannot do is check its own reading.** Mirroring is this
//! project's interpretation of a suite written for the other direction; a wrong
//! interpretation stays green here. Interop against `libquickfix` — step 4 of
//! the plan — is what stands in front of that.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::runner::{Conn, Input, Intent, Link, SessionUnderTest, run_mirrored};
use fixbolt_conformance::script::{Kind, Scenario};
use fixbolt_engine::journal::Store;
use fixbolt_session::{Config, Initiator, Session};

fn link(l: fixbolt_session::Link) -> Link {
    match l {
        fixbolt_session::Link::Up => Link::Up,
        fixbolt_session::Link::Dropped => Link::Dropped,
    }
}

/// One connection's state machine, plus the identity this end plays.
///
/// Mirrored, the CompIDs swap: the files are written from `ISLD`'s side and
/// this engine is `TW44`.
struct Adapter {
    conns: Vec<(Conn, Session<Initiator, 256>, Store)>,
    heart_bt_int: u32,
}

impl Adapter {
    fn new(s: &Scenario) -> Self {
        Self {
            conns: Vec::new(),
            heart_bt_int: proposed_heart_bt_int(s),
        }
    }

    fn at(&mut self, conn: Conn) -> usize {
        if let Some(i) = self.conns.iter().position(|(c, _, _)| *c == conn) {
            return i;
        }
        self.conns.push((
            conn,
            Session::new(
                Config::initiator(b"FIX.4.4", b"TW44", b"ISLD")
                    .with_heart_bt_int(self.heart_bt_int),
            ),
            Store::new(),
        ));
        self.conns.len() - 1
    }
}

/// The `108=` this file's own Logon asks for.
///
/// Mirrored, that Logon is **this engine's** output, so the number is a
/// configuration input rather than something read off the wire. 13 of the files
/// ask for 2 and 2 for 6; the rest for 30.
fn proposed_heart_bt_int(s: &Scenario) -> u32 {
    for step in &s.steps {
        if let Kind::Expect(m) = &step.kind
            && field(&m.wire, 35) == Some(b"A")
            && let Some(v) = field(&m.wire, 108)
            && let Ok(secs) = core::str::from_utf8(v).unwrap_or("").parse::<u32>()
        {
            return secs;
        }
    }
    30
}

fn field(wire: &[u8], tag: u32) -> Option<&[u8]> {
    let mut needle = [0u8; 12];
    let mut n = 0;
    let mut t = tag;
    let mut digits = [0u8; 10];
    let mut d = 0;
    if t == 0 {
        digits[0] = b'0';
        d = 1;
    }
    while t > 0 {
        digits[d] = b'0' + u8::try_from(t % 10).unwrap_or(0);
        d += 1;
        t /= 10;
    }
    for i in 0..d {
        needle[n] = digits[d - 1 - i];
        n += 1;
    }
    needle[n] = b'=';
    n += 1;
    let needle = &needle[..n];

    let mut at = 0;
    while at < wire.len() {
        let start = at;
        let end = wire[at..].iter().position(|b| *b == 1)? + at;
        if wire[start..end].starts_with(needle) {
            return Some(&wire[start + needle.len()..end]);
        }
        at = end + 1;
    }
    None
}

impl SessionUnderTest for Adapter {
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, mut emit: F) -> Link {
        let i = self.at(conn);
        let (_, s, journal) = &mut self.conns[i];
        // **The harness plays the operator, and this is the whole of what that
        // means.** Each arm is one of the six calls a real application makes;
        // none of them takes the expected message. What the runner hands over
        // is the value an operator would have chosen, and the session builds
        // the message from its own `Template` — so the header, the ordering and
        // the sequence number are still the code under test.
        //
        // A refused call (`false`) emits nothing, and the file then fails on
        // `NoOutput` for that line, which is the honest reading: the session
        // declined and the corpus expected a message.
        if let Input::Originate(intent) = input {
            let sent = match intent {
                Intent::Heartbeat => s.send_heartbeat(&mut emit),
                Intent::TestRequest(id) => s.send_test_request(id, &mut emit),
                Intent::ResendRequest(from, to) => s.send_resend_request(from, to, &mut emit),
                Intent::SequenceReset { new_seq, gap_fill } => {
                    // `123=Y` is a gap fill and stands in for messages this end
                    // will not replay; `123=N` is the honest reset. The session
                    // owns the first as an answer to a `ResendRequest`, so only
                    // the second is something an operator orders.
                    !gap_fill && s.send_sequence_reset(new_seq, &mut emit)
                }
                Intent::Logout(text) => {
                    s.begin_logout(text, &mut emit) == fixbolt_session::Link::Up
                }
                Intent::Application(msg) => {
                    s.send_application(msg, journal, &mut emit) == fixbolt_session::Link::Up
                }
            };
            let _ = sent;
            return Link::Up;
        }
        link(match input {
            Input::Connect => s.connect(emit),
            Input::Disconnect => s.disconnect(emit),
            Input::Bytes(b) => s.received(b, emit),
            Input::Tick(ms) => s.tick(ms, emit),
            Input::Originate(_) => unreachable!("handled above"),
        })
    }
}

/// The mirrored corpus, with the harness playing the operator.
///
/// # What the number means, and what the second assertion is for
///
/// `[measured 2026-09-02]` **10 / 50**, up from a score pinned at 0 since
/// 2026-08-30. Three things moved it, and the third is the point of having a
/// gate that can fall at all:
///
/// 1. **The harness can now originate** — `Input::Originate(Intent)`, fed only
///    to a mirrored scenario. 46 of these 50 files need a message nothing on
///    the wire asks for and no clock produces, and a pure state machine cannot
///    invent one.
/// 2. **The loader was feeding messages no session would accept.** An `E` line
///    carries `52=00000000-00:00:00.000`, because `Comparator.rb` matches that
///    tag by shape and it never had to be real. Mirrored, an `E` line is an
///    *input*, and a timestamp in the year 0 is 2 026 years of clock skew — so
///    the session refused the counterparty's Logon and dropped, and every later
///    line of every file read *"expected a message, got silence"*. See
///    `fixbolt_conformance::script::make_receivable`. That took it to **2**.
/// 3. **It then found two real defects, and 2 became 10.** A session that said
///    goodbye first answered the counterparty's acknowledgement with a *third*
///    `Logout`; and `begin_logout(b"")` wrote an empty `58=`. Neither was
///    visible to the acceptor corpus — an acceptor never starts a logout — and
///    `crates/session/tests/goodbye.rs` now holds both, with the pair that says
///    a goodbye we did **not** start is still answered.
///
/// **A score a harness can raise by driving harder is not a score**, so the
/// second assertion pins how much the harness drove, by `MsgType`, as exact
/// numbers rather than as a bound. Raising the first number by driving more
/// turns the second one red.
///
/// # What this gate still does not do
///
/// It does not check its own reading, and it never will — mirroring is this
/// project's interpretation of a suite written for the other direction, and its
/// ceiling has already moved 51 → 50 → 45 across two readings.
/// `scripts/interop.sh` is what stands in front of that, and it is the gate
/// phase 1 exit criterion 4 is about.
#[test]
fn the_mirrored_corpus_with_an_operator_at_the_keyboard() {
    let report = run_mirrored(Adapter::new).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.scenarios, 50, "50 mirrorable files:\n{report}");

    assert_eq!(
        report.passed_files,
        [
            "13b_UnsolicitedLogoutMessage.def",
            "1a_ValidLogonWithCorrectMsgSeqNum.def",
            "2a_MsgSeqNumCorrect.def",
            "2k_CompIDDoesNotMatchProfile.def",
            "2o_SendingTimeValueOutOfRange.def",
            "2q_MsgTypeNotValid.def",
            "4a_NoDataSentDuringHeartBtInt.def",
            "4b_ReceivedTestRequest.def",
            "AlreadyLoggedOn.def",
            "ReverseRoute.def",
        ],
        "by name, not by count — a different ten passing is a different result:\n{report}"
    );

    // **How much the harness drove.** Exact numbers, not `<=`: the whole risk
    // of an operator-driven gate is that somebody raises the score by driving
    // more, and a bound would let them.
    //
    // `app×35` is the widest and is still not a back door — it is what
    // `send_application` takes from a real application, and the session rewrites
    // the header and reorders the body through `Fix44` regardless.
    //
    // `[measured 2026-09-02]` **141, and it was 179 before the harness learned
    // not to speak over a session that had already answered.** The score was
    // `2 / 50` either way — which is exactly why this table is asserted next to
    // it. Two harnesses can reach the same number and one of them is talking
    // over the code under test.
    assert_eq!(
        report.driven,
        [
            ("0".to_owned(), 42),
            ("1".to_owned(), 21),
            ("2".to_owned(), 3),
            ("4".to_owned(), 10),
            ("5".to_owned(), 30),
            ("app".to_owned(), 35),
        ],
        "the harness originated something it did not before:\n{report}"
    );

    // The five whose first `I` line is wrong **on purpose** — a CompID that
    // does not match, a `SendingTime` 2001 years out, a `9=` 23 bytes short,
    // and one that is not a Logon at all. Mirrored, they ask this engine to
    // send those, and a correct engine cannot. That is what makes the ceiling
    // **45 and not 50**, and it is why those five are named here rather than
    // counted.
    let mut early: Vec<&str> = report
        .failures
        .iter()
        .filter(|f| f.line_no <= 4)
        .map(|f| f.file.as_str())
        .collect();
    early.dedup();
    assert_eq!(
        early,
        [
            "1c_InvalidSenderCompID.def",
            "1c_InvalidTargetCompID.def",
            "1d_InvalidLogonBadSendingTime.def",
            "1d_InvalidLogonLengthInvalid.def",
            "1e_NotLogonMessage.def",
        ],
        "the five whose Logon is deliberately wrong, and nothing else:\n{report}"
    );
}
