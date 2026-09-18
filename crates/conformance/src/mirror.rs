//! What each mirrored definition would need this end to **say**, file by file.
//!
//! `ADR-0006` gave the mirrored corpus a ceiling of 45 by reasoning.
//! `ADR-0076` withdraws that number: the ceiling is **the number of files whose
//! every `I` line, mirrored, is something the initiator's public API can be
//! asked to do** — a count produced by the table below, not an estimate.
//!
//! # How a row was decided
//!
//! Mirrored, an `I` line is this engine's *output*. A file is [`
//! MirrorClass::Reachable`] when every one of its `I` lines can be produced by
//! the six calls an application really makes — `connect`, `send_heartbeat`,
//! `send_test_request`, `send_resend_request`, `send_sequence_reset`,
//! `begin_logout`, `send_application` — plus the messages the session emits on
//! its own (a `Reject` for a bad inbound, a `ResendRequest` for a gap, a gap
//! fill in answer to one). Anything else names the class that refuses it, and
//! **each class names the decision that refuses it** rather than a gap.
//!
//! **A `Reachable` verdict is a claim until the file passes.** `ADR-0076`
//! *Consequences* says so: a `Reachable` file that is red is either a session
//! defect or a harness that cannot drive what the API offers, and either one is
//! its own item — never a reason to move the row.
//!
//! # What this table is not
//!
//! It is not a second opinion on the corpus. `crates/conformance/src/script.rs`
//! decides **which** files mirror (`ADR-0004` decision 6 as amended by
//! `ADR-0006`); this decides what the 50 survivors would cost. The tests beside
//! it check the table against the files' own bytes, so a row cannot drift from
//! the `.def` it describes.

use crate::script::{Kind, LoadError, Scenario, mirrors, scenarios_mirrored};

/// Why a mirrored definition is, or is not, something this end can be asked to
/// say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorClass {
    /// Every `I` line is a call the initiator's public API already takes.
    Reachable,
    /// An `I` line is a `SequenceReset` carrying `34=0` — QuickFIX's *not
    /// sequenced*.
    ///
    /// Producing it would mean the caller dictating a sequence number onto the
    /// wire, which is the byte back door `ADR-0042` decision 1 exists to
    /// refuse.
    NeedsUnsequencedReset,
    /// An `I` line is a `SequenceReset` with `123=Y`, ordered by the operator.
    ///
    /// A gap fill is the session's own answer to a `ResendRequest` and it
    /// already sends one; it is not an operator action, so
    /// `send_sequence_reset` deliberately offers only `123=N`. `ADR-0076`
    /// decision 4.
    NeedsGapFillAsAction,
    /// An `I` line carries a header field the caller cannot set on a message —
    /// `43=Y`/`122=` (a duplicate replayed on demand), `97=Y` (`PossResend`),
    /// or a `34=` the file chooses rather than the session.
    ///
    /// The header is built by the session from its own `Template`; a call site
    /// that could set these would be the same back door `ADR-0042` decision 1
    /// refuses.
    NeedsHeaderTheApiDoesNotSet,
    /// An `I` line is an admin message the API never originates — an
    /// unsolicited `Reject`, or a second `Logon` mid-connection.
    ///
    /// The session owns both: a `Reject` is an answer to something invalid, and
    /// a `Logon` belongs to `connect`. `CLAUDE.md` §2 non-negotiable 2 — the
    /// session layer is the state machine, not a message factory.
    NeedsAdminMessageTheApiDoesNotOriginate,
    /// Every field is sayable, but the file's field order is not the generated
    /// table's, and the session reorders.
    ///
    /// `CLAUDE.md` §2 non-negotiable 5 / `DESIGN.md` D3: field ordering comes
    /// from the generated tables, never from a call site. The comparator is
    /// positional, so a file that writes the fields in another order cannot be
    /// matched byte for byte by a correct engine.
    NeedsBodyOrderTheTableRefuses,
    /// An `I` line is bytes no correct engine emits: a wrong `9=`, a wrong
    /// `10=`, a missing required field, a repeated tag, a group count that
    /// lies, a `CompID` that is not this session's, a stale `52=`, or an admin
    /// message carrying a body field.
    ///
    /// These are the files whose *first* line is wrong on purpose because the
    /// suite is testing the acceptor's rejection of them. `ADR-0042` decision 1
    /// again: the only way to send them is to hand the session the bytes.
    NeedsMalformedOutput,
    /// The file needs a directive the harness has no analogue for — `ADR-0006`.
    ///
    /// Empty among these 50: the one file `ADR-0006` names,
    /// `1b_DuplicateIdentity.def`, never reaches this table because
    /// [`crate::script::mirrors`] drops it first. The variant stays so that a
    /// corpus change has somewhere honest to land.
    NeedsHarnessDirective,
    /// Every `I` line is sayable and this end says it, but the file's
    /// *script* side then falls silent where a real peer would answer, or
    /// expects a message a correct initiator does not send.
    ///
    /// Mirrored, the `E` side is the acceptor harness's own script, which
    /// never had to answer because the acceptor under test never asked —
    /// `ADR-0006`'s reasoning about `i1,DISCONNECT`, one step further.
    /// Refused by `CLAUDE.md` §2 non-negotiable 3 read the right way round:
    /// the 59 judge the session, and a session that goes quiet to match a
    /// script is a worse session. `ADR-0076` decision 2.
    NeedsAScriptThatAnswers,
    /// Not yet judged.
    ///
    /// `tests/mirror_classification.rs::no_definition_is_unclassified` forbids
    /// it — `ADR-0076` decision 2.
    Unclassified,
}

/// One mirrored definition and what it would cost.
#[derive(Debug, Clone, Copy)]
pub struct MirrorRow {
    /// The `.def` file's name, as [`crate::script::scenarios_mirrored`] reports
    /// it.
    pub file: &'static str,
    /// What the file needs, or [`MirrorClass::Reachable`].
    pub class: MirrorClass,
    /// One line: the `I` line that decided the row.
    pub why: &'static str,
}

const fn row(file: &'static str, class: MirrorClass, why: &'static str) -> MirrorRow {
    MirrorRow { file, class, why }
}

use MirrorClass::{
    NeedsAScriptThatAnswers as NoAnswer, NeedsAdminMessageTheApiDoesNotOriginate as NoAdmin,
    NeedsBodyOrderTheTableRefuses as NoOrder, NeedsGapFillAsAction as NoGapFill,
    NeedsHeaderTheApiDoesNotSet as NoHeader, NeedsMalformedOutput as NoBytes,
    NeedsUnsequencedReset as NoUnseq, Reachable,
};

/// Every mirrorable definition, classified `[measured 2026-09-18]`.
///
/// 50 rows, one per file [`crate::script::mirrors`] keeps. The tests beside
/// this module assert that this list **is** that set, that no row is
/// [`MirrorClass::Unclassified`], and that the `34=0` and `123=Y` rows really
/// contain those bytes.
pub const CLASSIFICATION: [MirrorRow; 50] = [
    row(
        "10_MsgSeqNumEqual.def",
        NoGapFill,
        "line 8 is a SequenceReset with 123=Y ordered by the operator",
    ),
    row(
        "10_MsgSeqNumGreater.def",
        NoGapFill,
        "line 8 is a SequenceReset with 123=Y ordered by the operator",
    ),
    row(
        "10_MsgSeqNumLess.def",
        NoGapFill,
        "lines 8 and 13 are SequenceResets with 123=Y, one of them also 43=Y",
    ),
    row(
        "11a_NewSeqNoGreater.def",
        NoUnseq,
        "lines 8 and 13 are SequenceResets with 34=0",
    ),
    row(
        "11b_NewSeqNoEqual.def",
        NoUnseq,
        "lines 8 and 13 are SequenceResets with 34=0",
    ),
    row(
        "11c_NewSeqNoLess.def",
        NoUnseq,
        "lines 8 and 15 are SequenceResets with 34=0",
    ),
    row(
        "13b_UnsolicitedLogoutMessage.def",
        Reachable,
        "a Logon and a begin_logout; passes",
    ),
    row(
        "14b_RequiredFieldMissing.def",
        NoBytes,
        "line 15 is a Heartbeat with no 56=, line 20 a NewOrderSingle with the header interleaved",
    ),
    row(
        "14c_TagNotDefinedForMsgType.def",
        NoBytes,
        "line 15 is a Heartbeat carrying 55=MSFT, which send_heartbeat cannot take",
    ),
    row(
        "14e_IncorrectEnumValue.def",
        NoOrder,
        "lines 14-24 write 40 before 38; the generated table orders them the other way",
    ),
    row(
        "14f_IncorrectDataFormat.def",
        NoOrder,
        "line 13 writes 40 before 38; the generated table orders them the other way",
    ),
    row(
        "14g_HeaderBodyTrailerFieldsOutOfOrder.def",
        NoOrder,
        "lines 15 and 20 put header fields after the body, which the Template never does",
    ),
    row("14h_RepeatedTag.def", NoBytes, "line 14 carries 40 twice"),
    row(
        "14i_RepeatingGroupCountNotEqual.def",
        NoBytes,
        "line 14 says 386=3 over two entries",
    ),
    row(
        "15_HeaderAndBodyFieldsOrderedDifferently.def",
        NoOrder,
        "line 20 reorders header and body on purpose; the session writes one order",
    ),
    row(
        "19a_PossResendMessageThatHAsAlreadyBeenSent.def",
        Reachable,
        "line 20 carries 97=Y, which send_application produces on the path this file drives; passes",
    ),
    row(
        "19b_PossResendMessageThatHasNotBeenSent.def",
        Reachable,
        "line 15 carries 97=Y, which send_application produces on the path this file drives; passes",
    ),
    row(
        "1a_ValidLogonMsgSeqNumTooHigh.def",
        NoAnswer,
        "the script's ResendRequest is never answered by the script; we gap-fill, the file expects our Logout",
    ),
    row(
        "1a_ValidLogonWithCorrectMsgSeqNum.def",
        Reachable,
        "a Logon and a begin_logout; passes",
    ),
    row(
        "1c_InvalidSenderCompID.def",
        NoBytes,
        "line 4 is a Logon signed 49=WT, which is not this session's identity",
    ),
    row(
        "1c_InvalidTargetCompID.def",
        NoBytes,
        "line 4 is a Logon addressed 56=DLSI, which is not this session's counterparty",
    ),
    row(
        "1d_InvalidLogonBadSendingTime.def",
        NoBytes,
        "line 4 is a Logon stamped 52=20010101-00:00:00; the session stamps now",
    ),
    row(
        "1d_InvalidLogonLengthInvalid.def",
        NoBytes,
        "line 4 declares 9=40 over a longer body",
    ),
    row(
        "1e_NotLogonMessage.def",
        NoBytes,
        "line 4 opens with a Heartbeat addressed to 56=DLSI instead of a Logon",
    ),
    row(
        "20_SimultaneousResendRequest.def",
        NoHeader,
        "lines 25-27 replay Heartbeats with 43=Y/122=, and line 15 picks 34=7 itself",
    ),
    row(
        "21_RepeatingGroupSpecifierWithValueOfZero.def",
        NoOrder,
        "line 15 writes the header 49,56,52; the Template writes 49,52,56",
    ),
    row(
        "2a_MsgSeqNumCorrect.def",
        Reachable,
        "a Logon, four Heartbeats and a Logout; passes",
    ),
    row(
        "2b_MsgSeqNumTooHigh.def",
        NoHeader,
        "line 12 jumps to 34=10 and lines 18-22 go back to 5-9, both chosen by the file",
    ),
    row(
        "2c_MsgSeqNumTooLow.def",
        NoHeader,
        "line 10 sends 34=2 again after 34=4",
    ),
    row(
        "2e_PossDupAlreadyReceived.def",
        NoHeader,
        "line 7 is a Heartbeat with 43=Y, 122= and 112=, replayed on demand",
    ),
    row(
        "2e_PossDupNotReceived.def",
        NoHeader,
        "lines 9 and 11 are Heartbeats with 43=Y and 122=, replayed on demand",
    ),
    row(
        "2f_PossDupOrigSendingTimeTooHigh.def",
        NoHeader,
        "line 15 replays 34=2 with 43=Y and 122=<TIME+10>",
    ),
    row(
        "2g_PossDupNoOrigSendingTime.def",
        NoHeader,
        "line 15 replays 34=2 with 43=Y and deliberately no 122=",
    ),
    row(
        "2k_CompIDDoesNotMatchProfile.def",
        Reachable,
        "a Logon, an application message and a Logout; passes",
    ),
    row(
        "2m_BodyLengthValueNotCorrect.def",
        NoBytes,
        "line 8 declares 9=30 over a longer body, and lines 26-27 re-pick 34",
    ),
    row(
        "2o_SendingTimeValueOutOfRange.def",
        Reachable,
        "a Logon, a Heartbeat and a Logout on two connections; passes",
    ),
    row(
        "2q_MsgTypeNotValid.def",
        Reachable,
        "35=* goes through send_application as bytes; passes",
    ),
    row(
        "2r_UnregisteredMsgType.def",
        NoOrder,
        "line 7 writes an ExecutionReport body 37,17,150,39,... ; the table sorts it",
    ),
    row(
        "3b_InvalidChecksum.def",
        NoBytes,
        "lines 8 and 12 declare 10=256, and lines 10-13 re-pick 34",
    ),
    row(
        "4a_NoDataSentDuringHeartBtInt.def",
        Reachable,
        "a Logon, two Heartbeats and a Logout; passes",
    ),
    row(
        "4b_ReceivedTestRequest.def",
        Reachable,
        "a Logon, a TestRequest and a Logout; passes",
    ),
    row(
        "6_SendTestRequest.def",
        NoAnswer,
        "we answer the TestRequest; no I line claims the answer — the script never answers it either",
    ),
    row(
        "7_ReceiveRejectMessage.def",
        NoAdmin,
        "line 8 is an unsolicited Reject; the session only ever answers with one",
    ),
    row(
        "8_AdminAndApplicationMessages.def",
        Reachable,
        "TestRequests, application messages and ResendRequests, all in the API",
    ),
    row(
        "8_OnlyAdminMessages.def",
        NoHeader,
        "line 22 re-sends a ResendRequest at 34=5 after 34=6",
    ),
    row(
        "8_OnlyApplicationMessages.def",
        Reachable,
        "application messages in table order, two ResendRequests and a TestRequest",
    ),
    row(
        "AlreadyLoggedOn.def",
        Reachable,
        "two connections, each a Logon; passes",
    ),
    row(
        "RejectResentMessage.def",
        NoHeader,
        "line 9 picks 34=3 over 34=2, and line 13 replays with 43=Y/122=",
    ),
    row(
        "ReverseRoute.def",
        Reachable,
        "six routed application messages and a Logout; passes",
    ),
    row(
        "SessionReset.def",
        NoAdmin,
        "line 21 is a second Logon with 141=Y, mid-connection",
    ),
];

/// How many of the 50 this end could be asked to say — the mirrored corpus's
/// ceiling.
///
/// `ADR-0076` decision 3: the score gate reads its ceiling from here, and
/// *passing* is `score == ceiling()`. A file in any `Needs*` class is reported
/// by name and never counted against the session.
#[must_use]
pub fn ceiling() -> usize {
    CLASSIFICATION
        .iter()
        .filter(|r| r.class == MirrorClass::Reachable)
        .count()
}

/// The files in one class, in table order.
#[must_use]
pub fn files_in(class: MirrorClass) -> Vec<&'static str> {
    CLASSIFICATION
        .iter()
        .filter(|r| r.class == class)
        .map(|r| r.file)
        .collect()
}

/// What the table says about `file`, if it is in it.
#[must_use]
pub fn class_of(file: &str) -> Option<MirrorClass> {
    CLASSIFICATION
        .iter()
        .find(|r| r.file == file)
        .map(|r| r.class)
}

/// The mirrored scenarios this table is about: the 50 that mirror.
///
/// # Errors
///
/// [`LoadError`] if the corpus cannot be read.
pub fn mirrorable() -> Result<Vec<Scenario>, LoadError> {
    Ok(scenarios_mirrored()?.into_iter().filter(mirrors).collect())
}

/// Does any `I` line of this mirrored scenario carry `tag=value`?
///
/// Mirrored, an `I` line is a [`Kind::Expect`] — what this engine is asked to
/// write. Used by the test that checks the `34=0` and `123=Y` rows against the
/// files rather than against the table's own prose.
#[must_use]
pub fn an_output_line_has(s: &Scenario, tag: &str, value: &str) -> bool {
    s.steps.iter().any(|step| match &step.kind {
        Kind::Expect(m) => has_field(&m.wire, tag, value),
        _ => false,
    })
}

/// `tag=value` as a whole SOH-delimited field.
///
/// Split at the delimiter and compared whole, so `34=0` never matches inside
/// `134=0` or `34=01`.
fn has_field(wire: &[u8], tag: &str, value: &str) -> bool {
    let mut needle = String::with_capacity(tag.len() + value.len() + 1);
    needle.push_str(tag);
    needle.push('=');
    needle.push_str(value);
    wire.split(|b| *b == 1)
        .any(|field| field == needle.as_bytes())
}
