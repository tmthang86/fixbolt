//! Cutting a byte stream into messages.
//!
//! One connection, one [`Framer`], one fixed buffer. Nothing here allocates and
//! nothing here parses: the only field it reads is `9=`, and the only question
//! it answers is where one message ends.
//!
//! # The rule, and where it came from
//!
//! `2m_BodyLengthValueNotCorrect.def` states it in its own two comments:
//!
//! | `9=` says | Reality | What the file expects |
//! |---|---|---|
//! | 30 | the body is 91 | *"Invalid message was ignored, and valid one was processed"* |
//! | 111 | the body is 91 | *"it will combine with the next message and be ignored"* |
//!
//! One rule covers both: **take `9=` at its word.** Count to where it says the
//! body ends; if a `10=` trailer is not there, the whole buffer is rubbish —
//! which is why a length that is too long takes the message after it with it.
//!
//! # The rubbish still goes to the session — on every frame but the first
//!
//! Dropping it here would lose `1d_InvalidLogonLengthInvalid.def`, which wants
//! the link dropped because the unreadable frame *claims to be a Logon*. That
//! rule lives in `fixbolt_session`: the engine hands the bytes over once and
//! lets the session decide. [`Cut::Garbage`] is how it says so.
//!
//! **`[2026-09-01]` with one boundary, and it is written down rather than
//! discovered.** [`crate::presession`] sits in front of the session and owns a
//! socket until its first whole message. A **first** frame that can never be a
//! message has no readable identity, so there is no shard to route it to and no
//! session to hand it to — it is dropped there, which is what `1d` asks for and
//! is enforced one layer earlier than this comment used to promise. Everything
//! after a connection has logged on still reaches the session, unchanged.
//! [ADR-0022](../../../docs/decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md).

/// What the front of the buffer holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    /// A whole message occupies the first `n` bytes.
    Message(usize),
    /// The first `n` bytes are not a message and never will be. Hand them to
    /// the session once — it decides whether an unreadable frame is fatal —
    /// and then take them.
    Garbage(usize),
    /// Not enough bytes yet.
    Need,
}

/// One connection's receive buffer.
///
/// `N` is the largest message this connection will accept. Anything bigger is
/// [`Cut::Garbage`]: a buffer that fills and never empties is a connection
/// wedged by a number the counterparty chose.
pub struct Framer<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Default for Framer<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Framer<N> {
    /// An empty buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    /// Where the next `recv` should write.
    ///
    /// Empty when the buffer is full, which [`Self::cut`] then reports as
    /// [`Cut::Garbage`] rather than leaving the caller to spin.
    pub const fn spare(&mut self) -> &mut [u8] {
        self.buf.split_at_mut(self.len).1
    }

    /// Say how many bytes [`Self::spare`] was given.
    pub const fn filled(&mut self, n: usize) {
        self.len += n;
        if self.len > N {
            self.len = N;
        }
    }

    /// The first `n` bytes, for handing to the session.
    #[must_use]
    pub fn bytes(&self, n: usize) -> &[u8] {
        let n = n.min(self.len);
        &self.buf[..n]
    }

    /// Everything in the buffer, whether or not it is a whole message.
    ///
    /// [`Self::bytes`] answers *"the message you just cut"*; this answers
    /// *"everything that has arrived"*. The pre-session stage needs the second:
    /// a counterparty may pipeline behind its `Logon`, and those bytes belong to
    /// the session too — handing on only the message would lose them silently.
    #[must_use]
    pub fn all(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    /// The buffer and how much of it is used, for moving the whole thing.
    ///
    /// Used to carry bytes across a channel to a shard thread without a heap
    /// allocation per connection: the array moves, and nothing is copied that
    /// a `Vec` would not have copied anyway.
    #[must_use]
    pub const fn into_parts(self) -> ([u8; N], usize) {
        (self.buf, self.len)
    }

    /// Drop the first `n` bytes and shuffle the rest down.
    ///
    /// The shuffle is a `copy_within` of whatever is left, which after a whole
    /// message is normally nothing. A ring buffer would avoid it and would also
    /// hand the session a message in two pieces; the session's API takes one
    /// slice, and `[measured]` no message in the corpus exceeds 200 bytes.
    pub fn take(&mut self, n: usize) {
        let n = if n > self.len { self.len } else { n };
        self.buf.copy_within(n..self.len, 0);
        self.len -= n;
    }

    /// What the front of the buffer holds.
    #[must_use]
    pub fn cut(&self) -> Cut {
        if self.len == 0 {
            return Cut::Need;
        }
        let rx = &self.buf[..self.len];

        // `\x019=` rather than `9=`: the length field is the second one, and a
        // body can hold a `9=` of its own inside a value.
        let Some(at) = rx.windows(3).position(|w| w == b"\x019=") else {
            // No length field yet. Either it has not arrived, or these bytes
            // are not a FIX message at all — and a full buffer settles which.
            return if self.len == N {
                Cut::Garbage(self.len)
            } else {
                Cut::Need
            };
        };
        let digits = &rx[at + 3..];
        let Some(end) = digits.iter().position(|b| *b == 1) else {
            return if self.len == N {
                Cut::Garbage(self.len)
            } else {
                Cut::Need
            };
        };
        let Some(body_len) = number(&digits[..end]) else {
            return Cut::Garbage(self.len);
        };
        let stop = at + 3 + end + 1 + body_len;

        // `10=` plus at least one digit plus SOH. The width is not fixed: 238
        // of the corpus's own lines carry the literal `10=0`.
        if self.len < stop + 4 {
            // It does not fit and never will.
            return if stop + 4 > N {
                Cut::Garbage(self.len)
            } else {
                Cut::Need
            };
        }
        if rx.get(stop..stop + 3) != Some(b"10=") {
            return Cut::Garbage(self.len);
        }
        match rx[stop + 3..].iter().position(|b| *b == 1) {
            Some(k) => Cut::Message(stop + 3 + k + 1),
            None if self.len == N => Cut::Garbage(self.len),
            None => Cut::Need,
        }
    }
}

/// ASCII digits to a length. `None` for anything else — a `9=` that is not a
/// number is a frame that cannot be read.
fn number(text: &[u8]) -> Option<usize> {
    if text.is_empty() || text.len() > 9 {
        return None;
    }
    let mut n = 0usize;
    for c in text {
        let d = c.checked_sub(b'0')?;
        if d > 9 {
            return None;
        }
        n = n * 10 + usize::from(d);
    }
    Some(n)
}
