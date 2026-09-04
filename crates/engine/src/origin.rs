//! The other-thread door: a message an application originates from wherever it
//! happens to be running. [ADR-0048] door 2.
//!
//! # Why this is not a fourth [`Command`]
//!
//! [`Command`] is `Copy`, fixed-size, and rides a queue of `Option<Command>`. A
//! message body is none of those. So origination gets its own queue — fixed
//! slots, filled once at construction, never grown — on the same `Arc` that
//! [`Observer`] and [`Admin`] already ride, with the same capability split: an
//! `Observer` cannot send and a [`Sender`] can.
//!
//! # What it copies from [`Command`], and why
//!
//! * **`submit` answers `false` at the call** when the queue is full or the
//!   message is longer than a slot. A lost origination is never silent.
//! * **The engine drains with `try_lock`, never `lock`** — non-negotiable 4.
//!   A refused lock takes nothing and loses nothing; the next turn tries again.
//! * **A relaxed `waiting` load comes before the lock is attempted**, so an
//!   engine nobody sends through pays one load per turn rather than a mutex.
//!   [`Sender::drains`] is what keeps that claim falsifiable.
//!
//! [ADR-0048]: ../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md
//! [`Command`]: crate::observe::Command
//! [`Observer`]: crate::observe::Observer
//! [`Admin`]: crate::observe::Admin

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::dispatch::ConnId;

/// How many originated messages may wait for the engine's next turn.
///
/// Filled once when the engine is built — `ORIGIN_CAPACITY × ORIGIN_LEN` bytes,
/// 32 KiB at the defaults — and never grown. A queue that is full refuses at
/// [`Sender::send`].
pub const ORIGIN_CAPACITY: usize = 64;

/// The longest message [`Sender::send`] will take, in bytes.
///
/// **The fifth ceiling**, after the four
/// [ADR-0047](../../../docs/decisions/ADR-0047-the-four-buffer-sizes-are-the-callers-through-a-second-function.md)
/// named — and unlike `Outbound::app`, the one it is most like, this one fails
/// *visibly*: a message that does not fit is refused at the call and the caller
/// is told `false`. It matches `journal::Store`'s slot length, because a
/// message too long to keep for a resend is a message that should not have gone
/// out.
pub const ORIGIN_LEN: usize = 512;

#[derive(Clone, Copy)]
struct Slot {
    id: ConnId,
    len: u16,
    bytes: [u8; ORIGIN_LEN],
}

impl Slot {
    const EMPTY: Self = Self {
        id: 0,
        len: 0,
        bytes: [0; ORIGIN_LEN],
    };
}

struct Queue {
    /// A `Vec` filled to `ORIGIN_CAPACITY` in [`Origin::new`] and never resized.
    ///
    /// Heap rather than inline because [`crate::observe::Shared`] is built by
    /// value before it is put in an `Arc`, and 32 KiB of stack temporary to
    /// construct a queue is a cost with no upside. **The allocation happens
    /// once, when the engine is built**; nothing here allocates afterwards,
    /// which `benches/alloc.rs` asserts.
    slots: Vec<Slot>,
    head: usize,
    len: usize,
}

/// The queue itself. Held by [`crate::observe::Shared`].
pub(crate) struct Origin {
    queue: Mutex<Queue>,
    /// How many are waiting. **Read before the lock is even attempted.**
    waiting: AtomicUsize,
    /// How many times the engine has reached for the lock. The number that
    /// keeps *"one relaxed load"* honest.
    drains: AtomicU64,
    /// Messages drained for a connection that had already gone.
    undeliverable: AtomicU64,
}

impl core::fmt::Debug for Origin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Origin")
            .field("waiting", &self.waiting.load(Ordering::Relaxed))
            .field("drains", &self.drains.load(Ordering::Relaxed))
            .field("undeliverable", &self.undeliverable.load(Ordering::Relaxed))
            .finish()
    }
}

impl Origin {
    pub(crate) fn new() -> Self {
        Self {
            queue: Mutex::new(Queue {
                slots: vec![Slot::EMPTY; ORIGIN_CAPACITY],
                head: 0,
                len: 0,
            }),
            waiting: AtomicUsize::new(0),
            drains: AtomicU64::new(0),
            undeliverable: AtomicU64::new(0),
        }
    }

    /// Called from the application's thread, which may block.
    fn submit(&self, id: ConnId, msg: &[u8]) -> bool {
        if msg.is_empty() || msg.len() > ORIGIN_LEN {
            return false;
        }
        let Ok(mut q) = self.queue.lock() else {
            return false;
        };
        if q.len == ORIGIN_CAPACITY {
            return false;
        }
        let at = (q.head + q.len) % ORIGIN_CAPACITY;
        let Some(slot) = q.slots.get_mut(at) else {
            return false;
        };
        slot.id = id;
        slot.len = u16::try_from(msg.len()).unwrap_or(0);
        let Some(d) = slot.bytes.get_mut(..msg.len()) else {
            return false;
        };
        d.copy_from_slice(msg);
        q.len += 1;
        // Release, and inside the lock: the engine's relaxed load may be stale
        // by one turn, which costs a turn's delay and never a lost message.
        self.waiting.store(q.len, Ordering::Release);
        true
    }

    /// Is there anything at all? **One relaxed load, and it is the entire cost
    /// of being sendable-to while nobody is sending.**
    pub(crate) fn waiting(&self) -> bool {
        self.waiting.load(Ordering::Relaxed) != 0
    }

    /// Called on the engine thread. `try_lock`, never `lock`.
    ///
    /// Hands each waiting message to `emit` in the order it was submitted. A
    /// refused lock takes nothing and loses nothing.
    pub(crate) fn drain<F: FnMut(ConnId, &[u8]) -> bool>(&self, mut emit: F) -> usize {
        self.drains.fetch_add(1, Ordering::Relaxed);
        let Ok(mut q) = self.queue.try_lock() else {
            return 0;
        };
        let n = q.len;
        let mut gone = 0u64;
        for _ in 0..n {
            let head = q.head;
            // Copied out of the lock's guard before `emit` runs: `emit` reaches
            // into the engine's connections, and holding a borrow of the queue
            // across that would put the two lifetimes in one expression for no
            // reason. `ORIGIN_LEN` is 512, so this is one small memcpy per
            // message actually sent.
            let (id, len, bytes) = match q.slots.get(head) {
                Some(s) => (s.id, usize::from(s.len), s.bytes),
                None => break,
            };
            q.head = (head + 1) % ORIGIN_CAPACITY;
            if !emit(id, bytes.get(..len).unwrap_or(b"")) {
                gone += 1;
            }
        }
        q.len = 0;
        self.waiting.store(0, Ordering::Release);
        if gone > 0 {
            self.undeliverable.fetch_add(gone, Ordering::Relaxed);
        }
        n
    }

    pub(crate) fn drains(&self) -> u64 {
        self.drains.load(Ordering::Relaxed)
    }

    pub(crate) fn undeliverable(&self) -> u64 {
        self.undeliverable.load(Ordering::Relaxed)
    }
}

/// The right to make an engine say something it was not asked for.
///
/// `Send + Sync + Clone`. Hold it on the thread that learns a fill happened, or
/// the one that produces a quote, or whichever thread has something to say that
/// no inbound message prompted.
///
/// **Nothing here waits for the engine.** [`Self::send`] copies the message into
/// a queue and returns; the engine takes it at the top of its next turn and
/// hands it to the session, which assigns `34=` and `52=`. What comes back is
/// whether the message was *taken*, never whether it was *sent* — a connection
/// can go between the two, and
/// [`EventKind::OriginationUndeliverable`](crate::observe::EventKind::OriginationUndeliverable)
/// is where that reports itself.
#[derive(Debug, Clone)]
pub struct Sender(pub(crate) std::sync::Arc<crate::observe::Shared>);

impl Sender {
    /// Queue one whole FIX message for `id`, to be sent on the engine's next
    /// turn.
    ///
    /// `msg` is a complete message — `8=…`, a `35=`, the body. **Write no `34=`
    /// and no `52=`**: the session writes both, along with `9=` and `10=`, and
    /// ignores whatever was there. `49=` must be this side and `56=` the
    /// counterparty; [`crate::observe::Snapshot`] names both for a caller that
    /// does not already know them.
    ///
    /// `false` means **nothing was taken**, for one of two reasons the caller
    /// can tell apart by looking at the message: it was empty or longer than
    /// [`ORIGIN_LEN`], or the queue was full ([`ORIGIN_CAPACITY`]). Either way
    /// the answer arrives *now*, at the call, rather than as a message that
    /// quietly never goes out.
    ///
    /// **`true` means queued, not sent.** A session that is not logged on
    /// discards it silently, which is the same answer
    /// [`fixbolt_session::Session::send_application`] gives an initiator that
    /// speaks too early.
    pub fn send(&self, id: ConnId, msg: &[u8]) -> bool {
        self.0.origin.submit(id, msg)
    }

    /// How many times the engine has reached for the origination lock.
    ///
    /// **This is what keeps *"a turn costs one relaxed load"* falsifiable.** An
    /// engine that attempts the mutex every turn behaves identically in every
    /// other respect and fails only this;
    /// `crates/engine/tests/originate.rs::an_engine_nobody_sends_through_does_not_reach_for_the_lock`
    /// is what notices. The same role [`crate::observe::Admin::drains`] plays
    /// for commands, and it exists because that gap has already been found once
    /// in this crate.
    #[must_use]
    pub fn drains(&self) -> u64 {
        self.0.origin.drains()
    }

    /// Messages drained for a connection that had already gone.
    ///
    /// **Zero on a healthy engine.** The running total; see
    /// [`EventKind::OriginationUndeliverable`](crate::observe::EventKind::OriginationUndeliverable)
    /// for the per-turn event.
    #[must_use]
    pub fn undeliverable(&self) -> u64 {
        self.0.origin.undeliverable()
    }
}
