//! Where a message goes once the session layer has finished with it.
//!
//! `DESIGN.md` D4 and [ADR-0002]: dispatch is a trait, **inline is the
//! default**, and the ring is the option for an application that may block.
//! The reversal is recorded in the ADR — the first draft had it the other way
//! round, on reasoning that turned out to be about the JVM.
//!
//! [ADR-0002]: ../../../docs/decisions/ADR-0002-engine-library-split.md
//!
//! # The two shapes
//!
//! | | Where the handler runs | Reply |
//! |---|---|---|
//! | [`InlineDispatch`] | the engine thread, immediately | returned from the same call |
//! | [`RingDispatch`] | another thread | comes back later, through [`Dispatch::collect`] |
//!
//! An inline reply costs nothing but the call. A ring reply costs a copy each
//! way and a turn of latency, and buys the one thing inline cannot: an
//! application that stalls does not stall the session layer.

use core::ops::Range;

use fixbolt_session::Application;

use crate::ring::{Consumer, Producer};

/// Which connection a message belongs to.
///
/// A number rather than an index: `Engine` removes a dead connection with
/// `swap_remove`, so an index is stale the moment anything hangs up, and a
/// reply routed by a stale index goes to the wrong counterparty.
pub type ConnId = u64;

/// The stamp the session hands an application, in bytes. 21 with milliseconds.
pub const STAMP: usize = 21;

/// What the engine does with a message the session accepted.
pub trait Dispatch {
    /// Whether [`Self::collect`] can ever produce anything.
    ///
    /// `false` for [`InlineDispatch`], and the engine then never calls
    /// `collect` at all — the branch is a constant and compiles away.
    const OUT_OF_BAND: bool;

    /// One message for the application.
    ///
    /// The contract is [`fixbolt_session::Application::on_message`]'s: write a
    /// whole FIX message into `out` and return its range to have it sent on
    /// this connection now. `None` says nothing — which for [`RingDispatch`]
    /// is *always* the answer, because the application is not on this thread.
    fn deliver(
        &mut self,
        conn: ConnId,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>>;

    /// Whatever the application produced somewhere else since the last turn.
    ///
    /// The engine sends each one through `Session::send_application`, so the
    /// sequence number and `SendingTime` are the session's own — the
    /// application does not have to know either, and cannot get them wrong.
    fn collect<F: FnMut(ConnId, &[u8])>(&mut self, emit: F);

    /// Whether this dispatch turned a message away since it was last asked.
    ///
    /// The engine asks **immediately after one connection's turn**, and a
    /// `true` therefore belongs to *that* connection: [`deliver`] is reached
    /// through an adapter built for one connection id and nothing else runs in
    /// between. That is why no id is carried here and why nothing is stored.
    ///
    /// A `true` ends the connection with a `Logout` naming the reason —
    /// [ADR-0011](../../../docs/decisions/ADR-0011-a-full-ring-disconnects.md)
    /// decision 1. A message the session has already accepted, numbered,
    /// journalled and acknowledged, which the application never sees, is not
    /// backpressure; it is silent loss, and for order flow the counterparty
    /// must be told.
    ///
    /// Defaults to `false`, which is the whole truth for [`InlineDispatch`]:
    /// the handler is on this thread, so there is nothing to refuse. The branch
    /// then folds away like [`Dispatch::OUT_OF_BAND`] does.
    ///
    /// Proven by `crates/engine/tests/dispatch.rs`:
    /// `a_full_ring_ends_the_connection_and_says_why`, with
    /// `a_ring_with_room_ends_nothing` and
    /// `inline_dispatch_never_ends_a_connection` as its other halves.
    ///
    /// [`deliver`]: Dispatch::deliver
    fn take_refusal(&mut self) -> bool {
        false
    }
}

/// The handler runs on the engine thread. The default (ADR-0002).
pub struct InlineDispatch<H> {
    handler: H,
}

impl<H> InlineDispatch<H> {
    /// Wrap an application.
    pub const fn new(handler: H) -> Self {
        Self { handler }
    }

    /// The application, for a caller that wants to look at what it recorded.
    pub const fn handler_mut(&mut self) -> &mut H {
        &mut self.handler
    }
}

impl<H: Application> Dispatch for InlineDispatch<H> {
    const OUT_OF_BAND: bool = false;

    fn deliver(
        &mut self,
        _conn: ConnId,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        out: &mut [u8],
    ) -> Option<Range<usize>> {
        self.handler.on_message(msg, seq, stamp, out)
    }

    fn collect<F: FnMut(ConnId, &[u8])>(&mut self, _emit: F) {}
}

/// The engine's end of a ring to an application on another thread.
///
/// `M` is the longest message either direction will carry; anything longer is
/// refused on the way out and dropped on the way back, and both are counted.
/// A silent truncation would be a FIX message that no longer checksums.
pub struct RingDispatch<const M: usize> {
    to_app: Producer,
    from_app: Consumer,
    scratch: [u8; M],
    refused: usize,
    /// Set by `deliver`, taken by the engine after this connection's turn.
    /// A `bool` rather than a list of ids: see [`Dispatch::take_refusal`].
    refused_since: bool,
    dropped: usize,
}

/// The layout of a record going out to the application.
///
/// `conn` and `seq` so a reply can be routed and correlated, and `stamp`
/// because [`Application::on_message`]'s contract says a reply must not
/// regenerate its own `SendingTime` — the session patches it again on the way
/// out, but the application still has to write a well-formed message.
const OUT_HEADER: usize = 8 + 4 + STAMP;

/// The layout coming back: just the connection.
const BACK_HEADER: usize = 8;

impl<const M: usize> RingDispatch<M> {
    /// The engine's end. [`RingApp`] is the other one.
    #[must_use]
    pub const fn new(to_app: Producer, from_app: Consumer) -> Self {
        Self {
            to_app,
            from_app,
            scratch: [0; M],
            refused: 0,
            refused_since: false,
            dropped: 0,
        }
    }

    /// Messages the ring would not take, because it was full or the message was
    /// longer than `M`.
    ///
    /// **Zero on a healthy engine.** Since ADR-0011 each one of these also ends
    /// the connection it happened on, so this counter is a diagnosis rather
    /// than a running toll: it says how many sessions were dropped because the
    /// application could not keep up. `Engine::refused_connections` counts the
    /// same events from the engine's side.
    #[must_use]
    pub const fn refused(&self) -> usize {
        self.refused
    }

    /// Replies that came back longer than `M` and were therefore lost.
    #[must_use]
    pub const fn dropped(&self) -> usize {
        self.dropped
    }
}

impl<const M: usize> Dispatch for RingDispatch<M> {
    const OUT_OF_BAND: bool = true;

    fn deliver(
        &mut self,
        conn: ConnId,
        msg: &[u8],
        seq: u32,
        stamp: &[u8],
        _out: &mut [u8],
    ) -> Option<Range<usize>> {
        let mut fixed = [0u8; OUT_HEADER];
        fixed[..8].copy_from_slice(&conn.to_le_bytes());
        fixed[8..12].copy_from_slice(&seq.to_le_bytes());
        let n = stamp.len().min(STAMP);
        fixed[12..12 + n].copy_from_slice(&stamp[..n]);
        if msg.len() > M || !self.to_app.push(&[&fixed, msg]) {
            self.refused += 1;
            self.refused_since = true;
        }
        // Never a reply on this call. The application is elsewhere.
        None
    }

    fn collect<F: FnMut(ConnId, &[u8])>(&mut self, mut emit: F) {
        loop {
            let Some(n) = self.from_app.pop(&mut self.scratch) else {
                return;
            };
            if n < BACK_HEADER {
                // `Some(0)` is the ring saying it threw away a record that did
                // not fit `M`. Anything else this short is not a record at all.
                self.dropped += 1;
                continue;
            }
            let mut id = [0u8; 8];
            id.copy_from_slice(&self.scratch[..8]);
            emit(ConnId::from_le_bytes(id), &self.scratch[BACK_HEADER..n]);
        }
    }

    fn take_refusal(&mut self) -> bool {
        core::mem::replace(&mut self.refused_since, false)
    }
}

/// The application's end of the ring. Runs on the application's thread.
///
/// It owns no session and no socket: it takes messages, calls the handler, and
/// puts whatever it says back on the return ring. Everything about sequence
/// numbers and timestamps stays on the engine thread, where the session is.
pub struct RingApp<const M: usize> {
    from_engine: Consumer,
    to_engine: Producer,
    scratch: [u8; M],
    reply: [u8; M],
    dropped: usize,
    /// Whom to tell that a reply is waiting.
    ///
    /// `None` in `hft`, where the engine is spinning and will see the reply on
    /// its next turn regardless. In `standard` the engine is asleep inside
    /// `poll`, which wakes for descriptors and not for a ring buffer — so
    /// without this a reply waits out the engine's whole timeout.
    #[cfg(all(feature = "standard", unix))]
    waker: Option<crate::waker::WakeHandle>,
}

impl<const M: usize> RingApp<M> {
    /// The application's end. [`RingDispatch`] is the other one.
    #[must_use]
    pub const fn new(from_engine: Consumer, to_engine: Producer) -> Self {
        Self {
            from_engine,
            to_engine,
            scratch: [0; M],
            reply: [0; M],
            dropped: 0,
            #[cfg(all(feature = "standard", unix))]
            waker: None,
        }
    }

    /// The same end, waking a `standard` engine each time a reply goes back.
    ///
    /// Get the handle from the [`crate::waker::Waker`] handed to
    /// [`crate::Engine::with_waker`]. Leaving it out costs nothing in `hft` and
    /// costs one whole timeout per reply in `standard`.
    #[cfg(all(feature = "standard", unix))]
    #[must_use]
    pub fn with_waker(mut self, waker: crate::waker::WakeHandle) -> Self {
        self.waker = Some(waker);
        self
    }

    /// Replies the return ring would not take. See [`RingDispatch::refused`].
    #[must_use]
    pub const fn dropped(&self) -> usize {
        self.dropped
    }

    /// Run the handler over everything waiting. Returns how many it handled.
    ///
    /// Never blocks: an empty ring is `0`, and what the application thread does
    /// then — park, spin, sleep — is the application's choice, not this crate's.
    pub fn pump<H: Application>(&mut self, handler: &mut H) -> usize {
        let mut done = 0;
        loop {
            let Some(n) = self.from_engine.pop(&mut self.scratch) else {
                return done;
            };
            if n < OUT_HEADER {
                self.dropped += 1;
                continue;
            }
            done += 1;
            let mut conn = [0u8; 8];
            conn.copy_from_slice(&self.scratch[..8]);
            let mut seq = [0u8; 4];
            seq.copy_from_slice(&self.scratch[8..12]);
            let seq = u32::from_le_bytes(seq);
            // Split the borrow: the handler reads the inbound message out of
            // `scratch` and writes into `reply`, and they are different fields.
            let Self {
                scratch,
                reply,
                to_engine,
                dropped,
                ..
            } = self;
            let (stamp, msg) = (&scratch[12..OUT_HEADER], &scratch[OUT_HEADER..n]);
            let mut pushed = false;
            if let Some(r) = handler.on_message(msg, seq, stamp, reply) {
                if to_engine.push(&[&conn, &reply[r]]) {
                    pushed = true;
                } else {
                    *dropped += 1;
                }
            }
            // **After the push, not before.** Waking first would send the
            // engine to look at a ring that does not hold the reply yet, and
            // the reply would then wait for the next wake or the timeout.
            #[cfg(all(feature = "standard", unix))]
            if pushed && let Some(w) = &self.waker {
                w.wake();
            }
            #[cfg(not(all(feature = "standard", unix)))]
            let _ = pushed;
        }
    }
}
