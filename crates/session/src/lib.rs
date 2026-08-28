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

use nanofix_codec::{FieldIndex, Parsed, TimestampCache, Validation, as_u32, parse_into};
use nanofix_dict::Fix44;

use crate::out::Outbound;
use crate::text::SessionText;

/// FIX tags this layer reads by number. Named so a call site never carries a
/// bare integer — `CLAUDE.md` §6, and it is how tag 12 was once mistaken for
/// `Currency` in a document.
pub(crate) mod tag {
    pub const BEGIN_STRING: u32 = 8;
    pub const MSG_SEQ_NUM: u32 = 34;
    pub const POSS_DUP_FLAG: u32 = 43;
    pub const MSG_TYPE: u32 = 35;
    pub const SENDER_COMP_ID: u32 = 49;
    pub const SENDING_TIME: u32 = 52;
    pub const TARGET_COMP_ID: u32 = 56;
    pub const TEXT: u32 = 58;
    pub const ENCRYPT_METHOD: u32 = 98;
    pub const HEART_BT_INT: u32 = 108;
}

/// `MsgType` values this layer acts on.
mod msg {
    pub const LOGON: &[u8] = b"A";
    pub const LOGOUT: &[u8] = b"5";
}

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
    /// The frame could not be read: bad `9=`, bad `10=`, an unreadable tag.
    Malformed,
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
    pub fn tick<F: FnMut(&[u8])>(&mut self, now_ms: u64, emit: F) -> Link {
        let _ = emit;
        self.now_ms = now_ms;
        match self.state {
            State::Disconnected => Link::Dropped,
            State::AwaitingLogon | State::LoggedOn => Link::Up,
            State::AwaitingLogout => Link::Dropped,
        }
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
        match self.judge(bytes, &mut emit) {
            Ok(link) => link,
            Err(_) => {
                self.state = State::Disconnected;
                Link::Dropped
            }
        }
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

    fn send<F: FnMut(&[u8])>(
        &mut self,
        which: Which,
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
        let seq = digits(self.next_out, &mut seq);

        let o = self.out.as_mut().ok_or(Refusal::CannotSend)?;
        let mut slots: [(u32, &[u8]); 8] = [(0, &[]); 8];
        slots[0] = (tag::MSG_SEQ_NUM, seq);
        slots[1] = (tag::SENDING_TIME, &stamp);
        let mut n = 2;
        for pair in extra {
            *slots.get_mut(n).ok_or(Refusal::CannotSend)? = *pair;
            n += 1;
        }

        // Destructured so the chosen template and the output buffer are two
        // disjoint borrows of one struct rather than two borrows of the whole.
        let Outbound { logon, logout, buf } = o;
        let template = match which {
            Which::Logon => &*logon,
            Which::Logout => &*logout,
        };
        let range = template
            .encode(buf, &slots[..n])
            .map_err(|_| Refusal::CannotSend)?;
        emit(&buf[range]);
        self.next_out += 1;
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
            Err(_) => return Err(Refusal::Malformed),
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

        if !view
            .get(tag::SENDER_COMP_ID)
            .is_some_and(|v| cfg.target_comp_id.matches(v))
        {
            return Err(Refusal::WrongSenderCompId);
        }
        if !view
            .get(tag::TARGET_COMP_ID)
            .is_some_and(|v| cfg.sender_comp_id.matches(v))
        {
            return Err(Refusal::WrongTargetCompId);
        }

        let sending_time = view
            .get(tag::SENDING_TIME)
            .and_then(clock::parse_utc)
            .ok_or(Refusal::BadSendingTime)?;
        if sending_time.abs_diff(self.now_ms) > cfg.max_skew_ms {
            return Err(Refusal::BadSendingTime);
        }

        // Everything below is decided from values already read out of `view`,
        // so the borrow of `self.idx` ends here and `send` can take `&mut self`.
        let msg_type = view.get(tag::MSG_TYPE).unwrap_or_default();
        let is_logon = msg_type == msg::LOGON;
        let is_logout = msg_type == msg::LOGOUT;
        let seq = view
            .get(tag::MSG_SEQ_NUM)
            .and_then(|v| as_u32(v).ok())
            .ok_or(Refusal::BadSeqNum)?;
        let poss_dup = view.get(tag::POSS_DUP_FLAG) == Some(b"Y");
        let encrypt = copy::<8>(view.get(tag::ENCRYPT_METHOD));
        let heart_bt = copy::<8>(view.get(tag::HEART_BT_INT));

        // A sequence number already used cannot be taken back, so the session
        // hangs up. Too *high* means a gap, which is a ResendRequest and not a
        // refusal — that is a later step, and until it exists such a message is
        // read and answered as if it were in order.
        if seq < self.next_in {
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
        self.next_in = seq + 1;

        if is_logon {
            let encrypt = encrypt.as_deref().ok_or(Refusal::LogonIncomplete)?;
            let heart_bt = heart_bt.as_deref().ok_or(Refusal::LogonIncomplete)?;
            self.state = State::LoggedOn;
            self.send(
                Which::Logon,
                &[
                    (tag::ENCRYPT_METHOD, encrypt),
                    (tag::HEART_BT_INT, heart_bt),
                ],
                emit,
            )?;
            return Ok(Link::Up);
        }

        if is_logout {
            self.send(Which::Logout, &[], emit)?;
            self.state = State::Disconnected;
            return Ok(Link::Dropped);
        }

        Ok(Link::Up)
    }
}

/// Which pre-sorted template a send uses.
enum Which {
    Logon,
    Logout,
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
