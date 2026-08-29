//! The FIX 4.4 session layer: pure, allocation-free, and driven entirely by
//! [`Input`]-shaped calls.
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
mod out;
pub mod text;

use core::marker::PhantomData;

use nanofix_codec::{
    Dictionary, FieldIndex, MessageView, ParseError, Parsed, TimestampCache, Validation, as_u32,
    parse_into, tag_text_at,
};
use nanofix_dict::{FieldType, Fix44};

use crate::out::Outbound;
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

/// QuickFIX's default `MaxLatency`, in milliseconds.
///
/// `[documented]` 120 seconds is what `libquickfix` applies to `SendingTime`,
/// and `1d_InvalidLogonBadSendingTime` is 2001 years out, so nothing in the
/// corpus distinguishes this number from any other. It is the documented
/// default, labelled as such.
pub const DEFAULT_MAX_SKEW_MS: u64 = 120_000;

/// Everything a session needs to know that is not on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    begin_string: Name<16>,
    /// Ours. Appears as `49=` on the way out and must appear as `56=` on the
    /// way in.
    sender_comp_id: Name<32>,
    /// Theirs. `56=` out, `49=` in.
    target_comp_id: Name<32>,
    max_skew_ms: u64,
}

impl Config {
    /// An acceptor's configuration. `sender_comp_id` is *this* end.
    #[must_use]
    pub fn acceptor(begin_string: &[u8], sender_comp_id: &[u8], target_comp_id: &[u8]) -> Self {
        Self {
            begin_string: Name::new(begin_string),
            sender_comp_id: Name::new(sender_comp_id),
            target_comp_id: Name::new(target_comp_id),
            max_skew_ms: DEFAULT_MAX_SKEW_MS,
        }
    }

    /// Override [`DEFAULT_MAX_SKEW_MS`].
    #[must_use]
    pub const fn with_max_skew_ms(mut self, ms: u64) -> Self {
        self.max_skew_ms = ms;
        self
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
    /// Logon exchanged. Application messages may flow.
    LoggedOn,
    /// This end sent a Logout and is waiting for the counterparty's. It may
    /// never come — `2i_BeginStringValueUnexpected.def` runs the same sequence
    /// twice, once with a reply and once without, and the link must go down
    /// either way. So the link is reported down at once and anything that
    /// arrives afterwards is read and ignored.
    AwaitingLogout,
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
    /// A message the session could not put on the wire: the configuration does
    /// not fit its own templates, or the output buffer is too small. A bug, and
    /// the session fails closed rather than sending something malformed.
    CannotSend,
}

/// One FIX session, parameterised by role.
///
/// `N` is the [`FieldIndex`] capacity — the caller picks it, per `CLAUDE.md`
/// §6. 256 covers every message in the acceptance corpus.
///
/// Not `Debug`: `FieldIndex` is not, and it is 3 KiB of offsets that no one
/// would read anyway.
pub struct Session<R: Role, const N: usize> {
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
    out: Option<Outbound>,
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
    /// Messages that arrived ahead of [`Self::next_in`], held until the gap in
    /// front of them closes.
    queue: [Queued; QUEUED],
    _role: PhantomData<R>,
}

impl<R: Role, const N: usize> Session<R, N> {
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
            beat_ms: 0,
            last_sent_ms: 0,
            last_recv_ms: 0,
            test_requests: 0,
            resend_from: 0,
            resend_to: 0,
            queue: [const {
                Queued {
                    seq: 0,
                    len: 0,
                    buf: [0; QUEUED_LEN],
                }
            }; QUEUED],
            _role: PhantomData,
        }
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
        // A new connection starts a new count. Persisting sequence numbers
        // across a reconnect is the journal's job, and the journal belongs to
        // `engine`; `2i_BeginStringValueUnexpected.def` logs on twice and
        // expects `34=1` back both times.
        self.next_out = 1;
        self.next_in = 1;
        // The heartbeat clock is deliberately **not** reset here. Every field
        // it uses is written again before it can be read: `beat_ms`,
        // `last_recv_ms` and `test_requests` by the Logon this connection must
        // now send, `last_sent_ms` by the reply. Clearing them as well changed
        // nothing when it was reversed, and a line that cannot be broken is
        // not a guard.
        // An initiator speaks first. Step 2 gives it something to say; until
        // then the constant is read here so the role parameter is not decoration.
        if R::SPEAKS_FIRST {
            return Link::Up;
        }
        Link::Up
    }

    /// A counterparty closed the connection.
    pub fn disconnect<F: FnMut(&[u8])>(&mut self, emit: F) -> Link {
        let _ = emit;
        self.state = State::Disconnected;
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
    pub fn tick<F: FnMut(&[u8])>(&mut self, now_ms: u64, mut emit: F) -> Link {
        self.now_ms = now_ms;
        match self.state {
            State::Disconnected | State::AwaitingLogout => return Link::Dropped,
            // Before a Logon there is no agreed interval, so there is nothing
            // to measure — and this is the **only** thing that says so, which
            // is why `connect` no longer clears the clock as well. A logon
            // timeout is the engine's business, and no acceptance definition
            // tests one. `tests/heartbeat.rs` holds this.
            State::AwaitingLogon => return Link::Up,
            State::LoggedOn => {}
        }
        // `108=0` means the counterparty asked for no heartbeats at all.
        if self.beat_ms == 0 {
            return Link::Up;
        }
        let quiet = now_ms.saturating_sub(self.last_recv_ms);
        let silent = now_ms.saturating_sub(self.last_sent_ms);

        if quiet >= self.beat_ms * 24 / 10 {
            self.state = State::Disconnected;
            return Link::Dropped;
        }
        if quiet >= self.beat_ms * 12 * u64::from(self.test_requests + 1) / 10 {
            self.test_requests += 1;
            let _ = self.send(
                Which::TestRequest,
                &[(tag::TEST_REQ_ID, OWN_TEST_REQ_ID)],
                &mut emit,
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
            let _ = self.send(Which::Heartbeat, &[], &mut emit);
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
            self.state = State::Disconnected;
            return Link::Dropped;
        }
        Link::Up
    }

    /// One whole message arrived. Framing is the transport's job.
    pub fn received<F: FnMut(&[u8])>(&mut self, bytes: &[u8], mut emit: F) -> Link {
        // Once the link is down, bytes still arrive — the counterparty's own
        // Logout crossing ours, or anything already in flight. Reading them is
        // free; answering them would put a message on a connection this end has
        // already given up, and the corpus catches it as unexpected output.
        if matches!(self.state, State::Disconnected | State::AwaitingLogout) {
            return Link::Dropped;
        }
        let link = match self.judge(bytes, &mut emit) {
            Ok(link) => link,
            Err(_) => {
                self.state = State::Disconnected;
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
        self.drain(&mut emit)
    }

    /// Judge every held message the count has caught up with.
    fn drain<F: FnMut(&[u8])>(&mut self, emit: &mut F) -> Link {
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
            match self.judge(&held[..len], emit) {
                Ok(Link::Up) => {}
                Ok(Link::Dropped) => return Link::Dropped,
                Err(_) => {
                    self.state = State::Disconnected;
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
            self.state = State::Disconnected;
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
            self.state = State::Disconnected;
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
            self.state = State::Disconnected;
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

    /// Read one message, decide, and answer. The order the checks run in is
    /// the order QuickFIX applies them, and it is load-bearing: two rules that
    /// share an outcome are indistinguishable to the corpus, so which one fires
    /// first is only visible to `tests/logon.rs`.
    fn judge<F: FnMut(&[u8])>(&mut self, bytes: &[u8], emit: &mut F) -> Result<Link, Refusal> {
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
                let view = self.idx.view(bytes);
                let r = Reject {
                    text: SessionText::InvalidTagNumber,
                    ref_tag: copy::<12>(tag_text_at(bytes, at as usize)),
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

        // "The first message must be a Logon" outranks the identity checks:
        // `1e_NotLogonMessage` sends `35=0` *and* a wrong `56=`, and its name
        // says which rule it is testing.
        if self.state == State::AwaitingLogon && view.get(tag::MSG_TYPE) != Some(msg::LOGON) {
            return Err(Refusal::NotALogon);
        }

        let sender_ok = view
            .get(tag::SENDER_COMP_ID)
            .is_some_and(|v| cfg.target_comp_id.matches(v));
        let target_ok = view
            .get(tag::TARGET_COMP_ID)
            .is_some_and(|v| cfg.sender_comp_id.matches(v));
        let time_ok = view
            .get(tag::SENDING_TIME)
            .and_then(clock::parse_utc)
            .is_some_and(|t| t.abs_diff(self.now_ms) <= cfg.max_skew_ms);

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
        let is_logon = msg_type == msg::LOGON;
        let is_logout = msg_type == msg::LOGOUT;
        let is_test_request = msg_type == msg::TEST_REQUEST;
        let is_sequence_reset = msg_type == msg::SEQUENCE_RESET;
        // `123=` defaults to `N` when absent — `11a` and `11b` leave it out and
        // their comments say "default to N".
        let gap_fill = view.get(tag::GAP_FILL_FLAG) == Some(b"Y");
        let new_seq_no = view.get(tag::NEW_SEQ_NO).and_then(|v| as_u32(v).ok());
        let is_resend_request = msg_type == msg::RESEND_REQUEST;
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
            self.send(Which::Logon, &extra[..n], emit)?;
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
            self.send(Which::Logout, &[], emit)?;
            self.state = State::Disconnected;
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
                let unix = self.now_ms.saturating_sub(clock::MILLIS_YEAR_ZERO_TO_EPOCH);
                let orig = *self.stamp.format(unix);
                let mut new_seq = [0u8; 10];
                let new_seq = digits(end + 1, &mut new_seq);
                self.send_as(
                    Which::GapFill,
                    Some(begin),
                    &[
                        (tag::POSS_DUP_FLAG, b"Y"),
                        (tag::ORIG_SENDING_TIME, &orig),
                        (tag::NEW_SEQ_NO, new_seq),
                        (tag::GAP_FILL_FLAG, b"Y"),
                    ],
                    emit,
                )?;
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
