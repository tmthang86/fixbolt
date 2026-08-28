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

use core::marker::PhantomData;

use nanofix_codec::{FieldIndex, Parsed, Validation, parse_into};
use nanofix_dict::Fix44;

/// FIX tags this layer reads by number. Named so a call site never carries a
/// bare integer — `CLAUDE.md` §6, and it is how tag 12 was once mistaken for
/// `Currency` in a document.
mod tag {
    pub const BEGIN_STRING: u32 = 8;
    pub const MSG_TYPE: u32 = 35;
    pub const SENDER_COMP_ID: u32 = 49;
    pub const SENDING_TIME: u32 = 52;
    pub const TARGET_COMP_ID: u32 = 56;
}

/// `MsgType` values this layer acts on.
mod msg {
    pub const LOGON: &[u8] = b"A";
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
    /// `49=` is not the configured counterparty.
    WrongSenderCompId,
    /// `56=` is not us.
    WrongTargetCompId,
    /// `52=` is absent, unreadable, or too far from the last [`Session::tick`].
    BadSendingTime,
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
    _role: PhantomData<R>,
}

impl<R: Role, const N: usize> Session<R, N> {
    /// A session that has not yet been connected.
    #[must_use]
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            state: State::Disconnected,
            now_ms: 0,
            idx: FieldIndex::new(),
            _role: PhantomData,
        }
    }

    /// True once a Logon has been accepted. Step 1 never gets here.
    #[must_use]
    pub const fn is_logged_on(&self) -> bool {
        false
    }

    /// A counterparty opened the connection.
    pub fn connect<F: FnMut(&[u8])>(&mut self, emit: F) -> Link {
        let _ = emit;
        self.state = State::AwaitingLogon;
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
            State::AwaitingLogon => Link::Up,
        }
    }

    /// One whole message arrived. Framing is the transport's job.
    pub fn received<F: FnMut(&[u8])>(&mut self, bytes: &[u8], emit: F) -> Link {
        let _ = emit;
        match self.judge(bytes) {
            Ok(()) => Link::Up,
            Err(_) => {
                self.state = State::Disconnected;
                Link::Dropped
            }
        }
    }

    /// The step-1 rule set, in the order QuickFIX applies it.
    fn judge(&mut self, bytes: &[u8]) -> Result<(), Refusal> {
        match parse_into::<Fix44, N>(bytes, &mut self.idx, Validation::ALL) {
            // A partial read is not a refusal: the next call brings the rest.
            Ok(Parsed::Incomplete) => return Ok(()),
            Ok(Parsed::Complete { .. }) => {}
            Err(_) => return Err(Refusal::Malformed),
        }

        let view = self.idx.view(bytes);
        let cfg = &self.cfg;

        if !view
            .get(tag::BEGIN_STRING)
            .is_some_and(|v| cfg.begin_string.matches(v))
        {
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

        Ok(())
    }
}
