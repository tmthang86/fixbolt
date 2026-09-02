//! [`Handler`] — what an application implements — and [`App`], the adapter that
//! makes one look like a [`fixbolt_session::Application`].
//!
//! # Why there is an adapter at all
//!
//! The session hands the application **raw bytes** and expects a
//! `Range<usize>` back (`crates/session/src/lib.rs`, `Application::on_message`).
//! That signature is right for the layer it is in: it costs nothing and it
//! commits to nothing. It is the wrong signature for somebody writing a trading
//! application, who then has to re-derive the message, the routing and the
//! frame every time.
//!
//! [`App`] does that once. It parses, reverses `49`/`56`, carries the session's
//! `34` and `52` into [`Reply`], and turns [`Answer`] back into the
//! `Option<Range<usize>>` the session wants.
//!
//! # The second parse
//!
//! The session already parsed this message — it had to, to decide the message
//! was the application's at all — and its index is private. So this is the
//! second parse of the same bytes.
//!
//! `[measured 2026-09-02]` **it is the small half of what this layer costs**:
//! ~190 ns of a ~2.1 µs path, where building a `Template` per reply is the
//! other ~91%. That is the reverse of what the plan for this crate assumed, and
//! it is why [ADR-0041] does not propose removing the parse. It is **not** an
//! allocation either: the index lives in [`App`] and is reused, which
//! `benches/alloc.rs` asserts.
//!
//! [ADR-0041]: ../../../docs/decisions/ADR-0041-the-library-layer-buys-an-api-with-a-template-per-message.md

use core::ops::Range;

use fixbolt_codec::{FieldIndex, MessageView, Validation, parse_into};
use fixbolt_dict::Fix44;
use fixbolt_session::Application;

use crate::reply::{Answer, Reply};

/// A message for the application, already parsed.
///
/// Borrowed into the engine's read buffer — nothing here is owned and nothing
/// is copied. `N` is how many fields the index holds; the caller picks it, as
/// `CLAUDE.md` §6 requires, and [`Handler`]'s default is 256.
pub struct Incoming<'a, const N: usize = 256> {
    view: MessageView<'a, N>,
}

impl<'a, const N: usize> Incoming<'a, N> {
    /// `35` **MsgType**, or empty when the message carries none.
    ///
    /// Empty rather than `Option`: a message with no `MsgType` never reaches an
    /// application — the session rejects it — so a handler branching on
    /// `Option` would be writing a case it cannot be given.
    #[must_use]
    pub fn msg_type(&self) -> &'a [u8] {
        self.view.get(35).unwrap_or(b"")
    }

    /// Any field, by tag. `None` when the message does not carry it.
    #[must_use]
    pub fn get(&self, tag: u32) -> Option<&'a [u8]> {
        self.view.get(tag)
    }

    /// `34` **MsgSeqNum** as it arrived, unparsed.
    ///
    /// The bytes rather than a `u32`, because a handler that needs the number
    /// can [`as_u32`](fixbolt_codec::as_u32) it and one that only logs it
    /// should not pay for the conversion.
    #[must_use]
    pub fn seq(&self) -> Option<&'a [u8]> {
        self.view.get(34)
    }

    /// `49` **SenderCompID** — the counterparty, since this message came *from*
    /// them.
    #[must_use]
    pub fn sender(&self) -> Option<&'a [u8]> {
        self.view.get(49)
    }

    /// `56` **TargetCompID** — this side.
    #[must_use]
    pub fn target(&self) -> Option<&'a [u8]> {
        self.view.get(56)
    }

    /// The whole index, for repeating groups and anything else this type does
    /// not name.
    ///
    /// [`fixbolt_codec::GroupIter`] takes a view; this is how a handler reaches
    /// one.
    #[must_use]
    pub fn view(&self) -> MessageView<'a, N> {
        self.view
    }
}

/// What an application implements.
///
/// **This runs on the engine thread.** A handler that blocks — a lock, a
/// database call, a log flush — stops the session layer with it, and the
/// counterparty sees missed heartbeats rather than a slow application
/// ([ADR-0002](../../../docs/decisions/ADR-0002-engine-library-split.md)).
/// `docs/GUIDE.md` §2 says what to do instead.
///
/// The three sizes are the caller's, and the defaults are the ones
/// `crates/conformance/src/echo.rs` has been running the acceptance corpus with
/// since 2026-08-28: `N` fields in the inbound index, `P` fields in a reply and
/// `S` bytes of them.
pub trait Handler<const N: usize = 256, const P: usize = 64, const S: usize = 1024> {
    /// One application message, and the right to answer it once.
    ///
    /// Return `reply.silent()` to say nothing. The session's outbound sequence
    /// number moves only when a message is actually written.
    fn on_message(&mut self, msg: &Incoming<'_, N>, reply: Reply<'_, P, S>) -> Answer;
}

/// A [`Handler`] wearing the [`fixbolt_session::Application`] the engine wants.
///
/// Build one with [`app`] and hand it to [`crate::serve`].
pub struct App<H, const N: usize = 256, const P: usize = 64, const S: usize = 1024> {
    handler: H,
    /// Reused across messages. Allocating one per message would be
    /// non-negotiable 1 broken on the busiest path this crate has.
    idx: FieldIndex<N>,
    unparsable: u64,
    failed: u64,
}

/// Wrap a handler for [`crate::serve`], with [`Handler`]'s default sizes.
pub fn app<H: Handler>(handler: H) -> App<H> {
    App {
        handler,
        idx: FieldIndex::new(),
        unparsable: 0,
        failed: 0,
    }
}

impl<H, const N: usize, const P: usize, const S: usize> App<H, N, P, S> {
    /// Wrap a handler, choosing the sizes yourself.
    pub fn with_sizes(handler: H) -> Self {
        Self {
            handler,
            idx: FieldIndex::new(),
            unparsable: 0,
            failed: 0,
        }
    }

    /// Messages that reached the application and could not be parsed.
    ///
    /// **Zero on a healthy engine**, because the session framed and parsed the
    /// same bytes before handing them over. Anything else means the two parses
    /// disagree, which is a defect in this crate rather than in a counterparty
    /// — and it is counted rather than logged because the engine never logs on
    /// the hot path.
    #[must_use]
    pub fn unparsable(&self) -> u64 {
        self.unparsable
    }

    /// Replies the handler wrote that could not be encoded.
    ///
    /// The counterparty saw silence. Without this counter that silence is
    /// indistinguishable from a handler deciding not to answer — the shape
    /// `docs/reference/silence-before-a-logon-has-many-causes.md` was written
    /// about.
    #[must_use]
    pub fn failed_replies(&self) -> u64 {
        self.failed
    }

    /// The handler back, for a caller that kept nothing else.
    pub fn handler(&mut self) -> &mut H {
        &mut self.handler
    }
}

impl<H: Handler<N, P, S>, const N: usize, const P: usize, const S: usize> Application
    for App<H, N, P, S>
{
    fn on_message(
        &mut self,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        // `Validation::NONE`: the frame was checked when this message was
        // accepted, and re-checking a body length here would reject the
        // deliberately-wrong ones the acceptance corpus sends on purpose. Same
        // reason, same words, as `crates/conformance/src/echo.rs`.
        if parse_into::<Fix44, N>(msg, &mut self.idx, Validation::NONE).is_err() {
            self.unparsable += 1;
            return None;
        }
        let view = self.idx.view(msg);

        // Everything the reply needs from the message, read before the handler
        // is given anything. A message with no `8`, `49` or `56` cannot be
        // answered — and cannot have reached here either, since the session
        // routes on all three.
        let (Some(begin_string), Some(their_sender), Some(their_target)) =
            (view.get(8), view.get(49), view.get(56))
        else {
            self.unparsable += 1;
            return None;
        };

        // The reversal, in the one place it happens: this side's sender is the
        // name they addressed the message to.
        let reply = Reply::<P, S>::new(begin_string, seq, stamp, their_target, their_sender, out);
        let incoming = Incoming { view };

        let answer = self.handler.on_message(&incoming, reply);
        if matches!(answer, Answer::Failed(_)) {
            self.failed += 1;
        }
        answer.range()
    }
}
