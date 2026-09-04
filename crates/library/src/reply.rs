//! The answer an application writes, and the seven fields it never writes.
//!
//! `DESIGN.md` §3 L4. A handler names the fields of *its* message —
//! `37=OrderID`, `150=ExecType` — and nothing else. `8`, `9`, `10`, `34`, `49`,
//! `52` and `56` are written here, from the session, because every one of them
//! is a value the application does not own:
//!
//! * `34` **MsgSeqNum** and `52` **SendingTime** belong to the session. An
//!   application that regenerates `52` moves the body and fails a test that
//!   says nothing about time — the `9=101` trap at the top of
//!   `crates/conformance/src/echo.rs`.
//! * `49`/`56` are **reversed**: this side's sender is the other side's target.
//!   Getting that wrong addresses the reply to yourself.
//! * `8`, `9` and `10` are the frame. [`fixbolt_codec::Template::encode_with`]
//!   writes them and refuses a caller who tries.
//!
//! **Field order is never a call site's choice.** Everything a handler names
//! goes through [`fixbolt_codec::TemplateBuilder::build`], which sorts from the
//! generated tables: `MsgType` first, then header tags ascending, then body tags
//! ascending. That is `CLAUDE.md` §2 non-negotiable 5, and it is the reason this
//! type exists at all rather than a `&mut [u8]` and a comment.

use core::ops::Range;

use fixbolt_codec::{EncodeError, GroupData, TemplateBuilder};
use fixbolt_dict::Fix44;

/// Fields written from the session, never from the handler.
///
/// A handler that names one of these is **ignored**, not merged and not
/// refused: the wire result is the same either way, and refusing would turn a
/// harmless habit into a dropped message. `8`, `9` and `10` are absent because
/// [`fixbolt_codec::Template`] refuses them itself with
/// [`EncodeError::ReservedTag`] — one rule, one place.
const SESSION_OWNED: [u32; 4] = [34, 49, 52, 56];

/// Why a reply could not be written.
///
/// Fieldless but for the codec's own error, which is itself `Copy` and
/// fieldless-per-variant: this sits on the engine thread, so `CLAUDE.md` §6
/// forbids anything that would allocate to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyError {
    /// The message could not be laid out or written. `out` is untouched.
    Encode(EncodeError),
}

impl core::fmt::Display for ReplyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Encode(e) => write!(f, "the reply could not be encoded: {e:?}"),
        }
    }
}

impl std::error::Error for ReplyError {}

/// What a handler decided.
///
/// A named type rather than `Option<Range<usize>>`, so *"nothing to say"* and
/// *"the reply did not fit"* are two answers instead of one. The engine sees
/// both as silence; [`crate::App`] counts the second
/// ([`crate::App::failed_replies`]), because a silence nobody counts is a
/// silence nobody can explain — `docs/reference/silence-before-a-logon-has-many-causes.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A message occupying this range of the output buffer. It does **not**
    /// start at `out[0]`: `BodyLength` is variable-width, so the prefix is
    /// right-aligned in front of the body.
    Sent(Range<usize>),
    /// Nothing to say. The counterparty gets no message and the session's
    /// outbound sequence number does not move.
    Silent,
    /// The reply could not be written. Nothing was sent.
    Failed(ReplyError),
}

impl Answer {
    /// The range, when there is one. What [`crate::App`] hands the session.
    #[must_use]
    pub fn range(&self) -> Option<Range<usize>> {
        match self {
            Self::Sent(r) => Some(r.clone()),
            Self::Silent | Self::Failed(_) => None,
        }
    }
}

/// The right to answer one message, with the session's own numbers already in
/// hand.
///
/// Built by [`crate::App`] once per application message and consumed by the
/// handler. `P` is how many fields a reply may carry and `S` how many bytes of
/// them — the caller picks both, as `CLAUDE.md` §6 requires of every size in
/// this workspace.
///
/// **The defaults are 64 and 1024, and they were measured rather than chosen.**
/// `[measured 2026-09-02, Intel Xeon @ 2.80GHz, NOT a `DESIGN.md` §9 machine]`
/// one twelve-field reply through this type:
///
/// | `P` | `S` | ns/op |
/// |---|---|---|
/// | 128 | 4096 | 3841 – 4008 |
/// | **64** | **1024** | **1992 – 2197** |
/// | 32 | 512 | 1447 – 1552 |
/// | 32 | 256 | 1421 – 1504 |
///
/// Both numbers are copied on every `.field()` call, so both are on the clock;
/// below `S = 512` the curve flattens and what is left is the parts array and
/// the sort. 64 and 1024 hold a realistic `ExecutionReport` with a small
/// repeating group and leave room, which is what a default is for. A caller who
/// knows their message writes `Handler<256, 32, 512>` and takes the rest.
///
/// **Every one of these is far above the 40 ns it costs to encode a template
/// that was built once** — see
/// [ADR-0041](../../../docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md),
/// which is the decision this table belongs to. `crates/library/benches/cost.rs`
/// is the committed benchmark.
pub struct Reply<'a, const P: usize = 64, const S: usize = 1024> {
    begin_string: &'a [u8],
    /// `None` when this is an **origination** rather than a reply.
    ///
    /// A reply is written into the session's own outbound buffer and carries
    /// the number the session already spent on it, so `34=` and `52=` go in
    /// here. An origination is written *before* the session has looked at it —
    /// `Session::send_application` assigns both on the way out — so writing
    /// them here would be writing two values that are about to be replaced.
    /// [ADR-0048](../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md)
    /// decision 2.
    seq: Option<u32>,
    stamp: &'a [u8],
    sender: &'a [u8],
    target: &'a [u8],
    out: &'a mut [u8],
}

impl<'a, const P: usize, const S: usize> Reply<'a, P, S> {
    /// Everything the session knows and the application does not.
    ///
    /// `sender` and `target` are **this side's**, already reversed — a caller
    /// building one by hand is the one place the reversal can go wrong, and
    /// [`crate::App`] is the caller that does it. Public because writing your
    /// own adapter over [`fixbolt_session::Application`] is a supported thing to
    /// do; it is not on the path a handler takes.
    #[must_use]
    pub fn new(
        begin_string: &'a [u8],
        seq: u32,
        stamp: &'a [u8],
        sender: &'a [u8],
        target: &'a [u8],
        out: &'a mut [u8],
    ) -> Self {
        Self {
            begin_string,
            seq: Some(seq),
            stamp,
            sender,
            target,
            out,
        }
    }

    /// The same, for a message **nobody asked for**.
    ///
    /// There is no inbound message to take a sequence number or a `SendingTime`
    /// from, and there does not need to be: `Session::send_application` writes
    /// `8=`, `9=`, `34=`, `52=` and `10=` on the way out and ignores anything
    /// this end put there. So an origination names its `MsgType` and its body,
    /// and nothing else.
    ///
    /// `sender` is **this** end and `target` the counterparty — the same way
    /// round [`fixbolt_session::Peer`] carries them, which is where a
    /// [`crate::Handler`] gets them.
    ///
    /// [ADR-0048](../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md)
    #[must_use]
    pub fn originate(
        begin_string: &'a [u8],
        sender: &'a [u8],
        target: &'a [u8],
        out: &'a mut [u8],
    ) -> Self {
        Self {
            begin_string,
            seq: None,
            stamp: b"",
            sender,
            target,
            out,
        }
    }

    /// Say nothing. The same answer as returning [`Answer::Silent`], spelled so
    /// that a handler which decides not to reply reads as a decision.
    #[must_use]
    pub fn silent(self) -> Answer {
        Answer::Silent
    }

    /// Begin a reply of this `MsgType`, such as `b"8"` for an
    /// `ExecutionReport`.
    ///
    /// The four session-owned fields go in here, before the handler names
    /// anything — so a handler cannot forget one, and cannot get the `49`/`56`
    /// reversal wrong, because neither is reachable from the API it is given.
    #[must_use]
    pub fn message(self, msg_type: &[u8]) -> Message<'a, P, S> {
        let mut digits = [0u8; 10];
        let mut b = TemplateBuilder::<P, S>::new(self.begin_string);
        b.field(35, msg_type)
            .field(49, self.sender)
            .field(56, self.target);
        // Only a reply carries these. An origination leaves them out and the
        // session writes them — see the `seq` field's own note.
        if let Some(n) = self.seq {
            b.field(34, render_u32(n, &mut digits))
                .field(52, self.stamp);
        }
        Message {
            out: self.out,
            b,
            err: None,
        }
    }
}

/// A reply being written. Name body fields; the header is already accounted
/// for.
pub struct Message<'a, const P: usize, const S: usize> {
    out: &'a mut [u8],
    /// Held by value, **not** behind an `Option`.
    ///
    /// `[measured 2026-09-02]` the first version wrapped it so that `send`
    /// could take it out of `&mut self`-free code, and the `take`/put pair cost
    /// two extra moves of an `S`-byte struct on **every** `.field()` call:
    /// `library, reply only` read 5653 ns/op with it and 2300 ns/op for the
    /// same fifteen fields written straight against `TemplateBuilder`. A
    /// convenience that costs more than the thing it wraps is not one.
    b: TemplateBuilder<P, S>,
    /// The first failure, kept so that a chain of `.field()` calls does not
    /// need a `?` on every line. Reported once by [`Self::send`].
    err: Option<ReplyError>,
}

impl<const P: usize, const S: usize> Message<'_, P, S> {
    /// Add a field. Order does not matter — the dictionary decides it.
    ///
    /// A tag the session owns (`34`, `49`, `52`, `56`) is **ignored**: the
    /// session's value is already in the message and writing a second copy
    /// would put two of the same tag on the wire.
    pub fn field(&mut self, tag: u32, value: &[u8]) -> &mut Self {
        if SESSION_OWNED.contains(&tag) {
            return self;
        }
        self.b.field(tag, value);
        self
    }

    /// Declare a repeating group by its counter tag, such as `453` for
    /// `NoPartyIDs`. The entries are supplied to [`Self::send_with_groups`].
    ///
    /// The counter's *value* is never stated: it is the number of entries, so
    /// the two cannot disagree.
    pub fn group(&mut self, counter: u32) -> &mut Self {
        self.b.group(counter);
        self
    }

    /// Write the message. The answer is what [`crate::Handler`] returns.
    #[must_use]
    pub fn send(&mut self) -> Answer {
        self.send_with_groups(&[])
    }

    /// As [`Self::send`], with the entries of every group declared by
    /// [`Self::group`].
    ///
    /// Groups are passed at the call rather than stored, so nothing here
    /// borrows a caller's stack for longer than one statement.
    #[must_use]
    pub fn send_with_groups(&mut self, groups: &[GroupData<'_>]) -> Answer {
        if let Some(e) = self.err {
            return Answer::Failed(e);
        }
        match self.b.build::<Fix44>() {
            Ok(t) => match t.encode_with::<Fix44>(&mut self.out[..], &[], groups) {
                Ok(r) => Answer::Sent(r),
                Err(e) => Answer::Failed(ReplyError::Encode(e)),
            },
            Err(e) => Answer::Failed(ReplyError::Encode(e)),
        }
    }
}

/// ASCII digits of `v`, right-aligned in `buf`.
///
/// Ten digits is `u32::MAX`, so the slice is always in range and the loop
/// always terminates — which is what makes this function free of the `unwrap`
/// non-negotiable 7 forbids. A sequence number is a `u32` in FIX 4.4 and this
/// is the only place this crate renders one.
fn render_u32(mut v: u32, buf: &mut [u8; 10]) -> &[u8] {
    if v == 0 {
        buf[9] = b'0';
        return &buf[9..];
    }
    let mut i = 10;
    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + u8::try_from(v % 10).unwrap_or(0);
        v /= 10;
    }
    &buf[i..]
}
