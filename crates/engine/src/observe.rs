//! What an operator can see, without touching the hot path.
//!
//! `[verified 2026-09-01]` an `Engine`'s entire observable surface was
//! `connections()`, `sources_missing()` and `refused_connections()` — three
//! numbers, none of them about a session — and all three readable only from the
//! engine's own thread. `STATUS.md` open item 30 (b) and (f).
//!
//! # On demand, and that is the whole design
//!
//! An engine that published its state every turn would pay for an operator who
//! is not there, on the path `DESIGN.md` D8 exists to keep empty. So nothing is
//! built until somebody asks: the cost of being observable, when nobody is
//! observing, is **one relaxed load per turn**.
//!
//! [`Observer::request`] raises a flag and returns the most recent snapshot the
//! engine published — `None` until it has published one. A caller that needs a
//! fresh one asks again; there is no blocking, in either direction.
//!
//! # Why not the ring
//!
//! `DESIGN.md` D10's ring is the application path, and
//! [ADR-0011](../../../docs/decisions/ADR-0011-a-full-ring-disconnects.md) says
//! a full ring disconnects the session. **An operator asking a question must not
//! be able to drop a connection.** Two mechanisms, two purposes, and this one
//! does not get to share that risk.
//!
//! # Why the engine never blocks, and there is no `unsafe`
//!
//! `try_lock`, never `lock`. Non-negotiable 4 forbids the engine thread
//! blocking in the kernel, and a mutex can. If the reader happens to hold the
//! cell at that instant the engine skips publishing and **leaves the request
//! standing**, so the next turn does it — a reader is never starved and the
//! engine is never stopped. The alternative was a seqlock over `UnsafeCell`, and
//! [ADR-0007](../../../docs/decisions/ADR-0007-spsc-ring-without-unsafe.md)
//! already settled this house's preference: safe first, `unsafe` only when a
//! measurement asks.
//!
//! # Why the snapshot is a fixed array
//!
//! Non-negotiable 1: no allocation on the hot path. [`MAX_SESSIONS`] slots and a
//! [`Snapshot::truncated`] flag, because `hft` carries a ceiling of four
//! ([ADR-0025](../../../docs/decisions/ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md))
//! but `standard` carries none — so *"there were more than this"* is a fact to
//! report, not a case to fail on.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::ConnId;

/// How many sessions one snapshot can carry.
///
/// Above this, [`Snapshot::truncated`] is set and the rest are not described.
/// Sixteen times `hft`'s ceiling of four, and a `standard` engine holding more
/// than this has an operator problem that a longer array would not solve.
pub const MAX_SESSIONS: usize = 64;

/// One session, as an operator sees it.
///
/// Plain `Copy` data. It is a **copy taken at an instant**, not a view: by the
/// time it is read the engine has moved on, and a number that could change
/// under the reader would be worse than one that is honestly a moment old.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SessionSnapshot {
    id: ConnId,
    logged_on: bool,
    next_out: u32,
    next_in: u32,
    last_skew_ms: Option<i64>,
    has_pending_output: bool,
}

impl SessionSnapshot {
    /// The connection id, which is what a reply from another thread is routed
    /// by.
    #[must_use]
    pub const fn id(&self) -> ConnId {
        self.id
    }

    /// Has this session exchanged a `Logon`?
    #[must_use]
    pub const fn logged_on(&self) -> bool {
        self.logged_on
    }

    /// `34=` on the next message this session will send.
    #[must_use]
    pub const fn next_out(&self) -> u32 {
        self.next_out
    }

    /// `34=` this session expects next from the counterparty.
    #[must_use]
    pub const fn next_in(&self) -> u32 {
        self.next_in
    }

    /// The engine's clock minus the `SendingTime` of the last message whose
    /// `52=` could be read, in milliseconds. Positive means the counterparty's
    /// stamp is behind ours.
    ///
    /// **This is the field that answers a 3 a.m. question.** `Config`'s
    /// `max_skew_ms` silently refuses a message whose `52=` is too far from this
    /// engine's clock, and nothing else in this engine would ever say why: on a
    /// box whose NTP has drifted, a counterparty simply stops working. `None`
    /// until a message with a readable `SendingTime` has arrived.
    ///
    /// **Recorded whether that message was accepted or refused** — the refusal
    /// is the case it exists to explain.
    #[must_use]
    pub const fn last_skew_ms(&self) -> Option<i64> {
        self.last_skew_ms
    }

    /// Are there bytes queued that have not reached the socket?
    #[must_use]
    pub const fn has_pending_output(&self) -> bool {
        self.has_pending_output
    }
}

impl SessionSnapshot {
    /// Build one. `pub(crate)`: only the engine describes a session.
    pub(crate) const fn describe(
        id: ConnId,
        logged_on: bool,
        next_out: u32,
        next_in: u32,
        last_skew_ms: Option<i64>,
        has_pending_output: bool,
    ) -> Self {
        Self {
            id,
            logged_on,
            next_out,
            next_in,
            last_skew_ms,
            has_pending_output,
        }
    }
}

/// One engine, at one instant.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    sessions: [SessionSnapshot; MAX_SESSIONS],
    len: usize,
    truncated: bool,
    connections: usize,
    refused_connections: usize,
    sources_missing: usize,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            sessions: [SessionSnapshot::default(); MAX_SESSIONS],
            len: 0,
            truncated: false,
            connections: 0,
            refused_connections: 0,
            sources_missing: 0,
        }
    }
}

impl Snapshot {
    /// Every session this engine held, up to [`MAX_SESSIONS`].
    #[must_use]
    pub fn sessions(&self) -> &[SessionSnapshot] {
        self.sessions.get(..self.len).unwrap_or(&[])
    }

    /// Were there more sessions than the snapshot could carry?
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// How many connections the engine held, whether described here or not.
    #[must_use]
    pub const fn connections(&self) -> usize {
        self.connections
    }

    /// Connections ended because the dispatch would not take a message —
    /// ADR-0011. **Zero on a healthy engine.**
    #[must_use]
    pub const fn refused_connections(&self) -> usize {
        self.refused_connections
    }

    /// Is this engine healthy enough to serve? Item 30 (f), and **a pure
    /// function on data already gathered** — no I/O, no second mechanism, so a
    /// health endpoint and an operator's `Debug` print can never disagree.
    ///
    /// Healthy means: at least one session, every one of them logged on, and
    /// neither counter that is only ever non-zero on a broken engine. It does
    /// **not** look at [`Self::truncated`]: more sessions than the snapshot can
    /// carry is a reporting limit, not an unhealthy engine.
    #[must_use]
    pub fn healthy(&self) -> bool {
        !self.sessions().is_empty()
            && self.sessions().iter().all(SessionSnapshot::logged_on)
            && self.refused_connections == 0
            && self.sources_missing == 0
    }

    /// Add one session, or record that it did not fit.
    ///
    /// `pub(crate)`: only the engine builds one of these. Beyond
    /// [`MAX_SESSIONS`] the session is dropped and [`Self::truncated`] is set —
    /// `standard` has no session ceiling, so *"there were more"* is a fact to
    /// report rather than a case to fail on.
    pub(crate) const fn push(&mut self, s: SessionSnapshot) {
        if self.len < MAX_SESSIONS {
            self.sessions[self.len] = s;
            self.len += 1;
        } else {
            self.truncated = true;
        }
    }

    /// Fill in the engine-wide counters. `pub(crate)`: only the engine builds
    /// one of these.
    pub(crate) const fn set_counters(
        &mut self,
        connections: usize,
        refused_connections: usize,
        sources_missing: usize,
    ) {
        self.connections = connections;
        self.refused_connections = refused_connections;
        self.sources_missing = sources_missing;
    }

    /// Connections that claimed to be pollable and produced no descriptor.
    /// **Zero on a healthy engine**; anything else is a `Transport` breaking its
    /// own contract, and the symptom is lateness rather than a crash.
    #[must_use]
    pub const fn sources_missing(&self) -> usize {
        self.sources_missing
    }
}

/// What the engine and the operator share. One allocation, at
/// [`crate::Engine::observer`], and never on a turn.
#[derive(Debug)]
pub(crate) struct Shared {
    /// Somebody asked. Cleared when the engine publishes.
    wanted: AtomicBool,
    /// How many snapshots the engine has built. **The number that keeps *"on
    /// demand"* honest** — an implementation publishing every turn passes every
    /// content assertion and fails this one.
    published: AtomicU64,
    cell: Mutex<Snapshot>,
    /// Events, which unlike a snapshot are **pushed** rather than requested:
    /// an event nobody asked for at the right moment is an event lost, and the
    /// whole point is that they are not.
    pub(crate) events: Events,
}

impl Shared {
    pub(crate) fn new() -> Self {
        Self {
            wanted: AtomicBool::new(false),
            published: AtomicU64::new(0),
            cell: Mutex::new(Snapshot::default()),
            events: Events::new(),
        }
    }

    /// Has somebody asked? One relaxed load, and it is the entire cost of being
    /// observable while nobody is observing.
    pub(crate) fn wanted(&self) -> bool {
        self.wanted.load(Ordering::Relaxed)
    }

    /// Record one event. See [`Events::push`] — `try_lock`, and a refusal is
    /// counted rather than waited on.
    pub(crate) fn emit(&self, id: ConnId, at_ms: u64, kind: EventKind) {
        self.events.push(Event { id, at_ms, kind });
    }

    /// Publish, unless the reader holds the cell right now.
    ///
    /// `try_lock`, never `lock` — non-negotiable 4. A refusal leaves `wanted`
    /// set, so the next turn publishes and the reader is not starved.
    pub(crate) fn publish(&self, snap: &Snapshot) {
        if let Ok(mut slot) = self.cell.try_lock() {
            *slot = *snap;
            self.wanted.store(false, Ordering::Release);
            self.published.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// An operator's handle on a running engine. `Send + Sync`; hold it on whatever
/// thread does the asking.
#[derive(Debug, Clone)]
pub struct Observer(pub(crate) std::sync::Arc<Shared>);

impl Observer {
    /// Ask for a snapshot, and take the most recent one the engine published.
    ///
    /// **Returns what is there now, and asks for a fresh one** — it does not
    /// wait, in either direction. `None` before the engine has published
    /// anything at all. A caller wanting a snapshot taken *after* this call
    /// asks again; a caller wanting one at all polls until it is `Some`.
    ///
    /// Blocking here would put the operator's thread at the mercy of the engine
    /// and vice versa, which is the coupling this whole module exists to avoid.
    #[must_use]
    pub fn request(&self) -> Option<Snapshot> {
        self.0.wanted.store(true, Ordering::Release);
        if self.0.published.load(Ordering::Acquire) == 0 {
            return None;
        }
        self.0.cell.lock().ok().map(|s| *s)
    }

    /// Append every event the engine has recorded, oldest first, and remove
    /// them from the ring. Returns how many were appended.
    ///
    /// **A `Vec` on purpose.** The reader is not the engine thread and may
    /// allocate; the engine's side of this is a fixed ring and allocates
    /// nothing, which is what non-negotiable 1 is about. Reusing one `Vec`
    /// across calls costs nothing after the first.
    ///
    /// Whatever it could not keep is counted by [`Self::events_lost`], which is
    /// **not** implied by the return value — a drain of zero with a rising loss
    /// count means the reader is being starved by its own timing, not that
    /// nothing happened.
    pub fn events(&self, out: &mut Vec<Event>) -> usize {
        self.0.events.drain(out)
    }

    /// How many events were never delivered — the ring was full, or the engine
    /// could not take the lock without blocking.
    ///
    /// **Monotonic, and it is the number that makes this stream honest.** A
    /// stream that loses silently is a source an operator will trust and should
    /// not. Non-zero means read more often, or with a bigger buffer.
    #[must_use]
    pub fn events_lost(&self) -> u64 {
        self.0.events.lost.load(Ordering::Relaxed)
    }

    /// How many snapshots the engine has built since it started.
    ///
    /// For a test, and for an operator wondering whether the engine is turning
    /// at all. **It is the assertion that keeps this module's central claim
    /// honest**: an engine publishing on every turn would satisfy every other
    /// question here and pay for it on the hot path.
    #[must_use]
    pub fn published(&self) -> u64 {
        self.0.published.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SESSIONS, SessionSnapshot, Snapshot};

    fn session(id: u64, logged_on: bool) -> SessionSnapshot {
        SessionSnapshot::describe(id, logged_on, 1, 1, None, false)
    }

    /// More sessions than the array holds is a **reported fact**, not a panic
    /// and not a silent short list. `standard` has no session ceiling.
    #[test]
    fn a_snapshot_that_cannot_carry_every_session_says_so() {
        let mut snap = Snapshot::default();
        for i in 0..MAX_SESSIONS {
            snap.push(session(i as u64, true));
        }
        assert!(!snap.truncated(), "exactly full is not truncated");
        assert_eq!(snap.sessions().len(), MAX_SESSIONS);

        snap.push(session(MAX_SESSIONS as u64, true));
        assert!(snap.truncated(), "one past the end is");
        assert_eq!(
            snap.sessions().len(),
            MAX_SESSIONS,
            "and the ones that fit are still all there"
        );
    }

    /// The health probe, item 30 (f). Each clause gets its own case, because a
    /// probe that is wrong in one direction reports a healthy engine as dead at
    /// 3 a.m. and a dead one as healthy for a whole trading session.
    #[test]
    fn health_is_every_session_logged_on_and_no_counter_that_should_be_zero() {
        let mut snap = Snapshot::default();
        assert!(
            !snap.healthy(),
            "an engine serving nobody is not a healthy acceptor"
        );

        snap.push(session(1, true));
        assert!(snap.healthy(), "one logged-on session, nothing broken");

        snap.push(session(2, false));
        assert!(
            !snap.healthy(),
            "a connection that never logged on is exactly what a probe is for"
        );

        let mut refused = Snapshot::default();
        refused.push(session(1, true));
        refused.set_counters(1, 1, 0);
        assert!(!refused.healthy(), "ADR-0011 dropped a session");

        let mut missing = Snapshot::default();
        missing.push(session(1, true));
        missing.set_counters(1, 0, 1);
        assert!(
            !missing.healthy(),
            "a transport broke its own contract; the symptom is lateness"
        );

        // Truncation is a reporting limit, not a sickness.
        let mut full = Snapshot::default();
        for i in 0..=MAX_SESSIONS {
            full.push(session(i as u64, true));
        }
        assert!(full.truncated());
        assert!(
            full.healthy(),
            "more sessions than we can list is not unhealthy"
        );
    }
}

/// How many events the buffer holds before the oldest are lost.
///
/// Events are **rare** — a logon, a logout, a gap, a disconnect — not one per
/// message, which D8 would forbid. A reader polling once a second has room for
/// a burst; one that stops reading loses the oldest and
/// [`Observer::events_lost`] says how many.
pub const EVENT_CAPACITY: usize = 256;

/// Something worth telling an operator about.
///
/// Deliberately **not** one per message: that is the hot path and `DESIGN.md`
/// D8 forbids work there for an observer who may not exist. These are the state
/// changes — a session came up, a session went away and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventKind {
    /// A session finished its `Logon` exchange.
    LoggedOn,
    /// A connection ended. **The reason is the point of this whole module** —
    /// see [`fixbolt_session::DropReason`].
    Ended(fixbolt_session::DropReason),
    /// A connection ended and the session had no reason to give. **Zero on a
    /// healthy engine**: it means something ended the link without going
    /// through `Session::end`, which is a gap in this engine rather than a
    /// fault of the counterparty.
    EndedWithoutReason,
}

/// One event, with enough context to act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    id: ConnId,
    at_ms: u64,
    kind: EventKind,
}

impl Event {
    /// Which connection.
    #[must_use]
    pub const fn id(&self) -> ConnId {
        self.id
    }

    /// When, on the engine's clock.
    #[must_use]
    pub const fn at_ms(&self) -> u64 {
        self.at_ms
    }

    /// What happened.
    #[must_use]
    pub const fn kind(&self) -> EventKind {
        self.kind
    }
}

/// A fixed ring of events plus a count of what it could not keep.
///
/// **The count is not optional.** An event stream that loses silently is worse
/// than no event stream: it is a source an operator will trust and should not.
#[derive(Debug)]
pub(crate) struct Events {
    ring: Mutex<EventRing>,
    /// Bumped when the engine could not take the lock, or when the ring was
    /// full. Read without the lock, so a reader can always learn it has missed
    /// something even while the engine holds the cell.
    lost: AtomicU64,
}

#[derive(Debug)]
struct EventRing {
    slots: [Option<Event>; EVENT_CAPACITY],
    head: usize,
    len: usize,
}

impl Events {
    pub(crate) fn new() -> Self {
        Self {
            ring: Mutex::new(EventRing {
                slots: [None; EVENT_CAPACITY],
                head: 0,
                len: 0,
            }),
            lost: AtomicU64::new(0),
        }
    }

    /// Record an event, unless the reader holds the ring right now.
    ///
    /// `try_lock`, never `lock` — non-negotiable 4. A refusal counts as a loss
    /// rather than blocking the engine thread, which is the trade this design
    /// makes and states.
    pub(crate) fn push(&self, e: Event) {
        let Ok(mut r) = self.ring.try_lock() else {
            self.lost.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if r.len == EVENT_CAPACITY {
            // Full: drop the oldest, and say so.
            let head = r.head;
            r.slots[head] = Some(e);
            r.head = (head + 1) % EVENT_CAPACITY;
            self.lost.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let at = (r.head + r.len) % EVENT_CAPACITY;
        r.slots[at] = Some(e);
        r.len += 1;
    }

    fn drain(&self, out: &mut Vec<Event>) -> usize {
        let Ok(mut r) = self.ring.lock() else {
            return 0;
        };
        let mut n = 0;
        while r.len > 0 {
            let head = r.head;
            if let Some(e) = r.slots[head].take() {
                out.push(e);
                n += 1;
            }
            r.head = (head + 1) % EVENT_CAPACITY;
            r.len -= 1;
        }
        n
    }
}
