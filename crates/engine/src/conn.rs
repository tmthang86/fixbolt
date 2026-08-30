//! One connection: a socket, a receive buffer, a state machine, and whatever
//! could not be written yet.

use nanofix_session::{Application, Link, Role, Session};

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
pub struct Connection<T, R: Role, const N: usize, const RX: usize, const TX: usize> {
    /// Which connection this is, for routing a reply that comes back from
    /// another thread. Never reused: the engine only ever counts up.
    pub id: ConnId,
    /// The socket. `None` once it has been given up.
    pub transport: T,
    pub session: Session<R, N>,
    rx: Framer<RX>,
    /// Written by the session, not yet accepted by the socket.
    tx: [u8; TX],
    tx_len: usize,
    /// Set when the session says the link is down, so the engine can drop it
    /// after the last bytes have been pushed out.
    closing: bool,
}

impl<T: Transport, R: Role, const N: usize, const RX: usize, const TX: usize>
    Connection<T, R, N, RX, TX>
{
    /// Wrap a socket and a session that has not been told about it yet.
    pub const fn new(id: ConnId, transport: T, session: Session<R, N>) -> Self {
        Self {
            id,
            transport,
            session,
            rx: Framer::new(),
            tx: [0; TX],
            tx_len: 0,
            closing: false,
        }
    }

    /// Tell the session the link is open, and queue whatever it says.
    ///
    /// An initiator answers with a Logon on its first `tick`; an acceptor says
    /// nothing until one arrives.
    pub fn opened(&mut self) {
        let tx = &mut self.tx;
        let tx_len = &mut self.tx_len;
        let link = self.session.connect(|b| push(tx, tx_len, b));
        self.closing |= link == Link::Dropped;
    }

    /// Offer the session a message the application originated.
    ///
    /// The session decides everything about it that matters — sequence number,
    /// `SendingTime`, whether the link is even up — so an application that
    /// hands over a stale or half-formed header cannot corrupt the stream.
    pub fn send_application(&mut self, msg: &[u8]) {
        let Self {
            session,
            tx,
            tx_len,
            ..
        } = self;
        if session.send_application(msg, |b| push(tx, tx_len, b)) == Link::Dropped {
            self.closing = true;
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
            let Self {
                session,
                rx,
                tx,
                tx_len,
                ..
            } = self;
            let link = session.received_with(rx.bytes(taken), app, |b| push(tx, tx_len, b));
            self.rx.take(taken);
            if link == Link::Dropped {
                self.closing = true;
                break;
            }
        }

        if !self.closing {
            let Self {
                session,
                tx,
                tx_len,
                ..
            } = self;
            if session.tick(now_ms, |b| push(tx, tx_len, b)) == Link::Dropped {
                self.closing = true;
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
        if self.closing && self.tx_len == 0 {
            return Turn::Gone;
        }
        Turn::Up(moved)
    }

    /// Push as much of the queue as the socket will take.
    ///
    /// A short write is ordinary and is not an error: the rest stays queued and
    /// goes on the next turn. What to do when the queue itself fills is
    /// `DESIGN.md` D10's question and step 5's work; until then the queue
    /// simply drops what does not fit, and says so by refusing to grow.
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
                false
            }
        }
    }
}

/// Append to the outbound queue, dropping what does not fit.
///
/// `[measured]` `TX` is 8 KiB against a longest corpus message of 200 bytes and
/// a longest burst of five, so nothing in the suite comes near it. A real slow
/// consumer does, and that is D10's policy rather than this function's.
fn push(tx: &mut [u8], tx_len: &mut usize, bytes: &[u8]) {
    let room = tx.len() - *tx_len;
    let n = room.min(bytes.len());
    tx[*tx_len..*tx_len + n].copy_from_slice(&bytes[..n]);
    *tx_len += n;
}
