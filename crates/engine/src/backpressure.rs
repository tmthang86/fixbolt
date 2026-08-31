//! What happens when the counterparty stops reading.
//!
//! `DESIGN.md` D10. A slow counterparty fills the socket's send buffer and
//! `send` answers `EAGAIN`; at 50 000 `ExecutionReport`/s against a QA
//! application this **will** happen. The engine must not block the session
//! machine and must not drop protocol messages in silence.
//!
//! # The queue is its own storage
//!
//! It is tempting to say the queued bytes are the ones the journal (D7)
//! already holds. They are not: under `JournalPolicy::None` the journal holds
//! nothing, and that is the policy a simulator runs. The queue is the
//! connection's own `TX` buffer, sized at startup and never grown.

/// What a connection does when its outbound queue is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backpressure {
    /// Queue into the whole of the connection's `TX` buffer; when that will
    /// not take the next message, end the session with
    /// `Logout(58=slow consumer)`.
    ///
    /// **The default.** A FIX counterparty that cannot keep up is a broken
    /// counterparty, and pretending otherwise turns a visible failure into a
    /// silent one.
    #[default]
    Disconnect,
    /// The same, with a bound tighter than `TX`.
    ///
    /// A bound larger than `TX` is not an error and is not honoured: `TX` is
    /// the buffer that exists.
    Queue {
        /// How many bytes may wait.
        max_bytes: usize,
    },
    /// Spin on the socket until it takes the bytes.
    ///
    /// **Never a default and never a deployment.** It is here because a test
    /// that wants to prove the queue's contents rather than its policy needs a
    /// policy that loses nothing. On a real engine it hands one slow
    /// counterparty the power to stop every other session on the thread.
    Block,
}

impl Backpressure {
    /// How many bytes may wait, given a `TX` buffer of `tx`.
    #[must_use]
    pub const fn bound(self, tx: usize) -> usize {
        match self {
            Self::Disconnect | Self::Block => tx,
            Self::Queue { max_bytes } => {
                if max_bytes < tx {
                    max_bytes
                } else {
                    tx
                }
            }
        }
    }

    /// Whether a full queue should be waited out rather than ended.
    #[must_use]
    pub const fn blocks(self) -> bool {
        matches!(self, Self::Block)
    }
}

/// The `58=` a session ends with when its queue filled. D10 names this text.
pub const SLOW_CONSUMER: &[u8] = b"slow consumer";

/// The `58=` a session ends with when the ring to the application filled.
///
/// **Deliberately not [`SLOW_CONSUMER`]**, though both end the session the same
/// way. That one means *the counterparty stopped reading the socket*; this one
/// means *our own application stopped reading the ring*, and the counterparty
/// is behaving perfectly. Sending the same text for both would tell the other
/// end their side is at fault when it is not, and an operator reading two logs
/// could not tell the two failures apart.
///
/// [ADR-0011](../../../docs/decisions/ADR-0011-a-full-ring-disconnects.md)
/// decision 2 — the refusal is never silent — and this is the half of it the
/// counterparty can see.
pub const SLOW_APPLICATION: &[u8] = b"slow application";
