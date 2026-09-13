//! TLS as a second [`crate::transport::Transport`] implementation, and a name for which mode is
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
/// `[2026-09-10]` **re-exported; it lives in [`crate::transport`] now.**
/// [ADR-0060] decision 3 puts `Transport::tls_mode` on the core trait with a
/// default so the engine can ask any transport without a downcast — and a type
/// named in that trait cannot sit behind the `tls` feature, because the trait
/// does not. The move is that decision's cost and ADR-0060's Consequences say
/// so.
///
/// [ADR-0005]: ../../../docs/decisions/ADR-0005-tls.md
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
pub use crate::transport::TlsMode;

#[cfg(target_os = "linux")]
mod side {
    //! Which end of the handshake a [`super::TlsTransport`] is.
    //!
    //! `[2026-09-13]` **step 5a of the `tls` plan, Sửa 6 item 6.4.1.** Everything
    //! in this file was written for the acceptor, and an initiator needs the
    //! same handshake driver and the same kernel handover from the other end.
    //! The plan decided one struct generic over the side rather than a second
    //! client struct, because the handover is what ADR-0018's four conditions
    //! paid for and two copies of it would drift.
    //!
    //! **It is not quite the pure type substitution the plan's 6.1 read it as**,
    //! and the difference is why this trait carries two functions rather than
    //! only associated types. `rustls` 0.23.44 defines `process_tls_records` in
    //! two *inherent* impls, one on `UnbufferedConnectionCommon<ClientConnectionData>`
    //! and one on `…<ServerConnectionData>` (`conn/unbuffered.rs:15-39`), not one
    //! generic impl, and `dangerous_into_kernel_connection` likewise lives on
    //! each connection type separately. Code generic over the side cannot name
    //! either, so each side names them once here.

    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use rustls::client::{ClientConnectionData, UnbufferedClientConnection};
    use rustls::kernel::KernelConnection;
    use rustls::server::{ServerConnectionData, UnbufferedServerConnection};
    use rustls::unbuffered::UnbufferedStatus;

    mod sealed {
        pub trait Sealed {}
    }

    /// The acceptor end: a socket that was accepted. **The default** for every
    /// type generic over [`Side`], so a spelling that predates the client —
    /// `TlsTransport`, `Handshake`, `Traffic` — still means this.
    #[derive(Debug)]
    pub enum Server {}

    /// The initiator end: a socket that was dialled.
    #[derive(Debug)]
    pub enum Client {}

    /// Which end of the handshake. **Sealed**: there are exactly two, and the
    /// handshake driver's correctness argument is made for both of them.
    pub trait Side: sealed::Sealed + Sized + 'static {
        /// The `rustls` unbuffered connection for this end.
        type Connection;
        /// The `rustls` per-side connection data.
        type Data;
        /// The session ktls-core drives after the handover. For [`Server`] it
        /// is what `dangerous_into_kernel_connection` hands back,
        /// `KernelConnection<ServerConnectionData>`; for [`Client`] it is that
        /// value wrapped in `Ticketless`, which drops session tickets unread
        /// ([ADR-0063] decision 1). Named as its own associated type because
        /// `ktls_core::Context` bounds its parameter on `TlsSession`, and only
        /// a bound written *here* is implied wherever `Self` is.
        ///
        /// [ADR-0063]: ../../../docs/decisions/ADR-0063-a-peers-key-update-is-the-second-named-carve-out-and-a-ticket-is-not-read.md
        type Kernel: ktls_core::TlsSession;

        #[doc(hidden)]
        fn process_tls_records<'c, 'i>(
            conn: &'c mut Self::Connection,
            incoming: &'i mut [u8],
        ) -> UnbufferedStatus<'c, 'i, Self::Data>;

        #[doc(hidden)]
        fn into_kernel(
            conn: Self::Connection,
        ) -> Result<(rustls::ExtractedSecrets, Self::Kernel), rustls::Error>;

        /// The counter of session tickets this end's kernel session set aside,
        /// shared so the transport can read it after ktls-core has taken the
        /// session. `None` for an end that never receives a ticket.
        #[doc(hidden)]
        fn tickets(kernel: &Self::Kernel) -> Option<Arc<AtomicU32>>;
    }

    /// A `rustls` unbuffered connection that knows which [`Side`] it is.
    ///
    /// **Exists so that `Handshake::new(conn)` still infers its side**, and that
    /// is the constraint the plan's default type parameter alone does not meet.
    /// A default applies in a *type* position; in an *expression* such as
    /// `Handshake::new(conn)` the parameter is inferred, and rustc cannot infer
    /// `S` backwards from `S::Connection == UnbufferedServerConnection`. Asking
    /// the connection type for its side runs the projection forwards, which it
    /// can. Sealed, like [`Side`].
    pub trait SideConnection: sealed::Sealed + Sized {
        /// The end this connection is.
        type Side: Side;

        #[doc(hidden)]
        fn into_side(self) -> <Self::Side as Side>::Connection;
    }

    impl sealed::Sealed for Server {}
    impl sealed::Sealed for Client {}
    impl sealed::Sealed for UnbufferedServerConnection {}
    impl sealed::Sealed for UnbufferedClientConnection {}

    impl Side for Server {
        type Connection = UnbufferedServerConnection;
        type Data = ServerConnectionData;
        type Kernel = KernelConnection<ServerConnectionData>;

        fn process_tls_records<'c, 'i>(
            conn: &'c mut Self::Connection,
            incoming: &'i mut [u8],
        ) -> UnbufferedStatus<'c, 'i, Self::Data> {
            conn.process_tls_records(incoming)
        }

        fn into_kernel(
            conn: Self::Connection,
        ) -> Result<(rustls::ExtractedSecrets, Self::Kernel), rustls::Error> {
            conn.dangerous_into_kernel_connection()
        }

        fn tickets(_kernel: &Self::Kernel) -> Option<Arc<AtomicU32>> {
            None
        }
    }

    impl Side for Client {
        type Connection = UnbufferedClientConnection;
        type Data = ClientConnectionData;
        type Kernel = Ticketless;

        fn process_tls_records<'c, 'i>(
            conn: &'c mut Self::Connection,
            incoming: &'i mut [u8],
        ) -> UnbufferedStatus<'c, 'i, Self::Data> {
            conn.process_tls_records(incoming)
        }

        fn into_kernel(
            conn: Self::Connection,
        ) -> Result<(rustls::ExtractedSecrets, Self::Kernel), rustls::Error> {
            let (secrets, inner) = conn.dangerous_into_kernel_connection()?;
            Ok((
                secrets,
                Ticketless {
                    inner,
                    // One allocation, at the handover, inside ADR-0005
                    // decision 1's handshake carve-out — beside the `Box` of
                    // the `Context` and the control-record buffer taken there.
                    ignored: Arc::new(AtomicU32::new(0)),
                },
            ))
        }

        fn tickets(kernel: &Self::Kernel) -> Option<Arc<AtomicU32>> {
            Some(Arc::clone(&kernel.ignored))
        }
    }

    /// The kernel-side session of a dialled connection: rustls's
    /// `KernelConnection<ClientConnectionData>`, except that a TLS 1.3
    /// `NewSessionTicket` is **counted and dropped unread**.
    ///
    /// `[2026-09-13]` **step 6c-2 of the `tls` plan, Sửa 7; [ADR-0063]
    /// decision 1.** A server sends its session tickets after the handshake, so
    /// a client receives them after the handover, on the engine thread — which
    /// for an initiator is also the dialling thread. rustls's own
    /// `handle_new_session_ticket` parses the ticket, derives its PSK and
    /// clones the peer's certificate chain before the store drops or keeps the
    /// result: `[measured 2026-09-13]` 16 allocations for a rustls server's two
    /// tickets with resumption enabled (step 6c), and **the same 16 with
    /// `Resumption::disabled()`** (step 6c-2, the ticket handed back to rustls
    /// as a reversal) — disabling resumption alone removes none of them.
    /// Nothing here would ever use a ticket, because this engine does not
    /// resume TLS sessions, so the payload is not read at all.
    ///
    /// **The consequence is a rule: the initiator never resumes a TLS session,
    /// and every dial is a full handshake.** `tls::client_config` sets
    /// `Resumption::disabled()` so the userspace fallback — where rustls reads
    /// tickets itself — agrees.
    ///
    /// The other four methods forward to ktls-core's own implementation for
    /// `KernelConnection`, unchanged; a peer's KeyUpdate still allocates the
    /// four key-schedule boxes ADR-0063 decision 2 names.
    ///
    /// Proven by `tests/tls_key_update.rs`:
    /// `a_session_ticket_after_the_handover_allocates_nothing` (zero
    /// allocations over the ticket window, and the counter moved by 2); the
    /// rule by `a_redial_does_a_full_handshake_not_a_resumption`.
    ///
    /// [ADR-0063]: ../../../docs/decisions/ADR-0063-a-peers-key-update-is-the-second-named-carve-out-and-a-ticket-is-not-read.md
    pub struct Ticketless {
        inner: KernelConnection<ClientConnectionData>,
        /// Tickets set aside. Shared with the transport, because ktls-core's
        /// `Context` owns this value and offers no way back to it.
        ignored: Arc<AtomicU32>,
    }

    impl ktls_core::TlsSession for Ticketless {
        fn peer(&self) -> ktls_core::Peer {
            ktls_core::TlsSession::peer(&self.inner)
        }

        fn protocol_version(&self) -> ktls_core::ProtocolVersion {
            ktls_core::TlsSession::protocol_version(&self.inner)
        }

        fn update_tx_secret(&mut self) -> ktls_core::error::Result<ktls_core::TlsCryptoInfoTx> {
            ktls_core::TlsSession::update_tx_secret(&mut self.inner)
        }

        fn update_rx_secret(&mut self) -> ktls_core::error::Result<ktls_core::TlsCryptoInfoRx> {
            ktls_core::TlsSession::update_rx_secret(&mut self.inner)
        }

        /// Counted, not read. ktls-core has already checked that this end is
        /// the client and the version is TLS 1.3 (`context.rs:493-513`), and
        /// the record sits in the buffer taken at the handover.
        fn handle_new_session_ticket(&mut self, _payload: &[u8]) -> ktls_core::error::Result<()> {
            self.ignored.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    impl SideConnection for UnbufferedServerConnection {
        type Side = Server;

        fn into_side(self) -> UnbufferedServerConnection {
            self
        }
    }

    impl SideConnection for UnbufferedClientConnection {
        type Side = Client;

        fn into_side(self) -> UnbufferedClientConnection {
            self
        }
    }
}

#[cfg(target_os = "linux")]
pub use side::{Client, Server, Side, SideConnection};

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
    //!
    //! `[2026-09-13]` **Both ends drive the same loop.** A client's first pump
    //! encodes its `ClientHello`, flushes it, and meets an empty socket exactly
    //! as an acceptor meets one before the `ClientHello` arrives; nothing below
    //! branches on [`super::Side`]. `tests/tls_client.rs` is the client's gate.

    use std::io;

    use rustls::unbuffered::{ConnectionState, UnbufferedStatus};

    use super::{Server, Side, SideConnection};
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

    /// A handshake in progress, on either [`Side`].
    pub struct Handshake<S: Side = Server> {
        conn: S::Connection,
        incoming: Vec<u8>,
        used: usize,
        outgoing: Vec<u8>,
        out_used: usize,
        out_sent: usize,
        early: Vec<u8>,
    }

    impl<S: Side> Handshake<S> {
        /// Begin, with buffers taken now rather than during the handshake.
        ///
        /// The side is the connection's: an `UnbufferedServerConnection`
        /// makes a `Handshake<Server>`, an `UnbufferedClientConnection` a
        /// `Handshake<Client>` — see [`SideConnection`] for why it is asked
        /// that way round.
        pub fn new<C: SideConnection<Side = S>>(conn: C) -> Self {
            Self {
                conn: conn.into_side(),
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
        pub fn into_connection(self) -> S::Connection {
            self.conn
        }

        /// Carry on in userspace, keeping the buffers rather than taking new
        /// ones.
        ///
        /// The fallback of ADR-0005 decision 3. Reached only when the kernel
        /// refused the offload, and **the buffers are the point**: this mode
        /// copies once per direction for the life of the session, so allocating
        /// per message on top of that would be a second cost on a path that has
        /// already left the guarantee.
        pub fn into_traffic(self) -> Traffic<S> {
            Traffic {
                conn: self.conn,
                incoming: self.incoming,
                used: self.used,
                outgoing: self.outgoing,
                out_used: self.out_used,
                out_sent: self.out_sent,
                plain: self.early,
                plain_at: 0,
            }
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
                let UnbufferedStatus { discard, state } =
                    S::process_tls_records(&mut self.conn, input);

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

    /// A TLS connection carrying application data **in userspace**.
    ///
    /// **This is the mode that leaves the hot-path guarantee**, and it exists so
    /// that a kernel without kTLS is a slower session rather than no session.
    /// Every byte is copied once on the way in and once on the way out;
    /// `benches/alloc.rs` does not cover this path and ADR-0005 decision 3 says
    /// it must be named rather than discovered.
    pub struct Traffic<S: Side = Server> {
        conn: S::Connection,
        incoming: Vec<u8>,
        used: usize,
        outgoing: Vec<u8>,
        out_used: usize,
        out_sent: usize,
        plain: Vec<u8>,
        plain_at: usize,
    }

    impl<S: Side> Traffic<S> {
        /// Take the bytes rustls decrypted during the handshake.
        ///
        /// They must be delivered before anything read afterwards: a `Logon`
        /// out of order is a sequence gap the counterparty did not cause.
        pub fn adopt_early(&mut self, early: Vec<u8>) {
            if self.plain.is_empty() {
                self.plain = early;
                self.plain_at = 0;
            } else {
                self.plain.extend_from_slice(&early);
            }
        }

        /// Move whatever is queued towards the socket, tolerating short writes.
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

        /// Hand the session decrypted bytes, reading and decrypting only when
        /// what is already decrypted has run out.
        pub fn recv(&mut self, sock: &mut TcpTransport, buf: &mut [u8]) -> Io {
            loop {
                if let Some(rest) = self.plain.get(self.plain_at..)
                    && !rest.is_empty()
                {
                    let n = rest.len().min(buf.len());
                    let (Some(src), Some(dst)) = (rest.get(..n), buf.get_mut(..n)) else {
                        return Io::Failed(io::ErrorKind::InvalidData);
                    };
                    dst.copy_from_slice(src);
                    self.plain_at += n;
                    return Io::Ready(n);
                }
                self.plain.clear();
                self.plain_at = 0;

                // Decrypt whatever ciphertext is already in hand before asking
                // the socket for more.
                let Some(input) = self.incoming.get_mut(..self.used) else {
                    return Io::Failed(io::ErrorKind::InvalidData);
                };
                let UnbufferedStatus { discard, state } =
                    S::process_tls_records(&mut self.conn, input);
                let mut got = false;
                match state {
                    Ok(ConnectionState::ReadTraffic(mut r)) => {
                        while let Some(record) = r.next_record() {
                            match record {
                                Ok(rec) => {
                                    self.plain.extend_from_slice(rec.payload);
                                    got = true;
                                }
                                Err(_) => return Io::Failed(io::ErrorKind::InvalidData),
                            }
                        }
                    }
                    Ok(ConnectionState::WriteTraffic(_) | ConnectionState::BlockedHandshake) => {}
                    Ok(ConnectionState::Closed) => return Io::Closed,
                    Ok(_) | Err(_) => return Io::Failed(io::ErrorKind::InvalidData),
                }
                if discard > 0 {
                    if discard > self.used {
                        return Io::Failed(io::ErrorKind::InvalidData);
                    }
                    self.incoming.copy_within(discard..self.used, 0);
                    self.used -= discard;
                }
                if got {
                    continue;
                }

                let Some(room) = self.incoming.get_mut(self.used..) else {
                    return Io::Failed(io::ErrorKind::InvalidData);
                };
                if room.is_empty() {
                    // A record larger than the buffer cannot be assembled, and
                    // waiting for room that will never appear is a hang.
                    return Io::Failed(io::ErrorKind::InvalidData);
                }
                match sock.recv(room) {
                    Io::Ready(n) => self.used += n,
                    other => return other,
                }
            }
        }

        /// Encrypt and send. A short write is remembered, not lost.
        pub fn send(&mut self, sock: &mut TcpTransport, buf: &[u8]) -> Io {
            match self.flush(sock) {
                Ok(true) => {}
                // Still draining the last message: the caller keeps its bytes,
                // which is exactly `DESIGN.md` D10's backpressure contract.
                Ok(false) => return Io::Idle,
                Err(k) => return Io::Failed(k),
            }
            if buf.is_empty() {
                return Io::Idle;
            }
            let UnbufferedStatus { state, .. } = S::process_tls_records(&mut self.conn, &mut []);
            let Ok(ConnectionState::WriteTraffic(mut w)) = state else {
                return Io::Idle;
            };
            let Ok(n) = w.encrypt(buf, &mut self.outgoing) else {
                // Not enough room for this message plus its overhead. The
                // caller may offer less; refusing is right, losing is not.
                return Io::Idle;
            };
            self.out_used = n;
            self.out_sent = 0;
            match self.flush(sock) {
                Ok(_) => Io::Ready(buf.len()),
                Err(k) => Io::Failed(k),
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub use handshake::{Handshake, Step, Traffic};

/// Which parts of the kernel handover a serving loop really performs.
///
/// `[2026-09-10]` **A test seam whose shape is a finding rather than a
/// convenience.** [ADR-0060] decision 1 has two halves — a startup probe and a
/// per-connection check — and the first attempt gave them **one boolean between
/// them**. That could not express the case the second half exists for: *the
/// kernel is capable and this handshake still fell back*. Setting the flag to
/// "no kernel" made the startup probe refuse, so the serving loop never bound
/// and the per-connection half was unreachable from any test —
/// `[measured 2026-09-10]` `the serving loop never bound the address`.
///
/// One flag for two independent facts is `STATUS.md` item 54's shape, where
/// three socket endings shared one word. Three named situations cost the same
/// single parameter and say exactly which pretence is in force.
///
/// **A deployment always passes [`TlsProbe::Real`].**
///
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsProbe {
    /// Ask the kernel, offload for real. **What a deployment passes.**
    Real,
    /// The startup probe reports that this kernel has no TLS ULP.
    ///
    /// Reaches ADR-0060 decision 1's **first** half on a desk whose kernel does
    /// offload — which is every desk this has run on, so without it that
    /// refusal would be code that compiles and has never run.
    PretendKernelCannotOffload,
    /// The startup probe is real; each handshake then lands in userspace.
    ///
    /// Reaches the **second** half: the suite-mismatch case, where the host is
    /// fine and this particular connection is not. **This is the arm a single
    /// boolean could not express.**
    PretendHandshakeFallsBack,
}

/// Can **this kernel** offload TLS at all?
///
/// `[2026-09-10]` **[ADR-0060] decision 1, the startup half.** One `socket`, one
/// `connect`, one `setsockopt`, once, before the listener opens — so a host
/// built without `CONFIG_TLS` is found before a counterparty is affected, rather
/// than through a dropped session that had nothing to do with that counterparty.
///
/// **The socket must be connected, and that is measured rather than assumed.**
/// `scripts/check-ktls-available.sh` learned it first: `TCP_ULP` on an
/// unconnected socket fails for a *different* reason, so a probe that skipped
/// the `connect` would report a refusal on a kernel that offloads perfectly
/// well. A probe wrong in the pessimistic direction is still wrong, and under
/// `TlsRequireKernel=Y` it would refuse to start a correct deployment.
///
/// **What it does not answer:** whether the suite *your counterparty* picks can
/// be offloaded. A capable kernel still lands a mismatched suite in userspace,
/// which is why ADR-0060 decision 1 has a second half.
///
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
#[must_use]
pub fn kernel_can_offload() -> bool {
    let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:0") else {
        return false;
    };
    let Ok(addr) = listener.local_addr() else {
        return false;
    };
    let Ok(client) = std::net::TcpStream::connect(addr) else {
        return false;
    };
    let Ok((server, _)) = listener.accept() else {
        return false;
    };
    let answer = ktls_core::setup_ulp(&client).is_ok();
    drop(server);
    answer
}

/// A `rustls::ServerConfig` this engine can actually hand to the kernel.
///
/// `[2026-09-10]` **step 4a of the `tls` plan.** `crate::serve_tls` takes a
/// certificate and a key rather than a finished `ServerConfig` and builds it
/// here, because two of the settings are load-bearing and neither is
/// discoverable:
///
/// 1. **`enable_secret_extraction = true`.** Without it the connection is
///    **dropped**, and it is worth being exact about why, because the obvious
///    guess is wrong. It does not quietly serve from userspace `rustls`: the
///    fallback is reachable only from a `setup_ulp` refusal, which is asked
///    while the rustls connection is still alive.
///    `dangerous_into_kernel_connection` **consumes** that connection, so a
///    refusal there leaves nothing to fall back *to* — the comment on
///    `TlsTransport::hand_over` says so, and it is the reason the two questions
///    are asked in that order. `[measured 2026-09-10]` flipping this flag to
///    `false` and running
///    `crates/engine/tests/tls_wire.rs::serve_tls_brings_a_session_up_through_tls`
///    reads `ConnectionReset` at the counterparty, with no FIX-level
///    explanation because there is no session yet to carry one.
/// 2. **TLS 1.3, `AES-128-GCM` only.** kTLS carries far fewer suites than
///    `rustls` will negotiate, so leaving the ends to agree freely means the
///    kernel offload succeeds or fails depending on what the counterparty
///    offered. `spikes/ktls` made the same narrowing for the same reason.
///
/// **This is a deliberate narrowing and it is a cost, not only a safeguard.** A
/// counterparty that cannot do `TLS13_AES_128_GCM_SHA256` cannot connect to this
/// acceptor at all. ADR-0005 open question 2 — which kernel and which suites are
/// the floor — is answered by measurement in step 6, and this is the answer
/// standing in until then.
///
/// # Errors
///
/// [`crate::ServeError::Tls`] if the provider has no TLS 1.3, or the certificate
/// and key do not match.
#[cfg(all(feature = "tls", target_os = "linux"))]
pub fn server_config(
    certs: Vec<rustls::pki_types::CertificateDer<'static>>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Result<std::sync::Arc<rustls::ServerConfig>, crate::ServeError> {
    let mut cfg = rustls::ServerConfig::builder_with_provider(offloadable_provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| crate::ServeError::Tls(format!("{e}")))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| crate::ServeError::Tls(format!("{e}")))?;
    cfg.enable_secret_extraction = true;
    Ok(std::sync::Arc::new(cfg))
}

/// `ring`, narrowed to the one suite this engine hands to the kernel —
/// [`server_config`]'s second numbered point, shared with [`client_config`] so
/// the two ends cannot come to disagree about it.
#[cfg(all(feature = "tls", target_os = "linux"))]
fn offloadable_provider() -> std::sync::Arc<rustls::crypto::CryptoProvider> {
    let mut provider = rustls::crypto::ring::default_provider();
    provider
        .cipher_suites
        .retain(|cs| cs.suite() == rustls::CipherSuite::TLS13_AES_128_GCM_SHA256);
    std::sync::Arc::new(provider)
}

/// A `rustls::ClientConfig` whose connections this engine can hand to the
/// kernel — [`server_config`]'s counterpart for the end that dials.
///
/// `[2026-09-13]` **step 5a of the `tls` plan, Sửa 6 item 6.4.2.** The same two
/// load-bearing settings as [`server_config`], for the same reasons:
/// `enable_secret_extraction = true`, without which the handover cannot take
/// the keys, and TLS 1.3 with `AES-128-GCM` only.
///
/// **The server's certificate is always verified, against `roots` and nothing
/// else.** With `ring` and neither `webpki-roots` nor `rustls-native-certs`
/// there is no system root store, and adding one is an ADR; a verifier that
/// accepts anything — QuickFIX's `CertificateVerifyLevel=0` — is not offered.
/// An **empty** `roots` is therefore refused here rather than accepted: it
/// would build a configuration under which no server can ever verify, and
/// that would surface per connection, as a handshake failure, rather than
/// once, at start-up.
///
/// `identity` is the client certificate chain and its key, for a venue that
/// asks for one; `None` sends none.
///
/// **What it does not decide:** the name the certificate must carry. That is
/// the `ServerName` given to `rustls::client::UnbufferedClientConnection::new`
/// with this configuration — an IP literal needs an IP SAN, and
/// `tests/tls_client.rs::a_certificate_for_another_name_fails_the_handshake`
/// shows the mismatch reads as a failed handshake.
///
/// # Errors
///
/// [`crate::ServeError::Tls`] if `roots` is empty, a root cannot be parsed as a
/// trust anchor, the provider has no TLS 1.3, or `identity`'s certificate and
/// key do not match.
#[cfg(all(feature = "tls", target_os = "linux"))]
pub fn client_config(
    roots: Vec<rustls::pki_types::CertificateDer<'static>>,
    identity: Option<(
        Vec<rustls::pki_types::CertificateDer<'static>>,
        rustls::pki_types::PrivateKeyDer<'static>,
    )>,
) -> Result<std::sync::Arc<rustls::ClientConfig>, crate::ServeError> {
    if roots.is_empty() {
        return Err(crate::ServeError::Tls(
            "no certification authority was given, so no server certificate could verify"
                .to_string(),
        ));
    }
    let mut store = rustls::RootCertStore::empty();
    for root in roots {
        store.add(root).map_err(|e| {
            crate::ServeError::Tls(format!("a certification authority is not usable: {e}"))
        })?;
    }
    let builder = rustls::ClientConfig::builder_with_provider(offloadable_provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| crate::ServeError::Tls(format!("{e}")))?
        .with_root_certificates(store);
    let mut cfg = match identity {
        None => builder.with_no_client_auth(),
        Some((certs, key)) => builder
            .with_client_auth_cert(certs, key)
            .map_err(|e| crate::ServeError::Tls(format!("the client certificate: {e}")))?,
    };
    cfg.enable_secret_extraction = true;
    // ADR-0063 decision 1: the initiator never resumes a TLS session. On the
    // kernel path the tickets are dropped unread whatever this says
    // (`side::Ticketless`); this line makes the userspace fallback, where
    // rustls reads them itself, agree — and stops a 256-entry cache nobody
    // reads. Proven by
    // `tests/tls_key_update.rs::a_redial_does_a_full_handshake_not_a_resumption`,
    // whose userspace arm is the one this line holds.
    cfg.resumption = rustls::client::Resumption::disabled();
    Ok(std::sync::Arc::new(cfg))
}

/// Everything an initiator needs to dial a TLS venue, as **one** parameter.
///
/// `[2026-09-13]` **step 5b of the `tls` plan, Sửa 6 item 6.4.5.**
/// `crate::connect_and_serve_tls` takes the parameters of
/// `crate::connect_and_serve` plus this — eight, the same count as the four
/// `*_with_recovery` doors. Spread out as four parameters it would be eleven,
/// which is the reopening condition [ADR-0054] named for a `Serve` builder; so
/// the struct is the decision, not an arrangement for the lint.
///
/// **A deployment builds it from settings** (step 5c) or by hand. Every field
/// is a fact about the venue or about this deployment; the test seam
/// [`TlsProbe`] is deliberately not one of them.
///
/// [ADR-0054]: ../../../docs/decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md
#[cfg(all(feature = "tls", target_os = "linux"))]
#[derive(Debug)]
pub struct ClientTls {
    /// The certification authorities the venue's certificate must chain to —
    /// `CertificationAuthoritiesFile`. **The only trust anchors**: see
    /// [`client_config`], which refuses an empty list.
    pub roots: Vec<rustls::pki_types::CertificateDer<'static>>,
    /// This end's certificate chain and key, for a venue that asks for one;
    /// `None` sends none.
    pub identity: Option<(
        Vec<rustls::pki_types::CertificateDer<'static>>,
        rustls::pki_types::PrivateKeyDer<'static>,
    )>,
    /// The name the venue's certificate must carry — `SocketConnectHost`. An IP
    /// literal is `ServerName::IpAddress` and needs an IP SAN.
    pub server_name: rustls::pki_types::ServerName<'static>,
    /// `TlsRequireKernel=Y`: refuse to dial on a kernel that cannot offload,
    /// and end any connection whose handshake still lands in userspace.
    /// **Either way the fallback is reported** ([ADR-0060] decision 2).
    ///
    /// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
    pub require_kernel: bool,
}

/// The certificate and key a [`crate::settings::TlsSettings`] names, read off
/// disk as DER.
///
/// **The joint between two halves that were built to different shapes.**
/// `TlsSettings` carries *paths*, on purpose — a configuration parser that
/// reads the filesystem fails for two unrelated reasons with one message. Every
/// `serve_tls*` entry point takes *DER*. Until this function there was no
/// supported way from one to the other, so the four settings keys of step 4c-1
/// were a promise in a document. `crates/engine/tests/tls_settings_wire.rs` is
/// the gate for the sentence *"a `.cfg` file can bring a TLS session up"*, and
/// before it that sentence was false.
///
/// **Four operator mistakes, four different sentences.** A path that names no
/// file, a file that cannot be read, a certificate file with no `CERTIFICATE`
/// section in it, and a key file with no private key in it are four different
/// things to go and fix, and each names the settings key *and* the path. They
/// were collapsed into one message in the first draft and separated on purpose:
/// "TLS setup failed" tells an operator to read the source.
///
/// **It reads whole files at start-up, and that is where it belongs.** Nothing
/// here is on any hot path — non-negotiable 1 is about the parse, serialise,
/// session and dispatch paths, and this runs once, before the listener binds.
///
/// # Errors
///
/// [`crate::ServeError::Tls`] for every one of the four, each naming the
/// settings key and the path. The variant is shared with
/// [`server_config`] because `ServeError` is a public enum and a new variant is
/// a breaking change for a distinction the message already makes.
#[cfg(all(feature = "tls", target_os = "linux"))]
pub fn load_pem(
    settings: &crate::settings::TlsSettings,
) -> Result<
    (
        Vec<rustls::pki_types::CertificateDer<'static>>,
        rustls::pki_types::PrivateKeyDer<'static>,
    ),
    crate::ServeError,
> {
    let certs = read_certificates(settings.certificate(), "ServerCertificateFile")?;
    let key = read_private_key(settings.private_key(), "ServerCertificateKeyFile")?;
    Ok((certs, key))
}

/// Every `CERTIFICATE` section of the PEM file at `path`, or a
/// [`crate::ServeError::Tls`] naming `key` — the settings key the operator
/// wrote the path under — and the path.
///
/// Shared by [`load_pem`] and [`load_client_pem`] so the three sentences about
/// a certificate file (not there, not PEM, no `CERTIFICATE` in it) are written
/// once and read the same for all three keys that name one.
#[cfg(all(feature = "tls", target_os = "linux"))]
fn read_certificates(
    path: &std::path::Path,
    key: &str,
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, crate::ServeError> {
    use rustls::pki_types::pem::{Error as PemError, PemObject as _};

    // `pem_file_iter` reports opening the file from the call and reading it
    // from the iterator — the split this function wants anyway, since a missing
    // path and a corrupt body are different things to fix.
    let certs = rustls::pki_types::CertificateDer::pem_file_iter(path)
        // `PemError`'s own `Display` prefixes `I/O error:`, which reads twice
        // here; the `io::Error` alone is the sentence an operator needs.
        .map_err(|e| match e {
            PemError::Io(io) => crate::ServeError::Tls(format!(
                "{key} {} could not be opened: {io}",
                path.display()
            )),
            other => crate::ServeError::Tls(format!(
                "{key} {} could not be opened: {other}",
                path.display()
            )),
        })?
        .collect::<Result<Vec<_>, PemError>>()
        .map_err(|e| {
            crate::ServeError::Tls(format!("{key} {} is not readable PEM: {e}", path.display()))
        })?;
    // An empty vector, not a `NoItemsFound`: the iterator yields nothing at all
    // for a well-formed PEM holding only sections of other kinds — which is
    // exactly what a private key handed in as a certificate looks like.
    if certs.is_empty() {
        return Err(crate::ServeError::Tls(format!(
            "{key} {} holds no CERTIFICATE section",
            path.display()
        )));
    }
    Ok(certs)
}

/// The private key in the PEM file at `path`, or a [`crate::ServeError::Tls`]
/// naming `key` and the path. The companion of [`read_certificates`].
#[cfg(all(feature = "tls", target_os = "linux"))]
fn read_private_key(
    path: &std::path::Path,
    key: &str,
) -> Result<rustls::pki_types::PrivateKeyDer<'static>, crate::ServeError> {
    use rustls::pki_types::pem::{Error as PemError, PemObject as _};

    rustls::pki_types::PrivateKeyDer::from_pem_file(path).map_err(|e| match e {
        PemError::Io(io) => crate::ServeError::Tls(format!(
            "{key} {} could not be opened: {io}",
            path.display()
        )),
        PemError::NoItemsFound => crate::ServeError::Tls(format!(
            "{key} {} holds no PRIVATE KEY section",
            path.display()
        )),
        other => crate::ServeError::Tls(format!(
            "{key} {} is not readable PEM: {other}",
            path.display()
        )),
    })
}

/// Everything `crate::connect_and_serve_tls` needs to dial a TLS venue, read
/// off disk from what a [`crate::settings::ClientTlsSettings`] names.
///
/// `[added 2026-09-13]` step 5c of `docs/plans/2026-09-04-tls.md` (Sửa 6, 6.4
/// item 6). The initiator's counterpart of [`load_pem`], and the joint between
/// a `.cfg` file and [`ClientTls`]; `tests/tls_initiator_wire.rs::a_configuration_file_brings_a_tls_initiator_up`
/// is the gate for the sentence *"a `.cfg` file can dial a TLS venue"*.
///
/// `host` is the name the venue's certificate must carry — `SocketConnectHost`
/// as written, which [`crate::settings::Settings::into_tls_initiator`] hands
/// back as the host part of the dial address. An IPv6 literal may be written
/// in brackets; they are taken off. An IP literal needs an IP SAN.
///
/// **Each operator mistake names its key and its path**, through the same
/// loader [`load_pem`] uses: a `CertificationAuthoritiesFile` that is not
/// there, is not PEM or holds no `CERTIFICATE`; a `ClientCertificateFile` the
/// same; a `ClientCertificateKeyFile` holding no private key. Whether the
/// certification authorities are *usable* as trust anchors, and whether the
/// client certificate matches its key, is decided by [`client_config`], which
/// `connect_and_serve_tls_with` calls before its first dial — so still before
/// any connection is made, and with the same error variant.
///
/// # Errors
///
/// [`crate::ServeError::Tls`] for every one of the above, and for a `host` that
/// is neither a DNS name nor an IP address.
#[cfg(all(feature = "tls", target_os = "linux"))]
pub fn load_client_pem(
    settings: &crate::settings::ClientTlsSettings,
    host: &str,
) -> Result<ClientTls, crate::ServeError> {
    let roots = read_certificates(settings.ca(), "CertificationAuthoritiesFile")?;
    let identity = match settings.identity() {
        None => None,
        Some((cert, key)) => Some((
            read_certificates(cert, "ClientCertificateFile")?,
            read_private_key(key, "ClientCertificateKeyFile")?,
        )),
    };
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let server_name = rustls::pki_types::ServerName::try_from(bare.to_owned()).map_err(|e| {
        crate::ServeError::Tls(format!(
            "SocketConnectHost {host} is not a name a certificate can carry: {e}"
        ))
    })?;
    Ok(ClientTls {
        roots,
        identity,
        server_name,
        require_kernel: settings.require_kernel(),
    })
}

#[cfg(target_os = "linux")]
mod transport_impl {
    //! [`TlsTransport`]: a socket that handshakes, hands its keys to the kernel,
    //! and then is an ordinary socket again.
    //!
    //! **The whole design is in that last clause.** After the handover the
    //! kernel holds the keys, `read(2)` returns plaintext and `write(2)` takes
    //! it, so [`Transport::recv`] and [`Transport::send`] are the same two
    //! syscalls the plain transport makes. D8's spin loop learns nothing new,
    //! the parser still works in place, and `benches/alloc.rs` still reads zero
    //! — none of which is true of the userspace fallback, which is why
    //! [`TlsMode`] exists and is reported rather than inferred.

    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use ktls_core::Context;

    use super::{Client, Handshake, Server, Side, Step, TlsMode, Traffic};
    use crate::transport::{Io, Source, TcpTransport, Transport};

    /// The capacity ktls-core 0.0.5 `recv_tls_record` reserves before reading
    /// any control record: `u16::MAX + 5` (`ffi.rs:110`). Taken once, at the
    /// handover, so reading one never allocates — see [`TlsTransport`]'s
    /// `advance`. 64 KiB per kernel-mode connection, for as long as it lives.
    const CONTROL_RECORD_BUF: usize = u16::MAX as usize + 5;

    /// What the socket is doing right now.
    enum Stage<S: Side> {
        /// Still negotiating. `recv`/`send` pump it and report [`Io::Idle`].
        Handshaking(Box<Handshake<S>>),
        /// The kernel holds the keys. Ordinary reads and writes, plus the one
        /// error path the offload adds. `S::Kernel` is
        /// `rustls::kernel::KernelConnection<S::Data>`, wrapped in
        /// `side::Ticketless` on the client.
        Kernel(Box<Context<S::Kernel>>),
        /// The kernel would not take the offload. **This is the mode that
        /// leaves the hot-path guarantee** — ADR-0005 decision 3 — and it is
        /// named rather than silently entered: `mode()` says so,
        /// [`TlsTransport::fell_back`] says so, and `TlsRequireKernel=Y` will
        /// refuse it outright.
        Userspace(Box<Traffic<S>>),
        /// It ended, and the reason is kept so the engine reports it once.
        Broken(io::ErrorKind),
    }

    /// A TLS socket: an accepted one by default, a dialled one as
    /// `TlsTransport<Client>`.
    ///
    /// `[2026-09-13]` generic over [`Side`] since step 5a of the `tls` plan. The
    /// default keeps every spelling that predates the client meaning the
    /// acceptor, and the four `tests/tls*.rs` files that were written against
    /// that spelling are the gate that it still does.
    pub struct TlsTransport<S: Side = Server> {
        sock: TcpTransport,
        stage: Stage<S>,
        /// Application bytes rustls decrypted before the handover, waiting to be
        /// handed to the session ahead of anything the kernel produces.
        early: Vec<u8>,
        early_at: usize,
        /// Whether this connection asked the kernel and was refused. Read once,
        /// by the engine, to raise the event.
        fell_back: bool,
        /// Whether to ask the kernel at all. Always `true` in a deployment; see
        /// [`TlsTransport::with_offload`].
        offload: bool,
        /// The kernel session's count of session tickets set aside — `Some`
        /// only on a client, and only once the keys are in the kernel.
        tickets: Option<Arc<AtomicU32>>,
    }

    impl<S: Side> TlsTransport<S> {
        /// Take a freshly accepted — or freshly connected — socket into a
        /// handshake.
        #[must_use]
        pub fn new(sock: TcpTransport, handshake: Handshake<S>) -> Self {
            Self::with_offload(sock, handshake, true)
        }

        /// The same, with the kernel offload disabled.
        ///
        /// **This exists so the userspace fallback can be tested at all.** That
        /// path is reached only when the kernel refuses `setup_ulp`, and this
        /// desk's kernel does not refuse — so without a way to ask for the
        /// refusal, the fallback would be code that compiles and has never run,
        /// which is what `CLAUDE.md` §10 calls a promise rather than evidence.
        /// It is not a deployment knob: `TlsRequireKernel` (step 4's settings
        /// half) is the operator-facing control and it points the other way.
        #[must_use]
        pub fn with_offload(sock: TcpTransport, handshake: Handshake<S>, offload: bool) -> Self {
            Self {
                sock,
                offload,
                stage: Stage::Handshaking(Box::new(handshake)),
                early: Vec::new(),
                early_at: 0,
                fell_back: false,
                tickets: None,
            }
        }

        /// Whether this connection fell back to userspace.
        ///
        /// **The engine must read this and say so**: a deployment that believes
        /// it is on kTLS and is not has a published latency figure describing a
        /// different code path, which is the whole of ADR-0005 open question 3.
        #[must_use]
        pub const fn fell_back(&self) -> bool {
            self.fell_back
        }

        /// Which of ADR-0005's answers is carrying the bytes.
        ///
        /// [`TlsMode::Plain`] is never returned by this type — it is what a
        /// `TcpTransport` is — and a handshake still in flight reports
        /// `Userspace`, because that is what is true while `rustls` is doing
        /// the work. **Read it after the session is up**, which is the only
        /// point at which the answer is stable and the only point a published
        /// latency figure is about.
        #[must_use]
        pub const fn mode(&self) -> TlsMode {
            match self.stage {
                Stage::Kernel(_) => TlsMode::Kernel,
                Stage::Handshaking(_) | Stage::Broken(_) | Stage::Userspace(_) => {
                    TlsMode::Userspace
                }
            }
        }

        /// Whether the handshake is finished and the keys are in the kernel.
        #[must_use]
        pub const fn is_ready(&self) -> bool {
            matches!(self.stage, Stage::Kernel(_))
        }

        /// Drive the handshake, and hand the keys over the moment it completes.
        ///
        /// Called from both `recv` and `send`, because either can be the first
        /// thing the engine asks of a socket.
        fn advance(&mut self) -> Result<(), io::ErrorKind> {
            let Stage::Handshaking(hs) = &mut self.stage else {
                return match self.stage {
                    Stage::Broken(k) => Err(k),
                    _ => Ok(()),
                };
            };
            match hs.pump(&mut self.sock) {
                Step::Pending => return Ok(()),
                Step::Failed(k) => {
                    self.stage = Stage::Broken(k);
                    return Err(k);
                }
                Step::Done => {}
            }

            // ADR-0018's third condition, asserted rather than assumed. Bytes
            // here are ciphertext the kernel will never see and whose place in
            // the sequence it has already counted, so handing over now
            // desynchronises the receive side and the next record fails with
            // `EBADMSG` — the spike's `hand-draining-desyncs-the-kernel`.
            if hs.leftover() != 0 {
                self.stage = Stage::Broken(io::ErrorKind::InvalidData);
                return Err(io::ErrorKind::InvalidData);
            }
            self.early = hs.take_early_data();
            self.early_at = 0;

            // **`setup_ulp` FIRST, and the order is forced rather than
            // stylistic.** `dangerous_into_kernel_connection` **consumes** the
            // rustls connection, and the fallback ADR-0005 decision 3 requires
            // needs that connection alive. Asking the kernel to attach its TLS
            // ULP costs nothing, needs no keys, and is the step that fails on a
            // kernel without the `tls` module — so it is the question to ask
            // while the answer can still be acted on. Converting first and
            // discovering the refusal afterwards leaves nothing to fall back
            // *to*, which is why the previous commit could only end the
            // connection.
            if !self.offload || ktls_core::setup_ulp(self.sock.socket()).is_err() {
                let Stage::Handshaking(hs) =
                    core::mem::replace(&mut self.stage, Stage::Broken(io::ErrorKind::InvalidData))
                else {
                    return Err(io::ErrorKind::InvalidData);
                };
                // The early bytes go into the traffic driver rather than
                // `self.early`, so one queue orders them and the userspace path
                // cannot deliver them twice.
                let mut traffic = hs.into_traffic();
                traffic.adopt_early(core::mem::take(&mut self.early));
                self.early_at = 0;
                self.fell_back = true;
                self.stage = Stage::Userspace(Box::new(traffic));
                return Ok(());
            }

            let Stage::Handshaking(hs) =
                core::mem::replace(&mut self.stage, Stage::Broken(io::ErrorKind::InvalidData))
            else {
                return Err(io::ErrorKind::InvalidData);
            };
            let Ok((secrets, kconn)) = S::into_kernel(hs.into_connection()) else {
                return Err(io::ErrorKind::InvalidData);
            };

            let version = ktls_core::TlsSession::protocol_version(&kconn);
            if super::push_keys(self.sock.socket(), secrets, version).is_err() {
                // The ULP attached and the keys did not go down — a cipher
                // suite this kernel does not carry (ADR-0005 open question 2).
                // The connection is unusable in either mode now: rustls has
                // been consumed and the socket has a ULP with no keys.
                self.stage = Stage::Broken(io::ErrorKind::Unsupported);
                return Err(io::ErrorKind::Unsupported);
            }
            // **Pre-sized, inside the handshake carve-out, and the size is
            // ktls-core's rather than a guess.** Every control record after the
            // handover — a client's session tickets, an alert, a KeyUpdate — is
            // read by ktls-core 0.0.5 `recv_tls_record` (`ffi.rs:110`), which
            // first calls `reserve(u16::MAX + 5)` on this buffer. `None` starts
            // an empty `Vec`, so the first such record allocated 64 KiB on the
            // engine thread. With the capacity taken here that `reserve` is a
            // no-op: the buffer's length stays 0, because the engine never
            // calls `Buffer::read`/`drain`, the only callers of the
            // `shrink_to(65536)` in `Buffer::reset` (`utils.rs:210-214`), and
            // the one branch that fills it — application data read as a control
            // record, `context.rs:286-295` — is not reached after a `read(2)`
            // that said `EIO`, which means a control record is at the head.
            // A 16 KiB + 5 buffer — one TLS record — would not do: `reserve`
            // asks for 65 540 regardless of the record.
            // Proven by `tests/tls_key_update.rs::a_key_update_allocates_only_the_boxes_rustls_key_schedule_forces`
            // (no allocation of this size on either side; rustls's own key schedule
            // still allocates four boxes per rekey, which that test counts exactly,
            // ADR-0063) and by `a_session_ticket_after_the_handover_allocates_nothing`.
            let buffer = ktls_core::Buffer::new(Vec::with_capacity(CONTROL_RECORD_BUF));
            self.tickets = S::tickets(&kconn);
            self.stage = Stage::Kernel(Box::new(Context::new(kconn, Some(buffer))));
            Ok(())
        }

        /// Serve from the pre-handover bytes first. Returns 0 when they are
        /// spent, which is the only time the kernel is asked.
        fn drain_early(&mut self, buf: &mut [u8]) -> usize {
            let Some(rest) = self.early.get(self.early_at..) else {
                return 0;
            };
            if rest.is_empty() {
                if !self.early.is_empty() {
                    self.early = Vec::new();
                    self.early_at = 0;
                }
                return 0;
            }
            let n = rest.len().min(buf.len());
            let (Some(src), Some(dst)) = (rest.get(..n), buf.get_mut(..n)) else {
                return 0;
            };
            dst.copy_from_slice(src);
            self.early_at += n;
            n
        }
    }

    impl TlsTransport<Client> {
        /// How many TLS 1.3 session tickets this connection received on the
        /// kernel path and **dropped unread**.
        ///
        /// `[2026-09-13]` step 6c-2 of the `tls` plan; [ADR-0063] decision 1.
        /// The initiator never resumes a TLS session, so a ticket is counted
        /// and not parsed, and costs no allocation on the engine thread. A
        /// handler that does nothing cannot otherwise show that it ran; this
        /// is that evidence, and a line a tool can print.
        ///
        /// `0` before the handover and on the userspace fallback, where rustls
        /// reads tickets itself and — with `Resumption::disabled()` in
        /// [`super::client_config`] — stores none. A rustls server sends two by
        /// default.
        ///
        /// [ADR-0063]: ../../../docs/decisions/ADR-0063-a-peers-key-update-is-the-second-named-carve-out-and-a-ticket-is-not-read.md
        #[must_use]
        pub fn tickets_ignored(&self) -> u32 {
            self.tickets
                .as_ref()
                .map_or(0, |n| n.load(Ordering::Relaxed))
        }
    }

    impl<S: Side> Transport for TlsTransport<S> {
        /// [ADR-0060] decision 3: the same answer as the inherent
        /// [`TlsTransport::mode`], reachable through the trait so a generic engine
        /// can ask it without a downcast.
        ///
        /// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
        fn tls_mode(&self) -> TlsMode {
            self.mode()
        }

        const POLLABLE: bool = cfg!(unix);

        fn source(&self) -> Option<Source> {
            self.sock.source()
        }

        fn recv(&mut self, buf: &mut [u8]) -> Io {
            // **Before the socket, always.** A `Logon` that arrived during the
            // handshake is already decrypted and in hand; reading the kernel
            // first would deliver it out of order, which on a FIX session is a
            // sequence gap the counterparty did not cause.
            let n = self.drain_early(buf);
            if n > 0 {
                return Io::Ready(n);
            }
            if let Err(k) = self.advance() {
                return Io::Failed(k);
            }
            // The userspace fallback carries its own buffers and its own
            // ordering; `self.early` was handed to it at the fork.
            if let Stage::Userspace(t) = &mut self.stage {
                return t.recv(&mut self.sock, buf);
            }
            let Stage::Kernel(ctx) = &mut self.stage else {
                return Io::Idle;
            };
            // **Read the socket here rather than through `TcpTransport::recv`,
            // and the reason is a hole in `Io` rather than a preference.**
            // `Io::Failed` carries an `io::ErrorKind`, and the error this path
            // must recognise is `EIO` — which has no stable `ErrorKind` at all
            // (`ErrorKind::Uncategorized` is unstable to name). Going through
            // the plain transport would throw away the one piece of information
            // that distinguishes a recoverable control record from a dead
            // socket.
            //
            // ADR-0018's first condition: **every** read error goes through
            // `Context::handle_io_error`. A TLS 1.3 session ticket arrives as a
            // control record the kernel will not decode, surfaces as `EIO`, and
            // an engine that treated it as a failure would drop a session that
            // was perfectly healthy — `[measured 2026-08-31]` the spike saw one
            // ticket and one recovery on every client run.
            match io::Read::read(&mut self.sock.socket(), buf) {
                Ok(0) => Io::Closed,
                Ok(n) => Io::Ready(n),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Io::Idle,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => Io::Idle,
                Err(e) => {
                    if ctx.handle_io_error(self.sock.socket(), e).is_ok() {
                        Io::Idle
                    } else {
                        Io::Failed(io::ErrorKind::InvalidData)
                    }
                }
            }
        }

        fn send(&mut self, buf: &[u8]) -> Io {
            if let Err(k) = self.advance() {
                return Io::Failed(k);
            }
            // `stage` and `sock` are different fields, so both can be borrowed
            // mutably at once — which is why `Traffic` takes the socket as an
            // argument and does not own one.
            if let Stage::Userspace(t) = &mut self.stage {
                return t.send(&mut self.sock, buf);
            }
            if !matches!(self.stage, Stage::Kernel(_)) {
                // Nothing may go out before the keys are in the kernel: it
                // would leave as plaintext on a socket the peer is reading as
                // TLS.
                return Io::Idle;
            }
            self.sock.send(buf)
        }
    }
}

#[cfg(target_os = "linux")]
pub use transport_impl::TlsTransport;

/// Hand the negotiated keys to the kernel, both directions.
///
/// Split out so the one `unsafe`-adjacent step — three `setsockopt` calls
/// behind `ktls-core` — sits in one place with one name.
///
/// # Errors
///
/// Any of `setup_ulp` or the two `set` calls. A kernel without the `tls` module
/// fails at `setup_ulp`, which is the case ADR-0005 decision 3's fallback is
/// for.
#[cfg(target_os = "linux")]
fn push_keys<S: std::os::fd::AsFd>(
    sock: &S,
    secrets: rustls::ExtractedSecrets,
    // Already `ktls_core`'s: `TlsSession::protocol_version` on a
    // `KernelConnection` is the rustls value `.into()`'d — ktls-core 0.0.5
    // `tls.rs:533-535, 576-578` — which is the conversion this function made
    // itself before it became generic over the side.
    version: ktls_core::ProtocolVersion,
) -> Result<(), ktls_core::Error> {
    let secrets = ktls_core::ExtractedSecrets::try_from(secrets)?;
    ktls_core::TlsCryptoInfoTx::new(version, secrets.tx.1, secrets.tx.0)?.set(sock)?;
    ktls_core::TlsCryptoInfoRx::new(version, secrets.rx.1, secrets.rx.0)?.set(sock)?;
    Ok(())
}
