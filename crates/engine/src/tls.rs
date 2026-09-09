//! TLS as a second [`Transport`] implementation, and a name for which mode is
//! actually carrying the bytes.
//!
//! [ADR-0005] decided the shape: the handshake runs in userspace through
//! `rustls`, the steady state on Linux is **kTLS** — the kernel holds the keys
//! and `read(2)`/`write(2)` carry plaintext, so [`crate::transport::Transport`]
//! is unchanged and D8's spin loop does not learn a new trick — and anywhere
//! else it is userspace `rustls`, which **leaves the hot-path guarantee** and
//! says so.
//!
//! [ADR-0018] answered the question that decided whether any of this was
//! possible: `ktls-core` can be driven from a plain non-blocking socket with no
//! async runtime, under four conditions.
//! `scripts/check-ktls-on-a-plain-socket.sh` re-proves them on the kernel of the
//! day — `[measured 2026-09-09]` 15 pass / 0 fail on `7.0.0-31-generic`.
//!
//! # The handshake lives in here, not in a pre-session stage
//!
//! `[decided 2026-09-09, this plan's Sửa 1]` The plan's draft added a
//! `Handshaking` stage to [`crate::presession::PendingSet`]. There is nowhere
//! for it to stand: `PendingSet<T, R, PRE>` holds one transport **type** from
//! `admit` to `take`, so a stage that changes the socket's type cannot be
//! expressed. The shape that does work is simpler and needs no change to
//! `presession.rs` at all — the handshake runs **inside** `recv`/`send`, which
//! report [`Io::Idle`] until it completes, and `PendingSet::turn` already
//! drives a transport that is not ready yet and already carries the
//! `Limits::logon_ms` deadline.
//!
//! [ADR-0005]: ../../../docs/decisions/ADR-0005-tls.md
//! [ADR-0018]: ../../../docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md
//! [`Io::Idle`]: crate::transport::Io

/// Which of [ADR-0005]'s three answers is carrying this connection's bytes.
///
/// **This type exists because a session that silently falls back to userspace
/// publishes a latency number that is about a different code path** — ADR-0005
/// open question 3. It is reported rather than inferred: `w2w` prints it beside
/// every figure, and a `hft` deployment can refuse anything but
/// [`TlsMode::Kernel`].
///
/// [ADR-0005]: ../../../docs/decisions/ADR-0005-tls.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TlsMode {
    /// No TLS at all — a plain `TcpTransport`. The default, and what every
    /// number published before this module existed was measured on.
    Plain,
    /// The keys are in the kernel: `read(2)` and `write(2)` carry plaintext and
    /// the engine thread does exactly what it does without TLS.
    Kernel,
    /// `rustls` in userspace, on the data path. **This leaves the hot-path
    /// guarantee**: it copies once per direction and allocates. ADR-0005
    /// decision 3 requires it be named rather than discovered.
    Userspace,
}

impl TlsMode {
    /// Whether this mode keeps the no-allocation, no-copy guarantee the `hft`
    /// numbers are measured under.
    ///
    /// [`TlsMode::Userspace`] is the only one that does not, and
    /// `TlsRequireKernel=Y` is how a deployment refuses it at startup rather
    /// than discovering it in a latency histogram.
    #[must_use]
    pub const fn keeps_the_hot_path(self) -> bool {
        matches!(self, Self::Plain | Self::Kernel)
    }
}

#[cfg(target_os = "linux")]
mod handshake {
    //! A TLS handshake driven **without blocking**, from inside `recv`/`send`.
    //!
    //! The spike (`spikes/ktls`) drives the same rustls state machine with a
    //! `spin_write_all` / `spin_read` pair, which is correct for a program whose
    //! only job is one connection. It is wrong here: this runs on the acceptor
    //! thread inside `PendingSet::turn`, which sweeps every waiting socket, and
    //! a spin on one of them stalls all the others behind it for a whole
    //! handshake.
    //!
    //! So the loop is turned inside out. Each call pumps the state machine as
    //! far as the socket allows and then returns, keeping every buffer and
    //! cursor, and reports [`Step::Pending`]. `PendingSet::turn` calls again on
    //! the next sweep. Nothing here waits.

    use std::io;

    use rustls::server::UnbufferedServerConnection;
    use rustls::unbuffered::{ConnectionState, UnbufferedStatus};

    use crate::transport::{Io, TcpTransport, Transport};

    /// 32 KiB each way, taken once when the socket is admitted.
    ///
    /// A handshake record can reach 16 KiB and a flight can hold several, so
    /// this is two records of headroom rather than a guess. **These are the
    /// allocation ADR-0005 decision 1 carves out**, and the carve-out is
    /// bounded to exactly here: they are dropped at the handover and nothing
    /// after it allocates.
    const HANDSHAKE_BUF: usize = 32 * 1024;

    /// How far the handshake got on this call.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Step {
        /// Not finished, and the socket has nothing more to give right now.
        /// Call again on the next sweep.
        Pending,
        /// The handshake is complete and no TLS data is left queued, which is
        /// what `dangerous_into_kernel_connection` requires.
        Done,
        /// It will not finish. The connection is over.
        Failed(io::ErrorKind),
    }

    /// The acceptor side of a handshake in progress.
    pub struct Handshake {
        conn: UnbufferedServerConnection,
        incoming: Vec<u8>,
        used: usize,
        outgoing: Vec<u8>,
        out_used: usize,
        out_sent: usize,
        early: Vec<u8>,
    }

    impl Handshake {
        /// Begin, with buffers taken now rather than during the handshake.
        pub fn new(conn: UnbufferedServerConnection) -> Self {
            Self {
                conn,
                incoming: vec![0u8; HANDSHAKE_BUF],
                used: 0,
                outgoing: vec![0u8; HANDSHAKE_BUF],
                out_used: 0,
                out_sent: 0,
                early: Vec::new(),
            }
        }

        /// Application plaintext that arrived **before** the handover.
        ///
        /// `[measured 2026-09-09]` this is not a corner case, it is the normal
        /// case, and the plan named it as a trap before the code was written: a
        /// counterparty that sends its `Logon` immediately after `Finished`
        /// puts application records on the wire while this end is still
        /// finishing. rustls reports `ReadTraffic` rather than `WriteTraffic`,
        /// and the very first run of
        /// `tests/tls.rs::a_handshake_completes_without_the_acceptor_ever_blocking`
        /// hit it.
        ///
        /// **These bytes must be given to the session before a single byte is
        /// read from the kernel**, or the first message of the connection is
        /// lost — and lost silently, since a FIX session with no `Logon` simply
        /// times out.
        ///
        /// Letting rustls decrypt them is what keeps the handover sound.
        /// ADR-0018's third condition is about **ciphertext** left in this
        /// engine's buffer, which the kernel would then re-count — see
        /// [`Handshake::leftover`]. Records rustls has properly consumed have
        /// advanced its own sequence number, and that is the sequence
        /// `dangerous_into_kernel_connection` hands down. The spike's
        /// `hand-draining-desyncs-the-kernel` check fails the *other* thing:
        /// bytes taken off the socket behind rustls's back.
        pub fn take_early_data(&mut self) -> Vec<u8> {
            core::mem::take(&mut self.early)
        }

        /// How many ciphertext bytes are sitting in this buffer unprocessed.
        ///
        /// **Must be zero at the handover.** Bytes here are ciphertext the
        /// kernel will never see, and its receive sequence number has already
        /// counted them — ADR-0018's third condition, and the spike's
        /// `hand-draining-desyncs-the-kernel` check is what happens when it is
        /// not: the next record fails with `EBADMSG`.
        pub const fn leftover(&self) -> usize {
            self.used
        }

        /// Give up the connection once [`Step::Done`] has been reported.
        pub fn into_connection(self) -> UnbufferedServerConnection {
            self.conn
        }

        /// Push whatever is queued towards the socket. Partial writes are the
        /// normal case on a non-blocking socket and are simply remembered.
        fn flush(&mut self, sock: &mut TcpTransport) -> Result<bool, io::ErrorKind> {
            while self.out_sent < self.out_used {
                let Some(pending) = self.outgoing.get(self.out_sent..self.out_used) else {
                    return Err(io::ErrorKind::InvalidData);
                };
                match sock.send(pending) {
                    Io::Ready(n) => self.out_sent += n,
                    Io::Idle => return Ok(false),
                    Io::Closed => return Err(io::ErrorKind::UnexpectedEof),
                    Io::Failed(k) => return Err(k),
                }
            }
            self.out_used = 0;
            self.out_sent = 0;
            Ok(true)
        }

        /// Drive the handshake as far as this socket allows, then return.
        ///
        /// Never blocks and never spins: every path out of the loop is either a
        /// state change or a socket that said `Idle`.
        pub fn pump(&mut self, sock: &mut TcpTransport) -> Step {
            loop {
                // Anything still queued goes first. Until it is gone the peer
                // cannot make progress, so there is no point reading.
                match self.flush(sock) {
                    Ok(true) => {}
                    Ok(false) => return Step::Pending,
                    Err(k) => return Step::Failed(k),
                }

                let Some(input) = self.incoming.get_mut(..self.used) else {
                    return Step::Failed(io::ErrorKind::InvalidData);
                };
                let UnbufferedStatus { discard, state } = self.conn.process_tls_records(input);

                let mut want_read = false;
                let mut done = false;

                match state {
                    Ok(ConnectionState::EncodeTlsData(mut s)) => {
                        let Some(room) = self.outgoing.get_mut(self.out_used..) else {
                            return Step::Failed(io::ErrorKind::InvalidData);
                        };
                        match s.encode(room) {
                            Ok(n) => self.out_used += n,
                            // A flight that does not fit in 32 KiB is not a
                            // handshake this engine is going to complete.
                            Err(_) => return Step::Failed(io::ErrorKind::InvalidData),
                        }
                    }
                    Ok(ConnectionState::TransmitTlsData(s)) => s.done(),
                    Ok(ConnectionState::BlockedHandshake) => want_read = true,
                    Ok(ConnectionState::WriteTraffic(_)) => done = true,
                    // The handshake is over and the peer has already sent
                    // application data. Let rustls decrypt it — see
                    // `take_early_data` for why that is the sound choice rather
                    // than the convenient one — and keep going until the
                    // ciphertext buffer is empty.
                    Ok(ConnectionState::ReadTraffic(mut r)) => {
                        while let Some(record) = r.next_record() {
                            match record {
                                Ok(rec) => self.early.extend_from_slice(rec.payload),
                                Err(_) => return Step::Failed(io::ErrorKind::InvalidData),
                            }
                        }
                    }
                    Ok(_) | Err(_) => return Step::Failed(io::ErrorKind::InvalidData),
                }

                if discard > 0 {
                    if discard > self.used {
                        return Step::Failed(io::ErrorKind::InvalidData);
                    }
                    self.incoming.copy_within(discard..self.used, 0);
                    self.used -= discard;
                }

                // **`discard == 0` is the second half of the condition and not
                // a tidy-up.** A TLS 1.3 server emits session tickets after it
                // reaches `WriteTraffic`, and
                // `dangerous_into_kernel_connection` refuses a connection with
                // TLS data still queued. Leaving on the first `WriteTraffic`
                // hands the kernel a connection that still owes bytes.
                if done && discard == 0 && self.out_used == 0 {
                    return Step::Done;
                }

                if want_read {
                    let Some(room) = self.incoming.get_mut(self.used..) else {
                        return Step::Failed(io::ErrorKind::InvalidData);
                    };
                    if room.is_empty() {
                        return Step::Failed(io::ErrorKind::InvalidData);
                    }
                    match sock.recv(room) {
                        Io::Ready(n) => self.used += n,
                        Io::Idle => return Step::Pending,
                        Io::Closed => return Step::Failed(io::ErrorKind::UnexpectedEof),
                        Io::Failed(k) => return Step::Failed(k),
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub use handshake::{Handshake, Step};
