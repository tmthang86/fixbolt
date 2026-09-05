//! The FIX 4.4 session layer: pure, allocation-free, and driven entirely by
//! `Input`-shaped calls.
//!
//! No socket, no clock, no `format!` — `CLAUDE.md` §2 non-negotiable 2. Time
//! arrives through [`Session::tick`] on the scale [`clock`] documents. What
//! proves the purity is the signature itself: nothing in this crate's public
//! API can reach a file descriptor or an allocator.
//!
//! # The gate
//!
//! [`docs/plans/2026-08-28-session-layer.md`] builds this in six steps, and
//! every step reports the same number: how many of the 59 QuickFIX FIX 4.4
//! acceptance definitions pass. `crates/session/tests/score.rs` is that gate.
//!
//! [`docs/plans/2026-08-28-session-layer.md`]: https://github.com/
#![forbid(unsafe_code)]

pub mod clock;
pub mod journal;
mod out;
pub mod schedule;
pub mod text;

use core::marker::PhantomData;

use fixbolt_codec::{
    Dictionary, FieldIndex, MessageView, ParseError, Parsed, SOH, TemplateBuilder, TimestampCache,
    Validation, as_u32, parse_into, tag_text_at,
};
use fixbolt_dict::{FieldType, Fix44};

use crate::journal::{Journal, NoJournal};
use crate::out::Outbound;
use crate::schedule::Schedule;
use crate::text::SessionText;

/// FIX tags this layer reads by number. Named so a call site never carries a
/// bare integer — `CLAUDE.md` §6, and it is how tag 12 was once mistaken for
/// `Currency` in a document.
pub(crate) mod tag {
    pub const BEGIN_STRING: u32 = 8;
    pub const MSG_SEQ_NUM: u32 = 34;
    pub const REF_SEQ_NUM: u32 = 45;
    pub const POSS_DUP_FLAG: u32 = 43;
    pub const ON_BEHALF_OF_COMP_ID: u32 = 115;
    pub const ON_BEHALF_OF_SUB_ID: u32 = 116;
    pub const ON_BEHALF_OF_LOCATION_ID: u32 = 144;
    pub const DELIVER_TO_COMP_ID: u32 = 128;
    pub const DELIVER_TO_SUB_ID: u32 = 129;
    pub const DELIVER_TO_LOCATION_ID: u32 = 145;
    pub const REF_TAG_ID: u32 = 371;
    pub const REF_MSG_TYPE: u32 = 372;
    pub const SESSION_REJECT_REASON: u32 = 373;
    pub const MSG_TYPE: u32 = 35;
    pub const SENDER_COMP_ID: u32 = 49;
    pub const SENDING_TIME: u32 = 52;
    pub const TARGET_COMP_ID: u32 = 56;
    pub const TEXT: u32 = 58;
    pub const ENCRYPT_METHOD: u32 = 98;
    pub const HEART_BT_INT: u32 = 108;
    pub const BEGIN_SEQ_NO: u32 = 7;
    pub const END_SEQ_NO: u32 = 16;
    pub const NEW_SEQ_NO: u32 = 36;
    pub const ORIG_SENDING_TIME: u32 = 122;
    pub const TEST_REQ_ID: u32 = 112;
    pub const GAP_FILL_FLAG: u32 = 123;
    pub const RESET_SEQ_NUM_FLAG: u32 = 141;
}

/// `MsgType` values this layer acts on.
mod msg {
    pub const LOGON: &[u8] = b"A";
    pub const LOGOUT: &[u8] = b"5";
    pub const TEST_REQUEST: &[u8] = b"1";
    pub const RESEND_REQUEST: &[u8] = b"2";
    pub const SEQUENCE_RESET: &[u8] = b"4";
}

/// The `TestReqID` this session writes when it asks the question itself.
///
/// **A constant because the oracle makes it one.** `112` is absent from
/// `test/definitions/fields.fmt`, so `Comparator.rb` matches it byte for byte,
/// and `6_SendTestRequest.def` writes `112=TEST` twice. Nothing in FIX 4.4 says
/// a `TestReqID` must be any particular string — a counter or a timestamp would
/// be just as correct on the wire and would fail this gate. It is QuickFIX's
/// default, and this is the one place that depends on it.
const OWN_TEST_REQ_ID: &[u8] = b"TEST";

/// What an application does with a message the session layer does not own.
///
/// The session owns the seven administrative types — `0 1 2 3 4 5 A` — and
/// hands everything else here. It supplies the two things an application does
/// not own, the outbound sequence number and the clock, and sends back exactly
/// what it is given.
///
/// The reply is written as **whole message bytes**, not as a body: the corpus's
/// own acceptance server re-sorts an incoming order through the dictionary and
/// sends it back, and re-encoding that a second time inside the session would
/// be work for nothing. It is the shape [ADR-0004] called `Action::Deliver`.
///
/// [ADR-0004]: https://github.com/
pub trait Application {
    /// One message for the application.
    ///
    /// Write a complete FIX message into `out` and return the range it
    /// occupies; the session emits it and spends `seq`. `None` says nothing,
    /// which is what `19a_PossResendMessageThatHAsAlreadyBeenSent.def` asks
    /// for: a `97=Y` whose order ID the application has already seen.
    ///
    /// `stamp` is 21 bytes with milliseconds. A reply that regenerates its own
    /// `SendingTime` is a body-length failure four bytes later.
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<core::ops::Range<usize>>;

    /// The application's chance to **speak first**, once per session.
    ///
    /// Asked with `nth = 0, 1, 2, …` immediately after this session comes up,
    /// until it answers `None`. Write one whole FIX message into `out` and
    /// return its range to have it sent; the caller sends each one as it comes,
    /// so nothing is accumulated and one buffer serves them all.
    ///
    /// **`34=` and `52=` are not yours and are not supplied.** Write the
    /// message without them, or write them wrong — either way
    /// [`Session::send_application`] rewrites both, along with `8=`, `9=` and
    /// `10=`. [`Peer`] carries the three values that *are* the application's to
    /// write, so a handler serving many counterparties can tell which session
    /// it is being asked about.
    ///
    /// **The default answers `None`**, which is why adding this method broke no
    /// existing implementation, and why an application with nothing to announce
    /// writes nothing.
    ///
    /// **Nothing in this crate calls this.** `Session` has no opinion about
    /// origination and never asks; `fixbolt_engine` does, at the turn where the
    /// session goes up, and bounds the loop. It is declared here because this
    /// is where the application seam is declared —
    /// [ADR-0048](../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md)
    /// pays for that in its *Consequences*.
    fn on_logon(
        &mut self,
        _nth: u32,
        _peer: Peer<'_>,
        _out: &mut [u8],
    ) -> Option<core::ops::Range<usize>> {
        None
    }
}

/// Which session an application is being asked to speak on.
///
/// A struct rather than three more positional arguments, for the reason
/// `JournalHealth` is one: three `&[u8]` in a row is a call nobody can read,
/// and two of them swapped addresses the message to yourself.
///
/// Everything here is **this session's configuration**, borrowed. `sender` is
/// this end and `target` is the counterparty — already the right way round for
/// `49=` and `56=` on a message going out, which is the one place that
/// reversal can go wrong.
#[derive(Debug, Clone, Copy)]
pub struct Peer<'a> {
    /// `8` **BeginString**, such as `b"FIX.4.4"`.
    pub begin_string: &'a [u8],
    /// `49` **SenderCompID** on the way out — **this** end.
    pub sender: &'a [u8],
    /// `56` **TargetCompID** on the way out — the **counterparty**.
    pub target: &'a [u8],
}

/// An application that never answers.
///
/// What [`Session::received`] uses, so a caller with no application at all
/// still has the same API. A session wired to this one is a pure session
/// machine: it answers the seven administrative types and drops the rest.
pub struct Silent;

impl Application for Silent {
    fn on_message(
        &mut self,
        _msg: &[u8],
        _seq: u32,
        _stamp: &[u8],
        _out: &mut [u8],
    ) -> Option<core::ops::Range<usize>> {
        None
    }
}

/// The seven message types the session layer answers itself.
///
/// Everything else is the application's. `7_ReceiveRejectMessage.def` is why
/// `3` is in the list: a Reject arriving is read and not answered, and an
/// application that echoed it would put a message on the wire the file does not
/// expect.
const ADMIN: [&[u8]; 7] = [b"0", b"1", b"2", b"3", b"4", b"5", b"A"];

/// Whether the connection survived the input.
///
/// The session never closes a socket — it does not have one. It says the link
/// should go, and the engine (or the harness) does it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Up,
    Dropped,
}

/// Which end of the connection this session is.
///
/// A marker, not a field: the two roles differ in *behaviour*, and behaviour
/// that branches on a runtime enum costs a predictable branch on the hot path
/// for a value that never changes. [ADR-0004] chose one session core
/// parameterised by role; this is that parameter.
///
/// [ADR-0004]: https://github.com/
pub trait Role: sealed::Sealed {
    /// Whether this end may speak first. An acceptor waits; an initiator opens
    /// with a Logon.
    const SPEAKS_FIRST: bool;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Acceptor {}
    impl Sealed for super::Initiator {}
}

/// The end that listens. The differentiator — see `docs/PRD.md` §1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Acceptor;

/// The end that connects out. [ADR-0004] brought it into phase 1.
///
/// [ADR-0004]: https://github.com/
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Initiator;

impl Role for Acceptor {
    const SPEAKS_FIRST: bool = false;
}

impl Role for Initiator {
    const SPEAKS_FIRST: bool = true;
}

/// A CompID or a BeginString, held inline.
///
/// `N` bytes and a length, so a `Config` allocates nothing and can be copied
/// into a per-connection slot. FIX bounds neither field, so a value that does
/// not fit is recorded as *not fitting* rather than truncated: `matches` then
/// answers `false` for everything, and the session refuses every message. Fail
/// closed. Silently keeping the first 32 bytes would let a session accept a
/// counterparty it was never configured for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Name<const N: usize> {
    buf: [u8; N],
    len: usize,
    fits: bool,
}

impl<const N: usize> Name<N> {
    fn new(src: &[u8]) -> Self {
        let mut buf = [0u8; N];
        // Keeps the truncation rather than blanking it, so `fits` is the only
        // thing standing between a too-long configuration and a match on its
        // first `N` bytes. Zeroing `len` here instead would make `fits`
        // decorative — it did, and the reversal that should have caught it
        // stayed green. See `tests/logon.rs`.
        let len = if src.len() < N { src.len() } else { N };
        buf[..len].copy_from_slice(&src[..len]);
        Self {
            buf,
            len,
            fits: src.len() <= N,
        }
    }

    fn matches(&self, other: &[u8]) -> bool {
        self.fits && self.buf[..self.len] == *other
    }

    /// The configured value, or `None` if it did not fit. Same fail-closed
    /// answer as [`Self::matches`]: a value that was truncated is not a value.
    fn get(&self) -> Option<&[u8]> {
        self.fits.then(|| &self.buf[..self.len])
    }
}

/// The most bytes a [`Config`] can hold for a `BeginString`.
///
/// Public because a caller that builds a [`Config`] from text — a configuration
/// file, say — has to refuse a value that would not fit, and **a second copy of
/// this number is a second rule that will disagree with this one**. An
/// over-long value is not shortened into something workable: the name keeps its
/// truncation and matches nothing at all, so it configures an acceptor that
/// silently serves nobody.
pub const MAX_BEGIN_STRING_LEN: usize = 16;

/// The most bytes a [`Config`] can hold for either CompID. See
/// [`MAX_BEGIN_STRING_LEN`] for why it is public and what an over-long value
/// costs.
pub const MAX_COMP_ID_LEN: usize = 32;

/// QuickFIX's default `MaxLatency`, in milliseconds.
///
/// `[documented]` 120 seconds is what `libquickfix` applies to `SendingTime`,
/// and `1d_InvalidLogonBadSendingTime` is 2001 years out, so nothing in the
/// corpus distinguishes this number from any other. It is the documented
/// default, labelled as such.
/// The default size of an [`Application`]'s reply scratch.
///
/// `[measured 2026-09-05]` **1 KiB, and it is the tightest ceiling in the
/// engine.** The longest reply in the acceptance corpus is 177 body bytes, so
/// the corpus can say nothing about it — a sweep against a real acceptor found
/// the wall between 200 and 1000 bytes while the receive buffer was 16 KiB.
/// An application that cannot lay out its reply returns `None`, which is legal,
/// so exceeding this is **silence** rather than an error:
/// `docs/reference/a-ceiling-has-more-than-one-floor.md`.
///
/// Raise it with the `APP` parameter — `fixbolt_engine::serve_with`. It must not
/// exceed the connection's `TX`, which is the queue the reply is then copied
/// into.
pub const DEFAULT_APP_SCRATCH: usize = 1024;

pub const DEFAULT_MAX_SKEW_MS: u64 = 120_000;

/// The `HeartBtInt` an initiator proposes unless told otherwise, in seconds.
///
/// `[measured]` 50 of the corpus's 65 Logons ask for 30, and it is QuickFIX's
/// own default.
pub const DEFAULT_HEART_BT_INT: u32 = 30;

/// Everything a session needs to know that is not on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    begin_string: Name<MAX_BEGIN_STRING_LEN>,
    /// Ours. Appears as `49=` on the way out and must appear as `56=` on the
    /// way in.
    sender_comp_id: Name<MAX_COMP_ID_LEN>,
    /// Theirs. `56=` out, `49=` in.
    target_comp_id: Name<MAX_COMP_ID_LEN>,
    max_skew_ms: u64,
    /// `108=`, in seconds. An acceptor never reads it — it throws the
    /// counterparty's back. An initiator proposes it.
    heart_bt_int: u32,
    /// When this session is open, and which instants belong to the same one.
    /// [`Schedule::always`] unless the caller says otherwise, and that default
    /// is **exactly neutral** — the 59 acceptance definitions run under it.
    schedule: Schedule,
    /// How many messages one call may put on the wire while answering a
    /// `ResendRequest`. See [`Self::with_resend_batch`].
    resend_batch: u16,
    /// When this session restarts its numbering of its own accord.
    /// [`ResetPolicy::new`] is neutral and is the default.
    reset: ResetPolicy,
    /// How long a connection may sit without completing its `Logon`.
    /// Zero is off, which is the default. [`Config::with_logon_timeout_ms`].
    logon_timeout_ms: u64,
    /// How long this end waits for a `Logout` it asked for.
    /// Zero is off, which is the default. [`Config::with_logout_timeout_ms`].
    logout_timeout_ms: u64,
}

/// When a session restarts both sequence numbers at 1 without being asked to
/// on the wire.
///
/// QuickFIX's `ResetOnLogon`, `ResetOnLogout` and `ResetOnDisconnect`, with
/// those names, because they mean the same three things — a desk arriving with
/// an existing configuration file recognises them.
///
/// # This is not [`Session::new`] versus [`Session::resume`]
///
/// Those two say **what the journal still has**: `resume` continues a count
/// that outlived a process, `new` starts one that never persisted anything
/// ([ADR-0010]). A reset policy says **what this session wants to happen next
/// time**, which is an operator's choice and lives in a file.
///
/// Collapsing the two would make `ResetOnLogon=Y` unrepresentable for exactly
/// the session that needs it: one that *was* resumed, from a journal holding
/// 500 messages, whose counterparty starts every morning at `34=1`.
///
/// # Neutral by default, in the same sense as [`Schedule::always`]
///
/// [`Self::new`] resets on nothing. The 59 acceptance definitions run under it
/// and not one of them mentions `ResetOn*`, so the default has to be the
/// behaviour they already prove — non-negotiable 3.
///
/// [ADR-0010]: ../../../docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResetPolicy {
    on_logon: bool,
    on_logout: bool,
    on_disconnect: bool,
}

impl ResetPolicy {
    /// Reset on nothing. The default, and exactly today's behaviour.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            on_logon: false,
            on_logout: false,
            on_disconnect: false,
        }
    }

    /// `ResetOnLogon=Y` — restart both counts as a connection is made, **even
    /// for a resumed session**, which is the only case where this changes
    /// anything.
    #[must_use]
    pub const fn on_logon(mut self) -> Self {
        self.on_logon = true;
        self
    }

    /// `ResetOnLogout=Y` — restart both counts once the `Logout` exchange is
    /// over.
    ///
    /// **After the goodbye is numbered, never before.** A `Logout` that went
    /// out as `34=1` because the reset ran first would put that number on the
    /// wire twice in one session.
    #[must_use]
    pub const fn on_logout(mut self) -> Self {
        self.on_logout = true;
        self
    }

    /// `ResetOnDisconnect=Y` — restart both counts when the link goes down for
    /// any other reason.
    ///
    /// **This is the one that loses messages**, and it is the one QuickFIX
    /// deployments most often turn on. A counterparty that reconnects after a
    /// network drop and asks for a resend gets numbers this end no longer
    /// believes in.
    #[must_use]
    pub const fn on_disconnect(mut self) -> Self {
        self.on_disconnect = true;
        self
    }

    /// Does this policy reset as the connection is made?
    #[must_use]
    pub const fn resets_on_logon(self) -> bool {
        self.on_logon
    }

    /// Does this policy reset after a `Logout`?
    #[must_use]
    pub const fn resets_on_logout(self) -> bool {
        self.on_logout
    }

    /// Does this policy reset when the link drops?
    #[must_use]
    pub const fn resets_on_disconnect(self) -> bool {
        self.on_disconnect
    }
}

/// The default for [`Config::with_resend_batch`].
///
/// Eight, because the constraint that matters is
/// `resend_batch × SLOT_LEN < TX`: 8 × 512 = 4 KiB against an 8 KiB transmit
/// buffer. [ADR-0046](../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)
/// decision 4.
pub const DEFAULT_RESEND_BATCH: u16 = 8;

impl Config {
    /// An acceptor's configuration. `sender_comp_id` is *this* end.
    #[must_use]
    pub fn acceptor(begin_string: &[u8], sender_comp_id: &[u8], target_comp_id: &[u8]) -> Self {
        Self {
            begin_string: Name::new(begin_string),
            sender_comp_id: Name::new(sender_comp_id),
            target_comp_id: Name::new(target_comp_id),
            max_skew_ms: DEFAULT_MAX_SKEW_MS,
            heart_bt_int: DEFAULT_HEART_BT_INT,
            schedule: Schedule::always(),
            resend_batch: DEFAULT_RESEND_BATCH,
            reset: ResetPolicy::new(),
            logon_timeout_ms: 0,
            logout_timeout_ms: 0,
        }
    }

    /// An initiator's configuration. `sender_comp_id` is *this* end.
    ///
    /// The one field an acceptor never uses is [`Self::with_heart_bt_int`]: an
    /// acceptor throws the counterparty's `108=` back, an initiator proposes
    /// its own.
    #[must_use]
    pub fn initiator(begin_string: &[u8], sender_comp_id: &[u8], target_comp_id: &[u8]) -> Self {
        Self::acceptor(begin_string, sender_comp_id, target_comp_id)
    }

    /// Override [`DEFAULT_HEART_BT_INT`]. Initiators only.
    #[must_use]
    pub const fn with_heart_bt_int(mut self, secs: u32) -> Self {
        self.heart_bt_int = secs;
        self
    }

    /// How many messages one call may put on the wire answering a
    /// `ResendRequest`. The rest go out on later calls.
    ///
    /// **This is what stops a resend ending the session it is answering.**
    /// `[2026-09-04]` before it existed the replay loop emitted the whole range
    /// in one call; a transmit buffer that could not hold it set `overflow`,
    /// and D10's `Disconnect` answered a counterparty's `ResendRequest` with
    /// `Logout 58=slow consumer` while that counterparty's own socket was
    /// empty. Fifty messages of 200 bytes was enough.
    ///
    /// **Choose it against the transmit buffer**: `n × SLOT_LEN` must stay
    /// under `TX`. The default is 8 against 8 KiB, and `GUIDE.md` §6 has the
    /// arithmetic.
    ///
    /// **Zero is read as one.** A batch of nothing would leave every resend
    /// permanently unfinished, which is worse than any reading of what the
    /// caller meant; the builders here are infallible and this one stays that
    /// way. `a_batch_of_zero_is_read_as_one` is the test.
    #[must_use]
    pub const fn with_resend_batch(mut self, n: u16) -> Self {
        self.resend_batch = if n == 0 { 1 } else { n };
        self
    }

    /// The batch size a resend is answered in.
    #[must_use]
    pub const fn resend_batch(&self) -> u16 {
        self.resend_batch
    }

    /// When this session restarts its numbering of its own accord.
    ///
    /// See [`ResetPolicy`], including why this is a configuration field rather
    /// than a choice between [`Session::new`] and [`Session::resume`].
    #[must_use]
    pub const fn with_reset(mut self, reset: ResetPolicy) -> Self {
        self.reset = reset;
        self
    }

    /// The reset policy this session keeps.
    #[must_use]
    pub const fn reset(&self) -> ResetPolicy {
        self.reset
    }

    /// QuickFIX's `LogonTimeout`: how long a connection may sit without
    /// completing its `Logon` before the session gives up, in **milliseconds**.
    ///
    /// **Zero is off**, and off is the default — the same reading `108=0`
    /// already has one screen down, and the behaviour every session in this
    /// repository had before 2026-09-05.
    ///
    /// **This is the initiator's, and it is nobody else's.** An acceptor has
    /// `presession::Limits::new(pending, logon_ms)` in front of it, which holds
    /// the socket before a `Session` exists at all; an initiator that dials a
    /// venue which accepts the connection and then says nothing has no such
    /// stage. Setting it on an acceptor is not an error and does the obvious
    /// thing, but it is a second deadline behind a first one.
    ///
    /// The clock starts at the **first [`Session::tick`] after
    /// [`Session::connect`]**, because a pure layer is given time in no other
    /// way (D1).
    #[must_use]
    pub const fn with_logon_timeout_ms(mut self, ms: u64) -> Self {
        self.logon_timeout_ms = ms;
        self
    }

    /// The logon deadline in milliseconds, or zero for none.
    #[must_use]
    pub const fn logon_timeout_ms(&self) -> u64 {
        self.logon_timeout_ms
    }

    /// QuickFIX's `LogoutTimeout`: how long this end waits for the `Logout` it
    /// asked for, in **milliseconds**.
    ///
    /// **Zero is off**, and off is the default.
    ///
    /// [`Session::begin_logout`] leaves the link up on purpose so the caller
    /// can wait for the answer, and until this existed nothing bounded that
    /// wait except the heartbeat rules — 2.4 × `HeartBtInt`, which is 72
    /// seconds on a default session. A venue that takes the goodbye and then
    /// dies holds the socket open through the close.
    ///
    /// The clock starts at the **first [`Session::tick`] after
    /// [`Session::begin_logout`]**, for the same reason as above.
    #[must_use]
    pub const fn with_logout_timeout_ms(mut self, ms: u64) -> Self {
        self.logout_timeout_ms = ms;
        self
    }

    /// The logout deadline in milliseconds, or zero for none.
    #[must_use]
    pub const fn logout_timeout_ms(&self) -> u64 {
        self.logout_timeout_ms
    }

    /// When this session is open, and when both ends start again at `34=1`.
    ///
    /// Without this a session is open forever and never resets, which is what
    /// every session built before schedules existed does. See
    /// [`schedule`] — in particular that the times are
    /// **UTC**, and that a fixed offset is not daylight saving.
    #[must_use]
    pub const fn with_schedule(mut self, schedule: Schedule) -> Self {
        self.schedule = schedule;
        self
    }

    /// The schedule this session keeps.
    #[must_use]
    pub const fn schedule(&self) -> Schedule {
        self.schedule
    }

    /// Override [`DEFAULT_MAX_SKEW_MS`].
    #[must_use]
    pub const fn with_max_skew_ms(mut self, ms: u64) -> Self {
        self.max_skew_ms = ms;
        self
    }

    /// Does an inbound `49=` name the counterparty this configuration serves?
    ///
    /// For an acceptor the incoming `SenderCompID` is *theirs*, so it is
    /// checked against [`Self::acceptor`]'s `target_comp_id`. Fail-closed: a
    /// configured value too long to fit matches nothing at all.
    #[must_use]
    pub fn inbound_sender_matches(&self, comp_id: &[u8]) -> bool {
        self.target_comp_id.matches(comp_id)
    }

    /// Does an inbound `56=` name **us**?
    #[must_use]
    pub fn inbound_target_matches(&self, comp_id: &[u8]) -> bool {
        self.sender_comp_id.matches(comp_id)
    }

    /// `8` **BeginString**, as configured.
    ///
    /// This and the two below are the only way to *read* an identity rather
    /// than test one. They exist for
    /// [ADR-0048](../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md):
    /// an application asked to speak first is told which session it is speaking
    /// on, and a predicate cannot say that.
    ///
    /// **Empty when the configured value did not fit**, the same fail-closed
    /// answer [`Self::inbound_sender_matches`] gives. Unreachable in practice:
    /// a `Config` whose names did not fit matches no inbound message, so no
    /// such session ever logs on to be asked.
    #[must_use]
    pub fn begin_string(&self) -> &[u8] {
        self.begin_string.get().unwrap_or(b"")
    }

    /// `49` **SenderCompID** on the way out — **this** end.
    #[must_use]
    pub fn sender_comp_id(&self) -> &[u8] {
        self.sender_comp_id.get().unwrap_or(b"")
    }

    /// `56` **TargetCompID** on the way out — the **counterparty**.
    #[must_use]
    pub fn target_comp_id(&self) -> &[u8] {
        self.target_comp_id.get().unwrap_or(b"")
    }

    /// Do these two configurations name the **same FIX session identity**?
    ///
    /// BeginString and both comp IDs, and deliberately nothing else: two entries
    /// for one counterparty that differ only in `HeartBtInt` or `MaxLatency` are
    /// still that one counterparty, and *"is this identity already logged on"*
    /// must say yes.
    ///
    /// `1b_DuplicateIdentity.def` is what this exists for, and its own comment
    /// is the specification: *"If two logons with the same
    /// SenderCompID/TargetCompID combination logon the second one must be
    /// disconnected"* — **per identity, not per acceptor**
    /// ([ADR-0030](../../../docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md)).
    #[must_use]
    pub fn same_identity_as(&self, other: &Self) -> bool {
        self.begin_string == other.begin_string
            && self.sender_comp_id == other.sender_comp_id
            && self.target_comp_id == other.target_comp_id
    }

    /// Both halves at once: is this the configuration for a connection whose
    /// `Logon` carries `49=sender` and `56=target`?
    ///
    /// The question a counterparty registry asks
    /// ([ADR-0026](../../../docs/decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)),
    /// and it is composed from the two predicates the session's own `Logon`
    /// check uses rather than written a second time. The session keeps them
    /// apart because each has its own refusal — `1c_InvalidLogonBadSenderCompID`
    /// and `2k_CompIDDoesNotMatchProfile` are different definitions — and a
    /// registry only needs the conjunction. **Two copies of this comparison
    /// would be two rules that disagree**, and the one that disagreed would be
    /// the one deciding whether to let a stranger in.
    #[must_use]
    pub fn serves(&self, sender: &[u8], target: &[u8]) -> bool {
        self.inbound_sender_matches(sender) && self.inbound_target_matches(target)
    }
}

/// Where the session is in its life.
///
/// Step 1 of the plan needs two states. Steps 2 and 5 add the rest; a state
/// added before a definition needs it is a state nothing tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// No connection, or the link has been dropped.
    Disconnected,
    /// Connected, and the next message must be a Logon.
    AwaitingLogon,
    /// Connected, and **this end** owes the Logon.
    ///
    /// Separate from [`State::AwaitingLogon`] because the two differ in what
    /// the next `tick` does, and `connect` has no clock to do it with — time
    /// enters this layer through `tick` and nowhere else (D1). An engine
    /// connects and then turns its loop; this is that turn.
    MustLogon,
    /// Logon exchanged. Application messages may flow.
    LoggedOn,
    /// This end sent a Logout and is waiting for the counterparty's. It may
    /// never come — `2i_BeginStringValueUnexpected.def` runs the same sequence
    /// twice, once with a reply and once without, and the link must go down
    /// either way. So the link is reported down at once and anything that
    /// arrives afterwards is read and ignored.
    AwaitingLogout,
    /// This end sent a `Logout` **as part of an ordered shutdown** and is still
    /// listening.
    ///
    /// Distinct from [`State::AwaitingLogout`], and the difference is the
    /// reason this state exists: `AwaitingLogout` reports the link **down at
    /// once** and ignores everything after, which is right for D10's paths
    /// where the point is to cut. An ordered shutdown is the healthy case — we
    /// are the ones leaving — so the link stays up, the heartbeat keeps
    /// running, and the counterparty's own `Logout` is read and answered as
    /// [`DropReason::PeerLogout`] rather than discarded.
    ///
    /// `[measured 2026-09-02]` folding this into `AwaitingLogout` was tried
    /// first and made every wait vacuous: the next `tick` returned `Dropped`
    /// from the state check with no reason recorded, so *"they answered"* and
    /// *"they never answered"* were the same observable.
    LoggingOut,
}

/// Why a message was refused.
///
/// Fieldless — non-negotiable 2. Step 1 answers every one of these by dropping
/// the link, which is what `1c`, `1d` and `1e` ask for. Step 3 turns the ones
/// that deserve a `Reject (35=3)` into one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// `8=` is not the configured BeginString.
    WrongBeginString,
    /// The first message on a connection was not a Logon.
    NotALogon,
    /// A Logon without `98=` or `108=`. FIX 4.4 makes both required, and a
    /// session cannot answer without echoing them.
    LogonIncomplete,
    /// `49=` is not the configured counterparty.
    WrongSenderCompId,
    /// `56=` is not us.
    WrongTargetCompId,
    /// `52=` is absent, unreadable, or too far from the last [`Session::tick`].
    BadSendingTime,
    /// `34=` is absent, unreadable, or lower than the one expected. FIX has no
    /// way back from a sequence number that has already been used.
    BadSeqNum,
    /// A message arrived while the schedule says this session is shut. Like
    /// every other pre-Logon fault it is answered with silence — there is no
    /// session to answer with, and `1c`/`1d`/`1e` establish the shape.
    OutsideSchedule,
    /// A message the session could not put on the wire: the configuration does
    /// not fit its own templates, or the output buffer is too small. A bug, and
    /// the session fails closed rather than sending something malformed.
    CannotSend,
}

/// Why a connection ended.
///
/// `Link::Dropped` is one bit and this session returns it from **eighteen**
/// places. A wrong `BeginString` means somebody is on the wrong FIX version; a
/// wrong `SenderCompID` means somebody is pointed at the wrong counterparty; a
/// `SendingTime` out of range means NTP; an hour outside the schedule means a
/// venue calendar. Different people fix them, on different days, and **before
/// this existed the engine said exactly the same thing about all four**.
///
/// `[measured 2026-09-02]` that cost hours twice in one week — a schedule test
/// that passed on the clock rule
/// (`docs/reference/two-time-rules-share-one-observable.md`) and a `Logon`
/// refused for a `FieldIndex` too small while the message blamed a missing
/// registry (`docs/reference/silence-before-a-logon-has-many-causes.md`). Both
/// write-ups end on the same sentence: make the reason observable.
///
/// **Fieldless**, so it sits on the error path of a pure layer with nothing to
/// allocate — non-negotiable 2. Read it with [`Session::last_drop_reason`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DropReason {
    /// `8=` is not the configured `BeginString`.
    WrongBeginString,
    /// The first message on the connection was not a `Logon`.
    NotALogon,
    /// A `Logon` without `98=` or `108=`, both required by FIX 4.4.
    LogonIncomplete,
    /// `49=` is not the configured counterparty.
    WrongSenderCompId,
    /// `56=` is not us.
    WrongTargetCompId,
    /// `52=` is absent, unreadable, or further from this engine's clock than
    /// `max_skew_ms`. **Check NTP** — [`Session::last_skew_ms`] says by how much.
    SendingTimeOutOfRange,
    /// `34=` is absent, unreadable, or already used.
    SequenceNumberTooLow,
    /// A message arrived while the schedule says this session is shut. **Check
    /// the venue calendar**, not the clock.
    OutsideSchedule,
    /// The session could not put a message on the wire. A bug, and it fails
    /// closed rather than sending something malformed.
    CannotSend,
    /// Nothing arrived for long enough that the session gave up, after an
    /// unanswered `TestRequest`.
    HeartbeatTimeout,
    /// The connection was made and the `Logon` exchange never completed inside
    /// [`Config::with_logon_timeout_ms`].
    ///
    /// **Not a heartbeat timeout, and telling them apart is the point.** A
    /// heartbeat timeout means a session that was working stopped answering; on
    /// an initiator this one usually means the venue is listening but not yet
    /// open, or is refusing this end without saying so.
    LogonTimedOut,
    /// This end said `Logout` and the answer never came inside
    /// [`Config::with_logout_timeout_ms`].
    ///
    /// **Not [`DropReason::PeerLogout`].** That one means the exchange
    /// completed; this one means the goodbye went unanswered, which is the
    /// difference between a clean shutdown and one to reconcile by hand.
    LogoutTimedOut,
    /// The counterparty sent a `Logout`.
    PeerLogout,
    /// The schedule's window closed on a live session. **Not a fault.**
    ScheduleClosed,
    /// The transport reported the connection gone.
    TransportClosed,
    /// This counterparty is already logged on somewhere else, so the engine
    /// refused the second connection — [ADR-0030](../../../docs/decisions/ADR-0030-one-engine-holds-many-counterparties.md).
    ///
    /// **An engine reason, not a session one.** The pre-session stage never
    /// hands the message to a session at all, which is why it arrives through
    /// [`Session::disconnect_with`] rather than from a refusal.
    DuplicateIdentity,
    /// The application behind the ring would not take a message, so the engine
    /// ended the connection — [ADR-0011](../../../docs/decisions/ADR-0011-a-full-ring-disconnects.md).
    /// **The counterparty is faultless.**
    SlowApplication,
    /// Output backed up further than the policy allows — `DESIGN.md` D10.
    SlowConsumer,
    /// The engine was asked to stop.
    ///
    /// Recorded for a connection that had **nothing to say goodbye to** — one
    /// that never logged on — and for one still there when the shutdown's
    /// deadline passed. A session that answered our `Logout` reports
    /// [`DropReason::PeerLogout`] instead, and telling those two apart is the
    /// difference between a clean shutdown and one to reconcile by hand.
    EngineShutdown,
}

impl From<Refusal> for DropReason {
    /// Exhaustive, with **no `_` arm**: a new [`Refusal`] must be given a public
    /// name here or the build stops. The rule
    /// `docs/reference/a-counter-that-must-be-remembered-is-not-a-counter.md`
    /// arrived at, applied to an enum instead of a struct of counts.
    fn from(r: Refusal) -> Self {
        match r {
            Refusal::WrongBeginString => Self::WrongBeginString,
            Refusal::NotALogon => Self::NotALogon,
            Refusal::LogonIncomplete => Self::LogonIncomplete,
            Refusal::WrongSenderCompId => Self::WrongSenderCompId,
            Refusal::WrongTargetCompId => Self::WrongTargetCompId,
            Refusal::BadSendingTime => Self::SendingTimeOutOfRange,
            Refusal::BadSeqNum => Self::SequenceNumberTooLow,
            Refusal::OutsideSchedule => Self::OutsideSchedule,
            Refusal::CannotSend => Self::CannotSend,
        }
    }
}

/// One FIX session, parameterised by role.
///
/// `N` is the [`FieldIndex`] capacity — the caller picks it, per `CLAUDE.md`
/// §6. 256 covers every message in the acceptance corpus.
///
/// Not `Debug`: `FieldIndex` is not, and it is 3 KiB of offsets that no one
/// would read anyway.
pub struct Session<R: Role, const N: usize, const APP: usize = DEFAULT_APP_SCRATCH> {
    cfg: Config,
    state: State,
    /// Milliseconds since 0000-01-01, from the last [`Session::tick`].
    ///
    /// Starts at the epoch. A session that has never been ticked therefore
    /// believes it is at year zero and refuses every present-day
    /// `SendingTime` — fail closed, and visible immediately rather than as a
    /// clock quietly two hours out.
    now_ms: u64,
    idx: FieldIndex<N>,
    /// `None` when the configuration cannot be turned into templates. The
    /// session then refuses everything — see [`out::Outbound::new`].
    out: Option<Outbound<APP>>,
    /// `SendingTime`, formatted once a minute rather than once a message (D9).
    stamp: TimestampCache,
    /// `34=` on the next message this session sends. FIX counts from 1.
    next_out: u32,
    /// `34=` this session expects next from the counterparty.
    next_in: u32,
    /// The counterparty's `108=`, in milliseconds. Zero until a Logon says
    /// otherwise, and zero means no heartbeats at all — which is what FIX 4.4
    /// says `108=0` means.
    beat_ms: u64,
    /// The last instant this session is known to have been active at, if the
    /// caller supplied one. `None` means *nobody said*, and a session that was
    /// never told cannot be told a boundary has passed — see
    /// [`Session::resume_at`].
    ///
    /// It is compared, never counted from: [`Schedule::same_session`] answers
    /// *did a boundary pass between then and now*, which is the only question
    /// an engine that slept through midnight can still answer.
    session_mark: Option<u64>,
    /// Why this session last ended, if it has. Cleared by [`Session::connect`],
    /// so a live session reports `None` rather than the previous connection's
    /// cause.
    last_drop_reason: Option<DropReason>,
    /// The engine's clock minus the `SendingTime` of the last message whose
    /// `52=` could be read, in milliseconds. Positive: their stamp is behind
    /// ours.
    ///
    /// **Recorded whether the message was accepted or refused**, because the
    /// case an operator needs it for is exactly the refused one: `max_skew_ms`
    /// drops a message in silence, and on a box whose NTP has drifted a
    /// counterparty simply stops working with nothing anywhere saying why.
    /// `STATUS.md` open item 30 (b).
    ///
    /// Pure — an `Option<i64>` on the struct, no clock read, no allocation. D1
    /// stands.
    last_skew_ms: Option<i64>,
    /// When this session last put a message on the wire, and last accepted one.
    /// Both are read only by [`Session::tick`].
    last_sent_ms: u64,
    last_recv_ms: u64,
    /// How many `TestRequest`s have gone out since the last message arrived.
    /// QuickFIX widens its patience by this count and so does this.
    test_requests: u32,
    /// The gap this session has already asked for: the `7=` it sent and the
    /// last number it is waiting on. `resend_from == 0` means none is
    /// outstanding, which is what stops a second `ResendRequest` going out for
    /// a gap already being filled.
    resend_from: u32,
    resend_to: u32,
    /// Whether this session was built from persisted state.
    ///
    /// **The one bit that separates a reconnect from a restart.** `false` for a
    /// session from [`Session::new`], which has never persisted anything and so
    /// starts a new count on every connection — that is what all seven
    /// `iCONNECT`s in the acceptance corpus expect. `true` for one from
    /// [`Session::resume`], which is continuing a session that outlived a
    /// process, and whose numbers a new connection must not touch.
    /// [ADR-0010](../../../docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md).
    resumed: bool,
    /// Messages that arrived ahead of [`Self::next_in`], held until the gap in
    /// front of them closes.
    queue: [Queued; QUEUED],
    /// A `ResendRequest` this end is still answering, and how far it has got.
    ///
    /// `None` when nothing is owed. **Deliberately not named `resend_*`**: the
    /// two fields above are the gap *this* end asked the counterparty to fill,
    /// and this is the one it owes them. Confusing the directions is how a
    /// session answers its own question.
    owed: Option<Replay>,
    /// Application messages the journal would not keep.
    ///
    /// See [`Self::puts_refused`].
    puts_refused: u32,
    /// Numbers a `ResendRequest` asked for that had fallen out of the journal.
    ///
    /// See [`Self::resend_beyond_journal`].
    resend_beyond_journal: u32,
    /// When the wait for a `Logon`, and the wait for an answer to our `Logout`,
    /// started being measured. `None` until the first [`Session::tick`] in that
    /// state.
    ///
    /// **Two fields and not one**, though the states never overlap: one field
    /// would have to be cleared at every state change, and the states are
    /// entered from more places than either deadline is read from. A stale
    /// instant here is a session that ends early for a reason it names wrongly,
    /// which is the failure this whole enum of reasons exists to prevent.
    ///
    /// Set on a tick rather than in `connect` / `begin_logout` because those
    /// two are given no clock: time reaches this layer through `tick` and
    /// nowhere else (D1). The cost is that a deadline is measured from the
    /// caller's first turn rather than from the connection, which for any
    /// engine in this repository is the same millisecond.
    awaiting_logon_since_ms: Option<u64>,
    logging_out_since_ms: Option<u64>,
    _role: PhantomData<R>,
}

impl<R: Role, const N: usize, const APP: usize> Session<R, N, APP> {
    /// A session that has not yet been connected.
    #[must_use]
    pub fn new(cfg: Config) -> Self {
        let out = match (cfg.begin_string.get(), cfg.sender_comp_id.get()) {
            (Some(begin), Some(sender)) => cfg
                .target_comp_id
                .get()
                .and_then(|target| Outbound::new(begin, sender, target)),
            _ => None,
        };
        Self {
            cfg,
            state: State::Disconnected,
            now_ms: 0,
            idx: FieldIndex::new(),
            out,
            stamp: TimestampCache::new(),
            next_out: 1,
            next_in: 1,
            resumed: false,
            beat_ms: 0,
            last_sent_ms: 0,
            last_recv_ms: 0,
            test_requests: 0,
            resend_from: 0,
            resend_to: 0,
            session_mark: None,
            last_drop_reason: None,
            last_skew_ms: None,
            queue: [const {
                Queued {
                    seq: 0,
                    len: 0,
                    buf: [0; QUEUED_LEN],
                }
            }; QUEUED],
            owed: None,
            puts_refused: 0,
            resend_beyond_journal: 0,
            awaiting_logon_since_ms: None,
            logging_out_since_ms: None,
            _role: PhantomData,
        }
    }

    /// A session continuing from numbers that outlived the process.
    ///
    /// `next_out` and `next_in` are what the caller recovered — from a
    /// [`Journal`] for the outbound side, and from
    /// whatever made the inbound side durable. **This layer does no I/O and
    /// never will** (D1, non-negotiable 2): recovering the numbers is the
    /// engine's job, and this is where it hands them over.
    ///
    /// Intended to be called **once, at startup**. Calling it per connection
    /// would defeat its purpose — see [`Self::connect`], which keeps the count
    /// for a session built this way precisely so that a reconnect does not have
    /// to be one.
    ///
    /// [ADR-0010](../../../docs/decisions/ADR-0010-a-reconnect-is-not-a-restart.md).
    /// Proven by `crates/engine/tests/recovery.rs`:
    /// `a_session_resumed_from_a_journal_keeps_counting`, with
    /// `a_new_session_still_restarts_on_every_connect` as its other half.
    #[must_use]
    pub fn resume(cfg: Config, next_out: u32, next_in: u32) -> Self {
        let mut s = Self::new(cfg);
        s.next_out = next_out;
        s.next_in = next_in;
        s.resumed = true;
        s
    }

    /// A session continuing from numbers **and from when they were last
    /// touched**.
    ///
    /// [`Self::resume`] carries the numbers and asserts nothing about the
    /// calendar, so a session resumed that way never resets on a boundary: it
    /// was not told when it was last active, and **a reset cannot be decided
    /// without that**. This is where a caller says.
    ///
    /// `last_active_ms` is on the scale [`Self::tick`] uses. On the first tick
    /// afterwards, if [`Schedule::same_session`] says a boundary has passed,
    /// both counts restart at 1 — **before** the message that follows is
    /// numbered, so the first `Logon` of a new trading day really carries
    /// `34=1`.
    ///
    /// Under [`Schedule::always`] this is identical to [`Self::resume`]: one
    /// session, no boundary, nothing to notice.
    #[must_use]
    pub fn resume_at(cfg: Config, next_out: u32, next_in: u32, last_active_ms: u64) -> Self {
        let mut s = Self::resume(cfg, next_out, next_in);
        s.session_mark = Some(last_active_ms);
        s
    }

    /// The last instant this session was told, or observed, that it was active.
    ///
    /// What an engine persists so the session it builds after a restart can be
    /// given it back through [`Self::resume_at`]. `None` until either a tick
    /// has landed or the caller supplied one.
    #[must_use]
    pub const fn last_active_ms(&self) -> Option<u64> {
        self.session_mark
    }

    /// Has a schedule boundary passed since this session was last active? If
    /// so, restart both counts.
    ///
    /// **Called at the top of every tick**, so the reset lands ahead of the
    /// numbering rather than behind it. Returns nothing: a boundary is not an
    /// error and there is nobody to tell — the observable effect is that the
    /// next message out is `34=1`.
    ///
    /// Pure, like everything else here. `now_ms` came from a tick.
    fn roll_if_a_boundary_passed(&mut self, now_ms: u64) {
        let Some(mark) = self.session_mark else {
            // Nobody said when this session was last active, so nothing can be
            // concluded. `resume` deliberately lands here — ADR-0010 says a
            // reconnect is not a restart, and inventing a boundary would make
            // it one.
            self.session_mark = Some(now_ms);
            return;
        };
        if !self.cfg.schedule.same_session(mark, now_ms) {
            self.next_out = 1;
            self.next_in = 1;
            self.resend_from = 0;
            self.resend_to = 0;
            // The numbers just restarted, so an outstanding replay is owed on
            // numbers that no longer mean anything.
            self.owed = None;
        }
        self.session_mark = Some(now_ms);
    }

    /// Why this session last ended, or `None` while it is up.
    ///
    /// The **latest** cause: a session that ends twice reports the second, and
    /// [`Self::connect`] clears it.
    /// `tests/drop_reason.rs::a_second_fault_replaces_the_first` holds that,
    /// because a field written after the state change would report the one
    /// before.
    #[must_use]
    pub const fn last_drop_reason(&self) -> Option<DropReason> {
        self.last_drop_reason
    }

    /// End the session, recording why.
    ///
    /// **One place, so a new way to end cannot forget to say why.**
    fn end(&mut self, why: DropReason) {
        // `ResetOnLogout` / `ResetOnDisconnect`, and this is the one place that
        // knows which of the two happened — every ending funnels through here
        // carrying its reason, which is what [`DropReason`] is for.
        //
        // **Called after the goodbye has been written, never before.** The
        // `Logout` reply is emitted by the caller above and then this runs, so
        // the message that ends the session still carries the number it was
        // owed. Resetting first would spend `34=1` twice in one session, and
        // the counterparty would refuse the second one as already used.
        //
        // A `Logout` in either direction is the logout case: this end sending
        // one leaves the session in `LoggingOut` or `AwaitingLogout` and the
        // counterparty's answer arrives here as `PeerLogout` too.
        let by_logout = matches!(why, DropReason::PeerLogout);
        let reset = self.cfg.reset;
        if (by_logout && reset.resets_on_logout()) || (!by_logout && reset.resets_on_disconnect()) {
            self.next_out = 1;
            self.next_in = 1;
        }
        self.last_drop_reason = Some(why);
        self.state = State::Disconnected;
        // **A replay does not survive the connection it was owed on.** The
        // numbers belong to a session that has ended; carrying the cursor into
        // the next connection would push that session's `43=Y` messages onto
        // it on the first tick.
        self.owed = None;
    }

    /// The engine's clock minus the last readable `SendingTime`, in ms.
    ///
    /// `None` until a message carrying a parseable `52=` has arrived. See the
    /// field's own note: it is recorded even when the message was refused,
    /// because that is the case it answers.
    #[must_use]
    pub const fn last_skew_ms(&self) -> Option<i64> {
        self.last_skew_ms
    }

    /// The configuration this session was built with.
    ///
    /// `Copy`, so it hands back a value rather than a borrow — an engine holding
    /// many counterparties reads it to answer *"is this identity already logged
    /// on"* while another connection is borrowed mutably.
    #[must_use]
    pub const fn config(&self) -> Config {
        self.cfg
    }

    /// The sequence number the next outbound message will carry.
    ///
    /// For an engine that must persist it. Not a hot-path accessor.
    #[must_use]
    pub const fn next_out(&self) -> u32 {
        self.next_out
    }

    /// Application messages this session sent that the journal would not keep.
    ///
    /// **Zero on a healthy acceptor.** Anything else means replies are longer
    /// than a journal slot, and every `ResendRequest` covering one of them is
    /// answered with a gap fill for the rest of this session's life. The
    /// message still went out; what is lost is the ability to send it again.
    ///
    /// Monotonic within a connection, and reset by a new one, like every other
    /// per-connection count here. Not a hot-path accessor.
    /// [ADR-0046](../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md).
    #[must_use]
    pub const fn puts_refused(&self) -> u32 {
        self.puts_refused
    }

    /// Sequence numbers a `ResendRequest` asked for that had already fallen out
    /// of the journal, and were gap-filled instead of replayed.
    ///
    /// **It counts messages, not events.** One resend that fills over thirteen
    /// numbers reads thirteen: the question an operator has is *how much did
    /// the counterparty ask for and not get*, and a count of occurrences
    /// answers a different one.
    ///
    /// **A number below the journal's floor only.** A gap fill over an
    /// administrative message, or over a number below anything the journal ever
    /// held, is not counted — none of those was ever resendable, and a counter
    /// that rose on every ordinary reconnect would be an alarm nobody reads.
    /// [`journal::Journal::oldest`] is the floor.
    ///
    /// Non-zero means **the ring is too small for this counterparty's
    /// disconnections** — `GUIDE.md` §6 has the arithmetic for choosing a
    /// bigger one.
    #[must_use]
    pub const fn resend_beyond_journal(&self) -> u32 {
        self.resend_beyond_journal
    }

    /// The sequence number this session next expects to receive.
    #[must_use]
    pub const fn next_in(&self) -> u32 {
        self.next_in
    }

    /// Set the number the next outbound message will carry. **Local only —
    /// nothing goes on the wire.**
    ///
    /// This is the 3 a.m. operation, and it is a lie until the counterparty is
    /// told: they still expect the old number and will answer the next message
    /// with a `ResendRequest`, or refuse it as too low. Use it when the
    /// counterparty has *already* told you what they expect —
    /// *"our next is 4812"* — which is the case it exists for. When **you** are
    /// the one changing the number, [`Session::send_sequence_reset`] is the
    /// honest form.
    ///
    /// QuickFIX's `setNextSenderMsgSeqNum` behaves the same way, and this is
    /// named to match rather than to improve on it.
    ///
    /// Returns `false` and changes nothing for `n == 0`: there is no `34=0`.
    pub const fn set_next_out(&mut self, n: u32) -> bool {
        if n == 0 {
            return false;
        }
        self.next_out = n;
        true
    }

    /// Set the number this session next expects to receive. **Local only, and
    /// unlike [`Session::set_next_out`] it is not a lie** — what you expect is
    /// your own business, and the counterparty never learns it except by
    /// whether you accept what they send.
    ///
    /// Lowering it invites a duplicate; raising it skips messages without a
    /// `ResendRequest` and they are gone. Both are sometimes what the operator
    /// means.
    ///
    /// Returns `false` and changes nothing for `n == 0`.
    pub const fn set_next_in(&mut self, n: u32) -> bool {
        if n == 0 {
            return false;
        }
        self.next_in = n;
        true
    }

    /// Tell the counterparty that the next message will carry `n`, and make it
    /// so — `35=4` with `123=N`.
    ///
    /// **The honest way to change an outbound number.** The reset itself goes
    /// out at the current number, and `next_out` becomes `n` after it, which is
    /// what `36=n` promises.
    ///
    /// A reset that moves the number **down** is legal and is a last resort:
    /// the counterparty will accept numbers it has already seen, so anything
    /// it kept for a resend is now ambiguous. Nothing here prevents it, because
    /// an operator on the phone at 3 a.m. sometimes needs exactly that.
    ///
    /// Returns `false` and sends nothing for `n == 0`, or if the session has no
    /// output buffer to build the message in.
    pub fn send_sequence_reset<F: FnMut(&[u8])>(&mut self, n: u32, mut emit: F) -> bool {
        if n == 0 {
            return false;
        }
        let mut new_seq = [0u8; 10];
        let new_seq = digits(n, &mut new_seq);
        if self
            .send_as(
                Which::GapFill,
                None,
                &[(tag::NEW_SEQ_NO, new_seq), (tag::GAP_FILL_FLAG, b"N")],
                &mut emit,
            )
            .is_err()
        {
            return false;
        }
        self.next_out = n;
        true
    }

    /// [`Self::send_sequence_reset`], telling `journal` the number the reset
    /// itself spent.
    ///
    /// **The reset goes out at the old number and `next_out` becomes the new
    /// one**, so the journal is told the higher of the two — which is what a
    /// restart must continue from. `mark_out` never goes backwards, so an
    /// operator winding the count *down* does not lower what the journal knows;
    /// that is deliberate, and it is the safe direction: the counterparty has
    /// already seen the numbers above. ADR-0036, ADR-0053.
    pub fn send_sequence_reset_with<J: Journal, F: FnMut(&[u8])>(
        &mut self,
        n: u32,
        journal: &mut J,
        emit: F,
    ) -> bool {
        let sent = self.send_sequence_reset(n, emit);
        self.tell_journal(journal);
        sent
    }

    // ---- what an operator can order this session to say --------------------
    //
    // Three functions with one shape, and the shape is the point. Each takes an
    // **intent** and never bytes: the session builds the message from its own
    // `Template` and keeps `8`, `9`, `34`, `49`, `52`, `56` and `10` for itself.
    //
    // A back door taking whole message bytes would have been less code and is
    // the reason these exist instead. `crates/session/tests/mirror.rs` drives
    // them from the mirrored corpus, and a back door there would have made that
    // gate compare the corpus with itself.
    //
    // `[measured 2026-08-30]` 46 of the 50 mirrorable definitions need at least
    // one message that nothing on the wire asks for and no clock produces. That
    // is the whole reason a pure state machine is not enough for an initiator —
    // see the `session-initiator` plan, Sửa 2.

    /// Send a `Heartbeat (35=0)` nobody asked for.
    ///
    /// **Not the heartbeat rule.** [`Self::tick`] sends one when `HeartBtInt`
    /// has elapsed and this is the operator asking for one anyway — a keepalive
    /// through a device that times a connection out faster than the session
    /// does. It carries no `112=`, because it answers nothing.
    ///
    /// Returns `false` and sends nothing unless the session is logged on, or if
    /// the message cannot be laid out.
    pub fn send_heartbeat<F: FnMut(&[u8])>(&mut self, mut emit: F) -> bool {
        if self.state != State::LoggedOn {
            return false;
        }
        self.send(Which::Heartbeat, &[], &mut emit).is_ok()
    }

    /// Send a `TestRequest (35=1)` carrying `id` as `112=`.
    ///
    /// **The `id` is the caller's and is written through unchanged.** The
    /// session has a `TestReqID` of its own for the request [`Self::tick`]
    /// raises after silence; this one is not it. A counterparty answers with a
    /// `Heartbeat` echoing `112=`, so an operator who chose the string can tell
    /// their own answer from a heartbeat that was merely due.
    ///
    /// Nothing is remembered: matching the answer is the caller's, because a
    /// session that waited for it would need a timeout, and a timeout is a
    /// clock this layer does not own. `GUIDE.md` carries that.
    ///
    /// Returns `false` and sends nothing unless the session is logged on, or if
    /// the message cannot be laid out — an `id` too long for the buffer, for
    /// one.
    pub fn send_test_request<F: FnMut(&[u8])>(&mut self, id: &[u8], mut emit: F) -> bool {
        if self.state != State::LoggedOn {
            return false;
        }
        self.send(Which::TestRequest, &[(tag::TEST_REQ_ID, id)], &mut emit)
            .is_ok()
    }

    /// Send a `ResendRequest (35=2)` asking for `from` through `to`.
    ///
    /// **`to == 0` is not an empty range** — FIX 4.4 spells *"and everything
    /// after"* as `16=0`, and it is the form a session recovering from a gap
    /// needs. It is passed through rather than rejected.
    ///
    /// This end's own gap detection already sends one of these by itself; this
    /// is the operator asking for a range nothing detected, which is what a
    /// counterparty's *"we lost your 40 through 60"* phone call turns into.
    ///
    /// Returns `false` and sends nothing unless the session is logged on, or if
    /// the message cannot be laid out.
    pub fn send_resend_request<F: FnMut(&[u8])>(
        &mut self,
        from: u32,
        to: u32,
        mut emit: F,
    ) -> bool {
        if self.state != State::LoggedOn {
            return false;
        }
        let mut a = [0u8; 10];
        let mut b = [0u8; 10];
        let begin = digits(from, &mut a);
        let end = digits(to, &mut b);
        self.send(
            Which::ResendRequest,
            &[(tag::BEGIN_SEQ_NO, begin), (tag::END_SEQ_NO, end)],
            &mut emit,
        )
        .is_ok()
    }

    /// True once a Logon has been accepted.
    #[must_use]
    pub const fn is_logged_on(&self) -> bool {
        matches!(self.state, State::LoggedOn)
    }

    /// A counterparty opened the connection.
    pub fn connect<F: FnMut(&[u8])>(&mut self, emit: F) -> Link {
        let _ = emit;
        self.state = State::AwaitingLogon;
        // A live session has nothing to explain, and a stale cause read as a
        // current one is worse than no cause at all.
        self.last_drop_reason = None;
        // Both deadlines belong to a connection, not to a session: an instant
        // carried across a reconnect would expire the new connection on the old
        // one's clock, which only moves forward. Guarded by
        // `tests/heartbeat.rs::a_reconnect_gets_a_whole_new_logon_deadline`,
        // written **because** removing these two lines broke nothing.
        self.awaiting_logon_since_ms = None;
        self.logging_out_since_ms = None;
        // **A new connection starts a new count; a resumed session does not.**
        // ADR-0010: FIX 4.4 numbers a session, not a connection, so a session
        // that outlived its process must keep counting — but a session that
        // never persisted anything has nothing to continue, and
        // `2i_BeginStringValueUnexpected.def` logs on twice expecting `34=1`
        // back both times. The corpus builds every session with
        // [`Self::new`], so it takes this branch and needs no exemption.
        //
        // The counterparty can still force a reset from the wire, and that is
        // what `141=Y` is for — the only thing that restarts a resumed
        // session's numbers. See the Logon path.
        //
        // `ResetOnLogon=Y` is the operator saying the resumed count is not
        // wanted this morning, so it takes the same branch. That is the only
        // case it changes: a session built with `new` restarts here anyway.
        if !self.resumed || self.cfg.reset.resets_on_logon() {
            self.next_out = 1;
            self.next_in = 1;
        }
        // The heartbeat clock is deliberately **not** reset here. Every field
        // it uses is written again before it can be read: `beat_ms`,
        // `last_recv_ms` and `test_requests` by the Logon this connection must
        // now send, `last_sent_ms` by the reply. Clearing them as well changed
        // nothing when it was reversed, and a line that cannot be broken is
        // not a guard.
        // An initiator speaks first — but not here. `connect` has no clock,
        // and a Logon carries a `52=`; time enters this layer through `tick`
        // and nowhere else (D1). So this records whose turn it is and the next
        // tick does the speaking, which is also what an engine does: connect,
        // then turn the loop.
        if R::SPEAKS_FIRST {
            self.state = State::MustLogon;
            return Link::Up;
        }
        Link::Up
    }

    /// A counterparty closed the connection.
    pub fn disconnect<F: FnMut(&[u8])>(&mut self, emit: F) -> Link {
        self.disconnect_with(DropReason::TransportClosed, emit)
    }

    /// Record why the **engine** is ending this session, without ending it here.
    ///
    /// For the backpressure paths, which send their own `Logout` and then let
    /// the connection close on the next turn: the reason has to be recorded
    /// while it is known, and the state change happens elsewhere. Like
    /// [`Self::disconnect_with`], a cause already known is not replaced.
    pub const fn note_drop_reason(&mut self, why: DropReason) {
        if self.last_drop_reason.is_none() {
            self.last_drop_reason = Some(why);
        }
    }

    /// As [`Self::disconnect`], naming a reason the **engine** knows and this
    /// layer cannot.
    ///
    /// A duplicate identity, a full application ring, output backed up past the
    /// policy — none of those is a protocol fault and none of them reaches a
    /// session, so without this the engine would have to report them as *"the
    /// socket went away"*.
    ///
    /// `[measured 2026-09-02]` **that is exactly what it did**, and
    /// `tests/events.rs` caught it: three connections refused by the
    /// single-logon rule all reported `TransportClosed`, blaming the network
    /// for a policy decision. An operator reading that would have gone looking
    /// at the wrong layer.
    pub fn disconnect_with<F: FnMut(&[u8])>(&mut self, why: DropReason, emit: F) -> Link {
        let _ = emit;
        // **A cause already known is not replaced.** A session that refused a
        // `Logon` closes its socket immediately afterwards, and the close would
        // otherwise overwrite the reason it was refused for.
        if self.last_drop_reason.is_none() {
            self.end(why);
        } else {
            self.state = State::Disconnected;
        }
        Link::Dropped
    }

    /// Time passed. `now_ms` is milliseconds since 0000-01-01 — see [`clock`].
    ///
    /// This is the only thing that makes a session speak without being spoken
    /// to, and the three thresholds are QuickFIX's, in QuickFIX's order:
    ///
    /// | Silence since the last message arrived | Answer |
    /// |---|---|
    /// | ≥ 2.4 × `HeartBtInt` | give up the link |
    /// | ≥ 1.2 × (n+1) × `HeartBtInt` | send a `TestRequest`, n being how many are already outstanding |
    /// | otherwise, and ≥ 1 × `HeartBtInt` since *we* last spoke | send a `Heartbeat` |
    ///
    /// **At most one message leaves per call**, which is what makes the corpus
    /// readable: `6_SendTestRequest.def` expects exactly one `E` line per
    /// interval of waiting, and a tick that sent both a test request and a
    /// heartbeat would put two where the file allows one.
    /// **`[2026-09-04]` A resend in progress continues here**, which is why
    /// [`Self::tick_with`] exists beside this: answering a `ResendRequest` is
    /// now spread over several calls, and time is the only thing that reaches
    /// a session with nothing arriving on it. This form passes
    /// [`journal::NoJournal`], so a caller with no journal keeps the
    /// signature it had.
    pub fn tick<F: FnMut(&[u8])>(&mut self, now_ms: u64, mut emit: F) -> Link {
        // **It does not advance a replay, and must not.** Continuing one
        // through [`NoJournal`](journal::NoJournal) would answer every
        // remaining number with a gap fill — one `SequenceReset` covering the
        // lot — and the counterparty would lose messages this end still holds,
        // because the caller happened to use the journal-less form of a
        // timer tick. Stalling is recoverable; filling is not.
        // `a_replay_stalls_on_the_journal_less_tick_and_says_nothing_wrong`.
        self.tick_inner(now_ms, &mut emit)
    }

    /// [`Self::tick`], with the journal an outstanding replay reads from.
    ///
    /// The same in every other respect. A caller that answers `ResendRequest`s
    /// from a real journal must use this one, or a replay longer than one batch
    /// stalls until the next inbound message.
    /// `[2026-09-05]` **The journal is `&mut` here, and it used to be `&`.** A
    /// tick is where a `Heartbeat` and a `TestRequest` are born, and each one
    /// spends a number the journal keeps no bytes for; telling it is a write.
    /// ADR-0053.
    pub fn tick_with<J: Journal, F: FnMut(&[u8])>(
        &mut self,
        now_ms: u64,
        journal: &mut J,
        mut emit: F,
    ) -> Link {
        let link = self.tick_after_inner(now_ms, journal, &mut emit);
        // **After every path out, not only this one** — see `tell_journal`.
        self.tell_journal(journal);
        link
    }

    fn tick_after_inner<J: Journal, F: FnMut(&[u8])>(
        &mut self,
        now_ms: u64,
        journal: &J,
        emit: &mut F,
    ) -> Link {
        let link = self.tick_inner(now_ms, emit);
        if link == Link::Up && self.owed.is_some() {
            if let Err(why) = self.continue_replay(journal, emit) {
                self.end(why.into());
                return Link::Dropped;
            }
        }
        link
    }

    /// Tell `journal` how far the outbound count has got.
    ///
    /// **The number is already here**: `next_out - 1` is the highest number
    /// this session has spent, whether it went out on an `ExecutionReport` the
    /// journal kept or on a `Heartbeat` it never heard about. So there is no
    /// new state, and no way for a session's own count and the journal's to
    /// drift — one is computed from the other.
    ///
    /// Called from every entry point that holds a journal, **on every path
    /// out**. That last part is not a detail: `received_with` returns early
    /// when the counterparty's `Logout` has been answered, and that is exactly
    /// the message whose number went missing. ADR-0053.
    ///
    /// `mark_out` is a high-water mark, so calling this on a turn that sent
    /// nothing is a comparison and no more — `FileJournal::mark_out` returns
    /// before it touches a disk.
    fn tell_journal<J: Journal>(&self, journal: &mut J) {
        // `next_out` is the *next* number, so the highest spent is one below —
        // and nothing has been spent before the first message, when it is 1.
        if self.next_out > 1 {
            journal.mark_out(self.next_out - 1);
        }
    }

    fn tick_inner<F: FnMut(&[u8])>(&mut self, now_ms: u64, emit: &mut F) -> Link {
        self.now_ms = now_ms;
        // Before anything else, because a boundary that passed while this end
        // was asleep must land **ahead** of the numbering, not behind it.
        // Neutral under `Schedule::always`, which is the default.
        self.roll_if_a_boundary_passed(now_ms);
        // The window shut on a live session. A `Logout` with no `58=`: FIX
        // makes the text optional and QuickFIX sends none here either, and
        // there is no session-level text for *"we are closed"* that would not
        // be inventing one. **The counterparty learns nothing about why**, and
        // that is `STATUS.md` item 30 (d)'s job, not this one's.
        if self.state == State::LoggedOn && !self.cfg.schedule.contains(now_ms) {
            let _ = self.send(Which::Logout, &[], &mut *emit);
            self.last_drop_reason = Some(DropReason::ScheduleClosed);
            self.state = State::AwaitingLogout;
            return Link::Dropped;
        }
        match self.state {
            State::Disconnected | State::AwaitingLogout => return Link::Dropped,
            // `LoggingOut` deliberately falls through to the heartbeat rules
            // below: a counterparty that never answers our `Logout` must not
            // hold the connection open for ever.
            // Before a Logon there is no agreed interval, so the heartbeat
            // rules have nothing to measure — and this is the **only** thing
            // that says so, which is why `connect` no longer clears the clock
            // as well. No acceptance definition tests a logon timeout.
            // `tests/heartbeat.rs` holds this.
            //
            // `[2026-09-05]` A deadline the caller *states* is a different
            // thing from an interval the counterparty agreed to, and this is
            // where it is measured. Off unless configured, which is why the
            // sentence above still holds for every session the corpus builds.
            State::AwaitingLogon => {
                let since = *self.awaiting_logon_since_ms.get_or_insert(now_ms);
                if self.cfg.logon_timeout_ms != 0
                    && now_ms.saturating_sub(since) >= self.cfg.logon_timeout_ms
                {
                    // No message: there is no agreed session to say anything
                    // on, and a `Logout` before a `Logon` is not FIX.
                    self.end(DropReason::LogonTimedOut);
                    return Link::Dropped;
                }
                return Link::Up;
            }
            // This end owes the Logon, and now it has a clock to date it with.
            State::MustLogon => {
                self.state = State::AwaitingLogon;
                let mut beat = [0u8; 10];
                let beat = digits(self.cfg.heart_bt_int, &mut beat);
                self.beat_ms = u64::from(self.cfg.heart_bt_int) * 1_000;
                self.last_recv_ms = now_ms;
                let _ = self.send(
                    Which::Logon,
                    &[(tag::ENCRYPT_METHOD, b"0"), (tag::HEART_BT_INT, beat)],
                    &mut *emit,
                );
                return Link::Up;
            }
            State::LoggedOn | State::LoggingOut => {}
        }
        // The goodbye is out and the answer is owed. `[2026-09-05]` Until this
        // existed the only bound was the heartbeat rules below — 2.4 ×
        // `HeartBtInt`, 72 seconds on a default session — so a venue that took
        // the `Logout` and then died held the socket through the close.
        //
        // Ends without saying anything: the `Logout` has already gone out, and
        // a second one is wrong on the wire for the same reason the inbound
        // path answers a goodbye only when it did not start the exchange.
        if self.state == State::LoggingOut {
            let since = *self.logging_out_since_ms.get_or_insert(now_ms);
            if self.cfg.logout_timeout_ms != 0
                && now_ms.saturating_sub(since) >= self.cfg.logout_timeout_ms
            {
                self.end(DropReason::LogoutTimedOut);
                return Link::Dropped;
            }
        }
        // `108=0` means the counterparty asked for no heartbeats at all.
        if self.beat_ms == 0 {
            return Link::Up;
        }
        let quiet = now_ms.saturating_sub(self.last_recv_ms);
        let silent = now_ms.saturating_sub(self.last_sent_ms);

        if quiet >= self.beat_ms * 24 / 10 {
            self.end(DropReason::HeartbeatTimeout);
            return Link::Dropped;
        }
        if quiet >= self.beat_ms * 12 * u64::from(self.test_requests + 1) / 10 {
            self.test_requests += 1;
            let _ = self.send(
                Which::TestRequest,
                &[(tag::TEST_REQ_ID, OWN_TEST_REQ_ID)],
                &mut *emit,
            );
        }
        // An unanswered `TestRequest` silences the heartbeat until it is
        // answered — `needHeartbeat` in QuickFIX carries `testRequest() == 0`.
        // The counter was incremented above, so this also makes the branch
        // above and this one mutually exclusive: one message per tick at most.
        //
        // **The corpus cannot see this rule.** Its harness ticks a whole
        // interval at a time, so the test request and the timeout land on
        // consecutive ticks with nothing in between. `tests/heartbeat.rs` ticks
        // by the millisecond and is what holds it.
        if self.test_requests == 0 && silent >= self.beat_ms {
            let _ = self.send(Which::Heartbeat, &[], &mut *emit);
        }
        Link::Up
    }

    /// A frame the codec could not read at all.
    ///
    /// QuickFIX identifies the type out of the raw bytes and hangs up **only**
    /// if it says Logon; anything else is logged and ignored, and the
    /// counterparty carries on from where it was.
    /// `1d_InvalidLogonLengthInvalid.def` is the Logon half — one bad `9=` and
    /// the link goes — and `2d`, `3c` and `2t` are the other, where the
    /// following message must be read as though the garbled one never arrived.
    fn garbled(&mut self, bytes: &[u8]) -> Link {
        if msg_type_of(bytes) == Some(msg::LOGON) {
            self.end(DropReason::LogonIncomplete);
            return Link::Dropped;
        }
        Link::Up
    }

    /// One whole message arrived. Framing is the transport's job.
    pub fn received<F: FnMut(&[u8])>(&mut self, bytes: &[u8], emit: F) -> Link {
        self.received_with(bytes, &mut Silent, &mut NoJournal, emit)
    }

    /// One whole message arrived, with an [`Application`] to hand it to.
    ///
    /// The same as [`Self::received`] in every other respect.
    pub fn received_with<A: Application, J: Journal, F: FnMut(&[u8])>(
        &mut self,
        bytes: &[u8],
        app: &mut A,
        journal: &mut J,
        mut emit: F,
    ) -> Link {
        let link = self.received_inner(bytes, app, journal, &mut emit);

        // **After delivery, never before** — ADR-0017. Writing it before the
        // application ran would mean an ill-timed crash *loses* the message:
        // this end has counted it, so it never asks for a resend and the
        // counterparty believes it arrived. Writing it after means the message
        // is delivered twice, and the second copy carries `43=Y` because it
        // comes from a `ResendRequest` this end issued. FIX has a flag for the
        // second failure and none for the first.
        //
        // **Out here rather than inside the body, and `[measured 2026-09-05]`
        // that is a fix rather than a tidy-up.** It used to sit after the
        // drain, which `judge` answering `Link::Dropped` returns before — and
        // the counterparty's own `Logout` is judged exactly that way. So the
        // number it arrived under was consumed and never marked; a resumed
        // session expected it again, the counterparty's next message was one
        // too high, and this end sent a `ResendRequest` for a message it
        // already had. **The interop gate found it, not this repository's own
        // tests**: with the outbound half of ADR-0053 fixed, the clean-logout
        // scenario reached far enough to read `35=2: 1` where it wanted none.
        // `crates/session/tests/numbering.rs` holds it now.
        //
        // `next_in` is the *next* expected number, so the highest consumed is
        // one below — and nothing is marked before the first message, when the
        // count is still 1. `mark_in` takes the max, so the paths that return
        // without consuming anything re-state a number the journal has.
        if self.next_in > 1 {
            journal.mark_in(self.next_in - 1);
        }
        // The outbound twin, and it is here for the same reason: a `Logout`
        // answered gives `Link::Dropped` straight out of `judge`, and the
        // answer spent a number. That is the clean logout `STATUS.md` item 48
        // is about. ADR-0053.
        self.tell_journal(journal);
        link
    }

    fn received_inner<A: Application, J: Journal, F: FnMut(&[u8])>(
        &mut self,
        bytes: &[u8],
        app: &mut A,
        journal: &mut J,
        emit: &mut F,
    ) -> Link {
        // Once the link is down, bytes still arrive — the counterparty's own
        // Logout crossing ours, or anything already in flight. Reading them is
        // free; answering them would put a message on a connection this end has
        // already given up, and the corpus catches it as unexpected output.
        if matches!(self.state, State::Disconnected | State::AwaitingLogout) {
            return Link::Dropped;
        }
        let link = match self.judge(bytes, app, journal, emit) {
            Ok(link) => link,
            Err(why) => {
                self.end(why.into());
                Link::Dropped
            }
        };
        if link == Link::Dropped {
            return link;
        }
        // A message that ran ahead of the count was held rather than answered.
        // Now that this one has been judged the gap may have closed, and the
        // held messages are due — in sequence order, which is the whole point.
        // Draining here rather than inside `judge` keeps the recursion out:
        // `judge` never calls this, so a held message that queues another
        // cannot nest.
        let link = self.drain(app, journal, emit);

        // **A resend in progress moves forward on every message too**, not
        // only on ticks. A counterparty that keeps talking gets its replay at
        // the rate its own traffic drives, and one that goes quiet gets it from
        // `tick_with`. Placed after `drain` so a held message that closes a gap
        // is delivered before the replay it may have been waiting behind.
        if link != Link::Dropped && self.owed.is_some() {
            if let Err(why) = self.continue_replay(journal, emit) {
                self.end(why.into());
                return Link::Dropped;
            }
        }
        link
    }

    /// Judge every held message the count has caught up with.
    fn drain<A: Application, J: Journal, F: FnMut(&[u8])>(
        &mut self,
        app: &mut A,
        journal: &mut J,
        emit: &mut F,
    ) -> Link {
        // Bounded by the queue itself, and every round empties one slot.
        for _ in 0..QUEUED {
            let Some(i) = (0..QUEUED).find(|&i| self.queue[i].seq == self.next_in) else {
                return Link::Up;
            };
            // Copied to the stack because `judge` takes `&mut self`. 512 bytes
            // and no allocation — non-negotiable 1 — and it happens only when
            // a gap closes, which is not the common path.
            let len = usize::from(self.queue[i].len);
            let mut held = [0u8; QUEUED_LEN];
            held[..len].copy_from_slice(&self.queue[i].buf[..len]);
            self.queue[i].seq = 0;
            match self.judge(&held[..len], app, journal, emit) {
                Ok(Link::Up) => {}
                Ok(Link::Dropped) => return Link::Dropped,
                Err(why) => {
                    self.end(why.into());
                    return Link::Dropped;
                }
            }
        }
        Link::Up
    }

    /// Hold a message that arrived ahead of the count.
    ///
    /// Silently drops it when there is no room, or when it is longer than a
    /// slot. That is not a hole in the protocol: the message was never
    /// acknowledged, `next_in` did not move, and the next message running ahead
    /// asks for it again. Losing it costs a round trip, and the alternative is
    /// an allocation on the receive path.
    fn enqueue(&mut self, seq: u32, bytes: &[u8]) {
        if bytes.len() > QUEUED_LEN {
            return;
        }
        let Some(i) = (0..QUEUED).find(|&i| self.queue[i].seq == seq || self.queue[i].seq == 0)
        else {
            return;
        };
        self.queue[i].seq = seq;
        self.queue[i].len = bytes.len() as u16;
        self.queue[i].buf[..bytes.len()].copy_from_slice(bytes);
    }

    /// Move the inbound count past `seq`, and close the gap if this was the
    /// last message it was waiting on.
    ///
    /// **The corpus never sees the closing.** Every file that opens a gap ends
    /// before opening a second one, so a session that asks once and then never
    /// asks again scores the same. Leaving it open would strand a real session
    /// on its next gap, in silence. `tests/resend.rs` holds it.
    fn advance_past(&mut self, seq: u32) {
        self.next_in = seq + 1;
        if self.resend_from != 0 && seq >= self.resend_to {
            self.resend_from = 0;
            self.resend_to = 0;
        }
    }

    /// A message running ahead of the count: hold it, and ask for the gap.
    fn too_high<F: FnMut(&[u8])>(&mut self, seq: u32, bytes: &[u8], emit: &mut F) -> Link {
        self.enqueue(seq, bytes);
        // **Ask once per gap.** `10_MsgSeqNumGreater.def` sends two messages
        // running ahead and expects exactly one `ResendRequest`; a second would
        // be output no line asked for, and the file fails on it.
        if self.resend_from != 0 && seq >= self.resend_from {
            return Link::Up;
        }
        self.resend_from = self.next_in;
        self.resend_to = seq - 1;
        let mut begin = [0u8; 10];
        let begin = digits(self.next_in, &mut begin);
        // `16=0` — "and everything after". FIX 4.2 and later say it that way;
        // 4.0 and 4.1 wrote `999999`, and this engine is 4.4 only.
        if self
            .send(
                Which::ResendRequest,
                &[(tag::BEGIN_SEQ_NO, begin), (tag::END_SEQ_NO, b"0")],
                emit,
            )
            .is_err()
        {
            self.end(DropReason::CannotSend);
            return Link::Dropped;
        }
        Link::Up
    }

    /// Write one templated message and hand it to `emit`.
    ///
    /// The template is chosen by `which`, not by a field order at this call
    /// site — non-negotiable 5. `34=` and `52=` are filled here because they
    /// are the two the session owns on every message it sends.
    /// Send a Logout carrying `58=`, then give up the link.
    ///
    /// The corpus is specific about which refusals get one. At Logon time there
    /// is no session to say goodbye to and the answer is silence
    /// (`1d`, `1e`); once logged on, the same fault gets a Logout with a reason
    /// (`2c`, `2i`). Same rule, different state.
    fn logout_with<F: FnMut(&[u8])>(&mut self, why: SessionText, emit: &mut F) -> Link {
        let mut text = [0u8; SessionText::MAX_LEN];
        let sent = why
            .render(&mut text)
            .map(|n| self.send(Which::Logout, &[(tag::TEXT, &text[..n])], emit));
        // A text that would not render is a bug in the table, not a reason to
        // keep the connection: either way this end is finished.
        let _ = sent;
        self.state = State::AwaitingLogout;
        Link::Dropped
    }

    /// Send a `Logout` carrying `58=text`, then give up the link.
    ///
    /// **For the engine, and for one policy: `DESIGN.md` D10.** A queue that
    /// has filled because the counterparty stopped reading is not something a
    /// pure state machine can see — there is no socket here — so the decision
    /// belongs to the engine. The *message* still belongs to the session, which
    /// owns the sequence number, the timestamp and the field order.
    ///
    /// `text` is written straight into `58=`; the caller supplies a literal, so
    /// nothing here allocates or formats.
    /// Say goodbye and **wait to be answered**.
    ///
    /// The ordered-shutdown counterpart of [`Session::logout_now`], and the
    /// difference is the whole point: this returns [`Link::Up`], so the caller
    /// keeps turning until the counterparty's own `Logout` arrives — at which
    /// point the ordinary path ends the session with
    /// [`DropReason::PeerLogout`] — or until the caller gives up.
    ///
    /// `logout_now` is left alone rather than given a flag. It is D10's path,
    /// where cutting immediately is the right answer, and one function serving
    /// both is how both come to be wrong.
    ///
    /// **The caller owns the deadline.** A counterparty that has already died
    /// never answers, and nothing here can tell that apart from one that is
    /// merely slow.
    ///
    /// Returns [`Link::Dropped`] and sends nothing if this session has already
    /// gone, or is already waiting for a `Logout` it asked for.
    pub fn begin_logout<F: FnMut(&[u8])>(&mut self, text: &[u8], mut emit: F) -> Link {
        // **Only a logged-on session has anything to say goodbye to.** FIX has
        // no `Logout` before a `Logon`, so a connection that never got that far
        // is ended here rather than sent a message it should not receive — and
        // it is ended with a reason, because a shutdown that closed sockets
        // anonymously would show up on the event stream as
        // `EndedWithoutReason`.
        if self.state != State::LoggedOn {
            if self.state != State::Disconnected {
                self.end(DropReason::EngineShutdown);
            }
            return Link::Dropped;
        }
        // **No words means no field, not an empty one.** `[measured 2026-09-02]`
        // `begin_logout(b"")` wrote `58=` with nothing after it — a field on
        // the wire that says nothing, and a field count no counterparty
        // expects. An unset slot is simply not written (`out.rs`), so the fix
        // is to pass no slot rather than an empty value.
        let extra: &[(u32, &[u8])] = if text.is_empty() {
            &[]
        } else {
            &[(tag::TEXT, text)]
        };
        if self.send(Which::Logout, extra, &mut emit).is_err() {
            // Nothing went out, so there is nothing to wait for. Ending here
            // is honest; pretending to wait would hang the shutdown on a
            // message that was never sent.
            self.end(DropReason::CannotSend);
            return Link::Dropped;
        }
        self.state = State::LoggingOut;
        Link::Up
    }

    /// [`Self::begin_logout`], telling `journal` the number the goodbye spent.
    ///
    /// The ordered-shutdown twin of [`Self::logout_now_with`], and it matters
    /// for the same reason: a process shutting down cleanly spends a number on
    /// its way out, and a journal that never heard about it sends the next
    /// process back one short — which is what a real `libquickfix` refused.
    /// ADR-0053.
    pub fn begin_logout_with<J: Journal, F: FnMut(&[u8])>(
        &mut self,
        text: &[u8],
        journal: &mut J,
        emit: F,
    ) -> Link {
        let link = self.begin_logout(text, emit);
        self.tell_journal(journal);
        link
    }

    pub fn logout_now<F: FnMut(&[u8])>(&mut self, text: &[u8], mut emit: F) -> Link {
        if matches!(self.state, State::Disconnected | State::AwaitingLogout) {
            return Link::Dropped;
        }
        let _ = self.send(Which::Logout, &[(tag::TEXT, text)], &mut emit);
        self.state = State::AwaitingLogout;
        Link::Dropped
    }

    /// [`Self::logout_now`], telling `journal` the number the goodbye spent.
    ///
    /// **The same message, and a different thing survives it.** This path is
    /// the last thing a dying process does — D10's backpressure cut and the
    /// engine's `SIGTERM` — so the number it spends is precisely the one a
    /// restart needs and the one nothing else will record. A deployment that
    /// calls the journal-less form and then restarts asks the counterparty to
    /// accept a number it has already seen. ADR-0053.
    pub fn logout_now_with<J: Journal, F: FnMut(&[u8])>(
        &mut self,
        text: &[u8],
        journal: &mut J,
        emit: F,
    ) -> Link {
        let link = self.logout_now(text, emit);
        self.tell_journal(journal);
        link
    }

    /// Write one `Reject (35=3)` and hand it to `emit`.
    fn send_reject<F: FnMut(&[u8])>(&mut self, r: &Reject, emit: &mut F) -> Link {
        let mut text = [0u8; SessionText::MAX_LEN];
        let mut code = [0u8; 10];
        let mut seq = [0u8; 10];
        let mut extra: [(u32, &[u8]); 12] = [(0, &[]); 12];
        let mut n = 0usize;

        for (t, held) in r.route.iter().flatten() {
            extra[n] = (*t, held);
            n += 1;
        }
        if let Some(v) = r.ref_seq {
            let d = digits(v, &mut seq);
            extra[n] = (tag::REF_SEQ_NUM, d);
            n += 1;
        }
        let Some(len) = r.text.render(&mut text) else {
            // A text that will not render is a bug in the table, not a reason
            // to answer with a malformed Reject.
            self.end(DropReason::CannotSend);
            return Link::Dropped;
        };
        extra[n] = (tag::TEXT, &text[..len]);
        n += 1;
        if let Some(ref t) = r.ref_tag {
            extra[n] = (tag::REF_TAG_ID, t);
            n += 1;
        }
        if let Some(ref mt) = r.ref_msg_type {
            extra[n] = (tag::REF_MSG_TYPE, mt);
            n += 1;
        }
        if let Some(reason) = r.text.session_reject_reason() {
            let d = digits(reason, &mut code);
            extra[n] = (tag::SESSION_REJECT_REASON, d);
            n += 1;
        }

        if self.send(Which::Reject, &extra[..n], emit).is_err() {
            self.end(DropReason::CannotSend);
            return Link::Dropped;
        }

        // Two of the twelve reasons end the session as well as answering it.
        // `2k_CompIDDoesNotMatchProfile.def` and
        // `2o_SendingTimeValueOutOfRange.def` both expect a Reject **and then a
        // Logout**, and then wait for the counterparty's. The other ten leave
        // the session running — `14a` rejects four messages in a row on one
        // connection.
        if matches!(
            r.text,
            SessionText::CompIdProblem | SessionText::SendingTimeAccuracyProblem
        ) {
            let _ = self.send(Which::Logout, &[], emit);
            self.state = State::AwaitingLogout;
            return Link::Dropped;
        }
        Link::Up
    }

    fn send<F: FnMut(&[u8])>(
        &mut self,
        which: Which,
        extra: &[(u32, &[u8])],
        emit: &mut F,
    ) -> Result<(), Refusal> {
        self.send_as(which, None, extra, emit)
    }

    /// As [`Self::send`], but with the sequence number chosen by the caller.
    ///
    /// `Some(n)` writes `34=n` and leaves the outbound count alone: a gap fill
    /// stands in for numbers already spent, so it must not spend another.
    /// `8_OnlyAdminMessages.def` fills `34=1` while the next real message is
    /// `34=5`, and both are in the file.
    fn send_as<F: FnMut(&[u8])>(
        &mut self,
        which: Which,
        at: Option<u32>,
        extra: &[(u32, &[u8])],
        emit: &mut F,
    ) -> Result<(), Refusal> {
        // Unix milliseconds, because that is what `TimestampCache` takes and it
        // is `no_std` and shared with callers that have no session. Saturating:
        // a session ticked before 1970 is a misconfiguration, and reporting
        // 1970 is better than wrapping into the year 292 million.
        let unix = self.now_ms.saturating_sub(clock::MILLIS_YEAR_ZERO_TO_EPOCH);
        let stamp = *self.stamp.format(unix);
        let mut seq = [0u8; 10];
        let seq = digits(at.unwrap_or(self.next_out), &mut seq);

        let o = self.out.as_mut().ok_or(Refusal::CannotSend)?;
        let mut slots: [(u32, &[u8]); 16] = [(0, &[]); 16];
        slots[0] = (tag::MSG_SEQ_NUM, seq);
        slots[1] = (tag::SENDING_TIME, &stamp);
        let mut n = 2;
        for pair in extra {
            *slots.get_mut(n).ok_or(Refusal::CannotSend)? = *pair;
            n += 1;
        }

        // Destructured so the chosen template and the output buffer are two
        // disjoint borrows of one struct rather than two borrows of the whole.
        let Outbound {
            logon,
            logout,
            reject,
            heartbeat,
            test_request,
            resend_request,
            gap_fill,
            buf,
            app: _,
        } = o;
        let template = match which {
            Which::Logon => &*logon,
            Which::Logout => &*logout,
            Which::Reject => &*reject,
            Which::Heartbeat => &*heartbeat,
            Which::TestRequest => &*test_request,
            Which::ResendRequest => &*resend_request,
            Which::GapFill => &*gap_fill,
        };
        let range = template
            .encode(buf, &slots[..n])
            .map_err(|_| Refusal::CannotSend)?;
        emit(&buf[range]);
        if at.is_none() {
            self.next_out += 1;
        }
        self.last_sent_ms = self.now_ms;
        Ok(())
    }

    /// Originate an application message.
    ///
    /// The acceptor never needs this — everything it sends is an answer. An
    /// **initiator** does: nothing on the wire asks it to send an order.
    ///
    /// `msg` is the message the application wants sent; the session takes over
    /// what the application does not own — the sequence number and the clock —
    /// and keeps a copy for a later resend. `8=`, `9=`, `34=`, `52=` and `10=`
    /// on the input are ignored and rewritten; everything else is carried
    /// through and **ordered by `Fix44`**, never by the caller.
    ///
    /// A message that cannot be laid out is **not sent and not counted** — the
    /// same fail-closed answer the rest of this layer gives. `Refusal` is
    /// private on purpose: nothing a caller can do about it differs from doing
    /// nothing.
    pub fn send_application<J: Journal, F: FnMut(&[u8])>(
        &mut self,
        msg: &[u8],
        journal: &mut J,
        mut emit: F,
    ) -> Link {
        // Nothing to say before the Logon is agreed, and nothing after the
        // Logout. Silence rather than an error: an application that offers a
        // message to a session that is not up has not done anything wrong.
        if self.state != State::LoggedOn {
            return Link::Up;
        }
        let unix = self.now_ms.saturating_sub(clock::MILLIS_YEAR_ZERO_TO_EPOCH);
        let now = *self.stamp.format(unix);
        let mut seq = [0u8; 10];
        let seq = digits(self.next_out, &mut seq);
        let seq_out = self.next_out;

        let Some(o) = self.out.as_mut() else {
            return Link::Up;
        };
        let out::Outbound { app: buf, .. } = o;
        let Some(r) = rebuild(msg, Some(seq), &now, false, buf) else {
            return Link::Up;
        };
        if !journal.put(seq_out, &buf[r.clone()]) {
            self.puts_refused += 1;
        }
        emit(&buf[r]);

        self.next_out += 1;
        self.last_sent_ms = self.now_ms;
        // **Also when `put` was kept, and deliberately so.** A kept message
        // raises the journal's own outbound mark, so this is a comparison that
        // writes nothing — and when `put` refused, it is the only record that
        // the number was spent at all. One line instead of a branch that could
        // be got wrong. ADR-0053.
        self.tell_journal(journal);
        Link::Up
    }

    /// Is `seq` still in the journal?
    fn kept<J: Journal>(journal: &J, seq: u32) -> bool {
        journal.get(seq).is_some()
    }

    /// Send the kept message numbered `seq` again, as a resend of itself.
    ///
    /// `false` if it is not in the journal — the caller then fills over it.
    /// A replay **spends no sequence number**: it carries the one it was sent
    /// with, which is the whole point of a resend.
    fn replay<J: Journal, F: FnMut(&[u8])>(
        &mut self,
        seq: u32,
        journal: &J,
        emit: &mut F,
    ) -> Result<bool, Refusal> {
        let unix = self.now_ms.saturating_sub(clock::MILLIS_YEAR_ZERO_TO_EPOCH);
        let now = *self.stamp.format(unix);
        let Some(kept) = journal.get(seq) else {
            return Ok(false);
        };
        let o = self.out.as_mut().ok_or(Refusal::CannotSend)?;
        let out::Outbound { app: buf, .. } = o;
        let r = as_resend(kept, &now, buf).ok_or(Refusal::CannotSend)?;
        emit(&buf[r]);
        self.last_sent_ms = self.now_ms;
        Ok(true)
    }

    /// Put up to one batch of the outstanding `ResendRequest` on the wire.
    ///
    /// **Bounded by messages, not by numbers.** A replay is one message and a
    /// gap fill is one message however many numbers it covers, so the batch is
    /// a count of what reaches the transmit buffer — which is the thing that
    /// overflows. [ADR-0046](../../../docs/decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md)
    /// decision 4.
    ///
    /// Called after every judged message and every tick while anything is
    /// owed. Interleaving a replay with new traffic is legal and unambiguous:
    /// a replayed message carries its **original** `34=` and `43=Y`, and a new
    /// one carries the next new number.
    fn continue_replay<J: Journal, F: FnMut(&[u8])>(
        &mut self,
        journal: &J,
        emit: &mut F,
    ) -> Result<(), Refusal> {
        let Some(mut r) = self.owed else {
            return Ok(());
        };
        let budget = u32::from(self.cfg.resend_batch.max(1));
        let mut spent = 0;
        while spent < budget && r.next <= r.end {
            if self.replay(r.next, journal, emit)? {
                r.next += 1;
                spent += 1;
                continue;
            }
            // A run this end cannot replay — every administrative message is
            // one — is covered by a single gap fill.
            // `8_AdminAndApplicationMessages.def` asks for 2..=8 and expects
            // fill(2..5), 5, 6, fill(7..9): the runs are found, not assumed.
            let from = r.next;
            // **The part below the journal's floor needs no search.** Every
            // number under it is certainly gone, so the run jumps straight
            // there instead of asking the ring once per number — which at
            // 4096 slots and a `7=1` resend is the difference between one
            // comparison and thousands, on the engine thread.
            let floor = journal.oldest().unwrap_or(u32::MAX);
            let mut n = if from < floor {
                floor.min(r.end.saturating_add(1))
            } else {
                from.saturating_add(1)
            };
            // Above the floor the misses are administrative messages, which
            // are few and adjacent. `from` itself could not be replayed, so
            // the run is at least one long and this cannot stand still.
            while n <= r.end && !Self::kept(journal, n) {
                n += 1;
            }
            // **What this fill cost, counted before it is sent.** Only the
            // part below the floor: a number above it that `get` could not
            // answer was an administrative message, never replayed by anybody,
            // and not a loss. ADR-0046 decision 1.
            if journal.oldest().is_some() {
                self.resend_beyond_journal += n.min(floor).saturating_sub(from);
            }
            self.fill(from, n, emit)?;
            r.next = n;
            spent += 1;
        }
        self.owed = (r.next <= r.end).then_some(r);
        Ok(())
    }

    /// One `SequenceReset` gap fill covering `from..upto`, numbered `from`.
    fn fill<F: FnMut(&[u8])>(&mut self, from: u32, upto: u32, emit: &mut F) -> Result<(), Refusal> {
        let unix = self.now_ms.saturating_sub(clock::MILLIS_YEAR_ZERO_TO_EPOCH);
        let orig = *self.stamp.format(unix);
        let mut new_seq = [0u8; 10];
        let new_seq = digits(upto, &mut new_seq);
        self.send_as(
            Which::GapFill,
            Some(from),
            &[
                (tag::POSS_DUP_FLAG, b"Y"),
                (tag::ORIG_SENDING_TIME, &orig),
                (tag::NEW_SEQ_NO, new_seq),
                (tag::GAP_FILL_FLAG, b"Y"),
            ],
            emit,
        )
    }

    /// Read one message, decide, and answer. The order the checks run in is
    /// the order QuickFIX applies them, and it is load-bearing: two rules that
    /// share an outcome are indistinguishable to the corpus, so which one fires
    /// first is only visible to `tests/logon.rs`.
    fn judge<A: Application, J: Journal, F: FnMut(&[u8])>(
        &mut self,
        bytes: &[u8],
        app: &mut A,
        journal: &mut J,
        emit: &mut F,
    ) -> Result<Link, Refusal> {
        match parse_into::<Fix44, N>(bytes, &mut self.idx, Validation::ALL) {
            // A partial read is not a refusal: the next call brings the rest.
            Ok(Parsed::Incomplete) => return Ok(Link::Up),
            Ok(Parsed::Complete { .. }) => {}
            // `14a_BadField.def` sends `-1=HI`, which is not a tag and never
            // will be — so there is no number to put in `371=`, only the text
            // the counterparty wrote. The codec specifies that the index still
            // holds every field read *before* the bad one, which is how `34=`
            // and `35=` are still available to answer with.
            Err(ParseError::BadTag { at }) if self.state == State::LoggedOn => {
                let text = tag_text_at(bytes, at as usize);
                // **One `ParseError`, two answers.** QuickFIX's own tokeniser
                // reads a tag with a signed-integer conversion, so `-1` is a
                // field — an absurd one, which the dictionary then refuses —
                // while `4garbled9` is not a number in any base and the whole
                // message is unreadable.
                //
                // `14a_BadField.def` sends `-1=HI` and expects
                // `Reject 373=0` naming `371=-1`. `2d_GarbledMessage.def` and
                // `3c_GarbledMessage.def` send `4garbled9=TW` and expect the
                // message **ignored**, with the counterparty carrying on. The
                // difference is only in the text.
                if !text.is_some_and(is_signed_integer) {
                    return Ok(self.garbled(bytes));
                }
                let view = self.idx.view(bytes);
                let r = Reject {
                    text: SessionText::InvalidTagNumber,
                    ref_tag: copy::<12>(text),
                    ref_msg_type: copy::<8>(view.get(tag::MSG_TYPE)),
                    ref_seq: view.get(tag::MSG_SEQ_NUM).and_then(|v| as_u32(v).ok()),
                    route: [None, None, None, None, None, None],
                };
                if let Some(seq) = r.ref_seq
                    && seq >= self.next_in
                {
                    self.next_in = seq + 1;
                }
                return Ok(self.send_reject(&r, emit));
            }
            Err(_) => return Ok(self.garbled(bytes)),
        }

        // `MsgType` must be the third field. This codec deliberately leaves
        // that to the session — `ParseError::BadFrameStart` covers `8=` and
        // `9=` and says so — and QuickFIX's own parser treats a message that
        // breaks it as unreadable rather than as a rejectable fault.
        // `[measured]` `2t_FirstThreeFieldsOutOfOrder.def` is the only file in
        // the corpus that sends one, and it expects both to be ignored.
        if self.idx.view(bytes).field_at(2).map(|(t, _)| t) != Some(tag::MSG_TYPE) {
            return Ok(self.garbled(bytes));
        }

        let view = self.idx.view(bytes);
        let cfg = &self.cfg;

        if !view
            .get(tag::BEGIN_STRING)
            .is_some_and(|v| cfg.begin_string.matches(v))
        {
            if self.state == State::LoggedOn {
                return Ok(self.logout_with(SessionText::IncorrectBeginString, emit));
            }
            return Err(Refusal::WrongBeginString);
        }

        // Outside its hours an acceptor is not a FIX endpoint at all, so this
        // outranks every identity and sequence rule below — none of them is
        // meaningful about a session that is shut. Neutral under
        // `Schedule::always`, which is what every existing caller has.
        if !self.cfg.schedule.contains(self.now_ms) {
            return Err(Refusal::OutsideSchedule);
        }

        // "The first message must be a Logon" outranks the identity checks:
        // `1e_NotLogonMessage` sends `35=0` *and* a wrong `56=`, and its name
        // says which rule it is testing.
        if self.state == State::AwaitingLogon && view.get(tag::MSG_TYPE) != Some(msg::LOGON) {
            return Err(Refusal::NotALogon);
        }

        let sender_ok = view
            .get(tag::SENDER_COMP_ID)
            .is_some_and(|v| cfg.inbound_sender_matches(v));
        let target_ok = view
            .get(tag::TARGET_COMP_ID)
            .is_some_and(|v| cfg.inbound_target_matches(v));
        let stamp = view.get(tag::SENDING_TIME).and_then(clock::parse_utc);
        // Recorded before the verdict, not after: the refusal is the case this
        // number exists to explain.
        if let Some(t) = stamp {
            self.last_skew_ms = Some(if self.now_ms >= t {
                i64::try_from(self.now_ms - t).unwrap_or(i64::MAX)
            } else {
                i64::try_from(t - self.now_ms).map_or(i64::MIN, i64::wrapping_neg)
            });
        }
        let time_ok = stamp.is_some_and(|t| t.abs_diff(self.now_ms) <= cfg.max_skew_ms);

        // Before a Logon there is no session to answer with, so these are all
        // one thing: hang up in silence (`1c`, `1d`). Afterwards each has its
        // own `373` and the connection stays up (`2k`, `2o`). Same faults,
        // different answer, and the state is what decides.
        if self.state == State::AwaitingLogon {
            if !sender_ok {
                return Err(Refusal::WrongSenderCompId);
            }
            if !target_ok {
                return Err(Refusal::WrongTargetCompId);
            }
            if !time_ok {
                return Err(Refusal::BadSendingTime);
            }
        }

        // ---- the dictionary's questions, in the order the corpus answers them
        //
        // Order is not cosmetic. Every one of these ends in `eDISCONNECT`-free
        // Reject, so the corpus sees only *which* `373` came back — and several
        // messages are faulty in two ways at once:
        //
        // * `14d` sends `56=` — empty **and** a CompID mismatch. `373=4` wins,
        //   so the field scan runs before the CompID check.
        // * `14b` sends no `56=` at all — missing **and** a CompID mismatch.
        //   `373=1` wins, so required-field runs before CompID too.
        // * `2q` sends `35=*` — an invalid type, and every tag in the message is
        //   then "not defined for this message type". `373=11` wins, so MsgType
        //   is settled before any per-field question is asked.
        let msg_type_bytes = copy::<8>(view.get(tag::MSG_TYPE));
        let ref_seq = view.get(tag::MSG_SEQ_NUM).and_then(|v| as_u32(v).ok());
        let mut route: [Option<(u32, Held<32>)>; 6] = [None, None, None, None, None, None];
        for (i, (from, to)) in ROUTING.into_iter().enumerate() {
            // An empty routing tag is not echoed:
            // `ReverseRouteWithEmptyRoutingTags.def` sends `116=` with a good
            // `115=JCD` and the Reject carries `128=JCD` and nothing else.
            if let Some(v) = view.get(from).filter(|v| !v.is_empty())
                && let Some(held) = copy::<32>(Some(v))
            {
                route[i] = Some((to, held));
            }
        }

        let mt = view.get(tag::MSG_TYPE).unwrap_or_default();
        let fault = if self.state != State::LoggedOn {
            None
        } else if !Fix44::is_msg_type(mt) {
            Some((SessionText::InvalidMsgType, None))
        } else {
            scan_fields(&view, mt)
                .or_else(|| missing_required(&view, mt))
                .or_else(|| bad_group_count(&view, mt))
                // A CompID that is merely wrong, once there is a session to say
                // so with. `2k_CompIDDoesNotMatchProfile.def` sends all three
                // combinations and expects `373=9` for each.
                .or_else(|| {
                    (!sender_ok || !target_ok).then_some((SessionText::CompIdProblem, None))
                })
                .or_else(|| (!time_ok).then_some((SessionText::SendingTimeAccuracyProblem, None)))
        };

        // Everything below is decided from values already read out of `view`,
        // so the borrow of `self.idx` ends here and `send` can take `&mut self`.
        let msg_type = view.get(tag::MSG_TYPE).unwrap_or_default();
        // Read while the index is still borrowed, used after it is not.
        let is_application = !ADMIN.contains(&msg_type);
        let is_logon = msg_type == msg::LOGON;
        let is_logout = msg_type == msg::LOGOUT;
        let is_test_request = msg_type == msg::TEST_REQUEST;
        let is_sequence_reset = msg_type == msg::SEQUENCE_RESET;
        // `123=` defaults to `N` when absent — `11a` and `11b` leave it out and
        // their comments say "default to N".
        let gap_fill = view.get(tag::GAP_FILL_FLAG) == Some(b"Y");
        let new_seq_no = view.get(tag::NEW_SEQ_NO).and_then(|v| as_u32(v).ok());
        let is_resend_request = msg_type == msg::RESEND_REQUEST;
        let orig_sending_time = view.get(tag::ORIG_SENDING_TIME).and_then(clock::parse_utc);
        let sending_time = view.get(tag::SENDING_TIME).and_then(clock::parse_utc);
        let begin_seq_no = view.get(tag::BEGIN_SEQ_NO).and_then(|v| as_u32(v).ok());
        let end_seq_no = view.get(tag::END_SEQ_NO).and_then(|v| as_u32(v).ok());
        let reset_seq = view.get(tag::RESET_SEQ_NUM_FLAG) == Some(b"Y");
        // 64 bytes: the corpus's longest is `HELLO1`. A longer one is dropped
        // rather than truncated, and the reply then carries no `112=` at all —
        // wrong, but visibly wrong, which a truncation would not be.
        let test_req_id = copy::<64>(view.get(tag::TEST_REQ_ID));
        let seq = view
            .get(tag::MSG_SEQ_NUM)
            .and_then(|v| as_u32(v).ok())
            .ok_or(Refusal::BadSeqNum)?;
        let poss_dup = view.get(tag::POSS_DUP_FLAG) == Some(b"Y");
        let encrypt = copy::<8>(view.get(tag::ENCRYPT_METHOD));
        let heart_bt = copy::<8>(view.get(tag::HEART_BT_INT));

        // A Logon carrying `141=Y` restarts both counts **before** its own
        // sequence number is judged: QuickFIX resets in `nextLogon` and only
        // then verifies. `SessionReset.def` sends `34=1` to a session sitting
        // at 11 and expects `34=1` back.
        if is_logon && reset_seq {
            self.next_in = 1;
            self.next_out = 1;
        }

        // Which messages have their sequence number checked, and which advance
        // it, is per `MsgType` in QuickFIX — `verify(msg, checkTooHigh,
        // checkTooLow)` is called with different arguments from each handler,
        // and `nextSequenceReset` is the one that never advances at all.
        //
        // | `35=` | too high | too low | advances `34=` in |
        // |---|---|---|---|
        // | `A` Logon | **after the reply** | yes | only if not too high |
        // | `5` Logout | no | no | yes |
        // | `2` ResendRequest | no | no | yes |
        // | `4` SequenceReset, no gap fill | no | no | **no** |
        // | `4` SequenceReset, gap fill | yes | yes | **no** |
        // | everything else | yes | yes | yes |
        //
        // `10_MsgSeqNumEqual.def` proves the Logout row: it gap-fills to 20,
        // then logs out with `34=3`, and expects the ordinary Logout reply.
        // `11a`/`11b`/`11c` prove the SequenceReset rows: all three send
        // `34=0`, which is below every expectation there has ever been.
        // `8_OnlyAdminMessages.def` proves the ResendRequest row: it sends
        // `34=5` twice, and the second time the count has passed it.
        // `1a_ValidLogonMsgSeqNumTooHigh.def` proves the Logon row: `34=5` on
        // an empty session, answered with a Logon **and then** a
        // `ResendRequest`, in that order.
        let plain_reset = is_sequence_reset && !gap_fill;
        let skips_sequencing = is_logout || is_resend_request || plain_reset;
        let check_too_low = !skips_sequencing;
        let check_too_high = !skips_sequencing && !is_logon;

        // A sequence number already used cannot be taken back, so the session
        // hangs up. Too *high* means a gap, which is a ResendRequest and not a
        // refusal — that is a later step, and until it exists such a message is
        // read and answered as if it were in order.
        if check_too_low && seq < self.next_in {
            // `43=Y` says the counterparty knows it is re-sending. A repeat it
            // has admitted to is dropped in silence; a repeat it has not is the
            // one fault FIX cannot recover from, because a sequence number that
            // has been used cannot be taken back.
            //
            // `2e_PossDupNotReceived.def` holds both halves in one file, and
            // getting this wrong costs the *other* half: answering the admitted
            // repeat puts an extra Logout on the wire, and the file's real
            // Logout then compares against it.
            if poss_dup {
                // QuickFIX's `doPossDup`, and it is only reached here: a
                // `43=Y` message that is *not* behind the count is never asked
                // these two questions. `20_SimultaneousResendRequest.def`
                // sends three of them in order and none is challenged.
                //
                // A `SequenceReset` is exempt — a gap fill stands in for
                // messages, so there is no original send time to carry.
                if !is_sequence_reset {
                    let Some(orig) = orig_sending_time else {
                        // `2g_PossDupNoOrigSendingTime.def`: `373=1` naming
                        // tag 122, and the sequence number is **not** spent —
                        // the file's next message is numbered as though this
                        // one never arrived.
                        let r = Reject {
                            text: SessionText::RequiredTagMissing,
                            ref_tag: tag_text(tag::ORIG_SENDING_TIME),
                            ref_msg_type: msg_type_bytes,
                            ref_seq,
                            route,
                        };
                        return Ok(self.send_reject(&r, emit));
                    };
                    // `2f_PossDupOrigSendingTimeTooHigh.def`: a message that
                    // claims to have been sent after it was resent. `373=10`,
                    // and `send_reject` puts the Logout after it.
                    if sending_time.is_some_and(|sent| orig > sent) {
                        let r = Reject {
                            text: SessionText::SendingTimeAccuracyProblem,
                            ref_tag: None,
                            ref_msg_type: msg_type_bytes,
                            ref_seq,
                            route,
                        };
                        return Ok(self.send_reject(&r, emit));
                    }
                }
                return Ok(Link::Up);
            }
            let why = SessionText::MsgSeqNumTooLow {
                expecting: self.next_in,
                received: seq,
            };
            return Ok(self.logout_with(why, emit));
        }
        // A gap. The message is held rather than answered, the count does not
        // move, and the session asks for what it missed.
        if check_too_high && seq > self.next_in {
            return Ok(self.too_high(seq, bytes, emit));
        }

        if !is_sequence_reset && !is_logon {
            self.advance_past(seq);
        }

        // This message is one the session accepts, which is what the heartbeat
        // clock measures silence against — and it is the thing that clears an
        // outstanding `TestRequest`.
        self.last_recv_ms = self.now_ms;
        self.test_requests = 0;

        // The line above has already consumed the sequence number, and a Reject
        // does not give it back: `14a_BadField.def` rejects four messages in a
        // row, 34=2..5, and the fifth is accepted as 34=6.
        if let Some((text, ref_tag)) = fault {
            let r = Reject {
                text,
                ref_tag,
                ref_msg_type: msg_type_bytes,
                ref_seq,
                route,
            };
            return Ok(self.send_reject(&r, emit));
        }

        if is_logon {
            let encrypt = encrypt.as_deref().ok_or(Refusal::LogonIncomplete)?;
            let heart_bt = heart_bt.as_deref().ok_or(Refusal::LogonIncomplete)?;
            // The interval is the counterparty's, echoed and then obeyed. A
            // `108=` this session cannot read is not a reason to refuse a
            // Logon it is about to answer — it is a reason to keep quiet, which
            // is exactly what `beat_ms == 0` means.
            self.beat_ms = as_u32(heart_bt).map_or(0, |s| u64::from(s) * 1_000);
            self.state = State::LoggedOn;
            let mut extra: [(u32, &[u8]); 3] = [
                (tag::ENCRYPT_METHOD, encrypt),
                (tag::HEART_BT_INT, heart_bt),
                (0, &[]),
            ];
            let n = if reset_seq {
                extra[2] = (tag::RESET_SEQ_NUM_FLAG, b"Y");
                3
            } else {
                2
            };
            // **Only the side that did not speak first answers.** A Logon is
            // one exchange: the initiator asks and the acceptor agrees. An
            // initiator that answers has started a second handshake on a
            // session that already has one.
            //
            // `[measured 2026-09-02]` this line was unconditional, and no gate
            // in this repository could see it. `tests/score.rs` is 59 / 59
            // because for an **acceptor** the reply is correct;
            // `tests/mirror.rs` was 0 / 50 and never read past the first Logon.
            // `scripts/interop.sh` found it on its first run: `libquickfix`
            // took the second Logon, dropped the connection without a word, and
            // five interop steps failed at once with nothing on the wire to say
            // why. `tests/initiator.rs` holds the regression, and
            // `docs/reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md`
            // holds why it survived so long.
            if !R::SPEAKS_FIRST {
                self.send(Which::Logon, &extra[..n], emit)?;
            }
            // **After the reply, not before.** A Logon that runs ahead is still
            // a Logon: `1a_ValidLogonMsgSeqNumTooHigh.def` sends `34=5` to an
            // empty session and expects the Logon answered first and the
            // `ResendRequest` second. Answering in the other order, or refusing
            // it, loses the session over a gap that is recoverable.
            if seq > self.next_in {
                return Ok(self.too_high(seq, bytes, emit));
            }
            self.advance_past(seq);
            return Ok(Link::Up);
        }

        if is_logout {
            // **Only answer a goodbye we did not start.** A `Logout` exchange
            // is one message each way; a third is wrong on the wire, and
            // QuickFIX's `nextLogout` replies only when it did not begin the
            // exchange either.
            //
            // `[measured 2026-09-02]` this was unconditional, and nothing could
            // see it. The acceptor corpus never has the acceptor start a
            // logout, so every `35=5` in those 59 files is a reply that
            // *should* go out; `tests/goodbye.rs::their_answer_ends_the_session`
            // passed an `emit` of `|_| {}` and counted nothing; and
            // `scripts/interop.sh` stops reading once it has seen the
            // counterparty's `35=5`, so the extra message arrived after it was
            // looking. The **mirrored** corpus found it, as "unexpected
            // output" on `10_MsgSeqNumEqual.def` — which is what a gate that
            // can fall is for.
            //
            // Same family as the `Logon` echo: an asymmetry the acceptor
            // corpus cannot show, because an acceptor is always the responder.
            // `crates/session/tests/goodbye.rs` holds both halves.
            if self.state != State::LoggingOut {
                self.send(Which::Logout, &[], emit)?;
            }
            self.end(DropReason::PeerLogout);
            return Ok(Link::Dropped);
        }

        // A `TestRequest` is answered with a `Heartbeat` carrying the **same**
        // `112=`, not one of this session's own: `4b_ReceivedTestRequest.def`
        // sends `112=HELLO` and `SessionReset.def` sends `112=1`, and the
        // comparator reads tag 112 byte for byte.
        if is_test_request {
            let id = test_req_id.as_deref().unwrap_or_default();
            self.send(Which::Heartbeat, &[(tag::TEST_REQ_ID, id)], emit)?;
            return Ok(Link::Up);
        }

        // An inbound `ResendRequest` asks for messages back. **Every message
        // this session has sent so far is an administrative one, and QuickFIX
        // never replays those** — it fills the gap instead, with one
        // `SequenceReset` covering the whole range. A store of application
        // messages is step 6's; until it exists this is the whole answer, and
        // `8_OnlyAdminMessages.def` is the file that says so in its name.
        if is_resend_request {
            let last = self.next_out.saturating_sub(1);
            // `16=0` means "and everything after", and a counterparty may also
            // ask for more than this end has ever sent.
            let end = match end_seq_no {
                Some(n) if n != 0 && n <= last => n,
                _ => last,
            };
            if let Some(begin) = begin_seq_no.filter(|b| *b <= end) {
                // **A new question replaces the one in progress; it does not
                // queue behind it.** A counterparty that asks twice wants the
                // second answer, and two cursors would interleave two replays
                // on one wire.
                // **Recorded here, sent from the tail of `received_with`.**
                // Answering it here as well would put two batches on the wire
                // for one question, and `resend_batch × SLOT_LEN < TX` is the
                // whole constraint. `a_long_resend_is_replayed_over_several_
                // ticks_in_order` read 16 where it wanted 8 when this line
                // sent as well as recorded.
                self.owed = Some(Replay { next: begin, end });
            }
            return Ok(Link::Up);
        }

        // `36=NewSeqNo` moves the inbound count, forwards only. Equal is a
        // no-op and backwards is a Reject — and it is a Reject with no `371=`,
        // because QuickFIX's `generateReject(msg, reason)` names no field here.
        if is_sequence_reset {
            if let Some(new_seq) = new_seq_no {
                if new_seq > self.next_in {
                    self.next_in = new_seq;
                } else if new_seq < self.next_in {
                    let r = Reject {
                        text: SessionText::ValueIsIncorrect,
                        ref_tag: None,
                        ref_msg_type: msg_type_bytes,
                        ref_seq,
                        route,
                    };
                    return Ok(self.send_reject(&r, emit));
                }
            }
            return Ok(Link::Up);
        }

        // Everything the session does not own belongs to the application.
        if is_application {
            let unix = self.now_ms.saturating_sub(clock::MILLIS_YEAR_ZERO_TO_EPOCH);
            let stamp = *self.stamp.format(unix);
            let seq_out = self.next_out;
            let mut kept = true;
            let sent = {
                let o = self.out.as_mut().ok_or(Refusal::CannotSend)?;
                let out::Outbound { app: buf, .. } = o;
                match app.on_message(bytes, seq_out, &stamp, buf) {
                    Some(r) => {
                        // Kept before it is sent, and only application messages
                        // are kept: QuickFIX never replays an administrative
                        // message, it fills over it.
                        //
                        // **A refusal is counted, not ignored.** The message
                        // still goes out — declining to keep it is not
                        // declining to send it — but every future
                        // `ResendRequest` covering this number will gap-fill
                        // over it, and without the counter nothing on this side
                        // ever says so. ADR-0046.
                        kept = journal.put(seq_out, &buf[r.clone()]);
                        emit(&buf[r]);
                        true
                    }
                    None => false,
                }
            };
            if sent {
                if !kept {
                    self.puts_refused += 1;
                }
                self.next_out += 1;
                self.last_sent_ms = self.now_ms;
            }
        }

        Ok(Link::Up)
    }
}

/// A `Reject (35=3)` decided but not yet written.
///
/// Everything is copied out of the message before the borrow of the index ends,
/// because `send` needs `&mut self` and the view does not survive that.
struct Reject {
    text: SessionText,
    /// `371=`, as **text** rather than a number: `14a_BadField.def` sends
    /// `-1=HI` and expects `371=-1`, which is not a `u32`.
    ref_tag: Option<Held<12>>,
    /// `372=`. Absent when the message had no `MsgType` at all.
    ref_msg_type: Option<Held<8>>,
    /// `45=`. Absent when `34=` could not be read.
    ref_seq: Option<u32>,
    /// The routing tags, already reversed. `115` in becomes `128` out.
    route: [Option<(u32, Held<32>)>; 6],
}

/// The three routing pairs, **read in both directions**.
///
/// `ReverseRoute.def` sends each pair one way and then the other, and expects
/// the swap each time: `115` in becomes `128` out, and `128` in becomes `115`
/// out. Mapping only one direction passes three of its six cases, which is the
/// kind of half-right that a count alone would not show.
const ROUTING: [(u32, u32); 6] = [
    (tag::ON_BEHALF_OF_COMP_ID, tag::DELIVER_TO_COMP_ID),
    (tag::ON_BEHALF_OF_SUB_ID, tag::DELIVER_TO_SUB_ID),
    (tag::ON_BEHALF_OF_LOCATION_ID, tag::DELIVER_TO_LOCATION_ID),
    (tag::DELIVER_TO_COMP_ID, tag::ON_BEHALF_OF_COMP_ID),
    (tag::DELIVER_TO_SUB_ID, tag::ON_BEHALF_OF_SUB_ID),
    (tag::DELIVER_TO_LOCATION_ID, tag::ON_BEHALF_OF_LOCATION_ID),
];

/// Which pre-sorted template a send uses.
enum Which {
    Logon,
    Logout,
    Reject,
    Heartbeat,
    TestRequest,
    ResendRequest,
    GapFill,
}

/// The dictionary's questions about one already-parsed message, and nothing
/// else — the pass [`Session::received_with`] runs before it decides anything.
///
/// Returns the first fault, in the order [`Session`] itself applies them: the
/// wire-order field scan, then the required tags, then the group counters.
/// `None` means the dictionary has no complaint; it says nothing about
/// sequence numbers, CompIDs, `SendingTime` or the session's state, none of
/// which this looks at.
///
/// # Why this is public
///
/// [ADR-0050](../../../docs/decisions/ADR-0050-the-dictionary-pass-is-public-so-it-can-be-timed.md):
/// `STATUS.md` open item 39 says this pass has never been timed, and it could
/// not be — the three functions below are private, the dictionary is written
/// into their bodies rather than passed to them, and every public route in
/// also parses, checks identity and calls the application. A bench target is a
/// separate crate, so `pub(crate)` would not have reached it either. Timed by
/// `crates/session/benches/validate.rs`.
///
/// It is a real API rather than a hole punched for a benchmark: "what would
/// the session fault this message for" is answerable without a session, and
/// the answers are the ones `docs/SESSION-BEHAVIOUR.md` documents.
///
/// # What it does not tell you
///
/// The `371=` tag reference that a `Reject` carries is **not** returned. It is
/// held in a fixed buffer that stays private, and a caller that needs it wants
/// the session, not this.
///
/// ```
/// use fixbolt_codec::{FieldIndex, Validation, parse_into};
/// use fixbolt_dict::Fix44;
/// use fixbolt_session::validate;
///
/// let msg: &[u8] = b"8=FIX.4.4\x019=51\x0135=0\x0134=2\x0149=TW44\x01\
/// 52=20260905-12:00:00.000\x0156=ISLD\x0110=253\x01";
/// let mut idx: FieldIndex<64> = FieldIndex::new();
/// let r = parse_into::<Fix44, 64>(msg, &mut idx, Validation::ALL);
/// assert!(r.is_ok());
/// assert_eq!(validate(&idx.view(msg), b"0"), None);
/// ```
pub fn validate<const N: usize>(view: &MessageView<'_, N>, msg_type: &[u8]) -> Option<SessionText> {
    scan_fields(view, msg_type)
        .or_else(|| missing_required(view, msg_type))
        .or_else(|| bad_group_count(view, msg_type))
        .map(|(text, _tag)| text)
}

/// Walk the message in wire order and return the first fault, if any.
///
/// One pass, first fault wins — which is what the corpus expects: `14h` sends
/// `40=1|40=2` among a dozen good fields and names `371=40`, not the first tag
/// in the message.
fn scan_fields<const N: usize>(
    view: &MessageView<'_, N>,
    msg_type: &[u8],
) -> Option<(SessionText, Option<Held<12>>)> {
    let mut in_body = false;
    for i in 0..view.len() {
        let Some((tag, value)) = view.field_at(i) else {
            continue;
        };

        // `373=14`: the header is over once a body tag has been seen.
        // `14g_HeaderBodyTrailerFieldsOutOfOrder.def` puts `34=` after `11=`
        // and names `371=34`. Within the header the order is free — `14b`
        // sends `49, 34, 56, 52` and is faulted for something else entirely.
        if Fix44::is_header(tag) {
            if in_body {
                return Some((SessionText::TagSpecifiedOutOfRequiredOrder, tag_text(tag)));
            }
        } else if tag != 10 {
            in_body = true;
        }

        if !Fix44::is_defined_tag(tag) {
            return Some((SessionText::InvalidTagNumber, tag_text(tag)));
        }
        // `373=4` before `373=6`: an empty value is its own fault, and
        // `14d_TagSpecifiedWithoutValue.def` says so with `56=`.
        if value.is_empty() && Fix44::field_type(tag) != Some(FieldType::Data) {
            return Some((SessionText::TagSpecifiedWithoutValue, tag_text(tag)));
        }
        if !Fix44::allows(msg_type, tag) {
            return Some((SessionText::TagNotDefinedForThisMessageType, tag_text(tag)));
        }
        // A repeat at the top level. Group members repeat by design, so a tag
        // that belongs to a group present in this message is skipped — that is
        // what `21_RepeatingGroupSpecifierWithValueOfZero.def` and `14i` sit on.
        if view.find_from(0, tag).is_some_and(|(at, _)| at < i) && !in_a_group(view, msg_type, tag)
        {
            return Some((SessionText::TagAppearsMoreThanOnce, tag_text(tag)));
        }
        if Fix44::enum_allows(tag, value) == Some(false) {
            return Some((SessionText::ValueIsIncorrect, tag_text(tag)));
        }
        if Fix44::field_type(tag).is_some_and(|t| !t.accepts(value)) {
            return Some((SessionText::IncorrectDataFormat, tag_text(tag)));
        }
    }
    None
}

/// `373=16`: a group counter that disagrees with the entries behind it.
///
/// `14i_RepeatingGroupCountNotEqual.def` declares `386=3` and sends two
/// entries. This is the **only** repeating group the 59 definitions populate,
/// and it is in a negative test — see `PRD.md` §4.
fn bad_group_count<const N: usize>(
    view: &MessageView<'_, N>,
    msg_type: &[u8],
) -> Option<(SessionText, Option<Held<12>>)> {
    for i in 0..view.len() {
        let (counter, _) = view.field_at(i)?;
        if Fix44::group_delimiter(msg_type, counter).is_none() {
            continue;
        }
        let group = view.group::<Fix44>(msg_type, counter)?;
        if group.declared() != Some(group.counted()) {
            return Some((SessionText::IncorrectNumInGroupCount, tag_text(counter)));
        }
    }
    None
}

/// Whether `tag` is a member of some repeating group this message carries.
fn in_a_group<const N: usize>(view: &MessageView<'_, N>, msg_type: &[u8], tag: u32) -> bool {
    for i in 0..view.len() {
        let Some((counter, _)) = view.field_at(i) else {
            continue;
        };
        if Fix44::group_delimiter(msg_type, counter).is_some()
            && Fix44::group_members(msg_type, counter).contains(&tag)
        {
            return true;
        }
    }
    false
}

/// `373=1`: a required field the message does not carry.
///
/// The header's requirements and the body's are two tables, because they answer
/// two questions — `14b_RequiredFieldMissing.def` needs both, once for `56=`
/// and once for `11=`.
fn missing_required<const N: usize>(
    view: &MessageView<'_, N>,
    msg_type: &[u8],
) -> Option<(SessionText, Option<Held<12>>)> {
    for &tag in Fix44::required_header() {
        if view.get(tag).is_none() {
            return Some((SessionText::RequiredTagMissing, tag_text(tag)));
        }
    }
    for &tag in Fix44::required(msg_type) {
        if view.get(tag).is_none() {
            return Some((SessionText::RequiredTagMissing, tag_text(tag)));
        }
    }
    None
}

/// The `35=` of a frame the parser could not read, straight out of the bytes.
///
/// QuickFIX does the same thing for the same reason: it has to decide whether
/// an unreadable message was a Logon, and it cannot ask the parser that failed.
/// Handles `35=` both as the first field and after a separator, because
/// `2t_FirstThreeFieldsOutOfOrder.def` sends it first.
fn msg_type_of(bytes: &[u8]) -> Option<&[u8]> {
    const NEEDLE: &[u8] = b"\x0135=";
    let at = if bytes.starts_with(b"35=") {
        3
    } else {
        bytes.windows(NEEDLE.len()).position(|w| w == NEEDLE)? + NEEDLE.len()
    };
    let end = bytes[at..].iter().position(|&b| b == 0x01)? + at;
    Some(&bytes[at..end])
}

/// Whether these bytes are what QuickFIX's tokeniser would accept as a tag:
/// an optional `-` and then at least one digit, and nothing else.
///
/// Not `is_ascii_digit` alone: `14a_BadField.def` turns on `-1` being a
/// readable number. Not "starts with a digit" either: `49garbled` does.
/// Rebuild a kept message as a resend of itself.
///
/// `[measured 2026-08-29]` what the corpus asks for, byte for byte: the
/// original `34=`, `43=Y` admitting the repeat, a **fresh** `52=`, and the
/// original `52=` carried as `122=`. `8_OnlyApplicationMessages.def` declares
/// `9=132` against the original's `9=101`, and the difference is exactly those
/// two fields.
///
/// **Nothing here decides an order.** The fields go into a [`TemplateBuilder`]
/// in whatever order they are read out of the kept bytes, and `Fix44` sorts
/// them — non-negotiable 5. That is what puts `43` among the header tags and
/// `122` after the last of them, and it is the same path
/// `15_HeaderAndBodyFieldsOrderedDifferently.def` proves for an echo.
///
/// Returns the range of `out` the message occupies, or `None` if the kept bytes
/// are not a message or the result does not fit.
fn as_resend(kept: &[u8], now: &[u8], out: &mut [u8]) -> Option<core::ops::Range<usize>> {
    rebuild(kept, None, now, true, out)
}

/// Write an application message this end is originating, or replaying.
///
/// `seq` renumbers it; `None` keeps the number already on it, which is what a
/// replay needs. `resend` adds `43=Y` and carries the old `52=` as `122=`.
///
/// **Nothing here decides an order.** Every field goes into a
/// [`TemplateBuilder`] in whatever order it is read, and `Fix44` sorts them —
/// non-negotiable 5.
fn rebuild(
    src: &[u8],
    seq: Option<&[u8]>,
    now: &[u8],
    resend: bool,
    out: &mut [u8],
) -> Option<core::ops::Range<usize>> {
    let at_35 = src.windows(4).position(|w| w == b"\x0135=")? + 1;
    let at_10 = src
        .windows(4)
        .position(|w| w == b"\x0110=")
        .map_or(src.len(), |i| i + 1);
    let begin_end = src.iter().position(|b| *b == SOH)?;
    let begin = src.get(2..begin_end)?;

    // **Bound first, then mutated in place.** `TemplateBuilder`'s methods take
    // `&mut self` since 2026-09-02 — moving an `S`-byte struct per field was
    // 48% of a reply, and `S` here is 1024
    // ([ADR-0044](../../../docs/decisions/ADR-0044-a-builder-that-is-not-moved-per-field.md)).
    // This path rebuilds a template **per resent message**, so it is one of the
    // two that pays for it.
    let mut b = TemplateBuilder::<128, 1024>::new(begin);
    b.field(tag::SENDING_TIME, now);
    if resend {
        b.field(tag::POSS_DUP_FLAG, b"Y");
    }
    if let Some(n) = seq {
        b.field(tag::MSG_SEQ_NUM, n);
    }

    for f in src.get(at_35..at_10)?.split(|c| *c == SOH) {
        if f.is_empty() {
            continue;
        }
        let eq = f.iter().position(|c| *c == b'=')?;
        let (text, value) = (&f[..eq], &f[eq + 1..]);
        let tag = digits_to_u32(text)?;
        match tag {
            // Written above with the values this send needs, not the ones the
            // source happened to carry.
            tag::POSS_DUP_FLAG | tag::ORIG_SENDING_TIME => {}
            tag::MSG_SEQ_NUM if seq.is_some() => {}
            // The clock the message first went out on becomes `122=`, and the
            // one above stands as `52=`.
            tag::SENDING_TIME if resend => {
                b.field(tag::ORIG_SENDING_TIME, value);
            }
            tag::SENDING_TIME => {}
            _ => {
                b.field(tag, value);
            }
        }
    }

    let t = b.build::<Fix44>().ok()?;
    t.encode_with::<Fix44>(out, &[], &[]).ok()
}

/// ASCII digits to a tag number. `None` for anything else — a kept message came
/// out of this crate, so this cannot fail without a bug.
fn digits_to_u32(text: &[u8]) -> Option<u32> {
    if text.is_empty() {
        return None;
    }
    let mut n: u32 = 0;
    for c in text {
        n = n
            .checked_mul(10)?
            .checked_add(u32::from(c.checked_sub(b'0')?))?;
        if *c > b'9' {
            return None;
        }
    }
    Some(n)
}

fn is_signed_integer(v: &[u8]) -> bool {
    let digits = v.strip_prefix(b"-").unwrap_or(v);
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
}

/// A tag number as the digits `371=` carries.
fn tag_text(tag: u32) -> Option<Held<12>> {
    let mut buf = [0u8; 10];
    copy::<12>(Some(digits(tag, &mut buf)))
}

/// A field value copied out of the view, so the borrow of the index can end
/// before `send` takes `&mut self`. Fixed-size and on the stack: `98=` and
/// `108=` are a digit or two, and `None` means the field was absent or longer
/// than `N`.
fn copy<const N: usize>(value: Option<&[u8]>) -> Option<Held<N>> {
    let v = value?;
    if v.len() > N {
        return None;
    }
    let mut buf = [0u8; N];
    buf[..v.len()].copy_from_slice(v);
    Some(Held { buf, len: v.len() })
}

/// How many messages running ahead of the count this session will hold.
///
/// `[measured 2026-08-29]` the deepest the acceptance corpus goes is **two** —
/// `10_MsgSeqNumGreater.def`, which sends a `SequenceReset` and a
/// `TestRequest` while the gap in front of them is open. Four is that with room
/// to spare, and holding more is not free: this array is per connection.
const QUEUED: usize = 4;

/// The longest held message. The same 512 as the outbound buffer, and for the
/// same reason: the longest message in the corpus is 101 bytes.
const QUEUED_LEN: usize = 512;

/// How far through a `ResendRequest` this end has got.
///
/// Two numbers and nothing else: **the journal is not copied and nothing is
/// buffered.** What is owed is a range, and the ring already holds the bytes.
#[derive(Debug, Clone, Copy)]
struct Replay {
    /// The next number to answer, replayed or filled over.
    next: u32,
    /// The last number the counterparty asked for, inclusive.
    end: u32,
}

/// One message held until the count catches up. `seq == 0` means the slot is
/// free — FIX counts from 1, so no real message can claim it.
struct Queued {
    seq: u32,
    len: u16,
    buf: [u8; QUEUED_LEN],
}

struct Held<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> core::ops::Deref for Held<N> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

/// `n` in decimal, into `buf`. No `format!`, no allocation — non-negotiable 2.
fn digits(n: u32, buf: &mut [u8; 10]) -> &[u8] {
    let mut at = buf.len();
    let mut n = n;
    loop {
        at -= 1;
        buf[at] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    &buf[at..]
}
