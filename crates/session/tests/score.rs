//! The score. This is the gate, and it is the only one that matters.
//!
//! `CLAUDE.md` §2 non-negotiable 3: a session change that has not run the 59
//! definitions is not done. Every step of
//! `docs/plans/2026-08-28-session-layer.md` predicts a number here, and a step
//! that misses its prediction — **or beats it** — stops until the difference is
//! understood.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::runner::{Conn, Input, Link, SessionUnderTest, run};
use fixbolt_conformance::script::FIXED_TIME_MILLIS;
use fixbolt_engine::frame::{Cut, Framer};
use fixbolt_engine::journal::Store;
use fixbolt_session::{Acceptor, Config, Session};

/// `session` cannot depend on `conformance` — that is the dev-dependency
/// direction, and reversing it is a cycle. So the two crates each own a `Link`
/// and this maps between them. Two names for one idea, and the alternative is
/// worse.
fn link(l: fixbolt_session::Link) -> Link {
    match l {
        fixbolt_session::Link::Up => Link::Up,
        fixbolt_session::Link::Dropped => Link::Dropped,
    }
}

/// The orphan rule: `SessionUnderTest` belongs to `conformance` and `Session`
/// to `session`, so neither is local here. A local wrapper is the whole reason
/// this type exists — and it has grown into the smallest engine the corpus
/// insists on.
///
/// # This is standing in for `engine`, and it says so
///
/// Two things here are **not** session rules and are not in the session crate:
///
/// * **which connection owns an identity.** `1b_DuplicateIdentity.def` opens a
///   second connection with the same CompIDs and expects it dropped. A session
///   object is one connection's state machine and cannot know about another;
///   deciding between them is what an engine does. `runner.rs` anticipated this
///   — `SessionUnderTest` takes a `Conn` and one instance sees every connection.
/// * **framing.** TCP delivers bytes, not messages. `2m_BodyLengthValueNotCorrect.def`
///   is entirely about that: a `9=` that promises too few bytes loses its own
///   message, and one that promises too many **swallows the next**. See
///   [`frame`].
/// * **the application.** [`EchoApp`] is the acceptance server's own behaviour,
///   wired in through [`fixbolt_session::Application`].
///
/// When `engine` exists, the first two move into it. Until then they are here,
/// in the open, rather than smuggled into `Session`.
struct Adapter {
    conns: Vec<Wire>,
    app: EchoApp,
}

/// One connection: its state machine and the bytes that have arrived for it.
struct Wire {
    conn: Conn,
    session: Session<Acceptor, 256>,
    /// The store moved to `engine` at step 6 of its plan; the score adapter
    /// stands in for an engine, so it supplies the real one.
    journal: Store,
    rx: Framer<RX>,
}

/// `[measured]` the longest message in the corpus is 200 bytes; 4 KiB leaves
/// room for an application message an order of magnitude bigger.
const RX: usize = 4096;

impl Adapter {
    fn new() -> Self {
        Self {
            conns: Vec::new(),
            app: EchoApp::default(),
        }
    }

    fn at(&mut self, conn: Conn) -> usize {
        if let Some(i) = self.conns.iter().position(|w| w.conn == conn) {
            return i;
        }
        self.conns.push(Wire {
            conn,
            session: Session::new(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44")),
            journal: Store::new(),
            rx: Framer::new(),
        });
        self.conns.len() - 1
    }
}

impl SessionUnderTest for Adapter {
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, mut emit: F) -> Link {
        let i = self.at(conn);
        let Input::Bytes(bytes) = input else {
            let s = &mut self.conns[i].session;
            return link(match input {
                Input::Connect => s.connect(emit),
                Input::Disconnect => s.disconnect(emit),
                Input::Tick(ms) => s.tick(ms, emit),
                Input::Bytes(_) => unreachable!("handled below"),
            });
        };

        // TCP delivers bytes, not messages. `fixbolt_engine::frame` is the
        // real thing an engine uses; this adapter is standing in for an engine,
        // so it calls it rather than keeping a second copy of the rule.
        {
            let spare = self.conns[i].rx.spare();
            let n = spare.len().min(bytes.len());
            spare[..n].copy_from_slice(&bytes[..n]);
            self.conns[i].rx.filled(n);
        }

        let mut result = Link::Up;
        loop {
            let taken = match self.conns[i].rx.cut() {
                Cut::Need => break,
                Cut::Message(n) => n,
                // The rubbish still goes to the session, once: it will fail to
                // parse, run its garbled rule, and drop the link only if the
                // frame claims to be a Logon — `1d_InvalidLogonLengthInvalid`.
                Cut::Garbage(n) => n,
            };

            // One identity, one connection. A Logon arriving on a second
            // connection while a first is logged on is refused by dropping it,
            // in silence — `1b_DuplicateIdentity.def` and `AlreadyLoggedOn.def`
            // both expect no reply at all on the second.
            let taken_is_logon = field(self.conns[i].rx.bytes(taken), 35) == Some(b"A");
            if taken_is_logon
                && self
                    .conns
                    .iter()
                    .enumerate()
                    .any(|(j, w)| j != i && w.session.is_logged_on())
            {
                self.conns[i].rx.take(taken);
                self.conns[i].session.disconnect(&mut emit);
                return Link::Dropped;
            }

            let app = &mut self.app;
            let w = &mut self.conns[i];
            result =
                link(
                    w.session
                        .received_with(w.rx.bytes(taken), app, &mut w.journal, &mut emit),
                );
            w.rx.take(taken);
            if result == Link::Dropped {
                break;
            }
        }
        result
    }
}

/// The acceptance server's own application.
///
/// `[2026-08-31]` shared with `crates/engine/tests/wire.rs` and
/// `tests/shard_wire.rs` rather than written out three times — it lives in
/// `fixbolt_conformance::echo::Echo`, which is where the corpus's other
/// fixtures already were. 59 / 59 before and after.
#[derive(Default)]
struct EchoApp(fixbolt_conformance::echo::Echo);

impl fixbolt_session::Application for EchoApp {
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<std::ops::Range<usize>> {
        self.0.reply(msg, seq, stamp, out)
    }
}

/// The acceptor the corpus talks to: it is ISLD and its counterparty is TW44.
fn acceptor() -> Adapter {
    Adapter::new()
}

/// The two crates agree on what time it is.
///
/// `conformance` states the corpus's instant as a number because the runner
/// needs one to tick with, and it has no timestamp parser to check it against.
/// `session` has the parser. Neither crate can prove this alone; this is the
/// only place that sees both.
#[test]
fn the_harness_clock_and_the_corpus_agree() {
    use fixbolt_conformance::script::{FIXED_TIME_IN, FIXED_TIME_MILLIS, FIXED_TIME_OUT};

    assert_eq!(
        fixbolt_session::clock::parse_utc(FIXED_TIME_IN.as_bytes()),
        Some(FIXED_TIME_MILLIS),
        "the runner would tick to an instant the corpus never writes"
    );
    assert_eq!(
        fixbolt_session::clock::parse_utc(FIXED_TIME_OUT.as_bytes()),
        Some(FIXED_TIME_MILLIS),
        "the two widths must name the same instant"
    );
}

/// `[measured 2026-08-29]` **55 / 59** — step 6a, and the plan predicted 52.
///
/// **Three more than predicted, and the reason is a real one.** The split
/// between 6a and 6b was drawn from the set of `35=` values each file expects
/// back, and that set cannot tell a message this session *echoes* from one it
/// *replays*: both are `35=D`. `3b_InvalidChecksum`, `2d_GarbledMessage` and
/// `3c_GarbledMessage` all look like they need a store of sent messages and
/// none of them does — in all three it is the **counterparty** that resends,
/// and this end only has to ask, ignore what it could not read, and echo what
/// it could.
///
/// The four that remain all replay something this session sent.
#[test]
fn step_six_b_replays_what_it_sent_and_scores_fifty_nine() {
    let report = run(|_| acceptor()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.passed, 59,
        "step 6a: the plan predicted 52 and the difference is explained above:\n{report}"
    );
    assert_eq!(
        report.passed_files,
        vec![
            "10_MsgSeqNumEqual.def",
            "10_MsgSeqNumGreater.def",
            "10_MsgSeqNumLess.def",
            "11a_NewSeqNoGreater.def",
            "11b_NewSeqNoEqual.def",
            "11c_NewSeqNoLess.def",
            "13b_UnsolicitedLogoutMessage.def",
            "14a_BadField.def",
            "14b_RequiredFieldMissing.def",
            "14c_TagNotDefinedForMsgType.def",
            "14d_TagSpecifiedWithoutValue.def",
            "14e_IncorrectEnumValue.def",
            "14f_IncorrectDataFormat.def",
            "14g_HeaderBodyTrailerFieldsOutOfOrder.def",
            "14h_RepeatedTag.def",
            "14i_RepeatingGroupCountNotEqual.def",
            "15_HeaderAndBodyFieldsOrderedDifferently.def",
            "19a_PossResendMessageThatHAsAlreadyBeenSent.def",
            "19b_PossResendMessageThatHasNotBeenSent.def",
            "1a_ValidLogonMsgSeqNumTooHigh.def",
            "1a_ValidLogonWithCorrectMsgSeqNum.def",
            "1b_DuplicateIdentity.def",
            "1c_InvalidSenderCompID.def",
            "1c_InvalidTargetCompID.def",
            "1d_InvalidLogonBadSendingTime.def",
            "1d_InvalidLogonLengthInvalid.def",
            "1d_InvalidLogonWrongBeginString.def",
            "1e_NotLogonMessage.def",
            "20_SimultaneousResendRequest.def",
            "21_RepeatingGroupSpecifierWithValueOfZero.def",
            "2a_MsgSeqNumCorrect.def",
            "2b_MsgSeqNumTooHigh.def",
            "2c_MsgSeqNumTooLow.def",
            "2d_GarbledMessage.def",
            "2e_PossDupAlreadyReceived.def",
            "2e_PossDupNotReceived.def",
            "2f_PossDupOrigSendingTimeTooHigh.def",
            "2g_PossDupNoOrigSendingTime.def",
            "2i_BeginStringValueUnexpected.def",
            "2k_CompIDDoesNotMatchProfile.def",
            "2m_BodyLengthValueNotCorrect.def",
            "2o_SendingTimeValueOutOfRange.def",
            "2q_MsgTypeNotValid.def",
            "2r_UnregisteredMsgType.def",
            "2t_FirstThreeFieldsOutOfOrder.def",
            "3b_InvalidChecksum.def",
            "3c_GarbledMessage.def",
            "4a_NoDataSentDuringHeartBtInt.def",
            "4b_ReceivedTestRequest.def",
            "6_SendTestRequest.def",
            "7_ReceiveRejectMessage.def",
            "8_AdminAndApplicationMessages.def",
            "8_OnlyAdminMessages.def",
            "8_OnlyApplicationMessages.def",
            "AlreadyLoggedOn.def",
            "RejectResentMessage.def",
            "ReverseRoute.def",
            "ReverseRouteWithEmptyRoutingTags.def",
            "SessionReset.def",
        ],
        "and these are the fifty-nine, named"
    );
}

/// Every `373` code the corpus asks for is actually reached.
///
/// Not implied by the count: `14a` alone carries four cases and a session that
/// answered every one of them with the same code would still pass the file, so
/// long as the code happened to be `0`. This walks the corpus's own `E` lines
/// and checks each distinct `373` value is produced somewhere.
#[test]
fn all_twelve_session_reject_reasons_are_produced() {
    use fixbolt_conformance::script::{Kind, scenarios};

    let mut wanted: Vec<u32> = Vec::new();
    for s in scenarios().unwrap_or_else(|e| panic!("{e}")) {
        for step in &s.steps {
            if let Kind::Expect(m) = &step.kind
                && let Some(code) = field(&m.wire, 373).and_then(|v| std::str::from_utf8(v).ok())
                && let Ok(n) = code.parse::<u32>()
                && !wanted.contains(&n)
            {
                wanted.push(n);
            }
        }
    }
    wanted.sort_unstable();
    assert_eq!(
        wanted,
        vec![0, 1, 2, 4, 5, 6, 9, 10, 11, 13, 14, 16],
        "the twelve 373 codes the corpus asks for"
    );

    let mut produced: Vec<u32> = Vec::new();
    for s in scenarios().unwrap_or_else(|e| panic!("{e}")) {
        let mut session = acceptor();
        let mut seen: Vec<u32> = Vec::new();
        for step in &s.steps {
            let conn = Conn(step.session.unwrap_or(1));
            let mut collect = |b: &[u8]| {
                if let Some(v) = field(b, 373)
                    && let Ok(n) = std::str::from_utf8(v).unwrap_or("x").parse::<u32>()
                {
                    seen.push(n);
                }
            };
            match &step.kind {
                Kind::Connect => {
                    session.step(conn, Input::Connect, &mut collect);
                    session.step(conn, Input::Tick(FIXED_TIME_MILLIS), &mut collect);
                }
                Kind::Disconnect => {
                    session.step(conn, Input::Disconnect, &mut collect);
                }
                Kind::Send(m) => {
                    session.step(conn, Input::Tick(FIXED_TIME_MILLIS), &mut collect);
                    session.step(conn, Input::Bytes(&m.wire), &mut collect);
                }
                Kind::Expect(_) | Kind::ExpectDisconnect => {}
            }
        }
        for n in seen {
            if !produced.contains(&n) {
                produced.push(n);
            }
        }
    }
    produced.sort_unstable();
    assert_eq!(
        produced, wanted,
        "every 373 code the corpus asks for must actually be produced"
    );
}

/// The value of one field, by tag.
fn field(wire: &[u8], tag: u32) -> Option<&[u8]> {
    let needle = format!("\u{1}{tag}=");
    let at = wire
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())?
        + needle.len();
    let end = wire[at..].iter().position(|&b| b == 1)? + at;
    Some(&wire[at..end])
}

/// The step-1 six are still in the fifty-five.
///
/// Not implied by the count: each step adds files and could lose one to a rule
/// that now fires earlier. Step 2 did exactly that to two files, and only a
/// named list caught it.
#[test]
fn the_step_one_six_are_still_there() {
    let report = run(|_| acceptor()).unwrap_or_else(|e| panic!("{e}"));
    for f in [
        "1c_InvalidSenderCompID.def",
        "1c_InvalidTargetCompID.def",
        "1d_InvalidLogonBadSendingTime.def",
        "1d_InvalidLogonLengthInvalid.def",
        "1d_InvalidLogonWrongBeginString.def",
        "1e_NotLogonMessage.def",
    ] {
        assert!(
            report.passed_files.iter().any(|p| p == f),
            "{f} passed at step 1 and does not now:\n{report}"
        );
    }
}
