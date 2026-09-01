//! One connection: a socket, a receive buffer, a state machine, and whatever
//! could not be written yet.

use fixbolt_session::journal::Journal as SessionJournal;
use fixbolt_session::{Application, Link, Role, Session};

use crate::backpressure::{Backpressure, SLOW_APPLICATION, SLOW_CONSUMER};
use crate::dispatch::ConnId;
use crate::frame::{Cut, Framer};
use crate::transport::{Io, Transport};

/// What happened to a connection on one turn of the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// Still up. `true` if anything at all moved — a byte in, a byte out, a
    /// message judged. The loop uses it to decide whether it is idle.
    Up(bool),
    /// Finished. The engine drops it after this.
    Gone,
}

/// One connection.
///
/// `N` sizes the session's field index, `RX` its receive buffer and `TX` the
/// bytes it has written but the socket has not taken. All three are the
/// caller's choice and none is a hidden constant — `CLAUDE.md` §6.
pub struct Connection<T, R: Role, J, const N: usize, const RX: usize, const TX: usize> {
    /// Which connection this is, for routing a reply that comes back from
    /// another thread. Never reused: the engine only ever counts up.
    pub id: ConnId,
    /// The socket. `None` once it has been given up.
    pub transport: T,
    pub session: Session<R, N>,
    /// What this connection has already sent, for a `ResendRequest` to be
    /// answered from. `DESIGN.md` D7 — the session says *keep this*, the
    /// journal decides how and whether it survives a restart.
    pub journal: J,
    rx: Framer<RX>,
    /// Written by the session, not yet accepted by the socket.
    tx: [u8; TX],
    tx_len: usize,
    /// Set when the session says the link is down, so the engine can drop it
    /// after the last bytes have been pushed out.
    closing: bool,
    /// Set when the **socket** is gone. Different from `closing`: a closing
    /// connection still has bytes to write, a dead one has nowhere to write
    /// them, and waiting for a queue to drain into a dead socket is a
    /// connection that never leaves.
    dead: bool,
    /// What to do when `tx` will not take the next message. `DESIGN.md` D10.
    policy: Backpressure,
    /// Set when a message did not fit. Read once per turn, and answered with a
    /// `Logout(58=slow consumer)`.
    overflow: bool,
}

impl<T: Transport, R: Role, J: SessionJournal, const N: usize, const RX: usize, const TX: usize>
    Connection<T, R, J, N, RX, TX>
{
    /// Wrap a socket and a session that has not been told about it yet.
    pub const fn new(id: ConnId, transport: T, session: Session<R, N>, journal: J) -> Self {
        Self {
            id,
            transport,
            session,
            journal,
            rx: Framer::new(),
            tx: [0; TX],
            tx_len: 0,
            closing: false,
            dead: false,
            policy: Backpressure::Disconnect,
            overflow: false,
        }
    }

    /// Bytes that arrived on this socket **before this connection existed**.
    ///
    /// The pre-session stage reads a `Logon` to decide where the socket goes
    /// ([ADR-0020]), and the session must still see it. This is how it gets
    /// there: straight into the receive buffer, so the very first `turn` frames
    /// it exactly as if the engine had read it itself.
    ///
    /// `false` when the bytes do not fit, and the caller must then refuse the
    /// connection. **Not truncated**: half a message would be framed as
    /// `Garbage` about bytes that were fine when they arrived, which destroys
    /// the evidence for the defect that caused it.
    ///
    /// [ADR-0020]: ../../../docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md
    #[must_use]
    pub fn prime(&mut self, bytes: &[u8]) -> bool {
        let spare = self.rx.spare();
        if bytes.len() > spare.len() {
            return false;
        }
        spare[..bytes.len()].copy_from_slice(bytes);
        self.rx.filled(bytes.len());
        true
    }

    /// The same connection under a different backpressure policy (D10).
    #[must_use]
    pub const fn with_backpressure(mut self, policy: Backpressure) -> Self {
        self.policy = policy;
        self
    }

    /// How many bytes may wait, under this connection's policy.
    const fn bound(&self) -> usize {
        self.policy.bound(TX)
    }

    /// Tell the session the link is open, and queue whatever it says.
    ///
    /// An initiator answers with a Logon on its first `tick`; an acceptor says
    /// nothing until one arrives.
    pub fn opened(&mut self) {
        let bound = self.bound();
        let blocks = self.policy.blocks();
        let Self {
            session,
            transport,
            tx,
            tx_len,
            overflow,
            dead,
            ..
        } = self;
        let mut out = Out {
            transport,
            tx,
            tx_len,
            bound,
            blocks,
            overflow,
            failed: dead,
        };
        let link = session.connect(|b| out.push(b));
        self.closing |= link == Link::Dropped;
    }

    /// Offer the session a message the application originated.
    ///
    /// The session decides everything about it that matters — sequence number,
    /// `SendingTime`, whether the link is even up — so an application that
    /// hands over a stale or half-formed header cannot corrupt the stream.
    pub fn send_application(&mut self, msg: &[u8]) {
        let bound = self.bound();
        let blocks = self.policy.blocks();
        let Self {
            session,
            transport,
            journal,
            tx,
            tx_len,
            overflow,
            dead,
            ..
        } = self;
        let mut out = Out {
            transport,
            tx,
            tx_len,
            bound,
            blocks,
            overflow,
            failed: dead,
        };
        if session.send_application(msg, journal, |b| out.push(b)) == Link::Dropped {
            self.closing = true;
        }
        if self.overflow {
            self.slow_consumer();
        }
        // Push it now rather than next turn. `turn` has already flushed by the
        // time the engine collects out-of-band replies, so without this every
        // reply from another thread would wait a whole extra pass — the ring
        // costs a hop, and it should not also cost a turn.
        self.flush();
    }

    /// Is there anything waiting to go out?
    #[must_use]
    pub const fn has_pending_output(&self) -> bool {
        self.tx_len > 0
    }

    /// What a poller can wait on for this connection, if anything.
    ///
    /// Read from the live transport every time rather than cached: a
    /// [`crate::transport::Source`] borrows a descriptor it does not own, and a
    /// cached one outliving its socket would quietly name whichever socket the
    /// kernel handed that number to next.
    #[must_use]
    pub fn source(&self) -> Option<crate::transport::Source> {
        self.transport.source()
    }

    /// One pass: push what is queued, read what has arrived, judge it, and let
    /// the clock move.
    ///
    /// Nothing here blocks. Every call into the transport is non-blocking and
    /// every answer of [`Io::Idle`] is "nothing to do", which is what makes
    /// non-negotiable 4 hold for the thread that calls this.
    /// `refuse` is asked about each whole message **before** the session sees
    /// it, and answering `true` drops the connection without a reply.
    ///
    /// It exists for exactly one rule, and that rule is the engine's rather
    /// than the session's: a Logon arriving on a second connection while a
    /// first holds the identity. `1b_DuplicateIdentity.def` and
    /// `AlreadyLoggedOn.def` both expect **no reply at all** on the second, so
    /// the question has to be asked before the message is judged, not after.
    pub fn turn<A: Application, G: FnMut(&[u8]) -> bool>(
        &mut self,
        now_ms: u64,
        app: &mut A,
        mut refuse: G,
    ) -> Turn {
        let mut moved = self.flush();

        if !self.closing {
            let bound = self.bound();
            let blocks = self.policy.blocks();
            let Self {
                session,
                transport,
                tx,
                tx_len,
                overflow,
                dead,
                ..
            } = self;
            let mut out = Out {
                transport,
                tx,
                tx_len,
                bound,
                blocks,
                overflow,
                failed: dead,
            };
            if session.tick(now_ms, |b| out.push(b)) == Link::Dropped {
                self.closing = true;
            }
            if self.overflow {
                self.slow_consumer();
            }
        }

        // Read once per turn, not until the socket is empty: a counterparty
        // that can write faster than this end can process must not be able to
        // starve every other connection on the thread.
        let mut closed = false;
        match self.transport.recv(self.rx.spare()) {
            Io::Ready(n) => {
                self.rx.filled(n);
                moved = true;
            }
            Io::Idle => {}
            Io::Closed | Io::Failed(_) => closed = true,
        }

        // Everything that arrived, judged in order.
        loop {
            let taken = match self.rx.cut() {
                Cut::Need => break,
                // Rubbish still goes to the session once: it owns the rule
                // about when an unreadable frame is fatal.
                Cut::Message(n) | Cut::Garbage(n) => n,
            };
            moved = true;
            if refuse(self.rx.bytes(taken)) {
                self.rx.take(taken);
                let _ = self.session.disconnect(|_| {});
                return Turn::Gone;
            }
            let bound = self.bound();
            let blocks = self.policy.blocks();
            let Self {
                session,
                rx,
                transport,
                journal,
                tx,
                tx_len,
                overflow,
                dead,
                ..
            } = self;
            let mut out = Out {
                transport,
                tx,
                tx_len,
                bound,
                blocks,
                overflow,
                failed: dead,
            };
            let link = session.received_with(rx.bytes(taken), app, journal, |b| out.push(b));
            self.rx.take(taken);
            if link == Link::Dropped {
                self.closing = true;
                break;
            }
            if self.overflow {
                // Stop reading: every further message would answer into a
                // queue that is already full.
                self.slow_consumer();
                break;
            }
        }

        // **Not a correctness guard, and the reversal says so.** Removing this
        // leaves the wire gate at 59/59, because the next turn flushes anyway.
        // It is here to save that turn: a Heartbeat the clock just produced
        // goes out now rather than one pass later.
        moved |= self.flush();

        if closed {
            // The peer hung up. The session hears about it, and anything it
            // says in reply has nowhere to go — which is correct.
            let _ = self.session.disconnect(|_| {});
            return Turn::Gone;
        }
        if self.dead {
            // Nowhere left to write. Waiting for the queue to drain would be
            // waiting for ever — `[measured 2026-08-30]` found by a test that
            // killed the socket with bytes still queued and watched the
            // connection stay `Up` for as long as it was turned.
            let _ = self.session.disconnect(|_| {});
            return Turn::Gone;
        }
        if self.closing && self.tx_len == 0 {
            return Turn::Gone;
        }
        Turn::Up(moved)
    }

    /// End the session because the ring to the application filled.
    ///
    /// ADR-0011 decision 1. The counterparty did nothing wrong, so it is told
    /// why in the `58=` — see [`SLOW_APPLICATION`]. Everything queued is still
    /// sent: unlike the `slow_consumer` path above, the socket here is draining
    /// perfectly and there is no reason to throw away messages that will go
    /// out.
    pub fn slow_application(&mut self) {
        let bound = TX;
        let Self {
            session,
            transport,
            tx,
            tx_len,
            overflow,
            dead,
            ..
        } = self;
        let mut out = Out {
            transport,
            tx,
            tx_len,
            bound,
            blocks: false,
            overflow,
            failed: dead,
        };
        let _ = session.logout_now(SLOW_APPLICATION, |b| out.push(b));
        self.closing = true;
    }

    /// End the session because its outbound queue filled. `DESIGN.md` D10.
    ///
    /// **The queue is thrown away first, and that is deliberate.** It holds
    /// messages for a counterparty that has stopped reading, and the Logout
    /// that says so has to fit somewhere. Keeping them would mean the one
    /// message that matters is the one that cannot be sent.
    fn slow_consumer(&mut self) {
        self.tx_len = 0;
        self.overflow = false;
        // **`TX`, not the policy's bound.** `Queue { max_bytes }` bounds how
        // much traffic may wait; it does not bound the message that says the
        // waiting is over. A bound smaller than one Logout would otherwise end
        // the session in silence, which is the one thing D10 forbids.
        let bound = TX;
        let Self {
            session,
            transport,
            tx,
            tx_len,
            overflow,
            dead,
            ..
        } = self;
        let mut out = Out {
            transport,
            tx,
            tx_len,
            bound,
            // Never block on the way out: the socket is the thing that is not
            // draining.
            blocks: false,
            overflow,
            failed: dead,
        };
        let _ = session.logout_now(SLOW_CONSUMER, |b| out.push(b));
        self.overflow = false;
        self.closing = true;
    }

    /// Push as much of the queue as the socket will take.
    ///
    /// A short write is ordinary and is not an error: the rest stays queued and
    /// goes on the next turn.
    fn flush(&mut self) -> bool {
        if self.tx_len == 0 {
            return false;
        }
        match self.transport.send(&self.tx[..self.tx_len]) {
            Io::Ready(n) => {
                self.tx.copy_within(n..self.tx_len, 0);
                self.tx_len -= n;
                true
            }
            Io::Idle => false,
            Io::Closed | Io::Failed(_) => {
                self.closing = true;
                self.dead = true;
                false
            }
        }
    }
}

/// The outbound queue, as the session's `emit` closure sees it.
///
/// Holds borrows of four disjoint fields rather than copies, so it costs
/// nothing and allocates nothing. It exists because [`Backpressure::Block`]
/// has to reach the socket from inside `emit`, and a free function taking
/// `(&mut [u8], &mut usize)` cannot.
///
/// `[measured]` `TX` is 8 KiB against a longest corpus message of 200 bytes and
/// a longest burst of five, so nothing in the acceptance suite comes near the
/// bound. A real slow consumer does, which is why D10 exists.
struct Out<'a, T> {
    transport: &'a mut T,
    tx: &'a mut [u8],
    tx_len: &'a mut usize,
    bound: usize,
    blocks: bool,
    overflow: &'a mut bool,
    /// Set when the socket died while being spun on. It is the connection's
    /// `dead` flag, borrowed.
    failed: &'a mut bool,
}

impl<T: Transport> Out<'_, T> {
    /// One whole message, or none of it.
    ///
    /// **Never a partial write.** Half a FIX message on the wire is a frame the
    /// counterparty cannot recover from; a message that did not fit is a
    /// session that ends with a reason.
    fn push(&mut self, bytes: &[u8]) {
        if self.blocks {
            // D10's `Block`: spin on the socket until there is room. A spin is
            // not a kernel sleep, so D8 and non-negotiable 4 still hold — but
            // one slow counterparty now stops every other session on this
            // thread, which is why it is never a default.
            while *self.tx_len + bytes.len() > self.bound {
                match self.transport.send(&self.tx[..*self.tx_len]) {
                    Io::Ready(n) => {
                        self.tx.copy_within(n..*self.tx_len, 0);
                        *self.tx_len -= n;
                    }
                    Io::Idle => core::hint::spin_loop(),
                    Io::Closed | Io::Failed(_) => {
                        *self.failed = true;
                        return;
                    }
                }
            }
        }
        if *self.tx_len + bytes.len() > self.bound {
            *self.overflow = true;
            return;
        }
        self.tx[*self.tx_len..*self.tx_len + bytes.len()].copy_from_slice(bytes);
        *self.tx_len += bytes.len();
    }
}
